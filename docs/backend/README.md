# Wave Backend API

This folder documents the **Tauri invoke commands** and **events** exposed by the Rust backend in `src-tauri/`. Use it when building or extending the React frontend.

## Quick start

All backend commands are called with Tauri’s `invoke` API:

```typescript
import { invoke } from "@tauri-apps/api/core";

const state = await invoke<PlaybackState>("get_playback_state");
await invoke("play_track", { path: "C:\\Music\\song.mp3" });
```

A typed wrapper already lives in [`src/utils/player.ts`](../../src/utils/player.ts). Prefer importing from there instead of calling `invoke` directly.

### Running the app

Commands only work inside the Tauri desktop window. Use:

```bash
npm run tauri dev
```

Opening the plain Vite URL in a browser will not have access to the backend.

## Architecture (mental model)

Wave separates three layers:

| Layer | Storage | Purpose |
|-------|---------|---------|
| **Library playlist** | SQLite (`wave-library.sqlite`) | Persisted track list for the default “Local Sessions” playlist |
| **Playback queue** | In-memory (Rust) | Order used by `play_next`, `play_previous`, shuffle, and repeat |
| **Audio engine** | In-memory (Rodio + Symphonia) | Actual decode/play/pause/seek/volume |

Important behaviors:

- **`get_playlist`** returns the persisted library playlist (rich `Track` metadata).
- **`get_queue`** returns the in-memory playback queue (file paths only).
- **`play_track_from_playlist`** loads the library playlist into the playback queue, sets the current index, and starts playback. Call this (not `play_track`) when the user picks a song from the library UI so next/previous/shuffle/repeat work correctly.
- **`play_track`** plays a single file by path without updating the queue.

## Documentation index

| Document | Contents |
|----------|----------|
| [Commands](./commands.md) | Full reference for every `invoke` command |
| [Types](./types.md) | Shared request/response shapes (TypeScript-friendly) |
| [Events](./events.md) | OS media control events emitted by the backend |

## Command summary

### Playback

| Command | Description |
|---------|-------------|
| `play_track` | Play a file by absolute path |
| `pause_track` | Pause current playback |
| `resume_track` | Resume paused playback |
| `stop_track` | Stop and unload current track |
| `get_playback_state` | Poll playing/paused state, position, duration, volume |
| `seek_track` | Seek to position in seconds |
| `set_volume` | Set volume (`0.0`–`1.0`) |

### Library & playlists

| Command | Description |
|---------|-------------|
| `add_track_to_playlist` | Extract metadata and add file to default playlist |
| `remove_track_from_playlist` | Remove file from default playlist by path |
| `get_playlist` | List all tracks in the default playlist |
| `clear_playlist` | Remove all tracks from the default playlist |
| `play_track_from_playlist` | Play track at index and sync playback queue |
| `index_music_library` | Scan a folder and import audio files |
| `list_playlists` | List playlists (optionally filtered by profile) |
| `get_library_database_path` | Absolute path to the SQLite database |
| `get_supported_audio_extensions` | Lowercase extensions the backend accepts |

### Queue & playback modes

| Command | Description |
|---------|-------------|
| `get_queue` | Current in-memory queue state |
| `play_next` | Advance queue (respects repeat/shuffle) |
| `play_previous` | Go back (rewinds if >3s into track) |
| `set_shuffle` | Enable/disable shuffle order |
| `set_repeat` | Set repeat mode: `"off"`, `"one"`, or `"all"` |
| `get_playback_mode` | Current repeat and shuffle settings |

### Albums & artists

Browse and query the library by album/artist metadata (for Spotify-like album
grids, “go to album”, and discography views). See [Commands → Albums &
artists](./commands.md#albums--artists).

| Command | Description |
|---------|-------------|
| `list_albums` | List every distinct album (grouped by album + album artist) |
| `list_artists` | List every distinct artist with track/album counts |
| `get_album_tracks` | Every track in an album (right-click → go to album) |
| `get_artist_tracks` | Every track by an artist (discography) |
| `create_album_playlist` | Persist an album’s tracks as a playlist |
| `create_artist_playlist` | Persist an artist’s tracks as a playlist |

### OS integration

| Command | Description |
|---------|-------------|
| `update_media_metadata` | Push now-playing info to system media UI |

## Error handling

Commands return `Promise<T>` on success. Failures reject with a **string** error message from the backend (for example `"No track is currently playing"` or `"Track is already in the playlist"`).

```typescript
try {
  await invoke("pause_track");
} catch (error) {
  // error is a string
  console.error(error);
}
```

## Recommended UI polling

The current frontend polls playback state every 500ms while the app is open:

```typescript
const state = await invoke<PlaybackState>("get_playback_state");
```

After any mutating command (`play_track`, `seek_track`, etc.), refresh state immediately rather than waiting for the next poll.

## File picker (not a backend command)

File selection uses the **Tauri dialog plugin**, not a Rust command. See `selectAudioFile` in `src/utils/player.ts`.

Supported extensions match `get_supported_audio_extensions`.
