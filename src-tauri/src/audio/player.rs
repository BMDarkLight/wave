use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::time::{Duration, Instant};

// ── Playback modes ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepeatMode {
    Off,
    One,
    All,
}

impl Default for RepeatMode {
    fn default() -> Self {
        Self::Off
    }
}

// ── Playback clock ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct PlaybackClock {
    started_at: Option<Instant>,
    elapsed_before_start: Duration,
    duration: Option<Duration>,
}

impl PlaybackClock {
    fn stopped() -> Self {
        Self {
            started_at: None,
            elapsed_before_start: Duration::ZERO,
            duration: None,
        }
    }

    fn position(&self) -> Duration {
        let elapsed = self
            .started_at
            .map(|started_at| self.elapsed_before_start + started_at.elapsed())
            .unwrap_or(self.elapsed_before_start);

        self.duration
            .map(|duration| elapsed.min(duration))
            .unwrap_or(elapsed)
    }
}

// ── Queue ─────────────────────────────────────────────────────────────────────

/// In-memory playback queue (separate from the library's persisted playlist).
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct Queue {
    tracks: Vec<String>,
    current_index: Option<usize>,
    shuffle_order: Option<Vec<usize>>,
    shuffle_pos: usize,
}

impl Queue {
    pub fn set_tracks(&mut self, tracks: Vec<String>) {
        self.tracks = tracks;
        self.current_index = None;
        self.shuffle_order = None;
        self.shuffle_pos = 0;
    }

    /// Rebuild the Fisher-Yates shuffle order when shuffle is enabled.
    fn rebuild_shuffle_order(&mut self) {
        if self.tracks.is_empty() {
            self.shuffle_order = None;
            return;
        }
        let mut order: Vec<usize> = (0..self.tracks.len()).collect();
        // Simple deterministic shuffle using indices (no external RNG dep needed).
        // Uses a linear-congruential-style permutation seeded from the current time.
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(42) as usize;
        for i in (1..order.len()).rev() {
            let j = (seed.wrapping_mul(i + 1).wrapping_add(seed)) % (i + 1);
            order.swap(i, j);
        }
        // Place the current track first in the shuffle order so it plays without interruption.
        if let Some(idx) = self.current_index {
            if let Some(pos) = order.iter().position(|&v| v == idx) {
                order.swap(0, pos);
            }
        }
        self.shuffle_pos = if self.current_index.is_some() { 0 } else { 0 };
        self.shuffle_order = Some(order);
    }

    pub fn current_path(&self) -> Option<&str> {
        let idx = self.current_index?;
        self.tracks.get(idx).map(String::as_str)
    }

    pub fn current_index(&self) -> Option<usize> {
        self.current_index
    }

    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    pub fn tracks(&self) -> &[String] {
        &self.tracks
    }

    /// Returns the track at the given raw (unshuffled) index.
    pub fn track_at(&self, index: usize) -> Option<&str> {
        self.tracks.get(index).map(String::as_str)
    }

    /// Jump to a specific raw index and return its path.
    pub fn jump(&mut self, index: usize) -> Option<&str> {
        if index >= self.tracks.len() {
            return None;
        }
        self.current_index = Some(index);
        if let Some(ref mut order) = self.shuffle_order {
            if let Some(pos) = order.iter().position(|&v| v == index) {
                self.shuffle_pos = pos;
            }
        }
        self.tracks.get(index).map(String::as_str)
    }

    /// Advance to the next track. Returns `None` when the queue is exhausted
    /// and `repeat == Off`. With `repeat == All` it wraps around.
    pub fn next(&mut self, repeat: &RepeatMode) -> Option<&str> {
        if self.tracks.is_empty() {
            return None;
        }
        let next_idx = if let Some(ref order) = self.shuffle_order.clone() {
            let next_pos = self.shuffle_pos + 1;
            match repeat {
                RepeatMode::All => {
                    self.shuffle_pos = next_pos % order.len();
                    Some(order[self.shuffle_pos])
                }
                RepeatMode::Off | RepeatMode::One => {
                    if next_pos < order.len() {
                        self.shuffle_pos = next_pos;
                        Some(order[self.shuffle_pos])
                    } else {
                        None
                    }
                }
            }
        } else {
            let current = self.current_index.unwrap_or(0);
            let next = current + 1;
            match repeat {
                RepeatMode::All => Some(next % self.tracks.len()),
                RepeatMode::Off | RepeatMode::One => {
                    if next < self.tracks.len() {
                        Some(next)
                    } else {
                        None
                    }
                }
            }
        };

        self.current_index = next_idx;
        next_idx.and_then(|i| self.tracks.get(i).map(String::as_str))
    }

