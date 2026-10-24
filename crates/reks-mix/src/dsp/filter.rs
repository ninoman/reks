//! The two sweepable filter units, X and Y (Xone:92 VCFs, R§9.1, no LFOs per
//! D21).
//!
//! One trapezoidal SVF per unit (2-pole; the hardware slope is UNVERIFIED).
//! The output mixes the three SVF outputs by the mode switches:
//! `y = lp·low + bp·k·band + hp·high + none·x`, where `none` is 1 when no mode
//! is selected (the unit passes the signal rather than muting it). HP+LP is a
//! notch at the cutoff; all three give back the input (`low + k·band + high
//! == x`), the guide's "all-pass effect". Every switch weight and the ON
//! wet/dry mix are smoothed, so switching never clicks.

use super::smooth::Smoother;
use super::svf::{SvfCoeffs, SvfState};
use crate::taper;

/// Mode switches; any combination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FilterModes {
    pub lp: bool,
    pub bp: bool,
    pub hp: bool,
}

/// One stereo filter unit.
#[derive(Debug, Clone)]
pub struct Filter {
    sample_rate: f64,
    freq: Smoother,
    resonance: Smoother,
    lp: Smoother,
    bp: Smoother,
    hp: Smoother,
    none: Smoother,
    wet: Smoother,
    coeffs: SvfCoeffs,
    state: [SvfState; 2],
}

impl Filter {
    /// Off, FREQ at its minimum (HP fully open), resonance at MILD, HP selected.
    pub fn new(sample_rate: u32) -> Self {
        let fs = f64::from(sample_rate);
        let s = |v: f32| Smoother::new(v, sample_rate);
        let mut f = Self {
            sample_rate: fs,
            freq: s(0.0),
            resonance: s(0.0),
            lp: s(0.0),
            bp: s(0.0),
            hp: s(1.0),
            none: s(0.0),
            wet: s(0.0),
            coeffs: SvfCoeffs::new(1_000.0, 1.0, fs),
            state: [SvfState::default(); 2],
        };
        f.update_coeffs();
        f
    }

    /// FREQ knob, 0..=1 (30 Hz–20 kHz).
    pub fn set_freq(&mut self, value: f32) {
        self.freq.set(taper::unit(value, 1.0));
    }

    /// RESONANCE knob, 0..=1 (MILD to WILD).
    pub fn set_resonance(&mut self, value: f32) {
        self.resonance.set(taper::unit(value, 0.0));
    }

    /// Mode switches; the change crossfades over the smoothing time.
    pub fn set_modes(&mut self, modes: FilterModes) {
        let w = |on: bool| if on { 1.0 } else { 0.0 };
        self.lp.set(w(modes.lp));
        self.bp.set(w(modes.bp));
        self.hp.set(w(modes.hp));
        self.none.set(w(!(modes.lp || modes.bp || modes.hp)));
    }

    /// ON switch: crossfades between the dry input and the filter.
    pub fn set_on(&mut self, on: bool) {
        self.wet.set(if on { 1.0 } else { 0.0 });
    }

    /// Jumps every parameter to its target (initial state, tests).
    pub fn settle(&mut self) {
        for s in self.smoothers() {
            s.snap(s.target());
        }
        self.update_coeffs();
    }

    /// Settles every parameter and clears the filter state, for a unit copy
    /// that starts again after sitting idle.
    pub fn reset(&mut self) {
        self.settle();
        self.state = [SvfState::default(); 2];
    }

    fn smoothers(&mut self) -> [&mut Smoother; 7] {
        [
            &mut self.freq,
            &mut self.resonance,
            &mut self.lp,
            &mut self.bp,
            &mut self.hp,
            &mut self.none,
            &mut self.wet,
        ]
    }

    fn update_coeffs(&mut self) {
        let fc = f64::from(taper::filter_freq_hz(self.freq.value()));
        let q = f64::from(taper::resonance_q(self.resonance.value()));
        self.coeffs = SvfCoeffs::new(fc, 1.0 / q, self.sample_rate);
    }

