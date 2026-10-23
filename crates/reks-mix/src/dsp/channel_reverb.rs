//! Channel REVERB: the DJM's SPACE on the channel FX knob
//! (`docs/research/djm-eq-fx.md` §3 REVERB).
//!
//! A send into a Freeverb tank; the dry signal stays at unity,
//! `y = x + WET · tank`. Left of centre the send is low-passed at 2 kHz
//! ("mids and lows"), right of centre high-passed at 500 Hz ("mids and
//! highs"); both filters always run and the side only changes while the
//! send is 0. PARAMETER sets the comb feedback, i.e. the decay time.
//!
//! The tank is our own code after Jezar's public-domain Freeverb (2000):
//! 8 parallel damped feedback combs and 4 series all-passes per side, the
//! right side's delays 23 samples longer. Unlike Freeverb each side's tank
//! is fed its own channel, so the wet level does not depend on how
//! correlated left and right are. Once the knob is settled in the dead zone
//! and the output has been quiet for 100 ms, the block is skipped and
//! passes its input bit for bit.

use super::channel_echo::{flush_tiny, TailGate, QUIET, QUIET_MS};
use super::smooth::Smoother;
use super::svf::{SvfCoeffs, SvfState};
use crate::taper;
use std::f64::consts::FRAC_1_SQRT_2;

/// Left low-pass cutoff (Butterworth). UNVERIFIED.
pub const LOW_HZ: f64 = 2_000.0;
/// Right high-pass cutoff (Butterworth). UNVERIFIED.
pub const HIGH_HZ: f64 = 500.0;
/// Freeverb comb lengths in samples at 44.1 kHz.
const COMBS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
/// Freeverb all-pass lengths in samples at 44.1 kHz.
const ALLPASSES: [usize; 4] = [556, 441, 341, 225];
/// Extra length of the right side's delays, samples at 44.1 kHz.
const STEREO_SPREAD: usize = 23;
/// All-pass feedback gain (Freeverb).
const ALLPASS_GAIN: f32 = 0.5;
/// One-pole damping pole in the comb loops at 44.1 kHz; other rates use
/// `DAMPING^(44 100 / fs)` so the damping corner stays put in Hz. UNVERIFIED.
const DAMPING: f64 = 0.3;
/// Tank input gain (Freeverb's `fixedgain`).
const INPUT_GAIN: f64 = 0.015;
/// Wet gain: pink noise at knob 1 and PARAMETER noon comes back about 6 dB
/// under the dry (the spec's level test). Calibrated, not measured on a
/// DJM: UNVERIFIED.
pub const WET: f64 = 2.18;

/// A damped feedback comb.
#[derive(Debug, Clone)]
struct Comb {
    buf: Vec<f32>,
    idx: usize,
    store: f32,
    damp: f32,
}

impl Comb {
    fn new(len: usize, damp: f32) -> Self {
        Self {
            buf: vec![0.0; len.max(1)],
            idx: 0,
            store: 0.0,
            damp,
        }
    }

    #[inline]
    fn tick(&mut self, x: f32, room: f32) -> f32 {
        let Some(slot) = self.buf.get_mut(self.idx) else {
            return 0.0;
        };
        let out = *slot;
        self.store = flush_tiny(f64::from(out * (1.0 - self.damp) + self.store * self.damp)) as f32;
        *slot = flush_tiny(f64::from(x + self.store * room)) as f32;
        self.idx += 1;
        if self.idx == self.buf.len() {
            self.idx = 0;
        }
        out
    }

    fn clear(&mut self) {
        self.buf.fill(0.0);
        self.store = 0.0;
    }
}

/// Freeverb's all-pass (an approximation with gain 0.5 feedback).
#[derive(Debug, Clone)]
struct AllPass {
    buf: Vec<f32>,
    idx: usize,
}

impl AllPass {
    fn new(len: usize) -> Self {
        Self {
            buf: vec![0.0; len.max(1)],
            idx: 0,
        }
    }

    #[inline]
    fn tick(&mut self, x: f32) -> f32 {
        let Some(slot) = self.buf.get_mut(self.idx) else {
            return x;
        };
        let b = *slot;
        *slot = flush_tiny(f64::from(x + b * ALLPASS_GAIN)) as f32;
        self.idx += 1;
        if self.idx == self.buf.len() {
            self.idx = 0;
        }
        b - x
    }

