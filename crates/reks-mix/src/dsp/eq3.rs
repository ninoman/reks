//! Three-band channel EQ for the 3-band mixer layout (DJM-900NXS2 / A9 EQ
//! mode, `docs/research/djm-eq-fx.md` §2): LOW shelf → MID bell → HI shelf,
//! −26 / +6 dB each, no kill.
//!
//! The shelves are Simper's SVF shelves (same paper as [`super::svf`]):
//! minimum-phase, flat down to the −26 dB floor with no dip, exactly `x` at
//! 0 dB, and safe to retune every sample. It runs in series with the 4-band
//! EQ; whichever layout is hidden sits at noon, where both blocks pass their
//! input bit for bit, so the engine needs no EQ mode.

use super::bell::Bell;
use super::smooth::Smoother;
use super::svf::{SvfCoeffs, SvfState};
use crate::taper;
use std::f64::consts::PI;

/// LOW shelf half-gain point. UNVERIFIED (the manuals give no corner).
pub const EQ3_LOW_HZ: f64 = 250.0;
/// MID centre (DJM manuals: 1 kHz).
pub const EQ3_MID_HZ: f64 = 1_000.0;
/// HI shelf half-gain point, as the 4-band HI. UNVERIFIED.
pub const EQ3_HI_HZ: f64 = 3_000.0;
/// MID width on boost and cut: a broad scoop between the shelves. UNVERIFIED.
pub const EQ3_MID_Q: f64 = 0.7;
/// Shelf Q: Butterworth, no overshoot.
const SHELF_Q: f64 = std::f64::consts::FRAC_1_SQRT_2;

/// One 3-band EQ knob.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Eq3Band {
    Hi,
    Mid,
    Lo,
}

