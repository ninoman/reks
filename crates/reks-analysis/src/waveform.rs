//! Waveform columns (Phase 3 task 6, D13): three frequency bands plus a mono
//! height, one column per 1/150 s, computed in a single streaming pass.
//!
//! The band split follows Mixxx's waveform analyser (`analyzerwaveform.cpp`,
//! GPL-2.0-or-later): 4th-order Bessel filters with crossovers at 600 Hz and
//! 4 kHz, and the peak of each band per column. This is a reimplementation,
//! not a port: the filters are designed here from the analog Bessel prototype
//! (bilinear transform with pre-warping), and the mid band is a 600 Hz
//! high-pass cascaded with a 4 kHz low-pass rather than Mixxx's band-pass
//! transform. The values are linear and uncalibrated; the Pioneer encoders in
//! `reks-anlz` apply the per-band gain/gamma that task 8 fits against
//! rekordbox.

/// Columns per second of audio; the rate of PWV3/PWV5/PWV7 (half-frames at
/// 75 frames/s).
pub const COLUMNS_PER_SECOND: u32 = 150;
/// Low/mid crossover, Hz (Mixxx's value; calibration may move it).
pub const LOW_MID_CROSSOVER_HZ: f64 = 600.0;
/// Mid/high crossover, Hz (Mixxx's value; calibration may move it).
pub const MID_HIGH_CROSSOVER_HZ: f64 = 4000.0;
/// Largest mono height (5 bits, as PWAV/PWV3/PWV5 store it).
pub const MONO_MAX: u8 = 31;

/// 4th-order Bessel prototype poles (upper half plane), normalised so the
/// magnitude response is -3 dB at 1 rad/s. Roots of the reverse Bessel
/// polynomial s⁴ + 10s³ + 45s² + 105s + 105 divided by its -3 dB frequency
/// (2.113917674904216 rad/s).
const BESSEL4_POLES: [(f64, f64); 2] = [
    (-1.370_067_830_551_445, 0.410_249_717_493_751_8),
    (-0.995_208_764_350_274_3, 1.257_105_739_454_665),
];

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WaveformError {
    #[error("sample rate must be positive")]
    ZeroSampleRate,
    #[error("channel count must be positive")]
    ZeroChannels,
    #[error("crossover {hz} Hz is not below Nyquist for {sample_rate} Hz audio")]
    CrossoverAboveNyquist { hz: u32, sample_rate: u32 },
}

/// Per-column waveform data at [`COLUMNS_PER_SECOND`]. All four vectors have
/// the same length: one column per started 1/150 s of audio, so a partial
/// last column counts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WaveformColumns {
    /// Peak of the < 600 Hz band, 0..=255 (linear, pre-calibration).
    pub low: Vec<u8>,
    /// Peak of the 600 Hz – 4 kHz band, 0..=255 (linear, pre-calibration).
    pub mid: Vec<u8>,
    /// Peak of the > 4 kHz band, 0..=255 (linear, pre-calibration).
    pub high: Vec<u8>,
    /// Peak of the unfiltered mono signal, 0..=[`MONO_MAX`].
    pub mono: Vec<u8>,
}

impl WaveformColumns {
    /// Number of columns.
    pub fn len(&self) -> usize {
        self.mono.len()
    }

    pub fn is_empty(&self) -> bool {
        self.mono.is_empty()
    }

    /// True when all four vectors have the same length.
    pub fn is_consistent(&self) -> bool {
        let n = self.mono.len();
        self.low.len() == n && self.mid.len() == n && self.high.len() == n
    }
}

/// Number of columns for `frames` sample frames at `sample_rate`:
/// ⌈frames × 150 / sample_rate⌉. Zero for a zero sample rate.
pub fn column_count(frames: u64, sample_rate: u32) -> u64 {
    if sample_rate == 0 {
        return 0;
    }
    let num = u128::from(frames) * u128::from(COLUMNS_PER_SECOND);
    let den = u128::from(sample_rate);
    u64::try_from(num.div_ceil(den)).unwrap_or(u64::MAX)
}

/// One biquad section, transposed direct form II, f64 state.
#[derive(Debug, Clone, Copy)]
struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
    z: [f64; 2],
}