    fn clear(&mut self) {
        self.buf.fill(0.0);
    }
}

/// One side's tank.
#[derive(Debug, Clone)]
struct Tank {
    combs: [Comb; 8],
    allpasses: [AllPass; 4],
}

impl Tank {
    fn new(sample_rate: f64, spread: usize) -> Self {
        let scale = |len: usize| ((len + spread) as f64 * sample_rate / 44_100.0).round() as usize;
        let damp = DAMPING.powf(44_100.0 / sample_rate) as f32;
        Self {
            combs: COMBS.map(|l| Comb::new(scale(l), damp)),
            allpasses: ALLPASSES.map(|l| AllPass::new(scale(l))),
        }
    }

    #[inline]
    fn tick(&mut self, x: f32, room: f32) -> f32 {
        let mut out = 0.0;
        for c in &mut self.combs {
            out += c.tick(x, room);
        }
        for a in &mut self.allpasses {
            out = a.tick(out);
        }
        out
    }

    fn clear(&mut self) {
        self.combs.iter_mut().for_each(Comb::clear);
        self.allpasses.iter_mut().for_each(AllPass::clear);
    }
}

/// A stereo channel reverb.
#[derive(Debug, Clone)]
pub struct ChannelReverb {
    knob: Smoother,
    parameter: Smoother,
    low: SvfCoeffs,
    high: SvfCoeffs,
    low_state: [SvfState; 2],
    high_state: [SvfState; 2],
    tanks: [Tank; 2],
    gate: TailGate,
}

impl ChannelReverb {
    /// Knob at centre (off), PARAMETER at noon. Allocates the tanks; nothing
    /// else does.
    pub fn new(sample_rate: u32) -> Self {
        let fs = f64::from(sample_rate.max(1));
        let butterworth = 1.0 / FRAC_1_SQRT_2;
        Self {
            knob: Smoother::new(0.5, sample_rate),
            parameter: Smoother::new(0.5, sample_rate),
            low: SvfCoeffs::new(LOW_HZ, butterworth, fs),
            high: SvfCoeffs::new(HIGH_HZ, butterworth, fs),
            low_state: [SvfState::default(); 2],
            high_state: [SvfState::default(); 2],
            tanks: [Tank::new(fs, 0), Tank::new(fs, STEREO_SPREAD)],
            gate: TailGate::new((QUIET_MS * fs / 1000.0).ceil() as u64),
        }
    }

    /// Channel FX knob, 0..=1, centre off. Smoothed.
    pub fn set_knob(&mut self, value: f32) {
        self.knob.set(taper::unit(value, 0.5));
    }

    /// PARAMETER, 0..=1: comb feedback by [`taper::reverb_room`]. Smoothed.
    pub fn set_parameter(&mut self, value: f32) {
        self.parameter.set(taper::unit(value, 0.5));
    }

    /// True while bypassed: `process` leaves the buffer untouched.
    pub fn is_idle(&self) -> bool {
        self.gate.is_idle()
    }

    /// Adds the reverb to interleaved stereo in place. No allocation.
    pub fn process(&mut self, buf: &mut [f32]) {
        if self.gate.skip(&mut self.knob, &mut self.parameter) {
            return;
        }
        for frame in buf.as_chunks_mut::<2>().0 {
            let send = taper::fx_send(self.knob.tick());
            let gain = f64::from(send.amount) * INPUT_GAIN;
            let room = taper::reverb_room(self.parameter.tick());
            let mut loud = false;
            for (ch, s) in frame.iter_mut().enumerate() {
                let x = f64::from(*s);
                let low = self.low_state[ch].tick(&self.low, x).low;
                let high = self.high_state[ch].tick(&self.high, x).high;
                let src = if send.side < 0.0 { low } else { high };
                let wet =
                    flush_tiny(WET * f64::from(self.tanks[ch].tick((gain * src) as f32, room)));
                loud |= wet.abs() >= QUIET;
                *s = (x + wet) as f32;
            }
            self.gate.frame(loud);
        }
        if self.gate.observe_block(&self.knob) {
            self.clear();
        }
    }

    /// Silences the block so the next activation starts from nothing.
    fn clear(&mut self) {
        self.low_state = [SvfState::default(); 2];
        self.high_state = [SvfState::default(); 2];
        self.tanks.iter_mut().for_each(Tank::clear);
    }
}

