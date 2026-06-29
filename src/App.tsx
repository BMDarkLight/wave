// The Code for Frontend of Wave is currently completely AI Generated and may contain bugs or rough edges. Please report any issues you encounter at

import { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import {
  BiShuffle,
  BiPlay,
  BiPause,
  BiStop,
  BiSkipPrevious,
  BiSkipNext,
  BiRepeat,
  BiVolumeLow,
  BiVolumeFull,
  BiVolumeMute,
  BiHeart,
  BiSolidHeart,
  BiDotsHorizontalRounded,
  BiX,
  BiPlus,
  BiImport,
  BiExport,
  BiEditAlt,
  BiTrash,
  BiMusic,
  BiListPlus,
  BiListUl,
} from "react-icons/bi";
import {
  addTrackToPlaylistById,
  addToQueue,
  clearPlaylistById,
  clearQueue,
  createPlaylist,
  deletePlaylist,
  exportPlaylist,
  getFileName,
  getFavorites,
  getPlaybackMode,
  getPlaybackState,
  getPlaylistTracksById,
  getQueueTracks,
  importPlaylist,
  isTrackInPlaylist,
  listPlaylists,
  listenToMediaControls,
  openPlaylistDialog,
  pauseTrack,
  playNext,
  playPrevious,
  playTrack,
  playTrackFromQueue,
  playTrackFromSpecificPlaylist,
  queueInsertNext,
  removeTrackFromPlaylistById,
  removeFromQueue,
  renamePlaylist,
  resumeTrack,
  savePlaylistDialog,
  seekTrack,
  selectAudioFile,
  setPlayerVolume,
  setRepeat,
  setShuffle,
  stopTrack,
  toggleFavorite,
  updateMediaMetadata,
  updateMediaPosition,
  listOutputDevices,
  setOutputDevice,
  type PlaybackMode,
  type PlaybackState,
  type PlaylistInfo,
  type QueueTrackState,
  type Track,
} from "./utils/player";
import "./App.css";

function formatInvokeError(err: unknown, fallback: string): string {
  if (err instanceof Error) return err.message;
  if (typeof err === "string" && err.trim()) return err;
  return fallback;
}

const emptyPlaybackState: PlaybackState = {
  is_playing: false,
  is_paused: false,
  current_path: null,
  position_seconds: 0,
  duration_seconds: null,
  volume: 0.8,
  output_device_name: "",
};

const formatTime = (seconds?: number | null) => {
  if (!seconds || !Number.isFinite(seconds)) return "0:00";
  const minutes = Math.floor(seconds / 60);
  const remaining = Math.floor(seconds % 60).toString().padStart(2, "0");
  return `${minutes}:${remaining}`;
};

const getTrackTitle = (track?: Track | null, fallbackPath?: string | null) => {
  if (track?.title) return track.title;
  if (track?.name) return track.name;
  return fallbackPath ? getFileName(fallbackPath) : "Choose a song";
};

const Artwork = ({
  track,
  fallback,
  className,
}: {
  track?: Track | null;
  fallback: string;
  className: string;
}) => {
  if (track?.cover_art_data_url) {
    return (
      <img
        className={className}
        src={track.cover_art_data_url}
        alt={`${getTrackTitle(track)} cover`}
        draggable={false}
      />
    );
  }

  return <div className={className}>{fallback}</div>;
};

function App() {
  const [playbackState, setPlaybackState] = useState<PlaybackState>(emptyPlaybackState);
  const [playlist, setPlaylist] = useState<Track[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [isAddingTracks, setIsAddingTracks] = useState(false);
  const [seekValue, setSeekValue] = useState(0);
  const [volumeValue, setVolumeValue] = useState(0.8);

  // Playlist management
  const [playlists, setPlaylists] = useState<PlaylistInfo[]>([]);
  const [selectedPlaylistId, setSelectedPlaylistId] = useState<string | null>(null);

  // Favorited track paths (for heart toggle state in the track list)
  const [favoritePaths, setFavoritePaths] = useState<Set<string>>(new Set());

  // Clear-playlist confirmation modal
  const [showClearConfirm, setShowClearConfirm] = useState(false);

  // Playback mode
  const [playbackMode, setPlaybackMode] = useState<PlaybackMode>({ repeat: "off", shuffle: false });

  // Queue panel
  const [queueData, setQueueData] = useState<QueueTrackState>({ tracks: [], current_index: null, is_shuffled: false });
  const [showQueue, setShowQueue] = useState(false);

  // Audio output device selection
  const [outputDevices, setOutputDevices] = useState<string[]>([]);
  const [showDeviceList, setShowDeviceList] = useState(false);

  // Track context menu
  const [menuTrackPath, setMenuTrackPath] = useState<string | null>(null);
  const [menuAnchor, setMenuAnchor] = useState<{ top: number; right: number } | null>(null);
  const [showAddToPlaylist, setShowAddToPlaylist] = useState(false);

  // Create / rename playlist dialog
  const [playlistDialog, setPlaylistDialog] = useState<
    { mode: "create" } | { mode: "rename"; playlistId: string; currentName: string } | null
  >(null);
  const [playlistNameInput, setPlaylistNameInput] = useState("");
  const [playlistDialogError, setPlaylistDialogError] = useState<string | null>(null);
  const playlistNameInputRef = useRef<HTMLInputElement>(null);

  const wasPlayingRef = useRef(false);

  const currentTrack = useMemo(() => {
    if (!playbackState.current_path) return null;
    const fromQueue = queueData.tracks.find((track) => track.path === playbackState.current_path);
    if (fromQueue) return fromQueue;
    const fromPlaylist = playlist.find((track) => track.path === playbackState.current_path);
    return fromPlaylist ?? null;
  }, [playbackState.current_path, queueData.tracks, playlist]);

  const currentPlaylistIndex = useMemo(
    () => playlist.findIndex((track) => track.path === playbackState.current_path),
    [playlist, playbackState.current_path]
  );

  const hasActiveQueue = queueData.tracks.length > 0;
  const canSkip = hasActiveQueue || playlist.length > 0;
  const displayDuration = playbackState.duration_seconds ?? currentTrack?.duration_seconds ?? 0;
  const displayPosition = Math.min(seekValue, displayDuration || seekValue);

  const selectedPlaylist = playlists.find((p) => p.id === selectedPlaylistId) ?? null;

  const updatePlaybackState = async () => {
    const state = await getPlaybackState();
    setPlaybackState({ ...emptyPlaybackState, ...state });
    setVolumeValue(state.volume ?? 0.8);
    if (!document.body.classList.contains("is-seeking")) {
      setSeekValue(state.position_seconds ?? 0);
    }
    // Keep the OS media controls position in sync during playback.
    if (state.is_playing) {
      updateMediaPosition(state.position_seconds, true).catch(console.error);
    }
  };

  const loadPlaylists = async () => {
    const list = await listPlaylists();
    setPlaylists(list);
    return list;
  };

  const loadPlaylistTracks = async (playlistId: string) => {
    const tracks = await getPlaylistTracksById(playlistId);
    setPlaylist(tracks);
    await loadFavoritePaths();
  };

  const loadPlaybackMode = async () => {
    try {
      const mode = await getPlaybackMode();
      setPlaybackMode(mode);
    } catch { /* ignore */ }
  };

  const loadQueueTracks = async () => {
    const data = await getQueueTracks();
    setQueueData(data);
  };

  // Refresh the set of favorited track paths (drives heart toggle state).
  const loadFavoritePaths = async () => {
    try {
      const favorites = await getFavorites();
      setFavoritePaths(new Set(favorites.map((t) => t.path)));
    } catch (err) {
      // Loading favorites is best-effort; don't surface hard errors for the heart UI.
      console.warn("Failed to load favorites:", err);
    }
  };

  // Resolve the default playlist ID from the playlists list.
  const getDefaultPlaylistId = (list: PlaylistInfo[]): string | null => {
    return (list.find((p) => p.name === "All Local Files") ?? list[0])?.id ?? null;
  };

  useEffect(() => {
    const initApp = async () => {
      await new Promise((resolve) => setTimeout(resolve, 300));
      try {
        const list = await loadPlaylists();
        const defaultId = getDefaultPlaylistId(list);
        if (defaultId) {
          setSelectedPlaylistId(defaultId);
          await loadPlaylistTracks(defaultId);
        }
        await updatePlaybackState();
        await loadQueueTracks();
        await loadPlaybackMode();
        await loadFavoritePaths();
        listOutputDevices().then(setOutputDevices).catch(console.error);
      } catch (err: any) {
        if (err?.message?.includes("not available") || err?.message?.includes("undefined")) {
          setError("Tauri API not available. Run `npm run tauri dev` instead of plain Vite.");
        }
      }
    };

    initApp();
    const interval = setInterval(() => updatePlaybackState().catch(() => {}), 500);
    const queueInterval = setInterval(() => loadQueueTracks().catch(() => {}), 2000);
    const modeInterval = setInterval(() => loadPlaybackMode().catch(() => {}), 2000);
    return () => {
      clearInterval(interval);
      clearInterval(queueInterval);
      clearInterval(modeInterval);
    };
  }, []);

  // Auto-advance when a track finishes naturally.
  // Falls back to the selected playlist if the queue is exhausted.
  useEffect(() => {
    const wasPlaying = wasPlayingRef.current;
    const { is_playing, is_paused, current_path, position_seconds, duration_seconds } = playbackState;

    if (
      wasPlaying &&
      !is_playing &&
      !is_paused &&
      current_path &&
      duration_seconds != null &&
      position_seconds >= duration_seconds - 1
    ) {
      (async () => {
        const path = await playNext();
        if (!path) {
          const mode = await getPlaybackMode();
          if (mode.repeat === "one") return;
          if (selectedPlaylistId && playlist.length > 0) {
            const nextIndex = (currentPlaylistIndex + 1) % playlist.length;
            await playTrackFromSpecificPlaylist(selectedPlaylistId, nextIndex);
          }
        }
        await updatePlaybackState();
        await loadQueueTracks();
        await loadPlaybackMode();
      })();
    }

    wasPlayingRef.current = is_playing;
  }, [playbackState]);

  // Poll queue more frequently while the panel is open
  useEffect(() => {
    if (!showQueue) return;
    loadQueueTracks().catch(() => {});
    const interval = setInterval(() => loadQueueTracks().catch(() => {}), 500);
    return () => clearInterval(interval);
  }, [showQueue]);

  useEffect(() => {
    if (!playlistDialog) return;
    playlistNameInputRef.current?.focus();
    playlistNameInputRef.current?.select();
  }, [playlistDialog]);

  // Push track metadata to OS media controls (Control Center, SMTC, MPRIS).
  // Uses primitive dependencies only (path + play state) to avoid spurious
  // re-fires when queue/playlist array references change during polling.
  useEffect(() => {
    if (currentTrack && playbackState.is_playing) {
      updateMediaMetadata({
        title: currentTrack.title,
        artist: currentTrack.artist,
        album: currentTrack.album,
        duration_seconds: currentTrack.duration_seconds,
        cover_url: currentTrack.cover_art_data_url,
      }).catch(console.error);
    }
  }, [currentTrack?.path, playbackState.is_playing]);

  // Listen for OS media control events (play/pause/next/prev/seek from OS)
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    const setup = async () => {
      unlisten = await listenToMediaControls({
        onPlay: () => {
          handlePlayPause().catch(console.error);
        },
        onPause: () => {
          if (playbackState.is_playing) pauseTrack().catch(console.error);
        },
        onNext: () => handleNext(),
        onPrevious: () => handlePrevious(),
        onSetPosition: (seconds) => {
          if (playbackState.current_path) seekTrack(seconds).catch(console.error);
        },
      });
    };
    setup();
    return () => {
      if (unlisten) unlisten();
    };
  }, [playbackState.is_playing, playbackState.current_path]);

  const handleAddTrack = async (multiple = false) => {
    try {
      setError(null);
      setIsAddingTracks(true);
      const paths = await selectAudioFile(multiple);
      if (paths?.length) {
        const playlistId = selectedPlaylistId ?? getDefaultPlaylistId(playlists);
        if (!playlistId) {
          setError("No playlist selected.");
          return;
        }
        let failCount = 0;
        for (const path of paths) {
          try {
            await addTrackToPlaylistById(playlistId, path);
          } catch (err) {
            failCount++;
            console.error("Failed to add track:", path, err);
          }
        }
        if (failCount > 0) setError(`Failed to add ${failCount} track(s).`);
        await loadPlaylistTracks(playlistId);
        await loadPlaylists();
      }
    } catch (err) {
      setError(formatInvokeError(err, "Failed to add track"));
    } finally {
      setIsAddingTracks(false);
    }
  };

  const handleRemoveTrack = async (path: string) => {
    try {
      setError(null);
      if (!selectedPlaylistId) return;
      await removeTrackFromPlaylistById(selectedPlaylistId, path);
      if (playbackState.current_path === path) {
        await stopTrack();
        setSeekValue(0);
      }
      await loadPlaylistTracks(selectedPlaylistId);
      await loadPlaylists();
      await updatePlaybackState();
    } catch (err) {
      setError(formatInvokeError(err, "Failed to remove track"));
    }
  };

  const handlePlayTrack = async (index: number) => {
    try {
      setError(null);
      setIsLoading(true);
      if (!selectedPlaylistId) return;
      await playTrackFromSpecificPlaylist(selectedPlaylistId, index);
      await updatePlaybackState();
      await loadQueueTracks();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to play track");
    } finally {
      setIsLoading(false);
    }
  };

  const isTrackPlayable = async (path: string | null | undefined): Promise<boolean> => {
    if (!path) return false;
    try {
      return await isTrackInPlaylist(path);
    } catch {
      return false;
    }
  };

  const ensureTrackPlayableOrPick = async (path: string | null | undefined): Promise<boolean> => {
    if (await isTrackPlayable(path)) return true;
    await stopTrack();
    setSeekValue(0);
    await handleAddTrack(false);
    return false;
  };

  const handlePlayPause = async () => {
    try {
      setError(null);
      setIsLoading(true);
      if (playbackState.is_playing) {
        await pauseTrack();
      } else if (playbackState.is_paused) {
        if (!(await ensureTrackPlayableOrPick(playbackState.current_path))) return;
        await resumeTrack();
      } else if (
        playbackState.current_path &&
        playbackState.duration_seconds != null &&
        playbackState.position_seconds < playbackState.duration_seconds - 1
      ) {
        if (!(await ensureTrackPlayableOrPick(playbackState.current_path))) return;
        await playTrack(playbackState.current_path);
      } else if (
        !playbackState.current_path &&
        playlist.length > 0 &&
        selectedPlaylistId
      ) {
        await playTrackFromSpecificPlaylist(selectedPlaylistId, 0);
      } else if (hasActiveQueue && queueData.current_index != null) {
        const nextIdx = queueData.current_index + 1;
        if (nextIdx < queueData.tracks.length && (await isTrackPlayable(queueData.tracks[nextIdx]?.path))) {
          await playTrackFromQueue(nextIdx);
        } else if (selectedPlaylistId && playlist.length > 0) {
          const nextIndex = currentPlaylistIndex >= 0
            ? (currentPlaylistIndex + 1) % playlist.length
            : 0;
          await playTrackFromSpecificPlaylist(selectedPlaylistId, nextIndex);
        } else if (await isTrackPlayable(queueData.tracks[queueData.current_index]?.path)) {
          await playTrackFromQueue(queueData.current_index);
        } else {
          await handleAddTrack(false);
          return;
        }
      } else if (hasActiveQueue) {
        if (await isTrackPlayable(queueData.tracks[0]?.path)) {
          await playTrackFromQueue(0);
        } else if (playlist.length > 0 && selectedPlaylistId) {
          await playTrackFromSpecificPlaylist(selectedPlaylistId, 0);
        } else {
          await handleAddTrack(false);
          return;
        }
      } else if (playlist.length > 0 && selectedPlaylistId) {
        await playTrackFromSpecificPlaylist(selectedPlaylistId, 0);
      } else {
        await handleAddTrack(false);
        return;
      }
      await updatePlaybackState();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to control playback");
    } finally {
      setIsLoading(false);
    }
  };

  const handleStop = async () => {
    try {
      setError(null);
      await stopTrack();
      setSeekValue(0);
      await updatePlaybackState();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to stop track");
    }
  };

  const handlePrevious = async () => {
    if (!canSkip) return;
    try {
      setError(null);
      const path = await playPrevious();
      if (!path && selectedPlaylistId && playlist.length > 0) {
        const prevIndex =
          currentPlaylistIndex > 0 ? currentPlaylistIndex - 1 : playlist.length - 1;
        await playTrackFromSpecificPlaylist(selectedPlaylistId, prevIndex);
      }
      await updatePlaybackState();
      await loadQueueTracks();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to go to previous track");
    }
  };

  const handleNext = async () => {
    if (!canSkip) return;
    try {
      setError(null);
      const path = await playNext();
      if (!path && selectedPlaylistId && playlist.length > 0) {
        const nextIndex =
          currentPlaylistIndex >= 0 ? (currentPlaylistIndex + 1) % playlist.length : 0;
        await playTrackFromSpecificPlaylist(selectedPlaylistId, nextIndex);
      }
      await updatePlaybackState();
      await loadQueueTracks();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to go to next track");
    }
  };

  const handleSeek = async (value: number) => {
    try {
      setSeekValue(value);
      if (playbackState.current_path) {
        await seekTrack(value);
        await updatePlaybackState();
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to seek track");
    } finally {
      document.body.classList.remove("is-seeking");
    }
  };

  const handleVolume = async (value: number) => {
    try {
      setVolumeValue(value);
      await setPlayerVolume(value);
      await updatePlaybackState();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to set volume");
    }
  };

  const handleClearPlaylist = async () => {
    setShowClearConfirm(true);
  };

  const confirmClearPlaylist = async () => {
    try {
      setError(null);
      setIsLoading(true);
      setShowClearConfirm(false);
      if (!selectedPlaylistId) return;
      await clearPlaylistById(selectedPlaylistId);
      await loadPlaylistTracks(selectedPlaylistId);
      await loadPlaylists();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to clear playlist");
    } finally {
      setIsLoading(false);
    }
  };

  // ── Playlist management ────────────────────────────────────────────────────

  const openCreatePlaylistDialog = () => {
    setPlaylistNameInput("");
    setPlaylistDialogError(null);
    setPlaylistDialog({ mode: "create" });
  };

  const openRenamePlaylistDialog = (playlistId: string, currentName: string) => {
    setPlaylistNameInput(currentName);
    setPlaylistDialogError(null);
    setPlaylistDialog({ mode: "rename", playlistId, currentName });
  };

  const closePlaylistDialog = () => {
    setPlaylistDialog(null);
    setPlaylistDialogError(null);
  };

  const submitPlaylistDialog = async () => {
    if (!playlistDialog) return;

    const name = playlistNameInput.trim();
    if (!name) {
      setPlaylistDialogError("Enter a playlist name.");
      return;
    }

    try {
      setError(null);
      setPlaylistDialogError(null);

      if (playlistDialog.mode === "create") {
        const info = await createPlaylist(name);
        await loadPlaylists();
        setSelectedPlaylistId(info.id);
        await loadPlaylistTracks(info.id);
      } else {
        await renamePlaylist(playlistDialog.playlistId, name);
        await loadPlaylists();
      }

      closePlaylistDialog();
    } catch (err) {
      setPlaylistDialogError(formatInvokeError(err, "Failed to save playlist"));
    }
  };

  const handleDeletePlaylist = async (id: string) => {
    const playlistInfo = playlists.find((p) => p.id === id);
    if (!confirm(`Delete playlist "${playlistInfo?.name ?? "Unknown"}"?`)) return;
    try {
      setError(null);
      await deletePlaylist(id);
      const list = await loadPlaylists();
      if (selectedPlaylistId === id) {
        const defaultId = getDefaultPlaylistId(list);
        if (defaultId) {
          setSelectedPlaylistId(defaultId);
          await loadPlaylistTracks(defaultId);
        }
      }
    } catch (err) {
      setError(formatInvokeError(err, "Failed to delete playlist"));
    }
  };

  const handleSelectPlaylist = async (id: string) => {
    setSelectedPlaylistId(id);
    setMenuTrackPath(null);
    try {
      await loadPlaylistTracks(id);
    } catch (err) {
      setError(formatInvokeError(err, "Failed to load playlist"));
    }
  };

  // ── Queue operations ───────────────────────────────────────────────────────

  const handleToggleFavorite = async (path: string) => {
    // Optimistic update: flip the heart immediately so the UI feels instant.
    const wasFavorited = favoritePaths.has(path);
    setFavoritePaths((prev) => {
      const next = new Set(prev);
      if (wasFavorited) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
    try {
      setError(null);
      await toggleFavorite(path);
      // Refresh playlist counts in the background (don't block the heart UI).
      loadPlaylists().catch(() => {});
      // If viewing the Favorites playlist, refresh its tracks so it stays accurate.
      const favPlaylist = playlists.find((p) => p.name === "Favorites");
      if (favPlaylist && selectedPlaylistId === favPlaylist.id) {
        await loadPlaylistTracks(favPlaylist.id);
      }
    } catch (err) {
      // Revert on failure.
      setFavoritePaths((prev) => {
        const next = new Set(prev);
        if (wasFavorited) {
          next.add(path);
        } else {
          next.delete(path);
        }
        return next;
      });
      setError(formatInvokeError(err, "Failed to toggle favorite"));
    }
  };

  const handlePlayNext = async (path: string) => {
    try {
      setError(null);
      await queueInsertNext(path);
      setMenuTrackPath(null);
      if (!playbackState.current_path && !playbackState.is_paused) {
        const data = await getQueueTracks();
        const idx = data.tracks.findIndex((track) => track.path === path);
        if (idx >= 0) {
          await playTrackFromQueue(idx);
          await updatePlaybackState();
        }
      }
      await loadQueueTracks();
    } catch (err) {
      setError(formatInvokeError(err, "Failed to add track to play next"));
    }
  };

  const handleAddToQueue = async (path: string) => {
    try {
      setError(null);
      await addToQueue(path);
      setMenuTrackPath(null);
      await loadQueueTracks();
    } catch (err) {
      setError(formatInvokeError(err, "Failed to add track to queue"));
    }
  };

  const handleAddTrackToPlaylist = async (targetPlaylistId: string, path: string) => {
    try {
      setError(null);
      await addTrackToPlaylistById(targetPlaylistId, path);
      setShowAddToPlaylist(false);
      setMenuTrackPath(null);
      await loadPlaylists();
      if (targetPlaylistId === selectedPlaylistId) {
        await loadPlaylistTracks(targetPlaylistId);
      }
    } catch (err) {
      setError(formatInvokeError(err, "Failed to add track to playlist"));
    }
  };

  const handleRemoveFromQueue = async (index: number) => {
    try {
      setError(null);
      await removeFromQueue(index);
      await loadQueueTracks();
    } catch (err) {
      setError(formatInvokeError(err, "Failed to remove from queue"));
    }
  };

  const handleClearQueue = async () => {
    try {
      setError(null);
      await clearQueue();
      await loadQueueTracks();
    } catch (err) {
      setError(formatInvokeError(err, "Failed to clear queue"));
    }
  };

  const handleToggleShuffle = async () => {
    try {
      const next = !playbackMode.shuffle;
      await setShuffle(next);
      await loadPlaybackMode();
    } catch (err) {
      setError(formatInvokeError(err, "Failed to toggle shuffle"));
    }
  };

  const handleCycleRepeat = async () => {
    try {
      const next = playbackMode.repeat === "off" ? "all" : playbackMode.repeat === "all" ? "one" : "off";
      await setRepeat(next);
      await loadPlaybackMode();
    } catch (err) {
      setError(formatInvokeError(err, "Failed to change repeat mode"));
    }
  };

  const handlePlayFromQueue = async (index: number) => {
    try {
      setError(null);
      await playTrackFromQueue(index);
      await updatePlaybackState();
      await loadQueueTracks();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to play track");
    }
  };

  // ── Export / Import ────────────────────────────────────────────────────────

  const handleExportPlaylistById = async (playlistId: string, playlistName: string) => {
    try {
      setError(null);
      const path = await savePlaylistDialog(playlistName);
      if (!path) return;
      const exportFormat = path.toLowerCase().endsWith(".json") ? "json" : "m3u";
      await exportPlaylist(playlistId, path, exportFormat);
    } catch (err) {
      setError(formatInvokeError(err, `Failed to export "${playlistName}"`));
    }
  };

  const handleImportPlaylist = async () => {
    try {
      setError(null);
      const path = await openPlaylistDialog();
      if (!path) return;
      const result = await importPlaylist(path);
      await loadPlaylists();
      setSelectedPlaylistId(result.playlist_id);
      await loadPlaylistTracks(result.playlist_id);
    } catch (err) {
      setError(formatInvokeError(err, "Failed to import playlist"));
    }
  };

  const isCurrentTrack = (track: Track) => track.path === playbackState.current_path;
  const coverLetters = getTrackTitle(currentTrack, playbackState.current_path).slice(0, 2).toUpperCase();

  return (
    <div className="app-container">
      <aside className="sidebar">
          <div className="brand-mark" style={{ textAlign: "center" }}>Wave</div>
          <div className="playlist-section">
            <div className="playlist-section-header">
              <p>Playlists</p>
              <button className="playlist-add-btn" onClick={handleImportPlaylist} type="button" title="Import playlist"><BiImport /></button>
              <button className="playlist-add-btn" onClick={openCreatePlaylistDialog} type="button" title="Create playlist"><BiPlus /></button>
            </div>
          <div className="playlist-list">
            {playlists.length === 0 ? (
              <div className="playlist-empty">
                <p>No playlists yet</p>
                <button className="btn-ghost btn-sm" onClick={openCreatePlaylistDialog} type="button">
                  Create one
                </button>
              </div>
            ) : (
              playlists.map((pl) => (
                <div
                  key={pl.id}
                  className={`playlist-item ${selectedPlaylistId === pl.id ? "active" : ""}`}
                  onClick={() => handleSelectPlaylist(pl.id)}
                >
                  <span className="playlist-item-name" title={pl.name}>{pl.name}</span>
                  <span className="playlist-item-count">{pl.track_count}</span>
                  <div className="playlist-item-actions">
                    <button
                      className="playlist-export-btn"
                      onClick={(e) => { e.stopPropagation(); handleExportPlaylistById(pl.id, pl.name); }}
                      title={`Export`}
                      type="button"
                    ><BiExport /></button>
                    {pl.name !== "All Local Files" && pl.name !== "Favorites" && (
                      <>
                        <button
                          className="playlist-rename-btn"
                          onClick={(e) => { e.stopPropagation(); openRenamePlaylistDialog(pl.id, pl.name); }}
                          title="Rename playlist"
                          type="button"
                        ><BiEditAlt /></button>
                        <button
                          className="playlist-delete-btn"
                          onClick={(e) => { e.stopPropagation(); handleDeletePlaylist(pl.id); }}
                          title="Delete playlist"
                          type="button"
                        ><BiTrash /></button>
                      </>
                    )}
                  </div>
                </div>
              ))
            )}
          </div>
        </div>
      </aside>

      <main className="main-content">
        <section className="hero-panel">
          <Artwork track={currentTrack} fallback={coverLetters} className="hero-art" />
          <div className="hero-copy">
            <h1>{selectedPlaylist?.name ?? "All Local Files"}</h1>
            <p>{playlist.length ? `${playlist.length} tracks in this playlist` : "No tracks in this playlist"}</p>
            <div className="hero-actions">
              <button className="big-play" onClick={handlePlayPause} disabled={isLoading} type="button" title="Play or pause">
                {playbackState.is_playing ? <BiPause /> : <BiPlay />}
              </button>
              <button className="btn-secondary" onClick={() => handleAddTrack(true)} disabled={isAddingTracks} type="button"><BiPlus /></button>
              {playlist.length > 0 && <button className="btn-ghost" onClick={handleClearPlaylist} type="button">Clear</button>}
            </div>
          </div>
        </section>

        {error && <div className="error-banner"><span>{error}</span><button onClick={() => setError(null)} type="button"><BiX /></button></div>}

        <section className="playlist-container">
          {playlist.length === 0 ? (
            <div className="empty-state">
              <div className="empty-icon"><BiMusic /></div>
              <h2>Your playlist is empty</h2>
              <button className="btn-primary" onClick={() => handleAddTrack(false)} disabled={isAddingTracks} type="button">Add your first track</button>
            </div>
          ) : (
            <div className="track-list">
              <div className="track-list-header">
                <div>#</div><div>Title</div><div>Album</div><div>Format</div><div>Duration</div><div></div>
              </div>
              {playlist.map((track, index) => (
                <div key={track.id} className={`track-item ${isCurrentTrack(track) ? "active" : ""}`} onClick={() => handlePlayTrack(index)}>
                  <div className="track-col-index">{isCurrentTrack(track) && playbackState.is_playing ? <span className="mini-bars"><i /><i /><i /></span> : index + 1}</div>
                  <div className="track-title-cell">
                    <Artwork track={track} fallback={getTrackTitle(track).slice(0, 1).toUpperCase()} className="track-thumb" />
                    <div>
                      <div className="track-name">{getTrackTitle(track)}</div>
                      <div className="track-meta">
                        {track.artist}
                        {track.lyrics ? " · lyrics" : ""}
                        {track.cover_art_source === "cover-art-archive" ? " · online cover" : ""}
                      </div>
                    </div>
                  </div>
                  <div className="track-album">{track.album}</div>
                  <div className="track-format">{track.format}</div>
                  <div className="track-duration">{formatTime(track.duration_seconds)}</div>
                  <div className="track-actions-cell">
                    <button
                      className={`track-action-btn favorite-btn ${favoritePaths.has(track.path) ? "active" : ""}`}
                      onClick={(event) => { event.stopPropagation(); handleToggleFavorite(track.path); }}
                      title={favoritePaths.has(track.path) ? "Remove from Favorites" : "Add to Favorites"}
                      type="button"
                    >{favoritePaths.has(track.path) ? <BiSolidHeart /> : <BiHeart />}</button>
                    <button className="track-action-btn" onClick={(event) => {
                      event.stopPropagation();
                      if (menuTrackPath === track.path) {
                        setMenuTrackPath(null);
                        setMenuAnchor(null);
                        setShowAddToPlaylist(false);
                      } else {
                        const rect = event.currentTarget.getBoundingClientRect();
                        setMenuTrackPath(track.path);
                        setMenuAnchor({ top: rect.bottom + 4, right: window.innerWidth - rect.right });
                        setShowAddToPlaylist(false);
                      }
                    }} title="More" type="button"><BiDotsHorizontalRounded /></button>
                    <button className="track-action-btn" onClick={(event) => { event.stopPropagation(); handleRemoveTrack(track.path); }} title="Remove" type="button"><BiX /></button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </section>

        {currentTrack?.lyrics && (
          <section className="lyrics-panel">
            <div>
              <p className="eyebrow">Lyrics</p>
              <h2>{getTrackTitle(currentTrack)}</h2>
              {currentTrack.lyrics_source && <p className="metadata-source">{currentTrack.lyrics_source}</p>}
            </div>
            <pre>{currentTrack.lyrics}</pre>
          </section>
        )}
      </main>

      {menuTrackPath && menuAnchor && (() => {
        const menuTrack = playlist.find((t) => t.path === menuTrackPath);
        if (!menuTrack) return null;
        const addToPlaylistOptions = playlists.filter((p) => p.id !== selectedPlaylistId && p.name !== "Favorites");
        return createPortal(
          <div
            className="track-context-menu"
            style={{ position: "fixed", top: `${menuAnchor.top}px`, right: `${menuAnchor.right}px` }}
            onClick={(e) => e.stopPropagation()}
          >
            <button type="button" onClick={() => handlePlayNext(menuTrack.path)}><BiListPlus /> Play Next</button>
            <button type="button" onClick={() => handleAddToQueue(menuTrack.path)}><BiListPlus /> Add to Queue</button>
            {addToPlaylistOptions.length > 0 && (
              <>
                <button type="button" onClick={() => setShowAddToPlaylist(true)}><BiListUl /> Add to Playlist...</button>
                {showAddToPlaylist && (
                  <div className="add-to-playlist-submenu">
                    {addToPlaylistOptions.map((p) => (
                      <button key={p.id} type="button" onClick={() => handleAddTrackToPlaylist(p.id, menuTrack.path)}>
                        {p.name}
                      </button>
                    ))}
                  </div>
                )}
              </>
            )}
          </div>,
          document.body
        );
      })()}

      {menuTrackPath && (
        <div className="context-menu-backdrop" onClick={() => { setMenuTrackPath(null); setMenuAnchor(null); setShowAddToPlaylist(false); }} />
      )}

      {playlistDialog && (
        <div className="modal-backdrop" onClick={closePlaylistDialog}>
          <div
            className="modal-dialog playlist-dialog"
            onClick={(event) => event.stopPropagation()}
            onKeyDown={(event) => {
              if (event.key === "Escape") closePlaylistDialog();
            }}
          >
            <div className="modal-header">
              <h2>{playlistDialog.mode === "create" ? "Create playlist" : "Rename playlist"}</h2>
              <button className="modal-close-btn" onClick={closePlaylistDialog} type="button" title="Close"><BiX /></button>
            </div>
            <form
              onSubmit={(event) => {
                event.preventDefault();
                submitPlaylistDialog();
              }}
            >
              <label className="modal-label" htmlFor="playlist-name-input">
                Name
              </label>
              <input
                id="playlist-name-input"
                ref={playlistNameInputRef}
                className="modal-input"
                type="text"
                value={playlistNameInput}
                onChange={(event) => setPlaylistNameInput(event.target.value)}
                placeholder="My playlist"
                autoComplete="off"
              />
              {playlistDialogError && <p className="modal-error">{playlistDialogError}</p>}
              <div className="modal-actions">
                <button className="btn-ghost" onClick={closePlaylistDialog} type="button">Cancel</button>
                <button className="btn-primary" type="submit">
                  {playlistDialog.mode === "create" ? "Create" : "Save"}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}

      {showClearConfirm && (
        <div className="modal-backdrop" onClick={() => setShowClearConfirm(false)}>
          <div
            className="modal-dialog confirm-dialog"
            onClick={(event) => event.stopPropagation()}
            onKeyDown={(event) => {
              if (event.key === "Escape") setShowClearConfirm(false);
            }}
          >
            <div className="modal-header">
              <h2>Clear playlist?</h2>
            </div>
            <p className="confirm-text">This will remove all tracks from this playlist. The files on disk won't be affected.</p>
            <div className="modal-actions">
              <button className="btn-ghost" onClick={() => setShowClearConfirm(false)} type="button">Cancel</button>
              <button className="btn-primary" onClick={confirmClearPlaylist} type="button">Clear</button>
            </div>
          </div>
        </div>
      )}

      {showQueue && (
        <div className="queue-panel">
          <div className="queue-header">
            <h2>Queue</h2>
            <div className="queue-header-actions">
              {queueData.tracks.length > 0 && (
                <button className="btn-ghost btn-sm" onClick={handleClearQueue} type="button">Clear</button>
              )}
              <button className="queue-close-btn" onClick={() => setShowQueue(false)} type="button" title="Close"><BiX /></button>
            </div>
          </div>
          <div className="queue-list">
            {queueData.tracks.length === 0 ? (
              <div className="queue-empty">
                <p>Queue is empty</p>
                <span>Add tracks with "Play Next" or "Add to Queue"</span>
              </div>
            ) : (
              queueData.tracks.map((track, index) => (
                <div
                  key={`${track.path}-${index}`}
                  className={`queue-item ${queueData.current_index === index ? "active" : ""}`}
                  onClick={() => handlePlayFromQueue(index)}
                >
                  <Artwork track={track} fallback={getTrackTitle(track).slice(0, 1).toUpperCase()} className="queue-thumb" />
                  <div className="queue-item-info">
                    <div className="queue-item-name">{getTrackTitle(track)}</div>
                    <div className="queue-item-artist">{track.artist}</div>
                  </div>
                  <div className="queue-item-duration">{formatTime(track.duration_seconds)}</div>
                  <button
                    className="queue-item-remove"
                    onClick={(e) => { e.stopPropagation(); handleRemoveFromQueue(index); }}
                    title="Remove from queue"
                    type="button"
                  ><BiX /></button>
                </div>
              ))
            )}
          </div>
        </div>
      )}

      <footer className="player-bar">
        <div className="player-left">
          <Artwork track={currentTrack} fallback={coverLetters} className="album-art" />
          <div className="now-playing-info">
            <div className="now-playing-name">{getTrackTitle(currentTrack, playbackState.current_path)}</div>
            <div className="now-playing-artist">{currentTrack?.artist ?? (playbackState.current_path ? "Local file" : "No track selected")}</div>
            <div className="now-playing-path">{currentTrack?.album ?? playbackState.current_path ?? "Add music to your playlist"}</div>
          </div>
        </div>

        <div className="player-center">
          <div className="player-controls">
            <button
              className={`control-btn shuffle-btn ${playbackMode.shuffle ? "active" : ""}`}
              onClick={handleToggleShuffle}
              type="button"
              title={playbackMode.shuffle ? "Disable shuffle" : "Enable shuffle"}
            ><BiShuffle /></button>
            <button className="control-btn" onClick={handlePrevious} disabled={!canSkip} type="button" title="Previous"><BiSkipPrevious /></button>
            <button className="control-btn" onClick={handleStop} disabled={!playbackState.current_path} type="button" title="Stop"><BiStop /></button>
            <button className="control-btn play-pause-btn" onClick={handlePlayPause} disabled={isLoading} type="button" title="Play/Pause">{playbackState.is_playing ? <BiPause /> : <BiPlay />}</button>
            <button className="control-btn" onClick={handleNext} disabled={!canSkip} type="button" title="Next"><BiSkipNext /></button>
            <button
              className={`control-btn repeat-btn ${playbackMode.repeat !== "off" ? "active" : ""} ${playbackMode.repeat === "one" ? "repeat-one" : ""}`}
              onClick={handleCycleRepeat}
              type="button"
              title={playbackMode.repeat === "off" ? "Repeat off" : playbackMode.repeat === "all" ? "Repeat all" : "Repeat one"}
            ><BiRepeat /></button>
          </div>
          <div className="seek-row">
            <span>{formatTime(displayPosition)}</span>
            <input
              className="range-slider"
              type="range"
              min="0"
              max={Math.max(displayDuration, 1)}
              step="1"
              value={displayPosition}
              disabled={!playbackState.current_path}
              onPointerDown={() => document.body.classList.add("is-seeking")}
              onChange={(event) => setSeekValue(Number(event.target.value))}
              onPointerUp={(event) => handleSeek(Number(event.currentTarget.value))}
            />
            <span>{formatTime(displayDuration)}</span>
          </div>
        </div>

        <div className="player-right">
          <div className="player-right-row">
            <button
              className={`control-btn queue-toggle ${showQueue ? "active" : ""}`}
              onClick={() => setShowQueue((v) => !v)}
              type="button"
              title="Toggle queue"
            ><BiListUl /></button>
            <span className={`status-dot ${playbackState.is_playing ? "playing" : playbackState.is_paused ? "paused" : ""}`} />
            <span className="volume-icon">{volumeValue === 0 ? <BiVolumeMute /> : volumeValue < 0.5 ? <BiVolumeLow /> : <BiVolumeFull />}</span>
            <input className="range-slider volume" type="range" min="0" max="1" step="0.01" value={volumeValue} onChange={(event) => handleVolume(Number(event.target.value))} />
            <span className="volume-percent">{Math.round(volumeValue * 100)}%</span>
          </div>
          <div className="device-selector">
            <button
              className="output-device-name"
              onClick={() => {
                listOutputDevices().then(setOutputDevices).catch(console.error);
                setShowDeviceList((v) => !v);
              }}
              title="Click to change audio output device"
              type="button"
            >
              {playbackState.output_device_name || "No device"}
            </button>
            {showDeviceList && (
              <>
                <div className="device-list-backdrop" onClick={() => setShowDeviceList(false)} />
                <div className="device-list">
                  {outputDevices.map((name) => (
                    <button
                      key={name}
                      className={`device-list-item ${name === playbackState.output_device_name ? "active" : ""}`}
                      onClick={async () => {
                        try {
                          await setOutputDevice(name);
                          await updatePlaybackState();
                          setShowDeviceList(false);
                        } catch (err) {
                          setError(err instanceof Error ? err.message : "Failed to change audio device");
                          setShowDeviceList(false);
                        }
                      }}
                      type="button"
                    >
                      {name}
                    </button>
                  ))}
                </div>
              </>
            )}
          </div>
        </div>
      </footer>

      {isLoading && (
        <div className="loading-indicator" role="status" aria-live="polite">
          <div className="spinner" /> Loading...
        </div>
      )}
    </div>
  );
}

export default App;