impl Biquad {
    /// Bilinear transform of the analog section
    /// (b2·s² + b1·s + b0) / (s² + a1·s + a0) with s = k·(1 − z⁻¹)/(1 + z⁻¹).
    fn from_analog(b: [f64; 3], a1: f64, a0: f64, k: f64) -> Self {
        let [b0, b1, b2] = b;
        let k2 = k * k;
        let n = k2 + a1 * k + a0;
        Self {
            b: [
                (b2 * k2 + b1 * k + b0) / n,
                2.0 * (b0 - b2 * k2) / n,
                (b2 * k2 - b1 * k + b0) / n,
            ],
            a: [2.0 * (a0 - k2) / n, (k2 - a1 * k + a0) / n],
            z: [0.0; 2],
        }
    }

    #[inline]
    fn process(&mut self, x: f64) -> f64 {
        let y = self.b[0] * x + self.z[0];
        self.z[0] = self.b[1] * x - self.a[0] * y + self.z[1];
        self.z[1] = self.b[2] * x - self.a[1] * y;
        y
    }
}

/// 4th-order Bessel low- or high-pass as two cascaded biquads.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Bessel4 {
    sections: [Biquad; 2],
}

impl Bessel4 {
    pub(crate) fn new(
        cutoff_hz: f64,
        sample_rate: u32,
        high_pass: bool,
    ) -> Result<Self, WaveformError> {
        let fs = f64::from(sample_rate);
        if !(cutoff_hz > 0.0 && cutoff_hz < fs / 2.0) {
            return Err(WaveformError::CrossoverAboveNyquist {
                hz: cutoff_hz as u32,
                sample_rate,
            });
        }
        let k = 2.0 * fs;
        // Pre-warp so the digital -3 dB point lands on `cutoff_hz`.
        let wc = k * (std::f64::consts::PI * cutoff_hz / fs).tan();
        let section = |(re, im): (f64, f64)| {
            let mag2 = re * re + im * im;
            if high_pass {
                // s → wc/s applied to |p|²/(s² − 2Re(p)s + |p|²).
                Biquad::from_analog([0.0, 0.0, 1.0], -2.0 * re * wc / mag2, wc * wc / mag2, k)
            } else {
                let w0 = mag2 * wc * wc;
                Biquad::from_analog([w0, 0.0, 0.0], -2.0 * re * wc, w0, k)
            }
        };
        Ok(Self {
            sections: [section(BESSEL4_POLES[0]), section(BESSEL4_POLES[1])],
        })
    }

    #[inline]
    pub(crate) fn process(&mut self, x: f64) -> f64 {
        let y = self.sections[0].process(x);
        self.sections[1].process(y)
    }
}

/// Streaming column builder: feed interleaved PCM chunks of any size with
/// [`push_interleaved`](Self::push_interleaved), then call
/// [`finish`](Self::finish). Memory use is the output columns only.
#[derive(Debug, Clone)]
pub struct WaveformAnalyzer {
    sample_rate: u32,
    channels: usize,
    low_pass: Bessel4,
    mid_high_pass: Bessel4,
    mid_low_pass: Bessel4,
    high_pass: Bessel4,
    /// Frames consumed so far.
    frames: u64,
    /// First frame of the next column.
    next_boundary: u64,
    /// Running peaks of the open column: low, mid, high, mono.
    peak: [f32; 4],
    /// The open column has at least one frame.
    open: bool,
    /// Samples of a frame split across chunks.
    partial: Vec<f32>,
    out: WaveformColumns,
}

impl WaveformAnalyzer {
    pub fn new(sample_rate: u32, channels: u16) -> Result<Self, WaveformError> {
        if sample_rate == 0 {
            return Err(WaveformError::ZeroSampleRate);
        }
        if channels == 0 {
            return Err(WaveformError::ZeroChannels);
        }
        let mut this = Self {
            sample_rate,
            channels: usize::from(channels),
            low_pass: Bessel4::new(LOW_MID_CROSSOVER_HZ, sample_rate, false)?,
            mid_high_pass: Bessel4::new(LOW_MID_CROSSOVER_HZ, sample_rate, true)?,
            mid_low_pass: Bessel4::new(MID_HIGH_CROSSOVER_HZ, sample_rate, false)?,
            high_pass: Bessel4::new(MID_HIGH_CROSSOVER_HZ, sample_rate, true)?,
            frames: 0,
            next_boundary: 0,
            peak: [0.0; 4],
            open: false,
            partial: Vec::new(),
            out: WaveformColumns::default(),
        };
        this.next_boundary = this.boundary(1);
        Ok(this)
    }

    /// First frame of column `k`: ⌊k × sample_rate / 150⌋, exact for any rate.
    fn boundary(&self, k: u64) -> u64 {
        let v = u128::from(k) * u128::from(self.sample_rate) / u128::from(COLUMNS_PER_SECOND);
        u64::try_from(v).unwrap_or(u64::MAX)
    }

