use serde::{Deserialize, Serialize};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::{MetadataOptions, StandardTagKey, Tag};
use symphonia::core::probe::Hint;
use uuid::Uuid;

const SUPPORTED_EXTENSIONS: &[&str] = &[
    "aac", "aiff", "alac", "caf", "flac", "m4a", "m4b", "m4p", "mka", "mkv", "mp1", "mp2",
    "mp3", "mp4", "oga", "ogg", "opus", "wav", "wave", "weba",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: String,
    pub path: String,
    pub name: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: Option<String>,
    pub genre: Option<String>,
    pub year: Option<i32>,
    pub track_number: Option<i32>,
    pub disc_number: Option<i32>,
    pub format: String,
    pub duration_seconds: Option<f64>,
    pub sample_rate: Option<i32>,
    pub channels: Option<i32>,
    pub bit_depth: Option<i32>,
    pub file_size: i64,
    pub modified_at: i64,
    pub indexed_at: i64,
}

#[derive(Default)]
struct Tags {
    title: Option<String>,
    artist: Option<String>,
    album: Option<String>,
    album_artist: Option<String>,
    genre: Option<String>,
    year: Option<i32>,
    track_number: Option<i32>,
    disc_number: Option<i32>,
}

pub fn supported_audio_extensions() -> Vec<String> {
    SUPPORTED_EXTENSIONS.iter().map(|value| value.to_string()).collect()
}

pub fn is_supported_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            SUPPORTED_EXTENSIONS
                .iter()
                .any(|supported| supported.eq_ignore_ascii_case(extension))
        })
        .unwrap_or(false)
}

pub fn extract_track(path: &str) -> Result<Track, String> {
    let path_buf = PathBuf::from(path);
    if !path_buf.is_file() {
        return Err("Audio file does not exist".to_string());
    }
    if !is_supported_audio_file(&path_buf) {
        return Err("Unsupported audio file extension".to_string());
    }

    let metadata = std::fs::metadata(&path_buf)
        .map_err(|error| format!("Failed to read file metadata: {error}"))?;
    let file_size = metadata.len() as i64;
    let modified_at = timestamp(metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH));
    let indexed_at = timestamp(SystemTime::now());

    let file = File::open(&path_buf).map_err(|error| format!("Failed to open audio file: {error}"))?;
    let source = MediaSourceStream::new(Box::new(file), MediaSourceStreamOptions::default());
    let mut hint = Hint::new();
    if let Some(extension) = path_buf.extension().and_then(|extension| extension.to_str()) {
        hint.with_extension(extension);
    }

    let mut probed = symphonia::default::get_probe()
        .format(
            &hint,
            source,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|error| format!("Failed to inspect audio file: {error}"))?;

    let mut format = probed.format;
    let codec_params = format
        .tracks()
        .iter()
        .find(|track| track.codec_params.codec != symphonia::core::codecs::CODEC_TYPE_NULL)
        .map(|track| &track.codec_params);

    let duration_seconds = codec_params.and_then(|params| {
        params
            .time_base
            .zip(params.n_frames)
            .map(|(time_base, frames)| {
                let time = time_base.calc_time(frames);
                time.seconds as f64 + time.frac
            })
    });
    let sample_rate = codec_params.and_then(|params| params.sample_rate).map(|value| value as i32);
    let channels = codec_params
        .and_then(|params| params.channels)
        .map(|channels| channels.count() as i32);
    let bit_depth = codec_params
        .and_then(|params| params.bits_per_sample.or(params.bits_per_coded_sample))
        .map(|value| value as i32);

    let mut tags = Tags::default();
    if let Some(metadata) = probed.metadata.get() {
        if let Some(revision) = metadata.current() {
            merge_tags(&mut tags, revision.tags());
        }
    }
    if let Some(revision) = format.metadata().current() {
        merge_tags(&mut tags, revision.tags());
    }

    let fallback = fallback_fields(&path_buf);
    let name = path_buf
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Unknown")
        .to_string();
    let extension = path_buf
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("Audio")
        .to_uppercase();

    Ok(Track {
        id: Uuid::new_v4().to_string(),
        path: path.to_string(),
        name,
        title: tags.title.unwrap_or(fallback.title),
        artist: tags.artist.unwrap_or(fallback.artist),
        album: tags.album.unwrap_or(fallback.album),
        album_artist: tags.album_artist,
        genre: tags.genre,
        year: tags.year,
        track_number: tags.track_number,
        disc_number: tags.disc_number,
        format: extension,
        duration_seconds,
        sample_rate,
        channels,
        bit_depth,
        file_size,
        modified_at,
        indexed_at,
    })
}

fn merge_tags(target: &mut Tags, tags: &[Tag]) {
    for tag in tags {
        let value = tag.value.to_string();
        if value.trim().is_empty() {
            continue;
        }

        match tag.std_key {
            Some(StandardTagKey::TrackTitle) => set_once(&mut target.title, value),
            Some(StandardTagKey::Artist) => set_once(&mut target.artist, value),
            Some(StandardTagKey::Album) => set_once(&mut target.album, value),
            Some(StandardTagKey::AlbumArtist) => set_once(&mut target.album_artist, value),
            Some(StandardTagKey::Genre) => set_once(&mut target.genre, value),
            Some(StandardTagKey::Date) => target.year = target.year.or_else(|| parse_year(&value)),
            Some(StandardTagKey::TrackNumber) => {
                target.track_number = target.track_number.or_else(|| parse_number(&value))
            }
            Some(StandardTagKey::DiscNumber) => {
                target.disc_number = target.disc_number.or_else(|| parse_number(&value))
            }
            _ => {}
        }
    }
}

fn set_once(target: &mut Option<String>, value: String) {
    if target.is_none() {
        *target = Some(value);
    }
}

struct FallbackFields {
    title: String,
    artist: String,
    album: String,
}

fn fallback_fields(path: &Path) -> FallbackFields {
    let file_stem = path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("Unknown");
    let album = path
        .parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        .unwrap_or("Local Files")
        .to_string();
    let (artist, title) = file_stem
        .split_once(" - ")
        .map(|(artist, title)| (artist.to_string(), title.to_string()))
        .unwrap_or_else(|| ("Unknown Artist".to_string(), file_stem.to_string()));

    FallbackFields {
        title,
        artist,
        album,
    }
}

fn parse_year(value: &str) -> Option<i32> {
    value
        .split(|character: char| !character.is_ascii_digit())
        .find(|part| part.len() == 4)
        .and_then(|part| part.parse().ok())
}

fn parse_number(value: &str) -> Option<i32> {
    value
        .split('/')
        .next()
        .and_then(|part| part.trim().parse().ok())
}

fn timestamp(system_time: SystemTime) -> i64 {
    system_time
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}
