//! Deterministic test signal for the reks audio fixtures: a 440 Hz sine with a
//! click on every beat at 128 BPM, 30 s, 44.1 kHz mono 16-bit PCM.
//!
//! Also two short stereo variants for the converter (Phase 2.6), written as
//! the forms older players reject: [`write_wav_hires`] (24-bit, 96 kHz) and
//! [`write_wav_float`] (32-bit float, 44.1 kHz). Both come out of `hound` as
//! `WAVE_FORMAT_EXTENSIBLE`. Tests generate them on the fly; they are not
//! checked in.
//!
//! The output must be byte-identical on every run and OS, so the sine is built
//! from `+ - * /` on `f64` only (IEEE 754 rounds these exactly the same
//! everywhere) instead of the platform `sin()`, whose last bit may differ.
//! See `fixtures/README.md` for the signal spec.

use std::io::{Seek, Write};

pub mod bars;
pub mod tempo;

/// Samples per second.
pub const SAMPLE_RATE: u32 = 44_100;
/// Tempo of the click track.
pub const BPM: u32 = 128;
/// Length in seconds.
pub const DURATION_SECS: u32 = 30;
/// Length in samples (mono, so also frames).
pub const DURATION_SAMPLES: usize = (SAMPLE_RATE * DURATION_SECS) as usize;
/// Beats in the file: 30 s at 128 BPM is exactly 64 beats (16 bars of 4/4).
pub const BEATS: usize = (DURATION_SECS * BPM / 60) as usize;

/// Sine frequency in Hz.
pub const SINE_HZ: u32 = 440;
/// Sine peak amplitude (full scale = 1.0).
pub const SINE_AMPLITUDE: f64 = 0.25;
/// Click burst frequency in Hz.
pub const CLICK_HZ: u32 = 2_000;
/// Click peak amplitude before its linear decay.
pub const CLICK_AMPLITUDE: f64 = 0.6;
/// Click length in samples (10 ms).
pub const CLICK_SAMPLES: usize = 441;

const FULL_SCALE: f64 = 32_767.0;

/// First sample of beat `n`: `n * 60 / BPM` seconds, rounded to the nearest
/// sample with integer arithmetic (the beat period is 20,671.875 samples).
pub fn beat_onset(n: usize) -> usize {
    let numerator = SAMPLE_RATE as usize * 60;
    let bpm = BPM as usize;
    (n * numerator + bpm / 2) / bpm
}

/// `sin(2π·turns)` for `turns` in `[0, 1)`, using only basic IEEE operations.
///
/// Folds to a quarter period, then evaluates the Taylor series to x^17 with
/// Horner's rule; the error on `[0, π/2]` is below 1e-11.
pub fn sin_turns(turns: f64) -> f64 {
    let (sign, half) = if turns >= 0.5 {
        (-1.0, turns - 0.5)
    } else {
        (1.0, turns)
    };
    let quarter = if half > 0.25 { 0.5 - half } else { half };
    let x = quarter * (2.0 * std::f64::consts::PI);
    let x2 = x * x;
    // 1/(2k+1)! for k = 1..=8, applied innermost first.
    let mut acc = 1.0;
    for k in (1..=8u32).rev() {
        let a = f64::from(2 * k);
        let b = f64::from(2 * k + 1);
        acc = 1.0 - acc * x2 / (a * b);
    }
    sign * x * acc
}

/// Phase of a `freq_hz` sine at sample `i` of a `SAMPLE_RATE` signal, in turns.
fn phase_turns(freq_hz: u32, i: usize) -> f64 {
    phase_turns_at(SAMPLE_RATE, freq_hz, i)
}

/// Phase of a `freq_hz` sine at sample `i` of a `rate` Hz signal, in turns,
/// from exact integer arithmetic so no rounding error accumulates over the file.
fn phase_turns_at(rate: u32, freq_hz: u32, i: usize) -> f64 {
    let rate = u64::from(rate);
    let cycles_num = (u64::from(freq_hz) * i as u64) % rate;
    cycles_num as f64 / rate as f64
}

/// Round half away from zero to a 16-bit sample.
fn quantise(v: f64) -> i16 {
    let scaled = v * FULL_SCALE;
    let rounded = if scaled >= 0.0 {
        (scaled + 0.5).floor()
    } else {
        -((-scaled + 0.5).floor())
    };
    rounded.clamp(-FULL_SCALE, FULL_SCALE) as i16
}

/// Render the full signal as mono 16-bit samples.
pub fn render() -> Vec<i16> {
    let mut mix: Vec<f64> = (0..DURATION_SAMPLES)
        .map(|i| SINE_AMPLITUDE * sin_turns(phase_turns(SINE_HZ, i)))
        .collect();
    for n in 0..BEATS {
        let onset = beat_onset(n);
        for k in 0..CLICK_SAMPLES {
            let Some(slot) = mix.get_mut(onset + k) else {
                break;
            };
            let envelope = (CLICK_SAMPLES - k) as f64 / CLICK_SAMPLES as f64;
            *slot += CLICK_AMPLITUDE * envelope * sin_turns(phase_turns(CLICK_HZ, k));
        }
    }
    mix.into_iter().map(quantise).collect()
}

