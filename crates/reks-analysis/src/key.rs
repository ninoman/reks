//! Key detection: a Rust port of libKeyFinder's pipeline (D12).
//!
//! Ported from libKeyFinder by Ibrahim Sha'ath (2011–2015), maintained by
//! the Mixxx project (<https://github.com/mixxxdj/libkeyfinder>, `main` as of
//! 2024-11), GPL-3.0-or-later. This module is a reimplementation of its
//! `KeyFinder::keyOfAudio` path; the tone profiles and octave weights are
//! data transcribed from its `constants.cpp`. Like the rest of reks it is
//! GPL-3.0-or-later.
//!
//! Pipeline (same order and constants as the C++):
//!
//! 1. Drop the trailing `len % factor` samples, then low-pass with a
//!    160th-order windowed-sinc FIR (brick-wall response designed with a
//!    2048-point inverse FFT, Hamming window, corner = top band × 1.012) and
//!    keep every `factor`-th output. `factor = ⌊sr / 2 / (top band × 1.1)⌋`,
//!    10 at 44.1 kHz (→ 4410 Hz). Other rates work the same way.
//! 2. Blackman-windowed 16384-point FFT frames every 4096 samples; the tail
//!    is zero-padded to a whole hop.
//! 3. Per frame, 72 chroma bands (C1–B6, 6 octaves × 12 semitones) from a
//!    "direct spectral kernel": a Hann-shaped window over the FFT bins around
//!    each band's centre (width 0.8 semitone-Q), normalised to model a CQT.
//! 4. Average all frames into one 72-band vector.
//! 5. Cosine similarity against the major and minor tone profiles (scaled
//!    per octave) rotated to all 12 tonics; best score wins, ties keep the
//!    earlier key in libKeyFinder's order (A major, A minor, B♭ major, …).
//!    A zero vector scores 0 everywhere and is reported as silence.
//!
//! [`KeyAnalysis::confidence`] is reks's own addition (libKeyFinder has none):
//! the winner's relative margin over the runner-up, `(best − second) / best`.
//! It is uncalibrated; parity measurement (phase 3 task 12) decides whether
//! it is useful as shown.
//!
//! Keys are [`reks_core::Key`], which already provides canonical spelling,
//! Camelot, Open Key and parsing. The rekordbox PDB stores key *names* in its
//! keys table (no numeric enum is documented in research §2/§3), so export
//! uses the canonical name; Engine DJ's 0–23 enum is Phase 5 (`engine_key`).
//! [`to_keyfinder_index`] / [`from_keyfinder_index`] map to libKeyFinder's
//! `key_t` for comparisons against the C++ library.

use reks_core::{Key, Mode};
use rustfft::num_complex::Complex;
use rustfft::FftPlanner;

const SEMITONES: usize = 12;
const OCTAVES: usize = 6;
const BANDS: usize = SEMITONES * OCTAVES;
const FFT_FRAME_SIZE: usize = 16384;
const HOP_SIZE: usize = FFT_FRAME_SIZE / 4;
const DIRECT_SK_STRETCH: f64 = 0.8;
const LPF_ORDER: usize = 160;
const LPF_DESIGN_FFT: usize = 2048;
/// libKeyFinder's `key_t::SILENCE`.
const KEYFINDER_SILENCE: u8 = 24;

/// Tone profiles, index 0 = tonic (libKeyFinder `MAJOR_PROFILE`/`MINOR_PROFILE`).
const MAJOR_PROFILE: [f64; SEMITONES] = [
    7.239_005_026_181_452,
    3.503_511_667_251_587,
    3.584_451_775_366_494,
    2.845_118_164_786_76,
    5.818_988_921_185_499,
    4.558_650_574_153_21,
    2.447_788_505_455_065,
    6.994_731_921_468_295,
    3.391_066_136_735_048_5,
    4.556_142_566_551_435,
    4.073_926_666_635_236,
    4.459_327_573_788_869,
];
const MINOR_PROFILE: [f64; SEMITONES] = [
    7.002_550_450_602_844,
    3.143_602_790_159_967,
    4.359_043_197_149_625,
    5.404_181_207_189_341,
    3.672_344_208_793_061,
    4.089_711_849_177_979,
    3.907_914_359_915_54,
    6.199_602_885_623_165,
    3.634_246_256_252_774,
    2.872_411_910_798_755_6,
    5.354_679_997_945_427,
    3.832_420_385_950_483_5,
];
/// Per-octave weights, C1 octave first (libKeyFinder `OCTAVE_WEIGHTS`).
const OCTAVE_WEIGHTS: [f64; OCTAVES] = [
    0.399_972_675_5,
    0.556_344_252_483_006_5,
    0.524_966_363_451_435_4,
    0.608_475_483_842_777_3,
    0.598_981_156_8,
    0.490_724_353_179_61,
];

