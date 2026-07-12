use std::ops::{Deref, DerefMut};
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use crate::app_settings::{AppSettings, AppSettingsState};
use crate::audio::player::AudioPlayer;
use crate::dto::{
    AlbumSummaryDto, ArtistSummaryDto, CloseAction, EqSettingsDto, ImportResultDto,
    PlaybackModeDto, PlaybackStateDto, QueueDto, QueueStateDto,
};
use crate::library::{Library, PlaylistInfo};
use crate::media_controls::TrackMetadata;
use crate::metadata::{enrich_lyrics_online, is_supported_audio_file, supported_audio_extensions, Track};
use crate::path_validation::{validate_audio_path, validate_safe_output_path};
use tauri::Manager;
use walkdir::WalkDir;

/// Lazily-initialized audio engine. Creation is deferred until first use so
/// Android can finish wiring JNI / ndk_context before cpal/oboe opens a stream.
pub struct PlayerState(pub Mutex<Option<AudioPlayer>>);
pub struct LibraryState(pub Mutex<Library>);
pub struct MediaBridgeState(pub crate::media_controls::MediaBridgeState);

// ── Helpers ───────────────────────────────────────────────────────────────────

fn lock_poisoned<T>(e: std::sync::PoisonError<T>) -> String {
    tracing::warn!("Mutex was poisoned, recovering: {e}");
    "State lock poisoned".to_string()
}

fn lock_player_state<'a>(
    state: &'a tauri::State<'a, PlayerState>,
) -> std::sync::MutexGuard<'a, Option<AudioPlayer>> {
    match state.0.lock() {
        Ok(g) => g,
        Err(poisoned) => {
            tracing::warn!("Player mutex was poisoned, recovering");
            poisoned.into_inner()
        }
    }
}

fn create_audio_player() -> Result<AudioPlayer, String> {
    // Never open the OS audio device during construction. On Android, cpal/oboe
    // can abort the process via JNI; queue/EQ/UI must stay usable without a stream.
    Ok(AudioPlayer::new_deferred())
}

pub(crate) fn ensure_player(slot: &mut Option<AudioPlayer>) -> Result<&mut AudioPlayer, String> {
    if slot.is_none() {
        *slot = Some(create_audio_player()?);
    }
    Ok(slot.as_mut().expect("player just initialized"))
}

pub struct PlayerGuard<'a>(MutexGuard<'a, Option<AudioPlayer>>);

impl Deref for PlayerGuard<'_> {
    type Target = AudioPlayer;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref().expect("player must be initialized before deref")
    }
}

impl DerefMut for PlayerGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0
            .as_mut()
            .expect("player must be initialized before deref_mut")
    }
}

fn lock_player<'a>(
    state: &'a tauri::State<'a, PlayerState>,
) -> Result<PlayerGuard<'a>, String> {
    // Recover from poisoned mutex: a previous panic may have left the lock in a
    // poisoned state, but the player data is still usable.
    let mut guard = match state.0.lock() {
        Ok(g) => g,
        Err(poisoned) => {
            tracing::warn!("Player mutex was poisoned, recovering: {poisoned}");
            poisoned.into_inner()
        }
    };
    ensure_player(&mut guard)?;
    Ok(PlayerGuard(guard))
}

fn with_app_player<R>(
    app: &tauri::AppHandle,
    f: impl FnOnce(&mut AudioPlayer) -> Result<R, String>,
) -> Result<R, String> {
    let state = app.state::<PlayerState>();
    let mut slot = match state.0.lock() {
        Ok(g) => g,
        Err(poisoned) => {
            tracing::warn!("Player mutex was poisoned, recovering: {poisoned}");
            poisoned.into_inner()
        }
    };
    let player = ensure_player(&mut slot)?;
    f(player)
}

fn lock_library<'a>(
    state: &'a tauri::State<'a, LibraryState>,
) -> Result<std::sync::MutexGuard<'a, Library>, String> {
    state.0.lock().map_err(lock_poisoned)
}

fn sync_bridge_playing(bridge: &tauri::State<MediaBridgeState>, position_secs: f64) {
    bridge.0.set_playing(position_secs);
}

/// Run a blocking operation on a background thread pool so the UI stays
/// responsive.  Returns the inner `Result` directly.
async fn blocking<F, T>(f: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| format!("Background task failed: {e}"))?
}

fn sync_queue_from_tracks(player: &mut AudioPlayer, tracks: &[Track], index: usize) {
    let new_paths: Vec<String> = tracks.iter().map(|track| track.path.clone()).collect();
    let old_paths: Vec<String> = player.queue.tracks().to_vec();

    // Preserve any manually-added queue items (those not in the new playlist).
    let manual: Vec<String> = old_paths
        .into_iter()
        .filter(|p| !new_paths.contains(p))
        .collect();

    player.queue.set_tracks(new_paths);
    if player.queue.jump(index).is_none() {
        tracing::warn!("Failed to align playback queue with playlist index {index}");
    }
    // Re-append manual items so they play after the playlist finishes.
    for path in manual {
        player.queue.enqueue(path);
    }
}