/// WAV format of the fixture.
pub fn wav_spec() -> hound::WavSpec {
    hound::WavSpec {
        channels: 1,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    }
}

/// Write the rendered signal as a WAV file.
pub fn write_wav<W: Write + Seek>(writer: W) -> Result<(), hound::Error> {
    let mut wav = hound::WavWriter::new(writer, wav_spec())?;
    let mut samples = wav.get_i16_writer(DURATION_SAMPLES as u32);
    for s in render() {
        samples.write_sample(s);
    }
    samples.flush()?;
    wav.finalize()
}

/// Length of the stereo converter fixtures: 8 beats at 128 BPM.
pub const SHORT_MS: u32 = 3_750;
/// Sample rate of [`write_wav_hires`].
pub const HIRES_RATE: u32 = 96_000;
/// Right-channel sine of the stereo fixtures (the left one is [`SINE_HZ`]),
/// so a channel swap shows.
pub const RIGHT_SINE_HZ: u32 = 660;

/// First frame of beat `n` at `rate` Hz, rounded to the nearest frame.
pub fn beat_onset_at(rate: u32, n: usize) -> usize {
    let numerator = rate as usize * 60;
    let bpm = BPM as usize;
    (n * numerator + bpm / 2) / bpm
}

/// The stereo test signal at `rate` Hz for [`SHORT_MS`], full scale ±1.0,
/// interleaved: left 440 Hz at 0.25, right 660 Hz at 0.2, clicks on both
/// channels on every beat.
pub fn render_stereo(rate: u32) -> Vec<f64> {
    let frames = (u64::from(rate) * u64::from(SHORT_MS) / 1000) as usize;
    let mut mix: Vec<f64> = (0..frames)
        .flat_map(|i| {
            [
                SINE_AMPLITUDE * sin_turns(phase_turns_at(rate, SINE_HZ, i)),
                0.2 * sin_turns(phase_turns_at(rate, RIGHT_SINE_HZ, i)),
            ]
        })
        .collect();
    let click_frames = (rate / 100) as usize; // 10 ms
    let beats = (SHORT_MS * BPM / 60_000) as usize;
    for n in 0..beats {
        let onset = beat_onset_at(rate, n);
        for k in 0..click_frames {
            if onset + k >= frames {
                break;
            }
            let envelope = (click_frames - k) as f64 / click_frames as f64;
            let click = CLICK_AMPLITUDE * envelope * sin_turns(phase_turns_at(rate, CLICK_HZ, k));
            mix[(onset + k) * 2] += click;
            mix[(onset + k) * 2 + 1] += click;
        }
    }
    mix
}

/// [`render_stereo`] at [`HIRES_RATE`] as 24-bit integers (round half away
/// from zero), using all 24 bits.
pub fn render_hires() -> Vec<i32> {
    const FULL_SCALE_24: f64 = 8_388_607.0;
    render_stereo(HIRES_RATE)
        .into_iter()
        .map(|v| {
            let scaled = v * FULL_SCALE_24;
            let rounded = if scaled >= 0.0 {
                (scaled + 0.5).floor()
            } else {
                -((-scaled + 0.5).floor())
            };
            rounded.clamp(-FULL_SCALE_24, FULL_SCALE_24) as i32
        })
        .collect()
}

/// Writes [`render_hires`]: 24-bit 96 kHz stereo, `WAVE_FORMAT_EXTENSIBLE`.
pub fn write_wav_hires<W: Write + Seek>(writer: W) -> Result<(), hound::Error> {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: HIRES_RATE,
        bits_per_sample: 24,
        sample_format: hound::SampleFormat::Int,
    };
    let mut wav = hound::WavWriter::new(writer, spec)?;
    for s in render_hires() {
        wav.write_sample(s)?;
    }
    wav.finalize()
}

/// [`render_stereo`] at [`SAMPLE_RATE`] as `f32`.
pub fn render_float() -> Vec<f32> {
    render_stereo(SAMPLE_RATE)
        .into_iter()
        .map(|v| v as f32)
        .collect()
}

