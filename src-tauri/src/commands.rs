use std::sync::Mutex;
use crate::audio::player::AudioPlayer;
use crate::library::{Library, PlaylistInfo};
use crate::media_controls::{MediaBridge, TrackMetadata};
use crate::metadata::{supported_audio_extensions, Track};

pub struct PlayerState(pub Mutex<AudioPlayer>);
pub struct LibraryState(pub Mutex<Library>);
pub struct MediaBridgeState(pub Mutex<MediaBridge>);

// ── Helpers ───────────────────────────────────────────────────────────────────

fn lock_player<'a>(state: &'a tauri::State<'a, PlayerState>) -> Result<std::sync::MutexGuard<'a, AudioPlayer>, String> {
    state.0.lock().map_err(|_| "Failed to lock player state".to_string())
}

fn lock_library<'a>(state: &'a tauri::State<'a, LibraryState>) -> Result<std::sync::MutexGuard<'a, Library>, String> {
    state.0.lock().map_err(|_| "Failed to lock library state".to_string())
}

/// Lock the bridge if it was successfully initialized; silently skip otherwise.
fn with_bridge<F>(bridge: &tauri::State<'_, MediaBridgeState>, f: F)
where
    F: FnOnce(&mut MediaBridge),
{
    if let Ok(mut b) = bridge.0.lock() {
        f(&mut b);
    }
}

// ── Playback commands ─────────────────────────────────────────────────────────

#[tauri::command]
pub fn play_track(
    path: String,
    state: tauri::State<PlayerState>,
    bridge: tauri::State<MediaBridgeState>,
) -> Result<(), String> {
    lock_player(&state)?.play(&path)?;
    with_bridge(&bridge, |b| b.set_playing(0.0));
    Ok(())
}

#[tauri::command]
pub fn pause_track(
    state: tauri::State<PlayerState>,
    bridge: tauri::State<MediaBridgeState>,
) -> Result<(), String> {
    let pos = {
        let player = lock_player(&state)?;
        let pos = player.position_seconds();
        drop(player);
        pos
    };
    lock_player(&state)?.pause()?;
    with_bridge(&bridge, |b| b.set_paused(pos));
    Ok(())
}

#[tauri::command]
pub fn resume_track(
    state: tauri::State<PlayerState>,
    bridge: tauri::State<MediaBridgeState>,
) -> Result<(), String> {
    let pos = lock_player(&state)?.position_seconds();
    lock_player(&state)?.resume()?;
    with_bridge(&bridge, |b| b.set_playing(pos));
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
pub fn get_playback_state(state: tauri::State<PlayerState>) -> Result<serde_json::Value, String> {
    let player = lock_player(&state)?;
    Ok(serde_json::json!({
        "is_playing": player.is_playing(),
        "is_paused": player.is_paused(),
        "current_path": player.get_current_path()
            .and_then(|p| p.to_str())
            .map(str::to_string),
        "position_seconds": player.position_seconds(),
        "duration_seconds": player.duration_seconds(),
        "volume": player.volume(),
    }))
}

#[tauri::command]
pub fn seek_track(
    seconds: f64,
    state: tauri::State<PlayerState>,
    bridge: tauri::State<MediaBridgeState>,
) -> Result<(), String> {
    lock_player(&state)?.seek(seconds)?;
    let playing = lock_player(&state)?.is_playing();
    with_bridge(&bridge, |b| b.update_position(seconds, playing));
    Ok(())
}

#[tauri::command]
pub fn set_volume(volume: f32, state: tauri::State<PlayerState>) -> Result<(), String> {
    lock_player(&state)?.set_volume(volume)
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
    index: usize,
    library: tauri::State<LibraryState>,
) -> Result<(), String> {
    lock_library(&library)?.remove_track_from_default_playlist(index)
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
    let track = lock_library(&library)?
        .get_default_playlist_track(index)?
        .ok_or("Track not found at that index")?;
    lock_player(&player)?.play(&track.path)?;
    with_bridge(&bridge, |b| {
        b.now_playing(&TrackMetadata {
            title: Some(track.title.clone()),
            artist: Some(track.artist.clone()),
            album: Some(track.album.clone()),
            duration_seconds: track.duration_seconds,
            cover_url: None,
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
pub fn get_queue(state: tauri::State<PlayerState>) -> Result<serde_json::Value, String> {
    let player = state
        .0
        .lock()
        .map_err(|_| "Failed to lock player state".to_string())?;
    Ok(serde_json::json!({
        "tracks": player.queue.tracks(),
        "current_index": player.queue.current_index(),
        "is_shuffled": player.queue.is_shuffled(),
    }))
}

#[tauri::command]
pub fn play_next(
    state: tauri::State<PlayerState>,
    bridge: tauri::State<MediaBridgeState>,
) -> Result<Option<String>, String> {
    let result = lock_player(&state)?.play_next()?;
    if result.is_some() {
        with_bridge(&bridge, |b| b.set_playing(0.0));
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
        with_bridge(&bridge, |b| b.set_playing(0.0));
    }
    Ok(result)
}

#[tauri::command]
pub fn set_shuffle(enabled: bool, state: tauri::State<PlayerState>) -> Result<(), String> {
    state
        .0
        .lock()
        .map_err(|_| "Failed to lock player state".to_string())?
        .queue
        .set_shuffle(enabled);
    Ok(())
}

#[tauri::command]
pub fn set_repeat(mode: String, state: tauri::State<PlayerState>) -> Result<(), String> {
    use crate::audio::player::RepeatMode;
    let repeat = match mode.as_str() {
        "off" => RepeatMode::Off,
        "one" => RepeatMode::One,
        "all" => RepeatMode::All,
        _ => return Err(format!("Invalid repeat mode: {}", mode)),
    };
    state
        .0
        .lock()
        .map_err(|_| "Failed to lock player state".to_string())?
        .repeat = repeat;
    Ok(())
}

#[tauri::command]
pub fn get_playback_mode(state: tauri::State<PlayerState>) -> Result<serde_json::Value, String> {
    let player = state
        .0
        .lock()
        .map_err(|_| "Failed to lock player state".to_string())?;
    Ok(serde_json::json!({
        "repeat": player.repeat,
        "shuffle": player.queue.is_shuffled(),
    }))
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