/// Build a minimal `Track` for a path that isn't in the library (e.g. a file
/// that was deleted or moved after being added to the queue).
fn placeholder_track(path: &str) -> Track {
    let name = Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Unknown")
        .to_string();
    Track {
        id: String::new(),
        path: path.to_string(),
        name: name.clone(),
        title: name,
        artist: "Unknown Artist".to_string(),
        album: "Local Files".to_string(),
        album_artist: None,
        genre: None,
        year: None,
        track_number: None,
        disc_number: None,
        format: Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("Audio")
            .to_uppercase(),
        duration_seconds: None,
        sample_rate: None,
        channels: None,
        bit_depth: None,
        lyrics: None,
        lyrics_source: None,
        cover_art_data_url: None,
        cover_art_mime: None,
        cover_art_source: None,
        fingerprint_sha256: None,
        acoustid_fingerprint: None,
        musicbrainz_recording_id: None,
        file_size: 0,
        modified_at: 0,
        indexed_at: 0,
    }
}

/// Look up a track by path in the library, falling back to a placeholder.
fn resolve_track(library: &Library, path: &str) -> Track {
    match library.get_tracks_by_paths(&[path.to_string()]) {
        Ok(results) if results.first().is_some_and(Option::is_some) => {
            results.into_iter().next().flatten().unwrap()
        }
        _ => placeholder_track(path),
    }
}

/// Push track metadata (and current shuffle/repeat mode) to the OS media bridge.
fn sync_bridge_now_playing(app: &tauri::AppHandle, track: &Track) {
    let bridge = app.state::<MediaBridgeState>();
    bridge.0.now_playing(TrackMetadata {
        title: Some(track.title.clone()),
        artist: Some(track.artist.clone()),
        album: Some(track.album.clone()),
        duration_seconds: track.duration_seconds,
        cover_url: track.cover_art_data_url.clone(),
    });
    sync_bridge_playback_mode(app, &bridge);
}

/// Push the current shuffle/repeat mode to the OS media bridge (e.g. so the
/// Android notification's shuffle/repeat buttons reflect the right state).
fn sync_bridge_playback_mode(app: &tauri::AppHandle, bridge: &tauri::State<MediaBridgeState>) {
    let state = app.state::<PlayerState>();
    let (shuffle, repeat) = {
        let guard = lock_player_state(&state);
        match guard.as_ref() {
            Some(player) => (player.queue.is_shuffled(), player.repeat.clone()),
            None => (false, crate::audio::player::RepeatMode::default()),
        }
    };
    bridge.0.set_playback_mode(shuffle, repeat_mode_str(&repeat).to_string());
}

fn repeat_mode_str(mode: &crate::audio::player::RepeatMode) -> &'static str {
    use crate::audio::player::RepeatMode;
    match mode {
        RepeatMode::Off => "off",
        RepeatMode::One => "one",
        RepeatMode::All => "all",
    }
}

// ── Platform / import helpers ─────────────────────────────────────────────────

#[tauri::command]
pub fn host_os() -> String {
    std::env::consts::OS.to_string()
}

#[derive(serde::Serialize)]
pub struct ImportAudioResult {
    pub paths: Vec<String>,
    pub errors: Vec<String>,
}

/// Copy picked files/content URIs into app-private storage and return local paths.
#[tauri::command]
pub async fn import_audio_sources(
    paths: Vec<String>,
    app: tauri::AppHandle,
) -> Result<ImportAudioResult, String> {
    let app = app.clone();
    blocking(move || {
        let (ok, errors) = crate::android_import::materialize_audio_sources(&app, &paths);
        Ok(ImportAudioResult { paths: ok, errors })
    })
    .await
}

// ── Playback commands ─────────────────────────────────────────────────────────

#[tauri::command]
pub async fn play_track(
    path: String,
    app: tauri::AppHandle,
) -> Result<(), String> {
    validate_audio_path(&path)?;
    let app_clone = app.clone();
    let path_clone = path.clone();
    let local_path = blocking(move || {
        crate::android_import::materialize_audio_source(&app_clone, &path_clone)
            .map(|p| p.to_string_lossy().into_owned())
            .map_err(|e| format!("Could not access audio file: {e}"))
    })
    .await?;

    let play_path = local_path.clone();
    let app_clone = app.clone();
    blocking(move || {
        with_app_player(&app_clone, |player| {
            player.play(&play_path).map_err(|e| format!("Playback failed: {e}"))
        })
    })
    .await?;

    let app_clone = app.clone();
    let lookup_path = local_path.clone();
    let track = blocking(move || {
        let lib = app_clone.state::<LibraryState>();
        let lib = lib.0.lock().map_err(|e| e.to_string())?;
        Ok::<_, String>(resolve_track(&lib, &lookup_path))
    })
    .await?;
    sync_bridge_now_playing(&app, &track);

    Ok(())
}

#[tauri::command]
pub async fn pause_track(
    state: tauri::State<'_, PlayerState>,
    bridge: tauri::State<'_, MediaBridgeState>,
) -> Result<(), String> {
    let position = {
        let mut player = lock_player(&state)?;
        let position = player.position_seconds();
        player.pause()?;
        position
    };
    bridge.0.set_paused(position);
    Ok(())
}

