mod app;
mod audio;
pub mod cli;
mod commands;
mod dto;
mod error;
mod integrations;
mod library;
mod metadata;
mod path_validation;
pub mod playback_daemon;
mod os_media;
mod android_import;

pub use app::paths as app_paths;
pub use app::settings as app_settings;
pub use app::single_instance;
pub use integrations::gui_tray;
pub use integrations::media_controls;

use app_settings::AppSettingsState;
use commands::{LibraryState, MediaBridgeState, PlayerState};
use dto::CloseAction;
use tauri::{Manager, WindowEvent};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Single-instance locking is a desktop concern. On Android, HOME/cwd-based
    // lock paths are unreliable and can abort launch before the UI starts.
    #[cfg(not(target_os = "android"))]
    let _instance = single_instance::try_acquire(single_instance::InstanceMode::Gui)
        .unwrap_or_else(|e| {
            eprintln!("{e}");
            std::process::exit(1);
        });

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .with(tracing_subscriber::fmt::layer())
        .init();

    os_media::set_app_user_model_id("app.bmdarklight.wave");

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            // Defer audio device creation until first playback command. Opening
            // cpal/oboe during setup can panic on Android before JNI is ready.
            app.manage(PlayerState(std::sync::Mutex::new(None)));

            let settings = app_settings::AppSettings::load(app.handle());
            app.manage(AppSettingsState(std::sync::Mutex::new(settings)));

            let library = library::Library::new(app.handle())?;
            app.manage(LibraryState(std::sync::Mutex::new(library)));

            let app_handle = app.handle().clone();
            app.manage(MediaBridgeState(media_controls::MediaBridgeState::new(
                app_handle.clone(),
            )));

            // SMTC requires a valid HWND — initialize on the UI thread once the window exists.
            if let Some(window) = app.get_webview_window("main") {
                let init_handle = app_handle.clone();
                window.on_window_event(move |event| {
                    if matches!(
                        event,
                        WindowEvent::Focused(true)
                            | WindowEvent::Resized(_)
                            | WindowEvent::ScaleFactorChanged { .. }
                    ) {
                        if let Some(state) = init_handle.try_state::<MediaBridgeState>() {
                            state.0.ensure_initialized_main();
                        }
                    }
                });

                // Schedule init without blocking setup (blocking here deadlocks the UI thread).
                let init_handle = app_handle.clone();
                let _ = app.run_on_main_thread(move || {
                    if let Some(state) = init_handle.try_state::<MediaBridgeState>() {
                        state.0.ensure_initialized_main();
                    }
                });
            } else {
                tracing::warn!("Main window not found — OS media controls will init on first use");
            }

            if let Err(e) = gui_tray::setup(app) {
                tracing::warn!("System tray unavailable: {e}");
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                let action = app
                    .try_state::<AppSettingsState>()
                    .and_then(|state| state.0.lock().ok().map(|s| s.close_action))
                    .unwrap_or(CloseAction::Quit);

                if action == CloseAction::HideWindow {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
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
            commands::clear_media_session,
            commands::create_playlist,
            commands::delete_playlist,
            commands::rename_playlist,
            commands::get_playlist_tracks_by_id,
            commands::add_track_to_playlist_by_id,
            commands::remove_track_from_playlist_by_id,
            commands::clear_playlist_by_id,
            commands::fetch_lyrics_for_track,
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
            commands::move_queue_track,
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
            commands::export_eq_settings,
            commands::import_eq_settings,
            commands::get_close_action,
            commands::set_close_action,
            commands::toggle_close_action,
            commands::host_os,
            commands::import_audio_sources,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