    /// Go back to the previous track.
    pub fn previous(&mut self, repeat: &RepeatMode) -> Option<&str> {
        if self.tracks.is_empty() {
            return None;
        }
        let prev_idx = if let Some(ref order) = self.shuffle_order.clone() {
            match (self.shuffle_pos, repeat) {
                (0, RepeatMode::All) => {
                    self.shuffle_pos = order.len() - 1;
                    Some(order[self.shuffle_pos])
                }
                (0, _) => None,
                (pos, _) => {
                    self.shuffle_pos = pos - 1;
                    Some(order[self.shuffle_pos])
                }
            }
        } else {
            let current = self.current_index.unwrap_or(0);
            match (current, repeat) {
                (0, RepeatMode::All) => Some(self.tracks.len() - 1),
                (0, _) => None,
                (idx, _) => Some(idx - 1),
            }
        };

        self.current_index = prev_idx;
        prev_idx.and_then(|i| self.tracks.get(i).map(String::as_str))
    }

    pub fn set_shuffle(&mut self, enabled: bool) {
        if enabled {
            self.rebuild_shuffle_order();
        } else {
            self.shuffle_order = None;
        }
    }

    pub fn is_shuffled(&self) -> bool {
        self.shuffle_order.is_some()
    }
}

// ── AudioPlayer ───────────────────────────────────────────────────────────────

/// Wrapper that makes `OutputStream` safe to put in a `Mutex<AudioPlayer>`.
///
/// `rodio::OutputStream` (and the underlying `cpal::Stream`) is `!Send` because
/// CoreAudio's stream handle contains a raw pointer to a callback. In practice it
/// is only ever accessed through the `Mutex` guard – never concurrently – so the
/// unsafety is sound as long as we never send the player to another thread without
/// the lock.
#[allow(dead_code)] // Field is held for its drop side-effect (silences audio on drop).
struct SendableStream(OutputStream);

// SAFETY: AudioPlayer is always accessed through a Mutex; the OutputStream is
// never accessed concurrently from multiple threads.
unsafe impl Send for SendableStream {}

pub struct AudioPlayer {
    /// The OutputStream must be kept alive for the duration of the player.
    /// Dropping it silences all audio.
    _stream: SendableStream,
    handle: OutputStreamHandle,
    sink: Option<Sink>,
    current_path: Option<PathBuf>,
    clock: PlaybackClock,
    volume: f32,
    pub queue: Queue,
    pub repeat: RepeatMode,
}

impl AudioPlayer {
    pub fn new() -> Result<Self, String> {
        let (stream, handle) = OutputStream::try_default()
            .map_err(|e| format!("Failed to create output stream: {}", e))?;

        Ok(Self {
            _stream: SendableStream(stream),
            handle,
            sink: None,
            current_path: None,
            clock: PlaybackClock::stopped(),
            volume: 0.8,
            queue: Queue::default(),
            repeat: RepeatMode::default(),
        })
    }

    fn build_source(
        path: &str,
    ) -> Result<(impl Source<Item = f32> + Send + 'static, Option<Duration>), String> {
        let file = File::open(path).map_err(|e| format!("Failed to open file: {e}"))?;
        let source = Decoder::new(BufReader::new(file))
            .map_err(|e| format!("Failed to decode audio: {e}"))?;
        let duration = source.total_duration();
        Ok((source.convert_samples(), duration))
    }