#[tauri::command]
pub async fn resume_track(
    state: tauri::State<'_, PlayerState>,
    bridge: tauri::State<'_, MediaBridgeState>,
) -> Result<(), String> {
    let position = {
        let mut player = lock_player(&state)?;
        player.resume()?;
        player.position_seconds()
    };
    sync_bridge_playing(&bridge, position);
    Ok(())
}

#[tauri::command]
pub async fn stop_track(
    state: tauri::State<'_, PlayerState>,
    bridge: tauri::State<'_, MediaBridgeState>,
) -> Result<(), String> {
    lock_player(&state)?.stop()?;
    bridge.0.set_stopped();
    Ok(())
}

#[tauri::command]
pub async fn get_playback_state(
    state: tauri::State<'_, PlayerState>,
) -> Result<PlaybackStateDto, String> {
    let guard = lock_player_state(&state);
    let Some(player) = guard.as_ref() else {
        return Ok(PlaybackStateDto {
            is_playing: false,
            is_paused: false,
            current_path: None,
            position_seconds: 0.0,
            duration_seconds: None,
            volume: 0.8,
            output_device_name: AudioPlayer::current_output_name(),
        });
    };
    Ok(PlaybackStateDto {
        is_playing: player.is_playing(),
        is_paused: player.is_paused(),
        current_path: player
            .get_current_path()
            .and_then(|path| path.to_str())
            .map(str::to_string),
        position_seconds: player.position_seconds(),
        duration_seconds: player.duration_seconds(),
        volume: player.volume(),
        output_device_name: AudioPlayer::current_output_name(),
    })
}

#[tauri::command]
pub async fn seek_track(
    seconds: f64,
    state: tauri::State<'_, PlayerState>,
    bridge: tauri::State<'_, MediaBridgeState>,
) -> Result<(), String> {
    let playing = {
        let mut player = lock_player(&state)?;
        player.seek(seconds)?;
        player.is_playing()
    };
    bridge.0.update_position(seconds, playing);
    Ok(())
}

#[tauri::command]
pub async fn set_volume(
    volume: f32,
    state: tauri::State<'_, PlayerState>,
    settings_state: tauri::State<'_, AppSettingsState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    lock_player(&state)?.set_volume(volume)?;
    let mut settings = lock_settings(&settings_state)?;
    settings.volume = volume;
    settings.save(&app)?;
    Ok(())
}

// ── Equalizer ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_eq_settings(
    state: tauri::State<'_, PlayerState>,
) -> Result<EqSettingsDto, String> {
    let guard = lock_player_state(&state);
    let eq = match guard.as_ref() {
        Some(player) => player.eq_settings(),
        None => crate::audio::dsp::EqConfig::default(),
    };
    Ok(EqSettingsDto {
        bands: eq.bands,
        enabled: eq.enabled,
    })
}

#[tauri::command]
pub async fn set_eq_bands(
    bands: Vec<f32>,
    state: tauri::State<'_, PlayerState>,
    settings_state: tauri::State<'_, AppSettingsState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    if bands.len() != 10 {
        return Err("Expected exactly 10 EQ band values".to_string());
    }
    for (index, gain) in bands.iter().enumerate() {
        if !gain.is_finite() {
            return Err(format!("EQ band {index} must be a finite number"));
        }
        if gain.abs() > 24.0 {
            return Err(format!("EQ band {index} gain must be between -24 and +24 dB"));
        }
    }
    let mut arr = [0.0f32; 10];
    arr.copy_from_slice(&bands);
    lock_player(&state)?.set_eq_bands(arr);
    let mut settings = lock_settings(&settings_state)?;
    settings.equalizer.bands = arr;
    settings.save(&app)?;
    Ok(())
}

#[tauri::command]
pub async fn set_eq_enabled(
    enabled: bool,
    state: tauri::State<'_, PlayerState>,
    settings_state: tauri::State<'_, AppSettingsState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    lock_player(&state)?.set_eq_enabled(enabled);
    let mut settings = lock_settings(&settings_state)?;
    settings.equalizer.enabled = enabled;
    settings.save(&app)?;
    Ok(())
}

#[tauri::command]
pub async fn export_eq_settings(
    path: String,
    name: Option<String>,
    state: tauri::State<'_, PlayerState>,
) -> Result<(), String> {
    validate_safe_output_path(&path, "json")?;
    let player = lock_player(&state)?;
    let eq = player.eq_settings();
    crate::audio::dsp::EqPresetFile::save_to(&path, &eq, name)
}

#[tauri::command]
pub async fn import_eq_settings(
    path: String,
    state: tauri::State<'_, PlayerState>,
    settings_state: tauri::State<'_, AppSettingsState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let eq = crate::audio::dsp::EqPresetFile::load_from(&path)?;
    {
        let mut player = lock_player(&state)?;
        player.set_eq_bands(eq.bands);
        player.set_eq_enabled(eq.enabled);
    }
    let mut settings = lock_settings(&settings_state)?;
    settings.equalizer = eq;
    settings.save(&app)?;
    Ok(())
}

// ── Library / playlist commands ───────────────────────────────────────────────

#[tauri::command]
pub async fn add_track_to_playlist(
    path: String,
    app: tauri::AppHandle,
) -> Result<Track, String> {
    let app = app.clone();
    blocking(move || {
        let local = crate::android_import::materialize_audio_source(&app, &path)?;
        let library = app.state::<LibraryState>();
        let lib = library.0.lock().map_err(|e| e.to_string())?;
        lib.add_track_to_default_playlist(local.to_string_lossy().into_owned())
    })
    .await
}

