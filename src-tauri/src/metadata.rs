use base64::{engine::general_purpose, Engine as _};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::{MetadataOptions, StandardTagKey, StandardVisualKey, Tag, Visual};
use symphonia::core::probe::Hint;
use uuid::Uuid;

const MAX_EMBEDDED_ART_BYTES: usize = 8 * 1024 * 1024;
const ONLINE_LOOKUP_TIMEOUT_SECONDS: u64 = 5;
const USER_AGENT: &str = "Wave/0.1.0 (local metadata enrichment)";

fn metadata_client() -> &'static Client {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        Client::builder()
            .timeout(Duration::from_secs(ONLINE_LOOKUP_TIMEOUT_SECONDS))
            .user_agent(USER_AGENT)
            .build()
            .expect("failed to build metadata HTTP client")
    })
}

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
    pub lyrics: Option<String>,
    pub lyrics_source: Option<String>,
    pub cover_art_data_url: Option<String>,
    pub cover_art_mime: Option<String>,
    pub cover_art_source: Option<String>,
    pub fingerprint_sha256: Option<String>,
    pub acoustid_fingerprint: Option<String>,
    pub musicbrainz_recording_id: Option<String>,
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
    lyrics: Option<String>,
    acoustid_fingerprint: Option<String>,
    musicbrainz_recording_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MusicBrainzSearch {
    recordings: Option<Vec<MusicBrainzRecording>>,
}

#[derive(Debug, Deserialize)]
struct MusicBrainzRecording {
    releases: Option<Vec<MusicBrainzRelease>>,
}

#[derive(Debug, Deserialize)]
struct MusicBrainzRelease {
    id: String,
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

    let fingerprint_sha256 = hash_file_sha256(&path_buf).ok();

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
    let mut cover_art = None;
    if let Some(metadata) = probed.metadata.get() {
        if let Some(revision) = metadata.current() {
            merge_tags(&mut tags, revision.tags());
            cover_art = cover_art.or_else(|| extract_cover_art(revision.visuals()));
        }
    }
    if let Some(revision) = format.metadata().current() {
        merge_tags(&mut tags, revision.tags());
        cover_art = cover_art.or_else(|| extract_cover_art(revision.visuals()));
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

    let lyrics = tags.lyrics.or_else(|| read_sidecar_lyrics(&path_buf));
    let lyrics_source = lyrics.as_ref().map(|_| "embedded-or-sidecar".to_string());

    let mut track = Track {
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
        lyrics,
        lyrics_source,
        cover_art_data_url: cover_art.as_ref().map(|cover| cover.data_url.clone()),
        cover_art_mime: cover_art.as_ref().map(|cover| cover.mime.clone()),
        cover_art_source: cover_art.as_ref().map(|cover| cover.source.clone()),
        fingerprint_sha256,
        acoustid_fingerprint: tags.acoustid_fingerprint,
        musicbrainz_recording_id: tags.musicbrainz_recording_id,
        file_size,
        modified_at,
        indexed_at,
    };

    if track.cover_art_data_url.is_none() {
        enrich_cover_art_online(&mut track);
    }
    Ok(track)
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
            Some(StandardTagKey::Lyrics) => set_once(&mut target.lyrics, value),
            Some(StandardTagKey::AcoustidFingerprint) => {
                set_once(&mut target.acoustid_fingerprint, value)
            }
            Some(StandardTagKey::MusicBrainzRecordingId)
            | Some(StandardTagKey::MusicBrainzTrackId) => {
                set_once(&mut target.musicbrainz_recording_id, value)
            }
            _ => {}
        }
    }
}

struct CoverArt {
    data_url: String,
    mime: String,
    source: String,
}

fn extract_cover_art(visuals: &[Visual]) -> Option<CoverArt> {
    let visual = visuals
        .iter()
        .find(|visual| matches!(visual.usage, Some(StandardVisualKey::FrontCover)))
        .or_else(|| visuals.first())?;

    if visual.data.is_empty() || visual.data.len() > MAX_EMBEDDED_ART_BYTES {
        return None;
    }

    let mime = if visual.media_type.trim().is_empty() {
        "image/jpeg".to_string()
    } else {
        visual.media_type.clone()
    };
    let encoded = general_purpose::STANDARD.encode(&visual.data);
    Some(CoverArt {
        data_url: format!("data:{mime};base64,{encoded}"),
        mime,
        source: "embedded".to_string(),
    })
}