/// Result of [`detect_key`].
#[derive(Debug, Clone, PartialEq)]
pub struct KeyAnalysis {
    /// Detected key; `None` when the input is silent (libKeyFinder `SILENCE`).
    pub key: Option<Key>,
    /// Winner's margin over the runner-up, 0..=1; 0 for silence. Uncalibrated.
    pub confidence: f32,
    /// Cosine similarity of the winning profile, 0..=1; 0 for silence.
    pub similarity: f32,
    /// Average chroma folded across octaves, index 0 = C; max-normalised to 1
    /// (all zero for silence).
    pub chroma: [f32; SEMITONES],
}

/// Input that key detection cannot analyse.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KeyError {
    /// Below ~4.35 kHz the top chroma band (B6, 1975.5 Hz) is above Nyquist.
    #[error("sample rate {0} Hz is too low for key detection")]
    SampleRateTooLow(u32),
    /// A NaN or infinite sample (decoder bug); libKeyFinder rejects these too.
    #[error("non-finite sample at index {0}")]
    NonFiniteSample(usize),
}

/// Detects the key of mono PCM (`samples` in −1..=1 at `sample_rate`).
///
/// 44.1 kHz is the expected rate; any rate ≥ ~4.35 kHz works. Silence and
/// inputs shorter than one hop do not error: they report `key: None` or a
/// low-information result.
pub fn detect_key(samples: &[f32], sample_rate: u32) -> Result<KeyAnalysis, KeyError> {
    if let Some(i) = samples.iter().position(|s| !s.is_finite()) {
        return Err(KeyError::NonFiniteSample(i));
    }
    let (downsampled, frame_rate) = lowpass_downsample(samples, sample_rate)?;
    let chroma = mean_chromagram(&downsampled, frame_rate);
    let scores = classify_scores(&chroma);
    Ok(analysis_from(&chroma, &scores))
}

/// libKeyFinder `key_t` index: 0 = A major, 1 = A minor, 2 = B♭ major, …,
/// 23 = A♭ minor; `None` (silence) is 24.
pub fn to_keyfinder_index(key: Option<Key>) -> u8 {
    match key {
        None => KEYFINDER_SILENCE,
        Some(k) => {
            let from_a = (k.tonic() + 12 - 9) % 12;
            from_a * 2 + u8::from(k.mode() == Mode::Minor)
        }
    }
}

/// Inverse of [`to_keyfinder_index`]; `None` for 24 (silence) and above.
pub fn from_keyfinder_index(index: u8) -> Option<Key> {
    if index >= KEYFINDER_SILENCE {
        return None;
    }
    let mode = if index.is_multiple_of(2) {
        Mode::Major
    } else {
        Mode::Minor
    };
    Some(Key::new(9 + index / 2, mode))
}

/// Centre frequency of chroma band `band` (0 = C1 … 71 = B6), equal
/// temperament at A4 = 440 Hz (band 45); matches libKeyFinder's table.
fn band_frequency(band: usize) -> f64 {
    440.0 * 2f64.powf((band as f64 - 45.0) / 12.0)
}

fn top_frequency() -> f64 {
    band_frequency(BANDS - 1)
}

fn hamming(n: usize, len: usize) -> f64 {
    let n = n as f64;
    let d = (len - 1) as f64;
    0.54 - 0.46 * (2.0 * std::f64::consts::PI * n / d).cos()
}

fn blackman(n: usize, len: usize) -> f64 {
    let n = n as f64;
    let d = (len - 1) as f64;
    let pi = std::f64::consts::PI;
    0.42 - 0.5 * (2.0 * pi * n / d).cos() + 0.08 * (4.0 * pi * n / d).cos()
}

