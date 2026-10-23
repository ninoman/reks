//! Channel ECHO: the DJM's DUB ECHO on the channel FX knob
//! (`docs/research/djm-eq-fx.md` §3 ECHO).
//!
//! A send into a feedback delay; the dry signal stays at unity, `y = x + echo`.
//! Left of centre the send is band-passed around 1 kHz ("mids"), right of
//! centre high-passed at 2.5 kHz ("highs"). Both band filters always run and
//! the side only picks which one feeds the send, which happens while the send
//! is 0 (the dead zone), so nothing switches filter state. The loop is
//! `delay → 1-pole LP 6 kHz → 1-pole HP 150 Hz → · fb → back`; loop gain is at
//! most `fb ≤ 0.9` at every frequency, so it is stable without a clipper.
//!
//! The delay time is 3/4 beat of the channel's deck (a dotted eighth); the
//! read head glides to a new time so a tempo change bends the repeats
//! instead of clicking. Once the knob is settled in the dead zone and the
//! tail has died, the block is skipped ([`TailGate`]) and passes its input
//! bit for bit.

use super::smooth::Smoother;
use super::svf::{SvfCoeffs, SvfState};
use crate::taper;
use std::f64::consts::{FRAC_1_SQRT_2, PI};

/// Left band-pass centre. UNVERIFIED (the manuals give no figure).
pub const BAND_HZ: f64 = 1_000.0;
/// Left band-pass Q. UNVERIFIED.
pub const BAND_Q: f64 = 0.7;
/// Right high-pass cutoff (Butterworth). UNVERIFIED.
pub const HIGH_HZ: f64 = 2_500.0;
/// Loop low-pass cutoff: each repeat is a little darker. UNVERIFIED.
pub const LOOP_LP_HZ: f64 = 6_000.0;
/// Loop high-pass cutoff: repeats never build up bass. UNVERIFIED.
pub const LOOP_HP_HZ: f64 = 150.0;
/// Echo time as a fraction of a beat (a dotted eighth). UNVERIFIED.
pub const BEAT_FRACTION: f64 = 0.75;
/// Tempo used when the deck has no beat grid.
pub const DEFAULT_BPM: f64 = 120.0;
/// Shortest and longest echo times.
pub const MIN_DELAY_S: f64 = 0.05;
pub const MAX_DELAY_S: f64 = 2.0;
/// Time constant of the read head's glide to a new echo time.
pub const GLIDE_MS: f64 = 100.0;

/// Level below which a send effect counts as silent.
pub(super) const QUIET: f64 = 1e-6;
/// How long a send effect must stay silent before it is bypassed.
pub(super) const QUIET_MS: f64 = 100.0;

/// Values far below hearing are written as exact zeros so state decaying in
/// silence never turns subnormal (see [`super::flush`]).
#[inline]
pub(super) fn flush_tiny(x: f64) -> f64 {
    if x.abs() < 1e-20 {
        0.0
    } else {
        x
    }
}

/// Bypass for a send effect: a block runs while its knob is off centre or
/// its tail is audible, and is skipped once the knob is settled in the dead
/// zone and the output has been quiet for `hold` frames. The owner clears
/// its state when [`TailGate::observe_block`] says so.
#[derive(Debug, Clone)]
pub(super) struct TailGate {
    idle: bool,
    quiet: u64,
    hold: u64,
}

impl TailGate {
    /// Starts idle: a new block has no state to ring out.
    pub(super) fn new(hold_frames: u64) -> Self {
        Self {
            idle: true,
            quiet: 0,
            hold: hold_frames.max(1),
        }
    }

    pub(super) fn is_idle(&self) -> bool {
        self.idle
    }

    /// Called before a block. While idle and the knob's target is in the
    /// dead zone, settles the smoothers and returns `true`: skip the block.
    /// Otherwise wakes up and returns `false`.
    pub(super) fn skip(&mut self, knob: &mut Smoother, parameter: &mut Smoother) -> bool {
        if !self.idle {
            return false;
        }
        if taper::fx_send(knob.target()).amount == 0.0 {
            knob.snap(knob.target());
            parameter.snap(parameter.target());
            return true;
        }
        self.idle = false;
        self.quiet = 0;
        false
    }

