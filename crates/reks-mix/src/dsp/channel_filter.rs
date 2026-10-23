//! The channel FX FILTER (`docs/research/djm-eq-fx.md` §3): one bipolar
//! knob per channel, low-pass to the left, high-pass to the right, off at
//! centre; resonance from the mixer-wide PARAMETER.
//!
//! Two SVFs in series, a low-pass then a high-pass, each wide open while the
//! knob is on the other side. One SVF that flipped from LP to HP at centre
//! would carry its state across the flip and click. Not the X/Y [`super::filter::Filter`]:
//! that is a shared unit with its own law.

use super::smooth::Smoother;
use super::svf::{SvfCoeffs, SvfState};
use crate::taper;

#[derive(Debug, Clone)]
pub struct ChannelFilter {
    sample_rate: f64,
    knob: Smoother,
    parameter: Smoother,
    lp: SvfCoeffs,
    hp: SvfCoeffs,
    wet: f64,
    state: [[SvfState; 2]; 2],
    /// Bypassed and cleared while the knob is settled in the dead zone.
    idle: bool,
}

impl ChannelFilter {
    /// Knob at centre (off), PARAMETER at noon.
    pub fn new(sample_rate: u32) -> Self {
        let mut f = Self {
            sample_rate: f64::from(sample_rate),
            knob: Smoother::new(0.5, sample_rate),
            parameter: Smoother::new(0.5, sample_rate),
            lp: SvfCoeffs::with_g(1.0, 1.0),
            hp: SvfCoeffs::with_g(1.0, 1.0),
            wet: 0.0,
            state: [[SvfState::default(); 2]; 2],
            idle: true,
        };
        f.retune();
        f
    }

    /// Channel FX knob, 0..=1, 0.5 off.
    pub fn set_knob(&mut self, value: f32) {
        self.knob.set(taper::unit(value, 0.5));
    }

    /// PARAMETER (resonance), 0..=1.
    pub fn set_parameter(&mut self, value: f32) {
        self.parameter.set(taper::unit(value, 0.5));
    }

    pub fn is_idle(&self) -> bool {
        self.idle
    }

    fn retune(&mut self) {
        let c = taper::channel_filter(self.knob.value(), self.parameter.value());
        let fs = self.sample_rate;
        self.lp = SvfCoeffs::new(f64::from(c.lp_hz), 1.0 / f64::from(c.lp_q), fs);
        self.hp = SvfCoeffs::new(f64::from(c.hp_hz), 1.0 / f64::from(c.hp_q), fs);
        self.wet = f64::from(c.wet);
    }

    fn at_rest(&self) -> bool {
        self.knob.is_settled() && taper::fx_send(self.knob.value()).amount == 0.0
    }

