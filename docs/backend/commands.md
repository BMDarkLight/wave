# Command reference

All commands are registered in `src-tauri/src/lib.rs` and implemented in `src-tauri/src/commands.rs`.

Unless noted, argument names below use **JavaScript camelCase** (Tauri maps Rust `snake_case` parameters automatically). Response field names use **snake_case** as serialized by Serde.

---

## Playback

### `play_track`

Start playing an audio file by absolute path. Does **not** update the playback queue.

**Arguments**

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `path` | `string` | yes | Absolute file path |

**Returns:** `void`

**Example**

```typescript
await invoke("play_track", { path: "D:\\Music\\album\\01.flac" });
```

**Errors (examples):** file open/decode failures, unsupported format.

---

### `pause_track`

Pause the currently playing track. Preserves playback position.

**Arguments:** none

**Returns:** `void`

**Errors:** `"No track is currently playing"` when nothing is loaded.

---

### `resume_track`

Resume after `pause_track`.

**Arguments:** none

**Returns:** `void`

**Errors:** `"No track is currently playing"` when nothing is loaded.

---

### `stop_track`

Stop playback and clear the loaded track.

**Arguments:** none

**Returns:** `void`

---

### `get_playback_state`

Returns a snapshot of the audio engine state. Safe to poll frequently.

**Arguments:** none