/// FIR coefficients of libKeyFinder's `LowPassFilter(160, rate, corner, 2048)`,
/// already divided by their sum (the C++ divides the input by the gain).
fn lowpass_coefficients(sample_rate: u32, corner: f64) -> Vec<f64> {
    let n = LPF_DESIGN_FFT;
    let cutoff = corner / f64::from(sample_rate);
    // Half spectrum as the C++ fills it for FFTW's c2r transform: bins
    // 0..n/2 are tau below the cutoff, bin n/2 copies bin n/2 - 1's rule.
    let tau = 0.5 / cutoff;
    let pass = |i: usize| {
        if i as f64 / n as f64 <= cutoff {
            tau
        } else {
            0.0
        }
    };
    let half: Vec<f64> = (0..n / 2).map(pass).chain([pass(n / 2 - 1)]).collect();
    let centre = LPF_ORDER / 2;
    let len = LPF_ORDER + 1;
    // Real inverse DFT of the Hermitian spectrum at lags -centre..=centre.
    let mut coeffs: Vec<f64> = (0..len)
        .map(|i| {
            let lag = i as f64 - centre as f64;
            let w = 2.0 * std::f64::consts::PI * lag / n as f64;
            let mut v = half[0] + half[n / 2] * (std::f64::consts::PI * lag).cos();
            for (k, h) in half.iter().enumerate().take(n / 2).skip(1) {
                if *h != 0.0 {
                    v += 2.0 * h * (w * k as f64).cos();
                }
            }
            v * hamming(i, len)
        })
        .collect();
    let gain: f64 = coeffs.iter().sum();
    if gain != 0.0 {
        for c in &mut coeffs {
            *c /= gain;
        }
    }
    coeffs
}

/// libKeyFinder `preprocess` for one whole buffer: truncate to a multiple of
/// the factor, low-pass, decimate. Returns the signal and its integer rate.
fn lowpass_downsample(samples: &[f32], sample_rate: u32) -> Result<(Vec<f64>, u32), KeyError> {
    let factor = (f64::from(sample_rate) / 2.0 / (top_frequency() * 1.10)).floor() as usize;
    if factor == 0 {
        return Err(KeyError::SampleRateTooLow(sample_rate));
    }
    let coeffs = lowpass_coefficients(sample_rate, top_frequency() * 1.012);
    let x = &samples[..samples.len() - samples.len() % factor];
    let delay = LPF_ORDER / 2;
    let out: Vec<f64> = (0..x.len() / factor)
        .map(|j| {
            let centre = j * factor;
            // y[m] = Σ c[k]·x[m − delay + k], zero outside the buffer.
            let first = delay.saturating_sub(centre);
            let last = coeffs.len().min(x.len() + delay - centre);
            (first..last)
                .map(|k| coeffs[k] * f64::from(x[centre + k - delay]))
                .sum()
        })
        .collect();
    // The C++ keeps the rate as an unsigned int, so this truncates.
    Ok((out, sample_rate / factor as u32))
}

/// One chroma band's slice of FFT bins and weights.
#[derive(Debug)]
struct BandKernel {
    first_bin: usize,
    weights: Vec<f64>,
}

/// libKeyFinder `ChromaTransform` for `frame_rate`.
fn chroma_kernel(frame_rate: u32) -> Vec<BandKernel> {
    let q = DIRECT_SK_STRETCH * (2f64.powf(1.0 / SEMITONES as f64) - 1.0);
    (0..BANDS)
        .map(|band| {
            let freq = band_frequency(band);
            let centre = freq * FFT_FRAME_SIZE as f64 / f64::from(frame_rate);
            let width = centre * q;
            let begin = centre - width / 2.0;
            let end = begin + width;
            let first_bin = begin.ceil().max(0.0) as usize;
            let last_bin = (end.floor().max(0.0) as usize).min(FFT_FRAME_SIZE / 2);
            let mut weights: Vec<f64> = (first_bin..=last_bin)
                .map(|bin| 1.0 - (2.0 * std::f64::consts::PI * (bin as f64 - begin) / width).cos())
                .collect();
            let sum: f64 = weights.iter().sum();
            if sum > 0.0 {
                for w in &mut weights {
                    *w = *w / sum * freq;
                }
            }
            BandKernel { first_bin, weights }
        })
        .collect()
}

/// Start offsets of the FFT frames libKeyFinder analyses for `len` samples:
/// whole frames every hop, then the remainder zero-padded to whole hops.
fn frame_starts(len: usize) -> Vec<usize> {
    let whole = if len >= FFT_FRAME_SIZE {
        1 + (len - FFT_FRAME_SIZE) / HOP_SIZE
    } else {
        0
    };
    let rest = len - whole * HOP_SIZE;
    let padded = rest.div_ceil(HOP_SIZE);
    (0..whole + padded).map(|h| h * HOP_SIZE).collect()
}

