// The Code for Frontend of Wave is currently completely AI Generated and may contain bugs or rough edges. Please report any issues you encounter at

import { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import trayTemplate from "../assets/tray-template.svg";
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
  BiFolderOpen,
  BiMenu,
  BiChevronUp,
  BiChevronDown,
  BiAlbum,
  BiUser,
  BiSync,
  BiMinus,
} from "react-icons/bi";
import {
  addTrackToPlaylistById,
  addToQueue,
  clearPlaylistById,
  clearQueue,
  createPlaylist,
  deletePlaylist,
  exportPlaylist,
  fetchLyricsForTrack,
  getFileName,
  getFavorites,
  getPlaybackMode,
  getPlaybackState,
  getPlaylistTracksById,
  getQueueTracks,
  importPlaylist,
  isTrackInPlaylist,
  scanDirectory,
  listPlaylists,
  listenToMediaControls,
  openPlaylistDialog,
  pauseTrack,
  playNext,
  playPrevious,
  playTrack,
  playTracks,
  playTrackFromQueue,
  playTrackFromSpecificPlaylist,
  queueInsertNext,
  removeTrackFromPlaylistById,
  removeTrackFromLibrary,
  removeFromQueue,
  moveQueueTrack,
  renamePlaylist,
  resumeTrack,
  savePlaylistDialog,
  seekTrack,
  selectAudioFile,
  selectAudioFolder,
  setPlayerVolume,
  setRepeat,
  setShuffle,
  stopTrack,
  toggleFavorite,
  clearMediaSession,
  updateMediaMetadata,
  updateMediaPosition,
  listOutputDevices,
  setOutputDevice,
  getEqSettings,
  setEqBands,
  setEqEnabled,
  EQ_BAND_LABELS,
  EQ_PRESETS,
  listMediaFolders,
  saveMediaFolder,
  scanDirectoryRecursive,
  importScannedAudio,
  setPlaylistSyncFolder,
  syncPlaylistFolder,
  isFolderSetupDismissed,
  dismissFolderSetup,
  type EqSettings,
  type PlaybackMode,
  type PlaybackState,
  type PlaylistInfo,
  type QueueTrackState,
  type Track,
} from "./utils/player";
import { isAndroid } from "./utils/platform";
import AlbumPage from "./components/AlbumPage";
import ArtistPage from "./components/ArtistPage";
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
  const remaining = Math.floor(seconds % 60)
    .toString()
    .padStart(2, "0");
  return `${minutes}:${remaining}`;
};

const LIBRARY_PLAYLIST_NAME = "Library";

const isLibraryPlaylistName = (name?: string | null) =>
  name === LIBRARY_PLAYLIST_NAME || name === "All Local Files";

const getTrackTitle = (track?: Track | null, fallbackPath?: string | null) => {
  if (track?.title) return track.title;
  if (track?.name) return track.name;
  return fallbackPath ? getFileName(fallbackPath) : "Choose a song";
};

type LyricLine = { time: number; text: string };

const LRC_TAG_RE = /\[(\d{1,2}):(\d{2})(?:\.(\d{1,3}))?\]/g;

