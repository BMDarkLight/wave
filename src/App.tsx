// The Code for Frontend of Wave is currently completely AI Generated and may contain bugs or rough edges. Please report any issues you encounter at

import { useEffect, useMemo, useRef, useState } from "react";
import {
  addTrackToPlaylistById,
  addToQueue,
  clearPlaylistById,
  clearQueue,
  createPlaylist,
  deletePlaylist,
  exportPlaylist,
  getFileName,
  getPlaybackState,
  getPlaylistTracksById,
  getQueueTracks,
  importPlaylist,
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
  resumeTrack,
  savePlaylistDialog,
  seekTrack,
  selectAudioFile,
  setPlayerVolume,
  stopTrack,
  updateMediaMetadata,
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
  const [seekValue, setSeekValue] = useState(0);
  const [volumeValue, setVolumeValue] = useState(0.8);

  // Playlist management
  const [playlists, setPlaylists] = useState<PlaylistInfo[]>([]);
  const [selectedPlaylistId, setSelectedPlaylistId] = useState<string | null>(null);

  // Queue panel
  const [queueData, setQueueData] = useState<QueueTrackState>({ tracks: [], current_index: null, is_shuffled: false });
  const [showQueue, setShowQueue] = useState(false);

  // Track context menu
  const [menuTrackPath, setMenuTrackPath] = useState<string | null>(null);
  const [showAddToPlaylist, setShowAddToPlaylist] = useState(false);

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
  };

  const loadPlaylists = async () => {
    const list = await listPlaylists();
    setPlaylists(list);
    return list;
  };

  const loadPlaylistTracks = async (playlistId: string) => {
    const tracks = await getPlaylistTracksById(playlistId);
    setPlaylist(tracks);
  };

  const loadQueueTracks = async () => {
    const data = await getQueueTracks();
    setQueueData(data);
  };

  // Resolve the default playlist ID from the playlists list.
  const getDefaultPlaylistId = (list: PlaylistInfo[]): string | null => {
    return (list.find((p) => p.name === "Local Sessions") ?? list[0])?.id ?? null;
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
      } catch (err: any) {
        if (err?.message?.includes("not available") || err?.message?.includes("undefined")) {
          setError("Tauri API not available. Run `npm run tauri dev` instead of plain Vite.");
        }
      }
    };

    initApp();
    const interval = setInterval(() => updatePlaybackState().catch(() => {}), 500);
    const queueInterval = setInterval(() => loadQueueTracks().catch(() => {}), 2000);
    return () => {
      clearInterval(interval);
      clearInterval(queueInterval);
    };
  }, []);

  // Auto-advance when a track finishes naturally
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
      playNext()
        .then(() => updatePlaybackState())
        .then(() => loadQueueTracks())
        .catch(console.error);
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

  // Push track metadata to OS media controls (Control Center, SMTC, MPRIS)
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
  }, [currentTrack, playbackState.is_playing]);

  // Listen for OS media control events (play/pause/next/prev/seek from OS)
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    const setup = async () => {
      unlisten = await listenToMediaControls({
        onPlay: () => {
          if (!playbackState.is_playing) resumeTrack().catch(console.error);
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
      setIsLoading(true);
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
      setIsLoading(false);
    }
  };

  const handleRemoveTrack = async (path: string) => {
    try {
      setError(null);
      if (!selectedPlaylistId) return;
      await removeTrackFromPlaylistById(selectedPlaylistId, path);
      await loadPlaylistTracks(selectedPlaylistId);
      await loadPlaylists();
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

  const handlePlayPause = async () => {
    try {
      setError(null);
      setIsLoading(true);
      if (playbackState.is_playing) {
        await pauseTrack();
      } else if (playbackState.is_paused) {
        await resumeTrack();
      } else if (playbackState.current_path) {
        await playTrack(playbackState.current_path);
      } else if (hasActiveQueue) {
        const startIndex = queueData.current_index ?? 0;
        await playTrackFromQueue(startIndex);
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
    if (!confirm("Clear the entire playlist?")) return;
    try {
      setError(null);
      setIsLoading(true);
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

  const handleCreatePlaylist = async () => {
    const name = prompt("Enter playlist name:");
    if (!name?.trim()) return;
    try {
      setError(null);
      const info = await createPlaylist(name.trim());
      await loadPlaylists();
      setSelectedPlaylistId(info.id);
      await loadPlaylistTracks(info.id);
    } catch (err) {
      setError(formatInvokeError(err, "Failed to create playlist"));
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

  const handleExportPlaylist = async () => {
    if (!selectedPlaylistId) return;
    try {
      setError(null);
      const name = selectedPlaylist?.name ?? "playlist";
      const path = await savePlaylistDialog(name);
      if (!path) return;
      const format = path.toLowerCase().endsWith(".json") ? "json" : "m3u";
      await exportPlaylist(selectedPlaylistId, path, format);
    } catch (err) {
      setError(formatInvokeError(err, "Failed to export playlist"));
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
        <div className="brand-mark"><span>W</span> Wave</div>
        <nav className="sidebar-nav">
          <button className="nav-item active" type="button"><span>Home</span></button>
          <button
            className={`nav-item ${showQueue ? "active" : ""}`}
            type="button"
            onClick={() => setShowQueue((v) => !v)}
          >
            <span>Queue{queueData.tracks.length > 0 ? ` (${queueData.tracks.length})` : ""}</span>
          </button>
        </nav>
        <div className="playlist-section">
          <div className="playlist-section-header">
            <p>Playlists</p>
            <button className="playlist-add-btn" onClick={handleCreatePlaylist} type="button" title="Create playlist">+</button>
          </div>
          <div className="playlist-list">
            {playlists.map((pl) => (
              <div
                key={pl.id}
                className={`playlist-item ${selectedPlaylistId === pl.id ? "active" : ""}`}
                onClick={() => handleSelectPlaylist(pl.id)}
              >
                <span className="playlist-item-name">{pl.name}</span>
                <span className="playlist-item-count">{pl.track_count}</span>
                {pl.name !== "Local Sessions" && (
                  <button
                    className="playlist-delete-btn"
                    onClick={(e) => { e.stopPropagation(); handleDeletePlaylist(pl.id); }}
                    title="Delete playlist"
                    type="button"
                  >x</button>
                )}
              </div>
            ))}
          </div>
          <div className="playlist-actions">
            <button className="btn-pill" onClick={() => handleAddTrack(true)} disabled={isLoading} type="button">Add music</button>
            <div className="playlist-io">
              <button className="btn-ghost btn-sm" onClick={handleExportPlaylist} disabled={!selectedPlaylistId} type="button">Export</button>
              <button className="btn-ghost btn-sm" onClick={handleImportPlaylist} type="button">Import</button>
            </div>
          </div>
        </div>
      </aside>

      <main className="main-content">
        <section className="hero-panel">
          <Artwork track={currentTrack} fallback={coverLetters} className="hero-art" />
          <div className="hero-copy">
            <p className="eyebrow">Playlist</p>
            <h1>{selectedPlaylist?.name ?? "Local Sessions"}</h1>
            <p>{playlist.length ? `${playlist.length} tracks in this playlist` : "Add songs to start listening"}</p>
            <div className="hero-actions">
              <button className="big-play" onClick={handlePlayPause} disabled={isLoading} type="button" title="Play or pause">
                {playbackState.is_playing ? "Pause" : "Play"}
              </button>
              <button className="btn-secondary" onClick={() => handleAddTrack(true)} disabled={isLoading} type="button">Add tracks</button>
              {playlist.length > 0 && <button className="btn-ghost" onClick={handleClearPlaylist} type="button">Clear</button>}
            </div>
          </div>
        </section>

        {error && <div className="error-banner"><span>{error}</span><button onClick={() => setError(null)} type="button">x</button></div>}
        {isLoading && <div className="loading-indicator"><div className="spinner" /> Loading...</div>}

        <section className="playlist-container">
          {playlist.length === 0 ? (
            <div className="empty-state">
              <div className="empty-icon">music</div>
              <h2>Your playlist is empty</h2>
              <p>Drop in MP3, WAV, FLAC, AAC, OGG, M4A, OPUS, or MKA files.</p>
              <button className="btn-primary" onClick={() => handleAddTrack(false)} disabled={isLoading} type="button">Add your first track</button>
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
                    <button className="track-action-btn" onClick={(event) => { event.stopPropagation(); setMenuTrackPath(menuTrackPath === track.path ? null : track.path); }} title="More" type="button">...</button>
                    <button className="track-action-btn" onClick={(event) => { event.stopPropagation(); handleRemoveTrack(track.path); }} title="Remove" type="button">x</button>
                    {menuTrackPath === track.path && (
                      <div className="track-context-menu" onClick={(e) => e.stopPropagation()}>
                        <button type="button" onClick={() => handlePlayNext(track.path)}>Play Next</button>
                        <button type="button" onClick={() => handleAddToQueue(track.path)}>Add to Queue</button>
                        <button type="button" onClick={() => setShowAddToPlaylist(true)}>Add to Playlist...</button>
                        {showAddToPlaylist && (
                          <div className="add-to-playlist-submenu">
                            {playlists.filter((p) => p.id !== selectedPlaylistId).map((p) => (
                              <button key={p.id} type="button" onClick={() => handleAddTrackToPlaylist(p.id, track.path)}>
                                {p.name}
                              </button>
                            ))}
                          </div>
                        )}
                      </div>
                    )}
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

      {menuTrackPath && (
        <div className="context-menu-backdrop" onClick={() => { setMenuTrackPath(null); setShowAddToPlaylist(false); }} />
      )}

      {showQueue && (
        <div className="queue-panel">
          <div className="queue-header">
            <h2>Queue</h2>
            <div className="queue-header-actions">
              {queueData.tracks.length > 0 && (
                <button className="btn-ghost btn-sm" onClick={handleClearQueue} type="button">Clear</button>
              )}
              <button className="queue-close-btn" onClick={() => setShowQueue(false)} type="button" title="Close">x</button>
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
                  >x</button>
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
            <button className="control-btn" onClick={handlePrevious} disabled={!canSkip} type="button" title="Previous">Prev</button>
            <button className="control-btn" onClick={handleStop} disabled={!playbackState.current_path} type="button" title="Stop">Stop</button>
            <button className="control-btn play-pause-btn" onClick={handlePlayPause} disabled={isLoading} type="button" title="Play/Pause">{playbackState.is_playing ? "||" : ">"}</button>
            <button className="control-btn" onClick={handleNext} disabled={!canSkip} type="button" title="Next">Next</button>
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
          <button
            className={`control-btn queue-toggle ${showQueue ? "active" : ""}`}
            onClick={() => setShowQueue((v) => !v)}
            type="button"
            title="Toggle queue"
          >Queue</button>
          <span className={`status-dot ${playbackState.is_playing ? "playing" : playbackState.is_paused ? "paused" : ""}`} />
          <span className="volume-icon">Vol</span>
          <input className="range-slider volume" type="range" min="0" max="1" step="0.01" value={volumeValue} onChange={(event) => handleVolume(Number(event.target.value))} />
          <span className="volume-percent">{Math.round(volumeValue * 100)}%</span>
        </div>
      </footer>
    </div>
  );
}

export default App;
