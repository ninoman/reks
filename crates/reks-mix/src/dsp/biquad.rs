//! Fixed-coefficient second-order sections for the EQ shelves.
//!
//! Coefficients follow M. Vicanek, "Matched Second Order Digital Filters"
//! (2016): poles from the impulse-invariant mapping, zeros chosen so the
//! magnitude matches the analogue prototype at DC, the cutoff and Nyquist.
//! A bilinear low-pass at 2.5 kHz is cramped towards Nyquist (about 14 dB per
//! octave at 5–10 kHz on 44.1 kHz), so the HI kill would not fall at the
//! documented 12 dB/oct; the matched design does (see `docs/mixer-dsp.md`).

use super::flush;
use std::f64::consts::PI;

/// A stereo biquad in transposed direct form II, `f64` state.
#[derive(Debug, Clone, Copy)]
pub struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
    s: [[f64; 2]; 2],
}

/// Pole coefficients `(a1, a2)` for a second-order prototype at `fc`, `q`.
/// Real poles (Q below 0.5) take the hyperbolic branch.
fn poles(w0: f64, q: f64) -> (f64, f64) {
    let zeta = 1.0 / (2.0 * q);
    let c = if zeta <= 1.0 {
        ((1.0 - zeta * zeta).sqrt() * w0).cos()
    } else {
        ((zeta * zeta - 1.0).sqrt() * w0).cosh()
    };
    (-2.0 * (-zeta * w0).exp() * c, (-2.0 * zeta * w0).exp())
}

/// Shared terms of Vicanek's magnitude matching.
struct Match {
    a: (f64, f64),
    a0: f64,
    phi0: f64,
    phi1: f64,
    r: f64,
}

fn matching(fc: f64, q: f64, sample_rate: f64) -> Match {
    let w0 = 2.0 * PI * fc / sample_rate;
    let (a1, a2) = poles(w0, q);
    let big_a0 = (1.0 + a1 + a2).powi(2);
    let big_a1 = (1.0 - a1 + a2).powi(2);
    let big_a2 = -4.0 * a2;
    let phi1 = (w0 / 2.0).sin().powi(2);
    let phi0 = 1.0 - phi1;
    let phi2 = 4.0 * phi0 * phi1;
    Match {
        a: (a1, a2),
        a0: big_a0,
        phi0,
        phi1,
        r: big_a0 * phi0 + big_a1 * phi1 + big_a2 * phi2,
    }
}

impl Biquad {
    /// Matched low-pass at `fc` Hz with quality `q`.
    pub fn lowpass(fc: f64, q: f64, sample_rate: f64) -> Self {
        let m = matching(fc, q, sample_rate);
        let r1 = m.r * q * q;
        let b0_sq = m.a0;
        let b1_sq = ((r1 - b0_sq * m.phi0) / m.phi1).max(0.0);
        let b0 = 0.5 * (b0_sq.sqrt() + b1_sq.sqrt());
        let b1 = b0_sq.sqrt() - b0;
        Self::new([b0, b1, 0.0], m.a)
    }

    /// Matched high-pass at `fc` Hz with quality `q`.
    pub fn highpass(fc: f64, q: f64, sample_rate: f64) -> Self {
        let m = matching(fc, q, sample_rate);
        let b0 = q * m.r.max(0.0).sqrt() / (4.0 * m.phi1);
        Self::new([b0, -2.0 * b0, b0], m.a)
    }

    fn new(b: [f64; 3], (a1, a2): (f64, f64)) -> Self {
        Self {
            b,
            a: [a1, a2],
            s: [[0.0; 2]; 2],
        }
    }

    /// Filters one sample of channel `ch` (0 = left, 1 = right).
    #[inline]
    pub fn tick(&mut self, ch: usize, x: f64) -> f64 {
        let s = &mut self.s[ch & 1];
        let y = self.b[0] * x + s[0];
        s[0] = self.b[1] * x - self.a[0] * y + s[1];
        s[1] = self.b[2] * x - self.a[1] * y;
        flush(s);
        y
    }
}
