use std::path::Path;
use std::sync::Mutex;

use crate::audio::player::AudioPlayer;
use crate::dto::{
    AlbumSummaryDto, ArtistSummaryDto, ImportResultDto, PlaybackModeDto, PlaybackStateDto,
    QueueDto, QueueStateDto,
};
use crate::library::{Library, PlaylistInfo};
use crate::media_controls::{MediaBridge, TrackMetadata};
use crate::metadata::{is_supported_audio_file, supported_audio_extensions, Track};
use tauri::Manager;
use walkdir::WalkDir;

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
pub async fn play_track(
    path: String,
    app: tauri::AppHandle,
    bridge: tauri::State<'_, MediaBridgeState>,
) -> Result<(), String> {
    let app = app.clone();
    blocking(move || {
        let player = app.state::<PlayerState>();
        let mut guard = player.0.lock().map_err(|e| e.to_string())?;
        guard.play(&path).map_err(|e| e.to_string())
    })
    .await?;
    sync_bridge_playing(&bridge, 0.0);
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
    with_bridge(&bridge, |b| b.set_paused(position));
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
    with_bridge(&bridge, |b| b.set_stopped());
    Ok(())
}

#[tauri::command]
pub async fn get_playback_state(
    state: tauri::State<'_, PlayerState>,
) -> Result<PlaybackStateDto, String> {
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
    with_bridge(&bridge, |b| b.update_position(seconds, playing));
    Ok(())
}

#[tauri::command]
pub async fn set_volume(
    volume: f32,
    state: tauri::State<'_, PlayerState>,
) -> Result<(), String> {
    lock_player(&state)?.set_volume(volume)?;
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
        let library = app.state::<LibraryState>();
        let lib = library.0.lock().map_err(|e| e.to_string())?;
        lib.add_track_to_default_playlist(path)
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
    let (tracks, track) = blocking(move || {
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

    let track_path = track.path.clone();
    let app_clone = app.clone();
    blocking(move || {
        let player = app_clone.state::<PlayerState>();
        let mut player = player.0.lock().map_err(|e| e.to_string())?;
        sync_queue_from_tracks(&mut player, &tracks, index);
        player.play(&track_path).map_err(|e| e.to_string())
    })
    .await?;

    let bridge = app.state::<MediaBridgeState>();
    if let Ok(mut bridge) = bridge.0.lock() {
        bridge.now_playing(&TrackMetadata {
            title: Some(track.title),
            artist: Some(track.artist),
            album: Some(track.album),
            duration_seconds: track.duration_seconds,
            cover_url: track.cover_art_data_url,
        });
    }
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
    let player = lock_player(&state)?;
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
        let player = app_clone.state::<PlayerState>();
        let mut guard = player.0.lock().map_err(|e| e.to_string())?;
        guard.play_next().map_err(|e| e.to_string())
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
        sync_bridge_now_playing(&app.state::<MediaBridgeState>(), &track);
    }

    Ok(path)
}

#[tauri::command]
pub async fn play_previous(
    app: tauri::AppHandle,
) -> Result<Option<String>, String> {
    let app_clone = app.clone();
    let path = blocking(move || {
        let player = app_clone.state::<PlayerState>();
        let mut guard = player.0.lock().map_err(|e| e.to_string())?;
        guard.play_previous().map_err(|e| e.to_string())
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
        sync_bridge_now_playing(&app.state::<MediaBridgeState>(), &track);
    }

    Ok(path)
}

#[tauri::command]
pub async fn set_shuffle(
    enabled: bool,
    state: tauri::State<'_, PlayerState>,
) -> Result<(), String> {
    lock_player(&state)?.queue.set_shuffle(enabled);
    Ok(())
}

#[tauri::command]
pub async fn set_repeat(
    mode: String,
    state: tauri::State<'_, PlayerState>,
) -> Result<(), String> {
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
pub async fn get_playback_mode(
    state: tauri::State<'_, PlayerState>,
) -> Result<PlaybackModeDto, String> {
    let player = lock_player(&state)?;
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
        let library = app.state::<LibraryState>();
        let lib = library.0.lock().map_err(|e| e.to_string())?;
        lib.add_track_to_playlist(&id, path)
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
pub async fn play_track_from_specific_playlist(
    playlist_id: String,
    index: usize,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let app_clone = app.clone();
    let (tracks, track) = blocking(move || {
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

    let track_path = track.path.clone();
    let app_clone = app.clone();
    blocking(move || {
        let player = app_clone.state::<PlayerState>();
        let mut player = player.0.lock().map_err(|e| e.to_string())?;
        sync_queue_from_tracks(&mut player, &tracks, index);
        player.play(&track_path).map_err(|e| e.to_string())
    })
    .await?;

    sync_bridge_now_playing(&app.state::<MediaBridgeState>(), &track);
    Ok(())
}

// ── Queue manipulation ────────────────────────────────────────────────────────

#[tauri::command]
pub async fn add_to_queue(
    path: String,
    state: tauri::State<'_, PlayerState>,
) -> Result<(), String> {
    lock_player(&state)?.enqueue(&path);
    Ok(())
}

#[tauri::command]
pub async fn queue_insert_next(
    path: String,
    state: tauri::State<'_, PlayerState>,
) -> Result<(), String> {
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
pub async fn play_track_from_queue(
    index: usize,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let app_clone = app.clone();
    let path = blocking(move || {
        let player = app_clone.state::<PlayerState>();
        let mut guard = player.0.lock().map_err(|e| e.to_string())?;
        guard.jump_to_queue_index(index).map_err(|e| e.to_string())
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
        sync_bridge_now_playing(&app.state::<MediaBridgeState>(), &track);
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
        match export_format.as_str() {
            "m3u" => lib.export_playlist_m3u(&playlist_id, &path),
            "json" => lib.export_playlist_json(&playlist_id, &path),
            _ => Err(format!("Unknown export format: {export_format}")),
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
    let mut guard = state.0.lock().map_err(|e| e.to_string())?;

    // Save state from the current player before replacing it.
    let was_playing = guard.is_playing();
    let was_paused = guard.is_paused();
    let current_path = guard.get_current_path().and_then(|p| p.to_str().map(String::from));
    let position = guard.position_seconds();
    let volume = guard.volume();
    let queue = std::mem::take(&mut guard.queue);
    let repeat = guard.repeat.clone();

    // Build a new player on the requested device.
    let mut new_player = AudioPlayer::new_with_device(&device_name)?;
    new_player.queue = queue;
    new_player.repeat = repeat;
    new_player.set_volume(volume)?;

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

    *guard = new_player;
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
    with_bridge(&bridge, |b| b.set_metadata(&metadata));
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
    with_bridge(&bridge, |b| b.update_position(position_seconds, is_playing));
    Ok(())
}