impl Eq3Band {
    fn index(self) -> usize {
        match self {
            Self::Lo => 0,
            Self::Mid => 1,
            Self::Hi => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShelfKind {
    Low,
    High,
}

/// An SVF shelf: `y = m0·x + m1·band + m2·low`.
#[derive(Debug, Clone, Copy)]
struct SvfShelf {
    kind: ShelfKind,
    /// `tan(π·fc/fs)` of the half-gain point.
    g0: f64,
    coeffs: SvfCoeffs,
    m: [f64; 3],
    state: [SvfState; 2],
}

impl SvfShelf {
    fn new(kind: ShelfKind, fc: f64, sample_rate: f64) -> Self {
        let fc = fc.clamp(1.0, 0.45 * sample_rate);
        let mut s = Self {
            kind,
            g0: (PI * fc / sample_rate).tan(),
            coeffs: SvfCoeffs::with_g(1.0, 1.0),
            m: [1.0, 0.0, 0.0],
            state: [SvfState::default(); 2],
        };
        s.set_db(0.0);
        s
    }

    fn set_db(&mut self, db: f64) {
        let a = 10f64.powf(db / 40.0);
        let k = 1.0 / SHELF_Q;
        match self.kind {
            ShelfKind::Low => {
                self.coeffs = SvfCoeffs::with_g(self.g0 / a.sqrt(), k);
                self.m = [1.0, k * (a - 1.0), a * a - 1.0];
            }
            ShelfKind::High => {
                self.coeffs = SvfCoeffs::with_g(self.g0 * a.sqrt(), k);
                self.m = [a * a, k * (1.0 - a) * a, 1.0 - a * a];
            }
        }
    }

    #[inline]
    fn tick(&mut self, ch: usize, x: f64) -> f64 {
        let out = self.state[ch & 1].tick(&self.coeffs, x);
        self.m[0] * x + self.m[1] * out.band + self.m[2] * out.low
    }

    fn reset(&mut self) {
        self.state = [SvfState::default(); 2];
    }
}

/// A stereo 3-band EQ.
#[derive(Debug, Clone)]
pub struct ThreeBandEq {
    sample_rate: f64,
    /// LOW, MID, HI.
    knobs: [Smoother; 3],
    lo: SvfShelf,
    mid: Bell,
    hi: SvfShelf,
    /// Skipped and cleared while every knob is settled at noon.
    idle: bool,
}

impl ThreeBandEq {
    /// All knobs at noon (flat, bypassed).
    pub fn new(sample_rate: u32) -> Self {
        let fs = f64::from(sample_rate);
        Self {
            sample_rate: fs,
            knobs: [Smoother::new(0.5, sample_rate); 3],
            lo: SvfShelf::new(ShelfKind::Low, EQ3_LOW_HZ, fs),
            mid: Bell::new(EQ3_MID_HZ, EQ3_MID_Q, EQ3_MID_Q, fs),
            hi: SvfShelf::new(ShelfKind::High, EQ3_HI_HZ, fs),
            idle: true,
        }
    }

    /// Moves a knob (0..=1, noon flat) with a ramp.
    pub fn set(&mut self, band: Eq3Band, value: f32) {
        self.knobs[band.index()].set(taper::unit(value, 0.5));
    }

    /// Sets a knob with no ramp.
    pub fn snap(&mut self, band: Eq3Band, value: f32) {
        let i = band.index();
        self.knobs[i].snap(taper::unit(value, 0.5));
        self.retune(i, self.knobs[i].value());
    }

    fn retune(&mut self, i: usize, v: f32) {
        let db = f64::from(taper::eq3_db(v));
        match i {
            0 => self.lo.set_db(db),
            1 => self.mid.set_db(db, self.sample_rate),
            _ => self.hi.set_db(db),
        }
    }

    fn at_rest(&self) -> bool {
        self.knobs
            .iter()
            .all(|k| k.is_settled() && k.value() == 0.5)
    }

    /// Filters interleaved stereo in place. No allocation.
    pub fn process(&mut self, buf: &mut [f32]) {
        if self.at_rest() {
            if !self.idle {
                self.lo.reset();
                self.mid.reset();
                self.hi.reset();
                for i in 0..3 {
                    self.retune(i, 0.5);
                }
                self.idle = true;
            }
            return;
        }
        if self.idle {
            for i in 0..3 {
                self.retune(i, self.knobs[i].value());
            }
            self.idle = false;
        }
        for frame in buf.as_chunks_mut::<2>().0 {
            for i in 0..3 {
                let ramping = !self.knobs[i].is_settled();
                let v = self.knobs[i].tick();
                if ramping {
                    self.retune(i, v);
                }
            }
            for (ch, s) in frame.iter_mut().enumerate() {
                let mut x = f64::from(*s);
                x = self.lo.tick(ch, x);
                x = self.mid.tick(ch, x);
                x = self.hi.tick(ch, x);
                *s = x as f32;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::testutil::{gain_db, log_sweep, RATES};
    use super::*;

    fn measure(rate: u32, band: Eq3Band, value: f32, hz: f64) -> f64 {
        let mut e = ThreeBandEq::new(rate);
        e.snap(band, value);
        gain_db(rate, hz, |buf| e.process(buf))
    }

    fn noise(len: usize) -> Vec<f32> {
        let mut seed = 0x1234_5678u32;
        (0..len)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                (seed as f32 / u32::MAX as f32) * 2.0 - 1.0
            })
            .collect()
    }

    #[test]
    fn noon_is_bit_for_bit() {
        for rate in RATES {
            let mut e = ThreeBandEq::new(rate);
            let input = noise(rate as usize * 2);
            let mut buf = input.clone();
            for c in buf.chunks_mut(512) {
                e.process(c);
            }
            assert_eq!(buf, input);
        }
    }

    #[test]
    fn back_at_noon_after_a_move_is_bit_for_bit_again() {
        let rate = 48_000;
        let mut e = ThreeBandEq::new(rate);
        e.set(Eq3Band::Lo, 0.0);
        let mut buf = noise(9_600);
        e.process(&mut buf);
        e.set(Eq3Band::Lo, 0.5);
        let mut buf = noise(9_600);
        e.process(&mut buf);
        let input = noise(4_800);
        let mut buf = input.clone();
        e.process(&mut buf);
        assert_eq!(buf, input);
    }

    #[test]
    fn low_full_cut_reaches_minus_26_with_no_dip() {
        for rate in RATES {
            let db = measure(rate, Eq3Band::Lo, 0.0, 20.0);
            assert!((db + 26.0).abs() <= 1.0, "{rate}: 20 Hz {db:.2}");
            assert!(measure(rate, Eq3Band::Lo, 0.0, 100.0) <= -23.0);
            for f in log_sweep(20.0, 20_000.0, rate, 6) {
                let db = measure(rate, Eq3Band::Lo, 0.0, f);
                assert!(db >= -27.0, "{rate}, {f:.0} Hz: {db:.2}");
            }
        }
    }

    #[test]
    fn hi_full_cut_reaches_minus_26() {
        for rate in RATES {
            let db = measure(rate, Eq3Band::Hi, 0.0, 16_000.0);
            assert!((db + 26.0).abs() <= 1.0, "{rate}: 16 kHz {db:.2}");
            assert!(measure(rate, Eq3Band::Hi, 0.0, 8_000.0) <= -23.0);
        }
    }

    #[test]
    fn mid_full_cut_is_minus_26_and_broad() {
        for rate in RATES {
            let db = measure(rate, Eq3Band::Mid, 0.0, EQ3_MID_HZ);
            assert!((db + 26.0).abs() <= 0.5, "{rate}: {db:.2}");
            for f in [250.0, 4_000.0] {
                let db = measure(rate, Eq3Band::Mid, 0.0, f);
                assert!(db > -7.0, "{rate}, {f} Hz: {db:.2}");
            }
        }
    }

    #[test]
    fn full_boost_is_plus_6_with_no_overshoot() {
        let cases = [
            (Eq3Band::Lo, 20.0),
            (Eq3Band::Mid, EQ3_MID_HZ),
            (Eq3Band::Hi, 16_000.0),
        ];
        for rate in RATES {
            for (band, hz) in cases {
                let db = measure(rate, band, 1.0, hz);
                assert!((db - 6.0).abs() <= 0.5, "{band:?} at {rate}: {db:.2}");
                for f in log_sweep(20.0, 20_000.0, rate, 6) {
                    let peak = measure(rate, band, 1.0, f);
                    assert!(peak <= 6.5, "{band:?} at {rate}, {f:.0} Hz: {peak:.2}");
                }
            }
        }
    }

    #[test]
    fn shelves_are_at_half_gain_at_their_corners() {
        for rate in RATES {
            let lo = measure(rate, Eq3Band::Lo, 0.0, EQ3_LOW_HZ);
            let hi = measure(rate, Eq3Band::Hi, 0.0, EQ3_HI_HZ);
            assert!((lo + 13.0).abs() <= 1.0, "{rate}: LOW {lo:.2}");
            assert!((hi + 13.0).abs() <= 1.0, "{rate}: HI {hi:.2}");
        }
    }

    #[test]
    fn a_10_ms_knob_sweep_does_not_step() {
        let rate = 48_000;
        for hz in [50.0f32, 5_000.0] {
            for band in [Eq3Band::Lo, Eq3Band::Mid, Eq3Band::Hi] {
                let mut e = ThreeBandEq::new(rate);
                let tone: Vec<f32> = (0..rate as usize / 5)
                    .flat_map(|i| {
                        let s =
                            0.5 * (2.0 * std::f32::consts::PI * hz * i as f32 / rate as f32).sin();
                        [s, s]
                    })
                    .collect();
                let mut buf = tone.clone();
                let (a, b) = buf.split_at_mut(rate as usize / 10);
                e.process(a);
                e.set(band, 0.0);
                e.process(b);
                let max_step = 0.5 * 2.0 * (std::f32::consts::PI * hz / rate as f32).sin();
                for w in buf.as_chunks::<2>().0.windows(2) {
                    assert!(
                        (w[1][0] - w[0][0]).abs() <= max_step + 0.01,
                        "{band:?} {hz} Hz"
                    );
                }
            }
        }
    }

    #[test]
    fn silence_after_a_burst_is_exact_zeros() {
        for rate in RATES {
            let mut e = ThreeBandEq::new(rate);
            for band in [Eq3Band::Lo, Eq3Band::Mid, Eq3Band::Hi] {
                e.snap(band, 1.0);
            }
            let mut burst: Vec<f32> = (0..2_048)
                .map(|i| if i % 7 < 3 { 1.0 } else { -1.0 })
                .collect();
            e.process(&mut burst);
            e.set(Eq3Band::Mid, 0.0);
            let mut tail = vec![0.0f32; rate as usize * 2 * 10];
            for chunk in tail.chunks_mut(512) {
                e.process(chunk);
            }
            assert!(tail.iter().all(|s| s.is_finite() && !s.is_subnormal()));
            assert!(tail[tail.len() - 1_024..].iter().all(|&s| s == 0.0));
        }
    }
}
