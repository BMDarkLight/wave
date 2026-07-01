mod audio;
pub mod cli;
mod commands;
mod dto;
mod error;
mod library;
mod media_controls;
mod metadata;

use commands::{LibraryState, MediaBridgeState, PlayerState};
use audio::player::AudioPlayer;
use tauri::Manager;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .with(tracing_subscriber::fmt::layer())
        .init();

    let player = AudioPlayer::new().expect("Failed to initialize audio player");

    let player_state = PlayerState(std::sync::Mutex::new(player));

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(player_state)
        .setup(|app| {
            let library = library::Library::new(app.handle())?;
            app.manage(LibraryState(std::sync::Mutex::new(library)));

            match media_controls::MediaBridge::new(app.handle()) {
                Ok(bridge) => {
                    app.manage(MediaBridgeState(std::sync::Mutex::new(bridge)));
                }
                Err(e) => {
                    tracing::warn!("OS media controls unavailable: {e}");
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::play_track,
            commands::pause_track,
            commands::resume_track,
            commands::stop_track,
            commands::get_playback_state,
            commands::seek_track,
            commands::set_volume,
            commands::add_track_to_playlist,
            commands::remove_track_from_playlist,
            commands::get_playlist,
            commands::clear_playlist,
            commands::add_track_to_favorites,
            commands::remove_track_from_favorites,
            commands::get_favorites,
            commands::is_track_in_favorites,
            commands::is_track_in_playlist,
            commands::toggle_favorite,
            commands::clear_favorites,
            commands::play_track_from_playlist,
            commands::scan_directory,
            commands::index_music_library,
            commands::list_playlists,
            commands::get_library_database_path,
            commands::get_supported_audio_extensions,
            commands::get_queue,
            commands::play_next,
            commands::play_previous,
            commands::set_shuffle,
            commands::set_repeat,
            commands::get_playback_mode,
            commands::update_media_metadata,
            commands::update_media_position,
            commands::create_playlist,
            commands::delete_playlist,
            commands::rename_playlist,
            commands::get_playlist_tracks_by_id,
            commands::add_track_to_playlist_by_id,
            commands::remove_track_from_playlist_by_id,
            commands::clear_playlist_by_id,
            commands::play_track_from_specific_playlist,
            commands::create_album_playlist,
            commands::create_artist_playlist,
            commands::list_albums,
            commands::list_artists,
            commands::get_album_tracks,
            commands::get_artist_tracks,
            commands::add_to_queue,
            commands::queue_insert_next,
            commands::remove_from_queue,
            commands::clear_queue,
            commands::get_queue_tracks,
            commands::play_track_from_queue,
            commands::export_playlist,
            commands::import_playlist,
            commands::list_output_devices,
            commands::set_output_device,
            commands::get_eq_settings,
            commands::set_eq_bands,
            commands::set_eq_enabled,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
