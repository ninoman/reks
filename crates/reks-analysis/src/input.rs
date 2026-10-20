//! Decode and conditioning (phase 3 task 1): one track as mono `f32` at the
//! rates the analysers want.
//!
//! - [`ANALYSIS_RATE`] (44.1 kHz) for key detection and waveforms, and the
//!   rate every sample position reks-analysis returns is stated at.
//! - [`BEAT_RATE`] (22.05 kHz), what the Beat This! mel front end expects.
//!
//! Both are resampled from the file's own rate with `rubato` (through
//! [`reks_audio::StreamResampler`], which removes the resampler's delay), so
//! sample 0 at either rate is the same instant: the first sample Symphonia's
//! gapless decode returns, after the encoder delay is trimmed. The delay and
//! padding the container reported are kept for the per-codec offset
//! calibration (phase 3 task 4, docs/decisions.md D4).

use std::path::Path;

use reks_audio::{decode_f32, Layout, StreamResampler};

/// Rate of [`AnalysisInput::mono`], and of all sample positions in this crate.
pub const ANALYSIS_RATE: u32 = 44_100;
/// Rate of [`AnalysisInput::beat_mono`]: the Beat This! model's input rate.
pub const BEAT_RATE: u32 = 22_050;

/// Errors building an [`AnalysisInput`].
#[derive(Debug, thiserror::Error)]
pub enum InputError {
    #[error(transparent)]
    Audio(#[from] reks_audio::Error),
    #[error("cannot resample {from} Hz to {to} Hz: {reason}")]
    Resample { from: u32, to: u32, reason: String },
    #[error("sample rate must be positive")]
    ZeroRate,
}

/// A decoded, conditioned track, ready for the analysers.
#[derive(Clone, PartialEq)]
pub struct AnalysisInput {
    /// Mono (mean of all channels) at [`ANALYSIS_RATE`], full scale ±1.0.
    pub mono: Vec<f32>,
    /// The same signal at [`BEAT_RATE`].
    pub beat_mono: Vec<f32>,
    /// The file's own sample rate and channel count.
    pub source_rate: u32,
    pub source_channels: u16,
    /// Encoder delay in source frames as the container reported it; already
    /// trimmed from the PCM. `None` for formats without one (PCM, FLAC).
    pub encoder_delay: Option<u32>,
    /// Encoder padding in source frames; already trimmed.
    pub encoder_padding: Option<u32>,
}

impl AnalysisInput {
    /// Decodes and conditions `path`. Blocking and CPU-heavy: call it from a
    /// worker thread (`spawn_blocking`).
    pub fn from_file(path: &Path) -> Result<Self, InputError> {
        let pcm = decode_f32(path, Layout::Mono)?;
        tracing::debug!(
            path = %path.display(),
            rate = pcm.sample_rate,
            channels = pcm.source_channels,
            delay = ?pcm.encoder_delay,
            padding = ?pcm.encoder_padding,
            "decoded for analysis"
        );
        let mut input = Self::from_mono(pcm.samples, pcm.sample_rate)?;
        input.source_channels = pcm.source_channels;
        input.encoder_delay = pcm.encoder_delay;
        input.encoder_padding = pcm.encoder_padding;
        Ok(input)
    }

    /// Conditions an in-memory mono signal at `rate` Hz (generated test
    /// signals, or PCM decoded elsewhere).
    pub fn from_mono(samples: Vec<f32>, rate: u32) -> Result<Self, InputError> {
        if rate == 0 {
            return Err(InputError::ZeroRate);
        }
        let beat_mono = resample(&samples, rate, BEAT_RATE)?;
        let mono = if rate == ANALYSIS_RATE {
            samples
        } else {
            resample(&samples, rate, ANALYSIS_RATE)?
        };
        Ok(Self {
            mono,
            beat_mono,
            source_rate: rate,
            source_channels: 1,
            encoder_delay: None,
            encoder_padding: None,
        })
    }

    /// Length of [`Self::mono`] in milliseconds, rounded down.
    pub fn duration_ms(&self) -> u64 {
        self.mono.len() as u64 * 1000 / u64::from(ANALYSIS_RATE)
    }
}

impl std::fmt::Debug for AnalysisInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnalysisInput")
            .field("frames", &self.mono.len())
            .field("beat_frames", &self.beat_mono.len())
            .field("source_rate", &self.source_rate)
            .field("source_channels", &self.source_channels)
            .field("encoder_delay", &self.encoder_delay)
            .field("encoder_padding", &self.encoder_padding)
            .finish()
    }
}

