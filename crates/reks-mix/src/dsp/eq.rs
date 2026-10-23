//! Four-band asymmetric channel EQ (Xone:96 channels 1–4, R§9.1).
//!
//! - HI: shelf above 3 kHz. `y = LP + g·(x − LP)` with a fixed 2-pole
//!   low-pass, so g = 1 is exactly flat and g = 0 is the low-pass alone:
//!   a 12 dB/oct kill. Boost uses `y = x + (g − 1)·(x − LP')` with a damped
//!   low-pass (Q 0.4) so the shelf does not overshoot +6 dB.
//! - LO: the mirror image with high-passes at 180 Hz.
//! - HI MID (1.1 kHz) and LO MID (350 Hz): SVF bells, +10 / −27 dB.
//!   Unlike HI and LO they never kill a band: a full cut is a deep notch.
//!
//! All four knobs are smoothed ([`super::smooth`]); both shelf filters run
//! all the time so moving through noon never restarts a filter.

use super::bell::Bell;
use super::biquad::Biquad;
use super::smooth::Smoother;
use crate::taper;

/// HI shelf turning point (Xone:96, R§9.1).
pub const HI_HZ: f64 = 3_000.0;
/// HI MID centre (Xone:96, R§9.1).
pub const MID1_HZ: f64 = 1_100.0;
/// LO MID centre (Xone:96, R§9.1).
pub const MID2_HZ: f64 = 350.0;
/// LO shelf turning point (Xone:96, R§9.1).
pub const LO_HZ: f64 = 180.0;

/// Q of the kill filters: Butterworth, −3 dB at the turning point. The
/// 12 dB/oct slope is the Xone:92's; the Xone:96 guide gives none (UNVERIFIED).
const KILL_Q: f64 = std::f64::consts::FRAC_1_SQRT_2;
/// Q of the boost shelves' filters: overdamped so full boost peaks at most
/// about +6.4 dB instead of +6.8 dB.
const BOOST_Q: f64 = 0.4;
/// Mid bell Q on boost and cut. In this bell form Q sets the width at half
/// the gain in dB, so a deep cut at a low Q takes out most of the spectrum
/// and the mids sound like isolator kills. Q 2 on cut keeps a full cut
/// above −3 dB two octaves away. The guides give no figures: UNVERIFIED.
pub const MID_Q_BOOST: f64 = 1.2;
pub const MID_Q_CUT: f64 = 2.0;

/// One EQ knob.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EqBand {
    Hi,
    Mid1,
    Mid2,
    Lo,
}

impl EqBand {
    fn index(self) -> usize {
        match self {
            Self::Hi => 0,
            Self::Mid1 => 1,
            Self::Mid2 => 2,
            Self::Lo => 3,
        }
    }
}

/// A shelf with a kill filter and a boost filter fed the same input.
#[derive(Debug, Clone, Copy)]
struct Shelf {
    kill: Biquad,
    boost: Biquad,
}

impl Shelf {
    /// `g` is the shelf amplitude from [`taper::shelf_gain`]. The kill and
    /// boost filters are both low-passes (HI) or both high-passes (LO); the
    /// band being shaped is `x − filter` in each case.
    #[inline]
    fn tick(&mut self, ch: usize, x: f64, g: f64) -> f64 {
        let kill = self.kill.tick(ch, x);
        let boost = self.boost.tick(ch, x);
        // Exact at noon, so a flat channel passes its deck bit for bit.
        if g == 1.0 {
            x
        } else if g < 1.0 {
            kill + g * (x - kill)
        } else {
            x + (g - 1.0) * (x - boost)
        }
    }
}

/// A stereo channel EQ.
#[derive(Debug, Clone)]
pub struct ChannelEq {
    sample_rate: f64,
    knobs: [Smoother; 4],
    hi: Shelf,
    mid1: Bell,
    mid2: Bell,
    lo: Shelf,
}

impl ChannelEq {
    /// All knobs at noon (flat).
    pub fn new(sample_rate: u32) -> Self {
        let fs = f64::from(sample_rate);
        Self {
            sample_rate: fs,
            knobs: [Smoother::new(0.5, sample_rate); 4],
            hi: Shelf {
                kill: Biquad::lowpass(HI_HZ, KILL_Q, fs),
                boost: Biquad::lowpass(HI_HZ, BOOST_Q, fs),
            },
            mid1: Bell::new(MID1_HZ, MID_Q_BOOST, MID_Q_CUT, fs),
            mid2: Bell::new(MID2_HZ, MID_Q_BOOST, MID_Q_CUT, fs),
            lo: Shelf {
                kill: Biquad::highpass(LO_HZ, KILL_Q, fs),
                boost: Biquad::highpass(LO_HZ, BOOST_Q, fs),
            },
        }
    }

