use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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

        self.duration.map(|duration| elapsed.min(duration)).unwrap_or(elapsed)
    }
}

pub struct AudioPlayer {
    handle: &'static OutputStreamHandle,
    sink: Arc<Mutex<Option<Sink>>>,
    current_path: Arc<Mutex<Option<PathBuf>>>,
    clock: Arc<Mutex<PlaybackClock>>,
    volume: Arc<Mutex<f32>>,
}

impl AudioPlayer {
    pub fn new() -> Result<Self, String> {
        let (stream, handle) = OutputStream::try_default()
            .map_err(|e| format!("Failed to create output stream: {}", e))?;

        // Keep the audio stream alive for the full app lifetime.
        Box::leak(Box::new(stream));
        let handle = Box::leak(Box::new(handle));

        Ok(Self {
            handle,
            sink: Arc::new(Mutex::new(None)),
            current_path: Arc::new(Mutex::new(None)),
            clock: Arc::new(Mutex::new(PlaybackClock::stopped())),
            volume: Arc::new(Mutex::new(0.8)),
        })
    }

    fn build_source(
        &self,
        path: &str,
        offset: Duration,
    ) -> Result<(impl Source<Item = f32> + Send + 'static, Option<Duration>), String> {
        let file = File::open(path).map_err(|e| format!("Failed to open file: {}", e))?;
        let source = Decoder::new(BufReader::new(file))
            .map_err(|e| format!("Failed to decode audio. This format may not be supported by the current audio backend: {}", e))?;
        let duration = source.total_duration();
        Ok((source.convert_samples().skip_duration(offset), duration))
    }

    fn play_from(&self, path: &str, offset: Duration, should_play: bool) -> Result<(), String> {
        if let Some(sink) = self.sink.lock().unwrap().take() {
            sink.stop();
        }

        let (source, duration) = self.build_source(path, offset)?;
        let sink = Sink::try_new(self.handle)
            .map_err(|e| format!("Failed to create sink: {}", e))?;
        sink.set_volume(*self.volume.lock().unwrap());
        sink.append(source);

        if should_play {
            sink.play();
        } else {
            sink.pause();
        }

        *self.sink.lock().unwrap() = Some(sink);
        *self.current_path.lock().unwrap() = Some(PathBuf::from(path));
        *self.clock.lock().unwrap() = PlaybackClock {
            started_at: should_play.then(Instant::now),
            elapsed_before_start: offset,
            duration,
        };

        Ok(())
    }

    pub fn play(&self, path: &str) -> Result<(), String> {
        self.play_from(path, Duration::ZERO, true)
    }

    pub fn pause(&self) -> Result<(), String> {
        if let Some(ref sink) = *self.sink.lock().unwrap() {
            sink.pause();
            let mut clock = self.clock.lock().unwrap();
            clock.elapsed_before_start = clock.position();
            clock.started_at = None;
            Ok(())
        } else {
            Err("No track is currently playing".to_string())
        }
    }

    pub fn resume(&self) -> Result<(), String> {
        if let Some(ref sink) = *self.sink.lock().unwrap() {
            sink.play();
            self.clock.lock().unwrap().started_at = Some(Instant::now());
            Ok(())
        } else {
            Err("No track is currently paused".to_string())
        }
    }

    pub fn stop(&self) -> Result<(), String> {
        if let Some(sink) = self.sink.lock().unwrap().take() {
            sink.stop();
        }
        *self.current_path.lock().unwrap() = None;
        *self.clock.lock().unwrap() = PlaybackClock::stopped();
        Ok(())
    }

    pub fn seek(&self, seconds: f64) -> Result<(), String> {
        let path = self
            .get_current_path()
            .and_then(|path| path.to_str().map(|value| value.to_string()))
            .ok_or("No track selected")?;
        let should_play = self.is_playing();
        let offset = Duration::from_secs_f64(seconds.max(0.0));
        self.play_from(&path, offset, should_play)
    }

    pub fn set_volume(&self, volume: f32) -> Result<(), String> {
        let clamped = volume.clamp(0.0, 1.0);
        *self.volume.lock().unwrap() = clamped;
        if let Some(ref sink) = *self.sink.lock().unwrap() {
            sink.set_volume(clamped);
        }
        Ok(())
    }

    pub fn is_playing(&self) -> bool {
        if let Some(ref sink) = *self.sink.lock().unwrap() {
            !sink.is_paused() && sink.len() > 0
        } else {
            false
        }
    }

    pub fn is_paused(&self) -> bool {
        if let Some(ref sink) = *self.sink.lock().unwrap() {
            sink.is_paused()
        } else {
            false
        }
    }

    pub fn get_current_path(&self) -> Option<PathBuf> {
        self.current_path.lock().unwrap().clone()
    }

    pub fn position_seconds(&self) -> f64 {
        self.clock.lock().unwrap().position().as_secs_f64()
    }

    pub fn duration_seconds(&self) -> Option<f64> {
        self.clock.lock().unwrap().duration.map(|duration| duration.as_secs_f64())
    }

    pub fn volume(&self) -> f32 {
        *self.volume.lock().unwrap()
    }
}
