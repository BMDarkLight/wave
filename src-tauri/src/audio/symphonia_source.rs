use rodio::Source;
use std::fs::File;
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;
use symphonia::core::audio::{SampleBuffer, SignalSpec};
use symphonia::core::codecs::{CodecRegistry, DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::error::AudioError;

/// A codec registry that combines all of Symphonia's built-in decoders with the
/// libopus-backed `OpusDecoder`, since Symphonia has no first-party Opus codec.
fn codec_registry() -> &'static CodecRegistry {
    static REGISTRY: OnceLock<CodecRegistry> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut registry = CodecRegistry::new();
        symphonia::default::register_enabled_codecs(&mut registry);
        registry.register_all::<symphonia_adapter_libopus::OpusDecoder>();
        registry
    })
}

pub struct SymphoniaSource {
    decoder: Box<dyn symphonia::core::codecs::Decoder>,
    track_id: u32,
    format: Box<dyn symphonia::core::formats::FormatReader>,
    buffer: SampleBuffer<i16>,
    current_frame_offset: usize,
    total_duration: Option<Duration>,
    spec: SignalSpec,
}

impl SymphoniaSource {
    pub fn new(path: &str) -> Result<Self, AudioError> {
        crate::path_validation::validate_audio_path(path)
            .map_err(|e| AudioError::FileOpen(e))?;
        let file = File::open(path).map_err(|error| AudioError::FileOpen(error.to_string()))?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());

        let mut hint = Hint::new();
        if let Some(ext) = Path::new(path).extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }

        let format_opts = FormatOptions::default();
        let metadata_opts = MetadataOptions::default();
        let decoder_opts = DecoderOptions::default();

        let mut probed = symphonia::default::get_probe()
            .format(&hint, mss, &format_opts, &metadata_opts)
            .map_err(|error| AudioError::Decode(format!("Failed to probe format: {error}")))?;

        let track_id = probed
            .format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or_else(|| AudioError::UnsupportedFormat("No supported audio track found".into()))?
            .id;

        let track = probed
            .format
            .tracks()
            .iter()
            .find(|t| t.id == track_id)
            .unwrap();

        let codec_params = track.codec_params.clone();

        let mut decoder = codec_registry()
            .make(&codec_params, &decoder_opts)
            .map_err(|error| AudioError::Decode(format!("Failed to create decoder: {error}")))?;

        let total_duration =
            codec_params
                .time_base
                .zip(codec_params.n_frames)
                .map(|(base, frames)| {
                    let time = base.calc_time(frames);
                    Duration::from_secs(time.seconds) + Duration::from_secs_f64(time.frac)
                });

        let (buffer, spec) = Self::decode_first_packet(&mut probed.format, &mut decoder, track_id)?;

        Ok(Self {
            decoder,
            track_id,
            format: probed.format,
            buffer,
            current_frame_offset: 0,
            total_duration,
            spec,
        })
    }

    fn decode_first_packet(
        format: &mut Box<dyn symphonia::core::formats::FormatReader>,
        decoder: &mut Box<dyn symphonia::core::codecs::Decoder>,
        track_id: u32,
    ) -> Result<(SampleBuffer<i16>, SignalSpec), AudioError> {
        loop {
            match format.next_packet() {
                Ok(packet) => {
                    if packet.track_id() != track_id {
                        continue;
                    }
                    match decoder.decode(&packet) {
                        Ok(decoded) => {
                            let spec = *decoded.spec();
                            let duration =
                                symphonia::core::units::Duration::from(decoded.capacity() as u64);
                            let mut buf = SampleBuffer::<i16>::new(duration, spec);
                            buf.copy_interleaved_ref(decoded);
                            return Ok((buf, spec));
                        }
                        Err(_) => continue,
                    }
                }
                Err(symphonia::core::errors::Error::IoError(ref e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    return Err(AudioError::Decode("Unexpected end of file".into()));
                }
                Err(symphonia::core::errors::Error::SeekError(_)) => {
                    return Err(AudioError::Decode("Seek error during initialization".into()));
                }
                Err(error) => {
                    return Err(AudioError::Decode(format!("Failed to read packet: {error}")));
                }
            }
        }
    }
}

impl Iterator for SymphoniaSource {
    type Item = i16;

    fn next(&mut self) -> Option<i16> {
        if self.current_frame_offset >= self.buffer.len() {
            loop {
                let packet = self.format.next_packet().ok()?;
                if packet.track_id() != self.track_id {
                    continue;
                }
                let decoded = self.decoder.decode(&packet).ok()?;
                decoded.spec().clone_into(&mut self.spec);
                let duration = symphonia::core::units::Duration::from(decoded.capacity() as u64);
                let mut buf = SampleBuffer::<i16>::new(duration, self.spec);
                buf.copy_interleaved_ref(decoded);
                self.buffer = buf;
                self.current_frame_offset = 0;
                break;
            }
        }

        let sample = self.buffer.samples().get(self.current_frame_offset)?;
        self.current_frame_offset += 1;
        Some(*sample)
    }
}

impl Source for SymphoniaSource {
    fn current_frame_len(&self) -> Option<usize> {
        Some(self.buffer.len())
    }

    fn channels(&self) -> u16 {
        self.spec.channels.count() as u16
    }

    fn sample_rate(&self) -> u32 {
        self.spec.rate
    }

    fn total_duration(&self) -> Option<Duration> {
        self.total_duration
    }

    fn try_seek(&mut self, pos: Duration) -> Result<(), rodio::source::SeekError> {
        use symphonia::core::formats::{SeekMode, SeekTo};

        let seek_beyond_end = self
            .total_duration()
            .is_some_and(|dur| dur.saturating_sub(pos).as_millis() < 1);

        let time: symphonia::core::units::Time = if seek_beyond_end {
            let dur = self.total_duration.expect("checked above");
            (dur.as_secs_f64() - 0.0001).max(0.0).into()
        } else {
            pos.as_secs_f64().into()
        };

        let to_skip = self.current_frame_offset % self.channels() as usize;

        let seek_res = self
            .format
            .seek(SeekMode::Accurate, SeekTo::Time { time, track_id: None })
            .map_err(|e| rodio::source::SeekError::Other(Box::new(e)))?;

        let mut samples_to_pass = seek_res.required_ts - seek_res.actual_ts;
        let packet = loop {
            match self.format.next_packet() {
                Ok(candidate) => {
                    if candidate.dur() > samples_to_pass {
                        break candidate;
                    }
                    samples_to_pass -= candidate.dur();
                }
                Err(e) => return Err(rodio::source::SeekError::Other(Box::new(e))),
            }
        };

        let decoded = self
            .decoder
            .decode(&packet)
            .map_err(|e| rodio::source::SeekError::Other(Box::new(e)))?;

        decoded.spec().clone_into(&mut self.spec);
        let duration = symphonia::core::units::Duration::from(decoded.capacity() as u64);
        let mut buf = SampleBuffer::<i16>::new(duration, self.spec);
        buf.copy_interleaved_ref(decoded);
        self.buffer = buf;
        self.current_frame_offset = samples_to_pass as usize * self.channels() as usize + to_skip;

        Ok(())
    }
}