    /// Counts one processed frame as loud or quiet.
    #[inline]
    pub(super) fn frame(&mut self, loud: bool) {
        self.quiet = if loud {
            0
        } else {
            self.quiet.saturating_add(1)
        };
    }

    /// Called after a block; returns `true` when the block goes idle and the
    /// owner must clear its state.
    pub(super) fn observe_block(&mut self, knob: &Smoother) -> bool {
        let closed = knob.is_settled() && taper::fx_send(knob.value()).amount == 0.0;
        if closed && self.quiet >= self.hold {
            self.idle = true;
            self.quiet = 0;
            return true;
        }
        false
    }
}

/// One-pole low-pass `y += a·(x − y)`; `x − y` is the matching high-pass.
#[derive(Debug, Clone, Copy, Default)]
struct OnePole {
    z: f64,
}

impl OnePole {
    /// Coefficient for cutoff `fc` (matched pole).
    fn coeff(fc: f64, sample_rate: f64) -> f64 {
        1.0 - (-2.0 * PI * fc / sample_rate).exp()
    }

    #[inline]
    fn low(&mut self, a: f64, x: f64) -> f64 {
        self.z = flush_tiny(self.z + a * (x - self.z));
        self.z
    }
}

/// A stereo channel echo.
#[derive(Debug, Clone)]
pub struct ChannelEcho {
    sample_rate: f64,
    knob: Smoother,
    parameter: Smoother,
    band: SvfCoeffs,
    high: SvfCoeffs,
    band_state: [SvfState; 2],
    high_state: [SvfState; 2],
    loop_lp_a: f64,
    loop_hp_a: f64,
    loop_lp: [OnePole; 2],
    loop_hp: [OnePole; 2],
    /// Stereo delay line, 2 s plus interpolation room.
    line: Vec<[f32; 2]>,
    write: usize,
    /// Current and target read distance in frames.
    delay: f64,
    target: f64,
    glide: f64,
    gate: TailGate,
}

impl ChannelEcho {
    /// Knob at centre (off), PARAMETER at noon, 120 BPM. Allocates the delay
    /// line; nothing else does.
    pub fn new(sample_rate: u32) -> Self {
        let fs = f64::from(sample_rate.max(1));
        let len = (MAX_DELAY_S * fs).ceil() as usize + 2;
        let target = Self::delay_frames(fs, None);
        Self {
            sample_rate: fs,
            knob: Smoother::new(0.5, sample_rate),
            parameter: Smoother::new(0.5, sample_rate),
            band: SvfCoeffs::new(BAND_HZ, 1.0 / BAND_Q, fs),
            high: SvfCoeffs::new(HIGH_HZ, 1.0 / FRAC_1_SQRT_2, fs),
            band_state: [SvfState::default(); 2],
            high_state: [SvfState::default(); 2],
            loop_lp_a: OnePole::coeff(LOOP_LP_HZ, fs),
            loop_hp_a: OnePole::coeff(LOOP_HP_HZ, fs),
            loop_lp: [OnePole::default(); 2],
            loop_hp: [OnePole::default(); 2],
            line: vec![[0.0; 2]; len],
            write: 0,
            delay: target,
            target,
            glide: 1.0 - (-1000.0 / (GLIDE_MS * fs)).exp(),
            // The whole line must be quiet, not just 100 ms of output: the
            // gap between two repeats can be longer than that.
            gate: TailGate::new(len as u64),
        }
    }

    /// Read distance for `beat_frames` (output frames per beat), clamped to
    /// the line; 120 BPM when there is no usable beat.
    fn delay_frames(fs: f64, beat_frames: Option<f64>) -> f64 {
        let beat = beat_frames
            .filter(|b| b.is_finite() && *b > 0.0)
            .unwrap_or(fs * 60.0 / DEFAULT_BPM);
        (BEAT_FRACTION * beat).clamp(MIN_DELAY_S * fs, MAX_DELAY_S * fs)
    }

