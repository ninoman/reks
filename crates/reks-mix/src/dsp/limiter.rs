//! Always-on safety limiter for the master and cue buses. The hardware has
//! none; reks needs one because a WILD resonance peak (+26 dB) goes straight
//! into headphones.
//!
//! Look-ahead peak limiter, stereo-linked, after the design in G. Burton
//! (Signalsmith Audio), "Designing a straightforward limiter" (2022):
//! 1. per frame, the gain that would bring the peak to the ceiling,
//! 2. an exponential release that may only go back up slowly,
//! 3. a moving minimum over the look-ahead window `L`,
//! 4. a moving average over the same `L`,
//! 5. the signal delayed by `L − 1` frames.
//!
//! Every gain averaged in step 4 is a minimum over a window that contains
//! the delayed frame, so the applied gain never exceeds what that frame needs:
//! the ceiling holds by construction, with a smooth attack of `L` frames.
//! A final clamp only covers float rounding in the running sum.

/// Output ceiling: −1 dBFS. Our choice, not a hardware figure.
pub const CEILING_DB: f32 = -1.0;
/// Look-ahead (and attack) time.
pub const LOOKAHEAD_MS: f32 = 1.5;
/// Time for the gain to recover by a factor of e towards unity.
pub const RELEASE_MS: f32 = 100.0;

#[derive(Debug, Clone)]
pub struct Limiter {
    ceiling: f32,
    release: f32,
    /// Gain after the release stage.
    held: f32,
    /// Rings of length `L`, all preallocated in `new`.
    released: Vec<f32>,
    minima: Vec<f32>,
    delay: Vec<[f32; 2]>,
    /// Running sum of `minima`, in `f64` so it does not drift.
    sum: f64,
    pos: usize,
}

impl Limiter {
    pub fn new(sample_rate: u32) -> Self {
        let fs = sample_rate.max(1) as f32;
        let len = ((LOOKAHEAD_MS / 1000.0 * fs).round() as usize).max(2);
        Self {
            ceiling: 10f32.powf(CEILING_DB / 20.0),
            release: 1.0 - (-1.0 / (RELEASE_MS / 1000.0 * fs)).exp(),
            held: 1.0,
            released: vec![1.0; len],
            minima: vec![1.0; len],
            delay: vec![[0.0; 2]; len],
            sum: len as f64,
            pos: 0,
        }
    }

    /// Latency in frames.
    pub fn latency(&self) -> usize {
        self.delay.len() - 1
    }

    /// Limits interleaved stereo in place. No allocation.
    pub fn process(&mut self, buf: &mut [f32]) {
        for frame in buf.as_chunks_mut::<2>().0 {
            let (gain, delayed) = self.step([frame[0], frame[1]]);
            for (out, x) in frame.iter_mut().zip(delayed) {
                *out = (x * gain).clamp(-self.ceiling, self.ceiling);
            }
        }
    }

    /// Steps 1–5 for one frame: the gain and the delayed frame it applies to.
    #[inline]
    fn step(&mut self, input: [f32; 2]) -> (f32, [f32; 2]) {
        let clean = |x: f32| if x.is_finite() { x } else { 0.0 };
        let input = [clean(input[0]), clean(input[1])];
        let peak = input[0].abs().max(input[1].abs());
        let wanted = if peak > self.ceiling {
            self.ceiling / peak
        } else {
            1.0
        };
        self.held = (self.held + (1.0 - self.held) * self.release).min(wanted);

        let len = self.delay.len();
        self.released[self.pos] = self.held;
        let minimum = self.released.iter().fold(1.0f32, |a, &g| a.min(g));
        self.sum += f64::from(minimum) - f64::from(self.minima[self.pos]);
        self.minima[self.pos] = minimum;
        let gain = (self.sum / len as f64) as f32;

        self.delay[self.pos] = input;
        self.pos = (self.pos + 1) % len;
        // The oldest frame still in the ring: `L − 1` frames ago.
        (gain, self.delay[self.pos])
    }
}

#[cfg(test)]
mod tests {
    use super::super::filter::{Filter, FilterModes};
    use super::super::testutil::RATES;
    use super::*;

    fn ceiling() -> f32 {
        10f32.powf(CEILING_DB / 20.0)
    }