    fn play_from(&mut self, path: &str, should_play: bool) -> Result<(), String> {
        // Stop and drop the previous sink.
        if let Some(sink) = self.sink.take() {
            sink.stop();
        }

        let (source, duration) = Self::build_source(path)?;
        let sink = Sink::try_new(&self.handle)
            .map_err(|e| format!("Failed to create sink: {e}"))?;
        sink.set_volume(self.volume);
        sink.append(source);

        if should_play {
            sink.play();
        } else {
            sink.pause();
        }

        self.sink = Some(sink);
        self.current_path = Some(PathBuf::from(path));
        self.clock = PlaybackClock {
            started_at: should_play.then(Instant::now),
            elapsed_before_start: Duration::ZERO,
            duration,
        };

        Ok(())
    }

    pub fn play(&mut self, path: &str) -> Result<(), String> {
        self.play_from(path, true)
    }

    pub fn pause(&mut self) -> Result<(), String> {
        match &self.sink {
            Some(sink) => {
                sink.pause();
                self.clock.elapsed_before_start = self.clock.position();
                self.clock.started_at = None;
                Ok(())
            }
            None => Err("No track is currently playing".to_string()),
        }
    }

    pub fn resume(&mut self) -> Result<(), String> {
        match &self.sink {
            Some(sink) => {
                sink.play();
                self.clock.started_at = Some(Instant::now());
                Ok(())
            }
            None => Err("No track is currently paused".to_string()),
        }
    }

    pub fn stop(&mut self) -> Result<(), String> {
        if let Some(sink) = self.sink.take() {
            sink.stop();
        }
        self.current_path = None;
        self.clock = PlaybackClock::stopped();
        Ok(())
    }

    pub fn seek(&mut self, seconds: f64) -> Result<(), String> {
        let offset = Duration::from_secs_f64(seconds.max(0.0));
        
        // Use rodio 0.19's native seek — instant, no re-decoding.
        match &self.sink {
            Some(sink) => {
                sink.try_seek(offset)
                    .map_err(|e| format!("Seek failed: {e}"))?;
                
                // Update our clock to match.
                let was_playing = self.is_playing();
                self.clock.elapsed_before_start = offset;
                self.clock.started_at = was_playing.then(Instant::now);
                
                Ok(())
            }
            None => Err("No track loaded".to_string()),
        }
    }

    pub fn set_volume(&mut self, volume: f32) -> Result<(), String> {
        let clamped = volume.clamp(0.0, 1.0);
        self.volume = clamped;
        if let Some(ref sink) = self.sink {
            sink.set_volume(clamped);
        }
        Ok(())
    }

    pub fn is_playing(&self) -> bool {
        match &self.sink {
            Some(sink) => !sink.is_paused() && sink.len() > 0,
            None => false,
        }
    }

    pub fn is_paused(&self) -> bool {
        match &self.sink {
            Some(sink) => sink.is_paused(),
            None => false,
        }
    }

    pub fn get_current_path(&self) -> Option<&PathBuf> {
        self.current_path.as_ref()
    }

    pub fn position_seconds(&self) -> f64 {
        self.clock.position().as_secs_f64()
    }

    pub fn duration_seconds(&self) -> Option<f64> {
        self.clock.duration.map(|d| d.as_secs_f64())
    }

    pub fn volume(&self) -> f32 {
        self.volume
    }

    // ── Queue / mode helpers ──────────────────────────────────────────────────

    pub fn play_next(&mut self) -> Result<Option<String>, String> {
        if self.repeat == RepeatMode::One {
            // Replay current track.
            if let Some(path) = self.current_path.clone() {
                let path_str = path
                    .to_str()
                    .ok_or("Invalid path encoding")?
                    .to_string();
                self.play(&path_str)?;
                return Ok(Some(path_str));
            }
        }
        let path = self.queue.next(&self.repeat).map(str::to_string);
        if let Some(ref p) = path {
            self.play(p)?;
        }
        Ok(path)
    }

    pub fn play_previous(&mut self) -> Result<Option<String>, String> {
        // If we're more than 3 seconds into a track, rewind instead of going back.
        if self.position_seconds() > 3.0 {
            self.seek(0.0)?;
            return Ok(self
                .current_path
                .as_ref()
                .and_then(|p| p.to_str().map(str::to_string)));
        }
        let path = self.queue.previous(&self.repeat).map(str::to_string);
        if let Some(ref p) = path {
            self.play(p)?;
        }
        Ok(path)
    }
}
