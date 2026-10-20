//! Whole-track decode to `f32` at the file's own rate, for analysis
//! (Phase 3, `reks-analysis`).
//!
//! Unlike [`crate::decode_stereo`] nothing is quantised or resampled here;
//! [`Layout::Mono`] mixes down while decoding so a long stereo track never
//! sits in memory twice. Symphonia's gapless decode trims the encoder delay
//! and padding; [`FloatPcm`] reports both as the container stated them, so
//! the delay convention can be calibrated against rekordbox (phase 3 task 4).

use std::path::Path;

use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error as SymErr;

use crate::decode::{open_track, too_long, OpenTrack, MAX_DECODE_MS};
use crate::error::{Error, Result};
use crate::probe::decode_err;

/// Channel layout [`decode_f32`] returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// Every channel of the file, interleaved.
    Interleaved,
    /// One channel: the mean of all the file's channels.
    Mono,
}

/// Decoded `f32` PCM at the file's sample rate, full scale ±1.0.
#[derive(Clone, PartialEq)]
pub struct FloatPcm {
    /// Interleaved frames, `channels` samples each.
    pub samples: Vec<f32>,
    /// 1 for [`Layout::Mono`].
    pub channels: u16,
    pub sample_rate: u32,
    /// The file's own channel count.
    pub source_channels: u16,
    /// Leading encoder-delay frames the container reported (already trimmed
    /// from `samples`); `None` when it reported none, as for PCM and FLAC.
    pub encoder_delay: Option<u32>,
    /// Trailing padding frames the container reported (already trimmed).
    pub encoder_padding: Option<u32>,
}

impl FloatPcm {
    /// Number of frames.
    pub fn frames(&self) -> usize {
        self.samples.len() / usize::from(self.channels.max(1))
    }
}

impl std::fmt::Debug for FloatPcm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FloatPcm")
            .field("frames", &self.frames())
            .field("channels", &self.channels)
            .field("sample_rate", &self.sample_rate)
            .field("source_channels", &self.source_channels)
            .field("encoder_delay", &self.encoder_delay)
            .field("encoder_padding", &self.encoder_padding)
            .finish()
    }
}

/// Decodes all of `path` to `f32` in `layout`. Corrupt packets are skipped
/// and a truncated file keeps what decoded, as in [`crate::decode_stereo`].
pub fn decode_f32(path: &Path, layout: Layout) -> Result<FloatPcm> {
    let OpenTrack {
        mut format,
        track_id,
        params,
        sample_rate,
        num_frames,
        delay,
        padding,
    } = open_track(path)?;
    let max_frames = MAX_DECODE_MS * u64::from(sample_rate) / 1000;
    if let Some(frames) = num_frames.filter(|&f| f > max_frames) {
        return Err(too_long(path, frames, sample_rate));
    }
    let source_channels = params
        .channels
        .as_ref()
        .and_then(|c| u16::try_from(c.count()).ok())
        .filter(|&c| c > 0)
        .ok_or_else(|| Error::Unsupported {
            path: path.to_path_buf(),
            reason: "unknown channel count".to_owned(),
        })?;
    let channels = match layout {
        Layout::Interleaved => source_channels,
        Layout::Mono => 1,
    };

    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(&params, &AudioDecoderOptions::default())
        .map_err(|source| decode_err(path, source))?;

    let capacity = num_frames.unwrap_or(0).min(max_frames) as usize * usize::from(channels);
    let mut samples: Vec<f32> = Vec::with_capacity(capacity);
    let mut scratch: Vec<f32> = Vec::new();
    let mut frame_count: u64 = 0;
    loop {
        let packet = match format.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) => break,
            Err(SymErr::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(source) => return Err(decode_err(path, source)),
        };
        if packet.track_id != track_id {
            continue;
        }
        let buf = match decoder.decode(&packet) {
            Ok(buf) => buf,
            Err(SymErr::DecodeError(msg)) => {
                tracing::debug!(path = %path.display(), msg, "skipped undecodable packet");
                continue;
            }
            Err(source) => return Err(decode_err(path, source)),
        };
        let n = buf.spec().channels().count();
        if n == 0 {
            continue;
        }
        buf.copy_to_vec_interleaved::<f32>(&mut scratch);
        match layout {
            Layout::Interleaved if n == usize::from(channels) => samples.extend(&scratch),
            Layout::Interleaved => {
                return Err(Error::Unsupported {
                    path: path.to_path_buf(),
                    reason: format!("channel count changed from {channels} to {n}"),
                })
            }
            Layout::Mono => {
                let scale = 1.0 / n as f32;
                samples.extend(
                    scratch
                        .chunks_exact(n)
                        .map(|f| f.iter().sum::<f32>() * scale),
                );
            }
        }
        frame_count += (scratch.len() / n) as u64;
        if frame_count > max_frames {
            return Err(too_long(path, frame_count, sample_rate));
        }
    }

    Ok(FloatPcm {
        samples,
        channels,
        sample_rate,
        source_channels,
        encoder_delay: delay,
        encoder_padding: padding,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/audio")
            .join(name)
    }

    #[test]
    fn wav_decodes_exactly() {
        let pcm = decode_f32(&fixture("sine-click-128.wav"), Layout::Interleaved).unwrap();
        assert_eq!(pcm.sample_rate, 44_100);
        assert_eq!((pcm.channels, pcm.source_channels), (1, 1));
        assert_eq!(pcm.frames(), reks_fixtures_gen::DURATION_SAMPLES);
        assert_eq!(pcm.encoder_delay.unwrap_or(0), 0);
        let expected = reks_fixtures_gen::render();
        for (i, (&got, &want)) in pcm.samples.iter().zip(&expected).enumerate() {
            assert_eq!(got, f32::from(want) / 32_768.0, "sample {i}");
        }
    }

    #[test]
    fn stereo_mixes_down_to_the_mean() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("float.wav");
        reks_fixtures_gen::write_wav_float(std::fs::File::create(&path).unwrap()).unwrap();
        let both = decode_f32(&path, Layout::Interleaved).unwrap();
        let mono = decode_f32(&path, Layout::Mono).unwrap();
        assert_eq!(
            (both.channels, mono.channels, mono.source_channels),
            (2, 1, 2)
        );
        assert_eq!(mono.frames(), both.frames());
        for (m, [l, r]) in mono.samples.iter().zip(both.samples.as_chunks::<2>().0) {
            assert!((m - (l + r) / 2.0).abs() < 1e-7);
        }
    }

    #[test]
    fn mp3_reports_encoder_delay_and_trims_it() {
        let pcm = decode_f32(&fixture("sine-click-128.mp3"), Layout::Mono).unwrap();
        // LAME's header states the delay (576 + decoder delay 529 = 1105)
        // and the padding; gapless decode removes both.
        assert_eq!(pcm.encoder_delay, Some(1105));
        assert!(pcm.encoder_padding.is_some());
        assert_eq!(pcm.frames(), reks_fixtures_gen::DURATION_SAMPLES);
    }

    #[test]
    fn missing_file_is_an_error() {
        let err = decode_f32(Path::new("/nonexistent/x.wav"), Layout::Mono).unwrap_err();
        assert!(matches!(err, Error::Io { .. }), "{err}");
    }
}