    /// Channel FX knob, 0..=1, centre off. Smoothed.
    pub fn set_knob(&mut self, value: f32) {
        self.knob.set(taper::unit(value, 0.5));
    }

    /// PARAMETER, 0..=1: feedback by [`taper::echo_feedback`]. Smoothed.
    pub fn set_parameter(&mut self, value: f32) {
        self.parameter.set(taper::unit(value, 0.5));
    }

    /// Output frames per beat of the channel's deck. `None`, non-finite or
    /// non-positive falls back to 120 BPM. The read head glides to the new
    /// time.
    pub fn set_beat_frames(&mut self, frames: Option<f64>) {
        self.target = Self::delay_frames(self.sample_rate, frames);
        if self.gate.is_idle() {
            self.delay = self.target;
        }
    }

    /// True while bypassed: `process` leaves the buffer untouched.
    pub fn is_idle(&self) -> bool {
        self.gate.is_idle()
    }

    /// Adds the echo to interleaved stereo in place. No allocation.
    pub fn process(&mut self, buf: &mut [f32]) {
        if self.gate.skip(&mut self.knob, &mut self.parameter) {
            return;
        }
        let len = self.line.len();
        for frame in buf.as_chunks_mut::<2>().0 {
            let send = taper::fx_send(self.knob.tick());
            let amount = f64::from(send.amount);
            let fb = f64::from(taper::echo_feedback(self.parameter.tick()));
            self.delay += self.glide * (self.target - self.delay);
            let read = self.read();
            let mut loud = false;
            let mut written = [0.0f32; 2];
            for (ch, s) in frame.iter_mut().enumerate() {
                let x = f64::from(*s);
                let band = self.band_state[ch].tick(&self.band, x);
                let high = self.high_state[ch].tick(&self.high, x).high;
                // Unity-peak band-pass; the side only changes at amount 0.
                let src = if send.side < 0.0 {
                    self.band.k * band.band
                } else {
                    high
                };
                let r = flush_tiny(read[ch]);
                let lp = self.loop_lp[ch].low(self.loop_lp_a, r);
                let hp = lp - self.loop_hp[ch].low(self.loop_hp_a, lp);
                let w = flush_tiny(amount * src + fb * hp);
                written[ch] = w as f32;
                loud |= r.abs() >= QUIET || w.abs() >= QUIET;
                *s = (x + r) as f32;
            }
            if let Some(slot) = self.line.get_mut(self.write) {
                *slot = written;
            }
            self.write = (self.write + 1) % len;
            self.gate.frame(loud);
        }
        if self.gate.observe_block(&self.knob) {
            self.clear();
        }
    }

    /// Linear interpolation `delay` frames behind the write head.
    #[inline]
    fn read(&self) -> [f64; 2] {
        let len = self.line.len();
        let mut pos = self.write as f64 - self.delay;
        if pos < 0.0 {
            pos += len as f64;
        }
        let i = (pos.floor() as usize).min(len - 1);
        let frac = pos - pos.floor();
        let a = self.line.get(i).copied().unwrap_or_default();
        let b = self.line.get((i + 1) % len).copied().unwrap_or_default();
        [0, 1].map(|ch| {
            let (a, b) = (f64::from(a[ch]), f64::from(b[ch]));
            a + (b - a) * frac
        })
    }

    /// Silences the block so the next activation starts from nothing.
    fn clear(&mut self) {
        self.line.fill([0.0; 2]);
        self.band_state = [SvfState::default(); 2];
        self.high_state = [SvfState::default(); 2];
        self.loop_lp = [OnePole::default(); 2];
        self.loop_hp = [OnePole::default(); 2];
        self.delay = self.target;
    }

