use std::sync::{Arc, Mutex};
use std::time::Duration;

use rodio::source::Source;

/// Standard ISO frequency bands for a 10-band graphic equalizer.
pub const EQ_BANDS_HZ: [f32; 10] = [
    31.0, 62.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
];

#[derive(Debug, Clone)]
pub struct EqConfig {
    /// Gain per band in dB.
    pub bands: [f32; 10],
    /// Master enable/disable for the EQ chain.
    pub enabled: bool,
}

impl Default for EqConfig {
    fn default() -> Self {
        Self {
            bands: [0.0; 10],
            enabled: false,
        }
    }
}

/// Transparent bi-quad filter (Direct Form 1).
///
/// Used as the atomic building block for peaking EQ, low/high-shelf,
/// and low/high-pass filters.
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl Biquad {
    /// Peaking (bell) EQ filter centred at `freq` Hz with gain `gain_db` dB
    /// and quality factor `q`.
    fn peaking_eq(sample_rate: f32, freq: f32, gain_db: f32, q: f32) -> Self {
        let a = 10f32.powf(gain_db / 40.0);
        let w0 = 2.0 * std::f32::consts::PI * freq / sample_rate;
        let alpha = w0.sin() / (2.0 * q);
        let cos_w0 = w0.cos();

        let b0 = 1.0 + alpha * a;
        let b1 = -2.0 * cos_w0;
        let b2 = 1.0 - alpha * a;
        let a0 = 1.0 + alpha / a;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha / a;

        let inv_a0 = 1.0 / a0;
        Self {
            b0: b0 * inv_a0,
            b1: b1 * inv_a0,
            b2: b2 * inv_a0,
            a1: a1 * inv_a0,
            a2: a2 * inv_a0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2 - self.a1 * self.y1
            - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }

    fn reset(&mut self) {
        self.x1 = 0.0;
        self.x2 = 0.0;
        self.y1 = 0.0;
        self.y2 = 0.0;
    }
}

/// `Source` wrapper that applies a 10-band graphic equalizer in real-time.
///
/// EQ bands are configured via the shared `EqConfig` so the frontend can
/// adjust gains while a track is playing. The filter coefficients are
/// lazily rebuilt whenever the config version changes.
pub struct Equalizer<S> {
    inner: S,
    filters: [Biquad; 10],
    config: Arc<Mutex<EqConfig>>,
    sr: f32,
    /// Cached enabled flag so we avoid locking on every sample when EQ is off.
    enabled: bool,
    /// Monotonically increasing version — bumped on every config write so the
    /// audio thread can cheaply detect changes.
    version: Arc<Mutex<u64>>,
    last_version: u64,
}

impl<S: Source<Item = f32>> Equalizer<S> {
    pub fn new(source: S, config: Arc<Mutex<EqConfig>>, version: Arc<Mutex<u64>>) -> Self {
        let sr = source.sample_rate() as f32;
        let cfg = config.lock().unwrap();
        let enabled = cfg.enabled;
        let mut filters = core::array::from_fn(|_| Biquad::peaking_eq(sr, 1000.0, 0.0, 1.41));
        for (i, (freq, gain)) in EQ_BANDS_HZ.iter().zip(cfg.bands.iter()).enumerate() {
            filters[i] = Biquad::peaking_eq(sr, *freq, *gain, 1.41);
        }
        drop(cfg);

        Self {
            inner: source,
            filters,
            config,
            sr,
            enabled,
            version,
            last_version: 0,
        }
    }

    fn sync_config(&mut self) {
        let v = *self.version.lock().unwrap();
        if v == self.last_version {
            return;
        }
        self.last_version = v;
        let cfg = self.config.lock().unwrap();
        self.enabled = cfg.enabled;
        for (i, (freq, gain)) in EQ_BANDS_HZ.iter().zip(cfg.bands.iter()).enumerate() {
            self.filters[i] = Biquad::peaking_eq(self.sr, *freq, *gain, 1.41);
        }
    }
}

impl<S: Source<Item = f32> + Send> Iterator for Equalizer<S> {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let sample = self.inner.next()?;
        self.sync_config();
        if self.enabled {
            Some(self.filters.iter_mut().fold(sample, |s, f| f.process(s)))
        } else {
            Some(sample)
        }
    }
}

impl<S: Source<Item = f32> + Send> Source for Equalizer<S> {
    fn current_frame_len(&self) -> Option<usize> {
        self.inner.current_frame_len()
    }

    fn channels(&self) -> u16 {
        self.inner.channels()
    }

    fn sample_rate(&self) -> u32 {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }

    fn try_seek(&mut self, pos: Duration) -> Result<(), rodio::source::SeekError> {
        for f in self.filters.iter_mut() {
            f.reset();
        }
        self.inner.try_seek(pos)
    }
}
