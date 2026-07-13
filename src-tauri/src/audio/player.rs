use std::sync::{Arc, Mutex};

use rodio::{OutputStream, OutputStreamHandle, Sink, Source};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::error::AudioError;

use super::dsp::{EqConfig, Equalizer};
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

    fn raw_elapsed(&self) -> Duration {
        self.started_at
            .map(|started_at| self.elapsed_before_start + started_at.elapsed())
            .unwrap_or(self.elapsed_before_start)
    }

    fn position(&self) -> Duration {
        let elapsed = self.raw_elapsed();
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

    /// Index that `next` would select, without mutating queue state.
    fn peek_next_index(&self, repeat: &RepeatMode) -> Option<usize> {
        if self.tracks.is_empty() {
            return None;
        }
        if let Some(ref order) = self.shuffle_order {
            let next_pos = self.shuffle_pos + 1;
            match repeat {
                RepeatMode::All => Some(order[next_pos % order.len()]),
                RepeatMode::Off | RepeatMode::One => {
                    if next_pos < order.len() {
                        Some(order[next_pos])
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
        }
    }

    /// Peek at the next track path without advancing the queue.
    pub fn peek_next(&self, repeat: &RepeatMode) -> Option<&str> {
        self.peek_next_index(repeat)
            .and_then(|i| self.tracks.get(i).map(String::as_str))
    }

    /// Advance to the next track. Returns `None` when the queue is exhausted
    /// and `repeat == Off`. With `repeat == All` it wraps around.
    pub fn next(&mut self, repeat: &RepeatMode) -> Option<&str> {
        let next_idx = self.peek_next_index(repeat)?;
        if let Some(ref order) = self.shuffle_order.clone() {
            let next_pos = self.shuffle_pos + 1;
            match repeat {
                RepeatMode::All => self.shuffle_pos = next_pos % order.len(),
                RepeatMode::Off | RepeatMode::One => self.shuffle_pos = next_pos,
            }
        }
        self.current_index = Some(next_idx);
        self.tracks.get(next_idx).map(String::as_str)
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

    /// Move a track from `from` to `to` within the queue, keeping
    /// `current_index` and shuffle indices consistent.
    pub fn move_track(&mut self, from: usize, to: usize) -> bool {
        let len = self.tracks.len();
        if from >= len || to >= len || from == to {
            return false;
        }

        let item = self.tracks.remove(from);
        self.tracks.insert(to, item);

        let remap = |idx: usize| -> usize {
            if idx == from {
                to
            } else if from < to {
                if idx > from && idx <= to {
                    idx - 1
                } else {
                    idx
                }
            } else if idx >= to && idx < from {
                idx + 1
            } else {
                idx
            }
        };

        if let Some(current) = self.current_index {
            self.current_index = Some(remap(current));
        }

        if let Some(ref mut order) = self.shuffle_order {
            for idx in order.iter_mut() {
                *idx = remap(*idx);
            }
        }

        true
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

struct AudioOutput {
    _stream: SendableStream,
    handle: OutputStreamHandle,
}

pub struct AudioPlayer {
    /// Lazily opened so Android can finish JNI setup before cpal/oboe runs.
    output: Option<AudioOutput>,
    sink: Option<Sink>,
    current_path: Option<PathBuf>,
    clock: PlaybackClock,
    volume: f32,
    pub queue: Queue,
    pub repeat: RepeatMode,
    /// Shared equalizer configuration (may be referenced by a running
    /// `Equalizer` source inside the current sink).
    pub eq_config: Arc<Mutex<EqConfig>>,
    pub(crate) eq_version: Arc<Mutex<u64>>,
    /// Next queue path already appended to the sink so playback can continue
    /// when the current source ends — critical on Android while backgrounded,
    /// where a delayed tick alone can miss the transition.
    prefetched_next: Option<(String, Option<Duration>)>,
}

impl AudioPlayer {
    /// Create a player without opening an OS audio device.
    ///
    /// Queue / EQ / volume state work immediately; the output stream is opened
    /// on first `play` (or explicit `ensure_output`).
    pub fn new_deferred() -> Self {
        Self {
            output: None,
            sink: None,
            current_path: None,
            clock: PlaybackClock::stopped(),
            volume: 0.8,
            queue: Queue::default(),
            repeat: RepeatMode::default(),
            eq_config: Arc::new(Mutex::new(EqConfig::default())),
            eq_version: Arc::new(Mutex::new(0)),
            prefetched_next: None,
        }
    }

    pub fn new() -> Result<Self, AudioError> {
        let mut player = Self::new_deferred();
        player.ensure_output()?;
        Ok(player)
    }

    /// Open the default output device if it is not open yet.
    pub fn ensure_output(&mut self) -> Result<(), AudioError> {
        if self.output.is_some() {
            return Ok(());
        }

        crate::android_jni::ensure_jni_thread_attached();
        if !crate::android_jni::android_audio_ready() {
            return Err(AudioError::StreamCreation(
                "Android audio context is not ready yet — try playing again in a moment"
                    .to_string(),
            ));
        }

        let opened = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            OutputStream::try_default()
        }));

        let (stream, handle) = match opened {
            Ok(Ok(pair)) => pair,
            Ok(Err(error)) => {
                return Err(AudioError::StreamCreation(format!(
                    "Could not open audio output device: {error}"
                )));
            }
            Err(payload) => {
                let msg = if let Some(s) = payload.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = payload.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "unknown panic".to_string()
                };
                tracing::error!("OutputStream::try_default panicked: {msg}");
                return Err(AudioError::StreamCreation(format!(
                    "Audio system crash: {msg}"
                )));
            }
        };

        self.output = Some(AudioOutput {
            _stream: SendableStream(stream),
            handle,
        });
        Ok(())
    }

    /// Create a player that outputs to a specific audio device (by name).
    pub fn new_with_device(device_name: &str) -> Result<Self, AudioError> {
        use cpal::traits::{DeviceTrait, HostTrait};

        crate::android_jni::ensure_jni_thread_attached();
        if !crate::android_jni::android_audio_ready() {
            return Err(AudioError::StreamCreation(
                "Android audio context is not ready yet — try again in a moment".to_string(),
            ));
        }

        let host = cpal::default_host();
        let device = host
            .output_devices()
            .map_err(|e| AudioError::DeviceUnavailable(e.to_string()))?
            .find(|d| d.name().map(|n| n == device_name).unwrap_or(false))
            .ok_or_else(|| AudioError::DeviceUnavailable(format!("Device not found: {device_name}")))?;

        let (stream, handle) = OutputStream::try_from_device(&device)
            .map_err(|error| AudioError::StreamCreation(error.to_string()))?;

        Ok(Self {
            output: Some(AudioOutput {
                _stream: SendableStream(stream),
                handle,
            }),
            sink: None,
            current_path: None,
            clock: PlaybackClock::stopped(),
            volume: 0.8,
            queue: Queue::default(),
            repeat: RepeatMode::default(),
            eq_config: Arc::new(Mutex::new(EqConfig::default())),
            eq_version: Arc::new(Mutex::new(0)),
            prefetched_next: None,
        })
    }

    /// List all available audio output device names.
    pub fn list_output_devices() -> Vec<String> {
        use cpal::traits::{DeviceTrait, HostTrait};
        crate::android_jni::ensure_jni_thread_attached();
        let listed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            match cpal::default_host().output_devices() {
                Ok(devices) => devices
                    .filter_map(|d| d.name().ok())
                    .filter(|n| !n.is_empty())
                    .collect(),
                Err(_) => vec![],
            }
        }));
        listed.unwrap_or_default()
    }

    fn build_source(
        path: &str,
        eq_config: Arc<Mutex<EqConfig>>,
        eq_version: Arc<Mutex<u64>>,
    ) -> Result<(impl Source<Item = f32> + Send + 'static, Option<Duration>), AudioError> {
        let source = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            SymphoniaSource::new(path)
        })) {
            Ok(result) => result?,
            Err(_) => {
                return Err(AudioError::Decode(format!(
                    "Audio decoding crashed while opening \"{path}\". \
                     The file may be corrupted."
                )));
            }
        };
        let duration = source.total_duration();
        let converted = source.convert_samples();
        let eq = Equalizer::new(converted, eq_config, eq_version);
        Ok((eq, duration))
    }

    pub fn play(&mut self, path: &str) -> Result<(), AudioError> {
        self.ensure_output()?;
        let handle = &self
            .output
            .as_ref()
            .expect("output ensured")
            .handle;

        if let Some(sink) = self.sink.take() {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                sink.stop();
            }));
        }
        self.prefetched_next = None;

        let (source, duration) =
            Self::build_source(path, self.eq_config.clone(), self.eq_version.clone())?;

        let sink = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            Sink::try_new(handle)
        })) {
            Ok(Ok(sink)) => sink,
            Ok(Err(error)) => {
                return Err(AudioError::SinkCreation(format!(
                    "Could not initialise audio playback: {error}"
                )));
            }
            Err(_) => {
                return Err(AudioError::SinkCreation(
                    "Audio playback initialisation crashed. \
                     Another app may be using the speaker exclusively."
                        .to_string(),
                ));
            }
        };

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

        // Prefetch the following queue item into the same sink so rodio keeps
        // playing when this source ends — even if the auto-advance tick is
        // delayed (common when Android backgrounds the activity).
        self.prefetch_next_into_sink();

        Ok(())
    }

    /// Append the upcoming queue track to the active sink (best-effort).
    fn prefetch_next_into_sink(&mut self) {
        if self.repeat == RepeatMode::One {
            return;
        }
        let Some(next_path) = self.queue.peek_next(&self.repeat).map(str::to_string) else {
            return;
        };
        let Ok((source, duration)) =
            Self::build_source(&next_path, self.eq_config.clone(), self.eq_version.clone())
        else {
            return;
        };
        if let Some(sink) = self.sink.as_ref() {
            sink.append(source);
            self.prefetched_next = Some((next_path, duration));
        }
    }

    /// Adopt a track that is already playing via sink prefetch (no restart).
    fn adopt_prefetched(&mut self, path: &str, duration: Option<Duration>) {
        self.current_path = Some(PathBuf::from(path));
        self.clock = PlaybackClock {
            started_at: Some(Instant::now()),
            elapsed_before_start: Duration::ZERO,
            duration,
        };
        self.prefetched_next = None;
        self.prefetch_next_into_sink();
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
        self.prefetched_next = None;
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
                // Seeking invalidates any sink-prefetched follow-up track.
                self.prefetched_next = None;

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

    /// True when the sink has drained (natural end-of-track).
    pub fn sink_exhausted(&self) -> bool {
        match &self.sink {
            Some(sink) => sink.empty(),
            None => self.current_path.is_some(),
        }
    }

    /// True when the current track has reached end-of-stream and should advance.
    ///
    /// On some Android/cpal backends the sink never reports empty (or pauses
    /// instead), so we also treat wall-clock past known duration as finished.
    /// When the next track was prefetched into the sink, duration-end means the
    /// prefetched source is already audible and we only need to adopt it.
    pub fn should_auto_advance(&self) -> bool {
        if self.current_path.is_none() {
            return false;
        }

        let at_duration_end = self.clock.duration.is_some_and(|duration| {
            let grace = Duration::from_millis(350);
            self.clock.raw_elapsed() >= duration.saturating_add(grace)
        });

        // Prefetched next is already in the sink — advance once the current
        // source is done so metadata/queue stay aligned without restarting audio.
        if self.prefetched_next.is_some() {
            if at_duration_end {
                return true;
            }
            // Decoder didn't report duration: treat a single remaining source
            // after we've actually been playing as the handoff point.
            let handed_off = self.clock.raw_elapsed() >= Duration::from_secs(1)
                && self
                    .sink
                    .as_ref()
                    .is_some_and(|sink| !sink.is_paused() && sink.len() <= 1);
            if handed_off {
                return true;
            }
        }

        if self.is_paused() {
            // Some Android backends pause when the source ends rather than
            // leaving an idle non-paused empty sink.
            return self.sink_exhausted() || at_duration_end;
        }

        if !self.is_playing() {
            return true;
        }

        at_duration_end
    }

    /// Back-compat alias used by older call sites.
    pub fn has_finished_naturally(&self) -> bool {
        self.should_auto_advance()
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

    // ── Equalizer ─────────────────────────────────────────────────────────────

    pub fn eq_settings(&self) -> EqConfig {
        self.eq_config.lock().unwrap().clone()
    }

    pub fn set_eq_bands(&mut self, bands: [f32; 10]) {
        let mut cfg = self.eq_config.lock().unwrap();
        cfg.bands = bands;
        *self.eq_version.lock().unwrap() += 1;
    }

    pub fn set_eq_enabled(&mut self, enabled: bool) {
        let mut cfg = self.eq_config.lock().unwrap();
        cfg.enabled = enabled;
        *self.eq_version.lock().unwrap() += 1;
    }

    pub fn apply_eq_preset(&mut self, name: &str) -> Result<(), String> {
        let mut cfg = self.eq_config.lock().unwrap();
        cfg.apply_preset(name)
            .ok_or_else(|| {
                let names: Vec<&str> = EqConfig::list_presets().map(|(n, _)| n).collect();
                format!("Unknown EQ preset \"{name}\". Available: {}", names.join(", "))
            })?;
        *self.eq_version.lock().unwrap() += 1;
        Ok(())
    }

    /// Query the current default audio output device name (live, every call).
    pub fn current_output_name() -> String {
        use cpal::traits::{DeviceTrait, HostTrait};
        let name = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            cpal::default_host()
                .default_output_device()
                .and_then(|d| d.name().ok())
                .unwrap_or_else(|| "Unknown".to_string())
        }));
        name.unwrap_or_else(|_| "Unknown".to_string())
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

        // If we already appended the next source, adopt it without tearing down
        // the sink — keeps audio continuous when the app is backgrounded.
        if let Some((prefetched, duration)) = self.prefetched_next.clone() {
            let path = self.queue.next(&self.repeat).map(str::to_string);
            if path.as_deref() == Some(prefetched.as_str()) {
                self.adopt_prefetched(&prefetched, duration);
                return Ok(Some(prefetched));
            }
            // Queue and prefetch diverged (shuffle/repeat changed) — restart.
            self.prefetched_next = None;
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

    pub fn move_queue_track(&mut self, from: usize, to: usize) -> bool {
        self.queue.move_track(from, to)
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
