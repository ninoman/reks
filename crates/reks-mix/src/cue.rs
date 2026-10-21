//! The headphone cue (Xone:92 cue section, R§9.1): the cued channels, taken
//! pre-EQ or post-EQ, blended with the master by CUE MIX or split (cue left,
//! master right), then CUE LEVEL and the always-on safety limiter.

use crate::control::MasterParam;
use crate::dsp::limiter::Limiter;
use crate::dsp::smooth::Smoother;
use crate::taper;

#[derive(Debug, Clone)]
pub(crate) struct CueBus {
    /// 0 pre-EQ, 1 post-EQ; smoothed, so the switch crossfades.
    post_eq: Smoother,
    mix: Smoother,
    level: Smoother,
    /// 0 blended, 1 split.
    split: Smoother,
    /// Split regardless of the switch: the device has one stereo output for
    /// both ([`crate::CueRouting::SplitMono`]).
    forced_split: bool,
    limiter: Limiter,
}

impl CueBus {
    /// Pre-EQ, cue only, level up, not split.
    pub(crate) fn new(sample_rate: u32) -> Self {
        let s = |v: f32| Smoother::new(v, sample_rate);
        Self {
            post_eq: s(0.0),
            mix: s(0.0),
            level: s(1.0),
            split: s(0.0),
            forced_split: false,
            limiter: Limiter::new(sample_rate),
        }
    }

    /// Takes the cue controls of the master section; ignores the rest.
    pub(crate) fn set(&mut self, param: MasterParam, value: f32) {
        let switch = |v: f32| if v >= 0.5 { 1.0 } else { 0.0 };
        match param {
            MasterParam::CueLevel => self.level.set(taper::unit(value, 0.0)),
            MasterParam::CueMix => self.mix.set(taper::unit(value, 0.0)),
            MasterParam::SplitCue => self.split.set(switch(value)),
            MasterParam::CuePostEq => self.post_eq.set(switch(value)),
            MasterParam::Level => {}
        }
    }

    pub(crate) fn force_split(&mut self, on: bool) {
        self.forced_split = on;
    }

    /// Mixes one block into `out`: `pre` and `post` are the cue sums at the
    /// two taps, `master` the master bus before its limiter.
    pub(crate) fn process(&mut self, pre: &[f32], post: &[f32], master: &[f32], out: &mut [f32]) {
        let frames = pre
            .as_chunks::<2>()
            .0
            .iter()
            .zip(post.as_chunks::<2>().0)
            .zip(master.as_chunks::<2>().0);
        for (((pre, post), m), o) in frames.zip(out.as_chunks_mut::<2>().0) {
            let p = self.post_eq.tick();
            let cue = [0, 1].map(|ch| pre[ch] + p * (post[ch] - pre[ch]));
            let (cg, mg) = taper::cue_mix_gains(self.mix.tick());
            let blend = [0, 1].map(|ch| cg * cue[ch] + mg * m[ch]);
            let split = [0.5 * (cue[0] + cue[1]), 0.5 * (m[0] + m[1])];
            let s = if self.forced_split {
                1.0
            } else {
                self.split.tick()
            };
            let level = taper::cue_level_gain(self.level.tick());
            *o = [0, 1].map(|ch| level * (blend[ch] + s * (split[ch] - blend[ch])));
        }
        self.limiter.process(out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    /// Runs `bus` for 100 ms on constant inputs; the last frame out.
    fn settle(bus: &mut CueBus, pre: [f32; 2], post: [f32; 2], master: [f32; 2]) -> [f32; 2] {
        let n = RATE as usize / 10;
        let fill = |f: [f32; 2]| f.repeat(n);
        let mut out = vec![0.0; n * 2];
        bus.process(&fill(pre), &fill(post), &fill(master), &mut out);
        [out[2 * n - 2], out[2 * n - 1]]
    }

    #[test]
    fn taps_mix_split_and_level() {
        let mut bus = CueBus::new(RATE);
        let (pre, post, master) = ([0.1, 0.2], [0.3, 0.4], [0.05, -0.15]);
        assert_eq!(settle(&mut bus, pre, post, master), pre, "pre-EQ, cue only");
        bus.set(MasterParam::CuePostEq, 1.0);
        let [l, r] = settle(&mut bus, pre, post, master);
        assert!((l - 0.3).abs() < 1e-6 && (r - 0.4).abs() < 1e-6, "post-EQ");
        bus.set(MasterParam::CueMix, 1.0);
        let [l, r] = settle(&mut bus, pre, post, master);
        assert!(
            (l - 0.05).abs() < 1e-6 && (r + 0.15).abs() < 1e-6,
            "master only"
        );
        bus.set(MasterParam::SplitCue, 1.0);
        let [l, r] = settle(&mut bus, pre, post, master);
        assert!((l - 0.35).abs() < 1e-6 && (r + 0.05).abs() < 1e-6, "split");
        bus.set(MasterParam::CueLevel, 0.5);
        let [l, _] = settle(&mut bus, pre, post, master);
        assert!((l - 0.35 * 0.125).abs() < 1e-6, "level");
    }

    #[test]
    fn forced_split_ignores_the_switch_and_the_limiter_holds() {
        let mut bus = CueBus::new(RATE);
        bus.force_split(true);
        let [l, r] = settle(&mut bus, [0.2, 0.4], [0.0; 2], [0.5, 0.1]);
        assert!((l - 0.3).abs() < 1e-6 && (r - 0.3).abs() < 1e-6);
        let [l, r] = settle(&mut bus, [3.0, 3.0], [0.0; 2], [-4.0, -4.0]);
        let ceiling = taper::db_to_gain(crate::dsp::limiter::CEILING_DB);
        assert!(l.abs() <= ceiling && r.abs() <= ceiling);
    }
}