    /// Feed interleaved samples (`channels` per frame). A frame may be split
    /// across calls.
    pub fn push_interleaved(&mut self, samples: &[f32]) {
        let mut rest = samples;
        if !self.partial.is_empty() {
            let need = self.channels - self.partial.len();
            let take = need.min(rest.len());
            self.partial.extend_from_slice(&rest[..take]);
            rest = &rest[take..];
            if self.partial.len() < self.channels {
                return;
            }
            let frame = std::mem::take(&mut self.partial);
            self.push_frame(&frame);
        }
        let mut frames = rest.chunks_exact(self.channels);
        for frame in &mut frames {
            self.push_frame(frame);
        }
        self.partial.extend_from_slice(frames.remainder());
    }

    fn push_frame(&mut self, frame: &[f32]) {
        let sum: f32 = frame.iter().sum();
        let x = f64::from(sum / frame.len() as f32);
        let low = self.low_pass.process(x);
        let mid = self.mid_low_pass.process(self.mid_high_pass.process(x));
        let high = self.high_pass.process(x);
        for (p, v) in self.peak.iter_mut().zip([low, mid, high, x]) {
            *p = p.max(v.abs() as f32);
        }
        self.open = true;
        self.frames += 1;
        if self.frames >= self.next_boundary {
            self.close_column();
        }
    }

    fn close_column(&mut self) {
        let to_byte = |v: f32, max: u8| (v.clamp(0.0, 1.0) * f32::from(max)).round() as u8;
        let [low, mid, high, mono] = self.peak;
        self.out.low.push(to_byte(low, u8::MAX));
        self.out.mid.push(to_byte(mid, u8::MAX));
        self.out.high.push(to_byte(high, u8::MAX));
        self.out.mono.push(to_byte(mono, MONO_MAX));
        self.peak = [0.0; 4];
        self.open = false;
        self.next_boundary = self.boundary(self.out.mono.len() as u64 + 1);
    }

    /// Close the last (possibly partial) column and return the columns. An
    /// incomplete trailing frame is dropped.
    pub fn finish(mut self) -> WaveformColumns {
        if self.open {
            self.close_column();
        }
        self.out
    }
}

