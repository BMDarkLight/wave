use std::path::{Component, Path, PathBuf};

use crate::metadata::is_supported_audio_file;

/// Maximum bytes hashed when fingerprinting a track during library indexing.
pub const MAX_FILE_HASH_BYTES: u64 = 500 * 1024 * 1024;

/// Maximum playlist JSON import size (10 MiB).
pub const MAX_PLAYLIST_IMPORT_BYTES: u64 = 10 * 1024 * 1024;

/// Validate that `path` points to a readable supported audio file.
pub fn validate_audio_path(path: &str) -> Result<PathBuf, String> {
    let path_buf = PathBuf::from(path);
    if !path_buf.is_file() {
        return Err(format!("Audio file does not exist: {path}"));
    }
    if !is_supported_audio_file(&path_buf) {
        return Err(format!("Unsupported audio file extension: {path}"));
    }
    Ok(path_buf)
}

/// Reject paths that traverse outside the filesystem root or contain parent refs.
pub fn validate_safe_output_path(path: &str, expected_extension: &str) -> Result<PathBuf, String> {
    let path_buf = PathBuf::from(path);
    if path.is_empty() {
        return Err("Output path cannot be empty".to_string());
    }
    for component in path_buf.components() {
        if matches!(component, Component::ParentDir) {
            return Err("Output path cannot contain '..'".to_string());
        }
    }
    let ext = path_buf
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();
    if ext != expected_extension {
        return Err(format!(
            "Output path must have .{expected_extension} extension, got .{ext}"
        ));
    }
    if let Some(parent) = path_buf.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            return Err(format!(
                "Parent directory does not exist: {}",
                parent.display()
            ));
        }
    }
    Ok(path_buf)
}

/// Validate playlist import file before reading into memory.
pub fn validate_playlist_import_path(path: &str) -> Result<(PathBuf, u64), String> {
    let path_buf = Path::new(path);
    if !path_buf.is_file() {
        return Err(format!("Playlist file not found: {path}"));
    }
    let size = std::fs::metadata(path_buf)
        .map_err(|e| format!("Failed to read playlist file metadata: {e}"))?
        .len();
    if size > MAX_PLAYLIST_IMPORT_BYTES {
        return Err(format!(
            "Playlist file too large ({size} bytes, max {MAX_PLAYLIST_IMPORT_BYTES})"
        ));
    }
    Ok((path_buf.to_path_buf(), size))
}
