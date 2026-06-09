use std::sync::Mutex;
use crate::audio::player::AudioPlayer;
use crate::library::{Library, PlaylistInfo};
use crate::metadata::{supported_audio_extensions, Track};

pub struct PlayerState(pub Mutex<AudioPlayer>);
pub struct LibraryState(pub Mutex<Library>);

#[tauri::command]
pub fn play_track(path: String, state: tauri::State<PlayerState>) -> Result<(), String> {
    state.0.lock().unwrap().play(&path)
}

#[tauri::command]
pub fn pause_track(state: tauri::State<PlayerState>) -> Result<(), String> {
    state.0.lock().unwrap().pause()
}

#[tauri::command]
pub fn resume_track(state: tauri::State<PlayerState>) -> Result<(), String> {
    state.0.lock().unwrap().resume()
}

#[tauri::command]
pub fn stop_track(state: tauri::State<PlayerState>) -> Result<(), String> {
    state.0.lock().unwrap().stop()
}

#[tauri::command]
pub fn get_playback_state(state: tauri::State<PlayerState>) -> Result<serde_json::Value, String> {
    let player = state.0.lock().unwrap();
    Ok(serde_json::json!({
        "is_playing": player.is_playing(),
        "is_paused": player.is_paused(),
        "current_path": player.get_current_path()
            .and_then(|p| p.to_str().map(|s| s.to_string())),
        "position_seconds": player.position_seconds(),
        "duration_seconds": player.duration_seconds(),
        "volume": player.volume(),
    }))
}

#[tauri::command]
pub fn seek_track(seconds: f64, state: tauri::State<PlayerState>) -> Result<(), String> {
    state.0.lock().unwrap().seek(seconds)
}

#[tauri::command]
pub fn set_volume(volume: f32, state: tauri::State<PlayerState>) -> Result<(), String> {
    state.0.lock().unwrap().set_volume(volume)
}

#[tauri::command]
pub fn add_track_to_playlist(
    path: String,
    library: tauri::State<LibraryState>,
) -> Result<Track, String> {
    library.0.lock().unwrap().add_track_to_default_playlist(path)
}

#[tauri::command]
pub fn remove_track_from_playlist(
    index: usize,
    library: tauri::State<LibraryState>,
) -> Result<(), String> {
    library.0.lock().unwrap().remove_track_from_default_playlist(index)
}

#[tauri::command]
pub fn get_playlist(library: tauri::State<LibraryState>) -> Result<Vec<Track>, String> {
    library.0.lock().unwrap().get_default_playlist_tracks()
}

#[tauri::command]
pub fn clear_playlist(library: tauri::State<LibraryState>) -> Result<(), String> {
    library.0.lock().unwrap().clear_default_playlist()
}

#[tauri::command]
pub fn play_track_from_playlist(
    index: usize,
    player: tauri::State<PlayerState>,
    library: tauri::State<LibraryState>,
) -> Result<(), String> {
    let track = library
        .0
        .lock()
        .unwrap()
        .get_default_playlist_track(index)?
        .ok_or("Track not found")?;
    player.0.lock().unwrap().play(&track.path)
}

#[tauri::command]
pub fn index_music_library(
    directory: String,
    profile_id: Option<String>,
    playlist_name: Option<String>,
    library: tauri::State<LibraryState>,
) -> Result<Vec<Track>, String> {
    library
        .0
        .lock()
        .unwrap()
        .index_directory(profile_id, playlist_name, directory)
}

#[tauri::command]
pub fn list_playlists(
    profile_id: Option<String>,
    library: tauri::State<LibraryState>,
) -> Result<Vec<PlaylistInfo>, String> {
    library.0.lock().unwrap().list_playlists(profile_id)
}

#[tauri::command]
pub fn get_library_database_path(library: tauri::State<LibraryState>) -> Result<String, String> {
    Ok(library.0.lock().unwrap().db_path())
}

#[tauri::command]
pub fn get_supported_audio_extensions() -> Result<Vec<String>, String> {
    Ok(supported_audio_extensions())
}
