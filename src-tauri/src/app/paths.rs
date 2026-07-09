use std::path::PathBuf;

/// Application data directory (`app.bmdarklight.wave` under the platform data root).
pub fn data_dir() -> PathBuf {
    if let Some(base) = data_root() {
        base.join("app.bmdarklight.wave")
    } else {
        PathBuf::from(".")
    }
}

/// Default SQLite library database path.
pub fn library_db_path() -> PathBuf {
    if let Ok(path) = std::env::var("WAVE_DB_PATH") {
        return PathBuf::from(path);
    }
    data_dir().join("wave-library.sqlite")
}

/// Path to the playback daemon state file (pid + port).
pub fn daemon_state_path() -> PathBuf {
    data_dir().join("playback-daemon.json")
}

/// Path to the primary-instance lock file.
pub fn instance_lock_path() -> PathBuf {
    data_dir().join("wave-instance.lock")
}

#[cfg(target_os = "macos")]
fn data_root() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(|h| PathBuf::from(h).join("Library/Application Support"))
}

#[cfg(target_os = "linux")]
fn data_root() -> Option<PathBuf> {
    std::env::var("XDG_DATA_HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| PathBuf::from(h).join(".local/share"))
        })
}

#[cfg(target_os = "windows")]
fn data_root() -> Option<PathBuf> {
    std::env::var("APPDATA").ok().map(PathBuf::from)
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn data_root() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(|h| PathBuf::from(h).join(".local/share"))
}
