//! Low-band onset curve (phase 3 task 12a): where the kick drum hits.
//!
//! The beat in dance music is almost always on the kick. The broadband
//! snap in [`crate::beats`] picks the sharpest rise of the whole signal,
//! which is often a hi-hat or clap a few ms away from the kick. This module
//! band-passes the signal to about 40–150 Hz (4th-order Butterworth
//! low-pass, 2nd-order high-pass), takes its energy per millisecond and
//! gives, for each ms, the rise in dB of the next [`AFTER_MS`] over the
//! previous [`BEFORE_MS`]: a kick attack is a rise of 10 dB or more, a
//! sustained bass line is not.

use crate::input::ANALYSIS_RATE;

/// Frames per second of [`KickOnsets`].
pub const KICK_RATE: u32 = 1000;
/// Band edges, Hz.
const LOW_HZ: f64 = 40.0;
const HIGH_HZ: f64 = 150.0;
/// Energy windows after and before a candidate onset (ms).
const AFTER_MS: usize = 10;
const BEFORE_MS: usize = 20;
/// Energy floor (−70 dBFS) so silence cannot divide by zero.
const FLOOR: f64 = 1e-7;
/// Smallest rise that counts as a kick, dB.
pub const MIN_KICK_DB: f32 = 6.0;

/// Low-band onset strength per ms: dB rise of the band's energy.
#[derive(Debug, Clone, PartialEq)]
pub struct KickOnsets {
    /// `rise_db[k]`: rise at `k` ms ([`KICK_RATE`]), 0 where it falls.
    pub rise_db: Vec<f32>,
}

/// One biquad section, transposed direct form II (RBJ cookbook).
#[derive(Debug, Clone, Copy)]
struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
    z: [f64; 2],
}

impl Biquad {
    fn new(hz: f64, q: f64, high_pass: bool) -> Self {
        let w0 = std::f64::consts::TAU * hz / f64::from(ANALYSIS_RATE);
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * q);
        let a0 = 1.0 + alpha;
        let b = if high_pass {
            [(1.0 + cos) / 2.0, -(1.0 + cos), (1.0 + cos) / 2.0]
        } else {
            [(1.0 - cos) / 2.0, 1.0 - cos, (1.0 - cos) / 2.0]
        };
        Self {
            b: b.map(|x| x / a0),
            a: [-2.0 * cos / a0, (1.0 - alpha) / a0],
            z: [0.0; 2],
        }
    }

    fn process(&mut self, x: f64) -> f64 {
        let y = self.b[0] * x + self.z[0];
        self.z[0] = self.b[1] * x - self.a[0] * y + self.z[1];
        self.z[1] = self.b[2] * x - self.a[1] * y;
        y
    }
}

impl KickOnsets {
    /// The onset curve of `mono` at [`ANALYSIS_RATE`].
    pub fn from_mono(mono: &[f32]) -> Self {
        let mut filters = [
            Biquad::new(LOW_HZ, std::f64::consts::FRAC_1_SQRT_2, true),
            Biquad::new(HIGH_HZ, 0.541_196_1, false),
            Biquad::new(HIGH_HZ, 1.306_563, false),
        ];
        // Energy per ms; ms k covers samples [k * 44.1, (k + 1) * 44.1).
        let per_ms = f64::from(ANALYSIS_RATE) / f64::from(KICK_RATE);
        let frames = (mono.len() as f64 / per_ms).ceil() as usize;
        let mut energy = vec![0.0f64; frames];
        for (i, &x) in mono.iter().enumerate() {
            let y = filters
                .iter_mut()
                .fold(f64::from(x), |acc, f| f.process(acc));
            let k = ((i as f64 / per_ms) as usize).min(frames - 1);
            energy[k] += y * y;
        }
        let mut prefix = Vec::with_capacity(frames + 1);
        prefix.push(0.0f64);
        let mut acc = 0.0;
        for e in &energy {
            acc += e / per_ms;
            prefix.push(acc);
        }
        // Before the first sample is silence; after the last too.
        let sum = |from: usize, to: usize| prefix[to.min(frames)] - prefix[from.min(frames)];
        let rise_db = (0..frames)
            .map(|k| {
                let after = sum(k, k + AFTER_MS) / AFTER_MS as f64;
                let before = sum(k.saturating_sub(BEFORE_MS), k) / BEFORE_MS as f64;
                let db = 10.0 * ((after + FLOOR) / (before + FLOOR)).log10();
                db.max(0.0) as f32
            })
            .collect();
        Self { rise_db }
    }

