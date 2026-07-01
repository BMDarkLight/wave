use crate::audio::eq::EqBand;
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

/// Summary of a distinct album in the library, used for browse/grid views.
///
/// Albums are grouped by `(album, album_artist)` — falling back to the track
/// `artist` when `album_artist` is missing — so that unrelated albums which
/// happen to share a name (e.g. several "Greatest Hits") are kept separate.
#[derive(Debug, Clone, Serialize)]
pub struct AlbumSummaryDto {
    pub name: String,
    /// Resolved album artist: the tag `album_artist` when present, else `artist`.
    pub album_artist: Option<String>,
    /// A representative track artist for the album (first found).
    pub artist: String,
    pub track_count: i64,
    pub year: Option<i32>,
    /// Representative cover art for the album (may be a `data:` URL or HTTPS URL).
    pub cover_art_data_url: Option<String>,
    pub cover_art_mime: Option<String>,
}

/// Summary of a distinct artist in the library, used for browse/discography views.
#[derive(Debug, Clone, Serialize)]
pub struct ArtistSummaryDto {
    pub name: String,
    pub track_count: i64,
    pub album_count: i64,
}

// ── EQ ─────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct EqBandDto {
    pub frequency: f64,
    pub gain_db: f32,
    pub active: bool,
}

impl From<&EqBand> for EqBandDto {
    fn from(band: &EqBand) -> Self {
        Self {
            frequency: band.frequency,
            gain_db: band.gain_db,
            active: band.active,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct EqStateDto {
    pub bands: Vec<EqBandDto>,
    pub enabled: bool,
}

// ── Audio file info ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct AudioFileInfoDto {
    pub path: String,
    pub sample_rate: Option<i32>,
    pub channels: Option<i32>,
    pub bit_depth: Option<i32>,
    /// Nominal bitrate in bits per second (may be 0 for lossless or unknown).
    pub bitrate_bps: Option<u32>,
    pub format: String,
}
