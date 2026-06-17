// Use dynamic imports to handle cases where Tauri API might not be available
let invokeFn: typeof import("@tauri-apps/api/core").invoke;
let openFn: typeof import("@tauri-apps/plugin-dialog").open;

const initTauri = async () => {
  try {
    const core = await import("@tauri-apps/api/core");
    const dialog = await import("@tauri-apps/plugin-dialog");
    invokeFn = core.invoke;
    openFn = dialog.open;
    console.log("Tauri APIs initialized");
    return true;
  } catch (error) {
    console.error("Failed to initialize Tauri APIs:", error);
    return false;
  }
};

let tauriInitialized = initTauri();

const safeInvoke = async <T = any>(cmd: string, args?: Record<string, unknown>): Promise<T> => {
  await tauriInitialized;

  if (!invokeFn) {
    throw new Error("Tauri API is not available. Make sure you're running 'npm run tauri dev' (not just 'npm run dev')");
  }

  return await invokeFn<T>(cmd, args);
};

export interface PlaybackState {
  is_playing: boolean;
  is_paused: boolean;
  current_path: string | null;
  position_seconds: number;
  duration_seconds: number | null;
  volume: number;
}

export interface Track {
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
  format: string;
  duration_seconds: number | null;
  sample_rate: number | null;
  channels: number | null;
  bit_depth: number | null;
  file_size: number;
  modified_at: number;
  indexed_at: number;
}

export interface PlaylistInfo {
  id: string;
  profile_id: string;
  name: string;
  track_count: number;
  created_at: number;
  updated_at: number;
}

export const playTrack = (path: string): Promise<void> => {
  return safeInvoke("play_track", { path });
};

export const pauseTrack = (): Promise<void> => {
  return safeInvoke("pause_track");
};

export const resumeTrack = (): Promise<void> => {
  return safeInvoke("resume_track");
};

export const stopTrack = (): Promise<void> => {
  return safeInvoke("stop_track");
};

export const seekTrack = (seconds: number): Promise<void> => {
  return safeInvoke("seek_track", { seconds });
};

export const setPlayerVolume = (volume: number): Promise<void> => {
  return safeInvoke("set_volume", { volume });
};

export const getPlaybackState = (): Promise<PlaybackState> => {
  return safeInvoke<PlaybackState>("get_playback_state");
};

export const selectAudioFile = async (multiple: boolean = false): Promise<string[] | null> => {
  await tauriInitialized;

  if (!openFn) {
    throw new Error("Tauri API is not available. Make sure you're running 'npm run tauri dev' (not just 'npm run dev')");
  }

  const selected = await openFn({
    multiple,
    filters: [
      {
        name: "Audio",
        extensions: [
          "aac", "aiff", "alac", "caf", "flac", "m4a", "m4b", "m4p", "mka", "mkv",
          "mp1", "mp2", "mp3", "mp4", "oga", "ogg", "opus", "wav", "wave", "weba",
        ],
      },
    ],
    title: multiple ? "Select Audio Files" : "Select Audio File",
  });

  if (selected === null) return null;
  if (Array.isArray(selected)) return selected;
  if (typeof selected === "string") return [selected];
  return null;
};

export const getFileName = (path: string | null): string => {
  if (!path) return "No track selected";
  const parts = path.split(/[/\\]/);
  return parts[parts.length - 1] || "Unknown";
};

export const addTrackToPlaylist = (path: string): Promise<Track> => {
  return safeInvoke<Track>("add_track_to_playlist", { path });
};

export const removeTrackFromPlaylist = (index: number): Promise<void> => {
  return safeInvoke("remove_track_from_playlist", { index });
};

export const getPlaylist = (): Promise<Track[]> => {
  return safeInvoke<Track[]>("get_playlist");
};

export const clearPlaylist = (): Promise<void> => {
  return safeInvoke("clear_playlist");
};

export const playTrackFromPlaylist = (index: number): Promise<void> => {
  return safeInvoke("play_track_from_playlist", { index });
};

export const indexMusicLibrary = (
  directory: string,
  profileId?: string,
  playlistName?: string
): Promise<Track[]> => {
  return safeInvoke<Track[]>("index_music_library", {
    directory,
    profileId,
    playlistName,
  });
};

export const listPlaylists = (profileId?: string): Promise<PlaylistInfo[]> => {
  return safeInvoke<PlaylistInfo[]>("list_playlists", { profileId });
};

export const getLibraryDatabasePath = (): Promise<string> => {
  return safeInvoke<string>("get_library_database_path");
};

export const getSupportedAudioExtensions = (): Promise<string[]> => {
  return safeInvoke<string[]>("get_supported_audio_extensions");
};

// ── OS Media Controls ─────────────────────────────────────────────────────────

export interface MediaMetadata {
  title?: string | null;
  artist?: string | null;
  album?: string | null;
  duration_seconds?: number | null;
  cover_url?: string | null;
}

/**
 * Push track metadata to the OS media interface (macOS Control Center,
 * Windows SMTC, Linux MPRIS).  Call this whenever the playing track changes.
 */
export const updateMediaMetadata = (metadata: MediaMetadata): Promise<void> => {
  return safeInvoke("update_media_metadata", { metadata });
};

/**
 * Listen for OS media control events (play, pause, next, previous, seek).
 * Returns an unlisten function — call it when your component unmounts.
 *
 * Usage:
 *   const unlisten = await listenToMediaControls({ onPlay, onPause, onNext, ... });
 *   // later:
 *   unlisten();
 */
export interface MediaControlHandlers {
  onPlay?: () => void;
  onPause?: () => void;
  onToggle?: () => void;
  onNext?: () => void;
  onPrevious?: () => void;
  onStop?: () => void;
  onSeekRelative?: (direction: "forward" | "backward") => void;
  onSeekBy?: (seconds: number) => void;
  onSetPosition?: (seconds: number) => void;
}

export const listenToMediaControls = async (
  handlers: MediaControlHandlers
): Promise<() => void> => {
  await tauriInitialized;
  const { listen } = await import("@tauri-apps/api/event");

  const unlisteners = await Promise.all([
    handlers.onPlay     && listen("media-control-play",     () => handlers.onPlay!()),
    handlers.onPause    && listen("media-control-pause",    () => handlers.onPause!()),
    handlers.onToggle   && listen("media-control-toggle",   () => handlers.onToggle!()),
    handlers.onNext     && listen("media-control-next",     () => handlers.onNext!()),
    handlers.onPrevious && listen("media-control-previous", () => handlers.onPrevious!()),
    handlers.onStop     && listen("media-control-stop",     () => handlers.onStop!()),
    handlers.onSeekRelative && listen<string>(
      "media-control-seek-relative",
      (e) => handlers.onSeekRelative!(e.payload as "forward" | "backward")
    ),
    handlers.onSeekBy && listen<number>(
      "media-control-seek-by",
      (e) => handlers.onSeekBy!(e.payload)
    ),
    handlers.onSetPosition && listen<number>(
      "media-control-set-position",
      (e) => handlers.onSetPosition!(e.payload)
    ),
  ]);

  return () => {
    unlisteners.forEach((u) => u && u());
  };
};
