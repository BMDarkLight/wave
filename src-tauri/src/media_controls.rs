use souvlaki::{
    MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig,
};
use std::time::Duration;
use tauri::Manager;
use tauri::{AppHandle, Emitter};

// ── Public metadata struct (mirrors what the frontend sends) ─────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TrackMetadata {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration_seconds: Option<f64>,
    pub cover_url: Option<String>,
}

// ── MediaBridge ───────────────────────────────────────────────────────────────

/// Owns the `souvlaki::MediaControls` instance and keeps it in sync with the
/// player state.  Created once during app setup and stored as Tauri state.
pub struct MediaBridge {
    controls: MediaControls,
}

// souvlaki's MediaControls is not Send on macOS (ObjC pointers), but we always
// access it through a Mutex, so this is safe.
unsafe impl Send for MediaBridge {}

impl MediaBridge {
    /// Create the bridge and attach OS event → Tauri event forwarding.
    pub fn new(app: &AppHandle) -> Result<Self, String> {
        #[cfg(target_os = "windows")]
        let hwnd = {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            let window = app
                .get_webview_window("main")
                .ok_or("Main window not found")?;
            match window
                .window_handle()
                .map_err(|e| format!("Failed to get window handle: {e}"))?
                .as_raw()
            {
                RawWindowHandle::Win32(h) => Some(h.hwnd.get() as *mut std::ffi::c_void),
                _ => None,
            }
        };

        #[cfg(not(target_os = "windows"))]
        let hwnd = None;

        let config = PlatformConfig {
            dbus_name: "wave",
            display_name: "Wave",
            hwnd,
        };

        let mut controls = MediaControls::new(config)
            .map_err(|e| format!("Failed to init media controls: {e:?}"))?;

        // Forward OS media control events as Tauri events.
        let app_handle = app.clone();
        controls
            .attach(move |event: MediaControlEvent| {
                let event_name = match event {
                    MediaControlEvent::Play => "media-control-play",
                    MediaControlEvent::Pause => "media-control-pause",
                    MediaControlEvent::Toggle => "media-control-toggle",
                    MediaControlEvent::Next => "media-control-next",
                    MediaControlEvent::Previous => "media-control-previous",
                    MediaControlEvent::Stop => "media-control-stop",
                    MediaControlEvent::Seek(dir) => {
                        let payload = format!("{:?}", dir).to_lowercase();
                        let _ = app_handle.emit("media-control-seek-relative", payload);
                        return;
                    }
                    MediaControlEvent::SeekBy(dir, dur) => {
                        use souvlaki::SeekDirection;
                        let secs = match dir {
                            SeekDirection::Forward => dur.as_secs_f64(),
                            SeekDirection::Backward => -dur.as_secs_f64(),
                        };
                        let _ = app_handle.emit("media-control-seek-by", secs);
                        return;
                    }
                    MediaControlEvent::SetPosition(pos) => {
                        let _ = app_handle.emit("media-control-set-position", pos.0.as_secs_f64());
                        return;
                    }
                    MediaControlEvent::OpenUri(_)
                    | MediaControlEvent::Raise
                    | MediaControlEvent::Quit
                    | MediaControlEvent::SetVolume(_) => return,
                };
                let _ = app_handle.emit(event_name, ());
            })
            .map_err(|e| format!("Failed to attach media controls handler: {e:?}"))?;

        Ok(Self { controls })
    }

    pub fn set_playback(&mut self, playback: MediaPlayback) {
        if let Err(error) = self.controls.set_playback(playback) {
            tracing::debug!("Failed to update OS media playback state: {error:?}");
        }
    }

    fn set_playback_state(&mut self, position_secs: f64, playing: bool) {
        let pos = MediaPosition(Duration::from_secs_f64(position_secs));
        let playback = if playing {
            MediaPlayback::Playing {
                progress: Some(pos),
            }
        } else {
            MediaPlayback::Paused {
                progress: Some(pos),
            }
        };
        self.set_playback(playback);
    }

    // ── Playback state ────────────────────────────────────────────────────────

    pub fn set_playing(&mut self, position_secs: f64) {
        self.set_playback_state(position_secs, true);
    }

    pub fn set_paused(&mut self, position_secs: f64) {
        self.set_playback_state(position_secs, false);
    }

    pub fn set_stopped(&mut self) {
        self.set_playback(MediaPlayback::Stopped);
    }

    // ── Metadata ──────────────────────────────────────────────────────────────

    pub fn set_metadata(&mut self, meta: &TrackMetadata) {
        let duration = meta.duration_seconds.map(Duration::from_secs_f64);
        if let Err(error) = self.controls.set_metadata(MediaMetadata {
            title: meta.title.as_deref(),
            artist: meta.artist.as_deref(),
            album: meta.album.as_deref(),
            duration,
            cover_url: meta.cover_url.as_deref(),
        }) {
            tracing::debug!("Failed to update OS media metadata: {error:?}");
        }
    }

    /// Convenience: set metadata and immediately mark as playing.
    pub fn now_playing(&mut self, meta: &TrackMetadata) {
        self.set_metadata(meta);
        self.set_playing(0.0);
    }

    // ── Position tick (call periodically while playing) ───────────────────────

    pub fn update_position(&mut self, position_secs: f64, playing: bool) {
        self.set_playback_state(position_secs, playing);
    }
}
