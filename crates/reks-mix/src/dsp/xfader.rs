//! Crossfader curve law and the X / off / Y assign (R§9.1).
//!
//! Position 0 is the X end, 1 the Y end. A side's gain depends on how far the
//! fader is from the opposite end, `t = clamp(distance / w, 0, 1)`, as
//! `sin(π/2 · t)^p`. The curve knob sets `w`, the travel a side needs to reach
//! full level: the whole travel on the progressive curve, 1/40 of it on the
//! sharpest. `p` puts the progressive curve's centre at exactly −4 dB (the
//! guide's figure); on sharper curves the centre is at 0 dB and the cut
//! happens within `w` of the end stop.

use std::f32::consts::{FRAC_1_SQRT_2, FRAC_PI_2};

use super::smooth::Smoother;
use crate::taper;

/// Centre dip of the progressive curve (R§9.1).
pub const PROGRESSIVE_CENTRE_DB: f32 = -4.0;

/// Where a channel goes: X side, Y side, or straight to the mix ("off").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Assign {
    X,
    #[default]
    Off,
    Y,
}

/// The two side gains for one crossfader position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct XfadeGains {
    pub x: f32,
    pub y: f32,
}

/// Exponent that puts the progressive curve's centre at −4 dB:
/// `sin(π/4)^p = 10^(−4/20)`.
fn exponent() -> f32 {
    (PROGRESSIVE_CENTRE_DB / 20.0 * std::f32::consts::LN_10) / FRAC_1_SQRT_2.ln()
}

fn side(distance: f32, width: f32) -> f32 {
    let t = (distance / width).clamp(0.0, 1.0);
    (FRAC_PI_2 * t).sin().powf(exponent())
}

/// Side gains for crossfader `pos` and `curve` knob (both 0..=1; curve 0 is
/// progressive, 1 near-instant cut).
pub fn crossfader_gains(pos: f32, curve: f32) -> XfadeGains {
    let pos = taper::unit(pos, 0.5);
    let w = taper::crossfader_width(curve);
    XfadeGains {
        x: side(1.0 - pos, w),
        y: side(pos, w),
    }
}

/// A crossfader whose position and curve ramp like any knob.
#[derive(Debug, Clone)]
pub struct Crossfader {
    pos: Smoother,
    curve: Smoother,
}

impl Crossfader {
    /// Centred, progressive curve.
    pub fn new(sample_rate: u32) -> Self {
        Self {
            pos: Smoother::new(0.5, sample_rate),
            curve: Smoother::new(0.0, sample_rate),
        }
    }

    pub fn set_pos(&mut self, pos: f32) {
        self.pos.set(taper::unit(pos, 0.5));
    }

    pub fn set_curve(&mut self, curve: f32) {
        self.curve.set(taper::unit(curve, 0.0));
    }

    /// Smoothed position, for VCF TO XFADER ([`super::filter::FilterPanel`]).
    pub fn pos(&self) -> f32 {
        self.pos.value()
    }

    /// Advances one frame and returns the side gains.
    #[inline]
    pub fn tick(&mut self) -> XfadeGains {
        crossfader_gains(self.pos.tick(), self.curve.tick())
    }
}

/// A channel's XFADE assign switch. Moving it crossfades between the old and
/// new routing instead of stepping.
#[derive(Debug, Clone)]
pub struct AssignSwitch {
    x: Smoother,
    off: Smoother,
    y: Smoother,
}

impl AssignSwitch {
    pub fn new(assign: Assign, sample_rate: u32) -> Self {
        let mut s = Self {
            x: Smoother::new(0.0, sample_rate),
            off: Smoother::new(0.0, sample_rate),
            y: Smoother::new(0.0, sample_rate),
        };
        s.set(assign);
        s.x.snap(s.x.target());
        s.off.snap(s.off.target());
        s.y.snap(s.y.target());
        s
    }