fn read_sidecar_lyrics(path: &Path) -> Option<String> {
    ["lrc", "txt"]
        .iter()
        .map(|extension| path.with_extension(extension))
        .find(|candidate| candidate.is_file())
        .and_then(|candidate| std::fs::read_to_string(candidate).ok())
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

fn hash_file_sha256(path: &Path) -> Result<String, String> {
    let metadata = std::fs::metadata(path)
        .map_err(|error| format!("Failed to read file metadata: {error}"))?;
    if metadata.len() > crate::path_validation::MAX_FILE_HASH_BYTES {
        return Err(format!(
            "File too large to fingerprint ({} bytes, max {})",
            metadata.len(),
            crate::path_validation::MAX_FILE_HASH_BYTES
        ));
    }

    let mut file = File::open(path).map_err(|error| format!("Failed to open file for hashing: {error}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];

    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("Failed to read file for hashing: {error}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

fn enrich_cover_art_online(track: &mut Track) {
    let Some(release_id) = find_musicbrainz_release_id(track) else {
        return;
    };

    track.cover_art_data_url = Some(format!(
        "https://coverartarchive.org/release/{release_id}/front-500"
    ));
    track.cover_art_mime = None;
    track.cover_art_source = Some("cover-art-archive".to_string());
}

pub fn enrich_lyrics_online(track: &mut Track) {
    let client = metadata_client();

    let mut request = client
        .get("https://lrclib.net/api/get")
        .query(&[
            ("artist_name", track.artist.as_str()),
            ("track_name", track.title.as_str()),
        ]);

    if track.album != "Local Files" {
        request = request.query(&[("album_name", track.album.as_str())]);
    }

    let duration_string;
    if let Some(duration) = track.duration_seconds {
        duration_string = duration.round().to_string();
        request = request.query(&[("duration", duration_string.as_str())]);
    }

    let response = match request.send().and_then(|response| response.error_for_status()) {
        Ok(response) => response,
        Err(_) => return,
    };

    let value = match response.json::<serde_json::Value>() {
        Ok(value) => value,
        Err(_) => return,
    };

    let lyrics = value
        .get("syncedLyrics")
        .and_then(|value| value.as_str())
        .or_else(|| value.get("plainLyrics").and_then(|value| value.as_str()))
        .map(str::trim)
        .filter(|value| !value.is_empty());

    if let Some(lyrics) = lyrics {
        track.lyrics = Some(lyrics.to_string());
        track.lyrics_source = Some("lrclib".to_string());
    }
}

fn find_musicbrainz_release_id(track: &Track) -> Option<String> {
    let client = metadata_client();

    let mut query_parts = Vec::new();
    if !track.title.trim().is_empty() {
        query_parts.push(format!("recording:\"{}\"", escape_musicbrainz_query(&track.title)));
    }
    if !track.artist.trim().is_empty() && track.artist != "Unknown Artist" {
        query_parts.push(format!("artist:\"{}\"", escape_musicbrainz_query(&track.artist)));
    }
    if !track.album.trim().is_empty() && track.album != "Local Files" {
        query_parts.push(format!("release:\"{}\"", escape_musicbrainz_query(&track.album)));
    }

    if query_parts.is_empty() {
        return None;
    }

    let response = client
        .get("https://musicbrainz.org/ws/2/recording")
        .query(&[
            ("query", query_parts.join(" AND ")),
            ("fmt", "json".to_string()),
            ("limit", "1".to_string()),
        ])
        .send()
        .ok()?
        .error_for_status()
        .ok()?
        .json::<MusicBrainzSearch>()
        .ok()?;

    response
        .recordings?
        .into_iter()
        .flat_map(|recording| recording.releases.unwrap_or_default())
        .next()
        .map(|release| release.id)
}

fn escape_musicbrainz_query(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
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
