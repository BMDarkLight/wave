# Shared types

JSON field names match what the backend serializes (snake_case). TypeScript interfaces in `src/utils/player.ts` mirror these types.

---

## `PlaybackState`

Returned by `get_playback_state`.

```typescript
interface PlaybackState {
  is_playing: boolean;
  is_paused: boolean;
  current_path: string | null;
  position_seconds: number;
  duration_seconds: number | null;
  volume: number; // 0.0 – 1.0
}
```

| Field | Description |
|-------|-------------|
| `is_playing` | `true` when audio is actively playing |
| `is_paused` | `true` when a track is loaded but paused |
| `current_path` | Absolute path of the loaded file, or `null` |
| `position_seconds` | Current playback head position |
| `duration_seconds` | Total track length when known, else `null` |
| `volume` | Current output volume |

---

## `QueueState`

Returned by `get_queue`.

```typescript
interface QueueState {
  tracks: string[];           // absolute file paths, in queue order
  current_index: number | null;
  is_shuffled: boolean;
}
```

| Field | Description |
|-------|-------------|
| `tracks` | Ordered list of paths in the in-memory queue |
| `current_index` | Zero-based index into `tracks` for the current song |
| `is_shuffled` | Whether a shuffle permutation is active |

---

## `PlaybackMode`

Returned by `get_playback_mode`.

```typescript
interface PlaybackMode {
  repeat: "off" | "one" | "all";
  shuffle: boolean;
}
```

| `repeat` value | Behavior |
|----------------|----------|
| `"off"` | Stop at end of queue |
| `"one"` | Repeat current track |
| `"all"` | Wrap around the queue |

---

## `Track`

Rich metadata for a library item. Returned by `add_track_to_playlist`, `get_playlist`, and `index_music_library`.

```typescript
interface Track {
  id: string;
  path: string;
  name: string;
  title: string;
  artist: string;
  album: string;
  album_artist: string | null;
  genre: string | null;
  year: number | null;
  track_number: number | null;
  disc_number: number | null;
  format: string;              // uppercase extension, e.g. "FLAC"
  duration_seconds: number | null;
  sample_rate: number | null;
  channels: number | null;
  bit_depth: number | null;
  lyrics: string | null;
  lyrics_source: string | null; // e.g. "embedded-or-sidecar", "lrclib"
  cover_art_data_url: string | null;  // data: URL or remote URL
  cover_art_mime: string | null;
  cover_art_source: string | null;    // e.g. "embedded", "cover-art-archive"
  fingerprint_sha256: string | null;
  acoustid_fingerprint: string | null;
  musicbrainz_recording_id: string | null;
  file_size: number;
  modified_at: number;   // Unix timestamp (seconds)
  indexed_at: number;    // Unix timestamp (seconds)
}
```

### Metadata notes for UI

- **Title / artist / album** fall back to filename and folder name when tags are missing.
- **Cover art** may be an embedded `data:` URL or a Cover Art Archive HTTPS URL.
- **Lyrics** may come from embedded tags, a sidecar `.lrc`/`.txt` file, or LRCLib online lookup during indexing.
- **`path`** is the stable key for remove/play operations in the default playlist.

---

## `PlaylistInfo`

Returned by `list_playlists`.

```typescript
interface PlaylistInfo {
  id: string;
  profile_id: string;
  name: string;
  track_count: number;
  created_at: number;  // Unix timestamp (seconds)
  updated_at: number;
}
```

---

## `AlbumSummary`

Returned by `list_albums`. One entry per distinct album, grouped by
`(album, album_artist)` — so two unrelated albums that happen to share a name
(e.g. several “Greatest Hits”) appear as separate entries.

```typescript
interface AlbumSummary {
  name: string;
  album_artist: string | null;  // resolved: tag album_artist, else track artist
  artist: string;               // representative track artist
  track_count: number;
  year: number | null;
  cover_art_data_url: string | null; // representative cover (data: URL or https URL)
  cover_art_mime: string | null;
}
```

| Field | Description |
|-------|-------------|
| `name` | Album title (from tags) |
| `album_artist` | The tag `album_artist` when present, otherwise the track `artist`. Pass this back to `get_album_tracks` for a precise “go to album” lookup |
| `artist` | A representative track artist for the album |
| `track_count` | Number of tracks in the album |
| `year` | Earliest year found on the album’s tracks, or `null` |
| `cover_art_data_url` | Representative cover art (a `data:` URL or Cover Art Archive URL) |
| `cover_art_mime` | MIME type of the representative cover, or `null` |

---

## `ArtistSummary`

Returned by `list_artists`. One entry per distinct track `artist` tag.

```typescript
interface ArtistSummary {
  name: string;
  track_count: number;
  album_count: number;  // distinct album names attributed to the artist
}
```

---

## `MediaMetadata`

Argument to `update_media_metadata`. All fields are optional.

```typescript
interface MediaMetadata {
  title?: string | null;
  artist?: string | null;
  album?: string | null;
  duration_seconds?: number | null;
  cover_url?: string | null;  // data: URL or https:// URL
}
```

Use the same values you show in the in-app now-playing UI. For cover art, prefer `Track.cover_art_data_url` mapped to `cover_url`.

---

## `EqSettings`

Returned by `get_eq_settings`.

```typescript
interface EqSettings {
  bands: number[];     // 10 gains in dB
  enabled: boolean;
}
```

| Field | Description |
|-------|-------------|
| `bands` | 10-element array of gains in dB, one per ISO band (31, 62, 125, 250, 500, 1000, 2000, 4000, 8000, 16000 Hz). Range typically –12 to +12 dB |
| `enabled` | Whether the EQ chain is active. When `false` audio passes through unprocessed |

---

## Copy-paste module

You can import types from the existing frontend wrapper:

```typescript
import type {
  PlaybackState,
  QueueState,
  PlaybackMode,
  Track,
  PlaylistInfo,
  AlbumSummary,
  ArtistSummary,
  MediaMetadata,
} from "../utils/player";
```

Or duplicate the interfaces above in a shared `src/types/backend.ts` if you split the API layer later.