#[tauri::command]
pub async fn remove_track_from_playlist(
    path: String,
    library: tauri::State<'_, LibraryState>,
) -> Result<(), String> {
    lock_library(&library)?.remove_track_from_default_playlist(path)
}

#[tauri::command]
pub async fn get_playlist(
    library: tauri::State<'_, LibraryState>,
) -> Result<Vec<Track>, String> {
    lock_library(&library)?.get_default_playlist_tracks()
}

#[tauri::command]
pub async fn clear_playlist(
    library: tauri::State<'_, LibraryState>,
) -> Result<(), String> {
    lock_library(&library)?.clear_default_playlist()
}

// ── Favorites ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn add_track_to_favorites(
    path: String,
    app: tauri::AppHandle,
) -> Result<Track, String> {
    let app = app.clone();
    blocking(move || {
        let library = app.state::<LibraryState>();
        let lib = library.0.lock().map_err(|e| e.to_string())?;
        lib.add_track_to_favorites(path)
    })
    .await
}

#[tauri::command]
pub async fn remove_track_from_favorites(
    path: String,
    library: tauri::State<'_, LibraryState>,
) -> Result<(), String> {
    lock_library(&library)?.remove_track_from_favorites(&path)
}

#[tauri::command]
pub async fn get_favorites(
    library: tauri::State<'_, LibraryState>,
) -> Result<Vec<Track>, String> {
    lock_library(&library)?.get_favorites()
}

#[tauri::command]
pub async fn is_track_in_favorites(
    path: String,
    library: tauri::State<'_, LibraryState>,
) -> Result<bool, String> {
    lock_library(&library)?.is_track_in_favorites(&path)
}

#[tauri::command]
pub async fn is_track_in_playlist(
    path: String,
    library: tauri::State<'_, LibraryState>,
) -> Result<bool, String> {
    lock_library(&library)?.is_track_in_any_playlist(&path)
}

#[tauri::command]
pub async fn toggle_favorite(
    path: String,
    app: tauri::AppHandle,
) -> Result<bool, String> {
    let app = app.clone();
    blocking(move || {
        let library = app.state::<LibraryState>();
        let lib = library.0.lock().map_err(|e| e.to_string())?;
        lib.toggle_favorite(&path)
    })
    .await
}

#[tauri::command]
pub async fn clear_favorites(
    library: tauri::State<'_, LibraryState>,
) -> Result<(), String> {
    lock_library(&library)?.clear_favorites()
}

#[tauri::command]
pub async fn play_track_from_playlist(
    index: usize,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let app_clone = app.clone();
    let (raw_tracks, track) = blocking(move || {
        let library = app_clone.state::<LibraryState>();
        let lib = library.0.lock().map_err(|e| e.to_string())?;
        let tracks = lib.get_default_playlist_tracks()?;
        let track = tracks
            .get(index)
            .ok_or_else(|| format!("Track not found at index {index}"))?
            .clone();
        Ok((tracks, track))
    })
    .await?;

    let app_clone = app.clone();
    let raw_track_paths: Vec<String> = raw_tracks.iter().map(|t| t.path.clone()).collect();
    let materialized_paths = blocking(move || {
        Ok::<_, String>(raw_track_paths
            .into_iter()
            .map(|path| {
                crate::android_import::materialize_audio_source(&app_clone, &path)
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|e| {
                        tracing::warn!("Failed to materialize track {path}: {e}");
                        path
                    })
            })
            .collect::<Vec<_>>())
    })
    .await?;

    let local_path = materialized_paths
        .get(index)
        .cloned()
        .unwrap_or_default();

    if local_path.is_empty() {
        return Err(format!("Audio file not found for track at index {index}"));
    }

    let tracks: Vec<Track> = raw_tracks
        .into_iter()
        .zip(materialized_paths.into_iter())
        .map(|(mut t, p)| { t.path = p; t })
        .collect();

    let app_clone = app.clone();
    let tracks_clone = tracks.clone();
    blocking(move || {
        with_app_player(&app_clone, |player| {
            sync_queue_from_tracks(player, &tracks_clone, index);
            player.play(&local_path).map_err(|e| format!("Playback failed: {e}"))
        })
    })
    .await?;

    sync_bridge_now_playing(&app, &track);
    Ok(())
}

#[tauri::command]
pub fn scan_directory(directory: String) -> Result<Vec<String>, String> {
    let dir_path = Path::new(&directory);
    if !dir_path.is_dir() {
        return Err("Path is not a directory".to_string());
    }

    let paths: Vec<String> = WalkDir::new(dir_path)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter(|e| is_supported_audio_file(e.path()))
        .filter_map(|e| e.path().to_str().map(str::to_string))
        .collect();

    Ok(paths)
}

#[tauri::command]
pub async fn index_music_library(
    directory: String,
    profile_id: Option<String>,
    playlist_name: Option<String>,
    app: tauri::AppHandle,
) -> Result<Vec<Track>, String> {
    let app = app.clone();
    blocking(move || {
        let library = app.state::<LibraryState>();
        let lib = library.0.lock().map_err(|e| e.to_string())?;
        lib.index_directory(profile_id, playlist_name, directory)
    })
    .await
}

