//! System tray for the Tauri GUI (taskbar on Windows/Linux, menu bar on macOS).

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};

use crate::commands::{LibraryState, PlayerState};

pub fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let playlists_sub = build_playlists_submenu(app)?;
    let play_pause = MenuItem::with_id(app, "tray_play_pause", "Play / Pause", true, None::<&str>)?;
    let prev = MenuItem::with_id(app, "tray_prev", "Previous", true, None::<&str>)?;
    let next = MenuItem::with_id(app, "tray_next", "Next", true, None::<&str>)?;
    let stop = MenuItem::with_id(app, "tray_stop", "Stop", true, None::<&str>)?;
    let show = MenuItem::with_id(app, "tray_show", "Show Window", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "tray_quit", "Quit", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &playlists_sub,
            &PredefinedMenuItem::separator(app)?,
            &play_pause,
            &prev,
            &next,
            &stop,
            &PredefinedMenuItem::separator(app)?,
            &show,
            &quit,
        ],
    )?;

    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or("No default window icon")?;

    let tray = TrayIconBuilder::with_id("wave-tray")
        .icon(icon)
        .menu(&menu)
        .tooltip("Wave")
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| handle_menu_event(app, event.id.as_ref()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                toggle_play_pause(app);
            }
        })
        .build(app)?;

    let _ = tray.set_show_menu_on_left_click(false);

    Ok(())
}

fn build_playlists_submenu(app: &mut tauri::App) -> Result<Submenu<tauri::Wry>, tauri::Error> {
    let sub = Submenu::with_id(app, "tray_playlists", "Playlists", true)?;
    if let Some(library_state) = app.try_state::<LibraryState>() {
        if let Ok(lib) = library_state.0.lock() {
            if let Ok(playlists) = lib.list_playlists(None) {
                for pl in playlists.iter().take(30) {
                    let item = MenuItem::with_id(
                        app,
                        format!("tray_playlist:{}", pl.id),
                        &pl.name,
                        true,
                        None::<&str>,
                    )?;
                    sub.append(&item)?;
                }
            }
        }
    }
    Ok(sub)
}

fn handle_menu_event(app: &AppHandle, id: &str) {
    if id == "tray_play_pause" {
        toggle_play_pause(app);
        return;
    }
    if id == "tray_prev" {
        let _ = app.state::<PlayerState>().0.lock().map(|mut p| p.play_previous());
        return;
    }
    if id == "tray_next" {
        let _ = app.state::<PlayerState>().0.lock().map(|mut p| p.play_next());
        return;
    }
    if id == "tray_stop" {
        let _ = app.state::<PlayerState>().0.lock().map(|mut p| p.stop());
        return;
    }
    if id == "tray_show" {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.show();
            let _ = window.set_focus();
        }
        return;
    }
    if id == "tray_quit" {
        app.exit(0);
        return;
    }
    if let Some(playlist_id) = id.strip_prefix("tray_playlist:") {
        play_playlist(app, playlist_id);
    }
}

fn toggle_play_pause(app: &AppHandle) {
    let player_state = app.state::<PlayerState>();
    let mut player = match player_state.0.lock() {
        Ok(p) => p,
        Err(_) => return,
    };

    if player.is_playing() {
        let _ = player.pause();
        return;
    }
    if player.is_paused() {
        let _ = player.resume();
        return;
    }

    // Stopped — resume the last loaded track if we still have one.
    if let Some(path) = player
        .get_current_path()
        .and_then(|p| p.to_str().map(str::to_string))
    {
        let _ = player.play(&path);
        return;
    }

    // Otherwise start from the in-memory queue.
    if !player.queue.tracks().is_empty() {
        let index = player.queue.current_index().unwrap_or(0);
        if let Some(path) = player.queue.jump(index) {
            let path = path.to_string();
            let _ = player.play(&path);
            return;
        }
    }

    drop(player);
    try_play_default_playlist(app);
}

fn try_play_default_playlist(app: &AppHandle) {
    let tracks = {
        let library_state = app.state::<LibraryState>();
        let lib = match library_state.0.lock() {
            Ok(l) => l,
            Err(_) => return,
        };
        match lib.get_default_playlist_tracks() {
            Ok(t) if !t.is_empty() => t,
            _ => return,
        }
    };

    let paths: Vec<String> = tracks.iter().map(|t| t.path.clone()).collect();
    let first_path = paths[0].clone();

    let player_state = app.state::<PlayerState>();
    let mut player = match player_state.0.lock() {
        Ok(p) => p,
        Err(_) => return,
    };
    player.queue.set_tracks(paths);
    player.queue.jump(0);
    let _ = player.play(&first_path);
}

fn play_playlist(app: &AppHandle, playlist_id: &str) {
    let tracks = {
        let library_state = app.state::<LibraryState>();
        let lib = match library_state.0.lock() {
            Ok(l) => l,
            Err(_) => return,
        };
        match lib.get_playlist_tracks(playlist_id) {
            Ok(t) if !t.is_empty() => t,
            _ => return,
        }
    };

    let first_path = tracks[0].path.clone();
    let paths: Vec<String> = tracks.iter().map(|t| t.path.clone()).collect();

    let player_state = app.state::<PlayerState>();
    let mut player = match player_state.0.lock() {
        Ok(p) => p,
        Err(_) => return,
    };
    player.queue.set_tracks(paths);
    player.queue.jump(0);
    let _ = player.play(&first_path);
}