    /// The strongest kick onset within ±`radius_ms` of `sample` (samples
    /// at [`ANALYSIS_RATE`]): its position in samples and rise in dB, if
    /// the rise reaches `min_db`.
    pub fn kick_near(&self, sample: i64, radius_ms: u32, min_db: f32) -> Option<(i64, f32)> {
        let per_ms = f64::from(ANALYSIS_RATE) / f64::from(KICK_RATE);
        let centre = (sample as f64 / per_ms).round() as i64;
        let lo = (centre - i64::from(radius_ms)).max(0);
        let hi = (centre + i64::from(radius_ms)).min(self.rise_db.len() as i64 - 1);
        let (k, db) = (lo..=hi)
            .map(|k| (k, self.rise_db[k as usize]))
            // First of equal maxima: the start of the rise.
            .fold(None, |best: Option<(i64, f32)>, (k, db)| match best {
                Some((_, b)) if b >= db => best,
                _ => Some((k, db)),
            })?;
        (db >= min_db).then(|| ((k as f64 * per_ms).round() as i64, db))
    }

    /// The curve as the grid builder uses it ([`KickPulse`]).
    pub fn pulse(&self) -> KickPulse {
        KickPulse::from_quarter_db(KickPulse::quantise(&self.downsample(PULSE_RATE)))
    }

    /// The curve at `rate` Hz (a divisor of [`KICK_RATE`]), each frame the
    /// maximum over the ms it covers.
    pub fn downsample(&self, rate: u32) -> Vec<f32> {
        let step = (KICK_RATE / rate.max(1)).max(1) as usize;
        self.rise_db
            .chunks(step)
            .map(|c| c.iter().copied().fold(0.0, f32::max))
            .collect()
    }
}

/// Frames per second of [`KickPulse`].
pub const PULSE_RATE: u32 = 200;

/// [`KickOnsets`] at [`PULSE_RATE`] (each frame the maximum of its 5 ms) in
/// quarter-dB steps: compact enough to store per track, fine enough to
/// line a grid up with the kicks.
///
/// The rise stays high for 20–30 ms after a kick's attack; its leading
/// edge (the frame-to-frame increase, [`KickPulse::at`]) peaks on the
/// attack, where rekordbox puts the grid line (measured on the parity
/// library, docs/analysis-parity.md).
#[derive(Debug, Clone, PartialEq)]
pub struct KickPulse {
    /// Rise per frame, dB.
    pub db: Vec<f32>,
    /// Increase of `db` over the previous frame (0 where it falls).
    edge: Vec<f32>,
}

impl KickPulse {
    /// One byte per frame: the rise in quarter dB, saturating at 63.75 dB.
    pub fn quantise(db: &[f32]) -> Vec<u8> {
        db.iter()
            .map(|&d| (d * 4.0).round().clamp(0.0, 255.0) as u8)
            .collect()
    }

    pub fn from_quarter_db(bytes: Vec<u8>) -> Self {
        let db: Vec<f32> = bytes.into_iter().map(|b| f32::from(b) / 4.0).collect();
        let edge = db
            .iter()
            .scan(0.0f32, |prev, &d| {
                let e = (d - *prev).max(0.0);
                *prev = d;
                Some(e)
            })
            .collect();
        Self { db, edge }
    }

