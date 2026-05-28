import { useEffect, useMemo, useState } from "react";
import {
  addTrackToPlaylist,
  clearPlaylist,
  getFileName,
  getPlaybackState,
  getPlaylist,
  pauseTrack,
  playTrack,
  playTrackFromPlaylist,
  removeTrackFromPlaylist,
  resumeTrack,
  seekTrack,
  selectAudioFile,
  setPlayerVolume,
  stopTrack,
  type PlaybackState,
  type Track,
} from "./utils/player";
import "./App.css";

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

function App() {
  const [playbackState, setPlaybackState] = useState<PlaybackState>(emptyPlaybackState);
  const [playlist, setPlaylist] = useState<Track[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [seekValue, setSeekValue] = useState(0);
  const [volumeValue, setVolumeValue] = useState(0.8);

  const currentIndex = useMemo(
    () => playlist.findIndex((track) => track.path === playbackState.current_path),
    [playlist, playbackState.current_path]
  );
  const currentTrack = currentIndex >= 0 ? playlist[currentIndex] : null;
  const displayDuration = playbackState.duration_seconds ?? currentTrack?.duration_seconds ?? 0;
  const displayPosition = Math.min(seekValue, displayDuration || seekValue);

  const updatePlaybackState = async () => {
    const state = await getPlaybackState();
    setPlaybackState({ ...emptyPlaybackState, ...state });
    setVolumeValue(state.volume ?? 0.8);
    if (!document.body.classList.contains("is-seeking")) {
      setSeekValue(state.position_seconds ?? 0);
    }
    setError(null);
  };

  const loadPlaylist = async () => {
    const tracks = await getPlaylist();
    setPlaylist(tracks);
  };

  useEffect(() => {
    const initApp = async () => {
      await new Promise((resolve) => setTimeout(resolve, 300));
      try {
        await updatePlaybackState();
        await loadPlaylist();
      } catch (err: any) {
        if (err?.message?.includes("not available") || err?.message?.includes("undefined")) {
          setError("Tauri API not available. Run `npm run tauri dev` instead of plain Vite.");
        }
      }
    };

    initApp();
    const interval = setInterval(() => updatePlaybackState().catch(() => {}), 500);
    return () => clearInterval(interval);
  }, []);

  const handleAddTrack = async (multiple = false) => {
    try {
      setError(null);
      setIsLoading(true);
      const paths = await selectAudioFile(multiple);
      if (paths?.length) {
        let failCount = 0;
        for (const path of paths) {
          try {
            await addTrackToPlaylist(path);
          } catch {
            failCount++;
          }
        }
        if (failCount > 0) setError(`Failed to add ${failCount} track(s).`);
        await loadPlaylist();
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to add track");
    } finally {
      setIsLoading(false);
    }
  };

  const handleRemoveTrack = async (index: number) => {
    try {
      setError(null);
      await removeTrackFromPlaylist(index);
      await loadPlaylist();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to remove track");
    }
  };

  const handlePlayTrack = async (index: number) => {
    try {
      setError(null);
      setIsLoading(true);
      await playTrackFromPlaylist(index);
      await updatePlaybackState();
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
      } else if (playlist.length > 0) {
        await playTrackFromPlaylist(0);
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
    if (!playlist.length) return;
    const nextIndex = currentIndex > 0 ? currentIndex - 1 : playlist.length - 1;
    await handlePlayTrack(nextIndex);
  };

  const handleNext = async () => {
    if (!playlist.length) return;
    const nextIndex = currentIndex >= 0 ? (currentIndex + 1) % playlist.length : 0;
    await handlePlayTrack(nextIndex);
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
      await clearPlaylist();
      await loadPlaylist();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to clear playlist");
    } finally {
      setIsLoading(false);
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
          <button className="nav-item" type="button"><span>Your Library</span></button>
        </nav>
        <div className="library-card">
          <p>Local Files</p>
          <strong>{playlist.length} songs</strong>
          <button className="btn-pill" onClick={() => handleAddTrack(true)} disabled={isLoading} type="button">Add music</button>
        </div>
      </aside>

      <main className="main-content">
        <section className="hero-panel">
          <div className="hero-art">{coverLetters}</div>
          <div className="hero-copy">
            <p className="eyebrow">Playlist</p>
            <h1>Local Sessions</h1>
            <p>{playlist.length ? `${playlist.length} tracks queued from your files` : "Add songs to start listening"}</p>
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
                <div key={`${track.path}-${index}`} className={`track-item ${isCurrentTrack(track) ? "active" : ""}`} onClick={() => handlePlayTrack(index)}>
                  <div className="track-col-index">{isCurrentTrack(track) && playbackState.is_playing ? <span className="mini-bars"><i /><i /><i /></span> : index + 1}</div>
                  <div className="track-title-cell">
                    <div className="track-thumb">{getTrackTitle(track).slice(0, 1).toUpperCase()}</div>
                    <div>
                      <div className="track-name">{getTrackTitle(track)}</div>
                      <div className="track-meta">{track.artist} - {track.name}</div>
                    </div>
                  </div>
                  <div className="track-album">{track.album}</div>
                  <div className="track-format">{track.format}</div>
                  <div className="track-duration">{formatTime(track.duration_seconds)}</div>
                  <button className="track-action-btn" onClick={(event) => { event.stopPropagation(); handleRemoveTrack(index); }} title="Remove" type="button">x</button>
                </div>
              ))}
            </div>
          )}
        </section>
      </main>

      <footer className="player-bar">
        <div className="player-left">
          <div className="album-art">{coverLetters}</div>
          <div className="now-playing-info">
            <div className="now-playing-name">{getTrackTitle(currentTrack, playbackState.current_path)}</div>
            <div className="now-playing-artist">{currentTrack?.artist ?? (playbackState.current_path ? "Local file" : "No track selected")}</div>
            <div className="now-playing-path">{currentTrack?.album ?? playbackState.current_path ?? "Add music to your playlist"}</div>
          </div>
        </div>

        <div className="player-center">
          <div className="player-controls">
            <button className="control-btn" onClick={handlePrevious} disabled={!playlist.length} type="button" title="Previous">Prev</button>
            <button className="control-btn" onClick={handleStop} disabled={!playbackState.current_path} type="button" title="Stop">Stop</button>
            <button className="control-btn play-pause-btn" onClick={handlePlayPause} disabled={isLoading} type="button" title="Play/Pause">{playbackState.is_playing ? "||" : ">"}</button>
            <button className="control-btn" onClick={handleNext} disabled={!playlist.length} type="button" title="Next">Next</button>
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