    /// Moves a knob (0..=1, noon flat) with a ramp.
    pub fn set(&mut self, band: EqBand, value: f32) {
        self.knobs[band.index()].set(taper::unit(value, 0.5));
    }

    /// Sets a knob with no ramp.
    pub fn snap(&mut self, band: EqBand, value: f32) {
        let v = taper::unit(value, 0.5);
        self.knobs[band.index()].snap(v);
        match band {
            EqBand::Mid1 => self
                .mid1
                .set_db(f64::from(taper::mid_db(v)), self.sample_rate),
            EqBand::Mid2 => self
                .mid2
                .set_db(f64::from(taper::mid_db(v)), self.sample_rate),
            EqBand::Hi | EqBand::Lo => {}
        }
    }

    /// Filters interleaved stereo in place. No allocation.
    pub fn process(&mut self, buf: &mut [f32]) {
        let fs = self.sample_rate;
        for frame in buf.as_chunks_mut::<2>().0 {
            let ramping_mid1 = !self.knobs[1].is_settled();
            let ramping_mid2 = !self.knobs[2].is_settled();
            let hi = f64::from(taper::shelf_gain(self.knobs[0].tick()));
            let m1 = self.knobs[1].tick();
            let m2 = self.knobs[2].tick();
            let lo = f64::from(taper::shelf_gain(self.knobs[3].tick()));
            if ramping_mid1 {
                self.mid1.set_db(f64::from(taper::mid_db(m1)), fs);
            }
            if ramping_mid2 {
                self.mid2.set_db(f64::from(taper::mid_db(m2)), fs);
            }
            for (ch, s) in frame.iter_mut().enumerate() {
                let mut x = f64::from(*s);
                x = self.hi.tick(ch, x, hi);
                x = self.mid1.tick(ch, x);
                x = self.mid2.tick(ch, x);
                x = self.lo.tick(ch, x, lo);
                *s = x as f32;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::testutil::{gain_db, log_sweep, RATES};
    use super::*;

    fn eq(rate: u32, band: EqBand, value: f32) -> ChannelEq {
        let mut eq = ChannelEq::new(rate);
        eq.snap(band, value);
        eq
    }

    fn measure(rate: u32, band: EqBand, value: f32, hz: f64) -> f64 {
        let mut e = eq(rate, band, value);
        gain_db(rate, hz, |buf| e.process(buf))
    }

    #[test]
    fn noon_is_flat_within_a_tenth_of_a_db() {
        for rate in RATES {
            for hz in log_sweep(20.0, 20_000.0, rate, 3) {
                let mut e = ChannelEq::new(rate);
                let db = gain_db(rate, hz, |buf| e.process(buf));
                assert!(db.abs() <= 0.1, "{rate} Hz rate, {hz:.0} Hz: {db:.3} dB");
            }
        }
    }

    #[test]
    fn full_boost_is_plus_6_on_shelves_and_plus_10_on_mids_within_half_a_db() {
        let cases = [
            (EqBand::Hi, 12_000.0, 6.0),
            (EqBand::Mid1, MID1_HZ, 10.0),
            (EqBand::Mid2, MID2_HZ, 10.0),
            (EqBand::Lo, 30.0, 6.0),
        ];
        for rate in RATES {
            for (band, hz, want) in cases {
                let db = measure(rate, band, 1.0, hz);
                assert!((db - want).abs() <= 0.5, "{band:?} at {rate}: {db:.3} dB");
                // No overshoot anywhere in the band either.
                for f in log_sweep(20.0, 20_000.0, rate, 6) {
                    let peak = measure(rate, band, 1.0, f);
                    assert!(
                        peak <= want + 0.5,
                        "{band:?} at {rate}, {f:.0} Hz: {peak:.3} dB"
                    );
                }
            }
        }
    }

    #[test]
    fn mids_full_cut_is_minus_27_db_within_1_db() {
        for rate in RATES {
            for (band, hz) in [(EqBand::Mid1, MID1_HZ), (EqBand::Mid2, MID2_HZ)] {
                let db = measure(rate, band, 0.0, hz);
                assert!((db + 27.0).abs() <= 1.0, "{band:?} at {rate}: {db:.3} dB");
            }
        }
    }

    #[test]
    fn mids_full_cut_is_a_notch_not_a_kill() {
        for rate in RATES {
            for (band, hz) in [(EqBand::Mid1, MID1_HZ), (EqBand::Mid2, MID2_HZ)] {
                for f in [hz / 4.0, hz * 4.0] {
                    let db = measure(rate, band, 0.0, f);
                    assert!(db > -3.0, "{band:?} at {rate}, {f:.0} Hz: {db:.3} dB");
                }
            }
        }
    }

    #[test]
    fn hi_and_lo_kills_fall_at_12_db_per_octave_past_the_turning_point() {
        for rate in RATES {
            // HI: one octave from 2× to 4× the turning point (6 → 12 kHz).
            let near = measure(rate, EqBand::Hi, 0.0, 2.0 * HI_HZ);
            let far = measure(rate, EqBand::Hi, 0.0, 4.0 * HI_HZ);
            let slope = near - far;
            assert!(
                (slope - 12.0).abs() <= 1.0,
                "HI at {rate}: {slope:.2} dB/oct"
            );
            // LO: two octaves below, 90 → 45 Hz and 45 → 22.5 Hz.
            for (near_hz, far_hz) in [(LO_HZ / 2.0, LO_HZ / 4.0), (LO_HZ / 4.0, LO_HZ / 8.0)] {
                let slope = measure(rate, EqBand::Lo, 0.0, near_hz)
                    - measure(rate, EqBand::Lo, 0.0, far_hz);
                assert!(
                    (slope - 12.0).abs() <= 1.0,
                    "LO at {rate}, {near_hz} Hz: {slope:.2} dB/oct"
                );
            }
            // The kill leaves the other side of the turning point alone.
            assert!(measure(rate, EqBand::Hi, 0.0, 200.0).abs() < 0.1);
            assert!(measure(rate, EqBand::Lo, 0.0, 5_000.0).abs() < 0.1);
        }
    }

    #[test]
    fn silence_in_gives_silence_out_with_no_nan_or_denormals() {
        for rate in RATES {
            let mut e = ChannelEq::new(rate);
            for band in [EqBand::Hi, EqBand::Mid1, EqBand::Mid2, EqBand::Lo] {
                e.snap(band, 1.0);
            }
            // Pure silence stays exactly zero.
            let mut buf = vec![0.0f32; 4_096];
            e.process(&mut buf);
            assert!(buf.iter().all(|&s| s == 0.0));
            // A loud burst, then ten seconds of silence while the knobs move.
            let mut burst: Vec<f32> = (0..2_048)
                .map(|i| if i % 7 < 3 { 1.0 } else { -1.0 })
                .collect();
            e.process(&mut burst);
            e.set(EqBand::Hi, 0.0);
            e.set(EqBand::Mid2, 0.0);
            let mut tail = vec![0.0f32; rate as usize * 2 * 10];
            for chunk in tail.chunks_mut(512) {
                e.process(chunk);
            }
            assert!(tail.iter().all(|s| s.is_finite() && !s.is_subnormal()));
            let last = &tail[tail.len() - 1_024..];
            assert!(last.iter().all(|&s| s == 0.0), "{:?}", &last[..8]);
        }
    }

    #[test]
    fn knob_moves_are_smoothed() {
        let rate = 48_000;
        let mut e = ChannelEq::new(rate);
        // DC through the HI shelf is untouched; use a 10 kHz tone and kill HI.
        let mut buf: Vec<f32> = (0..rate as usize / 10)
            .flat_map(|i| {
                let s = (2.0 * std::f32::consts::PI * 10_000.0 * i as f32 / rate as f32).sin();
                [s, s]
            })
            .collect();
        e.set(EqBand::Hi, 0.0);
        e.process(&mut buf);
        // No sample-to-sample step bigger than the tone itself can make.
        let max_step = 2.0 * (std::f32::consts::PI * 10_000.0 / rate as f32).sin() + 1e-3;
        for w in buf.as_chunks::<2>().0.windows(2) {
            assert!((w[1][0] - w[0][0]).abs() <= max_step);
        }
    }
}