/// One-shot convenience over [`WaveformAnalyzer`] for interleaved PCM.
pub fn columns_from_interleaved(
    samples: &[f32],
    sample_rate: u32,
    channels: u16,
) -> Result<WaveformColumns, WaveformError> {
    let mut analyzer = WaveformAnalyzer::new(sample_rate, channels)?;
    analyzer.push_interleaved(samples);
    Ok(analyzer.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::TAU;

    const SR: u32 = 44_100;

    fn sine(freq: f64, seconds: f64, sr: u32) -> Vec<f32> {
        let n = (seconds * f64::from(sr)) as usize;
        (0..n)
            .map(|i| (0.8 * (TAU * freq * i as f64 / f64::from(sr)).sin()) as f32)
            .collect()
    }

    /// Mean band bytes over the columns after the first 0.2 s (filter settling).
    fn band_means(c: &WaveformColumns) -> [f64; 3] {
        let skip = 30;
        let mean = |v: &[u8]| {
            let v = &v[skip..];
            v.iter().map(|&x| f64::from(x)).sum::<f64>() / v.len() as f64
        };
        [mean(&c.low), mean(&c.mid), mean(&c.high)]
    }

    #[test]
    fn sines_land_in_their_band() {
        for (freq, band) in [
            (80.0, 0),
            (200.0, 0),
            (1500.0, 1),
            (2500.0, 1),
            (9000.0, 2),
            (14000.0, 2),
        ] {
            let c = columns_from_interleaved(&sine(freq, 1.0, SR), SR, 1).unwrap();
            let m = band_means(&c);
            for other in 0..3 {
                if other != band {
                    assert!(
                        m[band] > 2.0 * m[other],
                        "{freq} Hz: band {band} = {}, band {other} = {}",
                        m[band],
                        m[other]
                    );
                }
            }
            // Mono height of a 0.8 peak sine.
            assert_eq!(c.mono[50], (0.8f32 * 31.0).round() as u8);
        }
    }

    #[test]
    fn passband_peak_is_near_unity() {
        let c = columns_from_interleaved(&sine(100.0, 1.0, SR), SR, 1).unwrap();
        let m = band_means(&c)[0];
        assert!((m - 0.8 * 255.0).abs() < 8.0, "low mean {m}");
    }

    #[test]
    fn band_bytes_track_a_sweep() {
        // Exponential sweep 40 Hz → 16 kHz over 6 s.
        let (f0, f1, secs) = (40.0f64, 16_000.0f64, 6.0f64);
        let n = (secs * f64::from(SR)) as usize;
        let rate = (f1 / f0).ln() / secs;
        let pcm: Vec<f32> = (0..n)
            .map(|i| {
                let t = i as f64 / f64::from(SR);
                let phase = TAU * f0 * ((rate * t).exp() - 1.0) / rate;
                (0.8 * phase.sin()) as f32
            })
            .collect();
        let c = columns_from_interleaved(&pcm, SR, 1).unwrap();
        let freq_at = |col: usize| f0 * (rate * col as f64 / 150.0).exp();
        let mut dominant = Vec::new();
        for i in 0..c.len() {
            let bands = [c.low[i], c.mid[i], c.high[i]];
            let best = (0..3).max_by_key(|&b| bands[b]).unwrap();
            dominant.push(best);
        }
        // Away from the crossovers (±1/2 octave), the dominant band matches
        // the instantaneous frequency.
        for (i, &d) in dominant.iter().enumerate().skip(15) {
            let f = freq_at(i);
            let expected = if f < 600.0 / 1.42 {
                Some(0)
            } else if f > 600.0 * 1.42 && f < 4000.0 / 1.42 {
                Some(1)
            } else if f > 4000.0 * 1.42 {
                Some(2)
            } else {
                None
            };
            if let Some(e) = expected {
                assert_eq!(d, e, "column {i} at {f:.0} Hz");
            }
        }
        // And the band changes only in the low → mid → high direction.
        assert!(dominant.windows(2).all(|w| w[1] >= w[0]));
    }

    #[test]
    fn column_count_for_odd_durations() {
        for (sr, frames) in [
            (44_100u32, 44_100u64 * 3 + 1),
            (44_100, 293),
            (44_100, 294),
            (44_100, 295),
            (48_000, 1_000),
            (11_025, 12_345),
            (22_050, 1),
            (96_000, 7_777_777),
        ] {
            let expected = (frames * 150).div_ceil(u64::from(sr));
            assert_eq!(column_count(frames, sr), expected);
            let pcm = vec![0.1f32; frames as usize];
            let c = columns_from_interleaved(&pcm, sr, 1).unwrap();
            assert_eq!(c.len() as u64, expected, "{sr} Hz, {frames} frames");
            assert!(c.is_consistent());
        }
        assert_eq!(column_count(0, 44_100), 0);
        assert_eq!(column_count(10, 0), 0);
    }

    #[test]
    fn streaming_matches_one_shot_and_mixes_stereo() {
        let mono = sine(440.0, 1.3, SR);
        let stereo: Vec<f32> = mono.iter().flat_map(|&s| [s, s]).collect();
        let whole = columns_from_interleaved(&stereo, SR, 2).unwrap();
        // Odd chunk sizes split frames across pushes.
        let mut a = WaveformAnalyzer::new(SR, 2).unwrap();
        for chunk in stereo.chunks(333) {
            a.push_interleaved(chunk);
        }
        assert_eq!(a.finish(), whole);
        assert_eq!(whole, columns_from_interleaved(&mono, SR, 1).unwrap());
    }

    #[test]
    fn silence_empty_and_bad_input() {
        let c = columns_from_interleaved(&[0.0; 4410], SR, 1).unwrap();
        assert_eq!(c.len(), 15);
        assert!(c
            .low
            .iter()
            .chain(&c.mid)
            .chain(&c.high)
            .chain(&c.mono)
            .all(|&v| v == 0));
        assert!(columns_from_interleaved(&[], SR, 2).unwrap().is_empty());
        // Clipped input saturates rather than wrapping.
        let c = columns_from_interleaved(&[4.0, -4.0, 4.0], SR, 1).unwrap();
        assert_eq!(c.mono, vec![MONO_MAX]);
        assert_eq!(
            WaveformAnalyzer::new(0, 1).unwrap_err(),
            WaveformError::ZeroSampleRate
        );
        assert_eq!(
            WaveformAnalyzer::new(SR, 0).unwrap_err(),
            WaveformError::ZeroChannels
        );
        assert!(matches!(
            WaveformAnalyzer::new(8_000, 1),
            Err(WaveformError::CrossoverAboveNyquist { hz: 4000, .. })
        ));
    }
}