#[tauri::command]
pub async fn list_playlists(
    profile_id: Option<String>,
    library: tauri::State<'_, LibraryState>,
) -> Result<Vec<PlaylistInfo>, String> {
    lock_library(&library)?.list_playlists(profile_id)
}

#[tauri::command]
pub async fn get_library_database_path(
    library: tauri::State<'_, LibraryState>,
) -> Result<String, String> {
    Ok(lock_library(&library)?.db_path())
}

#[tauri::command]
pub async fn get_supported_audio_extensions() -> Result<Vec<String>, String> {
    Ok(supported_audio_extensions())
}

#[tauri::command]
pub async fn get_queue(
    state: tauri::State<'_, PlayerState>,
) -> Result<QueueStateDto, String> {
    let guard = lock_player_state(&state);
    let Some(player) = guard.as_ref() else {
        return Ok(QueueStateDto {
            tracks: Vec::new(),
            current_index: None,
            is_shuffled: false,
        });
    };
    Ok(QueueStateDto {
        tracks: player.queue.tracks().to_vec(),
        current_index: player.queue.current_index(),
        is_shuffled: player.queue.is_shuffled(),
    })
}

#[tauri::command]
pub async fn play_next(
    app: tauri::AppHandle,
) -> Result<Option<String>, String> {
    let app_clone = app.clone();
    let path = blocking(move || {
        with_app_player(&app_clone, |guard| {
            guard.play_next().map_err(|e| e.to_string())
        })
    })
    .await?;

    if let Some(ref p) = path {
        let p = p.clone();
        let app_clone = app.clone();
        let track = blocking(move || {
            let lib = app_clone.state::<LibraryState>();
            let lib = lib.0.lock().map_err(|e| e.to_string())?;
            Ok::<_, String>(resolve_track(&lib, &p))
        })
        .await?;
        sync_bridge_now_playing(&app, &track);
    }

    Ok(path)
}

#[tauri::command]
pub async fn play_previous(
    app: tauri::AppHandle,
) -> Result<Option<String>, String> {
    let app_clone = app.clone();
    let path = blocking(move || {
        with_app_player(&app_clone, |guard| {
            guard.play_previous().map_err(|e| e.to_string())
        })
    })
    .await?;

    if let Some(ref p) = path {
        let p = p.clone();
        let app_clone = app.clone();
        let track = blocking(move || {
            let lib = app_clone.state::<LibraryState>();
            let lib = lib.0.lock().map_err(|e| e.to_string())?;
            Ok::<_, String>(resolve_track(&lib, &p))
        })
        .await?;
        sync_bridge_now_playing(&app, &track);
    }

    Ok(path)
}

#[tauri::command]
pub async fn set_shuffle(
    enabled: bool,
    state: tauri::State<'_, PlayerState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    lock_player(&state)?.queue.set_shuffle(enabled);
    let bridge = app.state::<MediaBridgeState>();
    sync_bridge_playback_mode(&app, &bridge);
    Ok(())
}

#[tauri::command]
pub async fn set_repeat(
    mode: String,
    state: tauri::State<'_, PlayerState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    use crate::audio::player::RepeatMode;

    let repeat = match mode.as_str() {
        "off" => RepeatMode::Off,
        "one" => RepeatMode::One,
        "all" => RepeatMode::All,
        _ => return Err(format!("Invalid repeat mode: {mode}")),
    };
    lock_player(&state)?.repeat = repeat;
    let bridge = app.state::<MediaBridgeState>();
    sync_bridge_playback_mode(&app, &bridge);
    Ok(())
}

#[tauri::command]
pub async fn get_playback_mode(
    state: tauri::State<'_, PlayerState>,
) -> Result<PlaybackModeDto, String> {
    use crate::audio::player::RepeatMode;

    let guard = lock_player_state(&state);
    let Some(player) = guard.as_ref() else {
        return Ok(PlaybackModeDto {
            repeat: RepeatMode::default(),
            shuffle: false,
        });
    };
    Ok(PlaybackModeDto {
        repeat: player.repeat.clone(),
        shuffle: player.queue.is_shuffled(),
    })
}

// ── Playlist CRUD ─────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn create_playlist(
    name: String,
    library: tauri::State<'_, LibraryState>,
) -> Result<PlaylistInfo, String> {
    lock_library(&library)?.create_playlist(&name)
}

#[tauri::command]
pub async fn delete_playlist(
    id: String,
    library: tauri::State<'_, LibraryState>,
) -> Result<(), String> {
    lock_library(&library)?.delete_playlist(&id)
}

#[tauri::command]
pub async fn rename_playlist(
    id: String,
    name: String,
    library: tauri::State<'_, LibraryState>,
) -> Result<(), String> {
    lock_library(&library)?.rename_playlist(&id, &name)
}

#[tauri::command]
pub async fn get_playlist_tracks_by_id(
    id: String,
    library: tauri::State<'_, LibraryState>,
) -> Result<Vec<Track>, String> {
    lock_library(&library)?.get_playlist_tracks(&id)
}

