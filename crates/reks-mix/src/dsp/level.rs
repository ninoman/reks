//! Channel LEVEL trim plus fader, and the master level, as smoothed gains.

use super::smooth::Smoother;
use crate::taper;

/// A channel's trim and fader gains for one frame. They are separate
/// because the cue taps the channel between them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LevelGains {
    pub trim: f32,
    pub fader: f32,
}

/// A channel's trim and fader. Both knobs ramp.
#[derive(Debug, Clone)]
pub struct ChannelLevel {
    trim: Smoother,
    fader: Smoother,
}

impl ChannelLevel {
    /// Trim at noon (0 dB), fader at `fader` with no ramp.
    pub fn new(fader: f32, sample_rate: u32) -> Self {
        Self {
            trim: Smoother::new(0.5, sample_rate),
            fader: Smoother::new(taper::unit(fader, 0.0), sample_rate),
        }
    }

    pub fn set_trim(&mut self, value: f32) {
        self.trim.set(taper::unit(value, 0.5));
    }

    pub fn set_fader(&mut self, value: f32) {
        self.fader.set(taper::unit(value, 0.0));
    }

    /// Advances one frame and returns both gains.
    #[inline]
    pub fn tick(&mut self) -> LevelGains {
        LevelGains {
            trim: taper::db_to_gain(taper::trim_db(self.trim.tick())),
            fader: taper::fader_gain(self.fader.tick()),
        }
    }
}

/// The master level knob as a smoothed gain.
#[derive(Debug, Clone)]
pub struct MasterLevel {
    level: Smoother,
}

impl MasterLevel {
    /// Starts at `value` with no ramp.
    pub fn new(value: f32, sample_rate: u32) -> Self {
        Self {
            level: Smoother::new(taper::unit(value, 0.0), sample_rate),
        }
    }

    pub fn set(&mut self, value: f32) {
        self.level.set(taper::unit(value, 0.0));
    }

    #[inline]
    pub fn tick(&mut self) -> f32 {
        taper::master_gain(self.level.tick())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settle(mut tick: impl FnMut() -> f32) -> f32 {
        let mut g = 0.0;
        for _ in 0..10_000 {
            g = tick();
        }
        g
    }

    #[test]
    fn channel_gains_follow_trim_and_fader() {
        let mut c = ChannelLevel::new(0.0, 48_000);
        assert_eq!(
            c.tick(),
            LevelGains {
                trim: 1.0,
                fader: 0.0
            }
        );
        c.set_fader(1.0);
        assert!((settle(|| c.tick().fader) - 1.0).abs() < 1e-6);
        c.set_trim(1.0);
        let g = settle(|| c.tick().trim);
        assert!((20.0 * g.log10() - 10.0).abs() < 1e-3);
        c.set_trim(0.0);
        c.set_fader(0.5);
        let g = settle(|| {
            let g = c.tick();
            g.trim * g.fader
        });
        assert!((20.0 * g.log10() + 10.0 + 18.06).abs() < 0.01);
    }

    #[test]
    fn master_ramps_to_its_taper() {
        let mut m = MasterLevel::new(1.0, 44_100);
        assert_eq!(m.tick(), 1.0);
        m.set(0.0);
        let first = m.tick();
        assert!(first > 0.9 && first < 1.0);
        assert_eq!(settle(|| m.tick()), 0.0);
    }
}