#[cfg(test)]
mod tests {
    use super::super::testutil::RATES;
    use super::*;

    /// xorshift64* white noise in −1..1.
    struct Noise(u64);
    impl Noise {
        fn next(&mut self) -> f32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            ((self.0 >> 40) as f32 / (1u64 << 23) as f32) - 1.0
        }
        fn block(&mut self, frames: usize) -> Vec<f32> {
            (0..2 * frames).map(|_| self.next()).collect()
        }
    }

    fn run(r: &mut ChannelReverb, buf: &mut [f32]) {
        for chunk in buf.chunks_mut(512) {
            r.process(chunk);
        }
    }

    fn assert_clean(buf: &[f32]) {
        for s in buf {
            assert!(s.is_finite(), "non-finite sample");
            assert!(*s == 0.0 || s.is_normal(), "subnormal {s:e}");
        }
    }

    fn ring_out(r: &mut ChannelReverb, rate: u32, close: f32) -> Vec<f32> {
        let mut noise = Noise(7);
        r.set_knob(1.0);
        let mut burst = noise.block(rate as usize / 2);
        run(r, &mut burst);
        assert_clean(&burst);
        r.set_knob(close);
        let mut out = Vec::new();
        for _ in 0..60 {
            let mut buf = vec![0.0; rate as usize];
            run(r, &mut buf);
            assert_clean(&buf);
            out.extend_from_slice(&buf);
            if r.is_idle() {
                return out;
            }
        }
        panic!("reverb never went idle at {rate}");
    }

    #[test]
    fn dead_zone_is_bit_for_bit_from_start_and_after_a_tail() {
        for rate in RATES {
            for v in [0.485, 0.5, 0.515] {
                let mut r = ChannelReverb::new(rate);
                r.set_knob(v);
                let x = Noise(3).block(rate as usize / 4);
                let mut y = x.clone();
                run(&mut r, &mut y);
                assert!(r.is_idle());
                assert_eq!(x, y, "{rate} {v} from start");

                ring_out(&mut r, rate, v);
                let mut y = x.clone();
                run(&mut r, &mut y);
                assert_eq!(x, y, "{rate} {v} after a tail");
            }
        }
    }

    #[test]
    fn silence_after_a_burst_and_its_tail_is_exact_zeros() {
        for rate in RATES {
            let mut r = ChannelReverb::new(rate);
            let tail = ring_out(&mut r, rate, 0.5);
            assert!(
                tail[..rate as usize / 2].iter().any(|s| s.abs() > 1e-3),
                "{rate}: no tail"
            );
            let mut buf = vec![0.0; rate as usize];
            run(&mut r, &mut buf);
            assert!(buf.iter().all(|s| s.to_bits() == 0), "{rate}");
        }
    }

    #[test]
    fn full_scale_noise_with_swept_controls_stays_bounded_for_60_s() {
        let rate = 48_000;
        let mut r = ChannelReverb::new(rate);
        let mut noise = Noise(11);
        let block = 512;
        let blocks = 60 * rate as usize / block;
        let mut peak = 0.0f32;
        for b in 0..blocks {
            let t = (b * block) as f32 / rate as f32;
            r.set_knob(0.5 + 0.5 * (2.0 * std::f32::consts::PI * t / 7.0).sin());
            r.set_parameter(0.5 + 0.5 * (2.0 * std::f32::consts::PI * t / 11.0).sin());
            let mut buf = noise.block(block);
            r.process(&mut buf);
            for s in &buf {
                assert!(s.is_finite());
                peak = peak.max(s.abs());
            }
        }
        assert!(peak < 8.0, "peak {peak}");
    }

    #[test]
    fn dry_path_is_untouched_while_the_tail_rings() {
        for rate in RATES {
            let mut a = ChannelReverb::new(rate);
            let mut b = ChannelReverb::new(rate);
            let mut burst = Noise(5).block(rate as usize / 4);
            let mut burst_b = burst.clone();
            for r in [&mut a, &mut b] {
                r.set_knob(0.0);
            }
            run(&mut a, &mut burst);
            run(&mut b, &mut burst_b);
            for r in [&mut a, &mut b] {
                r.set_knob(0.5);
            }
            let mut ramp_a = vec![0.0; 2 * 1024];
            let mut ramp_b = ramp_a.clone();
            run(&mut a, &mut ramp_a);
            run(&mut b, &mut ramp_b);
            let fs = f64::from(rate);
            let x: Vec<f32> = (0..rate as usize / 2)
                .flat_map(|n| {
                    let s =
                        (0.5 * (2.0 * std::f64::consts::PI * 440.0 * n as f64 / fs).sin()) as f32;
                    [s, -s]
                })
                .collect();
            let mut ya = x.clone();
            let mut yb = vec![0.0; x.len()];
            run(&mut a, &mut ya);
            run(&mut b, &mut yb);
            assert!(yb.iter().any(|s| s.abs() > 1e-3), "{rate}: no tail");
            for ((ya, yb), x) in ya.iter().zip(&yb).zip(&x) {
                assert!((ya - yb - x).abs() <= 1e-6, "{rate}");
            }
        }
    }

    /// RT60 from the impulse response's Schroeder decay, fitted over
    /// −5 … −25 dB (T20).
    fn rt60(rate: u32, parameter: f32, seconds: f64) -> f64 {
        let fs = f64::from(rate);
        let mut r = ChannelReverb::new(rate);
        r.set_knob(0.0);
        r.set_parameter(parameter);
        let n0 = 2_000;
        let frames = n0 + (seconds * fs) as usize;
        let mut buf = vec![0.0; 2 * frames];
        buf[2 * n0] = 1.0;
        buf[2 * n0 + 1] = 1.0;
        run(&mut r, &mut buf);
        let energy: Vec<f64> = buf
            .chunks(2)
            .skip(n0 + 1)
            .map(|f| f64::from(f[0]).powi(2) + f64::from(f[1]).powi(2))
            .collect();
        let mut edc = vec![0.0; energy.len()];
        let mut acc = 0.0;
        for (i, e) in energy.iter().enumerate().rev() {
            acc += e;
            edc[i] = acc;
        }
        let total = edc[0];
        let db = |i: usize| 10.0 * (edc[i] / total).log10();
        let t5 = (0..edc.len()).find(|&i| db(i) <= -5.0).unwrap_or(0);
        let t25 = (0..edc.len())
            .find(|&i| db(i) <= -25.0)
            .unwrap_or(edc.len());
        3.0 * (t25 - t5) as f64 / fs
    }

    #[test]
    fn decay_time_follows_parameter() {
        for rate in RATES {
            for (p, want) in [(0.0, 0.6), (0.5, 1.1), (1.0, 4.2)] {
                let got = rt60(rate, p, 2.0 * want + 1.0);
                assert!(
                    (got / want - 1.0).abs() <= 0.25,
                    "{rate} p {p}: RT60 {got:.2} s vs {want} s"
                );
            }
        }
    }

    /// Paul Kellet's economy pink-noise filter over white noise.
    fn pink(frames: usize, seed: u64) -> Vec<f32> {
        let mut noise = Noise(seed);
        let mut b = [[0.0f32; 3]; 2];
        (0..2 * frames)
            .map(|i| {
                let w = noise.next();
                let b = &mut b[i % 2];
                b[0] = 0.99765 * b[0] + w * 0.099_046;
                b[1] = 0.963 * b[1] + w * 0.296_516_4;
                b[2] = 0.57 * b[2] + w * 1.052_691_3;
                0.05 * (b[0] + b[1] + b[2] + w * 0.1848)
            })
            .collect()
    }

    #[test]
    fn wet_sits_6_db_under_the_dry_on_pink_noise() {
        for rate in RATES {
            let mut r = ChannelReverb::new(rate);
            r.set_knob(1.0);
            r.set_parameter(0.5);
            let settle = rate as usize;
            let x = pink(settle + 4 * rate as usize, 9);
            let mut y = x.clone();
            run(&mut r, &mut y);
            let rms = |v: &mut dyn Iterator<Item = f64>| {
                let (s, n) = v.fold((0.0, 0.0), |(s, n), x| (s + x * x, n + 1.0));
                (s / n).sqrt()
            };
            let dry = rms(&mut x[2 * settle..].iter().map(|s| f64::from(*s)));
            let wet = rms(&mut y[2 * settle..]
                .iter()
                .zip(&x[2 * settle..])
                .map(|(y, x)| f64::from(y - x)));
            let db = 20.0 * (wet / dry).log10();
            assert!((db + 6.0).abs() <= 1.0, "{rate}: wet {db:.2} dB");
        }
    }
}