#[tauri::command]
pub async fn add_track_to_playlist_by_id(
    id: String,
    path: String,
    app: tauri::AppHandle,
) -> Result<Track, String> {
    let app = app.clone();
    blocking(move || {
        let local = crate::android_import::materialize_audio_source(&app, &path)?;
        let library = app.state::<LibraryState>();
        let lib = library.0.lock().map_err(|e| e.to_string())?;
        lib.add_track_to_playlist(&id, local.to_string_lossy().into_owned())
    })
    .await
}

#[tauri::command]
pub async fn remove_track_from_playlist_by_id(
    id: String,
    path: String,
    library: tauri::State<'_, LibraryState>,
) -> Result<(), String> {
    lock_library(&library)?.remove_track_from_playlist_by_path(&id, &path)
}

#[tauri::command]
pub async fn clear_playlist_by_id(
    id: String,
    library: tauri::State<'_, LibraryState>,
) -> Result<(), String> {
    lock_library(&library)?.clear_playlist(&id)
}

#[tauri::command]
pub async fn create_album_playlist(
    album: String,
    name: Option<String>,
    app: tauri::AppHandle,
) -> Result<PlaylistInfo, String> {
    let app = app.clone();
    blocking(move || {
        let library = app.state::<LibraryState>();
        let lib = library.0.lock().map_err(|e| e.to_string())?;
        lib.create_album_playlist(&album, name.as_deref())
    })
    .await
}

#[tauri::command]
pub async fn create_artist_playlist(
    artist: String,
    name: Option<String>,
    app: tauri::AppHandle,
) -> Result<PlaylistInfo, String> {
    let app = app.clone();
    blocking(move || {
        let library = app.state::<LibraryState>();
        let lib = library.0.lock().map_err(|e| e.to_string())?;
        lib.create_artist_playlist(&artist, name.as_deref())
    })
    .await
}

// ── Album & artist browsing / querying ────────────────────────────────────────

/// List every distinct album in the library (grouped by album + album artist).
#[tauri::command]
pub async fn list_albums(
    library: tauri::State<'_, LibraryState>,
) -> Result<Vec<AlbumSummaryDto>, String> {
    lock_library(&library)?.list_albums()
}

/// List every distinct artist in the library with track and album counts.
#[tauri::command]
pub async fn list_artists(
    library: tauri::State<'_, LibraryState>,
) -> Result<Vec<ArtistSummaryDto>, String> {
    lock_library(&library)?.list_artists()
}

/// Return every track in an album. Pass `albumArtist` (from an
/// [`AlbumSummaryDto`] or a clicked `Track`'s `album_artist` falling back to
/// `artist`) to keep same-named albums by different artists apart.
#[tauri::command]
pub async fn get_album_tracks(
    album: String,
    album_artist: Option<String>,
    library: tauri::State<'_, LibraryState>,
) -> Result<Vec<Track>, String> {
    lock_library(&library)?.get_tracks_by_album(&album, album_artist.as_deref())
}

/// Return every track by an artist (a discography).
#[tauri::command]
pub async fn get_artist_tracks(
    artist: String,
    library: tauri::State<'_, LibraryState>,
) -> Result<Vec<Track>, String> {
    lock_library(&library)?.get_tracks_by_artist(&artist)
}

#[tauri::command]
pub async fn fetch_lyrics_for_track(
    path: String,
    app: tauri::AppHandle,
) -> Result<Track, String> {
    validate_audio_path(&path)?;
    let p = path.clone();
    let app_clone = app.clone();

    // Resolve under the library lock, then release it before any network I/O
    // so playback controls stay responsive while lyrics are fetched.
    let mut track = blocking(move || {
        let library = app_clone.state::<LibraryState>();
        let lib = library.0.lock().map_err(|e| e.to_string())?;
        Ok(resolve_track(&lib, &p))
    })
    .await?;

    if track.lyrics.is_some() {
        return Ok(track);
    }

    let enriched = blocking(move || {
        let mut track = track;
        enrich_lyrics_online(&mut track);
        Ok(track)
    })
    .await?;
    track = enriched;

    if let (Some(lyrics), Some(source)) = (&track.lyrics.clone(), &track.lyrics_source.clone()) {
        let track_id = track.id.clone();
        let lyrics = lyrics.clone();
        let source = source.clone();
        let app_clone = app.clone();
        let _ = blocking(move || {
            let library = app_clone.state::<LibraryState>();
            let lib = library.0.lock().map_err(|e| e.to_string())?;
            lib.set_track_lyrics(&track_id, &lyrics, &source)
        })
        .await;
    }

    Ok(track)
}