// Parses LRC-style "[mm:ss.xx] text" lyrics into timestamped lines. Returns
// null if the text doesn't look like it has real timestamps (plain lyrics),
// so the caller can fall back to rendering the raw text.
const parseTimedLyrics = (raw?: string | null): LyricLine[] | null => {
  if (!raw) return null;
  const lines = raw.split(/\r?\n/);
  const result: LyricLine[] = [];
  let matchedLines = 0;
  let nonEmptyLines = 0;

  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed) continue;
    nonEmptyLines++;
    const tags = [...trimmed.matchAll(LRC_TAG_RE)];
    if (tags.length === 0) continue;
    matchedLines++;
    const text = trimmed.replace(LRC_TAG_RE, "").trim();
    for (const tag of tags) {
      const minutes = parseInt(tag[1], 10);
      const seconds = parseInt(tag[2], 10);
      const fraction = tag[3] ? parseFloat(`0.${tag[3]}`) : 0;
      result.push({ time: minutes * 60 + seconds + fraction, text });
    }
  }

  if (nonEmptyLines === 0 || matchedLines < nonEmptyLines * 0.4) return null;
  result.sort((a, b) => a.time - b.time);
  return result;
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
  const [playbackState, setPlaybackState] =
    useState<PlaybackState>(emptyPlaybackState);
  const [playlist, setPlaylist] = useState<Track[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [lyricsFetchPath, setLyricsFetchPath] = useState<string | null>(null);
  const lyricsFetchIdRef = useRef(0);
  const [isAddingTracks, setIsAddingTracks] = useState(false);
  const [isImporting, setIsImporting] = useState(false);
  const [isLoadingPlaylist, setIsLoadingPlaylist] = useState(true);
  const [importedCount, setImportedCount] = useState(0);
  const [showAddTrackMenu, setShowAddTrackMenu] = useState(false);
  const [addTrackMenuAnchor, setAddTrackMenuAnchor] = useState<{
    top: number;
    left: number;
  } | null>(null);
  const [seekValue, setSeekValue] = useState(0);
  const [volumeValue, setVolumeValue] = useState(0.8);

  // Playlist management
  const [playlists, setPlaylists] = useState<PlaylistInfo[]>([]);
  const [selectedPlaylistId, setSelectedPlaylistId] = useState<string | null>(
    null,
  );

  // Album / artist page navigation
  const [viewingAlbum, setViewingAlbum] = useState<{
    name: string;
    albumArtist: string | null;
  } | null>(null);
  const [viewingArtist, setViewingArtist] = useState<string | null>(null);

  // Favorited track paths (for heart toggle state in the track list)
  const [favoritePaths, setFavoritePaths] = useState<Set<string>>(new Set());

  // Clear-playlist confirmation modal
  const [showClearConfirm, setShowClearConfirm] = useState(false);

  // Delete-playlist confirmation modal
  const [deletePlaylistConfirm, setDeletePlaylistConfirm] = useState<{
    id: string;
    name: string;
  } | null>(null);

  // Playback mode
  const [playbackMode, setPlaybackMode] = useState<PlaybackMode>({
    repeat: "off",
    shuffle: false,
  });

  // Queue panel
  const [queueData, setQueueData] = useState<QueueTrackState>({
    tracks: [],
    current_index: null,
    is_shuffled: false,
  });
  const [showQueue, setShowQueue] = useState(false);

  // Lyrics panel
  const [lyricsPanelTrack, setLyricsPanelTrack] = useState<Track | null>(null);
  const activeLyricLineRef = useRef<HTMLButtonElement>(null);

  // Audio output device selection
  const [outputDevices, setOutputDevices] = useState<string[]>([]);
  const [showDeviceList, setShowDeviceList] = useState(false);

  // Equalizer
  const [showEqPanel, setShowEqPanel] = useState(false);
  const [eqSettings, setEqSettings] = useState<EqSettings>({
    bands: Array(10).fill(0),
    enabled: false,
  });
  const [eqAnchor, setEqAnchor] = useState<{
    bottom: number;
    right: number;
  } | null>(null);
  const volumeIconRef = useRef<HTMLButtonElement>(null);

  // Resizable columns
  const [sidebarWidth, setSidebarWidth] = useState(252);
  const [rightPanelWidth, setRightPanelWidth] = useState(320);
  const rightPanelOpen = showQueue || !!lyricsPanelTrack || showDeviceList;
  const [rightPanelClosing, setRightPanelClosing] = useState(false);
  const rightPanelClosingRef = useRef(false);
  const rightPanelCloseTimer = useRef<ReturnType<typeof setTimeout> | null>(
    null,
  );

  const isMobileLayout = () => window.innerWidth <= 900;

  const closeRightPanelDelayed = () => {
    if (rightPanelClosingRef.current) return;
    if (!isMobileLayout()) {
      setShowQueue(false);
      setShowDeviceList(false);
      setLyricsPanelTrack(null);
      return;
    }
    rightPanelClosingRef.current = true;
    setRightPanelClosing(true);
    rightPanelCloseTimer.current = setTimeout(() => {
      rightPanelClosingRef.current = false;
      setRightPanelClosing(false);
      setShowQueue(false);
      setShowDeviceList(false);
      setLyricsPanelTrack(null);
    }, 280);
  };

  const cancelCloseRightPanel = () => {
    if (rightPanelCloseTimer.current) {
      clearTimeout(rightPanelCloseTimer.current);
      rightPanelCloseTimer.current = null;
    }
    if (rightPanelClosingRef.current) {
      rightPanelClosingRef.current = false;
      setRightPanelClosing(false);
    }
  };

  const [mobileNavOpen, setMobileNavOpen] = useState(false);
  const [androidHost, setAndroidHost] = useState(false);
  const [showFolderSetup, setShowFolderSetup] = useState(false);
  const [isScanningFolder, setIsScanningFolder] = useState(false);
  const [folderScanIsSync, setFolderScanIsSync] = useState(false);

  const clampRightPanelWidth = (width: number, sidebar = sidebarWidth) => {
    const reserved = sidebar + 8 + 340; // handles + minimum main column
    const max = Math.max(280, Math.min(400, window.innerWidth - reserved));
    return Math.max(280, Math.min(max, width));
  };

  // Track context menu
  const [menuTrackPath, setMenuTrackPath] = useState<string | null>(null);
  const [menuAnchor, setMenuAnchor] = useState<{
    top: number;
    right?: number;
    left?: number;
  } | null>(null);
  const [addToPlaylistTrack, setAddToPlaylistTrack] = useState<string | null>(
    null,
  );

  // Queue context menu
  const [queueMenuIndex, setQueueMenuIndex] = useState<number | null>(null);
  const [queueMenuAnchor, setQueueMenuAnchor] = useState<{
    top: number;
    right?: number;
    left?: number;
  } | null>(null);

  // Sort state — cycles asc → desc → off on repeated header clicks
  const [sortColumn, setSortColumn] = useState<"index" | "title" | "album">(
    "index",
  );
  const [sortDirection, setSortDirection] = useState<"asc" | "desc" | "none">(
    "asc",
  );

  // Resizable title/album split (album column width in px)
  const [albumColWidth, setAlbumColWidth] = useState(200);
  const trackGridCols = `48px minmax(80px, 1fr) ${albumColWidth}px 64px 40px`;

  const handleAlbumColResizeStart = (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    const startX = e.clientX;
    const startWidth = albumColWidth;
    const onMouseMove = (ev: MouseEvent) => {
      // Handle sits on the title/album boundary: drag right → title grows, album shrinks.
      const dx = ev.clientX - startX;
      setAlbumColWidth(Math.max(80, Math.min(480, startWidth - dx)));
    };
    const onMouseUp = () => {
      document.removeEventListener("mousemove", onMouseMove);
      document.removeEventListener("mouseup", onMouseUp);
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
    };
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
    document.addEventListener("mousemove", onMouseMove);
    document.addEventListener("mouseup", onMouseUp);
  };

  const sortedPlaylist = useMemo(() => {
    const sorted = [...playlist];
    if (sortDirection === "none") return sorted;
    if (sortColumn === "title") {
      sorted.sort((a, b) =>
        (getTrackTitle(a) ?? "").localeCompare(getTrackTitle(b) ?? ""),
      );
    } else if (sortColumn === "album") {
      sorted.sort((a, b) => (a.album ?? "").localeCompare(b.album ?? ""));
    }
    if (sortDirection === "desc") sorted.reverse();
    return sorted;
  }, [playlist, sortColumn, sortDirection]);

  const handleSort = (column: typeof sortColumn) => {
    if (sortColumn === column) {
      setSortDirection((d) =>
        d === "asc" ? "desc" : d === "desc" ? "none" : "asc",
      );
    } else {
      setSortColumn(column);
      setSortDirection("asc");
    }
  };

  // Panel toggles (only one open at a time)
  const handleToggleQueue = () => {
    setMobileNavOpen(false);
    setRightPanelWidth((width) => clampRightPanelWidth(width));
    if (showQueue) {
      closeRightPanelDelayed();
      return;
    }
    cancelCloseRightPanel();
    setShowDeviceList(false);
    setLyricsPanelTrack(null);
    setShowQueue(true);
    void loadEqSettings();
  };

  const handleToggleLyrics = () => {
    setMobileNavOpen(false);
    setRightPanelWidth((width) => clampRightPanelWidth(width));
    if (lyricsPanelTrack) {
      closeRightPanelDelayed();
      return;
    }
    cancelCloseRightPanel();
    setShowQueue(false);
    setShowDeviceList(false);
    setLyricsPanelTrack(currentTrack ?? null);
  };

  const handleOpenLyrics = () => {
    if (!currentTrack) return;
    setMobileNavOpen(false);
    setRightPanelWidth((width) => clampRightPanelWidth(width));
    cancelCloseRightPanel();
    setShowQueue(false);
    setShowDeviceList(false);
    setLyricsPanelTrack(currentTrack);
  };

  const handleToggleDevice = () => {
    setMobileNavOpen(false);
    setRightPanelWidth((width) => clampRightPanelWidth(width));
    if (showDeviceList) {
      closeRightPanelDelayed();
      return;
    }
    cancelCloseRightPanel();
    setShowQueue(false);
    setLyricsPanelTrack(null);
    setShowDeviceList(true);
  };

  // Create / rename playlist dialog
  const [playlistDialog, setPlaylistDialog] = useState<
    | { mode: "create" }
    | { mode: "rename"; playlistId: string; currentName: string }
    | null
  >(null);
  const [playlistNameInput, setPlaylistNameInput] = useState("");
  const [playlistSyncFolder, setPlaylistSyncFolderInput] = useState<
    string | null
  >(null);
  const [playlistDialogError, setPlaylistDialogError] = useState<string | null>(
    null,
  );
  const playlistNameInputRef = useRef<HTMLInputElement>(null);
  const addTrackBtnRef = useRef<HTMLButtonElement>(null);
  const selectedPlaylistIdRef = useRef<string | null>(null);
  /** Monotonic id so stale playlist fetches never overwrite a newer selection. */
  const playlistLoadSeqRef = useRef(0);

  const setActivePlaylistId = (id: string | null) => {
    selectedPlaylistIdRef.current = id;
    setSelectedPlaylistId(id);
  };

  const currentTrack = useMemo(() => {
    if (!playbackState.current_path) return null;
    const fromQueue = queueData.tracks.find(
      (track) => track.path === playbackState.current_path,
    );
    if (fromQueue) return fromQueue;
    const fromPlaylist = playlist.find(
      (track) => track.path === playbackState.current_path,
    );
    return fromPlaylist ?? null;
  }, [playbackState.current_path, queueData.tracks, playlist]);

  // Drag-to-resize for sidebar and right panel
  const [dragging, setDragging] = useState<"sidebar" | "right" | null>(null);
  const dragStartRef = useRef({ x: 0, width: 0 });

  useEffect(() => {
    const media = window.matchMedia("(max-width: 900px)");
    const onChange = () => {
      if (!media.matches) setMobileNavOpen(false);
    };
    onChange();
    media.addEventListener("change", onChange);
    return () => media.removeEventListener("change", onChange);
  }, []);

  useEffect(() => {
    void isAndroid().then(setAndroidHost);
  }, []);

  // On Android, prompt for a music folder if Library isn't synced yet
  // and the user hasn't dismissed the welcome prompt.
  useEffect(() => {
    if (!androidHost || playlists.length === 0) return;
    const allLocal = playlists.find((p) => isLibraryPlaylistName(p.name));
    if (allLocal && !allLocal.sync_folder) {
      void Promise.all([listMediaFolders(), isFolderSetupDismissed()])
        .then(([folders, dismissed]) => {
          if (folders.length === 0 && !dismissed) setShowFolderSetup(true);
        })
        .catch(() => setShowFolderSetup(true));
    }
  }, [androidHost, playlists]);

  const skipFolderSetup = async () => {
    setShowFolderSetup(false);
    try {
      await dismissFolderSetup();
    } catch {
      /* ignore */
    }
  };

  /** Reconcile every playlist that has a sync_folder with its folder on disk. */
  const syncFolderPlaylists = async (
    list: PlaylistInfo[],
    isAndroidDevice: boolean,
  ) => {
    const synced = list.filter((p) => p.sync_folder);
    if (!synced.length) return;

    setIsScanningFolder(true);
    setFolderScanIsSync(true);
    let failed = 0;
    for (const [i, pl] of synced.entries()) {
      try {
        const folder = pl.sync_folder!;
        const paths = isAndroidDevice
          ? await scanDirectoryRecursive(folder)
          : null; // desktop: Rust walks sync_folder itself
        await syncPlaylistFolder(pl.id, paths);
        // Soft-refresh the open playlist without clearing the list first.
        const viewId = selectedPlaylistIdRef.current;
        if (viewId === pl.id || (!viewId && i === 0)) {
          const id = viewId ?? pl.id;
          getPlaylistTracksById(id)
            .then((tracks) => {
              if (selectedPlaylistIdRef.current === id) {
                setPlaylist(tracks);
              }
            })
            .catch(() => {});
        }
        loadPlaylists().catch(() => {});
      } catch (err) {
        failed++;
        console.warn(`Failed to sync playlist "${pl.name}":`, err);
      }
      // Let the UI process clicks between playlists.
      await new Promise((r) => setTimeout(r, 0));
    }
    setIsScanningFolder(false);
    setFolderScanIsSync(false);
    if (failed > 0) {
      setError(`Folder sync finished with ${failed} issue(s).`);
    }
  };

  useEffect(() => {
    const clamp = () =>
      setRightPanelWidth((width) => clampRightPanelWidth(width));
    clamp();
    window.addEventListener("resize", clamp);
    return () => window.removeEventListener("resize", clamp);
  }, [sidebarWidth]);

  useEffect(() => {
    if (!mobileNavOpen) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setMobileNavOpen(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [mobileNavOpen]);

  useEffect(() => {
    if (!dragging) return;
    const onMouseMove = (e: MouseEvent) => {
      const dx = e.clientX - dragStartRef.current.x;
      if (dragging === "sidebar") {
        setSidebarWidth(
          Math.max(180, Math.min(400, dragStartRef.current.width + dx)),
        );
      } else {
        setRightPanelWidth(
          clampRightPanelWidth(dragStartRef.current.width - dx),
        );
      }
    };
    const onMouseUp = () => {
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
      document.documentElement.style.userSelect = "";
      setDragging(null);
    };
    document.addEventListener("mousemove", onMouseMove);
    document.addEventListener("mouseup", onMouseUp, { once: true });
    return () => {
      document.removeEventListener("mousemove", onMouseMove);
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
      document.documentElement.style.userSelect = "";
    };
  }, [dragging]);

  const onDragStart = (which: "sidebar" | "right") => (e: React.MouseEvent) => {
    e.preventDefault();
    dragStartRef.current = {
      x: e.clientX,
      width: which === "sidebar" ? sidebarWidth : rightPanelWidth,
    };
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
    document.documentElement.style.userSelect = "none";
    setDragging(which);
  };

  // Close lyrics panel and auto-fetch lyrics when track changes
  useEffect(() => {
    if (!currentTrack) {
      setLyricsFetchPath(null);
      return;
    }
    setLyricsPanelTrack(null);
    if (
      currentTrack.lyrics &&
      (parseTimedLyrics(currentTrack.lyrics) || currentTrack.lyrics_source === "lrclib")
    ) {
      setLyricsFetchPath(null);
      return;
    }

    const path = currentTrack.path;
    const fetchId = ++lyricsFetchIdRef.current;
    setLyricsFetchPath(path);

    let cancelled = false;
    fetchLyricsForTrack(path)
      .then((updated) => {
        if (cancelled || lyricsFetchIdRef.current !== fetchId) return;
        setLyricsFetchPath(null);
        if (!updated?.lyrics) return;
        setPlaylist((prev) =>
          prev.map((t) =>
            t.path === path
              ? {
                  ...t,
                  lyrics: updated.lyrics,
                  lyrics_source: updated.lyrics_source,
                }
              : t,
          ),
        );
        setQueueData((prev) => ({
          ...prev,
          tracks: prev.tracks.map((t) =>
            t.path === path
              ? {
                  ...t,
                  lyrics: updated.lyrics,
                  lyrics_source: updated.lyrics_source,
                }
              : t,
          ),
        }));
        setLyricsPanelTrack((prev) =>
          prev && prev.path === path
            ? { ...prev, lyrics: updated.lyrics, lyrics_source: updated.lyrics_source }
            : prev,
        );
      })
      .catch(() => {
        if (!cancelled && lyricsFetchIdRef.current === fetchId) {
          setLyricsFetchPath(null);
        }
      });

    return () => {
      cancelled = true;
      if (lyricsFetchIdRef.current === fetchId) {
        lyricsFetchIdRef.current += 1;
      }
    };
  }, [currentTrack?.path]);

  const cancelLyricsFetch = () => {
    lyricsFetchIdRef.current += 1;
    setLyricsFetchPath(null);
  };

  const currentPlaylistIndex = useMemo(
    () =>
      playlist.findIndex((track) => track.path === playbackState.current_path),
    [playlist, playbackState.current_path],
  );

  const hasActiveQueue = queueData.tracks.length > 0;
  const canSkip = hasActiveQueue || playlist.length > 0;
  const displayDuration =
    playbackState.duration_seconds ?? currentTrack?.duration_seconds ?? 0;
  const displayPosition = Math.min(seekValue, displayDuration || seekValue);

  // Live (LRC-style) timestamped lyrics for the open lyrics panel.
  const timedLyrics = useMemo(
    () => parseTimedLyrics(lyricsPanelTrack?.lyrics),
    [lyricsPanelTrack?.lyrics],
  );
  const isLyricsPanelOnCurrentTrack =
    !!lyricsPanelTrack && lyricsPanelTrack.path === playbackState.current_path;
  const activeLyricIndex = useMemo(() => {
    if (!timedLyrics || !isLyricsPanelOnCurrentTrack) return -1;
    let idx = -1;
    for (let i = 0; i < timedLyrics.length; i++) {
      if (timedLyrics[i].time > displayPosition) break;
      idx = i;
    }
    return idx;
  }, [timedLyrics, isLyricsPanelOnCurrentTrack, displayPosition]);

  useEffect(() => {
    if (activeLyricIndex < 0) return;
    activeLyricLineRef.current?.scrollIntoView({
      block: "center",
      behavior: "smooth",
    });
  }, [activeLyricIndex]);

  const selectedPlaylist =
    playlists.find((p) => p.id === selectedPlaylistId) ?? null;

  const sortedPlaylists = useMemo(() => {
    const priority = [LIBRARY_PLAYLIST_NAME, "Favorites"];
    return [...playlists].sort((a, b) => {
      const ai = priority.indexOf(a.name);
      const bi = priority.indexOf(b.name);
      return (ai === -1 ? 99 : ai) - (bi === -1 ? 99 : bi);
    });
  }, [playlists]);

  const updatePlaybackState = async () => {
    const state = await getPlaybackState();
    setPlaybackState({ ...emptyPlaybackState, ...state });
    setVolumeValue(state.volume ?? 0.8);
    if (!document.body.classList.contains("is-seeking")) {
      setSeekValue(state.position_seconds ?? 0);
    }
    // Keep the OS media controls position in sync during playback and pause.
    if (state.current_path) {
      updateMediaPosition(state.position_seconds, state.is_playing).catch(
        console.error,
      );
    } else {
      clearMediaSession().catch(console.error);
    }
  };

  const loadPlaylists = async () => {
    const list = await listPlaylists();
    setPlaylists(list);
    return list;
  };

  const loadPlaylistTracks = async (playlistId: string) => {
    const seq = ++playlistLoadSeqRef.current;
    const tracks = await getPlaylistTracksById(playlistId);
    // Ignore stale responses from a prior playlist selection.
    if (
      seq !== playlistLoadSeqRef.current ||
      selectedPlaylistIdRef.current !== playlistId
    ) {
      return false;
    }
    setPlaylist(tracks);
    await loadFavoritePaths();
    return true;
  };

  const loadPlaybackMode = async () => {
    try {
      const mode = await getPlaybackMode();
      setPlaybackMode(mode);
    } catch {
      /* ignore */
    }
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
    return (
      (list.find((p) => isLibraryPlaylistName(p.name)) ?? list[0])?.id ?? null
    );
  };

  useEffect(() => {
    const initApp = async () => {
      await new Promise((resolve) => setTimeout(resolve, 300));
      try {
        setIsLoadingPlaylist(true);
        const list = await loadPlaylists();
        const defaultId = getDefaultPlaylistId(list);
        if (defaultId) {
          setActivePlaylistId(defaultId);
          await loadPlaylistTracks(defaultId);
        }
        await updatePlaybackState();
        await loadQueueTracks();
        await loadPlaybackMode();
        await loadEqSettings();
        await loadFavoritePaths();
        listOutputDevices().then(setOutputDevices).catch(console.error);

        // Reconcile synced playlists in the background — UI stays interactive.
        const android = await isAndroid().catch(() => false);
        void syncFolderPlaylists(list, android);
      } catch (err: any) {
        if (
          err?.message?.includes("not available") ||
          err?.message?.includes("undefined")
        ) {
          setError(
            "Tauri API not available. Run `npm run tauri dev` instead of plain Vite.",
          );
        }
      } finally {
        setIsLoadingPlaylist(false);
      }
    };

    initApp();
    const interval = setInterval(
      () => updatePlaybackState().catch(() => {}),
      500,
    );
    const queueInterval = setInterval(
      () => loadQueueTracks().catch(() => {}),
      2000,
    );
    const modeInterval = setInterval(
      () => loadPlaybackMode().catch(() => {}),
      2000,
    );
    return () => {
      clearInterval(interval);
      clearInterval(queueInterval);
      clearInterval(modeInterval);
    };
  }, []);

  // Auto-advance is handled natively in Rust (`tick_auto_advance`) so the
  // queue keeps going on Android even when sink-empty detection is flaky.
  // Frontend only polls playback state for the UI.

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
  // Fires on track change regardless of play state so the flyout updates immediately.
  useEffect(() => {
    if (currentTrack && playbackState.current_path) {
      updateMediaMetadata({
        title: currentTrack.title,
        artist: currentTrack.artist,
        album: currentTrack.album,
        duration_seconds: currentTrack.duration_seconds,
        cover_url: currentTrack.cover_art_data_url,
      }).catch(console.error);
    }
  }, [currentTrack?.path, playbackState.current_path]);

  // Listen for OS media control events (play/pause/next/prev/seek from OS).
  // Uses backend playback state directly — never opens the file picker.
  useEffect(() => {
    let unlisten: (() => void) | null = null;

    const osTogglePlayback = async () => {
      const state = await getPlaybackState();
      if (state.is_playing) {
        await pauseTrack();
      } else if (state.is_paused) {
        await resumeTrack();
      } else if (state.current_path) {
        await playTrack(state.current_path);
      } else {
        const list = await listPlaylists();
        const defaultId =
          list.find((p) => isLibraryPlaylistName(p.name))?.id ??
          list[0]?.id ??
          null;
        if (defaultId) {
          const tracks = await getPlaylistTracksById(defaultId);
          if (tracks.length > 0) {
            await playTrackFromSpecificPlaylist(defaultId, 0);
          }
        }
      }
      await updatePlaybackState();
    };

    const setup = async () => {
      unlisten = await listenToMediaControls({
        onPlay: async () => {
          const state = await getPlaybackState();
          if (!state.is_playing) await osTogglePlayback();
        },
        onPause: async () => {
          const state = await getPlaybackState();
          if (state.is_playing) {
            await pauseTrack();
            await updatePlaybackState();
          }
        },
        onToggle: () => {
          osTogglePlayback().catch(console.error);
        },
        onNext: async () => {
          await playNext();
          await updatePlaybackState();
          await loadQueueTracks();
        },
        onPrevious: async () => {
          await playPrevious();
          await updatePlaybackState();
          await loadQueueTracks();
        },
        onStop: () => {
          handleStop().catch(console.error);
        },
        onSetPosition: async (seconds) => {
          const state = await getPlaybackState();
          if (state.current_path) {
            await seekTrack(seconds);
            await updatePlaybackState();
          }
        },
        onShuffle: async () => {
          const mode = await getPlaybackMode();
          await setShuffle(!mode.shuffle);
          await loadPlaybackMode();
        },
        onRepeat: async () => {
          const mode = await getPlaybackMode();
          const next =
            mode.repeat === "off" ? "all" : mode.repeat === "all" ? "one" : "off";
          await setRepeat(next);
          await loadPlaybackMode();
        },
      });
    };
    setup();
    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  const handleAddTrack = async (multiple = false) => {
    try {
      setError(null);
      // Desktop: close the menu first so the native dialog isn't blocked by the
      // overlay/history sentinel. Android needs the picker in the same gesture
      // chain, so the menu stays open until after the picker returns.
      if (!androidHost) {
        setShowAddTrackMenu(false);
        setAddTrackMenuAnchor(null);
      }
      const paths = await selectAudioFile(multiple);
      setShowAddTrackMenu(false);
      setAddTrackMenuAnchor(null);
      if (!paths?.length) {
        return;
      }
      setIsAddingTracks(true);
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
    } catch (err) {
      setShowAddTrackMenu(false);
      setAddTrackMenuAnchor(null);
      setError(formatInvokeError(err, "Failed to add track"));
    } finally {
      setIsAddingTracks(false);
    }
  };

  const handleAddFolder = async () => {
    try {
      setError(null);
      // Desktop-only path: close menu before opening the folder dialog.
      setShowAddTrackMenu(false);
      setAddTrackMenuAnchor(null);
      const directory = await selectAudioFolder();
      if (!directory) {
        return;
      }
      setIsAddingTracks(true);
      const paths = await scanDirectory(directory);
      if (!paths.length) {
        setError("No audio files found in the selected folder.");
        setIsAddingTracks(false);
        return;
      }
      const playlistId = selectedPlaylistId ?? getDefaultPlaylistId(playlists);
      if (!playlistId) {
        setError("No playlist selected.");
        setIsAddingTracks(false);
        return;
      }
      setIsAddingTracks(false);
      runFolderImport(paths, playlistId).catch(() => {});
    } catch (err) {
      setShowAddTrackMenu(false);
      setAddTrackMenuAnchor(null);
      setError(formatInvokeError(err, "Failed to add folder"));
      setIsAddingTracks(false);
    }
  };

  const handleAddFolderAndroid = async () => {
    try {
      setError(null);
      // Keep the user-gesture chain intact for the SAF folder picker.
      const directory = await selectAudioFolder();
      setShowFolderSetup(false);
      if (!directory) return;

      setIsScanningFolder(true);
      setFolderScanIsSync(false);

      const paths = await scanDirectoryRecursive(directory);
      if (!paths.length) {
        setError("No audio files found in the selected folder.");
        setIsScanningFolder(false);
        return;
      }

      // Android media scan always targets Library.
      const list = playlists.length > 0 ? playlists : await loadPlaylists();
      const playlistId =
        list.find((p) => isLibraryPlaylistName(p.name))?.id ??
        getDefaultPlaylistId(list);
      if (!playlistId) {
        setError("No playlist selected.");
        setIsScanningFolder(false);
        return;
      }

      await setPlaylistSyncFolder(playlistId, directory);
      await saveMediaFolder(directory).catch(() => {});

      // Import in batches of 10 to show progress
      const BATCH = 10;
      let failCount = 0;
      for (let i = 0; i < paths.length; i += BATCH) {
        const batch = paths.slice(i, i + BATCH);
        try {
          const result = await importScannedAudio(batch, playlistId);
          failCount += result.errors.length;
        } catch {
          failCount += batch.length;
        }
      }

      setIsScanningFolder(false);
      if (failCount > 0) {
        setError(`Imported with ${failCount} error(s).`);
      }
      setActivePlaylistId(playlistId);
      await loadPlaylistTracks(playlistId);
      await loadPlaylists();
    } catch (err) {
      setIsScanningFolder(false);
      setError(formatInvokeError(err, "Failed to scan folder"));
    }
  };

  const handleAddFolderAsPlaylist = async () => {
    try {
      setError(null);
      // Desktop-only: close menu, then bind sync_folder and import.
      setShowAddTrackMenu(false);
      setAddTrackMenuAnchor(null);
      const directory = await selectAudioFolder();
      if (!directory) {
        return;
      }
      setIsAddingTracks(true);
      const folderName = getFileName(directory);
      const info = await createPlaylist(folderName, directory);
      setActivePlaylistId(info.id);
      await loadPlaylists();
      await loadPlaylistTracks(info.id);
      const paths = await scanDirectory(directory);
      if (!paths.length) {
        setError(`No audio files found in "${folderName}".`);
        setIsAddingTracks(false);
        return;
      }
      setIsAddingTracks(false);
      runFolderImport(paths, info.id).catch(() => {});
    } catch (err) {
      setShowAddTrackMenu(false);
      setAddTrackMenuAnchor(null);
      setError(formatInvokeError(err, "Failed to add folder as playlist"));
      setIsAddingTracks(false);
    }
  };

  const runFolderImport = async (paths: string[], playlistId: string) => {
    setIsImporting(true);
    setImportedCount(0);
    let failCount = 0;
    for (const [i, path] of paths.entries()) {
      try {
        await addTrackToPlaylistById(playlistId, path);
        setImportedCount(i + 1);
        if ((i + 1) % 5 === 0) {
          loadPlaylists().catch(() => {});
        }
      } catch {
        failCount++;
      }
    }
    setIsImporting(false);
    if (failCount > 0) {
      setError(`Finished importing folder with ${failCount} failure(s).`);
    }
    // Only refresh track list if the user is still viewing that playlist
    if (selectedPlaylistIdRef.current === playlistId) {
      await loadPlaylistTracks(playlistId);
    }
    await loadPlaylists();
  };

  const handleRemoveFromLibrary = async (path: string) => {
    try {
      setError(null);
      await removeTrackFromLibrary(path);
      if (playbackState.current_path === path) {
        await stopTrack();
        setSeekValue(0);
      }
      if (selectedPlaylistId) {
        await loadPlaylistTracks(selectedPlaylistId);
      }
      await loadPlaylists();
      await loadFavoritePaths();
      await loadQueueTracks();
      await updatePlaybackState();
    } catch (err) {
      setError(formatInvokeError(err, "Failed to remove from library"));
    }
  };

  const handleRemoveFromPlaylist = async (path: string) => {
    try {
      setError(null);
      if (!selectedPlaylistId) return;
      if (isLibraryPlaylistName(selectedPlaylist?.name)) {
        await handleRemoveFromLibrary(path);
        return;
      }
      await removeTrackFromPlaylistById(selectedPlaylistId, path);
      await loadPlaylistTracks(selectedPlaylistId);
      await loadPlaylists();
      await loadFavoritePaths();
    } catch (err) {
      setError(formatInvokeError(err, "Failed to remove from playlist"));
    }
  };

  const handlePlayTrack = async (sortedIndex: number) => {
    try {
      setError(null);
      if (!selectedPlaylistId) return;
      const sortedPaths = sortedPlaylist.map((t) => t.path);
      await playTrackFromSpecificPlaylist(
        selectedPlaylistId,
        sortedIndex,
        sortDirection !== "none" ? sortedPaths : undefined,
      );
      await updatePlaybackState();
      await loadQueueTracks();
    } catch (err) {
      setError(formatInvokeError(err, "Could not start playback"));
    }
  };

  const isTrackPlayable = async (
    path: string | null | undefined,
  ): Promise<boolean> => {
    if (!path) return false;
    try {
      return await isTrackInPlaylist(path);
    } catch {
      return false;
    }
  };

  const ensureTrackPlayableOrPick = async (
    path: string | null | undefined,
  ): Promise<boolean> => {
    if (await isTrackPlayable(path)) return true;
    await stopTrack();
    setSeekValue(0);
    await handleAddTrack(false);
    return false;
  };

  const handlePlayPause = async () => {
    try {
      setError(null);
      if (playbackState.is_playing) {
        await pauseTrack();
      } else if (playbackState.is_paused) {
        if (!(await ensureTrackPlayableOrPick(playbackState.current_path)))
          return;
        await resumeTrack();
      } else if (
        playbackState.current_path &&
        playbackState.duration_seconds != null &&
        playbackState.position_seconds < playbackState.duration_seconds - 1
      ) {
        if (!(await ensureTrackPlayableOrPick(playbackState.current_path)))
          return;
        await playTrack(playbackState.current_path);
      } else if (
        !playbackState.current_path &&
        playlist.length > 0 &&
        selectedPlaylistId
      ) {
        await playTrackFromSpecificPlaylist(selectedPlaylistId, 0);
      } else if (hasActiveQueue && queueData.current_index != null) {
        const nextIdx = queueData.current_index + 1;
        if (
          nextIdx < queueData.tracks.length &&
          (await isTrackPlayable(queueData.tracks[nextIdx]?.path))
        ) {
          await playTrackFromQueue(nextIdx);
        } else if (selectedPlaylistId && playlist.length > 0) {
          const nextIndex =
            currentPlaylistIndex >= 0
              ? (currentPlaylistIndex + 1) % playlist.length
              : 0;
          await playTrackFromSpecificPlaylist(selectedPlaylistId, nextIndex);
        } else if (
          await isTrackPlayable(queueData.tracks[queueData.current_index]?.path)
        ) {
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
      setError(formatInvokeError(err, "Failed to control playback"));
    }
  };

  const handleStop = async () => {
    try {
      setError(null);
      await stopTrack();
      setSeekValue(0);
      await updatePlaybackState();
    } catch (err) {
      setError(formatInvokeError(err, "Failed to stop track"));
    }
  };

  const handlePrevious = async () => {
    if (!canSkip) return;
    try {
      setError(null);
      const path = await playPrevious();
      if (!path && selectedPlaylistId && playlist.length > 0) {
        const prevIndex =
          currentPlaylistIndex > 0
            ? currentPlaylistIndex - 1
            : playlist.length - 1;
        await playTrackFromSpecificPlaylist(selectedPlaylistId, prevIndex);
      }
      await updatePlaybackState();
      await loadQueueTracks();
    } catch (err) {
      setError(formatInvokeError(err, "Failed to go to previous track"));
    }
  };

  const handleNext = async () => {
    if (!canSkip) return;
    try {
      setError(null);
      const path = await playNext();
      if (!path && selectedPlaylistId && playlist.length > 0) {
        const nextIndex =
          currentPlaylistIndex >= 0
            ? (currentPlaylistIndex + 1) % playlist.length
            : 0;
        await playTrackFromSpecificPlaylist(selectedPlaylistId, nextIndex);
      }
      await updatePlaybackState();
      await loadQueueTracks();
    } catch (err) {
      setError(formatInvokeError(err, "Failed to go to next track"));
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
      setError(formatInvokeError(err, "Failed to seek track"));
    } finally {
      document.body.classList.remove("is-seeking");
    }
  };

  const handlePlayPauseRef = useRef(handlePlayPause);
  const handleSeekRef = useRef(handleSeek);
  const mediaKeysRef = useRef({ position: 0, duration: 0, hasTrack: false });
  handlePlayPauseRef.current = handlePlayPause;
  handleSeekRef.current = handleSeek;
  mediaKeysRef.current = {
    position: displayPosition,
    duration: displayDuration,
    hasTrack: Boolean(playbackState.current_path),
  };

  useEffect(() => {
    const isEditableTarget = (target: EventTarget | null) => {
      if (!(target instanceof HTMLElement)) return false;
      const tag = target.tagName;
      return (
        tag === "INPUT" ||
        tag === "TEXTAREA" ||
        tag === "SELECT" ||
        target.isContentEditable
      );
    };

    const onKeyDown = (event: KeyboardEvent) => {
      if (
        event.metaKey ||
        event.ctrlKey ||
        event.altKey ||
        isEditableTarget(event.target)
      )
        return;

      if (event.code === "Space" || event.key === " ") {
        event.preventDefault();
        void handlePlayPauseRef.current();
        return;
      }

      if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
        const { position, duration, hasTrack } = mediaKeysRef.current;
        if (!hasTrack) return;
        event.preventDefault();
        const delta = event.key === "ArrowLeft" ? -5 : 5;
        const next = Math.max(0, position + delta);
        void handleSeekRef.current(
          duration > 0 ? Math.min(duration, next) : next,
        );
      }
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  const handleVolume = async (value: number) => {
    try {
      setVolumeValue(value);
      await setPlayerVolume(value);
      await updatePlaybackState();
    } catch (err) {
      setError(formatInvokeError(err, "Failed to set volume"));
    }
  };

  const loadEqSettings = async () => {
    try {
      const settings = await getEqSettings();
      const bands = Array.from(
        { length: 10 },
        (_, i) => settings.bands[i] ?? 0,
      );
      setEqSettings({ bands, enabled: settings.enabled });
    } catch (err) {
      console.error("Failed to load EQ settings", err);
    }
  };

  const handleToggleEqPanel = async () => {
    if (showEqPanel) {
      setShowEqPanel(false);
      setEqAnchor(null);
      return;
    }
    await loadEqSettings();
    if (volumeIconRef.current) {
      const rect = volumeIconRef.current.getBoundingClientRect();
      setEqAnchor({
        bottom: window.innerHeight - rect.top + 8,
        right: Math.max(12, window.innerWidth - rect.right),
      });
    }
    setShowEqPanel(true);
  };

  const handleEqEnabled = async (enabled: boolean) => {
    const previous = eqSettings;
    setEqSettings((s) => ({ ...s, enabled }));
    try {
      await setEqEnabled(enabled);
    } catch (err) {
      setEqSettings(previous);
      setError(formatInvokeError(err, "Failed to toggle equalizer"));
    }
  };

  const handleEqBandChange = async (index: number, gain: number) => {
    const bands = eqSettings.bands.map((value, i) =>
      i === index ? gain : value,
    );
    setEqSettings((s) => ({ ...s, bands, enabled: true }));
    try {
      await setEqBands(bands);
      if (!eqSettings.enabled) await setEqEnabled(true);
    } catch (err) {
      setError(formatInvokeError(err, "Failed to update EQ band"));
      await loadEqSettings();
    }
  };

  const handleEqPreset = async (presetId: string) => {
    const preset = EQ_PRESETS.find((p) => p.id === presetId);
    if (!preset) return;
    const bands = [...preset.bands];
    setEqSettings({ bands, enabled: true });
    try {
      await setEqBands(bands);
      await setEqEnabled(true);
    } catch (err) {
      setError(formatInvokeError(err, "Failed to apply EQ preset"));
      await loadEqSettings();
    }
  };

  const handleEqReset = async () => {
    await handleEqPreset("flat");
  };

  const handleClearPlaylist = async () => {
    if (selectedPlaylist?.sync_folder) {
      setError("Synced playlists cannot be cleared.");
      return;
    }
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
      setError(formatInvokeError(err, "Failed to clear playlist"));
    } finally {
      setIsLoading(false);
    }
  };

  // ── Playlist management ────────────────────────────────────────────────────

  const openCreatePlaylistDialog = () => {
    setMobileNavOpen(false);
    setPlaylistNameInput("");
    setPlaylistSyncFolderInput(null);
    setPlaylistDialogError(null);
    setPlaylistDialog({ mode: "create" });
  };

  const openRenamePlaylistDialog = (
    playlistId: string,
    currentName: string,
  ) => {
    setMobileNavOpen(false);
    setPlaylistNameInput(currentName);
    setPlaylistSyncFolderInput(null);
    setPlaylistDialogError(null);
    setPlaylistDialog({ mode: "rename", playlistId, currentName });
  };

  const closePlaylistDialog = () => {
    setPlaylistDialog(null);
    setPlaylistSyncFolderInput(null);
    setPlaylistDialogError(null);
  };

  const pickPlaylistSyncFolder = async () => {
    try {
      const directory = await selectAudioFolder();
      if (!directory) return;
      setPlaylistSyncFolderInput(directory);
      if (!playlistNameInput.trim()) {
        setPlaylistNameInput(getFileName(directory));
      }
    } catch (err) {
      setPlaylistDialogError(
        formatInvokeError(err, "Failed to select folder"),
      );
    }
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
        const info = await createPlaylist(name, playlistSyncFolder);
        await loadPlaylists();
        setActivePlaylistId(info.id);
        await loadPlaylistTracks(info.id);
        if (playlistSyncFolder) {
          closePlaylistDialog();
          const paths = androidHost
            ? await scanDirectoryRecursive(playlistSyncFolder)
            : await scanDirectory(playlistSyncFolder);
          if (!paths.length) {
            setError(`No audio files found in the selected folder.`);
            return;
          }
          if (androidHost) {
            setIsScanningFolder(true);
            setFolderScanIsSync(false);
            const BATCH = 10;
            let failCount = 0;
            for (let i = 0; i < paths.length; i += BATCH) {
              const batch = paths.slice(i, i + BATCH);
              try {
                const result = await importScannedAudio(batch, info.id);
                failCount += result.errors.length;
              } catch {
                failCount += batch.length;
              }
            }
            setIsScanningFolder(false);
            if (failCount > 0) {
              setError(`Imported with ${failCount} error(s).`);
            }
            await loadPlaylistTracks(info.id);
            await loadPlaylists();
          } else {
            runFolderImport(paths, info.id).catch(() => {});
          }
          return;
        }
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
    setDeletePlaylistConfirm({ id, name: playlistInfo?.name ?? "Unknown" });
  };

  const confirmDeletePlaylist = async () => {
    if (!deletePlaylistConfirm) return;
    const { id } = deletePlaylistConfirm;
    setDeletePlaylistConfirm(null);
    try {
      setError(null);
      await deletePlaylist(id);
      const list = await loadPlaylists();
      if (selectedPlaylistId === id) {
        const defaultId = getDefaultPlaylistId(list);
        if (defaultId) {
          setActivePlaylistId(defaultId);
          setPlaylist([]);
          setIsLoadingPlaylist(true);
          try {
            await loadPlaylistTracks(defaultId);
          } finally {
            if (selectedPlaylistIdRef.current === defaultId) {
              setIsLoadingPlaylist(false);
            }
          }
        }
      }
    } catch (err) {
      setError(formatInvokeError(err, "Failed to delete playlist"));
    }
  };

  const handleSelectPlaylist = (id: string) => {
    const samePlaylist = selectedPlaylistIdRef.current === id;
    setViewingAlbum(null);
    setViewingArtist(null);
    setMenuTrackPath(null);
    setMobileNavOpen(false);
    setIsImporting(false);

    // Already showing this playlist (e.g. leaving an album view) — no refetch flash.
    if (samePlaylist) {
      return;
    }

    setActivePlaylistId(id);
    // Clear immediately so the title and list never disagree while loading.
    setPlaylist([]);
    setIsLoadingPlaylist(true);

    void (async () => {
      try {
        await loadPlaylistTracks(id);
      } catch (err) {
        if (selectedPlaylistIdRef.current === id) {
          setError(formatInvokeError(err, "Failed to load playlist"));
        }
      } finally {
        if (selectedPlaylistIdRef.current === id) {
          setIsLoadingPlaylist(false);
        }
      }
    })();
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

  const handleAddTrackToPlaylist = async (
    targetPlaylistId: string,
    path: string,
  ) => {
    try {
      setError(null);
      await addTrackToPlaylistById(targetPlaylistId, path);
      setAddToPlaylistTrack(null);
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

  const handleMoveQueueTrack = async (from: number, to: number) => {
    if (to < 0 || to >= queueData.tracks.length) return;
    try {
      setError(null);
      setQueueMenuIndex(null);
      setQueueMenuAnchor(null);
      await moveQueueTrack(from, to);
      await loadQueueTracks();
    } catch (err) {
      setError(formatInvokeError(err, "Failed to reorder queue"));
    }
  };

  const openTrackContextMenu = (
    path: string,
    anchor: { top: number; right?: number; left?: number },
  ) => {
    setQueueMenuIndex(null);
    setQueueMenuAnchor(null);
    setMenuTrackPath(path);
    setMenuAnchor(anchor);
    setAddToPlaylistTrack(null);
  };

  const closeTrackContextMenu = () => {
    setMenuTrackPath(null);
    setMenuAnchor(null);
  };

  const closeQueueContextMenu = () => {
    setQueueMenuIndex(null);
    setQueueMenuAnchor(null);
  };

  const openQueueContextMenu = (
    index: number,
    anchor: { top: number; right?: number; left?: number },
  ) => {
    setMenuTrackPath(null);
    setMenuAnchor(null);
    setAddToPlaylistTrack(null);
    setQueueMenuIndex(index);
    setQueueMenuAnchor(anchor);
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
      const next =
        playbackMode.repeat === "off"
          ? "all"
          : playbackMode.repeat === "all"
            ? "one"
            : "off";
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
      setError(formatInvokeError(err, "Could not start playback"));
    }
  };

  // ── Export / Import ────────────────────────────────────────────────────────

  const handleExportPlaylistById = async (
    playlistId: string,
    playlistName: string,
  ) => {
    try {
      setError(null);
      const path = await savePlaylistDialog(playlistName);
      if (!path) return;
      const exportFormat = path.toLowerCase().endsWith(".json")
        ? "json"
        : "m3u";
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
      setActivePlaylistId(result.playlist_id);
      await loadPlaylistTracks(result.playlist_id);
    } catch (err) {
      setError(formatInvokeError(err, "Failed to import playlist"));
    }
  };

  // ── Hardware/OS back button ─────────────────────────────────────────────
  // Whenever a modal, popover, or the mobile sidebar/right panel is open, a
  // browser history "sentinel" entry is pushed. Pressing back then just
  // navigates that sentinel (dismissing the topmost overlay) instead of
  // letting the WebView fall through to its default behaviour of exiting
  // the app. Kept fresh on every render so the popstate handler (registered
  // once) always sees the latest overlay state via the ref.
  const overlaySnapshotRef = useRef({
    menuTrackPath,
    queueMenuIndex,
    showAddTrackMenu,
    showEqPanel,
    playlistDialog,
    showClearConfirm,
    deletePlaylistConfirm,
    addToPlaylistTrack,
    mobileNavOpen,
    rightPanelOpen,
    viewingAlbum,
    viewingArtist,
  });
  overlaySnapshotRef.current = {
    menuTrackPath,
    queueMenuIndex,
    showAddTrackMenu,
    showEqPanel,
    playlistDialog,
    showClearConfirm,
    deletePlaylistConfirm,
    addToPlaylistTrack,
    mobileNavOpen,
    rightPanelOpen,
    viewingAlbum,
    viewingArtist,
  };

  const isAnyOverlayOpen = () => {
    const s = overlaySnapshotRef.current;
    return !!(
      s.menuTrackPath ||
      s.queueMenuIndex != null ||
      s.showAddTrackMenu ||
      s.showEqPanel ||
      s.playlistDialog ||
      s.showClearConfirm ||
      s.deletePlaylistConfirm ||
      s.addToPlaylistTrack ||
      s.mobileNavOpen ||
      s.rightPanelOpen ||
      s.viewingAlbum ||
      s.viewingArtist
    );
  };

  // Closes whichever overlay is "on top" — small popovers/context menus
  // first, then modals, then the mobile sidebar/right panel — and reports
  // whether anything was actually closed.
  const closeTopOverlay = (): boolean => {
    const s = overlaySnapshotRef.current;
    if (s.menuTrackPath || s.queueMenuIndex != null) {
      closeTrackContextMenu();
      closeQueueContextMenu();
      return true;
    }
    if (s.showAddTrackMenu) {
      setShowAddTrackMenu(false);
      setAddTrackMenuAnchor(null);
      return true;
    }
    if (s.showEqPanel) {
      setShowEqPanel(false);
      setEqAnchor(null);
      return true;
    }
    if (s.playlistDialog) {
      closePlaylistDialog();
      return true;
    }
    if (s.showClearConfirm) {
      setShowClearConfirm(false);
      return true;
    }
    if (s.deletePlaylistConfirm) {
      setDeletePlaylistConfirm(null);
      return true;
    }
    if (s.addToPlaylistTrack) {
      setAddToPlaylistTrack(null);
      return true;
    }
    if (s.mobileNavOpen) {
      setMobileNavOpen(false);
      return true;
    }
    if (s.viewingAlbum) {
      setViewingAlbum(null);
      return true;
    }
    if (s.viewingArtist) {
      setViewingArtist(null);
      return true;
    }
    if (s.rightPanelOpen) {
      closeRightPanelDelayed();
      return true;
    }
    return false;
  };

  const historyPushedRef = useRef(false);

  // Keep the history sentinel in sync with overlay state on every render.
  // This naturally also handles nested overlays (e.g. a context menu on top
  // of a modal): once the top layer closes, this re-runs and pushes a fresh
  // sentinel for whatever is still open.
  useEffect(() => {
    if (isAnyOverlayOpen() && !historyPushedRef.current) {
      window.history.pushState({ waveOverlay: true }, "");
      historyPushedRef.current = true;
    } else if (!isAnyOverlayOpen() && historyPushedRef.current) {
      historyPushedRef.current = false;
      if ((window.history.state as { waveOverlay?: boolean } | null)?.waveOverlay) {
        window.history.back();
      }
    }
  });

  useEffect(() => {
    const onPopState = () => {
      const closed = closeTopOverlay();
      // The sentinel (if any) has just been consumed by this navigation.
      historyPushedRef.current = false;
      if (!closed) return;
    };
    window.addEventListener("popstate", onPopState);
    return () => window.removeEventListener("popstate", onPopState);
  }, []);

  const isCurrentTrack = (track: Track) =>
    track.path === playbackState.current_path;
  const coverLetters = getTrackTitle(currentTrack, playbackState.current_path)
    .slice(0, 2)
    .toUpperCase();

  return (
    <div
      className={`app-container${mobileNavOpen ? " nav-open" : ""}${rightPanelOpen || rightPanelClosing ? " panel-open" : ""}${rightPanelClosing ? " panel-closing" : ""}`}
      style={
        {
          "--sidebar-width": `${sidebarWidth}px`,
          "--right-panel-width":
            rightPanelOpen || rightPanelClosing
              ? `${rightPanelWidth}px`
              : "0px",
          "--right-handle-width":
            rightPanelOpen || rightPanelClosing ? "4px" : "0px",
        } as React.CSSProperties
      }
    >
      <header className="mobile-topbar">
        <button
          className="mobile-topbar-btn"
          onClick={() => {
            setShowQueue(false);
            setShowDeviceList(false);
            setLyricsPanelTrack(null);
            setMobileNavOpen(true);
          }}
          type="button"
          title="Open playlists"
          aria-label="Open playlists"
        >
          <BiMenu />
        </button>
        <div className="mobile-topbar-title">
          <img src={trayTemplate} alt="Wave" className="mobile-topbar-logo" />
          {isScanningFolder ? (
            <span
              className="brand-sync-spinner"
              title={folderScanIsSync ? "Syncing folders…" : "Importing…"}
              aria-label={folderScanIsSync ? "Syncing folders" : "Importing"}
              role="status"
            />
          ) : null}
        </div>
        <button
          className={`mobile-topbar-btn ${showQueue ? "active" : ""}`}
          onClick={handleToggleQueue}
          type="button"
          title="Queue"
          aria-label="Toggle queue"
        >
          <BiListUl />
        </button>
      </header>

      <button
        className={`nav-backdrop${mobileNavOpen || rightPanelOpen || rightPanelClosing ? " nav-backdrop-open" : ""}${rightPanelClosing ? " nav-backdrop-closing" : ""}`}
        onClick={() => {
          setMobileNavOpen(false);
          closeRightPanelDelayed();
        }}
        type="button"
        aria-label="Close panel"
      />

      <aside className="sidebar">
        <div className="brand-mark">
          <img src={trayTemplate} alt="Wave" className="brand-logo" />
          {isScanningFolder ? (
            <span
              className="brand-sync-spinner"
              title={folderScanIsSync ? "Syncing folders…" : "Importing…"}
              aria-label={folderScanIsSync ? "Syncing folders" : "Importing"}
              role="status"
            />
          ) : null}
        </div>
        <div className="playlist-section">
          <div className="playlist-section-header">
            <p>Playlists</p>
            <button
              className="playlist-add-btn"
              onClick={handleImportPlaylist}
              type="button"
              title="Import playlist"
            >
              <BiImport />
            </button>
            <button
              className="playlist-add-btn"
              onClick={openCreatePlaylistDialog}
              type="button"
              title="Create playlist"
            >
              <BiPlus />
            </button>
          </div>
          <div className="playlist-list">
            {playlists.length === 0 ? (
              <div className="playlist-empty">
                <p>No playlists yet</p>
                <button
                  className="btn-ghost btn-sm"
                  onClick={openCreatePlaylistDialog}
                  type="button"
                >
                  Create one
                </button>
              </div>
            ) : (
              sortedPlaylists.map((pl) => (
                <div
                  key={pl.id}
                  className={`playlist-item ${!viewingAlbum && !viewingArtist && selectedPlaylistId === pl.id ? "active" : ""}`}
                  onClick={() => handleSelectPlaylist(pl.id)}
                >
                  <span className="playlist-item-name" title={pl.name}>
                    {(isScanningFolder &&
                      (pl.sync_folder || isLibraryPlaylistName(pl.name)) &&
                      (selectedPlaylistId === pl.id ||
                        isLibraryPlaylistName(pl.name))) ||
                    (isImporting &&
                      pl.sync_folder &&
                      selectedPlaylistId === pl.id) ? (
                      <BiSync
                        className="playlist-sync-icon playlist-sync-spin"
                        title="Syncing with folder"
                        aria-label="Syncing with folder"
                      />
                    ) : pl.sync_folder ? (
                      <BiSync
                        className="playlist-sync-icon"
                        title="Synced with a folder"
                        aria-label="Synced with a folder"
                      />
                    ) : null}
                    {pl.name}
                  </span>
                  {!isLibraryPlaylistName(pl.name) && (
                    <span className="playlist-item-count">
                      {pl.track_count}
                    </span>
                  )}
                  <div className="playlist-item-actions">
                    <button
                      className="playlist-export-btn"
                      onClick={(e) => {
                        e.stopPropagation();
                        handleExportPlaylistById(pl.id, pl.name);
                      }}
                      title={`Export`}
                      type="button"
                    >
                      <BiExport />
                    </button>
                    {!isLibraryPlaylistName(pl.name) &&
                      pl.name !== "Favorites" && (
                        <>
                          <button
                            className="playlist-rename-btn"
                            onClick={(e) => {
                              e.stopPropagation();
                              openRenamePlaylistDialog(pl.id, pl.name);
                            }}
                            title="Rename playlist"
                            type="button"
                          >
                            <BiEditAlt />
                          </button>
                          <button
                            className="playlist-delete-btn"
                            onClick={(e) => {
                              e.stopPropagation();
                              handleDeletePlaylist(pl.id);
                            }}
                            title="Delete playlist"
                            type="button"
                          >
                            <BiTrash />
                          </button>
                        </>
                      )}
                  </div>
                </div>
              ))
            )}
          </div>
        </div>
      </aside>

      <div
        className="drag-handle drag-handle-sidebar"
        onMouseDown={onDragStart("sidebar")}
      />

      {viewingAlbum ? (
        <AlbumPage
          album={viewingAlbum.name}
          albumArtist={viewingAlbum.albumArtist}
          onBack={() => setViewingAlbum(null)}
          onPlayTrack={(path, tracks) => {
            const index = Math.max(
              0,
              tracks.findIndex((t) => t.path === path),
            );
            void playTracks(
              tracks.map((t) => t.path),
              index,
            ).then(() => {
              updatePlaybackState();
              loadQueueTracks();
            });
          }}
          onArtistClick={(name) => {
            setViewingAlbum(null);
            setViewingArtist(name);
          }}
          playbackState={playbackState}
        />
      ) : viewingArtist ? (
        <ArtistPage
          artist={viewingArtist}
          onBack={() => setViewingArtist(null)}
          onPlayTrack={(path, tracks) => {
            const index = Math.max(
              0,
              tracks.findIndex((t) => t.path === path),
            );
            void playTracks(
              tracks.map((t) => t.path),
              index,
            ).then(() => {
              updatePlaybackState();
              loadQueueTracks();
            });
          }}
          onAlbumClick={(name, albumArtist) => {
            setViewingArtist(null);
            setViewingAlbum({ name, albumArtist });
          }}
          playbackState={playbackState}
        />
      ) : (
      <main className="main-content">
        <div className="hero-copy">
          <h1>{selectedPlaylist?.name ?? LIBRARY_PLAYLIST_NAME}</h1>
          <p>
            {playlist.length
              ? `${playlist.length} tracks in this playlist`
              : isLoadingPlaylist
                ? "Loading tracks…"
                : "No tracks in this playlist"}
            {(isScanningFolder &&
              (selectedPlaylist?.sync_folder ||
                isLibraryPlaylistName(selectedPlaylist?.name))) ||
            (isImporting && selectedPlaylist?.sync_folder) ? (
              <>
                {" · "}
                <span className="playlist-sync-badge playlist-sync-badge-active">
                  <BiSync className="playlist-sync-spin" /> Syncing…
                </span>
              </>
            ) : selectedPlaylist?.sync_folder ? (
              <>
                {" · "}
                <span
                  className="playlist-sync-badge"
                  title={selectedPlaylist.sync_folder}
                >
                  <BiSync /> Synced folder
                </span>
              </>
            ) : null}
          </p>
          <div className="hero-actions">
            <button
              className="big-play"
              onClick={handlePlayPause}
              type="button"
              title="Play or pause"
            >
              {playbackState.is_playing ? <BiPause /> : <BiPlay />}
            </button>
            {selectedPlaylist?.name !== "Favorites" && (
              <div className="add-track-wrap">
                <button
                  ref={addTrackBtnRef}
                  className="btn-secondary"
                  onClick={() => {
                    if (androidHost) {
                      // On Android, open folder picker for media scanning.
                      void handleAddFolderAndroid();
                      return;
                    }
                    if (addTrackBtnRef.current) {
                      const rect =
                        addTrackBtnRef.current.getBoundingClientRect();
                      setAddTrackMenuAnchor({
                        top: rect.bottom + 6,
                        left: rect.left,
                      });
                    }
                    setShowAddTrackMenu((v) => !v);
                  }}
                  disabled={isAddingTracks}
                  type="button"
                  title={androidHost ? "Scan media folder" : "Add tracks"}
                >
                  <BiPlus />
                </button>
              </div>
            )}
            {playlist.length > 0 &&
              !isLibraryPlaylistName(selectedPlaylist?.name) &&
              selectedPlaylist?.name !== "Favorites" &&
              !selectedPlaylist?.sync_folder && (
                <button
                  className="btn-ghost"
                  onClick={handleClearPlaylist}
                  type="button"
                >
                  Clear
                </button>
              )}
          </div>
        </div>

        <section className="playlist-container">
          {playlist.length === 0 && isLoadingPlaylist ? (
            <div className="empty-state">
              <div className="empty-icon">
                <span className="import-spinner" />
              </div>
              <h2>Loading…</h2>
            </div>
          ) : playlist.length === 0 && isImporting ? (
            <div className="empty-state">
              <div className="empty-icon">
                <span className="import-spinner" />
              </div>
              <h2>
                Importing songs
                {importedCount > 0 ? ` (${importedCount} added)` : ""}…
              </h2>
              <p className="import-subtitle">
                Your songs will appear here as they are added.
              </p>
            </div>
          ) : playlist.length === 0 ? (
            <div className="empty-state">
              <div className="empty-icon">
                <BiMusic />
              </div>
              <h2>Your playlist is empty</h2>
              {!isLibraryPlaylistName(selectedPlaylist?.name) &&
                selectedPlaylist?.name !== "Favorites" && (
                  <button
                    className="btn-primary"
                    onClick={() => handleAddTrack(false)}
                    disabled={isAddingTracks}
                    type="button"
                  >
                    Add your first track
                  </button>
                )}
            </div>
          ) : (
            <div
              className="track-list"
              style={{ "--track-grid": trackGridCols } as React.CSSProperties}
            >
              <div className="track-list-header">
                <div
                  className="track-col-index sort-header"
                  onClick={() => handleSort("index")}
                >
                  #
                  {sortColumn === "index" && sortDirection !== "none"
                    ? sortDirection === "asc"
                      ? " ▲"
                      : " ▼"
                    : ""}
                </div>
                <div
                  className="track-title-cell sort-header"
                  onClick={() => handleSort("title")}
                >
                  Title
                  {sortColumn === "title" && sortDirection !== "none"
                    ? sortDirection === "asc"
                      ? " ▲"
                      : " ▼"
                    : ""}
                  <div
                    className="resize-handle"
                    onMouseDown={handleAlbumColResizeStart}
                    onClick={(e) => e.stopPropagation()}
                    title="Resize columns"
                    role="separator"
                    aria-orientation="vertical"
                    aria-label="Resize title and album columns"
                  />
                </div>
                <div
                  className="track-album sort-header"
                  onClick={() => handleSort("album")}
                >
                  Album
                  {sortColumn === "album" && sortDirection !== "none"
                    ? sortDirection === "asc"
                      ? " ▲"
                      : " ▼"
                    : ""}
                </div>
                <div className="track-duration track-duration-header">
                  Duration
                </div>
              </div>
              {sortedPlaylist.map((track) => (
                <div
                  key={track.id}
                  className={`track-item ${isCurrentTrack(track) ? "active" : ""}`}
                  onClick={() =>
                    handlePlayTrack(
                      sortedPlaylist.findIndex((t) => t.path === track.path),
                    )
                  }
                  onContextMenu={(event) => {
                    event.preventDefault();
                    event.stopPropagation();
                    openTrackContextMenu(track.path, {
                      top: event.clientY,
                      left: event.clientX,
                    });
                  }}
                >
                  <div className="track-col-index">
                    {isCurrentTrack(track) && playbackState.is_playing ? (
                      <span className="mini-bars">
                        <i />
                        <i />
                        <i />
                      </span>
                    ) : (
                      playlist.findIndex((t) => t.path === track.path) + 1
                    )}
                  </div>
                  <div className="track-title-cell">
                    <Artwork
                      track={track}
                      fallback={getTrackTitle(track).slice(0, 1).toUpperCase()}
                      className="track-thumb"
                    />
                    <div>
                      <div className="track-name">{getTrackTitle(track)}</div>
                      <div className="track-meta">
                        <button
                          className="track-meta-link"
                          onClick={(e) => {
                            e.stopPropagation();
                            setViewingArtist(track.artist);
                          }}
                          type="button"
                        >
                          {track.artist}
                        </button>
                        {track.lyrics ? " · lyrics" : ""}
                        {track.cover_art_source === "cover-art-archive"
                          ? " · online cover"
                          : ""}
                      </div>
                    </div>
                  </div>
                  <div
                    className="track-album"
                    onClick={(e) => {
                      e.stopPropagation();
                      setViewingAlbum({
                        name: track.album,
                        albumArtist: track.album_artist || track.artist,
                      });
                    }}
                  >
                    {track.album}
                  </div>
                  <div className="track-duration">
                    {formatTime(track.duration_seconds)}
                  </div>
                  <div className="track-actions-cell">
                    <div className="track-actions-hover">
                      <button
                        className="track-action-btn"
                        onClick={(event) => {
                          event.stopPropagation();
                          if (menuTrackPath === track.path) {
                            setMenuTrackPath(null);
                            setMenuAnchor(null);
                            setAddToPlaylistTrack(null);
                          } else {
                            const rect =
                              event.currentTarget.getBoundingClientRect();
                            openTrackContextMenu(track.path, {
                              top: rect.bottom + 4,
                              right: window.innerWidth - rect.right,
                            });
                          }
                        }}
                        title="More"
                        type="button"
                      >
                        <BiDotsHorizontalRounded />
                      </button>
                      {!isLibraryPlaylistName(selectedPlaylist?.name) && (
                        <button
                          className="track-action-btn track-remove-action"
                          onClick={(event) => {
                            event.stopPropagation();
                            void handleRemoveFromPlaylist(track.path);
                          }}
                          title="Remove from playlist"
                          type="button"
                        >
                          <BiMinus />
                        </button>
                      )}
                      <button
                        className="track-action-btn track-remove-action"
                        onClick={(event) => {
                          event.stopPropagation();
                          void handleRemoveFromLibrary(track.path);
                        }}
                        title="Remove from library"
                        type="button"
                      >
                        <BiTrash />
                      </button>
                    </div>
                    <button
                      className={`track-action-btn favorite-btn ${favoritePaths.has(track.path) ? "active" : ""}`}
                      onClick={(event) => {
                        event.stopPropagation();
                        handleToggleFavorite(track.path);
                      }}
                      title={
                        favoritePaths.has(track.path)
                          ? "Remove from Favorites"
                          : "Add to Favorites"
                      }
                      type="button"
                    >
                      {favoritePaths.has(track.path) ? (
                        <BiSolidHeart />
                      ) : (
                        <BiHeart />
                      )}
                    </button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </section>
      </main>
      )}

      {(rightPanelOpen || rightPanelClosing) && (
        <div
          className="drag-handle drag-handle-right"
          onMouseDown={onDragStart("right")}
        />
      )}

      <aside className="right-panel">
        {showQueue && (
          <div className="right-panel-content">
            <div className="right-panel-header">
              <h2>Queue</h2>
              <div className="right-panel-header-actions">
                {queueData.tracks.length > 0 && (
                  <button
                    className="btn-ghost btn-sm"
                    onClick={handleClearQueue}
                    type="button"
                  >
                    Clear
                  </button>
                )}
                <button
                  className="right-panel-close"
                  onClick={closeRightPanelDelayed}
                  type="button"
                  title="Close"
                >
                  <BiX />
                </button>
              </div>
            </div>
            <div className="right-panel-list">
              {queueData.tracks.length === 0 ? (
                <div className="queue-empty">
                  <p>Queue is empty</p>
                  <span>Add tracks with "Play Next" or "Add to Queue"</span>
                </div>
              ) : (
                queueData.tracks.map((track, index) => (
                  <div
                    key={`${track.path}-${index}`}
                    className={`queue-item ${queueData.current_index === index ? "active" : ""} ${queueMenuIndex === index ? "menu-open" : ""}`}
                    onClick={() => handlePlayFromQueue(index)}
                    onContextMenu={(event) => {
                      event.preventDefault();
                      event.stopPropagation();
                      openQueueContextMenu(index, {
                        top: event.clientY,
                        left: event.clientX,
                      });
                    }}
                  >
                    <Artwork
                      track={track}
                      fallback={getTrackTitle(track).slice(0, 1).toUpperCase()}
                      className="queue-thumb"
                    />
                    <div className="queue-item-info">
                      <div className="queue-item-name">
                        {getTrackTitle(track)}
                      </div>
                      <div className="queue-item-artist">{track.artist}</div>
                    </div>
                    <div className="queue-item-duration">
                      {formatTime(track.duration_seconds)}
                    </div>
                    <div className="queue-item-actions">
                      <button
                        className="queue-item-menu"
                        onClick={(event) => {
                          event.stopPropagation();
                          if (queueMenuIndex === index) {
                            setQueueMenuIndex(null);
                            setQueueMenuAnchor(null);
                          } else {
                            const rect =
                              event.currentTarget.getBoundingClientRect();
                            openQueueContextMenu(index, {
                              top: rect.bottom + 4,
                              right: window.innerWidth - rect.right,
                            });
                          }
                        }}
                        title="More"
                        type="button"
                      >
                        <BiDotsHorizontalRounded />
                      </button>
                      <button
                        className="queue-item-remove"
                        onClick={(e) => {
                          e.stopPropagation();
                          handleRemoveFromQueue(index);
                        }}
                        title="Remove from queue"
                        type="button"
                      >
                        <BiX />
                      </button>
                    </div>
                  </div>
                ))
              )}
            </div>
            <div className="queue-eq-mini">
              <div className="queue-mobile-transport">
                <label className="queue-mobile-volume">
                  <span>Volume</span>
                  <input
                    className="range-slider"
                    type="range"
                    min="0"
                    max="1"
                    step="0.01"
                    value={volumeValue}
                    onChange={(event) =>
                      handleVolume(Number(event.target.value))
                    }
                  />
                  <span>{Math.round(volumeValue * 100)}%</span>
                  <br />
                </label>
              </div>
              <div className="queue-eq-mini-header">
                <span>Equalizer</span>
                <label className="eq-enable">
                  <input
                    type="checkbox"
                    checked={eqSettings.enabled}
                    onChange={(event) => handleEqEnabled(event.target.checked)}
                  />
                  On
                </label>
                <select
                  className="eq-preset-select eq-preset-select-mini"
                  value=""
                  onChange={(event) => {
                    if (event.target.value)
                      void handleEqPreset(event.target.value);
                  }}
                  aria-label="EQ preset"
                >
                  <option value="" disabled>
                    Presets
                  </option>
                  {EQ_PRESETS.map((preset) => (
                    <option key={preset.id} value={preset.id}>
                      {preset.label}
                    </option>
                  ))}
                </select>
              </div>
              <div
                className={`queue-eq-mini-bands ${eqSettings.enabled ? "" : "disabled"}`}
              >
                {EQ_BAND_LABELS.map((label, index) => (
                  <div className="eq-band eq-band-mini" key={label}>
                    <input
                      type="range"
                      min={-12}
                      max={12}
                      step={0.5}
                      value={eqSettings.bands[index] ?? 0}
                      onChange={(event) =>
                        handleEqBandChange(index, Number(event.target.value))
                      }
                      aria-label={`${label} Hz`}
                      title={`${label} Hz: ${(eqSettings.bands[index] ?? 0).toFixed(1)} dB`}
                    />
                    <span className="eq-band-label">{label}</span>
                  </div>
                ))}
              </div>
            </div>
          </div>
        )}
        {lyricsPanelTrack && (
          <div className="right-panel-content lyrics-panel">
            <button
              className="right-panel-close lyrics-close-float"
              onClick={closeRightPanelDelayed}
              type="button"
              title="Close"
            >
              <BiX />
            </button>
            <div className="lyrics-panel-scroll">
              <div className="lyrics-panel-cover">
                <Artwork
                  track={lyricsPanelTrack}
                  fallback={getTrackTitle(lyricsPanelTrack)
                    .slice(0, 2)
                    .toUpperCase()}
                  className="lyrics-cover"
                />
              </div>
              <div className="lyrics-panel-header">
                <div className="right-panel-header">
                  <h2>{getTrackTitle(lyricsPanelTrack)}</h2>
                </div>
                {lyricsPanelTrack.artist && (
                  <p className="lyrics-artist">
                    by{" "}
                    <button
                      className="lyrics-link"
                      onClick={() => {
                        setViewingArtist(lyricsPanelTrack.artist);
                        closeRightPanelDelayed();
                      }}
                      type="button"
                    >
                      {lyricsPanelTrack.artist}
                    </button>
                  </p>
                )}
                {lyricsPanelTrack.album && (
                  <p className="lyrics-album">
                    From{" "}
                    <button
                      className="lyrics-link"
                      onClick={() => {
                        setViewingAlbum({
                          name: lyricsPanelTrack.album,
                          albumArtist:
                            lyricsPanelTrack.album_artist ||
                            lyricsPanelTrack.artist,
                        });
                        closeRightPanelDelayed();
                      }}
                      type="button"
                    >
                      {lyricsPanelTrack.album}
                    </button>
                  </p>
                )}
              </div>
              <div className="lyrics-panel-body">
                {timedLyrics ? (
                  <div className="lyrics-lines">
                    {timedLyrics.map((line, index) => (
                      <button
                        key={`${line.time}-${index}`}
                        ref={
                          index === activeLyricIndex ? activeLyricLineRef : null
                        }
                        type="button"
                        className={`lyrics-line ${index === activeLyricIndex ? "active" : ""}`}
                        onClick={() => {
                          if (!isLyricsPanelOnCurrentTrack) return;
                          void handleSeek(line.time);
                        }}
                        disabled={!isLyricsPanelOnCurrentTrack}
                        title={
                          isLyricsPanelOnCurrentTrack
                            ? "Jump to this line"
                            : undefined
                        }
                      >
                        {line.text || "\u00A0"}
                      </button>
                    ))}
                  </div>
                ) : lyricsPanelTrack.lyrics ? (
                  <pre>{lyricsPanelTrack.lyrics}</pre>
                ) : (
                  <p className="lyrics-empty">No lyrics available</p>
                )}
                {lyricsPanelTrack.lyrics && (
                  <p className="lyrics-source">
                    {lyricsPanelTrack.lyrics_source === "lrclib"
                      ? "Lyrics provided by LRCLIB"
                      : "Lyrics pulled from the file"}
                  </p>
                )}
              </div>
            </div>
          </div>
        )}
        {showDeviceList && (
          <div className="right-panel-content">
            <div className="right-panel-header">
              <h2>Audio Output</h2>
              <button
                className="right-panel-close"
                onClick={closeRightPanelDelayed}
                type="button"
                title="Close"
              >
                <BiX />
              </button>
            </div>
            <div className="right-panel-list">
              {outputDevices.map((name) => (
                <button
                  key={name}
                  className={`device-panel-item ${name === playbackState.output_device_name ? "active" : ""}`}
                  onClick={async () => {
                    try {
                      await setOutputDevice(name);
                      await updatePlaybackState();
                      setShowDeviceList(false);
                    } catch (err) {
                      setError(
                        err instanceof Error
                          ? err.message
                          : "Failed to change audio device",
                      );
                      setShowDeviceList(false);
                    }
                  }}
                  type="button"
                >
                  {name}
                </button>
              ))}
            </div>
          </div>
        )}
      </aside>

      {showAddTrackMenu &&
        addTrackMenuAnchor &&
        createPortal(
          <>
            <div
              className="context-menu-backdrop"
              onClick={() => {
                setShowAddTrackMenu(false);
                setAddTrackMenuAnchor(null);
              }}
            />
            <div
              className="add-track-menu"
              style={{
                position: "fixed",
                top: `${addTrackMenuAnchor.top}px`,
                left: `${addTrackMenuAnchor.left}px`,
              }}
              onClick={(e) => e.stopPropagation()}
            >
              <button
                type="button"
                onClick={() => {
                  void handleAddTrack(true);
                }}
              >
                <BiPlus /> Add files
              </button>
              {!androidHost && (
                <>
                  <button
                    type="button"
                    onClick={() => {
                      void handleAddFolder();
                    }}
                  >
                    <BiFolderOpen /> Add folder
                  </button>
                  <button
                    type="button"
                    onClick={() => {
                      void handleAddFolderAsPlaylist();
                    }}
                  >
                    <BiFolderOpen /> Add folder as playlist
                  </button>
                </>
              )}
              {androidHost && (
                <p className="add-track-menu-hint">
                  On Android, tap the + button to scan a music folder into
                  Library.
                </p>
              )}
            </div>
          </>,
          document.body,
        )}

      {menuTrackPath &&
        menuAnchor &&
        (() => {
          const menuTrack = playlist.find((t) => t.path === menuTrackPath);
          if (!menuTrack) return null;
          const addToPlaylistOptions = playlists.filter(
            (p) => p.id !== selectedPlaylistId && p.name !== "Favorites",
          );
          return createPortal(
            <div
              className="track-context-menu"
              style={{
                position: "fixed",
                top: `${menuAnchor.top}px`,
                ...(menuAnchor.left != null
                  ? { left: `${menuAnchor.left}px` }
                  : { right: `${menuAnchor.right ?? 0}px` }),
              }}
              onClick={(e) => e.stopPropagation()}
            >
              <button
                type="button"
                onClick={() => {
                  closeTrackContextMenu();
                  handlePlayNext(menuTrack.path);
                }}
              >
                <BiListPlus /> Play Next
              </button>
              <button
                type="button"
                onClick={() => {
                  closeTrackContextMenu();
                  handleAddToQueue(menuTrack.path);
                }}
              >
                <BiListPlus /> Add to Queue
              </button>
              {addToPlaylistOptions.length > 0 && (
                <button
                  type="button"
                  onClick={() => {
                    closeTrackContextMenu();
                    setAddToPlaylistTrack(menuTrack.path);
                  }}
                >
                  <BiListUl /> Add to Playlist...
                </button>
              )}
              {menuTrack.album && (
                <button
                  type="button"
                  onClick={() => {
                    closeTrackContextMenu();
                    setViewingAlbum({
                      name: menuTrack.album,
                      albumArtist: menuTrack.album_artist || menuTrack.artist,
                    });
                  }}
                >
                  <BiAlbum /> Go to Album
                </button>
              )}
              {menuTrack.artist && (
                <button
                  type="button"
                  onClick={() => {
                    closeTrackContextMenu();
                    setViewingArtist(menuTrack.artist);
                  }}
                >
                  <BiUser /> Go to Artist
                </button>
              )}
              {!isLibraryPlaylistName(selectedPlaylist?.name) && (
                <button
                  className="delete-action"
                  type="button"
                  onClick={() => {
                    closeTrackContextMenu();
                    void handleRemoveFromPlaylist(menuTrack.path);
                  }}
                >
                  <BiMinus /> Remove from Playlist
                </button>
              )}
              <button
                className="delete-action"
                type="button"
                onClick={() => {
                  closeTrackContextMenu();
                  void handleRemoveFromLibrary(menuTrack.path);
                }}
              >
                <BiTrash /> Remove from Library
              </button>
            </div>,
            document.body,
          );
        })()}

      {queueMenuIndex != null &&
        queueMenuAnchor &&
        createPortal(
          <div
            className="track-context-menu"
            style={{
              position: "fixed",
              top: `${queueMenuAnchor.top}px`,
              ...(queueMenuAnchor.left != null
                ? { left: `${queueMenuAnchor.left}px` }
                : { right: `${queueMenuAnchor.right ?? 0}px` }),
            }}
            onClick={(e) => e.stopPropagation()}
          >
            <button
              type="button"
              disabled={queueMenuIndex <= 0}
              onClick={() => {
                closeQueueContextMenu();
                handleMoveQueueTrack(queueMenuIndex, queueMenuIndex - 1);
              }}
            >
              <BiChevronUp /> Move Up
            </button>
            <button
              type="button"
              disabled={queueMenuIndex >= queueData.tracks.length - 1}
              onClick={() => {
                closeQueueContextMenu();
                handleMoveQueueTrack(queueMenuIndex, queueMenuIndex + 1);
              }}
            >
              <BiChevronDown /> Move Down
            </button>
            <button
              type="button"
              onClick={() => {
                const index = queueMenuIndex;
                closeQueueContextMenu();
                handleRemoveFromQueue(index);
              }}
            >
              <BiX /> Remove
            </button>
          </div>,
          document.body,
        )}

      {(menuTrackPath || queueMenuIndex != null) && (
        <div
          className="context-menu-backdrop"
          onClick={() => {
            setMenuTrackPath(null);
            setMenuAnchor(null);
            setAddToPlaylistTrack(null);
            setQueueMenuIndex(null);
            setQueueMenuAnchor(null);
          }}
          onContextMenu={(event) => {
            event.preventDefault();
            setMenuTrackPath(null);
            setMenuAnchor(null);
            setAddToPlaylistTrack(null);
            setQueueMenuIndex(null);
            setQueueMenuAnchor(null);
          }}
        />
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
              <h2>
                {playlistDialog.mode === "create"
                  ? "Create playlist"
                  : "Rename playlist"}
              </h2>
              <button
                className="modal-close-btn"
                onClick={closePlaylistDialog}
                type="button"
                title="Close"
              >
                <BiX />
              </button>
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
              {playlistDialog.mode === "create" && (
                <div className="playlist-sync-field">
                  <span className="modal-label">Sync with folder</span>
                  <p className="modal-hint">
                    Optional. Keep this playlist tied to a music folder
                    {androidHost ? " (media scan)" : ""}.
                  </p>
                  {playlistSyncFolder ? (
                    <div className="playlist-sync-selected">
                      <BiSync className="playlist-sync-icon" />
                      <span
                        className="playlist-sync-path"
                        title={playlistSyncFolder}
                      >
                        {getFileName(playlistSyncFolder)}
                      </span>
                      <button
                        type="button"
                        className="btn-ghost btn-sm"
                        onClick={() => setPlaylistSyncFolderInput(null)}
                      >
                        Clear
                      </button>
                    </div>
                  ) : (
                    <button
                      type="button"
                      className="btn-secondary playlist-sync-pick"
                      onClick={() => void pickPlaylistSyncFolder()}
                    >
                      <BiFolderOpen /> Choose folder
                    </button>
                  )}
                </div>
              )}
              {playlistDialogError && (
                <p className="modal-error">{playlistDialogError}</p>
              )}
              <div className="modal-actions">
                <button
                  className="btn-ghost"
                  onClick={closePlaylistDialog}
                  type="button"
                >
                  Cancel
                </button>
                <button className="btn-primary" type="submit">
                  {playlistDialog.mode === "create" ? "Create" : "Save"}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}

      {showClearConfirm && (
        <div
          className="modal-backdrop"
          onClick={() => setShowClearConfirm(false)}
        >
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
            <p className="confirm-text">
              This will remove all tracks from this playlist. The files on disk
              won't be affected.
            </p>
            <div className="modal-actions">
              <button
                className="btn-ghost"
                onClick={() => setShowClearConfirm(false)}
                type="button"
              >
                Cancel
              </button>
              <button
                className="btn-primary"
                onClick={confirmClearPlaylist}
                type="button"
              >
                Clear
              </button>
            </div>
          </div>
        </div>
      )}

      {deletePlaylistConfirm && (
        <div
          className="modal-backdrop"
          onClick={() => setDeletePlaylistConfirm(null)}
        >
          <div
            className="modal-dialog confirm-dialog"
            onClick={(event) => event.stopPropagation()}
            onKeyDown={(event) => {
              if (event.key === "Escape") setDeletePlaylistConfirm(null);
            }}
          >
            <div className="modal-header">
              <h2>Delete playlist?</h2>
            </div>
            <p className="confirm-text">
              This will permanently delete "{deletePlaylistConfirm.name}". This
              action cannot be undone.
            </p>
            <div className="modal-actions">
              <button
                className="btn-ghost"
                onClick={() => setDeletePlaylistConfirm(null)}
                type="button"
              >
                Cancel
              </button>
              <button
                className="btn-danger"
                onClick={confirmDeletePlaylist}
                type="button"
              >
                Delete
              </button>
            </div>
          </div>
        </div>
      )}

      {addToPlaylistTrack && (
        <div
          className="modal-backdrop"
          onClick={() => setAddToPlaylistTrack(null)}
        >
          <div
            className="modal-dialog"
            onClick={(event) => event.stopPropagation()}
            onKeyDown={(event) => {
              if (event.key === "Escape") setAddToPlaylistTrack(null);
            }}
          >
            <div className="modal-header">
              <h2>Add to playlist</h2>
              <button
                className="modal-close-btn"
                onClick={() => setAddToPlaylistTrack(null)}
                type="button"
              >
                <BiX />
              </button>
            </div>
            <div className="playlist-picker-list">
              {playlists
                .filter(
                  (p) => p.id !== selectedPlaylistId && p.name !== "Favorites",
                )
                .map((p) => (
                  <button
                    key={p.id}
                    className="playlist-picker-item"
                    type="button"
                    onClick={() =>
                      handleAddTrackToPlaylist(p.id, addToPlaylistTrack)
                    }
                  >
                    {p.name}
                  </button>
                ))}
            </div>
          </div>
        </div>
      )}

      <footer className="player-bar">
        <div className="player-left">
          <button
            className="album-art-btn"
            onClick={handleOpenLyrics}
            disabled={!currentTrack}
            type="button"
            title={currentTrack ? "Show lyrics" : undefined}
          >
            <Artwork
              track={currentTrack}
              fallback={coverLetters}
              className="album-art"
            />
          </button>
          <div className="now-playing-info">
            <button
              type="button"
              className="now-playing-name"
              onClick={handleOpenLyrics}
              disabled={!currentTrack}
              title={currentTrack ? "Show lyrics" : undefined}
            >
              {getTrackTitle(currentTrack, playbackState.current_path)}
            </button>
            <button
              className="now-playing-artist"
              onClick={() => {
                if (currentTrack?.artist) setViewingArtist(currentTrack.artist);
              }}
              type="button"
              disabled={!currentTrack?.artist}
            >
              {currentTrack?.artist ??
                (playbackState.current_path
                  ? "Local file"
                  : "No track selected")}
            </button>
            <button
              className="now-playing-path"
              onClick={() => {
                if (currentTrack?.album)
                  setViewingAlbum({
                    name: currentTrack.album,
                    albumArtist: currentTrack.album_artist || currentTrack.artist,
                  });
              }}
              type="button"
              disabled={!currentTrack?.album}
            >
              {currentTrack?.album ??
                playbackState.current_path ??
                "Add music to your playlist"}
            </button>
          </div>
        </div>

        <div className="player-controls">
          <button
            className={`control-btn shuffle-btn ${playbackMode.shuffle ? "active" : ""}`}
            onClick={handleToggleShuffle}
            type="button"
            title={playbackMode.shuffle ? "Disable shuffle" : "Enable shuffle"}
          >
            <BiShuffle />
          </button>
          <button
            className="control-btn"
            onClick={handlePrevious}
            disabled={!canSkip}
            type="button"
            title="Previous"
          >
            <BiSkipPrevious />
          </button>
          <button
            className="control-btn desktop-only-control"
            onClick={handleStop}
            disabled={!playbackState.current_path}
            type="button"
            title="Stop"
          >
            <BiStop />
          </button>
          <button
            className="control-btn play-pause-btn"
            onClick={handlePlayPause}
            type="button"
            title="Play/Pause"
          >
            {playbackState.is_playing ? <BiPause /> : <BiPlay />}
          </button>
          <button
            className="control-btn"
            onClick={handleNext}
            disabled={!canSkip}
            type="button"
            title="Next"
          >
            <BiSkipNext />
          </button>
          <button
            className={`control-btn repeat-btn ${playbackMode.repeat !== "off" ? "active" : ""} ${playbackMode.repeat === "one" ? "repeat-one" : ""}`}
            onClick={handleCycleRepeat}
            type="button"
            title={
              playbackMode.repeat === "off"
                ? "Repeat off"
                : playbackMode.repeat === "all"
                  ? "Repeat all"
                  : "Repeat one"
            }
          >
            <BiRepeat />
          </button>
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
            onPointerUp={(event) =>
              handleSeek(Number(event.currentTarget.value))
            }
          />
          <span>{formatTime(displayDuration)}</span>
        </div>

        <div className="player-right">
          <div className="player-right-row">
            {currentTrack?.lyrics && (
              <button
                className={`control-btn lyrics-btn ${lyricsPanelTrack ? "active" : ""}`}
                onClick={handleToggleLyrics}
                type="button"
                title="Toggle lyrics"
              >
                <BiMusic />
              </button>
            )}
            <button
              className={`control-btn queue-toggle desktop-queue-btn ${showQueue ? "active" : ""}`}
              onClick={handleToggleQueue}
              type="button"
              title="Toggle queue"
            >
              <BiListUl />
            </button>
            <span
              className={`status-dot ${playbackState.is_playing ? "playing" : playbackState.is_paused ? "paused" : ""}`}
            />
            <button
              ref={volumeIconRef}
              className={`volume-icon desktop-only-control ${showEqPanel ? "active" : ""} ${eqSettings.enabled ? "eq-on" : ""}`}
              onClick={handleToggleEqPanel}
              type="button"
              title="Equalizer"
              aria-label="Open equalizer"
            >
              {volumeValue === 0 ? (
                <BiVolumeMute />
              ) : volumeValue < 0.5 ? (
                <BiVolumeLow />
              ) : (
                <BiVolumeFull />
              )}
            </button>
            <input
              className="range-slider volume"
              type="range"
              min="0"
              max="1"
              step="0.01"
              value={volumeValue}
              onChange={(event) => handleVolume(Number(event.target.value))}
            />
            <span className="volume-percent">
              {Math.round(volumeValue * 100)}%
            </span>
          </div>
          <div className="device-selector">
            <button
              className="output-device-name"
              onClick={() => {
                listOutputDevices().then(setOutputDevices).catch(console.error);
                handleToggleDevice();
              }}
              title="Click to change audio output device"
              type="button"
            >
              {playbackState.output_device_name || "No device"}
            </button>
          </div>
        </div>
      </footer>

      {showEqPanel &&
        eqAnchor &&
        createPortal(
          <>
            <div
              className="context-menu-backdrop"
              onClick={() => {
                setShowEqPanel(false);
                setEqAnchor(null);
              }}
            />
            <div
              className="eq-panel"
              style={{
                position: "fixed",
                bottom: `${eqAnchor.bottom}px`,
                right: `${eqAnchor.right}px`,
              }}
              onClick={(e) => e.stopPropagation()}
              role="dialog"
              aria-label="Equalizer"
            >
              <div className="eq-panel-header">
                <h3>Equalizer</h3>
                <label className="eq-enable">
                  <input
                    type="checkbox"
                    checked={eqSettings.enabled}
                    onChange={(event) => handleEqEnabled(event.target.checked)}
                  />
                  On
                </label>
                <button
                  className="eq-close"
                  onClick={() => {
                    setShowEqPanel(false);
                    setEqAnchor(null);
                  }}
                  type="button"
                  title="Close"
                  aria-label="Close equalizer"
                >
                  <BiX />
                </button>
              </div>
              <div className="eq-panel-toolbar">
                <select
                  className="eq-preset-select"
                  value=""
                  onChange={(event) => {
                    if (event.target.value)
                      void handleEqPreset(event.target.value);
                  }}
                  aria-label="EQ preset"
                >
                  <option value="" disabled>
                    Presets
                  </option>
                  {EQ_PRESETS.map((preset) => (
                    <option key={preset.id} value={preset.id}>
                      {preset.label}
                    </option>
                  ))}
                </select>
                <button
                  className="btn-ghost btn-sm"
                  onClick={handleEqReset}
                  type="button"
                >
                  Reset
                </button>
              </div>
              <div
                className={`eq-bands ${eqSettings.enabled ? "" : "disabled"}`}
              >
                {EQ_BAND_LABELS.map((label, index) => (
                  <div className="eq-band" key={label}>
                    <span className="eq-band-gain">
                      {(eqSettings.bands[index] ?? 0) > 0 ? "+" : ""}
                      {(eqSettings.bands[index] ?? 0).toFixed(0)}
                    </span>
                    <input
                      type="range"
                      min={-12}
                      max={12}
                      step={0.5}
                      value={eqSettings.bands[index] ?? 0}
                      onChange={(event) =>
                        handleEqBandChange(index, Number(event.target.value))
                      }
                      aria-label={`${label} Hz`}
                      title={`${label} Hz`}
                    />
                    <span className="eq-band-label">{label}</span>
                  </div>
                ))}
              </div>
              <div className="eq-scale">
                <span>+12 dB</span>
                <span>0</span>
                <span>−12 dB</span>
              </div>
            </div>
          </>,
          document.body,
        )}

      {showFolderSetup && androidHost && (
        <div className="modal-backdrop" onClick={() => {}}>
          <div
            className="modal-dialog"
            onClick={(e) => e.stopPropagation()}
          >
            <h2>Welcome to Wave</h2>
            <p className="modal-desc">
              Select a folder containing your music to get started. Wave will
              scan it and import all supported audio files.
            </p>
            <div className="modal-actions">
              <button
                className="btn-ghost"
                onClick={() => void skipFolderSetup()}
                type="button"
              >
                Skip for now
              </button>
              <button
                className="btn-primary"
                onClick={() => void handleAddFolderAndroid()}
                type="button"
              >
                <BiFolderOpen /> Select Music Folder
              </button>
            </div>
          </div>
        </div>
      )}

      {error && (
        <div className="error-toast" role="alert" aria-live="assertive">
          {error}
          <button onClick={() => setError(null)} type="button">
            <BiX />
          </button>
        </div>
      )}

      {lyricsFetchPath && (
        <div
          className="loading-indicator lyrics-fetch-indicator"
          role="status"
          aria-live="polite"
        >
          <div className="spinner" /> Fetching
          <button
            className="loading-cancel-btn"
            onClick={cancelLyricsFetch}
            type="button"
          >
            Cancel
          </button>
        </div>
      )}

      {isLoading && (
        <div className="loading-indicator" role="status" aria-live="polite">
          <div className="spinner" /> Loading...
        </div>
      )}
    </div>
  );
}

export default App;
