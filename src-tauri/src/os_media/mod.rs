//! OS-level media integration (SMTC flyout, media keys, taskbar controls).
//!
//! Platform code lives in one place per OS:
//! - Windows → `os_media/windows.rs` (everything: AppUserModelID, SMTC, taskbar)
//! - Linux/macOS → souvlaki via `integrations/media_controls.rs`

#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "windows")]
pub use windows::WindowsMedia;

/// Set the Windows AppUserModelID so the shell shows the correct app name.
pub fn set_app_user_model_id(app_id: &str) {
    #[cfg(target_os = "windows")]
    windows::set_app_user_model_id(app_id);
    #[cfg(not(target_os = "windows"))]
    let _ = app_id;
}