    /// Filters interleaved stereo in place. No allocation.
    pub fn process(&mut self, buf: &mut [f32]) {
        if self.at_rest() {
            if !self.idle {
                self.state = [[SvfState::default(); 2]; 2];
                self.idle = true;
            }
            // Settle PARAMETER too, so the next move starts from it.
            if !self.parameter.is_settled() {
                self.parameter.snap(self.parameter.target());
            }
            return;
        }
        if self.idle {
            self.retune();
            self.idle = false;
        }
        for frame in buf.as_chunks_mut::<2>().0 {
            let ramping = !self.knob.is_settled() || !self.parameter.is_settled();
            self.knob.tick();
            self.parameter.tick();
            if ramping {
                self.retune();
            }
            for (ch, s) in frame.iter_mut().enumerate() {
                let x = f64::from(*s);
                let low = self.state[0][ch].tick(&self.lp, x).low;
                let high = self.state[1][ch].tick(&self.hp, low).high;
                *s = (x + self.wet * (high - x)) as f32;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::testutil::{gain_db, gain_db_settled, RATES};
    use super::*;

    fn filter(rate: u32, knob: f32, parameter: f32) -> ChannelFilter {
        let mut f = ChannelFilter::new(rate);
        f.knob.snap(knob);
        f.parameter.snap(parameter);
        f
    }

    fn noise(len: usize) -> Vec<f32> {
        let mut seed = 0x9e37_79b9u32;
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
    fn dead_zone_is_bit_for_bit() {
        for rate in RATES {
            for v in [0.485, 0.5, 0.515] {
                let mut f = filter(rate, v, 0.5);
                let input = noise(rate as usize);
                let mut buf = input.clone();
                for c in buf.chunks_mut(512) {
                    f.process(c);
                }
                assert_eq!(buf, input, "{rate} {v}");
                assert!(f.is_idle());
            }
        }
    }

    #[test]
    fn back_at_centre_is_bit_for_bit_again() {
        let mut f = ChannelFilter::new(48_000);
        f.set_knob(0.0);
        let mut buf = noise(9_600);
        f.process(&mut buf);
        f.set_knob(0.5);
        let mut buf = noise(9_600);
        f.process(&mut buf);
        let input = noise(4_800);
        let mut buf = input.clone();
        f.process(&mut buf);
        assert_eq!(buf, input);
    }

    #[test]
    fn full_travel_removes_the_far_band() {
        for rate in RATES {
            let mut f = filter(rate, 0.0, 0.5);
            let lp = gain_db(rate, 1_000.0, |b| f.process(b));
            let mut f = filter(rate, 1.0, 0.5);
            let hp = gain_db(rate, 100.0, |b| f.process(b));
            assert!(lp < -40.0 && hp < -40.0, "{rate}: LP {lp:.1}, HP {hp:.1}");
        }
    }

    #[test]
    fn gain_at_the_cutoff_follows_resonance() {
        for rate in RATES {
            for (knob, lp) in [(0.3, true), (0.7, false)] {
                for (parameter, want) in [(0.0, -3.0), (0.5, 2.7)] {
                    let c = taper::channel_filter(knob, parameter);
                    let hz = f64::from(if lp { c.lp_hz } else { c.hp_hz });
                    let mut f = filter(rate, knob, parameter);
                    let db = gain_db_settled(rate, hz, 0.5, |b| f.process(b));
                    assert!(
                        (db - want).abs() <= 0.3,
                        "{rate}, knob {knob}, P {parameter}: {db:.2}"
                    );
                }
            }
        }
    }

    fn tone(rate: u32, hz: f32, amp: f32, frames: usize) -> Vec<f32> {
        (0..frames)
            .flat_map(|i| {
                let s = amp * (2.0 * std::f32::consts::PI * hz * i as f32 / rate as f32).sin();
                [s, s]
            })
            .collect()
    }

    #[test]
    fn sweeping_through_centre_does_not_step() {
        let rate = 48_000;
        // Program level, −12 dBFS.
        const AMP: f32 = 0.25;
        for hz in [50.0f32, 5_000.0] {
            let mut f = ChannelFilter::new(rate);
            let mut buf = tone(rate, hz, AMP, rate as usize);
            for (i, c) in buf.chunks_mut(256).enumerate() {
                f.set_knob((i as f32 * 0.02).sin() * 0.5 + 0.5);
                f.process(c);
            }
            // The tone's own steepest step, times the resonance peak at
            // PARAMETER noon (+3.2 dB): what a static filter could give.
            let max_step = 1.45 * AMP * 2.0 * (std::f32::consts::PI * hz / rate as f32).sin();
            for (n, w) in buf.as_chunks::<2>().0.windows(2).enumerate() {
                let d = (w[1][0] - w[0][0]).abs() - max_step;
                assert!(d <= 0.01, "{hz} Hz, frame {n}: excess {d}");
            }
        }
    }

    /// A full-travel jump sweeps 17 octaves in one 10 ms ramp. The resonant
    /// low-pass then releases its stored bass as a short swell (about +6 dB
    /// for 10 ms at 50 Hz, steps up to 0.012 at −12 dBFS), not a click: it
    /// stays bounded and decays. A knob drag never moves this fast.
    #[test]
    fn full_travel_jumps_stay_bounded() {
        let rate = 48_000;
        const AMP: f32 = 0.25;
        for hz in [50.0f32, 5_000.0] {
            let mut f = ChannelFilter::new(rate);
            let mut buf = tone(rate, hz, AMP, 12_000);
            for (i, c) in buf.chunks_mut(8_000).enumerate() {
                f.set_knob([0.0, 1.0, 0.5][i]);
                f.process(c);
            }
            let peak = buf.iter().fold(0.0f32, |p, s| p.max(s.abs()));
            assert!(peak <= 2.2 * AMP, "{hz} Hz: peak {peak}");
            let max_step = 2.2 * AMP * 2.0 * (std::f32::consts::PI * hz / rate as f32).sin();
            for w in buf.as_chunks::<2>().0.windows(2) {
                assert!((w[1][0] - w[0][0]).abs() <= max_step + 0.01, "{hz} Hz");
            }
        }
    }

    #[test]
    fn noise_with_everything_swept_stays_finite() {
        let rate = 48_000;
        let mut f = ChannelFilter::new(rate);
        let mut peak = 0.0f32;
        for i in 0..(60 * rate as usize / 512) {
            f.set_knob(((i as f32) * 0.003).sin() * 0.5 + 0.5);
            f.set_parameter(((i as f32) * 0.0011).cos() * 0.5 + 0.5);
            let mut buf = noise(1_024);
            f.process(&mut buf);
            peak = buf.iter().fold(peak, |p, s| p.max(s.abs()));
            assert!(buf.iter().all(|s| s.is_finite()));
        }
        assert!(peak < 8.0, "{peak}");
    }

    #[test]
    fn silence_after_a_burst_is_exact_zeros() {
        for rate in RATES {
            let mut f = filter(rate, 0.1, 1.0);
            let mut burst: Vec<f32> = (0..2_048)
                .map(|i| if i % 7 < 3 { 1.0 } else { -1.0 })
                .collect();
            f.process(&mut burst);
            let mut tail = vec![0.0f32; rate as usize * 2 * 10];
            for chunk in tail.chunks_mut(512) {
                f.process(chunk);
            }
            assert!(tail.iter().all(|s| s.is_finite() && !s.is_subnormal()));
            assert!(tail[tail.len() - 1_024..].iter().all(|&s| s == 0.0));
        }
    }
}