    /// Filters interleaved stereo in place. The SVF runs even when the unit
    /// is off, so switching it on starts from warm state. No allocation.
    pub fn process(&mut self, buf: &mut [f32]) {
        for frame in buf.as_chunks_mut::<2>().0 {
            let sweeping = !(self.freq.is_settled() && self.resonance.is_settled());
            self.freq.tick();
            self.resonance.tick();
            if sweeping {
                self.update_coeffs();
            }
            let lp = f64::from(self.lp.tick());
            let bp = f64::from(self.bp.tick()) * self.coeffs.k;
            let hp = f64::from(self.hp.tick());
            let none = f64::from(self.none.tick());
            let wet = f64::from(self.wet.tick());
            for (ch, s) in frame.iter_mut().enumerate() {
                let x = f64::from(*s);
                let o = self.state[ch].tick(&self.coeffs, x);
                let y = lp * o.low + bp * o.band + hp * o.high + none * x;
                *s = (x + wet * (y - x)) as f32;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::testutil::{gain_db, gain_db_settled, log_sweep, RATES};
    use super::*;

    /// FREQ knob value for a cutoff in Hz (inverse of the taper).
    fn knob_for(hz: f64) -> f32 {
        let min = f64::from(taper::FILTER_MIN_HZ);
        let max = f64::from(taper::FILTER_MAX_HZ);
        ((hz / min).ln() / (max / min).ln()) as f32
    }

    fn unit(rate: u32, modes: FilterModes, hz: f64, resonance: f32) -> Filter {
        let mut f = Filter::new(rate);
        f.set_on(true);
        f.set_modes(modes);
        f.set_freq(knob_for(hz));
        f.set_resonance(resonance);
        f.settle();
        f
    }

    const LP: FilterModes = FilterModes {
        lp: true,
        bp: false,
        hp: false,
    };
    const HP: FilterModes = FilterModes {
        lp: false,
        bp: false,
        hp: true,
    };

    #[test]
    fn lp_and_hp_are_minus_3_db_at_the_set_cutoff_at_minimum_resonance() {
        for rate in RATES {
            for hz in [30.0, 100.0, 1_000.0, 5_000.0, 15_000.0] {
                if hz > 0.45 * f64::from(rate) {
                    continue;
                }
                for (name, modes) in [("LP", LP), ("HP", HP)] {
                    let mut f = unit(rate, modes, hz, 0.0);
                    let db = gain_db(rate, hz, |b| f.process(b));
                    assert!(
                        (db + 3.01).abs() <= 0.1,
                        "{name} {hz} Hz at {rate}: {db:.3} dB"
                    );
                }
            }
        }
    }

    /// The knob direction end to end: LP cuts highs as FREQ turns left
    /// (anticlockwise), HP cuts lows as FREQ turns right, and the other end
    /// of each leaves the signal alone.
    #[test]
    fn lp_cuts_highs_to_the_left_and_hp_cuts_lows_to_the_right() {
        let (low, high) = (60.0, 8_000.0);
        let gain = |modes, knob: f32, hz| {
            let rate = 48_000;
            let mut f = Filter::new(rate);
            f.set_on(true);
            f.set_modes(modes);
            f.set_freq(knob);
            f.settle();
            gain_db(rate, hz, |b| f.process(b))
        };
        // LP fully left (30 Hz): the high sine is gone, the low one mostly kept.
        assert!(gain(LP, 0.0, high) < -60.0);
        assert!(gain(LP, 0.0, low) > -15.0);
        // LP fully right (20 kHz): both pass.
        assert!(gain(LP, 1.0, high).abs() < 0.5);
        assert!(gain(LP, 1.0, low).abs() < 0.1);
        // HP fully right (20 kHz): the low sine is gone, the high one cut too.
        assert!(gain(HP, 1.0, low) < -60.0);
        assert!(gain(HP, 1.0, high) < -20.0);
        // HP fully left (30 Hz): both pass.
        assert!(gain(HP, 0.0, low).abs() < 0.5);
        assert!(gain(HP, 0.0, high).abs() < 0.1);
        // Noon: each removes its own side and keeps the other.
        assert!(gain(LP, 0.5, high) < -30.0 && gain(LP, 0.5, low).abs() < 0.1);
        assert!(gain(HP, 0.5, low) < -30.0 && gain(HP, 0.5, high).abs() < 0.1);
    }

    #[test]
    fn modes_combine_as_documented() {
        let rate = 48_000;
        let fc = 1_000.0;
        let all = FilterModes {
            lp: true,
            bp: true,
            hp: true,
        };
        let notch = FilterModes {
            lp: true,
            bp: false,
            hp: true,
        };
        let band = FilterModes {
            lp: false,
            bp: true,
            hp: false,
        };
        for hz in [100.0, 1_000.0, 10_000.0] {
            let mut f = unit(rate, all, fc, 1.0);
            let db = gain_db_settled(rate, hz, 0.5, |b| f.process(b));
            assert!(db.abs() < 0.01, "all three at {hz} Hz: {db:.3} dB");
            let mut f = unit(rate, FilterModes::default(), fc, 1.0);
            let db = gain_db(rate, hz, |b| f.process(b));
            assert!(db.abs() < 0.01, "no mode at {hz} Hz: {db:.3} dB");
        }
        let mut f = unit(rate, notch, fc, 0.0);
        assert!(gain_db(rate, fc, |b| f.process(b)) < -40.0);
        let mut f = unit(rate, band, fc, 0.0);
        assert!(gain_db(rate, fc, |b| f.process(b)).abs() < 0.05);
    }

    #[test]
    fn off_passes_the_input_untouched() {
        let rate = 44_100;
        let mut f = Filter::new(rate);
        f.set_modes(HP);
        f.set_freq(0.9);
        f.settle();
        let mut buf: Vec<f32> = (0..2_000)
            .map(|i| ((i * 37) % 101) as f32 / 101.0 - 0.5)
            .collect();
        let dry = buf.clone();
        f.process(&mut buf);
        assert_eq!(buf, dry);
    }

    #[test]
    fn max_resonance_peaks_at_the_cutoff() {
        let rate = 48_000;
        let mut f = unit(rate, LP, 1_000.0, 1.0);
        let db = gain_db_settled(rate, 1_000.0, 0.5, |b| f.process(b));
        let want = 20.0 * f64::from(taper::RESONANCE_MAX_Q).log10();
        assert!((db - want).abs() < 0.2, "{db:.2} dB, want {want:.2}");
    }

    /// Deterministic white-ish noise in −1..1.
    fn noise(len: usize, seed: &mut u32) -> Vec<f32> {
        (0..len)
            .map(|_| {
                *seed ^= *seed << 13;
                *seed ^= *seed >> 17;
                *seed ^= *seed << 5;
                (*seed as f32 / u32::MAX as f32) * 2.0 - 1.0
            })
            .collect()
    }

    #[test]
    fn stable_at_maximum_resonance_from_30_hz_to_20_khz() {
        let mut seed = 0x9e37_79b9;
        for rate in RATES {
            let modes = [
                LP,
                HP,
                FilterModes {
                    lp: false,
                    bp: true,
                    hp: false,
                },
            ];
            // Fixed cutoffs, 1/3 octave apart, including 20 kHz itself.
            let mut cutoffs = log_sweep(30.0, 20_000.0, 96_000, 3);
            cutoffs.push(20_000.0);
            for hz in cutoffs {
                for m in modes {
                    let mut f = unit(rate, m, hz, 1.0);
                    let mut buf = noise(rate as usize / 2, &mut seed);
                    f.process(&mut buf);
                    let peak = buf.iter().fold(0.0f32, |a, s| a.max(s.abs()));
                    assert!(
                        peak.is_finite() && peak < 200.0,
                        "{hz} Hz at {rate}: {peak}"
                    );
                    // Rings down once the input stops (Q 10 at 30 Hz decays
                    // by e every 0.11 s).
                    let mut tail = vec![0.0f32; rate as usize * 3];
                    f.process(&mut tail);
                    let block_peak = |b: &[f32]| b.iter().fold(0.0f32, |a, s| a.max(s.abs()));
                    let first = block_peak(&tail[..512]);
                    let last = block_peak(&tail[tail.len() - 512..]);
                    assert!(last <= first * 0.01, "{hz} Hz at {rate}: {first} -> {last}");
                }
            }
            // And while FREQ sweeps end to end and back, every 10 ms.
            for m in modes {
                let mut f = unit(rate, m, 30.0, 1.0);
                let mut peak = 0.0f32;
                let block = rate as usize / 100 * 2;
                for i in 0..400 {
                    let v = if i < 200 {
                        i as f32 / 199.0
                    } else {
                        (399 - i) as f32 / 199.0
                    };
                    f.set_freq(v);
                    let mut buf = noise(block, &mut seed);
                    f.process(&mut buf);
                    peak = buf.iter().fold(peak, |a, s| a.max(s.abs()));
                }
                assert!(peak.is_finite() && peak < 200.0, "sweep at {rate}: {peak}");
            }
        }
    }

    #[test]
    fn mode_switches_and_on_do_not_click() {
        let rate = 48_000;
        let mut f = unit(rate, LP, 200.0, 0.3);
        // A 50 Hz tone: LP passes it, HP removes it; the switch must ramp.
        let tone =
            |n: usize| 0.5 * (2.0 * std::f32::consts::PI * 50.0 * n as f32 / rate as f32).sin();
        let mut buf: Vec<f32> = (0..rate as usize / 5)
            .flat_map(|n| [tone(n), tone(n)])
            .collect();
        let half = buf.len() / 2;
        f.process(&mut buf[..half]);
        f.set_modes(HP);
        f.set_on(false);
        f.process(&mut buf[half..]);
        let max_step = buf
            .as_chunks::<2>()
            .0
            .windows(2)
            .fold(0.0f32, |a, w| a.max((w[1][0] - w[0][0]).abs()));
        // A 50 Hz sine at 0.5 moves at most 0.0033 per sample; allow some
        // filter shape on top, but nothing like a step.
        assert!(max_step < 0.01, "max step {max_step}");
    }
}