    #[test]
    fn quiet_signal_passes_unchanged_after_the_latency() {
        let mut l = Limiter::new(48_000);
        let input: Vec<f32> = (0..4_000)
            .map(|i| 0.5 * ((i as f32) * 0.01).sin())
            .collect();
        let mut buf = input.clone();
        l.process(&mut buf);
        let lat = l.latency() * 2;
        for (o, i) in buf[lat..].iter().zip(&input) {
            assert!((o - i).abs() < 1e-6);
        }
    }

    #[test]
    fn ceiling_holds_on_a_step_with_a_smooth_attack() {
        let mut l = Limiter::new(48_000);
        let mut buf = vec![0.1f32; 2_000];
        buf.extend(std::iter::repeat_n(4.0f32, 2_000));
        let input = buf.clone();
        let mut max_unclamped = 0.0f32;
        let mut l2 = l.clone();
        for f in input.as_chunks::<2>().0 {
            let (g, d) = l2.step(*f);
            max_unclamped = max_unclamped.max((d[0] * g).abs());
        }
        l.process(&mut buf);
        assert!(buf.iter().all(|s| s.abs() <= ceiling()));
        // The look-ahead, not the clamp, does the work.
        assert!(max_unclamped <= ceiling() * (1.0 + 1e-5), "{max_unclamped}");
    }

    #[test]
    fn ceiling_holds_on_a_maximum_resonance_filter_sweep() {
        for rate in RATES {
            for modes in [
                FilterModes {
                    lp: true,
                    bp: false,
                    hp: false,
                },
                FilterModes {
                    lp: false,
                    bp: false,
                    hp: true,
                },
                FilterModes {
                    lp: true,
                    bp: true,
                    hp: false,
                },
            ] {
                let mut f = Filter::new(rate);
                f.set_on(true);
                f.set_modes(modes);
                f.set_resonance(1.0);
                f.set_freq(0.0);
                f.settle();
                let mut l = Limiter::new(rate);
                let mut seed = 0x1234_5678u32;
                let mut worst = 0.0f32;
                let mut worst_unclamped = 0.0f32;
                let mut loudest_in = 0.0f32;
                // Full-scale saw-plus-noise, FREQ swept 30 Hz → 20 kHz → 30 Hz
                // over four seconds in 1 ms steps.
                let block = (rate / 1000) as usize;
                let steps = 4_000;
                let mut phase = 0.0f32;
                let mut buf = vec![0.0f32; block * 2];
                for i in 0..steps {
                    let v = if i < steps / 2 {
                        i as f32 / (steps / 2) as f32
                    } else {
                        (steps - i) as f32 / (steps / 2) as f32
                    };
                    f.set_freq(v);
                    for frame in buf.as_chunks_mut::<2>().0 {
                        seed ^= seed << 13;
                        seed ^= seed >> 17;
                        seed ^= seed << 5;
                        let n = (seed as f32 / u32::MAX as f32) * 2.0 - 1.0;
                        phase = (phase + 110.0 / rate as f32).fract();
                        let s = 0.7 * (2.0 * phase - 1.0) + 0.3 * n;
                        *frame = [s, -s];
                    }
                    f.process(&mut buf);
                    loudest_in = buf.iter().fold(loudest_in, |a, s| a.max(s.abs()));
                    for fr in buf.as_chunks_mut::<2>().0 {
                        let (g, d) = l.step(*fr);
                        let y = [d[0] * g, d[1] * g];
                        worst_unclamped = worst_unclamped.max(y[0].abs().max(y[1].abs()));
                        *fr = y.map(|x| x.clamp(-l.ceiling, l.ceiling));
                    }
                    worst = buf.iter().fold(worst, |a, s| a.max(s.abs()));
                }
                assert!(loudest_in > 4.0, "sweep too tame at {rate}: {loudest_in}");
                assert!(worst <= ceiling(), "{rate} {modes:?}: {worst}");
                assert!(
                    worst_unclamped <= ceiling() * (1.0 + 1e-5),
                    "{rate} {modes:?}: unclamped {worst_unclamped}"
                );
            }
        }
    }

    #[test]
    fn recovers_after_a_peak_and_survives_nan() {
        let rate = 48_000;
        let mut l = Limiter::new(rate);
        let mut buf = vec![f32::NAN, 8.0, 0.0, 0.0];
        l.process(&mut buf);
        let mut quiet = vec![0.25f32; rate as usize * 4]; // 2 s, 20 release time constants
        l.process(&mut quiet);
        assert!(quiet.iter().all(|s| s.is_finite()));
        assert!((quiet[quiet.len() - 1] - 0.25).abs() < 1e-3);
    }
}