/// Writes [`render_float`]: 32-bit float 44.1 kHz stereo,
/// `WAVE_FORMAT_EXTENSIBLE` with the IEEE float subtype.
pub fn write_wav_float<W: Write + Seek>(writer: W) -> Result<(), hound::Error> {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut wav = hound::WavWriter::new(writer, spec)?;
    for s in render_float() {
        wav.write_sample(s)?;
    }
    wav.finalize()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// Largest |sample| the sine alone can produce.
    const SINE_CEILING: i32 = 8_192;

    fn wav_bytes() -> Vec<u8> {
        let mut buf = Cursor::new(Vec::new());
        write_wav(&mut buf).unwrap();
        buf.into_inner()
    }

    #[test]
    fn wav_header_and_length() {
        let bytes = wav_bytes();
        let reader = hound::WavReader::new(Cursor::new(bytes)).unwrap();
        let spec = reader.spec();
        assert_eq!(spec.sample_rate, 44_100);
        assert_eq!(spec.channels, 1);
        assert_eq!(spec.bits_per_sample, 16);
        assert_eq!(spec.sample_format, hound::SampleFormat::Int);
        assert_eq!(reader.duration() as usize, 1_323_000);
        assert_eq!(reader.len() as usize, DURATION_SAMPLES);
    }

    #[test]
    fn beat_onsets_follow_128_bpm() {
        assert_eq!(BEATS, 64);
        assert_eq!(beat_onset(0), 0);
        // 64 beats at 128 BPM span exactly 30 s.
        assert_eq!(beat_onset(BEATS), DURATION_SAMPLES);
        for n in 0..BEATS {
            let gap = beat_onset(n + 1) - beat_onset(n);
            assert!(gap == 20_671 || gap == 20_672, "beat {n}: gap {gap}");
            // Never more than half a sample from the exact position.
            let exact = n as f64 * 44_100.0 * 60.0 / 128.0;
            assert!((beat_onset(n) as f64 - exact).abs() <= 0.5);
        }
    }

    #[test]
    fn clicks_only_at_beat_positions() {
        let samples = render();
        assert_eq!(samples.len(), DURATION_SAMPLES);
        let mut in_click = vec![false; samples.len()];
        for n in 0..BEATS {
            let onset = beat_onset(n);
            let window = &samples[onset..onset + CLICK_SAMPLES];
            let peak = window.iter().map(|&s| i32::from(s).abs()).max().unwrap();
            assert!(peak > 10_000, "beat {n} at {onset}: peak {peak}");
            // The click starts at the onset: its first quarter period is loud.
            let early = samples[onset..onset + 12]
                .iter()
                .map(|&s| i32::from(s).abs())
                .max()
                .unwrap();
            assert!(early > 10_000, "beat {n}: click not at onset");
            in_click[onset..onset + CLICK_SAMPLES].fill(true);
        }
        for (i, &s) in samples.iter().enumerate() {
            if !in_click[i] {
                assert!(i32::from(s).abs() <= SINE_CEILING, "stray peak at {i}");
            }
        }
    }

    #[test]
    fn no_clipping() {
        let peak = render().iter().map(|&s| i32::from(s).abs()).max().unwrap();
        assert!(peak < 32_000, "peak {peak}");
    }

    #[test]
    fn polynomial_sine_matches_libm() {
        for i in 0..100_000 {
            let t = f64::from(i) / 100_000.0;
            let expected = (t * 2.0 * std::f64::consts::PI).sin();
            assert!((sin_turns(t) - expected).abs() < 1e-9, "t = {t}");
        }
    }

    #[test]
    fn render_is_deterministic() {
        assert_eq!(wav_bytes(), wav_bytes());
    }

    #[test]
    fn hires_fixture() {
        let mut buf = Cursor::new(Vec::new());
        write_wav_hires(&mut buf).unwrap();
        let bytes = buf.into_inner();
        // hound writes EXTENSIBLE above 16 bits: the form older players reject.
        assert_eq!(&bytes[20..22], &[0xFE, 0xFF]);
        let reader = hound::WavReader::new(Cursor::new(bytes)).unwrap();
        assert_eq!(reader.spec().sample_rate, 96_000);
        assert_eq!(reader.spec().bits_per_sample, 24);
        assert_eq!(reader.duration(), 360_000);
        let samples = render_hires();
        assert_eq!(samples.len(), 720_000);
        // Uses the low byte: not 16-bit audio padded to 24.
        assert!(samples.iter().filter(|&&s| s & 0xFF != 0).count() > 700_000);
        let peak = samples.iter().map(|s| s.abs()).max().unwrap();
        assert!((6_500_000..8_388_607).contains(&peak), "peak {peak}");
        // Clicks on the beat in both channels; the 2 kHz burst peaks a
        // quarter period (12 frames) after the onset.
        let onset = beat_onset_at(HIRES_RATE, 4);
        assert_eq!(onset, 180_000);
        assert!(samples[(onset + 12) * 2].abs() > 3_000_000);
        assert!(samples[(onset + 12) * 2 + 1].abs() > 3_000_000);
    }

    #[test]
    fn float_fixture() {
        let mut buf = Cursor::new(Vec::new());
        write_wav_float(&mut buf).unwrap();
        let reader = hound::WavReader::new(Cursor::new(buf.into_inner())).unwrap();
        assert_eq!(reader.spec().sample_format, hound::SampleFormat::Float);
        assert_eq!(reader.duration(), 165_375);
        let peak = render_float().iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!((0.8..1.0).contains(&peak), "peak {peak}");
    }

    #[test]
    fn matches_checked_in_wav() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../audio/sine-click-128.wav");
        let golden = std::fs::read(path).unwrap();
        assert!(
            golden == wav_bytes(),
            "{path} differs from generator output"
        );
    }
}
