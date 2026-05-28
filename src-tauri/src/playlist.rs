use rodio::{Decoder, Source};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub path: String,
    pub name: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub format: String,
    pub duration_seconds: Option<f64>,
}

impl Track {
    pub fn from_path(path: String) -> Self {
        let path_buf = PathBuf::from(&path);
        let file_stem = path_buf
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("Unknown");
        let name = path_buf
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Unknown")
            .to_string();
        let format = path_buf
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("Audio")
            .to_uppercase();
        let album = path_buf
            .parent()
            .and_then(|parent| parent.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("Local Files")
            .to_string();
        let (artist, title) = file_stem
            .split_once(" - ")
            .map(|(artist, title)| (artist.to_string(), title.to_string()))
            .unwrap_or_else(|| ("Unknown Artist".to_string(), file_stem.to_string()));

        Self {
            path: path.clone(),
            name,
            title,
            artist,
            album,
            format,
            duration_seconds: read_duration_seconds(&path),
        }
    }
}

fn read_duration_seconds(path: &str) -> Option<f64> {
    let file = File::open(path).ok()?;
    let source = Decoder::new(BufReader::new(file)).ok()?;
    source.total_duration().map(|duration| duration.as_secs_f64())
}

pub struct Playlist {
    tracks: Mutex<Vec<Track>>,
}

impl Playlist {
    pub fn new() -> Self {
        Self {
            tracks: Mutex::new(Vec::new()),
        }
    }

    pub fn add_track(&self, path: String) -> Track {
        let track = Track::from_path(path);
        self.tracks.lock().unwrap().push(track.clone());
        track
    }

    pub fn remove_track(&self, index: usize) -> Result<(), String> {
        let mut tracks = self.tracks.lock().unwrap();
        if index < tracks.len() {
            tracks.remove(index);
            Ok(())
        } else {
            Err("Index out of bounds".to_string())
        }
    }

    pub fn get_tracks(&self) -> Vec<Track> {
        self.tracks.lock().unwrap().clone()
    }

    pub fn clear(&self) {
        self.tracks.lock().unwrap().clear();
    }

    pub fn get_track(&self, index: usize) -> Option<Track> {
        self.tracks.lock().unwrap().get(index).cloned()
    }
}