    #[cfg(test)]
    fn read_ms(&self) -> f64 {
        1000.0 * self.delay / self.sample_rate
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

    fn run(e: &mut ChannelEcho, buf: &mut [f32]) {
        for chunk in buf.chunks_mut(512) {
            e.process(chunk);
        }
    }

    fn assert_clean(buf: &[f32]) {
        for s in buf {
            assert!(s.is_finite(), "non-finite sample");
            assert!(*s == 0.0 || s.is_normal(), "subnormal {s:e}");
        }
    }

    /// Burst of full-scale noise at knob 1, then the knob to `close` and
    /// silence until the block goes idle. Returns the silence it took.
    fn ring_out(e: &mut ChannelEcho, rate: u32, close: f32) -> Vec<f32> {
        let mut noise = Noise(7);
        e.set_knob(1.0);
        let mut burst = noise.block(rate as usize / 2);
        run(e, &mut burst);
        assert_clean(&burst);
        e.set_knob(close);
        let mut out = Vec::new();
        for _ in 0..120 {
            let mut buf = vec![0.0; rate as usize];
            run(e, &mut buf);
            assert_clean(&buf);
            out.extend_from_slice(&buf);
            if e.is_idle() {
                return out;
            }
        }
        panic!("echo never went idle at {rate}");
    }

    #[test]
    fn dead_zone_is_bit_for_bit_from_start_and_after_a_tail() {
        for rate in RATES {
            for v in [0.485, 0.5, 0.515] {
                let mut e = ChannelEcho::new(rate);
                e.set_knob(v);
                let x = Noise(3).block(rate as usize / 4);
                let mut y = x.clone();
                run(&mut e, &mut y);
                assert!(e.is_idle());
                assert_eq!(x, y, "{rate} {v} from start");

                ring_out(&mut e, rate, v);
                let mut y = x.clone();
                run(&mut e, &mut y);
                assert_eq!(x, y, "{rate} {v} after a tail");
            }
        }
    }

    #[test]
    fn silence_after_a_burst_and_its_tail_is_exact_zeros() {
        for rate in RATES {
            let mut e = ChannelEcho::new(rate);
            let tail = ring_out(&mut e, rate, 0.5);
            // The tail rang well past the knob closing.
            let early = &tail[..rate as usize];
            assert!(early.iter().any(|s| s.abs() > 1e-3), "{rate}: no tail");
            let mut buf = vec![0.0; rate as usize];
            run(&mut e, &mut buf);
            assert!(buf.iter().all(|s| s.to_bits() == 0), "{rate}");
        }
    }

    #[test]
    fn full_scale_noise_with_swept_controls_stays_bounded_for_60_s() {
        let rate = 48_000;
        let mut e = ChannelEcho::new(rate);
        let mut noise = Noise(11);
        let block = 512;
        let blocks = 60 * rate as usize / block;
        let mut peak = 0.0f32;
        for b in 0..blocks {
            let t = (b * block) as f32 / rate as f32;
            e.set_knob(0.5 + 0.5 * (2.0 * std::f32::consts::PI * t / 7.0).sin());
            e.set_parameter(0.5 + 0.5 * (2.0 * std::f32::consts::PI * t / 11.0).sin());
            let mut buf = noise.block(block);
            e.process(&mut buf);
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
            // Same controls, same burst; only `a` gets a tone afterwards.
            let mut a = ChannelEcho::new(rate);
            let mut b = ChannelEcho::new(rate);
            let mut burst = Noise(5).block(rate as usize / 4);
            let mut burst_b = burst.clone();
            for e in [&mut a, &mut b] {
                e.set_knob(0.0);
            }
            run(&mut a, &mut burst);
            run(&mut b, &mut burst_b);
            for e in [&mut a, &mut b] {
                e.set_knob(0.5);
            }
            // Let the knob ramp close, then compare.
            let mut ramp_a = vec![0.0; 2 * 1024];
            let mut ramp_b = ramp_a.clone();
            run(&mut a, &mut ramp_a);
            run(&mut b, &mut ramp_b);
            let fs = f64::from(rate);
            let x: Vec<f32> = (0..rate as usize)
                .flat_map(|n| {
                    let s = (0.5 * (2.0 * PI * 440.0 * n as f64 / fs).sin()) as f32;
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

    #[test]
    fn first_repeat_is_three_quarters_of_a_beat_at_120_bpm() {
        for rate in RATES {
            let mut e = ChannelEcho::new(rate);
            e.set_knob(1.0);
            e.set_beat_frames(Some(f64::from(rate) / 2.0));
            let n0 = 2_000;
            let frames = n0 + rate as usize / 2;
            let mut buf = vec![0.0; 2 * frames];
            buf[2 * n0] = 1.0;
            buf[2 * n0 + 1] = 1.0;
            run(&mut e, &mut buf);
            let (peak, _) = buf.iter().step_by(2).enumerate().skip(n0 + 1).fold(
                (0, 0.0f32),
                |(i, m), (n, s)| {
                    if s.abs() > m {
                        (n, s.abs())
                    } else {
                        (i, m)
                    }
                },
            );
            let want = n0 as f64 + 0.375 * f64::from(rate);
            assert!(
                (peak as f64 - want).abs() <= 1.0,
                "{rate}: {peak} vs {want}"
            );
            // Nothing before it.
            assert!(buf[2 * (n0 + 1)..2 * (want as usize - 1)]
                .iter()
                .all(|s| *s == 0.0));
        }
    }

    /// Gain of one trip round the loop at `hz` (feedback aside), from the
    /// transfer functions: the two loop filters and the read head's linear
    /// interpolation at the 120 BPM delay, a low-pass of its own when the
    /// delay is not a whole number of frames (375 ms at 44.1 kHz).
    fn loop_gain(rate: u32, hz: f64) -> f64 {
        let fs = f64::from(rate);
        let w = 2.0 * PI * hz / fs;
        let frac = (0.375 * fs).fract();
        // |(1 − f) + f·e^{−jw}|
        let interp = ((1.0 - frac + frac * w.cos()).powi(2) + (frac * w.sin()).powi(2)).sqrt();
        let lp = |a: f64| {
            let b = 1.0 - a;
            // a / (1 − b·e^{−jw})
            a / ((1.0 - b * w.cos()).powi(2) + (b * w.sin()).powi(2)).sqrt()
        };
        // HP = 1 − LP = b(1 − e^{−jw}) / (1 − b·e^{−jw})
        let hp = |a: f64| {
            let b = 1.0 - a;
            let num = b * ((1.0 - w.cos()).powi(2) + w.sin().powi(2)).sqrt();
            num / ((1.0 - b * w.cos()).powi(2) + (b * w.sin()).powi(2)).sqrt()
        };
        interp * lp(OnePole::coeff(LOOP_LP_HZ, fs)) * hp(OnePole::coeff(LOOP_HP_HZ, fs))
    }

    /// Wet energy around each of the first `n` repeats of a burst at `n0`.
    fn repeat_levels(buf: &[f32], n0: usize, delay: usize, n: usize) -> Vec<f64> {
        (1..=n)
            .map(|k| {
                let c = n0 + k * delay;
                let e: f64 = buf[2 * (c - delay / 2)..2 * (c + delay / 2)]
                    .iter()
                    .map(|s| f64::from(*s).powi(2))
                    .sum();
                e.sqrt()
            })
            .collect()
    }

    fn tone_burst(rate: u32, n0: usize, hz: f64, ms: f64, frames: usize) -> Vec<f32> {
        let fs = f64::from(rate);
        let len = (ms * fs / 1000.0) as usize;
        let mut buf = vec![0.0; 2 * frames];
        for n in 0..len {
            let win = 0.5 - 0.5 * (2.0 * PI * n as f64 / len as f64).cos();
            let s = (0.5 * win * (2.0 * PI * hz * n as f64 / fs).sin()) as f32;
            buf[2 * (n0 + n)] = s;
            buf[2 * (n0 + n) + 1] = s;
        }
        buf
    }

    #[test]
    fn repeats_fall_by_feedback_times_the_loop_filters() {
        for rate in RATES {
            for p in [0.0, 0.5, 1.0] {
                let mut e = ChannelEcho::new(rate);
                e.set_knob(1.0);
                e.set_parameter(p);
                let delay = (0.375 * f64::from(rate)).round() as usize;
                let n0 = 2_000;
                let mut buf = tone_burst(rate, n0, 3_000.0, 40.0, n0 + 4 * delay);
                let dry = buf.clone();
                run(&mut e, &mut buf);
                for (y, x) in buf.iter_mut().zip(&dry) {
                    *y -= x;
                }
                let l = repeat_levels(&buf, n0, delay, 3);
                let want = f64::from(taper::echo_feedback(p)) * loop_gain(rate, 3_000.0);
                for k in 0..2 {
                    let ratio = l[k + 1] / l[k];
                    assert!(
                        (ratio - want).abs() <= 0.01,
                        "{rate} p {p}: repeat {k} ratio {ratio:.4} vs {want:.4}"
                    );
                }
            }
        }
    }

    #[test]
    fn repeats_continue_after_the_knob_returns_to_centre() {
        for rate in RATES {
            let mut e = ChannelEcho::new(rate);
            e.set_knob(1.0);
            let delay = (0.375 * f64::from(rate)).round() as usize;
            let n0 = 2_000;
            let mut buf = tone_burst(rate, n0, 3_000.0, 40.0, n0 + 4 * delay);
            let dry = buf.clone();
            // Process the burst, then close the knob before the first repeat.
            let split = 2 * (n0 + delay / 2);
            let (head, tail) = buf.split_at_mut(split);
            run(&mut e, head);
            e.set_knob(0.5);
            run(&mut e, tail);
            assert_eq!(&buf[2 * (n0 + 100)..split], &dry[2 * (n0 + 100)..split]);
            let l = repeat_levels(&buf, n0, delay, 3);
            let want = f64::from(taper::echo_feedback(0.5)) * loop_gain(rate, 3_000.0);
            assert!(l[0] > 0.1, "{rate}: {l:?}");
            assert!((l[2] / l[1] - want).abs() <= 0.01, "{rate}: {l:?}");
        }
    }

    #[test]
    fn tempo_change_glides_the_read_head_without_a_step() {
        for rate in RATES {
            let fs = f64::from(rate);
            let mut e = ChannelEcho::new(rate);
            e.set_knob(0.0);
            e.set_beat_frames(Some(fs * 60.0 / 120.0));
            let tone = |n: usize| (0.25 * (2.0 * PI * 200.0 * n as f64 / fs).sin()) as f32;
            let frames = (2.0 * fs) as usize;
            let x: Vec<f32> = (0..frames).flat_map(|n| [tone(n), tone(n)]).collect();
            let mut y = x.clone();
            let change = frames / 2;
            let after = (0.5 * fs) as usize;
            run(&mut e, &mut y[..2 * change]);
            e.set_beat_frames(Some(fs * 60.0 / 128.0));
            run(&mut e, &mut y[2 * change..2 * (change + after)]);
            let ms = e.read_ms();
            assert!((351.5..=352.0).contains(&ms), "{rate}: {ms} ms");
            run(&mut e, &mut y[2 * (change + after)..]);
            let wet: Vec<f32> = y.iter().zip(&x).map(|(y, x)| y - x).step_by(2).collect();
            assert!(wet.iter().any(|w| w.abs() > 0.05), "{rate}: no echo");
            let step = wet
                .windows(2)
                .skip(change - 1000)
                .map(|w| (w[1] - w[0]).abs())
                .fold(0.0, f32::max);
            assert!(step <= 0.01, "{rate}: step {step}");
        }
    }

    #[test]
    fn unusable_beats_fall_back_to_120_bpm_and_times_are_clamped() {
        let fs = 48_000.0;
        for b in [None, Some(f64::NAN), Some(0.0), Some(-5.0)] {
            assert_eq!(ChannelEcho::delay_frames(fs, b), 0.375 * fs);
        }
        assert_eq!(ChannelEcho::delay_frames(fs, Some(1.0)), MIN_DELAY_S * fs);
        assert_eq!(ChannelEcho::delay_frames(fs, Some(1e9)), MAX_DELAY_S * fs);
    }
}