**Returns:** [`PlaybackState`](./types.md#playbackstate)

**Example**

```typescript
const state = await invoke<PlaybackState>("get_playback_state");
// state.is_playing, state.position_seconds, ...
```

---

### `seek_track`

Seek to a position within the current track. Uses Rodio’s native seek (no full re-decode).

**Arguments**

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `seconds` | `number` | yes | Target position (≥ 0) |

**Returns:** `void`

**Errors:** `"No track loaded"`, seek failures for the current format.

---

### `set_volume`

Set output volume.

**Arguments**

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `volume` | `number` | yes | Linear volume from `0.0` (mute) to `1.0` (full) |

**Returns:** `void`

**Errors:** `"Volume must be between 0.0 and 1.0"` if out of range.

---

## Library & playlists

The default profile is `"default"` and the default playlist is `"Local Sessions"`. Most library commands operate on that playlist unless noted.

### `add_track_to_playlist`

Read metadata from disk, upsert into SQLite, and append to the default playlist.

**Arguments**

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `path` | `string` | yes | Absolute path to an audio file |

**Returns:** [`Track`](./types.md#track) (with extracted metadata)

**Errors:** file missing, unsupported extension, `"Track is already in the playlist"`.

---

### `remove_track_from_playlist`

Remove a track from the default playlist by file path. Does not delete the file from disk.

**Arguments**

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `path` | `string` | yes | Absolute file path |

**Returns:** `void`

**Errors:** `"Track is not in the playlist"`.

---

### `get_playlist`

Load all tracks from the default persisted playlist, ordered by position.

**Arguments:** none

**Returns:** `Track[]`

---

### `clear_playlist`

Remove every track entry from the default playlist. Does not delete files or track records from the library database.

**Arguments:** none

**Returns:** `void`

---

### `play_track_from_playlist`

Play the track at a **zero-based index** in the default playlist. Also:

1. Loads the full playlist into the in-memory playback queue
2. Sets the queue’s current index
3. Pushes metadata to OS media controls

**Prefer this over `play_track`** when the user selects a song from the library UI.

**Arguments**

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `index` | `number` | yes | Zero-based index in the default playlist |

**Returns:** `void`

**Errors:** `"Track not found at index {n}"`, audio playback errors.

---

### `index_music_library`

Recursively scan a directory for supported audio files, extract metadata, and import into a playlist.

**Arguments**

| Name | Type | Required | Default | Description |
|------|------|----------|---------|-------------|
| `directory` | `string` | yes | — | Folder to scan |
| `profileId` | `string` | no | `"default"` | Profile id |
| `playlistName` | `string` | no | `"Local Sessions"` | Target playlist name |

**Returns:** `Track[]` — only tracks **newly added** to the playlist during this scan (skipped/duplicate files are omitted)

**Errors:** `"Library path is not a directory"`. Individual file failures are logged server-side and skipped.

---

### `list_playlists`

**Arguments**

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `profileId` | `string` | no | Filter to a single profile; omit for all profiles |

**Returns:** [`PlaylistInfo[]`](./types.md#playlistinfo)

---

### `get_library_database_path`

**Arguments:** none

**Returns:** `string` — absolute path to `wave-library.sqlite` in the app data directory

---

### `get_supported_audio_extensions`

**Arguments:** none

**Returns:** `string[]` — lowercase extensions without dots

```
aac, aiff, alac, caf, flac, m4a, m4b, m4p, mka, mkv, mp1, mp2,
mp3, mp4, oga, ogg, opus, wav, wave, weba
```

---

## Albums & artists

These commands let the frontend build Spotify-style browse views and “go to
album” / “go to artist” flows straight from file metadata — no playlists
required.

Albums are grouped by `(album, album_artist)` (falling back to the track
`artist` when the `album_artist` tag is missing), so same-named albums by
different artists stay separate. Artists are grouped by the track `artist`
tag.

### `list_albums`

List every distinct album in the library with aggregate info. Use this to
render an album grid.

**Arguments:** none

**Returns:** [`AlbumSummary[]`](./types.md#albumsummary)

```typescript
const albums = await invoke<AlbumSummary[]>("list_albums");
// albums[0] => { name: "Abbey Road", album_artist: "The Beatles",
//                track_count: 3, year: 1969, cover_art_data_url: "data:..." }
```

---

### `list_artists`

List every distinct artist with track and album counts. Use this to render an
artist list / discography index.

**Arguments:** none

**Returns:** [`ArtistSummary[]`](./types.md#artistsummary)

```typescript
const artists = await invoke<ArtistSummary[]>("list_artists");
// artists[0] => { name: "The Beatles", track_count: 12, album_count: 3 }
```

---

### `get_album_tracks`

Return every track belonging to an album — the backend of the “right-click a
song → go to album” flow.

**Arguments**

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `album` | `string` | yes | Album name (matches the track `album` tag) |
| `albumArtist` | `string` \| null | no | Resolved album artist. Pass `Track.album_artist ?? Track.artist` (or the `AlbumSummary.album_artist` value) to keep same-named albums apart. Omit to match the album name only |

**Returns:** `Track[]` — ordered by disc number then track number.

**Errors:** `"Album name cannot be empty"`.

```typescript
// From a clicked track:
const tracks = await invoke<Track[]>("get_album_tracks", {
  album: track.album,
  albumArtist: track.album_artist ?? track.artist,
});
```

When `albumArtist` is omitted, every track whose `album` tag matches is
returned, even across different artists.

---

### `get_artist_tracks`

Return every track by an artist (a discography) — the backend of the
“right-click a song → go to artist” flow. Matches the track `artist` tag.

**Arguments**

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `artist` | `string` | yes | Artist name (matches the track `artist` tag) |

**Returns:** `Track[]` — ordered by album, then disc number, then track number.

**Errors:** `"Artist name cannot be empty"`.

```typescript
const tracks = await invoke<Track[]>("get_artist_tracks", { artist: "The Beatles" });
```

---

### `create_album_playlist`

Create a new persisted playlist from every track matching an album name. Useful
when you want the album as a real playlist (e.g. to export, reorder, or keep).

**Arguments**

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `album` | `string` | yes | Album name |
| `name` | `string` \| null | no | Playlist name; defaults to the album name. Auto-suffixed on collision |

**Returns:** [`PlaylistInfo`](./types.md#playlistinfo)

**Errors:** `"Album name cannot be empty"`, `"No tracks found for album \"{album}\""`.

---

### `create_artist_playlist`

Create a new persisted playlist from every track matching an artist name (a
discography playlist).

**Arguments**

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `artist` | `string` | yes | Artist name |
| `name` | `string` \| null | no | Playlist name; defaults to the artist name. Auto-suffixed on collision |

**Returns:** [`PlaylistInfo`](./types.md#playlistinfo)

**Errors:** `"Artist name cannot be empty"`, `"No tracks found for artist \"{artist}\""`.

> `get_album_tracks` / `get_artist_tracks` are the read-only queries you’ll
> usually want for browse views. The `create_*_playlist` commands persist the
> same result into a playlist you can save/export.

---

## Queue & playback modes

The playback queue is **in-memory** and separate from the SQLite playlist. It is populated automatically when you call `play_track_from_playlist`.

### `get_queue`

**Arguments:** none

**Returns:** [`QueueState`](./types.md#queuestate)

---

### `play_next`

Advance to the next track in the queue.

**Behavior:**

- **`repeat: "one"`** — replays the current track
- **`repeat: "all"`** — wraps to the first track
- **`repeat: "off"`** — returns `null` at end of queue
- Respects shuffle order when enabled

**Arguments:** none

**Returns:** `string | null` — path of the newly playing track, or `null` if queue exhausted

---

### `play_previous`

Go to the previous track, with a common UX rule:

- If **more than 3 seconds** into the current track → seek to start (returns current path)
- Otherwise → previous queue entry (respects shuffle/repeat)

**Arguments:** none

**Returns:** `string | null` — path of the track now playing, or `null`

---

### `set_shuffle`

**Arguments**

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `enabled` | `boolean` | yes | Turn shuffle on or off |

**Returns:** `void`

When enabled, rebuilds a shuffle order with the current track kept first.

---

### `set_repeat`

**Arguments**

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `mode` | `string` | yes | `"off"`, `"one"`, or `"all"` |

**Returns:** `void`

**Errors:** `"Invalid repeat mode: {mode}"` for unknown values.

---

### `get_playback_mode`

**Arguments:** none

**Returns:** [`PlaybackMode`](./types.md#playbackmode)

---

## OS media controls

### `update_media_metadata`

Push now-playing metadata to the OS media surface (Windows SMTC, macOS Control Center, Linux MPRIS).

Call when the displayed track changes or when playback starts, so the system UI shows title/artist/album/artwork.

**Arguments**

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `metadata` | [`MediaMetadata`](./types.md#mediametadata) | yes | Track info for the OS |

**Returns:** `void`

**Example**

```typescript
await invoke("update_media_metadata", {
  metadata: {
    title: "Song Title",
    artist: "Artist Name",
    album: "Album Name",
    duration_seconds: 240,
    cover_url: "data:image/jpeg;base64,...",
  },
});
```

If OS media controls failed to initialize at startup, this command succeeds silently (no-op).

See also [Events](./events.md) for inbound OS button presses.
