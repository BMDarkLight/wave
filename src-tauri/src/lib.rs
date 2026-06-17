mod audio;
mod commands;
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

    let player = AudioPlayer::new()
        .expect("Failed to initialize audio player");

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
            commands::play_track_from_playlist,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
