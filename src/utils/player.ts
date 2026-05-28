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
  path: string;
  name: string;
  title: string;
  artist: string;
  album: string;
  format: string;
  duration_seconds: number | null;
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
        extensions: ["mp3", "wav", "flac", "aac", "ogg", "m4a", "opus", "mka"],
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
