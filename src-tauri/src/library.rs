use crate::dto::{AlbumSummaryDto, ArtistSummaryDto};
use crate::metadata::{extract_track, is_supported_audio_file, Track};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::Manager;
use uuid::Uuid;
use walkdir::WalkDir;

const TRACK_SELECT_COLUMNS: &str = "t.id, t.path, t.name, t.title, t.artist, t.album, t.album_artist, t.genre,
                        t.year, t.track_number, t.disc_number, t.format, t.duration_seconds,
                        t.sample_rate, t.channels, t.bit_depth, t.lyrics, t.lyrics_source,
                        t.cover_art_data_url, t.cover_art_mime, t.cover_art_source,
                        t.fingerprint_sha256, t.acoustid_fingerprint, t.musicbrainz_recording_id,
                        t.file_size, t.modified_at, t.indexed_at";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistInfo {
    pub id: String,
    pub profile_id: String,
    pub name: String,
    pub track_count: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Serialize, Deserialize)]
struct PlaylistExportJson {
    format: String,
    version: u32,
    name: String,
    exported_at: i64,
    tracks: Vec<TrackExportJson>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TrackExportJson {
    path: String,
    title: String,
    artist: String,
    album: String,
    duration_seconds: Option<f64>,
}

/// Cached ID for the "All Local Files" playlist, so we don't need to hit the
/// database on every single read operation.
pub struct Library {
    db_path: PathBuf,
    connection: Mutex<Connection>,
    default_playlist_id_cache: OnceLock<String>,
    favorites_playlist_id_cache: OnceLock<String>,
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
            default_playlist_id_cache: OnceLock::new(),
            favorites_playlist_id_cache: OnceLock::new(),
        };
        library.initialize()?;
        Ok(library)
    }

    pub fn db_path(&self) -> String {
        self.db_path.to_string_lossy().to_string()
    }

    fn lock_connection(&self) -> Result<std::sync::MutexGuard<'_, Connection>, String> {
        self.connection
            .lock()
            .map_err(|_| "Failed to lock database connection".to_string())
    }

    fn initialize(&self) -> Result<(), String> {
        let connection = self.lock_connection()?;
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
                    lyrics TEXT,
                    lyrics_source TEXT,
                    cover_art_data_url TEXT,
                    cover_art_mime TEXT,
                    cover_art_source TEXT,
                    fingerprint_sha256 TEXT,
                    acoustid_fingerprint TEXT,
                    musicbrainz_recording_id TEXT,
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

        ensure_track_column(&connection, "lyrics", "TEXT")?;
        ensure_track_column(&connection, "lyrics_source", "TEXT")?;
        ensure_track_column(&connection, "cover_art_data_url", "TEXT")?;
        ensure_track_column(&connection, "cover_art_mime", "TEXT")?;
        ensure_track_column(&connection, "cover_art_source", "TEXT")?;
        ensure_track_column(&connection, "fingerprint_sha256", "TEXT")?;
        ensure_track_column(&connection, "acoustid_fingerprint", "TEXT")?;
        ensure_track_column(&connection, "musicbrainz_recording_id", "TEXT")?;

        // Seed the default profile and playlist. We do this once at startup.
        let profile_id = ensure_profile_with_connection(&connection, "default", "Default")?;
        let playlist_id =
            ensure_playlist_with_connection(&connection, &profile_id, "All Local Files")?;
        let favorites_id =
            ensure_playlist_with_connection(&connection, &profile_id, "Favorites")?;

        // Warm the caches.
        let _ = self.default_playlist_id_cache.set(playlist_id.clone());
        let _ = self.favorites_playlist_id_cache.set(favorites_id);

        if let Err(error) = repair_all_playlist_positions(&connection) {
            tracing::warn!("Failed to repair playlist positions on startup: {error}");
        }

        Ok(())
    }

    /// Returns the cached default playlist id, seeding if necessary.
    pub fn default_playlist_id(&self) -> Result<String, String> {
        if let Some(id) = self.default_playlist_id_cache.get() {
            return Ok(id.clone());
        }
        let connection = self.lock_connection()?;
        let profile_id = ensure_profile_with_connection(&connection, "default", "Default")?;
        let playlist_id =
            ensure_playlist_with_connection(&connection, &profile_id, "All Local Files")?;
        let _ = self.default_playlist_id_cache.set(playlist_id.clone());
        Ok(playlist_id)
    }

    /// Returns the cached favorites playlist id, seeding if necessary.
    pub fn favorites_playlist_id(&self) -> Result<String, String> {
        if let Some(id) = self.favorites_playlist_id_cache.get() {
            return Ok(id.clone());
        }
        let connection = self.lock_connection()?;
        let profile_id = ensure_profile_with_connection(&connection, "default", "Default")?;
        let playlist_id =
            ensure_playlist_with_connection(&connection, &profile_id, "Favorites")?;
        let _ = self.favorites_playlist_id_cache.set(playlist_id.clone());
        Ok(playlist_id)
    }

    pub fn add_track_to_default_playlist(&self, path: String) -> Result<Track, String> {
        let playlist_id = self.default_playlist_id()?;
        self.add_track_to_playlist(&playlist_id, path)
    }

    pub fn add_track_to_playlist(&self, playlist_id: &str, path: String) -> Result<Track, String> {
        let existing = {
            let connection = self.lock_connection()?;
            connection
                .query_row(
                    &format!(
                        "SELECT {TRACK_SELECT_COLUMNS}
                         FROM tracks t
                         WHERE t.path = ?1"
                    ),
                    params![path],
                    row_to_track,
                )
                .optional()
                .map_err(|error| format!("Failed to look up track: {error}"))?
        };

        let mut track = match existing {
            Some(track) => track,
            None => extract_track(&path)?,
        };
        let now = now_timestamp();
        let mut connection = self.lock_connection()?;
        let tx = connection
            .transaction()
            .map_err(|e| format!("Failed to begin transaction: {e}"))?;

        let track_id = upsert_track(&tx, &track)?;
        track.id = track_id.clone();

        let already_in_playlist = tx
            .query_row(
                "SELECT 1 FROM playlist_tracks
                 WHERE playlist_id = ?1 AND track_id = ?2",
                params![playlist_id, track_id],
                |_| Ok(()),
            )
            .optional()
            .map_err(|error| format!("Failed to check playlist membership: {error}"))?
            .is_some();
        if already_in_playlist {
            return Err("Track is already in the playlist".to_string());
        }

        let position = next_playlist_position(&tx, playlist_id)?;
        tx.execute(
            "INSERT INTO playlist_tracks (playlist_id, track_id, position, added_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![playlist_id, track_id, position, now],
        )
        .map_err(|error| format!("Failed to add track to playlist: {error}"))?;

        tx.commit()
            .map_err(|e| format!("Failed to commit transaction: {e}"))?;

        Ok(track)
    }

    pub fn remove_track_from_default_playlist(&self, path: String) -> Result<(), String> {
        let playlist_id = self.default_playlist_id()?;
        self.remove_track_from_playlist_by_path(&playlist_id, &path)
    }

    pub fn remove_track_from_playlist_by_path(
        &self,
        playlist_id: &str,
        path: &str,
    ) -> Result<(), String> {
        let mut connection = self.lock_connection()?;
        let tx = connection
            .transaction()
            .map_err(|e| format!("Failed to begin transaction: {e}"))?;

        let deleted = tx
            .execute(
                "DELETE FROM playlist_tracks
                 WHERE playlist_id = ?1
                   AND track_id = (SELECT id FROM tracks WHERE path = ?2)",
                params![playlist_id, path],
            )
            .map_err(|error| format!("Failed to remove playlist track: {error}"))?;
        if deleted == 0 {
            return Err("Track is not in the playlist".to_string());
        }

        tx.commit()
            .map_err(|e| format!("Failed to commit transaction: {e}"))?;

        Ok(())
    }

    pub fn get_default_playlist_tracks(&self) -> Result<Vec<Track>, String> {
        let playlist_id = self.default_playlist_id()?;
        self.get_playlist_tracks(&playlist_id)
    }

    pub fn get_playlist_tracks(&self, playlist_id: &str) -> Result<Vec<Track>, String> {
        let connection = self.lock_connection()?;
        let mut statement = connection
            .prepare(
                &format!(
                    "SELECT {TRACK_SELECT_COLUMNS}
                 FROM playlist_tracks pt
                 JOIN tracks t ON t.id = pt.track_id
                 WHERE pt.playlist_id = ?1
                 ORDER BY pt.position"
                ),
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
        let connection = self.lock_connection()?;
        connection
            .execute(
                "DELETE FROM playlist_tracks WHERE playlist_id = ?1",
                params![playlist_id],
            )
            .map_err(|error| format!("Failed to clear playlist: {error}"))?;
        Ok(())
    }

    // ── Favorites ────────────────────────────────────────────────────────────

    /// Add a track to the Favorites playlist. Extracts metadata and upserts the
    /// track into the library first (so favorites work for any file, not just
    /// already-indexed ones).
    pub fn add_track_to_favorites(&self, path: String) -> Result<Track, String> {
        let playlist_id = self.favorites_playlist_id()?;
        self.add_track_to_playlist(&playlist_id, path)
    }

    /// Remove a track from the Favorites playlist by file path.
    pub fn remove_track_from_favorites(&self, path: &str) -> Result<(), String> {
        let playlist_id = self.favorites_playlist_id()?;
        self.remove_track_from_playlist_by_path(&playlist_id, path)
    }

    /// List every track in the Favorites playlist, ordered by position.
    pub fn get_favorites(&self) -> Result<Vec<Track>, String> {
        let playlist_id = self.favorites_playlist_id()?;
        self.get_playlist_tracks(&playlist_id)
    }

    /// Whether a track (by file path) is in the Favorites playlist.
    pub fn is_track_in_favorites(&self, path: &str) -> Result<bool, String> {
        let playlist_id = self.favorites_playlist_id()?;
        let connection = self.lock_connection()?;
        let in_favorites = connection
            .query_row(
                "SELECT 1 FROM playlist_tracks
                 WHERE playlist_id = ?1
                   AND track_id = (SELECT id FROM tracks WHERE path = ?2)",
                params![playlist_id, path],
                |_| Ok(()),
            )
            .optional()
            .map_err(|error| format!("Failed to check favorites: {error}"))?
            .is_some();
        Ok(in_favorites)
    }

    /// Whether a track is registered in the library and belongs to at least one playlist.
    pub fn is_track_in_any_playlist(&self, path: &str) -> Result<bool, String> {
        let connection = self.lock_connection()?;
        let in_playlist = connection
            .query_row(
                "SELECT 1
                 FROM tracks t
                 INNER JOIN playlist_tracks pt ON pt.track_id = t.id
                 WHERE t.path = ?1
                 LIMIT 1",
                params![path],
                |_| Ok(()),
            )
            .optional()
            .map_err(|error| format!("Failed to check playlist membership: {error}"))?
            .is_some();
        Ok(in_playlist)
    }

    /// Toggle the favorite state of a track. Returns the new state
    /// (`true` = now favorited, `false` = now unfavorited).
    pub fn toggle_favorite(&self, path: &str) -> Result<bool, String> {
        if self.is_track_in_favorites(path)? {
            self.remove_track_from_favorites(path)?;
            Ok(false)
        } else {
            self.add_track_to_favorites(path.to_string())?;
            Ok(true)
        }
    }

    /// Remove every track from the Favorites playlist.
    pub fn clear_favorites(&self) -> Result<(), String> {
        let playlist_id = self.favorites_playlist_id()?;
        let connection = self.lock_connection()?;
        connection
            .execute(
                "DELETE FROM playlist_tracks WHERE playlist_id = ?1",
                params![playlist_id],
            )
            .map_err(|error| format!("Failed to clear favorites: {error}"))?;
        Ok(())
    }

    pub fn index_directory(
        &self,
        profile_id: Option<String>,
        playlist_name: Option<String>,
        directory: String,
    ) -> Result<Vec<Track>, String> {
        let profile_id_str = profile_id.unwrap_or_else(|| "default".to_string());
        let playlist_name_str =
            playlist_name.unwrap_or_else(|| "All Local Files".to_string());

        // Resolve / create the profile and playlist outside the connection lock.
        let playlist_id = {
            let connection = self.lock_connection()?;
            ensure_profile_with_connection(&connection, &profile_id_str, &profile_id_str)?;
            ensure_playlist_with_connection(&connection, &profile_id_str, &playlist_name_str)?
        };

        let directory_path = Path::new(&directory);
        if !directory_path.is_dir() {
            return Err("Library path is not a directory".to_string());
        }

        // Collect all audio file paths first so the WalkDir iterator isn't
        // held across the DB lock acquisition.
        let audio_paths: Vec<String> = WalkDir::new(directory_path)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file())
            .filter(|e| is_supported_audio_file(e.path()))
            .filter_map(|e| e.path().to_str().map(str::to_string))
            .collect();

        let mut tracks = Vec::with_capacity(audio_paths.len());
        let mut failed: Vec<String> = Vec::new();

        // Import everything in a single transaction for much better performance.
        let mut connection = self.lock_connection()?;
        let tx = connection
            .transaction()
            .map_err(|e| format!("Failed to begin transaction: {e}"))?;

        let now = now_timestamp();
        for path in &audio_paths {
            match extract_track(path) {
                Ok(mut track) => {
                    let track_id = match upsert_track(&tx, &track) {
                        Ok(id) => id,
                        Err(e) => {
                            failed.push(format!("{path}: {e}"));
                            continue;
                        }
                    };
                    track.id = track_id.clone();
                    let position = match next_playlist_position(&tx, &playlist_id) {
                        Ok(p) => p,
                        Err(e) => {
                            failed.push(format!("{path}: {e}"));
                            continue;
                        }
                    };
                    match tx.execute(
                        "INSERT OR IGNORE INTO playlist_tracks
                         (playlist_id, track_id, position, added_at)
                         VALUES (?1, ?2, ?3, ?4)",
                        params![playlist_id, track_id, position, now],
                    ) {
                        Ok(0) => {}
                        Ok(_) => tracks.push(track),
                        Err(e) => failed.push(format!("{path}: {e}")),
                    }
                }
                Err(e) => {
                    failed.push(format!("{path}: {e}"));
                }
            }
        }

        tx.commit()
            .map_err(|e| format!("Failed to commit import transaction: {e}"))?;

        if !failed.is_empty() {
            tracing::warn!(
                "Skipped {} file(s) during library scan:\n{}",
                failed.len(),
                failed.join("\n")
            );
        }

        Ok(tracks)
    }

    pub fn list_playlists(&self, profile_id: Option<String>) -> Result<Vec<PlaylistInfo>, String> {
        let connection = self.lock_connection()?;
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

    // ── Playlist CRUD ────────────────────────────────────────────────────────

    pub fn create_playlist(&self, name: &str) -> Result<PlaylistInfo, String> {
        self.insert_playlist(name, false)
    }

    /// Create a playlist for import flows, auto-suffixing duplicate names.
    pub fn create_playlist_for_import(&self, name: &str) -> Result<PlaylistInfo, String> {
        self.insert_playlist(name, true)
    }

    fn insert_playlist(&self, name: &str, allow_duplicate_suffix: bool) -> Result<PlaylistInfo, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Playlist name cannot be empty".to_string());
        }

        let connection = self.lock_connection()?;
        let profile_id = ensure_profile_with_connection(&connection, "default", "Default")?;
        let final_name = if allow_duplicate_suffix {
            self.resolve_unique_playlist_name(&connection, &profile_id, name)?
        } else if self.playlist_name_exists(&connection, &profile_id, name)? {
            return Err(format!("A playlist named \"{name}\" already exists"));
        } else {
            name.to_string()
        };

        let id = Uuid::new_v4().to_string();
        let now = now_timestamp();
        connection
            .execute(
                "INSERT INTO playlists (id, profile_id, name, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?4)",
                params![id, profile_id, final_name, now],
            )
            .map_err(|error| format!("Failed to create playlist: {error}"))?;

        self.playlist_info(&connection, &id)?
            .ok_or_else(|| "Playlist vanished immediately after creation".to_string())
    }

    fn playlist_name_exists(
        &self,
        connection: &Connection,
        profile_id: &str,
        name: &str,
    ) -> Result<bool, String> {
        connection
            .query_row(
                "SELECT 1 FROM playlists WHERE profile_id = ?1 AND name = ?2",
                params![profile_id, name],
                |_| Ok(()),
            )
            .optional()
            .map_err(|error| format!("Failed to check playlist name: {error}"))
            .map(|row| row.is_some())
    }

    fn resolve_unique_playlist_name(
        &self,
        connection: &Connection,
        profile_id: &str,
        base: &str,
    ) -> Result<String, String> {
        if !self.playlist_name_exists(connection, profile_id, base)? {
            return Ok(base.to_string());
        }

        for index in 2..1000 {
            let candidate = format!("{base} ({index})");
            if !self.playlist_name_exists(connection, profile_id, &candidate)? {
                return Ok(candidate);
            }
        }

        Err(format!("Could not find a unique name for playlist \"{base}\""))
    }

    pub fn delete_playlist(&self, playlist_id: &str) -> Result<(), String> {
        let connection = self.lock_connection()?;
        let name: Option<String> = connection
            .query_row(
                "SELECT name FROM playlists WHERE id = ?1",
                params![playlist_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| format!("Failed to look up playlist: {error}"))?;

        match name {
            None => return Err("Playlist not found".to_string()),
            Some(name) if name == "All Local Files" => {
                return Err("The default \"All Local Files\" playlist cannot be deleted".to_string());
            }
            Some(name) if name == "Favorites" => {
                return Err("The \"Favorites\" playlist cannot be deleted".to_string());
            }
            Some(_) => {}
        }

        let deleted = connection
            .execute(
                "DELETE FROM playlists WHERE id = ?1",
                params![playlist_id],
            )
            .map_err(|error| format!("Failed to delete playlist: {error}"))?;
        if deleted == 0 {
            return Err("Playlist not found".to_string());
        }
        Ok(())
    }

    pub fn rename_playlist(&self, playlist_id: &str, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Playlist name cannot be empty".to_string());
        }

        let connection = self.lock_connection()?;
        let (current_name, profile_id): (String, String) = connection
            .query_row(
                "SELECT name, profile_id FROM playlists WHERE id = ?1",
                params![playlist_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|error| format!("Failed to look up playlist: {error}"))?
            .ok_or_else(|| "Playlist not found".to_string())?;

        if current_name == "All Local Files" {
            return Err("The default \"All Local Files\" playlist cannot be renamed".to_string());
        }

        if current_name == "Favorites" {
            return Err("The \"Favorites\" playlist cannot be renamed".to_string());
        }

        if name != current_name && self.playlist_name_exists(&connection, &profile_id, name)? {
            return Err(format!("A playlist named \"{name}\" already exists"));
        }

        let now = now_timestamp();
        connection
            .execute(
                "UPDATE playlists SET name = ?1, updated_at = ?2 WHERE id = ?3",
                params![name, now, playlist_id],
            )
            .map_err(|error| format!("Failed to rename playlist: {error}"))?;
        Ok(())
    }

    pub fn clear_playlist(&self, playlist_id: &str) -> Result<(), String> {
        let connection = self.lock_connection()?;
        connection
            .execute(
                "DELETE FROM playlist_tracks WHERE playlist_id = ?1",
                params![playlist_id],
            )
            .map_err(|error| format!("Failed to clear playlist: {error}"))?;
        Ok(())
    }

    /// Create a playlist from all tracks matching the given album name.
    /// Uses the album name as the playlist name unless `playlist_name` is provided.
    /// Returns an error if no tracks are found for the given album.
    pub fn create_album_playlist(
        &self,
        album: &str,
        playlist_name: Option<&str>,
    ) -> Result<PlaylistInfo, String> {
        let album = album.trim();
        if album.is_empty() {
            return Err("Album name cannot be empty".to_string());
        }

        let mut connection = self.lock_connection()?;
        let tracks = Self::get_tracks_by_column(&connection, "album", album)?;
        if tracks.is_empty() {
            return Err(format!("No tracks found for album \"{album}\""));
        }

        let name = playlist_name.unwrap_or(album);
        let playlist_name_str = name.to_string();

        // Resolve profile and create the playlist outside the transaction.
        let profile_id =
            ensure_profile_with_connection(&connection, "default", "Default")?;
        // Allow duplicate suffixes for auto-generated playlists.
        let final_name =
            self.resolve_unique_playlist_name(&connection, &profile_id, &playlist_name_str)?;

        let playlist_id = Uuid::new_v4().to_string();
        let now = now_timestamp();
        connection
            .execute(
                "INSERT INTO playlists (id, profile_id, name, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?4)",
                params![playlist_id, profile_id, final_name, now],
            )
            .map_err(|error| format!("Failed to create playlist: {error}"))?;

        let tx = connection
            .transaction()
            .map_err(|e| format!("Failed to begin transaction: {e}"))?;

        for (index, track) in tracks.iter().enumerate() {
            tx.execute(
                "INSERT INTO playlist_tracks (playlist_id, track_id, position, added_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![playlist_id, track.id, index as i64, now],
            )
            .map_err(|error| {
                format!("Failed to add track to album playlist: {error}")
            })?;
        }

        tx.commit()
            .map_err(|e| format!("Failed to commit album playlist transaction: {e}"))?;

        self.playlist_info(&connection, &playlist_id)?
            .ok_or_else(|| "Playlist vanished immediately after creation".to_string())
    }

    /// Create a playlist from all tracks matching the given artist name.
    /// Uses the artist name as the playlist name unless `playlist_name` is provided.
    /// Returns an error if no tracks are found for the given artist.
    pub fn create_artist_playlist(
        &self,
        artist: &str,
        playlist_name: Option<&str>,
    ) -> Result<PlaylistInfo, String> {
        let artist = artist.trim();
        if artist.is_empty() {
            return Err("Artist name cannot be empty".to_string());
        }

        let mut connection = self.lock_connection()?;
        let tracks = Self::get_tracks_by_column(&connection, "artist", artist)?;
        if tracks.is_empty() {
            return Err(format!("No tracks found for artist \"{artist}\""));
        }

        let name = playlist_name.unwrap_or(artist);
        let playlist_name_str = name.to_string();

        let profile_id =
            ensure_profile_with_connection(&connection, "default", "Default")?;
        let final_name =
            self.resolve_unique_playlist_name(&connection, &profile_id, &playlist_name_str)?;

        let playlist_id = Uuid::new_v4().to_string();
        let now = now_timestamp();
        connection
            .execute(
                "INSERT INTO playlists (id, profile_id, name, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?4)",
                params![playlist_id, profile_id, final_name, now],
            )
            .map_err(|error| format!("Failed to create playlist: {error}"))?;

        let tx = connection
            .transaction()
            .map_err(|e| format!("Failed to begin transaction: {e}"))?;

        for (index, track) in tracks.iter().enumerate() {
            tx.execute(
                "INSERT INTO playlist_tracks (playlist_id, track_id, position, added_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![playlist_id, track.id, index as i64, now],
            )
            .map_err(|error| {
                format!("Failed to add track to artist playlist: {error}")
            })?;
        }

        tx.commit()
            .map_err(|e| format!("Failed to commit artist playlist transaction: {e}"))?;

        self.playlist_info(&connection, &playlist_id)?
            .ok_or_else(|| "Playlist vanished immediately after creation".to_string())
    }

    /// Query tracks where a given column equals a value, ordered by
    /// album → disc_number → track_number for sensible ordering.
    fn get_tracks_by_column(
        connection: &Connection,
        column: &str,
        value: &str,
    ) -> Result<Vec<Track>, String> {
        let sql = format!(
            "SELECT {TRACK_SELECT_COLUMNS}
             FROM tracks t
             WHERE t.{column} = ?1
             ORDER BY t.album, COALESCE(t.disc_number, 1), COALESCE(t.track_number, 0)"
        );
        let mut statement = connection
            .prepare(&sql)
            .map_err(|error| format!("Failed to prepare query by {column}: {error}"))?;
        let tracks = statement
            .query_map(params![value], row_to_track)
            .map_err(|error| format!("Failed to query by {column}: {error}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("Failed to read tracks by {column}: {error}"))?;
        Ok(tracks)
    }

    // ── Album / artist browsing & querying ───────────────────────────────────

    /// List every distinct album in the library grouped by
    /// `(album, COALESCE(album_artist, artist))`, ordered by album artist then
    /// album name. Each entry carries aggregate info (track count, year,
    /// representative cover art) suitable for a browse grid.
    pub fn list_albums(&self) -> Result<Vec<AlbumSummaryDto>, String> {
        let connection = self.lock_connection()?;
        let mut statement = connection
            .prepare(
                "SELECT
                    t.album,
                    COALESCE(NULLIF(t.album_artist, ''), t.artist) AS album_artist,
                    MIN(t.artist) AS artist,
                    COUNT(*) AS track_count,
                    MIN(t.year) AS year,
                    MIN(t.cover_art_data_url) AS cover_art_data_url,
                    MIN(t.cover_art_mime) AS cover_art_mime
                 FROM tracks t
                 GROUP BY t.album, COALESCE(NULLIF(t.album_artist, ''), t.artist)
                 ORDER BY album_artist, t.album",
            )
            .map_err(|error| format!("Failed to prepare albums query: {error}"))?;
        let albums = statement
            .query_map([], row_to_album_summary)
            .map_err(|error| format!("Failed to query albums: {error}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("Failed to read albums: {error}"))?;
        Ok(albums)
    }

    /// List every distinct artist in the library (by the track `artist` tag),
    /// with aggregate track and album counts, ordered by artist name.
    pub fn list_artists(&self) -> Result<Vec<ArtistSummaryDto>, String> {
        let connection = self.lock_connection()?;
        let mut statement = connection
            .prepare(
                "SELECT
                    t.artist,
                    COUNT(*) AS track_count,
                    COUNT(DISTINCT t.album) AS album_count
                 FROM tracks t
                 GROUP BY t.artist
                 ORDER BY t.artist",
            )
            .map_err(|error| format!("Failed to prepare artists query: {error}"))?;
        let artists = statement
            .query_map([], row_to_artist_summary)
            .map_err(|error| format!("Failed to query artists: {error}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("Failed to read artists: {error}"))?;
        Ok(artists)
    }

    /// Return every track belonging to an album.
    ///
    /// When `album_artist` is provided, tracks are matched on both `album` and
    /// the resolved album artist (`COALESCE(album_artist, artist)`). This keeps
    /// same-named albums by different artists apart — pass the value from an
    /// [`AlbumSummaryDto`] (or a clicked `Track`'s `album_artist` falling back to
    /// `artist`) for a precise, Spotify-style "go to album" result.
    ///
    /// When `album_artist` is `None`, only the `album` name is matched (which may
    /// merge same-named albums across artists).
    ///
    /// Tracks are ordered by disc number then track number.
    pub fn get_tracks_by_album(
        &self,
        album: &str,
        album_artist: Option<&str>,
    ) -> Result<Vec<Track>, String> {
        let album = album.trim();
        if album.is_empty() {
            return Err("Album name cannot be empty".to_string());
        }
        let album_artist = album_artist.map(str::trim).filter(|a| !a.is_empty());

        let connection = self.lock_connection()?;
        let sql = if album_artist.is_some() {
            format!(
                "SELECT {TRACK_SELECT_COLUMNS}
                 FROM tracks t
                 WHERE t.album = ?1
                   AND COALESCE(NULLIF(t.album_artist, ''), t.artist) = ?2
                 ORDER BY COALESCE(t.disc_number, 1), COALESCE(t.track_number, 0)"
            )
        } else {
            format!(
                "SELECT {TRACK_SELECT_COLUMNS}
                 FROM tracks t
                 WHERE t.album = ?1
                 ORDER BY COALESCE(t.disc_number, 1), COALESCE(t.track_number, 0)"
            )
        };

        let mut statement = connection
            .prepare(&sql)
            .map_err(|error| format!("Failed to prepare album tracks query: {error}"))?;

        let rows = match album_artist {
            Some(album_artist) => statement
                .query_map(params![album, album_artist], row_to_track)
                .map_err(|error| format!("Failed to query album tracks: {error}"))?,
            None => statement
                .query_map(params![album], row_to_track)
                .map_err(|error| format!("Failed to query album tracks: {error}"))?,
        };

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("Failed to read album tracks: {error}"))
    }

    /// Return every track by an artist (a discography), ordered by album then
    /// disc number then track number. Matches the track `artist` tag.
    pub fn get_tracks_by_artist(&self, artist: &str) -> Result<Vec<Track>, String> {
        let artist = artist.trim();
        if artist.is_empty() {
            return Err("Artist name cannot be empty".to_string());
        }
        let connection = self.lock_connection()?;
        Self::get_tracks_by_column(&connection, "artist", artist)
    }

    pub fn get_playlist_info(&self, playlist_id: &str) -> Result<Option<PlaylistInfo>, String> {
        let connection = self.lock_connection()?;
        self.playlist_info(&connection, playlist_id)
    }

    fn playlist_info(
        &self,
        connection: &Connection,
        playlist_id: &str,
    ) -> Result<Option<PlaylistInfo>, String> {
        connection
            .query_row(
                "SELECT p.id, p.profile_id, p.name, COUNT(pt.track_id), p.created_at, p.updated_at
                 FROM playlists p
                 LEFT JOIN playlist_tracks pt ON pt.playlist_id = p.id
                 WHERE p.id = ?1
                 GROUP BY p.id",
                params![playlist_id],
                row_to_playlist,
            )
            .optional()
            .map_err(|error| format!("Failed to query playlist: {error}"))
    }

    /// Look up full `Track` records for a list of file paths (used by the queue).
    /// Returns `Some(track)` for found tracks and `None` for paths not in the
    /// library, preserving the input order.
    pub fn get_tracks_by_paths(&self, paths: &[String]) -> Result<Vec<Option<Track>>, String> {
        if paths.is_empty() {
            return Ok(Vec::new());
        }
        let connection = self.lock_connection()?;
        let mut tracks = Vec::with_capacity(paths.len());
        for path in paths {
            let track = connection
                .query_row(
                    &format!(
                        "SELECT {TRACK_SELECT_COLUMNS}
                         FROM tracks t
                         WHERE t.path = ?1"
                    ),
                    params![path],
                    row_to_track,
                )
                .optional()
                .map_err(|error| format!("Failed to query track by path: {error}"))?;
            tracks.push(track);
        }
        Ok(tracks)
    }

    // ── Export / Import ──────────────────────────────────────────────────────

    /// Export a playlist as an M3U8 file (a plain-text list of file paths).
    pub fn export_playlist_m3u(
        &self,
        playlist_id: &str,
        output_path: &str,
    ) -> Result<(), String> {
        let tracks = self.get_playlist_tracks(playlist_id)?;
        let mut content = String::from("#EXTM3U\n");
        for track in &tracks {
            let duration = track.duration_seconds.map(|d| d as i64).unwrap_or(-1);
            content.push_str(&format!(
                "#EXTINF:{},{} - {}\n",
                duration, track.artist, track.title
            ));
            content.push_str(&track.path);
            content.push('\n');
        }
        std::fs::write(output_path, content)
            .map_err(|error| format!("Failed to write M3U file: {error}"))?;
        Ok(())
    }

    /// Import an M3U/M3U8 file, creating a new playlist and adding all
    /// referenced files to it. Returns the new playlist id and imported tracks.
    pub fn import_playlist_m3u(
        &self,
        m3u_path: &str,
        playlist_name: Option<&str>,
    ) -> Result<(String, Vec<Track>), String> {
        let content =
            std::fs::read_to_string(m3u_path).map_err(|error| format!("Failed to read M3U file: {error}"))?;
        let name = playlist_name.unwrap_or_else(|| {
            Path::new(m3u_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Imported Playlist")
        });

        let playlist_info = self.create_playlist_for_import(name)?;
        let playlist_id = playlist_info.id;

        let mut tracks = Vec::new();
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            match self.add_track_to_playlist(&playlist_id, line.to_string()) {
                Ok(track) => tracks.push(track),
                Err(error) => tracing::warn!("Skipped during M3U import — {line}: {error}"),
            }
        }
        Ok((playlist_id, tracks))
    }

    /// Export a playlist as a Wave JSON file (paths + metadata).
    pub fn export_playlist_json(
        &self,
        playlist_id: &str,
        output_path: &str,
    ) -> Result<(), String> {
        let tracks = self.get_playlist_tracks(playlist_id)?;
        let info = self
            .get_playlist_info(playlist_id)?
            .ok_or("Playlist not found")?;

        let export = PlaylistExportJson {
            format: "wave-playlist".to_string(),
            version: 1,
            name: info.name,
            exported_at: now_timestamp(),
            tracks: tracks
                .iter()
                .map(|t| TrackExportJson {
                    path: t.path.clone(),
                    title: t.title.clone(),
                    artist: t.artist.clone(),
                    album: t.album.clone(),
                    duration_seconds: t.duration_seconds,
                })
                .collect(),
        };

        let json = serde_json::to_string_pretty(&export)
            .map_err(|error| format!("Failed to serialize playlist JSON: {error}"))?;
        std::fs::write(output_path, json)
            .map_err(|error| format!("Failed to write JSON file: {error}"))?;
        Ok(())
    }

    /// Import a Wave JSON playlist file, creating a new playlist.
    pub fn import_playlist_json(
        &self,
        json_path: &str,
        playlist_name: Option<&str>,
    ) -> Result<(String, Vec<Track>), String> {
        let content = std::fs::read_to_string(json_path)
            .map_err(|error| format!("Failed to read JSON file: {error}"))?;
        let export: PlaylistExportJson = serde_json::from_str(&content)
            .map_err(|error| format!("Failed to parse playlist JSON: {error}"))?;

        let name = playlist_name.unwrap_or(&export.name);
        let playlist_info = self.create_playlist_for_import(name)?;
        let playlist_id = playlist_info.id;

        let mut tracks = Vec::new();
        for track in &export.tracks {
            match self.add_track_to_playlist(&playlist_id, track.path.clone()) {
                Ok(t) => tracks.push(t),
                Err(error) => {
                    tracing::warn!("Skipped during JSON import — {}: {error}", track.path)
                }
            }
        }
        Ok((playlist_id, tracks))
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
        lyrics: row.get(16)?,
        lyrics_source: row.get(17)?,
        cover_art_data_url: row.get(18)?,
        cover_art_mime: row.get(19)?,
        cover_art_source: row.get(20)?,
        fingerprint_sha256: row.get(21)?,
        acoustid_fingerprint: row.get(22)?,
        musicbrainz_recording_id: row.get(23)?,
        file_size: row.get(24)?,
        modified_at: row.get(25)?,
        indexed_at: row.get(26)?,
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

fn row_to_album_summary(row: &rusqlite::Row<'_>) -> rusqlite::Result<AlbumSummaryDto> {
    Ok(AlbumSummaryDto {
        name: row.get(0)?,
        album_artist: row.get(1)?,
        artist: row.get(2)?,
        track_count: row.get(3)?,
        year: row.get(4)?,
        cover_art_data_url: row.get(5)?,
        cover_art_mime: row.get(6)?,
    })
}

fn row_to_artist_summary(row: &rusqlite::Row<'_>) -> rusqlite::Result<ArtistSummaryDto> {
    Ok(ArtistSummaryDto {
        name: row.get(0)?,
        track_count: row.get(1)?,
        album_count: row.get(2)?,
    })
}

/// A trait that abstracts over `Connection` and `Transaction` so that our
/// helpers can be called in both contexts without duplicating code.
trait Queryable {
    fn exec<P: rusqlite::Params>(&self, sql: &str, params: P) -> rusqlite::Result<usize>;
    fn query_opt<T, P, F>(&self, sql: &str, params: P, f: F) -> rusqlite::Result<Option<T>>
    where
        P: rusqlite::Params,
        F: FnOnce(&rusqlite::Row<'_>) -> rusqlite::Result<T>;
}

impl Queryable for Connection {
    fn exec<P: rusqlite::Params>(&self, sql: &str, params: P) -> rusqlite::Result<usize> {
        self.execute(sql, params)
    }
    fn query_opt<T, P, F>(&self, sql: &str, params: P, f: F) -> rusqlite::Result<Option<T>>
    where
        P: rusqlite::Params,
        F: FnOnce(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
    {
        self.query_row(sql, params, f).optional()
    }
}

impl Queryable for Transaction<'_> {
    fn exec<P: rusqlite::Params>(&self, sql: &str, params: P) -> rusqlite::Result<usize> {
        self.execute(sql, params)
    }
    fn query_opt<T, P, F>(&self, sql: &str, params: P, f: F) -> rusqlite::Result<Option<T>>
    where
        P: rusqlite::Params,
        F: FnOnce(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
    {
        self.query_row(sql, params, f).optional()
    }
}

impl Queryable for std::sync::MutexGuard<'_, Connection> {
    fn exec<P: rusqlite::Params>(&self, sql: &str, params: P) -> rusqlite::Result<usize> {
        self.execute(sql, params)
    }
    fn query_opt<T, P, F>(&self, sql: &str, params: P, f: F) -> rusqlite::Result<Option<T>>
    where
        P: rusqlite::Params,
        F: FnOnce(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
    {
        self.query_row(sql, params, f).optional()
    }
}

fn ensure_profile_with_connection(
    conn: &impl Queryable,
    id: &str,
    name: &str,
) -> Result<String, String> {
    let now = now_timestamp();
    conn.exec(
        "INSERT INTO profiles (id, name, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?3)
         ON CONFLICT(id) DO UPDATE SET updated_at = excluded.updated_at",
        params![id, name, now],
    )
    .map_err(|error| format!("Failed to ensure profile: {error}"))?;
    Ok(id.to_string())
}

fn ensure_playlist_with_connection(
    conn: &impl Queryable,
    profile_id: &str,
    name: &str,
) -> Result<String, String> {
    if let Some(id) = conn
        .query_opt(
            "SELECT id FROM playlists WHERE profile_id = ?1 AND name = ?2",
            params![profile_id, name],
            |row| row.get::<_, String>(0),
        )
        .map_err(|error| format!("Failed to find playlist: {error}"))?
    {
        return Ok(id);
    }

    let now = now_timestamp();
    let id = Uuid::new_v4().to_string();
    conn.exec(
        "INSERT INTO playlists (id, profile_id, name, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?4)",
        params![id, profile_id, name, now],
    )
    .map_err(|error| format!("Failed to create playlist: {error}"))?;
    Ok(id)
}

fn lookup_track_id(conn: &impl Queryable, path: &str) -> Result<String, String> {
    conn.query_opt(
        "SELECT id FROM tracks WHERE path = ?1",
        params![path],
        |row| row.get(0),
    )
    .map_err(|error| format!("Failed to look up track id: {error}"))?
    .ok_or_else(|| format!("Track not found in library: {path}"))
}

fn upsert_track(conn: &impl Queryable, track: &Track) -> Result<String, String> {
    conn.exec(
        "INSERT INTO tracks (
            id, path, name, title, artist, album, album_artist, genre, year, track_number,
            disc_number, format, duration_seconds, sample_rate, channels, bit_depth,
            lyrics, lyrics_source, cover_art_data_url, cover_art_mime, cover_art_source,
            fingerprint_sha256, acoustid_fingerprint, musicbrainz_recording_id, file_size,
            modified_at, indexed_at
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
            ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20,
            ?21, ?22, ?23, ?24, ?25, ?26, ?27
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
            lyrics = excluded.lyrics,
            lyrics_source = excluded.lyrics_source,
            cover_art_data_url = excluded.cover_art_data_url,
            cover_art_mime = excluded.cover_art_mime,
            cover_art_source = excluded.cover_art_source,
            fingerprint_sha256 = excluded.fingerprint_sha256,
            acoustid_fingerprint = excluded.acoustid_fingerprint,
            musicbrainz_recording_id = excluded.musicbrainz_recording_id,
            file_size = excluded.file_size,
            modified_at = excluded.modified_at,
            indexed_at = excluded.indexed_at",
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
            track.lyrics,
            track.lyrics_source,
            track.cover_art_data_url,
            track.cover_art_mime,
            track.cover_art_source,
            track.fingerprint_sha256,
            track.acoustid_fingerprint,
            track.musicbrainz_recording_id,
            track.file_size,
            track.modified_at,
            track.indexed_at
        ],
    )
    .map_err(|error| format!("Failed to upsert track: {error}"))?;
    lookup_track_id(conn, &track.path)
}

fn ensure_track_column(
    connection: &Connection,
    column_name: &str,
    column_type: &str,
) -> Result<(), String> {
    let mut statement = connection
        .prepare("PRAGMA table_info(tracks)")
        .map_err(|error| format!("Failed to inspect tracks schema: {error}"))?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| format!("Failed to inspect tracks columns: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Failed to read tracks columns: {error}"))?;

    if columns.iter().any(|column| column == column_name) {
        return Ok(());
    }

    connection
        .execute(
            &format!("ALTER TABLE tracks ADD COLUMN {column_name} {column_type}"),
            [],
        )
        .map_err(|error| format!("Failed to add tracks.{column_name}: {error}"))?;
    Ok(())
}

fn next_playlist_position(conn: &impl Queryable, playlist_id: &str) -> Result<i64, String> {
    conn.query_opt(
        "SELECT COALESCE(MAX(position), -1) + 1 FROM playlist_tracks WHERE playlist_id = ?1",
        params![playlist_id],
        |row| row.get(0),
    )
    .map_err(|error| format!("Failed to calculate playlist position: {error}"))?
    .ok_or_else(|| "Failed to compute next playlist position".to_string())
}

/// Re-numbers all positions in a playlist so they are contiguous starting at 0.
/// Uses a two-phase update so UNIQUE(playlist_id, position) is never violated mid-flight.
fn repair_all_playlist_positions(connection: &Connection) -> Result<(), String> {
    let mut statement = connection
        .prepare("SELECT id FROM playlists")
        .map_err(|error| format!("Failed to prepare playlist repair query: {error}"))?;
    let playlist_ids = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| format!("Failed to query playlists for repair: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Failed to read playlist ids for repair: {error}"))?;

    for playlist_id in playlist_ids {
        let tx = connection
            .unchecked_transaction()
            .map_err(|error| format!("Failed to begin playlist repair transaction: {error}"))?;
        compact_playlist_positions(&tx, &playlist_id)?;
        tx.commit()
            .map_err(|error| format!("Failed to commit playlist repair transaction: {error}"))?;
    }

    Ok(())
}

fn compact_playlist_positions(
    tx: &Transaction<'_>,
    playlist_id: &str,
) -> Result<(), String> {
    let mut statement = tx
        .prepare(
            "SELECT track_id FROM playlist_tracks
             WHERE playlist_id = ?1
             ORDER BY position, added_at",
        )
        .map_err(|error| format!("Failed to prepare playlist compaction query: {error}"))?;

    let track_ids = statement
        .query_map(params![playlist_id], |row| row.get::<_, String>(0))
        .map_err(|error| format!("Failed to read playlist tracks for compaction: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Failed to read playlist track ids: {error}"))?;

    for (index, track_id) in track_ids.iter().enumerate() {
        tx.execute(
            "UPDATE playlist_tracks
             SET position = ?1
             WHERE playlist_id = ?2 AND track_id = ?3",
            params![-(index as i64 + 1), playlist_id, track_id],
        )
        .map_err(|error| format!("Failed to stage playlist positions: {error}"))?;
    }

    for (index, track_id) in track_ids.iter().enumerate() {
        tx.execute(
            "UPDATE playlist_tracks
             SET position = ?1
             WHERE playlist_id = ?2 AND track_id = ?3",
            params![index as i64, playlist_id, track_id],
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn open_test_library() -> Result<Library, String> {
        let connection = Connection::open_in_memory()
            .map_err(|error| format!("Failed to open in-memory database: {error}"))?;
        let library = Library {
            db_path: PathBuf::from(":memory:"),
            connection: Mutex::new(connection),
            default_playlist_id_cache: OnceLock::new(),
            favorites_playlist_id_cache: OnceLock::new(),
        };
        library.initialize()?;
        Ok(library)
    }

    fn sample_track(id: &str, path: &str) -> Track {
        Track {
            id: id.to_string(),
            path: path.to_string(),
            name: "song.mp3".to_string(),
            title: "Song".to_string(),
            artist: "Artist".to_string(),
            album: "Album".to_string(),
            album_artist: None,
            genre: None,
            year: None,
            track_number: None,
            disc_number: None,
            format: "MP3".to_string(),
            duration_seconds: Some(180.0),
            sample_rate: Some(44_100),
            channels: Some(2),
            bit_depth: None,
            lyrics: None,
            lyrics_source: None,
            cover_art_data_url: None,
            cover_art_mime: None,
            cover_art_source: None,
            fingerprint_sha256: None,
            acoustid_fingerprint: None,
            musicbrainz_recording_id: None,
            file_size: 1,
            modified_at: 1,
            indexed_at: 1,
        }
    }

    fn insert_playlist_track_with_connection(
        connection: &Connection,
        playlist_id: &str,
        track_id: &str,
        position: i64,
    ) -> Result<(), String> {
        connection
            .execute(
                "INSERT INTO playlist_tracks (playlist_id, track_id, position, added_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![playlist_id, track_id, position, now_timestamp()],
            )
            .map_err(|error| format!("Failed to insert playlist track: {error}"))?;
        Ok(())
    }

    #[test]
    fn upsert_track_preserves_existing_id_for_same_path() {
        let library = open_test_library().expect("library");
        let connection = library.lock_connection().expect("connection");
        let first = sample_track("stable-id", "/music/song.mp3");
        let first_id = upsert_track(&*connection, &first).expect("first upsert");
        assert_eq!(first_id, "stable-id");

        let second = sample_track("new-random-id", "/music/song.mp3");
        let second_id = upsert_track(&*connection, &second).expect("second upsert");
        assert_eq!(second_id, "stable-id");
    }

    #[test]
    fn playlist_track_can_be_removed_and_readded_by_path() {
        let library = open_test_library().expect("library");
        let playlist_id = library.default_playlist_id().expect("playlist");
        let track_path = "/music/replay.mp3";

        let track = sample_track("track-a", track_path);
        {
            let connection = library.lock_connection().expect("connection");
            let track_id = upsert_track(&*connection, &track).expect("upsert");
            insert_playlist_track_with_connection(&connection, &playlist_id, &track_id, 0)
                .expect("insert");
        }

        library
            .remove_track_from_playlist_by_path(&playlist_id, track_path)
            .expect("remove");

        {
            let connection = library.lock_connection().expect("connection");
            let refreshed = sample_track("track-b", track_path);
            let canonical_id = upsert_track(&*connection, &refreshed).expect("re-upsert");
            insert_playlist_track_with_connection(&connection, &playlist_id, &canonical_id, 0)
                .expect("reinsert");
        }

        let tracks = library
            .get_playlist_tracks(&playlist_id)
            .expect("playlist tracks");
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].id, "track-a");
        assert_eq!(tracks[0].path, track_path);
    }

    #[test]
    fn compact_playlist_positions_renumbers_without_unique_conflicts() {
        let library = open_test_library().expect("library");
        let playlist_id = library.default_playlist_id().expect("playlist");

        {
            let mut connection = library.lock_connection().expect("connection");
            let track_a = sample_track("a", "/music/a.mp3");
            let track_b = sample_track("b", "/music/b.mp3");
            let track_c = sample_track("c", "/music/c.mp3");
            let id_a = upsert_track(&*connection, &track_a).expect("upsert a");
            let id_b = upsert_track(&*connection, &track_b).expect("upsert b");
            let id_c = upsert_track(&*connection, &track_c).expect("upsert c");

            insert_playlist_track_with_connection(&connection, &playlist_id, &id_a, 0)
                .expect("insert a");
            insert_playlist_track_with_connection(&connection, &playlist_id, &id_b, 2)
                .expect("insert b");
            insert_playlist_track_with_connection(&connection, &playlist_id, &id_c, 4)
                .expect("insert c");

            let tx = connection.transaction().expect("transaction");
            compact_playlist_positions(&tx, &playlist_id).expect("compact");
            tx.commit().expect("commit");
        }

        let connection = library.lock_connection().expect("connection");
        let positions: Vec<i64> = connection
            .prepare(
                "SELECT position FROM playlist_tracks
                 WHERE playlist_id = ?1
                 ORDER BY position",
            )
            .expect("prepare")
            .query_map(params![playlist_id], |row| row.get(0))
            .expect("query")
            .collect::<Result<Vec<_>, _>>()
            .expect("rows");

        assert_eq!(positions, vec![0, 1, 2]);
    }

    #[test]
    fn remove_track_by_path_fails_for_missing_entries() {
        let library = open_test_library().expect("library");
        let playlist_id = library.default_playlist_id().expect("playlist");

        let err = library
            .remove_track_from_playlist_by_path(&playlist_id, "/music/missing.mp3")
            .expect_err("missing track should fail");
        assert!(err.contains("not in the playlist"));
    }

    #[test]
    fn remove_track_leaves_position_gaps_without_error() {
        let library = open_test_library().expect("library");
        let playlist_id = library.default_playlist_id().expect("playlist");

        {
            let connection = library.lock_connection().expect("connection");
            let track_a = sample_track("a", "/music/a.mp3");
            let track_b = sample_track("b", "/music/b.mp3");
            let id_a = upsert_track(&*connection, &track_a).expect("upsert a");
            let id_b = upsert_track(&*connection, &track_b).expect("upsert b");
            insert_playlist_track_with_connection(&connection, &playlist_id, &id_a, 0)
                .expect("insert a");
            insert_playlist_track_with_connection(&connection, &playlist_id, &id_b, 1)
                .expect("insert b");
        }

        library
            .remove_track_from_playlist_by_path(&playlist_id, "/music/a.mp3")
            .expect("remove first track");

        let tracks = library
            .get_playlist_tracks(&playlist_id)
            .expect("playlist tracks");
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].path, "/music/b.mp3");
    }

    // ── Album / artist browsing & querying ──────────────────────────────────

    /// Build a `Track` with customizable album/artist metadata for browse tests.
    fn track_with(
        id: &str,
        path: &str,
        artist: &str,
        album: &str,
        album_artist: Option<&str>,
        track_number: Option<i32>,
        disc_number: Option<i32>,
        year: Option<i32>,
        cover: Option<&str>,
    ) -> Track {
        let mut t = sample_track(id, path);
        t.artist = artist.to_string();
        t.album = album.to_string();
        t.album_artist = album_artist.map(String::from);
        t.track_number = track_number;
        t.disc_number = disc_number;
        t.year = year;
        t.cover_art_data_url = cover.map(String::from);
        t.cover_art_mime = cover.map(|_| "image/jpeg".to_string());
        t
    }

    fn upsert_many(connection: &Connection, tracks: &[Track]) {
        for track in tracks {
            upsert_track(connection, track).expect("upsert");
        }
    }

    fn seed_library_for_browse_tests(library: &Library) {
        let connection = library.lock_connection().expect("connection");
        upsert_many(
            &connection,
            &[
                // "Abbey Road" by The Beatles (album_artist set, 3 tracks).
                track_with("b1", "/m/abbey-1.flac", "The Beatles", "Abbey Road", Some("The Beatles"), Some(1), Some(1), Some(1969), Some("data:abbey")),
                track_with("b2", "/m/abbey-2.flac", "The Beatles", "Abbey Road", Some("The Beatles"), Some(2), Some(1), Some(1969), Some("data:abbey")),
                track_with("b3", "/m/abbey-3.flac", "The Beatles", "Abbey Road", Some("The Beatles"), Some(3), Some(1), Some(1969), Some("data:abbey")),
                // A second Beatles album so album_count > 1.
                track_with("b4", "/m/letitbe-1.flac", "The Beatles", "Let It Be", Some("The Beatles"), Some(1), Some(1), Some(1970), Some("data:letitbe")),
                // "Greatest Hits" collision: Queen (2 tracks) vs ABBA (1 track, no cover).
                track_with("q1", "/m/queen-1.flac", "Queen", "Greatest Hits", Some("Queen"), Some(1), Some(1), Some(1981), Some("data:queen")),
                track_with("q2", "/m/queen-2.flac", "Queen", "Greatest Hits", Some("Queen"), Some(2), Some(1), Some(1981), Some("data:queen")),
                track_with("a1", "/m/abba-1.flac", "ABBA", "Greatest Hits", Some("ABBA"), Some(1), Some(1), Some(1975), None),
                // Album with no album_artist tag → resolved album_artist falls back to artist.
                track_with("s1", "/m/solo-1.flac", "Solo", "No Album Artist", None, Some(1), Some(1), Some(2020), None),
            ],
        );
    }

    #[test]
    fn list_albums_groups_by_album_and_resolved_album_artist() {
        let library = open_test_library().expect("library");
        seed_library_for_browse_tests(&library);

        let albums = library.list_albums().expect("albums");

        // 5 distinct (album, album_artist) groups — "Greatest Hits" appears twice
        // and The Beatles have two separate albums.
        assert_eq!(albums.len(), 5);

        // Ordered by album_artist then album.
        let names: Vec<(&str, &str, i64)> = albums
            .iter()
            .map(|a| (a.name.as_str(), a.album_artist.as_deref().unwrap_or(""), a.track_count))
            .collect();
        assert_eq!(
            names,
            vec![
                ("Greatest Hits", "ABBA", 1),
                ("Greatest Hits", "Queen", 2),
                ("No Album Artist", "Solo", 1),
                ("Abbey Road", "The Beatles", 3),
                ("Let It Be", "The Beatles", 1),
            ]
        );

        let abbey = albums.iter().find(|a| a.name == "Abbey Road").unwrap();
        assert_eq!(abbey.artist, "The Beatles");
        assert_eq!(abbey.year, Some(1969));
        assert_eq!(abbey.cover_art_data_url.as_deref(), Some("data:abbey"));
        assert_eq!(abbey.cover_art_mime.as_deref(), Some("image/jpeg"));

        let abba = albums.iter().find(|a| a.name == "Greatest Hits" && a.album_artist.as_deref() == Some("ABBA")).unwrap();
        assert_eq!(abba.year, Some(1975));
        assert!(abba.cover_art_data_url.is_none());

        // An album with a NULL album_artist tag resolves to the track artist.
        let solo = albums.iter().find(|a| a.name == "No Album Artist").unwrap();
        assert_eq!(solo.album_artist.as_deref(), Some("Solo"));
    }

    #[test]
    fn list_artists_aggregates_track_and_album_counts() {
        let library = open_test_library().expect("library");
        seed_library_for_browse_tests(&library);

        let artists = library.list_artists().expect("artists");

        // Ordered by artist name.
        let by_name: Vec<(&str, i64, i64)> = artists
            .iter()
            .map(|a| (a.name.as_str(), a.track_count, a.album_count))
            .collect();
        assert_eq!(
            by_name,
            vec![
                ("ABBA", 1, 1),
                ("Queen", 2, 1),
                ("Solo", 1, 1),
                ("The Beatles", 4, 2), // 4 tracks across 2 albums
            ]
        );
    }

    #[test]
    fn get_tracks_by_album_disambiguates_same_named_albums() {
        let library = open_test_library().expect("library");
        seed_library_for_browse_tests(&library);

        // Precise match using album_artist keeps the two "Greatest Hits" apart.
        let queen = library
            .get_tracks_by_album("Greatest Hits", Some("Queen"))
            .expect("queen");
        assert_eq!(queen.len(), 2);
        assert!(queen.iter().all(|t| t.artist == "Queen"));
        // Ordered by track_number.
        assert_eq!(queen[0].track_number, Some(1));
        assert_eq!(queen[1].track_number, Some(2));

        let abba = library
            .get_tracks_by_album("Greatest Hits", Some("ABBA"))
            .expect("abba");
        assert_eq!(abba.len(), 1);

        // Without album_artist, both merge into one result set.
        let merged = library
            .get_tracks_by_album("Greatest Hits", None)
            .expect("merged");
        assert_eq!(merged.len(), 3);
    }

    #[test]
    fn get_tracks_by_album_matches_resolved_album_artist_when_tag_is_null() {
        let library = open_test_library().expect("library");
        seed_library_for_browse_tests(&library);

        // "No Album Artist" has a NULL album_artist tag; resolved value is "Solo".
        let tracks = library
            .get_tracks_by_album("No Album Artist", Some("Solo"))
            .expect("tracks");
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].artist, "Solo");
    }

    #[test]
    fn get_tracks_by_album_orders_by_disc_then_track() {
        let library = open_test_library().expect("library");
        {
            let connection = library.lock_connection().expect("connection");
            upsert_many(
                &connection,
                &[
                    track_with("d1", "/m/d1.flac", "X", "Double", Some("X"), Some(1), Some(2), None, None),
                    track_with("d2", "/m/d2.flac", "X", "Double", Some("X"), Some(2), Some(1), None, None),
                    track_with("d3", "/m/d3.flac", "X", "Double", Some("X"), Some(1), Some(1), None, None),
                ],
            );
        }

        let tracks = library
            .get_tracks_by_album("Double", Some("X"))
            .expect("tracks");
        let order: Vec<(Option<i32>, Option<i32>)> = tracks
            .iter()
            .map(|t| (t.disc_number, t.track_number))
            .collect();
        assert_eq!(
            order,
            vec![(Some(1), Some(1)), (Some(1), Some(2)), (Some(2), Some(1))]
        );
    }

    #[test]
    fn get_tracks_by_artist_returns_discography_ordered_by_album_disc_track() {
        let library = open_test_library().expect("library");
        seed_library_for_browse_tests(&library);

        let tracks = library.get_tracks_by_artist("The Beatles").expect("tracks");
        assert_eq!(tracks.len(), 4);
        // Ordered by album name: "Abbey Road" before "Let It Be".
        assert_eq!(tracks[0].album, "Abbey Road");
        assert_eq!(tracks[3].album, "Let It Be");
        // Within Abbey Road: disc 1, tracks 1..3.
        assert_eq!(tracks[0].track_number, Some(1));
        assert_eq!(tracks[1].track_number, Some(2));
        assert_eq!(tracks[2].track_number, Some(3));
    }

    #[test]
    fn get_tracks_by_album_rejects_empty_name() {
        let library = open_test_library().expect("library");
        let err = library
            .get_tracks_by_album("   ", None)
            .expect_err("empty album should fail");
        assert!(err.contains("cannot be empty"));
    }

    #[test]
    fn get_tracks_by_artist_rejects_empty_name() {
        let library = open_test_library().expect("library");
        let err = library
            .get_tracks_by_artist("")
            .expect_err("empty artist should fail");
        assert!(err.contains("cannot be empty"));
    }

    // ── Favorites ───────────────────────────────────────────────────────────

    #[test]
    fn favorites_playlist_is_seeded_and_listed() {
        let library = open_test_library().expect("library");
        let playlists = library.list_playlists(None).expect("playlists");
        let names: Vec<&str> = playlists.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"Favorites"));
        assert!(names.contains(&"All Local Files"));
    }

    #[test]
    fn favorites_id_is_stable_across_calls() {
        let library = open_test_library().expect("library");
        let a = library.favorites_playlist_id().expect("id a");
        let b = library.favorites_playlist_id().expect("id b");
        assert_eq!(a, b);
    }

    #[test]
    fn is_track_in_favorites_reflects_membership() {
        let library = open_test_library().expect("library");
        let favorites_id = library.favorites_playlist_id().expect("favorites");
        let track_path = "/music/fav.mp3";

        assert!(!library.is_track_in_favorites(track_path).expect("absent"));

        {
            let connection = library.lock_connection().expect("connection");
            let track = sample_track("fav-1", track_path);
            let id = upsert_track(&*connection, &track).expect("upsert");
            insert_playlist_track_with_connection(&connection, &favorites_id, &id, 0)
                .expect("insert");
        }

        assert!(library.is_track_in_favorites(track_path).expect("present"));

        library
            .remove_track_from_favorites(track_path)
            .expect("remove");

        assert!(!library.is_track_in_favorites(track_path).expect("removed"));
    }

    #[test]
    fn is_track_in_any_playlist_requires_registered_membership() {
        let library = open_test_library().expect("library");
        let playlist_id = library.default_playlist_id().expect("playlist");
        let track_path = "/music/registered.mp3";

        assert!(!library
            .is_track_in_any_playlist(track_path)
            .expect("absent before insert"));

        {
            let connection = library.lock_connection().expect("connection");
            let track = sample_track("reg-1", track_path);
            let id = upsert_track(&*connection, &track).expect("upsert");
            insert_playlist_track_with_connection(&connection, &playlist_id, &id, 0)
                .expect("insert");
        }

        assert!(library
            .is_track_in_any_playlist(track_path)
            .expect("present after insert"));

        library
            .remove_track_from_playlist_by_path(&playlist_id, track_path)
            .expect("remove");

        assert!(!library
            .is_track_in_any_playlist(track_path)
            .expect("absent after remove"));
    }

    #[test]
    fn toggle_favorite_removes_when_present() {
        let library = open_test_library().expect("library");
        let favorites_id = library.favorites_playlist_id().expect("favorites");
        let track_path = "/music/toggle.mp3";

        {
            let connection = library.lock_connection().expect("connection");
            let track = sample_track("tog-1", track_path);
            let id = upsert_track(&*connection, &track).expect("upsert");
            insert_playlist_track_with_connection(&connection, &favorites_id, &id, 0)
                .expect("insert");
        }

        let now_favorited = library.toggle_favorite(track_path).expect("toggle off");
        assert!(!now_favorited);
        assert!(library.get_favorites().expect("favorites").is_empty());
    }

    #[test]
    fn get_favorites_returns_tracks_in_position_order() {
        let library = open_test_library().expect("library");
        let favorites_id = library.favorites_playlist_id().expect("favorites");

        {
            let connection = library.lock_connection().expect("connection");
            let t1 = sample_track("f1", "/music/f1.mp3");
            let t2 = sample_track("f2", "/music/f2.mp3");
            let id1 = upsert_track(&*connection, &t1).expect("upsert 1");
            let id2 = upsert_track(&*connection, &t2).expect("upsert 2");
            insert_playlist_track_with_connection(&connection, &favorites_id, &id1, 0)
                .expect("insert 1");
            insert_playlist_track_with_connection(&connection, &favorites_id, &id2, 1)
                .expect("insert 2");
        }

        let favorites = library.get_favorites().expect("favorites");
        assert_eq!(favorites.len(), 2);
        assert_eq!(favorites[0].path, "/music/f1.mp3");
        assert_eq!(favorites[1].path, "/music/f2.mp3");
    }

    #[test]
    fn clear_favorites_removes_all() {
        let library = open_test_library().expect("library");
        let favorites_id = library.favorites_playlist_id().expect("favorites");

        {
            let connection = library.lock_connection().expect("connection");
            let t = sample_track("cf-1", "/music/cf.mp3");
            let id = upsert_track(&*connection, &t).expect("upsert");
            insert_playlist_track_with_connection(&connection, &favorites_id, &id, 0)
                .expect("insert");
        }

        assert_eq!(library.get_favorites().expect("favorites").len(), 1);
        library.clear_favorites().expect("clear");
        assert!(library.get_favorites().expect("favorites").is_empty());
    }

    #[test]
    fn delete_playlist_rejects_favorites() {
        let library = open_test_library().expect("library");
        let favorites_id = library.favorites_playlist_id().expect("favorites");
        let err = library
            .delete_playlist(&favorites_id)
            .expect_err("should not delete favorites");
        assert!(err.contains("cannot be deleted"));
    }

    #[test]
    fn rename_playlist_rejects_favorites() {
        let library = open_test_library().expect("library");
        let favorites_id = library.favorites_playlist_id().expect("favorites");
        let err = library
            .rename_playlist(&favorites_id, "My Songs")
            .expect_err("should not rename favorites");
        assert!(err.contains("cannot be renamed"));
    }

    #[test]
    fn add_track_to_playlist_reuses_existing_track_without_extraction() {
        let library = open_test_library().expect("library");
        let favorites_id = library.favorites_playlist_id().expect("favorites");
        let default_id = library.default_playlist_id().expect("default");
        let track_path = "/music/already-indexed.mp3";

        let original_track = sample_track("seeded-id", track_path);
        {
            let connection = library.lock_connection().expect("connection");
            let id = upsert_track(&*connection, &original_track).expect("upsert");
            assert_eq!(id, "seeded-id");
            // Add it to the default playlist first.
            insert_playlist_track_with_connection(&connection, &default_id, &id, 0)
                .expect("insert to default");
        }

        let added = library
            .add_track_to_playlist(&favorites_id, track_path.to_string())
            .expect("add existing track to favorites");

        assert_eq!(added.id, "seeded-id");
        assert_eq!(added.title, original_track.title);
        assert_eq!(added.artist, original_track.artist);

        let favorites = library.get_favorites().expect("favorites");
        assert_eq!(favorites.len(), 1);
        assert_eq!(favorites[0].path, track_path);
    }
}
