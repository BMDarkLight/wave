use rodio::{OutputStream, OutputStreamHandle, Sink, Source};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::error::AudioError;

use super::symphonia_source::SymphoniaSource;

// ── Playback modes ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepeatMode {
    #[default]
    Off,
    One,
    All,
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
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(42) as usize;
        for i in (1..order.len()).rev() {
            let j = (seed.wrapping_mul(i + 1).wrapping_add(seed)) % (i + 1);
            order.swap(i, j);
        }
        if let Some(idx) = self.current_index {
            if let Some(pos) = order.iter().position(|&v| v == idx) {
                order.swap(0, pos);
            }
        }
        self.shuffle_pos = 0;
        self.shuffle_order = Some(order);
    }

    pub fn current_index(&self) -> Option<usize> {
        self.current_index
    }

    pub fn tracks(&self) -> &[String] {
        &self.tracks
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
            match self.current_index {
                None if !self.tracks.is_empty() => Some(0),
                None => None,
                Some(current) => {
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

    /// Append a track to the end of the queue.
    pub fn enqueue(&mut self, path: String) {
        let new_idx = self.tracks.len();
        self.tracks.push(path);
        if let Some(ref mut order) = self.shuffle_order {
            order.push(new_idx);
        }
    }

    /// Insert a track so it plays immediately after the current track.
    /// If nothing is playing, it is appended.
    pub fn insert_next(&mut self, path: String) {
        let insert_at = match self.current_index {
            Some(idx) => (idx + 1).min(self.tracks.len()),
            None => self.tracks.len(),
        };
        self.tracks.insert(insert_at, path.clone());
        if let Some(ref mut order) = self.shuffle_order {
            for idx in order.iter_mut() {
                if *idx >= insert_at {
                    *idx += 1;
                }
            }
            let pos = (self.shuffle_pos + 1).min(order.len());
            order.insert(pos, insert_at);
        }
    }

    /// Remove the track at `index` from the raw track list, adjusting
    /// `current_index` and shuffle order accordingly. Returns the removed path.
    pub fn remove_at(&mut self, index: usize) -> Option<String> {
        if index >= self.tracks.len() {
            return None;
        }

        let removed = self.tracks.remove(index);

        match self.current_index {
            Some(current) => {
                if current == index {
                    if self.tracks.is_empty() {
                        self.current_index = None;
                    } else if current >= self.tracks.len() {
                        self.current_index = Some(self.tracks.len() - 1);
                    }
                } else if current > index {
                    self.current_index = Some(current - 1);
                }
            }
            None => {}
        }

        if let Some(ref mut order) = self.shuffle_order {
            let mut new_order = Vec::with_capacity(order.len().saturating_sub(1));
            let mut removed_pos = None;
            for (i, &idx) in order.iter().enumerate() {
                if idx == index {
                    removed_pos = Some(i);
                    continue;
                }
                new_order.push(if idx > index { idx - 1 } else { idx });
            }
            *order = new_order;
            if let Some(rp) = removed_pos {
                if rp <= self.shuffle_pos {
                    self.shuffle_pos = self.shuffle_pos.saturating_sub(1);
                }
            }
            if order.is_empty() {
                self.shuffle_order = None;
                self.shuffle_pos = 0;
            }
        }

        Some(removed)
    }

    /// Remove all tracks from the queue except the one currently playing.
    pub fn clear_upcoming(&mut self) {
        if let Some(current_idx) = self.current_index {
            if let Some(path) = self.tracks.get(current_idx).cloned() {
                self.tracks = vec![path];
                self.current_index = Some(0);
            } else {
                self.tracks.clear();
                self.current_index = None;
            }
        } else {
            self.tracks.clear();
        }
        self.shuffle_order = None;
        self.shuffle_pos = 0;
    }

    /// Jump to a specific index in the queue and return its path.
    pub fn jump_to(&mut self, index: usize) -> Option<&str> {
        self.jump(index)
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
/// Held for its drop side-effect (silences audio when the stream is dropped).
#[allow(dead_code)]
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
    pub fn new() -> Result<Self, AudioError> {
        let (stream, handle) = OutputStream::try_default()
            .map_err(|error| AudioError::StreamCreation(error.to_string()))?;

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
    ) -> Result<(impl Source<Item = f32> + Send + 'static, Option<Duration>), AudioError> {
        let source = SymphoniaSource::new(path)?;
        let duration = source.total_duration();
        Ok((source.convert_samples(), duration))
    }

    pub fn play(&mut self, path: &str) -> Result<(), AudioError> {
        if let Some(sink) = self.sink.take() {
            sink.stop();
        }

        let (source, duration) = Self::build_source(path)?;
        let sink = Sink::try_new(&self.handle)
            .map_err(|error| AudioError::SinkCreation(error.to_string()))?;
        sink.set_volume(self.volume);
        sink.append(source);
        sink.play();

        self.sink = Some(sink);
        self.current_path = Some(PathBuf::from(path));
        self.clock = PlaybackClock {
            started_at: Some(Instant::now()),
            elapsed_before_start: Duration::ZERO,
            duration,
        };

        Ok(())
    }

    pub fn pause(&mut self) -> Result<(), AudioError> {
        match &self.sink {
            Some(sink) => {
                sink.pause();
                self.clock.elapsed_before_start = self.clock.position();
                self.clock.started_at = None;
                Ok(())
            }
            None => Err(AudioError::NoTrackLoaded),
        }
    }

    pub fn resume(&mut self) -> Result<(), AudioError> {
        match &self.sink {
            Some(sink) => {
                sink.play();
                self.clock.started_at = Some(Instant::now());
                Ok(())
            }
            None => Err(AudioError::NoTrackLoaded),
        }
    }

    pub fn stop(&mut self) -> Result<(), AudioError> {
        if let Some(sink) = self.sink.take() {
            sink.stop();
        }
        self.current_path = None;
        self.clock = PlaybackClock::stopped();
        Ok(())
    }

    pub fn seek(&mut self, seconds: f64) -> Result<(), AudioError> {
        let offset = Duration::from_secs_f64(seconds.max(0.0));

        match &self.sink {
            Some(sink) => {
                sink.try_seek(offset)
                    .map_err(|error| AudioError::Decode(format!("Seek failed: {error}")))?;

                let was_playing = self.is_playing();
                self.clock.elapsed_before_start = offset;
                self.clock.started_at = was_playing.then(Instant::now);

                Ok(())
            }
            None => Err(AudioError::NoTrackLoaded),
        }
    }

    pub fn set_volume(&mut self, volume: f32) -> Result<(), AudioError> {
        if !(0.0..=1.0).contains(&volume) {
            return Err(AudioError::InvalidVolume);
        }
        self.volume = volume;
        if let Some(ref sink) = self.sink {
            sink.set_volume(volume);
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
        self.clock.duration.map(|duration| duration.as_secs_f64())
    }

    pub fn volume(&self) -> f32 {
        self.volume
    }

    pub fn play_next(&mut self) -> Result<Option<String>, AudioError> {
        if self.repeat == RepeatMode::One {
            if let Some(path) = self.current_path.clone() {
                let path_str = path
                    .to_str()
                    .ok_or_else(|| AudioError::Decode("Invalid path encoding".to_string()))?
                    .to_string();
                self.play(&path_str)?;
                return Ok(Some(path_str));
            }
        }
        let path = self.queue.next(&self.repeat).map(str::to_string);
        if let Some(ref next_path) = path {
            self.play(next_path)?;
        }
        Ok(path)
    }

    pub fn play_previous(&mut self) -> Result<Option<String>, AudioError> {
        if self.position_seconds() > 3.0 {
            self.seek(0.0)?;
            return Ok(self
                .current_path
                .as_ref()
                .and_then(|path| path.to_str().map(str::to_string)));
        }
        let path = self.queue.previous(&self.repeat).map(str::to_string);
        if let Some(ref previous_path) = path {
            self.play(previous_path)?;
        }
        Ok(path)
    }

    // ── Queue manipulation ───────────────────────────────────────────────────

    pub fn enqueue(&mut self, path: &str) {
        self.queue.enqueue(path.to_string());
    }

    pub fn insert_next(&mut self, path: &str) {
        self.queue.insert_next(path.to_string());
    }

    pub fn remove_from_queue(&mut self, index: usize) -> Option<String> {
        self.queue.remove_at(index)
    }

    pub fn clear_upcoming(&mut self) {
        self.queue.clear_upcoming();
    }

    pub fn jump_to_queue_index(&mut self, index: usize) -> Result<Option<String>, AudioError> {
        let path = self.queue.jump_to(index).map(str::to_string);
        if let Some(ref path) = path {
            self.play(path)?;
        }
        Ok(path)
    }
}