    /// The leading edge at `sample` ([`ANALYSIS_RATE`]); 0 outside the
    /// track.
    pub fn at(&self, sample: f64) -> f32 {
        let frame = sample * f64::from(PULSE_RATE) / f64::from(ANALYSIS_RATE);
        if frame < 0.0 {
            return 0.0;
        }
        self.edge.get(frame as usize).copied().unwrap_or(0.0)
    }

    /// Length of the track it covers, samples.
    pub fn len_samples(&self) -> f64 {
        self.db.len() as f64 * f64::from(ANALYSIS_RATE) / f64::from(PULSE_RATE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 60 Hz kick bursts (decaying) every `period` samples from `start`,
    /// with a 6 kHz hi-hat tick `hat_lead` samples before each kick.
    fn kicks_and_hats(len: usize, start: usize, period: usize, hat_lead: usize) -> Vec<f32> {
        let sr = f64::from(ANALYSIS_RATE);
        let mut out = vec![0.0f32; len];
        let mut t0 = start;
        while t0 < len {
            for i in 0..(sr * 0.15) as usize {
                if let Some(s) = out.get_mut(t0 + i) {
                    let t = i as f64 / sr;
                    *s +=
                        (0.8 * (-t * 25.0).exp() * (std::f64::consts::TAU * 60.0 * t).sin()) as f32;
                }
            }
            let hat = t0.saturating_sub(hat_lead);
            for i in 0..(sr * 0.01) as usize {
                if let Some(s) = out.get_mut(hat + i) {
                    let t = i as f64 / sr;
                    *s += (0.5 * (std::f64::consts::TAU * 6000.0 * t).sin()) as f32;
                }
            }
            t0 += period;
        }
        out
    }

    #[test]
    fn finds_the_kick_not_the_hat() {
        // Kicks every 0.5 s from 1 s; hats 15 ms before each kick.
        let mono = kicks_and_hats(44_100 * 4, 44_100, 22_050, 661);
        let onsets = KickOnsets::from_mono(&mono);
        for kick in [44_100i64, 66_150, 88_200] {
            // The model's guess 20 ms late, then 20 ms early.
            for guess in [kick + 882, kick - 882] {
                let (got, db) = onsets.kick_near(guess, 30, MIN_KICK_DB).unwrap();
                assert!((got - kick).abs() <= 5 * 44, "{guess} → {got}, kick {kick}");
                assert!(db > 10.0, "{db}");
            }
        }
        // Between kicks: nothing.
        assert_eq!(onsets.kick_near(44_100 + 11_025, 30, MIN_KICK_DB), None);
    }

    #[test]
    fn silence_and_edges_have_no_kick() {
        let onsets = KickOnsets::from_mono(&vec![0.0; 44_100]);
        assert!(onsets.rise_db.iter().all(|&d| d == 0.0));
        assert_eq!(onsets.kick_near(22_050, 30, MIN_KICK_DB), None);
        assert_eq!(onsets.kick_near(-5_000, 30, MIN_KICK_DB), None);
        assert_eq!(onsets.kick_near(1_000_000, 30, MIN_KICK_DB), None);
        assert!(KickOnsets::from_mono(&[]).rise_db.is_empty());
    }

    #[test]
    fn downsample_keeps_peaks() {
        let onsets = KickOnsets {
            rise_db: vec![0.0, 3.0, 0.0, 0.0, 0.0, 0.0, 0.0, 9.0, 0.0, 0.0, 1.0],
        };
        assert_eq!(onsets.downsample(200), [3.0, 9.0, 1.0]);
        let pulse = onsets.pulse();
        assert_eq!(pulse.db, [3.0, 9.0, 1.0]);
        assert_eq!(pulse.at(-1.0), 0.0);
        assert_eq!(pulse.at(1.0), 3.0);
        assert_eq!(pulse.at(5.0 * 44.1 + 1.0), 6.0);
        assert_eq!(pulse.at(10.0 * 44.1 + 1.0), 0.0);
        assert_eq!(pulse.at(1e9), 0.0);
        assert_eq!(KickPulse::quantise(&[0.1, 2.3, 99.0]), [0, 9, 255]);
    }
}
