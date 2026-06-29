use std::path::Path;
use std::sync::Mutex;

use crate::audio::player::AudioPlayer;
use crate::dto::{ImportResultDto, PlaybackModeDto, PlaybackStateDto, QueueDto, QueueStateDto};
use crate::library::{Library, PlaylistInfo};
use crate::media_controls::{MediaBridge, TrackMetadata};
use crate::metadata::{supported_audio_extensions, Track};

pub struct PlayerState(pub Mutex<AudioPlayer>);
pub struct LibraryState(pub Mutex<Library>);
pub struct MediaBridgeState(pub Mutex<MediaBridge>);

// ── Helpers ───────────────────────────────────────────────────────────────────

fn lock_poisoned<T>(_: std::sync::PoisonError<T>) -> String {
    "State lock poisoned".to_string()
}

fn lock_player<'a>(
    state: &'a tauri::State<'a, PlayerState>,
) -> Result<std::sync::MutexGuard<'a, AudioPlayer>, String> {
    state.0.lock().map_err(lock_poisoned)
}

fn lock_library<'a>(
    state: &'a tauri::State<'a, LibraryState>,
) -> Result<std::sync::MutexGuard<'a, Library>, String> {
    state.0.lock().map_err(lock_poisoned)
}

/// Lock the bridge if it was successfully initialized; silently skip otherwise.
fn with_bridge<F>(bridge: &tauri::State<'_, MediaBridgeState>, f: F)
where
    F: FnOnce(&mut MediaBridge),
{
    if let Ok(mut bridge) = bridge.0.lock() {
        f(&mut bridge);
    }
}

fn sync_bridge_playing(bridge: &tauri::State<MediaBridgeState>, position_secs: f64) {
    with_bridge(bridge, |b| b.set_playing(position_secs));
}

fn sync_queue_from_tracks(player: &mut AudioPlayer, tracks: &[Track], index: usize) {
    let paths: Vec<String> = tracks.iter().map(|track| track.path.clone()).collect();
    player.queue.set_tracks(paths);
    if player.queue.jump(index).is_none() {
        tracing::warn!("Failed to align playback queue with playlist index {index}");
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

/// Push track metadata to the OS media bridge.
fn sync_bridge_now_playing(bridge: &tauri::State<MediaBridgeState>, track: &Track) {
    with_bridge(bridge, |b| {
        b.now_playing(&TrackMetadata {
            title: Some(track.title.clone()),
            artist: Some(track.artist.clone()),
            album: Some(track.album.clone()),
            duration_seconds: track.duration_seconds,
            cover_url: track.cover_art_data_url.clone(),
        });
    });
}

// ── Playback commands ─────────────────────────────────────────────────────────

#[tauri::command]
pub fn play_track(
    path: String,
    state: tauri::State<PlayerState>,
    bridge: tauri::State<MediaBridgeState>,
) -> Result<(), String> {
    lock_player(&state)?.play(&path)?;
    sync_bridge_playing(&bridge, 0.0);
    Ok(())
}

#[tauri::command]
pub fn pause_track(
    state: tauri::State<PlayerState>,
    bridge: tauri::State<MediaBridgeState>,
) -> Result<(), String> {
    let position = {
        let mut player = lock_player(&state)?;
        let position = player.position_seconds();
        player.pause()?;
        position
    };
    with_bridge(&bridge, |b| b.set_paused(position));
    Ok(())
}

#[tauri::command]
pub fn resume_track(
    state: tauri::State<PlayerState>,
    bridge: tauri::State<MediaBridgeState>,
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
pub fn stop_track(
    state: tauri::State<PlayerState>,
    bridge: tauri::State<MediaBridgeState>,
) -> Result<(), String> {
    lock_player(&state)?.stop()?;
    with_bridge(&bridge, |b| b.set_stopped());
    Ok(())
}

#[tauri::command]
pub fn get_playback_state(state: tauri::State<PlayerState>) -> Result<PlaybackStateDto, String> {
    let player = lock_player(&state)?;
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
    })
}

#[tauri::command]
pub fn seek_track(
    seconds: f64,
    state: tauri::State<PlayerState>,
    bridge: tauri::State<MediaBridgeState>,
) -> Result<(), String> {
    let playing = {
        let mut player = lock_player(&state)?;
        player.seek(seconds)?;
        player.is_playing()
    };
    with_bridge(&bridge, |b| b.update_position(seconds, playing));
    Ok(())
}

#[tauri::command]
pub fn set_volume(volume: f32, state: tauri::State<PlayerState>) -> Result<(), String> {
    lock_player(&state)?.set_volume(volume)?;
    Ok(())
}

// ── Library / playlist commands ───────────────────────────────────────────────

#[tauri::command]
pub fn add_track_to_playlist(
    path: String,
    library: tauri::State<LibraryState>,
) -> Result<Track, String> {
    lock_library(&library)?.add_track_to_default_playlist(path)
}

#[tauri::command]
pub fn remove_track_from_playlist(
    path: String,
    library: tauri::State<LibraryState>,
) -> Result<(), String> {
    lock_library(&library)?.remove_track_from_default_playlist(path)
}

#[tauri::command]
pub fn get_playlist(library: tauri::State<LibraryState>) -> Result<Vec<Track>, String> {
    lock_library(&library)?.get_default_playlist_tracks()
}

#[tauri::command]
pub fn clear_playlist(library: tauri::State<LibraryState>) -> Result<(), String> {
    lock_library(&library)?.clear_default_playlist()
}

#[tauri::command]
pub fn play_track_from_playlist(
    index: usize,
    player: tauri::State<PlayerState>,
    library: tauri::State<LibraryState>,
    bridge: tauri::State<MediaBridgeState>,
) -> Result<(), String> {
    let tracks = lock_library(&library)?.get_default_playlist_tracks()?;
    let track = tracks
        .get(index)
        .ok_or_else(|| format!("Track not found at index {index}"))?
        .clone();

    {
        let mut player = lock_player(&player)?;
        sync_queue_from_tracks(&mut player, &tracks, index);
        player.play(&track.path)?;
    }

    with_bridge(&bridge, |b| {
        b.now_playing(&TrackMetadata {
            title: Some(track.title),
            artist: Some(track.artist),
            album: Some(track.album),
            duration_seconds: track.duration_seconds,
            cover_url: track.cover_art_data_url,
        });
    });
    Ok(())
}

#[tauri::command]
pub fn index_music_library(
    directory: String,
    profile_id: Option<String>,
    playlist_name: Option<String>,
    library: tauri::State<LibraryState>,
) -> Result<Vec<Track>, String> {
    lock_library(&library)?.index_directory(profile_id, playlist_name, directory)
}

#[tauri::command]
pub fn list_playlists(
    profile_id: Option<String>,
    library: tauri::State<LibraryState>,
) -> Result<Vec<PlaylistInfo>, String> {
    lock_library(&library)?.list_playlists(profile_id)
}

#[tauri::command]
pub fn get_library_database_path(library: tauri::State<LibraryState>) -> Result<String, String> {
    Ok(lock_library(&library)?.db_path())
}

#[tauri::command]
pub fn get_supported_audio_extensions() -> Result<Vec<String>, String> {
    Ok(supported_audio_extensions())
}

#[tauri::command]
pub fn get_queue(state: tauri::State<PlayerState>) -> Result<QueueStateDto, String> {
    let player = lock_player(&state)?;
    Ok(QueueStateDto {
        tracks: player.queue.tracks().to_vec(),
        current_index: player.queue.current_index(),
        is_shuffled: player.queue.is_shuffled(),
    })
}

#[tauri::command]
pub fn play_next(
    state: tauri::State<PlayerState>,
    bridge: tauri::State<MediaBridgeState>,
) -> Result<Option<String>, String> {
    let result = lock_player(&state)?.play_next()?;
    if result.is_some() {
        sync_bridge_playing(&bridge, 0.0);
    }
    Ok(result)
}

#[tauri::command]
pub fn play_previous(
    state: tauri::State<PlayerState>,
    bridge: tauri::State<MediaBridgeState>,
) -> Result<Option<String>, String> {
    let result = lock_player(&state)?.play_previous()?;
    if result.is_some() {
        sync_bridge_playing(&bridge, 0.0);
    }
    Ok(result)
}

#[tauri::command]
pub fn set_shuffle(enabled: bool, state: tauri::State<PlayerState>) -> Result<(), String> {
    lock_player(&state)?.queue.set_shuffle(enabled);
    Ok(())
}

#[tauri::command]
pub fn set_repeat(mode: String, state: tauri::State<PlayerState>) -> Result<(), String> {
    use crate::audio::player::RepeatMode;

    let repeat = match mode.as_str() {
        "off" => RepeatMode::Off,
        "one" => RepeatMode::One,
        "all" => RepeatMode::All,
        _ => return Err(format!("Invalid repeat mode: {mode}")),
    };
    lock_player(&state)?.repeat = repeat;
    Ok(())
}

#[tauri::command]
pub fn get_playback_mode(state: tauri::State<PlayerState>) -> Result<PlaybackModeDto, String> {
    let player = lock_player(&state)?;
    Ok(PlaybackModeDto {
        repeat: player.repeat.clone(),
        shuffle: player.queue.is_shuffled(),
    })
}

// ── Playlist CRUD ─────────────────────────────────────────────────────────────

#[tauri::command]
pub fn create_playlist(
    name: String,
    library: tauri::State<LibraryState>,
) -> Result<PlaylistInfo, String> {
    lock_library(&library)?.create_playlist(&name)
}

#[tauri::command]
pub fn delete_playlist(id: String, library: tauri::State<LibraryState>) -> Result<(), String> {
    lock_library(&library)?.delete_playlist(&id)
}

#[tauri::command]
pub fn rename_playlist(
    id: String,
    name: String,
    library: tauri::State<LibraryState>,
) -> Result<(), String> {
    lock_library(&library)?.rename_playlist(&id, &name)
}

#[tauri::command]
pub fn get_playlist_tracks_by_id(
    id: String,
    library: tauri::State<LibraryState>,
) -> Result<Vec<Track>, String> {
    lock_library(&library)?.get_playlist_tracks(&id)
}

#[tauri::command]
pub fn add_track_to_playlist_by_id(
    id: String,
    path: String,
    library: tauri::State<LibraryState>,
) -> Result<Track, String> {
    lock_library(&library)?.add_track_to_playlist(&id, path)
}

#[tauri::command]
pub fn remove_track_from_playlist_by_id(
    id: String,
    path: String,
    library: tauri::State<LibraryState>,
) -> Result<(), String> {
    lock_library(&library)?.remove_track_from_playlist_by_path(&id, &path)
}

#[tauri::command]
pub fn clear_playlist_by_id(
    id: String,
    library: tauri::State<LibraryState>,
) -> Result<(), String> {
    lock_library(&library)?.clear_playlist(&id)
}

#[tauri::command]
pub fn play_track_from_specific_playlist(
    playlist_id: String,
    index: usize,
    player: tauri::State<PlayerState>,
    library: tauri::State<LibraryState>,
    bridge: tauri::State<MediaBridgeState>,
) -> Result<(), String> {
    let tracks = lock_library(&library)?.get_playlist_tracks(&playlist_id)?;
    let track = tracks
        .get(index)
        .ok_or_else(|| format!("Track not found at index {index}"))?
        .clone();

    {
        let mut player = lock_player(&player)?;
        sync_queue_from_tracks(&mut player, &tracks, index);
        player.play(&track.path)?;
    }

    sync_bridge_now_playing(&bridge, &track);
    Ok(())
}

// ── Queue manipulation ────────────────────────────────────────────────────────

#[tauri::command]
pub fn add_to_queue(
    path: String,
    state: tauri::State<PlayerState>,
) -> Result<(), String> {
    lock_player(&state)?.enqueue(&path);
    Ok(())
}

#[tauri::command]
pub fn queue_insert_next(
    path: String,
    state: tauri::State<PlayerState>,
) -> Result<(), String> {
    lock_player(&state)?.insert_next(&path);
    Ok(())
}

#[tauri::command]
pub fn remove_from_queue(
    index: usize,
    state: tauri::State<PlayerState>,
) -> Result<Option<String>, String> {
    Ok(lock_player(&state)?.remove_from_queue(index))
}

#[tauri::command]
pub fn clear_queue(state: tauri::State<PlayerState>) -> Result<(), String> {
    lock_player(&state)?.clear_upcoming();
    Ok(())
}

#[tauri::command]
pub fn get_queue_tracks(
    state: tauri::State<PlayerState>,
    library: tauri::State<LibraryState>,
) -> Result<QueueDto, String> {
    let (paths, current_index, is_shuffled) = {
        let player = lock_player(&state)?;
        (
            player.queue.tracks().to_vec(),
            player.queue.current_index(),
            player.queue.is_shuffled(),
        )
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
pub fn play_track_from_queue(
    index: usize,
    player: tauri::State<PlayerState>,
    library: tauri::State<LibraryState>,
    bridge: tauri::State<MediaBridgeState>,
) -> Result<(), String> {
    let path = {
        let mut player = lock_player(&player)?;
        player.jump_to_queue_index(index)?
    };

    if let Some(ref path) = path {
        let lib = lock_library(&library)?;
        let track = resolve_track(&lib, path);
        sync_bridge_now_playing(&bridge, &track);
    }
    Ok(())
}

// ── Playlist export / import ─────────────────────────────────────────────────

#[tauri::command]
pub fn export_playlist(
    id: String,
    path: String,
    format: String,
    library: tauri::State<LibraryState>,
) -> Result<(), String> {
    let lib = lock_library(&library)?;
    match format.as_str() {
        "m3u" => lib.export_playlist_m3u(&id, &path),
        "json" => lib.export_playlist_json(&id, &path),
        _ => Err(format!("Unknown export format: {format}")),
    }
}

#[tauri::command]
pub fn import_playlist(
    path: String,
    name: Option<String>,
    library: tauri::State<LibraryState>,
) -> Result<ImportResultDto, String> {
    let lib = lock_library(&library)?;
    let extension = Path::new(&path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    let (playlist_id, tracks) = match extension.as_str() {
        "json" => lib.import_playlist_json(&path, name.as_deref())?,
        "m3u" | "m3u8" => lib.import_playlist_m3u(&path, name.as_deref())?,
        _ => return Err(format!("Unsupported playlist file format: .{extension}")),
    };

    let info = lib
        .get_playlist_info(&playlist_id)?
        .ok_or("Imported playlist not found")?;

    Ok(ImportResultDto {
        playlist_id,
        playlist_name: info.name,
        track_count: tracks.len(),
    })
}

// ── OS media controls ─────────────────────────────────────────────────────────

/// Called by the frontend whenever the currently playing track changes.
/// Pushes rich metadata (title, artist, album, duration, cover art URL) to the
/// OS media interface so it shows up in the system media overlay / Control Center.
#[tauri::command]
pub fn update_media_metadata(
    metadata: TrackMetadata,
    bridge: tauri::State<MediaBridgeState>,
) -> Result<(), String> {
    with_bridge(&bridge, |b| b.set_metadata(&metadata));
    Ok(())
}
