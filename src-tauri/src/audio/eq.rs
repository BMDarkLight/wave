use serde::{Deserialize, Serialize};

/// Standard pre-defined EQ bands (ISO 1/3-octave centered on the audible range).
pub const DEFAULT_BAND_FREQUENCIES: &[f64] = &[
    31.0, 62.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
];

/// A single EQ band with a centre frequency and gain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EqBand {
    /// Centre frequency in Hz.
    pub frequency: f64,
    /// Gain in decibels (0 dB = flat).
    pub gain_db: f32,
    /// Whether this band is active in the EQ filter chain.
    pub active: bool,
}

impl EqBand {
    pub fn new(frequency: f64) -> Self {
        Self {
            frequency,
            gain_db: 0.0,
            active: true,
        }
    }
}

/// Container for all EQ bands and the master enable flag.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EqualizerSettings {
    pub bands: Vec<EqBand>,
    /// Master EQ toggle.
    pub enabled: bool,
}

impl Default for EqualizerSettings {
    fn default() -> Self {
        let bands: Vec<EqBand> = DEFAULT_BAND_FREQUENCIES
            .iter()
            .copied()
            .map(EqBand::new)
            .collect();
        Self {
            bands,
            enabled: false,
        }
    }
}

impl EqualizerSettings {
    /// Find a band by its centre frequency (within 1 Hz tolerance).
    pub fn find_band_mut(&mut self, frequency: f64) -> Option<&mut EqBand> {
        self.bands
            .iter_mut()
            .find(|b| (b.frequency - frequency).abs() < 1.0)
    }

    /// Find a band index by frequency.
    pub fn find_band_index(&self, frequency: f64) -> Option<usize> {
        self.bands
            .iter()
            .position(|b| (b.frequency - frequency).abs() < 1.0)
    }

    /// Add or update a band. If a band with the same frequency already exists,
    /// its gain and active flag are updated; otherwise a new band is appended.
    pub fn set_band(&mut self, frequency: f64, gain_db: f32, active: bool) {
        match self.find_band_mut(frequency) {
            Some(band) => {
                band.gain_db = gain_db;
                band.active = active;
            }
            None => self.bands.push(EqBand {
                frequency,
                gain_db,
                active,
            }),
        }
    }

    /// Remove a band by frequency (cant remove the last band).
    pub fn remove_band(&mut self, frequency: f64) {
        if let Some(idx) = self.find_band_index(frequency) {
            if self.bands.len() > 1 {
                self.bands.remove(idx);
            }
        }
    }

    /// Reset all bands to 0 dB and re-activate them.
    pub fn reset(&mut self) {
        for band in &mut self.bands {
            band.gain_db = 0.0;
            band.active = true;
        }
    }

    /// Return bands sorted by frequency ascending.
    #[allow(dead_code)]
    pub fn sorted_bands(&self) -> Vec<EqBand> {
        let mut bands = self.bands.clone();
        bands.sort_by(|a, b| a.frequency.partial_cmp(&b.frequency).unwrap());
        bands
    }
}
