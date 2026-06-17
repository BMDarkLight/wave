use std::sync::Mutex;
use crate::audio::player::AudioPlayer;
use crate::library::{Library, PlaylistInfo};
use crate::metadata::{supported_audio_extensions, Track};

pub struct PlayerState(pub Mutex<AudioPlayer>);
pub struct LibraryState(pub Mutex<Library>);

// ── Helpers ───────────────────────────────────────────────────────────────────

fn lock_player<'a>(state: &'a tauri::State<'a, PlayerState>) -> Result<std::sync::MutexGuard<'a, AudioPlayer>, String> {
    state.0.lock().map_err(|_| "Failed to lock player state".to_string())
}

fn lock_library<'a>(state: &'a tauri::State<'a, LibraryState>) -> Result<std::sync::MutexGuard<'a, Library>, String> {
    state.0.lock().map_err(|_| "Failed to lock library state".to_string())
}

// ── Playback commands ─────────────────────────────────────────────────────────

#[tauri::command]
pub fn play_track(path: String, state: tauri::State<PlayerState>) -> Result<(), String> {
    lock_player(&state)?.play(&path)
}

#[tauri::command]
pub fn pause_track(state: tauri::State<PlayerState>) -> Result<(), String> {
    lock_player(&state)?.pause()
}

#[tauri::command]
pub fn resume_track(state: tauri::State<PlayerState>) -> Result<(), String> {
    lock_player(&state)?.resume()
}

#[tauri::command]
pub fn stop_track(state: tauri::State<PlayerState>) -> Result<(), String> {
    lock_player(&state)?.stop()
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
pub fn seek_track(seconds: f64, state: tauri::State<PlayerState>) -> Result<(), String> {
    lock_player(&state)?.seek(seconds)
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
) -> Result<(), String> {
    let track = lock_library(&library)?
        .get_default_playlist_track(index)?
        .ok_or("Track not found at that index")?;
    lock_player(&player)?.play(&track.path)
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
pub fn play_next(state: tauri::State<PlayerState>) -> Result<Option<String>, String> {
    state
        .0
        .lock()
        .map_err(|_| "Failed to lock player state".to_string())?
        .play_next()
}

#[tauri::command]
pub fn play_previous(state: tauri::State<PlayerState>) -> Result<Option<String>, String> {
    state
        .0
        .lock()
        .map_err(|_| "Failed to lock player state".to_string())?
        .play_previous()
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
