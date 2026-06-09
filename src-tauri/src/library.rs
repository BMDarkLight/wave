use crate::metadata::{extract_track, is_supported_audio_file, Track};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::Manager;
use uuid::Uuid;
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistInfo {
    pub id: String,
    pub profile_id: String,
    pub name: String,
    pub track_count: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

pub struct Library {
    db_path: PathBuf,
    connection: Mutex<Connection>,
}

impl Library {
    pub fn new(app_handle: &tauri::AppHandle) -> Result<Self, String> {
        let app_dir = app_handle
            .path()
            .app_data_dir()
            .map_err(|error| format!("Failed to resolve application data directory: {error}"))?;
        std::fs::create_dir_all(&app_dir)
            .map_err(|error| format!("Failed to create application data directory: {error}"))?;

        let db_path = app_dir.join("wave-library.sqlite");
        let connection = Connection::open(&db_path)
            .map_err(|error| format!("Failed to open library database: {error}"))?;
        let library = Self {
            db_path,
            connection: Mutex::new(connection),
        };
        library.initialize()?;
        Ok(library)
    }

    pub fn db_path(&self) -> String {
        self.db_path.to_string_lossy().to_string()
    }

    fn initialize(&self) -> Result<(), String> {
        let connection = self.connection.lock().unwrap();
        connection
            .execute_batch(
                "
                PRAGMA journal_mode = WAL;
                PRAGMA foreign_keys = ON;
                PRAGMA synchronous = NORMAL;

                CREATE TABLE IF NOT EXISTS profiles (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL UNIQUE,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL
                );

                CREATE TABLE IF NOT EXISTS tracks (
                    id TEXT PRIMARY KEY,
                    path TEXT NOT NULL UNIQUE,
                    name TEXT NOT NULL,
                    title TEXT NOT NULL,
                    artist TEXT NOT NULL,
                    album TEXT NOT NULL,
                    album_artist TEXT,
                    genre TEXT,
                    year INTEGER,
                    track_number INTEGER,
                    disc_number INTEGER,
                    format TEXT NOT NULL,
                    duration_seconds REAL,
                    sample_rate INTEGER,
                    channels INTEGER,
                    bit_depth INTEGER,
                    file_size INTEGER NOT NULL,
                    modified_at INTEGER NOT NULL,
                    indexed_at INTEGER NOT NULL
                );

                CREATE TABLE IF NOT EXISTS playlists (
                    id TEXT PRIMARY KEY,
                    profile_id TEXT NOT NULL,
                    name TEXT NOT NULL,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    UNIQUE(profile_id, name),
                    FOREIGN KEY(profile_id) REFERENCES profiles(id) ON DELETE CASCADE
                );

                CREATE TABLE IF NOT EXISTS playlist_tracks (
                    playlist_id TEXT NOT NULL,
                    track_id TEXT NOT NULL,
                    position INTEGER NOT NULL,
                    added_at INTEGER NOT NULL,
                    PRIMARY KEY(playlist_id, track_id),
                    UNIQUE(playlist_id, position),
                    FOREIGN KEY(playlist_id) REFERENCES playlists(id) ON DELETE CASCADE,
                    FOREIGN KEY(track_id) REFERENCES tracks(id) ON DELETE CASCADE
                );

                CREATE INDEX IF NOT EXISTS idx_tracks_artist_album ON tracks(artist, album);
                CREATE INDEX IF NOT EXISTS idx_tracks_title ON tracks(title);
                CREATE INDEX IF NOT EXISTS idx_playlist_tracks_position
                    ON playlist_tracks(playlist_id, position);
                ",
            )
            .map_err(|error| format!("Failed to initialize library database: {error}"))?;
        drop(connection);

        let profile_id = self.ensure_profile("default", "Default")?;
        self.ensure_playlist(&profile_id, "Local Sessions")?;
        Ok(())
    }

    pub fn default_playlist_id(&self) -> Result<String, String> {
        let profile_id = self.ensure_profile("default", "Default")?;
        self.ensure_playlist(&profile_id, "Local Sessions")
    }

    pub fn add_track_to_default_playlist(&self, path: String) -> Result<Track, String> {
        let playlist_id = self.default_playlist_id()?;
        self.add_track_to_playlist(&playlist_id, path)
    }

    pub fn add_track_to_playlist(&self, playlist_id: &str, path: String) -> Result<Track, String> {
        let track = extract_track(&path)?;
        let now = now_timestamp();
        let connection = self.connection.lock().unwrap();

        connection
            .execute(
                "
                INSERT INTO tracks (
                    id, path, name, title, artist, album, album_artist, genre, year, track_number,
                    disc_number, format, duration_seconds, sample_rate, channels, bit_depth,
                    file_size, modified_at, indexed_at
                ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                    ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19
                )
                ON CONFLICT(path) DO UPDATE SET
                    name = excluded.name,
                    title = excluded.title,
                    artist = excluded.artist,
                    album = excluded.album,
                    album_artist = excluded.album_artist,
                    genre = excluded.genre,
                    year = excluded.year,
                    track_number = excluded.track_number,
                    disc_number = excluded.disc_number,
                    format = excluded.format,
                    duration_seconds = excluded.duration_seconds,
                    sample_rate = excluded.sample_rate,
                    channels = excluded.channels,
                    bit_depth = excluded.bit_depth,
                    file_size = excluded.file_size,
                    modified_at = excluded.modified_at,
                    indexed_at = excluded.indexed_at
                ",
                params![
                    track.id,
                    track.path,
                    track.name,
                    track.title,
                    track.artist,
                    track.album,
                    track.album_artist,
                    track.genre,
                    track.year,
                    track.track_number,
                    track.disc_number,
                    track.format,
                    track.duration_seconds,
                    track.sample_rate,
                    track.channels,
                    track.bit_depth,
                    track.file_size,
                    track.modified_at,
                    track.indexed_at
                ],
            )
            .map_err(|error| format!("Failed to upsert track: {error}"))?;

        let position = next_playlist_position(&connection, playlist_id)?;
        connection
            .execute(
                "
                INSERT OR IGNORE INTO playlist_tracks (playlist_id, track_id, position, added_at)
                VALUES (?1, ?2, ?3, ?4)
                ",
                params![playlist_id, track.id, position, now],
            )
            .map_err(|error| format!("Failed to add track to playlist: {error}"))?;

        Ok(track)
    }

    pub fn remove_track_from_default_playlist(&self, index: usize) -> Result<(), String> {
        let playlist_id = self.default_playlist_id()?;
        self.remove_track_from_playlist(&playlist_id, index)
    }

    pub fn remove_track_from_playlist(&self, playlist_id: &str, index: usize) -> Result<(), String> {
        let connection = self.connection.lock().unwrap();
        let track_id: String = connection
            .query_row(
                "
                SELECT track_id FROM playlist_tracks
                WHERE playlist_id = ?1
                ORDER BY position
                LIMIT 1 OFFSET ?2
                ",
                params![playlist_id, index as i64],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| format!("Failed to read playlist track: {error}"))?
            .ok_or("Index out of bounds")?;

        connection
            .execute(
                "DELETE FROM playlist_tracks WHERE playlist_id = ?1 AND track_id = ?2",
                params![playlist_id, track_id],
            )
            .map_err(|error| format!("Failed to remove playlist track: {error}"))?;

        compact_playlist_positions(&connection, playlist_id)
    }

    pub fn get_default_playlist_tracks(&self) -> Result<Vec<Track>, String> {
        let playlist_id = self.default_playlist_id()?;
        self.get_playlist_tracks(&playlist_id)
    }

    pub fn get_playlist_tracks(&self, playlist_id: &str) -> Result<Vec<Track>, String> {
        let connection = self.connection.lock().unwrap();
        let mut statement = connection
            .prepare(
                "
                SELECT t.id, t.path, t.name, t.title, t.artist, t.album, t.album_artist, t.genre,
                       t.year, t.track_number, t.disc_number, t.format, t.duration_seconds,
                       t.sample_rate, t.channels, t.bit_depth, t.file_size, t.modified_at,
                       t.indexed_at
                FROM playlist_tracks pt
                JOIN tracks t ON t.id = pt.track_id
                WHERE pt.playlist_id = ?1
                ORDER BY pt.position
                ",
            )
            .map_err(|error| format!("Failed to prepare playlist query: {error}"))?;

        let tracks = statement
            .query_map(params![playlist_id], row_to_track)
            .map_err(|error| format!("Failed to query playlist: {error}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("Failed to read playlist track: {error}"))?;
        Ok(tracks)
    }

    pub fn clear_default_playlist(&self) -> Result<(), String> {
        let playlist_id = self.default_playlist_id()?;
        let connection = self.connection.lock().unwrap();
        connection
            .execute(
                "DELETE FROM playlist_tracks WHERE playlist_id = ?1",
                params![playlist_id],
            )
            .map_err(|error| format!("Failed to clear playlist: {error}"))?;
        Ok(())
    }

    pub fn get_default_playlist_track(&self, index: usize) -> Result<Option<Track>, String> {
        Ok(self.get_default_playlist_tracks()?.get(index).cloned())
    }

    pub fn index_directory(
        &self,
        profile_id: Option<String>,
        playlist_name: Option<String>,
        directory: String,
    ) -> Result<Vec<Track>, String> {
        let profile_id = profile_id.unwrap_or_else(|| "default".to_string());
        self.ensure_profile(&profile_id, &profile_id)?;
        let playlist_id = self.ensure_playlist(
            &profile_id,
            playlist_name.as_deref().unwrap_or("Local Sessions"),
        )?;

        let directory_path = Path::new(&directory);
        if !directory_path.is_dir() {
            return Err("Library path is not a directory".to_string());
        }

        let mut tracks = Vec::new();
        for entry in WalkDir::new(directory_path)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
        {
            let path = entry.path();
            if !is_supported_audio_file(path) {
                continue;
            }

            if let Some(path) = path.to_str() {
                if let Ok(track) = self.add_track_to_playlist(&playlist_id, path.to_string()) {
                    tracks.push(track);
                }
            }
        }

        Ok(tracks)
    }

    pub fn list_playlists(&self, profile_id: Option<String>) -> Result<Vec<PlaylistInfo>, String> {
        let connection = self.connection.lock().unwrap();
        let mut sql = "
            SELECT p.id, p.profile_id, p.name, COUNT(pt.track_id), p.created_at, p.updated_at
            FROM playlists p
            LEFT JOIN playlist_tracks pt ON pt.playlist_id = p.id
        "
        .to_string();

        if profile_id.is_some() {
            sql.push_str(" WHERE p.profile_id = ?1");
        }

        sql.push_str(" GROUP BY p.id ORDER BY p.updated_at DESC, p.name");

        let mut statement = connection
            .prepare(&sql)
            .map_err(|error| format!("Failed to prepare playlists query: {error}"))?;

        let rows = if let Some(profile_id) = profile_id {
            statement.query_map(params![profile_id], row_to_playlist)
        } else {
            statement.query_map([], row_to_playlist)
        }
        .map_err(|error| format!("Failed to query playlists: {error}"))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("Failed to read playlists: {error}"))
    }

    fn ensure_profile(&self, id: &str, name: &str) -> Result<String, String> {
        let now = now_timestamp();
        let connection = self.connection.lock().unwrap();
        connection
            .execute(
                "
                INSERT INTO profiles (id, name, created_at, updated_at)
                VALUES (?1, ?2, ?3, ?3)
                ON CONFLICT(id) DO UPDATE SET updated_at = excluded.updated_at
                ",
                params![id, name, now],
            )
            .map_err(|error| format!("Failed to ensure profile: {error}"))?;
        Ok(id.to_string())
    }

    fn ensure_playlist(&self, profile_id: &str, name: &str) -> Result<String, String> {
        let now = now_timestamp();
        let connection = self.connection.lock().unwrap();
        let existing: Option<String> = connection
            .query_row(
                "SELECT id FROM playlists WHERE profile_id = ?1 AND name = ?2",
                params![profile_id, name],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| format!("Failed to find playlist: {error}"))?;

        if let Some(id) = existing {
            return Ok(id);
        }

        let id = Uuid::new_v4().to_string();
        connection
            .execute(
                "
                INSERT INTO playlists (id, profile_id, name, created_at, updated_at)
                VALUES (?1, ?2, ?3, ?4, ?4)
                ",
                params![id, profile_id, name, now],
            )
            .map_err(|error| format!("Failed to create playlist: {error}"))?;
        Ok(id)
    }
}

fn row_to_track(row: &rusqlite::Row<'_>) -> rusqlite::Result<Track> {
    Ok(Track {
        id: row.get(0)?,
        path: row.get(1)?,
        name: row.get(2)?,
        title: row.get(3)?,
        artist: row.get(4)?,
        album: row.get(5)?,
        album_artist: row.get(6)?,
        genre: row.get(7)?,
        year: row.get(8)?,
        track_number: row.get(9)?,
        disc_number: row.get(10)?,
        format: row.get(11)?,
        duration_seconds: row.get(12)?,
        sample_rate: row.get(13)?,
        channels: row.get(14)?,
        bit_depth: row.get(15)?,
        file_size: row.get(16)?,
        modified_at: row.get(17)?,
        indexed_at: row.get(18)?,
    })
}

fn row_to_playlist(row: &rusqlite::Row<'_>) -> rusqlite::Result<PlaylistInfo> {
    Ok(PlaylistInfo {
        id: row.get(0)?,
        profile_id: row.get(1)?,
        name: row.get(2)?,
        track_count: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

fn next_playlist_position(connection: &Connection, playlist_id: &str) -> Result<i64, String> {
    connection
        .query_row(
            "SELECT COALESCE(MAX(position), -1) + 1 FROM playlist_tracks WHERE playlist_id = ?1",
            params![playlist_id],
            |row| row.get(0),
        )
        .map_err(|error| format!("Failed to calculate playlist position: {error}"))
}

fn compact_playlist_positions(connection: &Connection, playlist_id: &str) -> Result<(), String> {
    let track_ids = {
        let mut statement = connection
            .prepare(
                "SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position",
            )
            .map_err(|error| format!("Failed to prepare playlist compaction: {error}"))?;
        let rows = statement
            .query_map(params![playlist_id], |row| row.get::<_, String>(0))
            .map_err(|error| format!("Failed to query playlist compaction: {error}"))?;
        rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("Failed to read playlist compaction: {error}"))?
    };

    for (position, track_id) in track_ids.iter().enumerate() {
        connection
            .execute(
                "
                UPDATE playlist_tracks
                SET position = ?1
                WHERE playlist_id = ?2 AND track_id = ?3
                ",
                params![position as i64, playlist_id, track_id],
            )
            .map_err(|error| format!("Failed to compact playlist positions: {error}"))?;
    }

    Ok(())
}

fn now_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}
