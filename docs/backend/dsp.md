# Digital Signal Processing (DSP)

The DSP module at `src-tauri/src/audio/dsp.rs` provides real-time audio processing
that is inserted into the playback pipeline between decoding and output.

## Architecture

```
File → SymphoniaSource (i16)
  → .convert_samples::<f32>()
  → Equalizer (10-band EQ)    ← inserted here when enabled
  → Rodio Sink
  → CPAL audio output
```

The `Equalizer<S>` is a generic `Source` wrapper — it wraps any `Source<Item = f32>`
and applies DSP per-sample in `Iterator::next()`.

## The biquad filter

Every EQ band is a **biquad (bi-quadratic) filter** in Direct Form 1:

```
y[n] = b0·x[n] + b1·x[n-1] + b2·x[n-2] - a1·y[n-1] - a2·y[n-2]
```

The `Biquad` struct implements the **peaking (bell) EQ** variant using the
RBJ cookbook formulae. When the centre frequency, gain, or Q factor changes
the coefficients are recalculated.

### Biquad coefficients (peaking EQ)

```
A     = 10^(gain_db / 40)
ω0    = 2π · freq / sample_rate
α     = sin(ω0) / (2 · Q)

b0 = 1 + α·A     a0 = 1 + α/A
b1 = -2·cos(ω0)  a1 = -2·cos(ω0)
b2 = 1 - α·A     a2 = 1 - α/A
```

Final coefficients are normalised by dividing `b0`–`b2`, `a1`, `a2` by `a0`.

## 10-band graphic equalizer

### Frequency bands

| Band | Frequency |
|------|-----------|
| 0  | 31 Hz    |
| 1  | 62 Hz    |
| 2  | 125 Hz   |
| 3  | 250 Hz   |
| 4  | 500 Hz   |
| 5  | 1 kHz    |
| 6  | 2 kHz    |
| 7  | 4 kHz    |
| 8  | 8 kHz    |
| 9  | 16 kHz   |

### Q factor

All bands use a fixed Q of `1.41` (1/√2, the Butterworth approximation)
for a smooth, musically-neutral response curve. Adjacent bands overlap
enough to produce a flat combined response when all gains are set to 0 dB.

### Real-time updates

The `EqConfig` is shared between `AudioPlayer` and `Equalizer` through an
`Arc<Mutex<EqConfig>>` plus a version counter:

1. The frontend calls `set_eq_bands` or `set_eq_enabled`
2. `AudioPlayer` updates the shared config and bumps the version
3. On the next `next()` call, the `Equalizer` detects the version change
4. Filter coefficients are lazily rebuilt before any samples are processed
5. Filtered output continues seamlessly

### Seeking

When a seek occurs (`try_seek`), all biquad filter states are reset (x1/x2/y1/y2
set to zero) to avoid filter artefacts caused by the sample discontinuity.

## Extending

To add a new DSP effect:

1. Create a struct that wraps `S` (the inner source)
2. Implement `Iterator` (call `self.inner.next()`, apply processing)
3. Implement `Source` (delegate to `self.inner` for `channels`, `sample_rate`,
   `total_duration`, `current_frame_len`, `try_seek`)
4. Optionally share mutable state via `Arc<Mutex<T>>` for real-time parameter
   changes
5. Insert the wrapper in `AudioPlayer::build_source()`

### Example: crossfade

```rust
pub struct Crossfade<S1, S2> {
    outgoing: S1,
    incoming: S2,
    position: f64, // 0.0 → 1.0
    duration_samples: usize,
}

impl<S1: Source<Item = f32>, S2: Source<Item = f32>> Iterator for Crossfade<S1, S2> {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        let out = self.outgoing.next()?;
        let inc = self.incoming.next()?;
        let gain = (self.position as f32).min(1.0);
        self.position += 1.0 / self.duration_samples as f64;
        Some(out * (1.0 - gain) + inc * gain)
    }
}
```

### Example: simple bass boost

Apply a low-shelf biquad filter directly. This can be added as another
`Biquad` instance in the `Equalizer` chain, or as its own wrapper.