/// Spectrum analysis plus `Chromagram::collapseToOneHop`: the mean 72-band
/// chroma over all frames (zeros when there are none).
fn mean_chromagram(signal: &[f64], frame_rate: u32) -> [f64; BANDS] {
    let mut mean = [0.0; BANDS];
    let starts = frame_starts(signal.len());
    if starts.is_empty() {
        return mean;
    }
    let kernel = chroma_kernel(frame_rate);
    let window: Vec<f64> = (0..FFT_FRAME_SIZE)
        .map(|n| blackman(n, FFT_FRAME_SIZE))
        .collect();
    let fft = FftPlanner::<f64>::new().plan_fft_forward(FFT_FRAME_SIZE);
    let mut buf = vec![Complex::new(0.0, 0.0); FFT_FRAME_SIZE];
    let mut scratch = vec![Complex::new(0.0, 0.0); fft.get_inplace_scratch_len()];
    let hops = starts.len() as f64;
    for start in starts {
        for (n, slot) in buf.iter_mut().enumerate() {
            let s = signal.get(start + n).copied().unwrap_or(0.0);
            *slot = Complex::new(s * window[n], 0.0);
        }
        fft.process_with_scratch(&mut buf, &mut scratch);
        for (band, k) in kernel.iter().enumerate() {
            let sum: f64 = k
                .weights
                .iter()
                .enumerate()
                .map(|(j, w)| buf[k.first_bin + j].norm() * w)
                .sum();
            mean[band] += sum / hops;
        }
    }
    mean
}

/// Cosine similarity for all 24 keys, in libKeyFinder `key_t` order.
fn classify_scores(chroma: &[f64; BANDS]) -> [f64; 24] {
    let mut scores = [0.0; 24];
    for (i, pair) in scores.as_chunks_mut::<2>().0.iter_mut().enumerate() {
        pair[0] = cosine_similarity(chroma, &MAJOR_PROFILE, i);
        pair[1] = cosine_similarity(chroma, &MINOR_PROFILE, i);
    }
    scores
}

/// libKeyFinder `ToneProfile::cosineSimilarity` with the tonic `from_a`
/// semitones above A. Band 0 is C, which is 3 semitones above A.
fn cosine_similarity(chroma: &[f64; BANDS], profile: &[f64; SEMITONES], from_a: usize) -> f64 {
    let mut intersection = 0.0;
    let mut profile_norm = 0.0;
    let mut input_norm = 0.0;
    for (o, weight) in OCTAVE_WEIGHTS.iter().enumerate() {
        for s in 0..SEMITONES {
            let p = weight * profile[(s + 3 + SEMITONES - from_a) % SEMITONES];
            let x = chroma[o * SEMITONES + s];
            intersection += x * p;
            profile_norm += p * p;
            input_norm += x * x;
        }
    }
    if profile_norm > 0.0 && input_norm > 0.0 {
        intersection / (profile_norm.sqrt() * input_norm.sqrt())
    } else {
        0.0
    }
}