#[tauri::command]
pub async fn play_track_from_specific_playlist(
    playlist_id: String,
    index: usize,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let app_clone = app.clone();
    let (raw_tracks, track) = blocking(move || {
        let library = app_clone.state::<LibraryState>();
        let lib = library.0.lock().map_err(|e| e.to_string())?;
        let tracks = lib.get_playlist_tracks(&playlist_id)?;
        let track = tracks
            .get(index)
            .ok_or_else(|| format!("Track not found at index {index}"))?
            .clone();
        Ok((tracks, track))
    })
    .await?;

    let app_clone = app.clone();
    let raw_track_paths: Vec<String> = raw_tracks.iter().map(|t| t.path.clone()).collect();
    let materialized_paths = blocking(move || {
        Ok::<_, String>(raw_track_paths
            .into_iter()
            .map(|path| {
                crate::android_import::materialize_audio_source(&app_clone, &path)
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|e| {
                        tracing::warn!("Failed to materialize track {path}: {e}");
                        path
                    })
            })
            .collect::<Vec<_>>())
    })
    .await?;

    let local_path = materialized_paths
        .get(index)
        .cloned()
        .unwrap_or_default();

    if local_path.is_empty() {
        return Err(format!("Audio file not found for track at index {index}"));
    }

    let tracks: Vec<Track> = raw_tracks
        .into_iter()
        .zip(materialized_paths.into_iter())
        .map(|(mut t, p)| { t.path = p; t })
        .collect();

    let app_clone = app.clone();
    let tracks_clone = tracks.clone();
    blocking(move || {
        with_app_player(&app_clone, |player| {
            sync_queue_from_tracks(player, &tracks_clone, index);
            player.play(&local_path).map_err(|e| format!("Playback failed: {e}"))
        })
    })
    .await?;

    sync_bridge_now_playing(&app, &track);
    Ok(())
}

// ── Queue manipulation ────────────────────────────────────────────────────────

#[tauri::command]
pub async fn add_to_queue(
    path: String,
    state: tauri::State<'_, PlayerState>,
) -> Result<(), String> {
    validate_audio_path(&path)?;
    lock_player(&state)?.enqueue(&path);
    Ok(())
}

#[tauri::command]
pub async fn queue_insert_next(
    path: String,
    state: tauri::State<'_, PlayerState>,
) -> Result<(), String> {
    validate_audio_path(&path)?;
    lock_player(&state)?.insert_next(&path);
    Ok(())
}

#[tauri::command]
pub async fn remove_from_queue(
    index: usize,
    state: tauri::State<'_, PlayerState>,
) -> Result<Option<String>, String> {
    Ok(lock_player(&state)?.remove_from_queue(index))
}

#[tauri::command]
pub async fn move_queue_track(
    from: usize,
    to: usize,
    state: tauri::State<'_, PlayerState>,
) -> Result<(), String> {
    let moved = lock_player(&state)?.move_queue_track(from, to);
    if moved {
        Ok(())
    } else {
        Err("Invalid queue move".into())
    }
}

#[tauri::command]
pub async fn clear_queue(
    state: tauri::State<'_, PlayerState>,
) -> Result<(), String> {
    lock_player(&state)?.clear_upcoming();
    Ok(())
}

#[tauri::command]
pub async fn get_queue_tracks(
    state: tauri::State<'_, PlayerState>,
    library: tauri::State<'_, LibraryState>,
) -> Result<QueueDto, String> {
    let (paths, current_index, is_shuffled) = {
        let guard = lock_player_state(&state);
        match guard.as_ref() {
            Some(player) => (
                player.queue.tracks().to_vec(),
                player.queue.current_index(),
                player.queue.is_shuffled(),
            ),
            None => (Vec::new(), None, false),
        }
    };

    let lookup = lock_library(&library)?.get_tracks_by_paths(&paths)?;
    let tracks: Vec<Track> = paths
        .iter()
        .enumerate()
        .map(|(i, path)| match lookup.get(i).and_then(|o| o.as_ref()) {
            Some(track) => track.clone(),
            None => placeholder_track(path),
        })
        .collect();

    Ok(QueueDto {
        tracks,
        current_index,
        is_shuffled,
    })
}

#[tauri::command]
pub async fn play_track_from_queue(
    index: usize,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let app_clone = app.clone();
    let path = blocking(move || {
        with_app_player(&app_clone, |guard| {
            guard.jump_to_queue_index(index).map_err(|e| e.to_string())
        })
    })
    .await?;

    if let Some(ref p) = path {
        let p = p.clone();
        let app_clone = app.clone();
        let track = blocking(move || {
            let lib = app_clone.state::<LibraryState>();
            let lib = lib.0.lock().map_err(|e| e.to_string())?;
            Ok::<_, String>(resolve_track(&lib, &p))
        })
        .await?;
        sync_bridge_now_playing(&app, &track);
    }

    Ok(())
}

// ── Playlist export / import ─────────────────────────────────────────────────

#[tauri::command]
pub async fn export_playlist(
    playlist_id: String,
    path: String,
    export_format: String,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let app = app.clone();
    blocking(move || {
        let library = app.state::<LibraryState>();
        let lib = library.0.lock().map_err(|e| e.to_string())?;
        let expected_ext = match export_format.as_str() {
            "m3u" => "m3u",
            "json" => "json",
            _ => return Err(format!("Unknown export format: {export_format}")),
        };
        validate_safe_output_path(&path, expected_ext)?;
        match export_format.as_str() {
            "m3u" => lib.export_playlist_m3u(&playlist_id, &path),
            "json" => lib.export_playlist_json(&playlist_id, &path),
            _ => unreachable!(),
        }
    })
    .await
}

