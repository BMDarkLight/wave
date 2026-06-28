use crate::audio::player::RepeatMode;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct PlaybackStateDto {
    pub is_playing: bool,
    pub is_paused: bool,
    pub current_path: Option<String>,
    pub position_seconds: f64,
    pub duration_seconds: Option<f64>,
    pub volume: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct QueueStateDto {
    pub tracks: Vec<String>,
    pub current_index: Option<usize>,
    pub is_shuffled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaybackModeDto {
    pub repeat: RepeatMode,
    pub shuffle: bool,
}