    pub fn set(&mut self, assign: Assign) {
        let w = |a: Assign| if a == assign { 1.0 } else { 0.0 };
        self.x.set(w(Assign::X));
        self.off.set(w(Assign::Off));
        self.y.set(w(Assign::Y));
    }

    /// Advances one frame; the channel's crossfader gain for `gains`.
    #[inline]
    pub fn tick(&mut self, gains: XfadeGains) -> f32 {
        self.x.tick() * gains.x + self.off.tick() + self.y.tick() * gains.y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db(g: f32) -> f32 {
        20.0 * g.log10()
    }

    #[test]
    fn progressive_curve_at_ends_and_centre() {
        let g = crossfader_gains(0.0, 0.0);
        assert_eq!((g.x, g.y), (1.0, 0.0));
        let g = crossfader_gains(1.0, 0.0);
        assert_eq!((g.x, g.y), (0.0, 1.0));
        let g = crossfader_gains(0.5, 0.0);
        assert!((db(g.x) + 4.0).abs() < 0.01, "{}", db(g.x));
        assert!((db(g.y) + 4.0).abs() < 0.01, "{}", db(g.y));
    }

    #[test]
    fn sharp_curve_at_ends_and_centre() {
        let g = crossfader_gains(0.0, 1.0);
        assert_eq!((g.x, g.y), (1.0, 0.0));
        let g = crossfader_gains(1.0, 1.0);
        assert_eq!((g.x, g.y), (0.0, 1.0));
        let g = crossfader_gains(0.5, 1.0);
        assert_eq!((g.x, g.y), (1.0, 1.0));
        // Near-instant: the far side is at full level 1/40 off its end stop.
        let g = crossfader_gains(1.0 / 40.0, 1.0);
        assert_eq!(g.y, 1.0);
        assert!(crossfader_gains(1.0 / 400.0, 1.0).y < 0.2);
    }

    #[test]
    fn curves_are_monotonic_and_symmetric() {
        for curve in [0.0, 0.3, 0.7, 1.0] {
            let mut last = crossfader_gains(0.0, curve);
            for i in 1..=200 {
                let pos = i as f32 / 200.0;
                let g = crossfader_gains(pos, curve);
                assert!(g.x <= last.x && g.y >= last.y, "curve {curve} pos {pos}");
                let mirror = crossfader_gains(1.0 - pos, curve);
                assert!((g.x - mirror.y).abs() < 1e-6);
                last = g;
            }
        }
    }

    #[test]
    fn assign_picks_a_side_or_bypasses() {
        let g = crossfader_gains(0.0, 0.0);
        let gain = |a| AssignSwitch::new(a, 48_000).tick(g);
        assert_eq!(gain(Assign::X), 1.0);
        assert_eq!(gain(Assign::Y), 0.0);
        assert_eq!(gain(Assign::Off), 1.0);
        assert_eq!(crossfader_gains(f32::NAN, 0.0), crossfader_gains(0.5, 0.0));
    }

    #[test]
    fn assign_switch_ramps_between_routings() {
        let rate = 48_000;
        let gains = crossfader_gains(0.0, 0.0); // X open, Y shut.
        let mut s = AssignSwitch::new(Assign::X, rate);
        assert_eq!(s.tick(gains), 1.0);
        s.set(Assign::Y);
        let mut last = 1.0;
        let mut frames = 0;
        loop {
            let g = s.tick(gains);
            assert!(g <= last && last - g < 0.01, "step {last} -> {g}");
            last = g;
            frames += 1;
            if g == 0.0 {
                break;
            }
        }
        assert!(frames >= rate as usize * 5 / 1000);
    }

    #[test]
    fn crossfader_moves_are_smoothed() {
        let mut xf = Crossfader::new(48_000);
        xf.set_pos(0.0);
        let first = xf.tick();
        assert!(first.x < 0.7 && first.y > 0.6, "{first:?}");
        for _ in 0..480 {
            xf.tick();
        }
        assert_eq!(xf.tick(), crossfader_gains(0.0, 0.0));
        assert_eq!(xf.pos(), 0.0);
    }
}