#[tauri::command]
pub async fn import_playlist(
    path: String,
    name: Option<String>,
    app: tauri::AppHandle,
) -> Result<ImportResultDto, String> {
    let app = app.clone();
    let extension = Path::new(&path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    let (playlist_id, tracks) = match extension.as_str() {
        "json" => {
            blocking({
                let app = app.clone();
                move || {
                    let library = app.state::<LibraryState>();
                    let lib = library.0.lock().map_err(|e| e.to_string())?;
                    lib.import_playlist_json(&path, name.as_deref())
                }
            })
            .await?
        }
        "m3u" | "m3u8" => {
            let app = app.clone();
            blocking(move || {
                let library = app.state::<LibraryState>();
                let lib = library.0.lock().map_err(|e| e.to_string())?;
                lib.import_playlist_m3u(&path, name.as_deref())
            })
            .await?
        }
        _ => return Err(format!("Unsupported playlist file format: .{extension}")),
    };

    let pid = playlist_id.clone();
    let info = blocking(move || {
        let library = app.state::<LibraryState>();
        let lib = library.0.lock().map_err(|e| e.to_string())?;
        lib.get_playlist_info(&pid)?
            .ok_or_else(|| "Imported playlist not found".to_string())
    })
    .await?;

    Ok(ImportResultDto {
        playlist_id,
        playlist_name: info.name,
        track_count: tracks.len(),
    })
}

// ── Audio output devices ─────────────────────────────────────────────────────

#[tauri::command]
pub async fn list_output_devices() -> Result<Vec<String>, String> {
    Ok(AudioPlayer::list_output_devices())
}

#[tauri::command]
pub async fn set_output_device(
    device_name: String,
    state: tauri::State<'_, PlayerState>,
) -> Result<(), String> {
    let mut slot = lock_player_state(&state);
    let guard = ensure_player(&mut slot)?;

    // Save state from the current player before replacing it.
    let was_playing = guard.is_playing();
    let was_paused = guard.is_paused();
    let current_path = guard.get_current_path().and_then(|p| p.to_str().map(String::from));
    let position = guard.position_seconds();
    let volume = guard.volume();
    let queue = std::mem::take(&mut guard.queue);
    let repeat = guard.repeat.clone();
    let eq_config = guard.eq_config.lock().unwrap().clone();
    let eq_version = *guard.eq_version.lock().unwrap();

    // Build a new player on the requested device.
    let mut new_player = AudioPlayer::new_with_device(&device_name)?;
    new_player.queue = queue;
    new_player.repeat = repeat;
    new_player.set_volume(volume)?;
    *new_player.eq_config.lock().unwrap() = eq_config;
    *new_player.eq_version.lock().unwrap() = eq_version;

    // Resume playback if something was playing.
    if let Some(ref path) = current_path {
        if was_playing || was_paused {
            new_player.play(path)?;
            if position > 0.0 {
                new_player.seek(position)?;
            }
            if was_paused {
                new_player.pause()?;
            }
        }
    }

    *slot = Some(new_player);
    Ok(())
}

// ── OS media controls ─────────────────────────────────────────────────────────

/// Called by the frontend whenever the currently playing track changes.
/// Pushes rich metadata (title, artist, album, duration, cover art URL) to the
/// OS media interface so it shows up in the system media overlay / Control Center.
#[tauri::command]
pub async fn update_media_metadata(
    metadata: TrackMetadata,
    bridge: tauri::State<'_, MediaBridgeState>,
) -> Result<(), String> {
    bridge.0.set_metadata(metadata);
    Ok(())
}

/// Called periodically (every 500 ms) by the frontend to keep the OS media
/// interface playback position in sync with the actual audio clock.
#[tauri::command]
pub async fn update_media_position(
    position_seconds: f64,
    is_playing: bool,
    bridge: tauri::State<'_, MediaBridgeState>,
) -> Result<(), String> {
    bridge.0.update_position(position_seconds, is_playing);
    Ok(())
}

/// Clear the OS media session when nothing is loaded (Stopped, no metadata).
#[tauri::command]
pub async fn clear_media_session(
    bridge: tauri::State<'_, MediaBridgeState>,
) -> Result<(), String> {
    bridge.0.set_stopped();
    Ok(())
}

// ── Window / app settings ─────────────────────────────────────────────────────

fn lock_settings<'a>(
    state: &'a tauri::State<'a, AppSettingsState>,
) -> Result<std::sync::MutexGuard<'a, AppSettings>, String> {
    state.0.lock().map_err(lock_poisoned)
}

/// Return what the window close button currently does.
#[tauri::command]
pub fn get_close_action(
    state: tauri::State<'_, AppSettingsState>,
) -> Result<CloseAction, String> {
    Ok(lock_settings(&state)?.close_action)
}

/// Set what the window close button does.
#[tauri::command]
pub fn set_close_action(
    action: CloseAction,
    state: tauri::State<'_, AppSettingsState>,
    app: tauri::AppHandle,
) -> Result<CloseAction, String> {
    let mut settings = lock_settings(&state)?;
    settings.close_action = action;
    settings.save(&app)?;
    Ok(settings.close_action)
}

/// Toggle the window close button between quitting and hiding the window.
#[tauri::command]
pub fn toggle_close_action(
    state: tauri::State<'_, AppSettingsState>,
    app: tauri::AppHandle,
) -> Result<CloseAction, String> {
    let mut settings = lock_settings(&state)?;
    settings.toggle_close_action();
    settings.save(&app)?;
    Ok(settings.close_action)
}
