//! Cover art handling — resize large embeds and store a compact data URL.
//!
//! Also writes a disk cache under app data so Android ExoPlayer / MediaSession
//! can load artwork from a real file path when needed.

use std::fs;
use std::path::PathBuf;

use base64::{engine::general_purpose, Engine as _};
use tauri::{AppHandle, Manager};

const COVER_ART_DIR: &str = "cover_art";
const MAX_COVER_ART_SIZE: usize = 200 * 1024; // 200 KiB after resize

/// Cover art extracted from an audio file or downloaded from the network.
pub struct ExtractedCoverArt {
    pub data: Vec<u8>,
    pub mime: String,
}

/// Resize (if needed), cache to disk, and return a `data:` URL for the DB / UI.
pub fn save_cover_art(
    app: &AppHandle,
    track_id: &str,
    cover_art: ExtractedCoverArt,
) -> Result<String, String> {
    let mime = normalize_mime(&cover_art.mime);
    let data = resize_if_needed(cover_art.data, &mime)?;

    if let Err(e) = write_disk_cache(app, track_id, &mime, &data) {
        tracing::debug!("Cover art disk cache skipped: {e}");
    }

    Ok(to_data_url(&mime, &data))
}

/// Resize + encode a cover as a `data:` URL without touching disk (CLI / no app).
pub fn encode_cover_data_url(data: Vec<u8>, mime: &str) -> Result<String, String> {
    let mime = normalize_mime(mime);
    let data = resize_if_needed(data, &mime)?;
    Ok(to_data_url(&mime, &data))
}

fn to_data_url(mime: &str, data: &[u8]) -> String {
    let encoded = general_purpose::STANDARD.encode(data);
    format!("data:{mime};base64,{encoded}")
}

/// Absolute path to a previously cached cover file, if it exists.
pub fn cached_cover_path(app: &AppHandle, track_id: &str) -> Option<PathBuf> {
    let app_dir = app.path().app_data_dir().ok()?;
    let cover_dir = app_dir.join(COVER_ART_DIR);
    for ext in ["jpg", "png", "webp"] {
        let path = cover_dir.join(format!("{track_id}.{ext}"));
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

fn write_disk_cache(
    app: &AppHandle,
    track_id: &str,
    mime: &str,
    data: &[u8],
) -> Result<PathBuf, String> {
    let app_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to resolve app data dir: {e}"))?;
    let cover_dir = app_dir.join(COVER_ART_DIR);
    fs::create_dir_all(&cover_dir).map_err(|e| format!("Failed to create cover art dir: {e}"))?;

    let ext = match mime {
        "image/png" => "png",
        "image/webp" => "webp",
        _ => "jpg",
    };
    let filepath = cover_dir.join(format!("{track_id}.{ext}"));
    fs::write(&filepath, data).map_err(|e| format!("Failed to write cover art: {e}"))?;
    Ok(filepath)
}

fn normalize_mime(mime: &str) -> String {
    match mime.to_ascii_lowercase().as_str() {
        "image/jpg" | "image/jpeg" => "image/jpeg".into(),
        "image/png" => "image/png".into(),
        "image/webp" => "image/webp".into(),
        other if other.starts_with("image/") => other.to_string(),
        _ => "image/jpeg".into(),
    }
}

fn resize_if_needed(data: Vec<u8>, mime: &str) -> Result<Vec<u8>, String> {
    if data.len() <= MAX_COVER_ART_SIZE {
        return Ok(data);
    }

    let img = image::load_from_memory(&data).map_err(|e| format!("Failed to load image: {e}"))?;

    // Approximate scale from byte ratio; clamp so we always shrink oversized art.
    let scale = (MAX_COVER_ART_SIZE as f64 / data.len() as f64).sqrt().clamp(0.15, 0.95);
    let new_width = (img.width() as f64 * scale).round().max(64.0) as u32;
    let new_height = (img.height() as f64 * scale).round().max(64.0) as u32;

    let resized = img.resize(new_width, new_height, image::imageops::FilterType::Lanczos3);

    let mut buf = Vec::new();
    let format = if mime == "image/png" {
        image::ImageFormat::Png
    } else {
        image::ImageFormat::Jpeg
    };
    resized
        .write_to(&mut std::io::Cursor::new(&mut buf), format)
        .map_err(|e| format!("Failed to encode cover art: {e}"))?;

    // If still too large (rare for JPEG), force a smaller JPEG.
    if buf.len() > MAX_COVER_ART_SIZE && format != image::ImageFormat::Jpeg {
        buf.clear();
        resized
            .write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Jpeg)
            .map_err(|e| format!("Failed to encode cover art JPEG: {e}"))?;
    }

    Ok(buf)
}