fn analysis_from(chroma: &[f64; BANDS], scores: &[f64; 24]) -> KeyAnalysis {
    // Silence profile scores 0; a key must beat it strictly (KeyClassifier).
    let mut best = 0.0;
    let mut best_index = KEYFINDER_SILENCE;
    for (i, &s) in scores.iter().enumerate() {
        if s > best {
            best = s;
            best_index = i as u8;
        }
    }
    let key = from_keyfinder_index(best_index);
    let confidence = if key.is_some() {
        let second = scores
            .iter()
            .enumerate()
            .filter(|&(i, _)| i != usize::from(best_index))
            .map(|(_, &s)| s)
            .fold(0.0, f64::max);
        ((best - second) / best).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let mut folded = [0.0f64; SEMITONES];
    for (band, v) in chroma.iter().enumerate() {
        folded[band % SEMITONES] += v;
    }
    let max = folded.iter().copied().fold(0.0, f64::max);
    let chroma12 = folded.map(|v| if max > 0.0 { (v / max) as f32 } else { 0.0 });
    KeyAnalysis {
        key,
        confidence: confidence as f32,
        similarity: if key.is_some() { best as f32 } else { 0.0 },
        chroma: chroma12,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn band_frequencies_match_libkeyfinder_table() {
        // First, A1 (55 Hz), A4 and last entries of the C++ FREQUENCIES table.
        for (band, hz) in [
            (0, 32.703_195_662_574_8),
            (9, 55.0),
            (45, 440.0),
            (71, 1_975.533_205_024_5),
        ] {
            assert!((band_frequency(band) - hz).abs() < 1e-9, "band {band}");
        }
    }

    #[test]
    fn keyfinder_index_round_trips_and_matches_key_t() {
        assert_eq!(to_keyfinder_index(None), 24);
        assert_eq!(from_keyfinder_index(24), None);
        assert_eq!(from_keyfinder_index(200), None);
        // Spot checks against libKeyFinder's key_t enum.
        for (index, name) in [(0, "A"), (1, "Am"), (2, "Bb"), (6, "C"), (7, "Cm")] {
            assert_eq!(from_keyfinder_index(index), Some(Key::parse(name).unwrap()));
        }
        for (index, name) in [(18, "F#"), (19, "F#m"), (22, "Ab"), (23, "Abm")] {
            assert_eq!(from_keyfinder_index(index), Some(Key::parse(name).unwrap()));
        }
        for i in 0..24 {
            assert_eq!(to_keyfinder_index(from_keyfinder_index(i)), i);
        }
    }

    #[test]
    fn downsample_factor_and_rate() {
        let silence = vec![0.0f32; 44_100];
        let (y, rate) = lowpass_downsample(&silence, 44_100).unwrap();
        assert_eq!(rate, 4410);
        assert_eq!(y.len(), 4410);
        let (y, rate) = lowpass_downsample(&silence, 48_000).unwrap();
        assert_eq!(rate, 48_000 / 11);
        assert_eq!(y.len(), 44_100 / 11);
        assert_eq!(
            lowpass_downsample(&silence, 4000),
            Err(KeyError::SampleRateTooLow(4000))
        );
        assert_eq!(
            lowpass_downsample(&silence, 0),
            Err(KeyError::SampleRateTooLow(0))
        );
    }

    #[test]
    fn lowpass_passes_dc_and_kills_highs() {
        let coeffs = lowpass_coefficients(44_100, top_frequency() * 1.012);
        assert_eq!(coeffs.len(), 161);
        assert!((coeffs.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        // Symmetric (linear phase).
        for i in 0..80 {
            assert!((coeffs[i] - coeffs[160 - i]).abs() < 1e-12);
        }
        let response = |hz: f64| {
            let (re, im) = coeffs
                .iter()
                .enumerate()
                .fold((0.0, 0.0), |(re, im), (n, c)| {
                    let w = 2.0 * std::f64::consts::PI * hz / 44_100.0 * n as f64;
                    (re + c * w.cos(), im - c * w.sin())
                });
            f64::hypot(re, im)
        };
        assert!((response(440.0) - 1.0).abs() < 0.01);
        assert!(response(5_000.0) < 0.01);
    }

    #[test]
    fn frame_layout_matches_progressive_then_final_chromagram() {
        assert!(frame_starts(0).is_empty());
        assert_eq!(frame_starts(1), vec![0]);
        // One whole frame consumes one hop; the 12288 left pad to 3 hops.
        assert_eq!(frame_starts(FFT_FRAME_SIZE), vec![0, 4096, 8192, 12288]);
        assert_eq!(frame_starts(FFT_FRAME_SIZE + 1).len(), 1 + 4);
        assert_eq!(frame_starts(26_460).len(), 3 + 4);
    }

    #[test]
    fn kernel_bins_cover_band_centres() {
        let kernel = chroma_kernel(4410);
        for (band, k) in kernel.iter().enumerate() {
            assert!(!k.weights.is_empty(), "band {band}");
            let centre = band_frequency(band) * FFT_FRAME_SIZE as f64 / 4410.0;
            let last = (k.first_bin + k.weights.len() - 1) as f64;
            assert!(
                k.first_bin as f64 <= centre && centre <= last,
                "band {band}"
            );
            let sum: f64 = k.weights.iter().sum();
            assert!((sum - band_frequency(band)).abs() < 1e-9);
        }
    }

    #[test]
    fn profile_rotation_puts_tonic_on_its_band() {
        // A chroma vector that is exactly the C major profile scores 1 for C.
        let c_from_a = 3;
        let mut chroma = [0.0; BANDS];
        for (o, w) in OCTAVE_WEIGHTS.iter().enumerate() {
            for s in 0..SEMITONES {
                chroma[o * SEMITONES + s] = w * MAJOR_PROFILE[s];
            }
        }
        let scores = classify_scores(&chroma);
        assert!((scores[c_from_a * 2] - 1.0).abs() < 1e-12);
        let a = analysis_from(&chroma, &scores);
        assert_eq!(a.key, Some(Key::parse("C").unwrap()));
        assert!(a.confidence > 0.0);
    }

    #[test]
    fn silence_short_and_empty_inputs_report_no_key() {
        for len in [0, 1, 100, 44_100 * 3] {
            let a = detect_key(&vec![0.0; len], 44_100).unwrap();
            assert_eq!(a.key, None, "len {len}");
            assert_eq!(a.confidence, 0.0);
            assert_eq!(a.chroma, [0.0; 12]);
        }
    }

    #[test]
    fn rejects_non_finite_samples() {
        let mut x = vec![0.0f32; 1000];
        x[7] = f32::NAN;
        assert_eq!(detect_key(&x, 44_100), Err(KeyError::NonFiniteSample(7)));
    }
}
