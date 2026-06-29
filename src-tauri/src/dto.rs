use crate::audio::player::RepeatMode;
use crate::metadata::Track;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct PlaybackStateDto {
    pub is_playing: bool,
    pub is_paused: bool,
    pub current_path: Option<String>,
    pub position_seconds: f64,
    pub duration_seconds: Option<f64>,
    pub volume: f32,
    pub output_device_name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct QueueStateDto {
    pub tracks: Vec<String>,
    pub current_index: Option<usize>,
    pub is_shuffled: bool,
}

/// Queue with full track metadata (for UI display).
#[derive(Debug, Clone, Serialize)]
pub struct QueueDto {
    pub tracks: Vec<Track>,
    pub current_index: Option<usize>,
    pub is_shuffled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaybackModeDto {
    pub repeat: RepeatMode,
    pub shuffle: bool,
}

/// Result of importing a playlist — the new playlist id and its tracks.
#[derive(Debug, Clone, Serialize)]
pub struct ImportResultDto {
    pub playlist_id: String,
    pub playlist_name: String,
    pub track_count: usize,
}