/// Mono `from` → `to` Hz; output length `ceil(len * to / from)`.
fn resample(samples: &[f32], from: u32, to: u32) -> Result<Vec<f32>, InputError> {
    if from == to {
        return Ok(samples.to_vec());
    }
    let err = |e: reks_audio::resample::ResampleError| InputError::Resample {
        from,
        to,
        reason: e.to_string(),
    };
    let mut rs = StreamResampler::<f32>::new(from, to, 1).map_err(err)?;
    let mut out = Vec::with_capacity(rs.output_len(samples.len() as u64) as usize);
    rs.push(samples, &mut out).map_err(err)?;
    rs.finish(&mut out).map_err(err)?;
    Ok(out)
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

    /// Index of the largest |sample| in `s[from..to]`.
    fn peak(s: &[f32], from: usize, to: usize) -> usize {
        (from..to.min(s.len()))
            .max_by(|&a, &b| s[a].abs().total_cmp(&s[b].abs()))
            .unwrap()
    }

    /// Clicks stay where they are in the generated signal, at both rates:
    /// the loudest sample near each onset matches the reference's.
    fn assert_clicks_aligned(input: &AnalysisInput, tolerance_ms: f64) {
        let reference: Vec<f32> = reks_fixtures_gen::render()
            .into_iter()
            .map(|s| f32::from(s) / 32_768.0)
            .collect();
        for n in [1, 16, 63] {
            let onset = reks_fixtures_gen::beat_onset(n);
            let want = peak(&reference, onset, onset + 441) as f64 / 44_100.0;
            for (signal, rate) in [(&input.mono, ANALYSIS_RATE), (&input.beat_mono, BEAT_RATE)] {
                let at = (want * f64::from(rate)) as usize;
                let window = (rate / 200) as usize; // 5 ms either side
                let got = peak(signal, at - window, at + window) as f64 / f64::from(rate);
                assert!(
                    (got - want).abs() * 1000.0 <= tolerance_ms,
                    "beat {n} at {rate} Hz: peak at {got:.5} s, reference {want:.5} s"
                );
            }
        }
    }

    #[test]
    fn wav_keeps_44k1_and_halves_for_the_beat_model() {
        let input = AnalysisInput::from_file(&fixture("sine-click-128.wav")).unwrap();
        assert_eq!(input.source_rate, 44_100);
        assert_eq!(input.mono.len(), reks_fixtures_gen::DURATION_SAMPLES);
        assert_eq!(
            input.beat_mono.len(),
            reks_fixtures_gen::DURATION_SAMPLES / 2
        );
        assert_eq!(input.duration_ms(), 30_000);
        assert_eq!(input.encoder_delay, None);
        // The 44.1 kHz signal is the file, untouched.
        let expected = reks_fixtures_gen::render();
        assert_eq!(input.mono[1000], f32::from(expected[1000]) / 32_768.0);
        assert_clicks_aligned(&input, 0.15);
    }

    #[test]
    fn stereo_96k_is_mixed_down_and_resampled() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hires.wav");
        reks_fixtures_gen::write_wav_hires(std::fs::File::create(&path).unwrap()).unwrap();
        let input = AnalysisInput::from_file(&path).unwrap();
        assert_eq!((input.source_rate, input.source_channels), (96_000, 2));
        let frames = u64::from(reks_fixtures_gen::SHORT_MS) * 44_100 / 1000;
        assert_eq!(input.mono.len() as u64, frames);
        assert_eq!(input.beat_mono.len() as u64, frames.div_ceil(2));
        // Beat 4's click lands where it does in the generated 96 kHz signal.
        let reference: Vec<f32> = reks_fixtures_gen::render_stereo(96_000)
            .as_chunks::<2>()
            .0
            .iter()
            .map(|[l, r]| ((l + r) / 2.0) as f32)
            .collect();
        let onset = reks_fixtures_gen::beat_onset_at(96_000, 4);
        let want = peak(&reference, onset, onset + 960) as f64 / 96_000.0;
        for (signal, rate) in [(&input.mono, ANALYSIS_RATE), (&input.beat_mono, BEAT_RATE)] {
            let at = (want * f64::from(rate)) as usize;
            let got = peak(signal, at - 100, at + 100) as f64 / f64::from(rate);
            assert!(
                (got - want).abs() * 1000.0 < 0.15,
                "{rate} Hz: {got} vs {want}"
            );
        }
    }

    #[test]
    fn mp3_reports_delay_and_stays_on_the_beat() {
        let input = AnalysisInput::from_file(&fixture("sine-click-128.mp3")).unwrap();
        assert_eq!(input.encoder_delay, Some(1105));
        assert!(input.encoder_padding.is_some());
        assert_eq!(input.mono.len(), reks_fixtures_gen::DURATION_SAMPLES);
        // Gapless trim puts the clicks where the WAV has them.
        assert_clicks_aligned(&input, 0.15);
    }

    #[test]
    fn lossless_formats_condition_identically() {
        for name in ["flac", "aiff"] {
            let input =
                AnalysisInput::from_file(&fixture(&format!("sine-click-128.{name}"))).unwrap();
            assert_eq!(input.source_rate, 44_100, "{name}");
            assert_eq!(
                input.mono.len(),
                reks_fixtures_gen::DURATION_SAMPLES,
                "{name}"
            );
            assert_eq!(input.encoder_delay, None, "{name}");
            assert_clicks_aligned(&input, 0.15);
        }
    }

    /// Symphonia 0.6 reports no delay for ffmpeg's AAC (the priming is in
    /// the MP4 edit list, which it does not apply), so the audio comes out
    /// 1024 frames late. Pinned here so a Symphonia change shows up; the
    /// offset convention is phase 3 task 4.
    #[test]
    fn aac_keeps_its_1024_priming_frames() {
        let input = AnalysisInput::from_file(&fixture("sine-click-128.m4a")).unwrap();
        assert_eq!((input.encoder_delay, input.encoder_padding), (None, None));
        let reference: Vec<f32> = reks_fixtures_gen::render()
            .into_iter()
            .map(|s| f32::from(s) / 32_768.0)
            .collect();
        let correlation = |lag: usize| -> f64 {
            (10_000..30_000)
                .map(|i| f64::from(reference[i]) * f64::from(input.mono[i + lag]))
                .sum()
        };
        let lag = (0..1200).max_by(|&a, &b| correlation(a).total_cmp(&correlation(b)));
        assert_eq!(lag, Some(1024));
    }

    #[test]
    fn zero_rate_is_rejected() {
        assert!(matches!(
            AnalysisInput::from_mono(vec![0.0; 10], 0),
            Err(InputError::ZeroRate)
        ));
    }
}
