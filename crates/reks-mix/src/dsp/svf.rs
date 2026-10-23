//! Trapezoidal (zero-delay-feedback) state-variable filter.
//!
//! Our own code from A. Simper, "Linear Trapezoidal Integrated State Variable
//! Filter" / "Solving the continuous SVF equations using trapezoidal
//! integration and equivalent currents" (Cytomic technical papers, 2013,
//! https://cytomic.com/technical-papers). Unlike a direct-form biquad it stays
//! stable and click-free while cutoff and Q change every sample, which the
//! sweepable filters and the mid bells need.

use super::flush;
use std::f64::consts::PI;

/// Coefficients for one cutoff and damping.
#[derive(Debug, Clone, Copy)]
pub struct SvfCoeffs {
    a1: f64,
    a2: f64,
    a3: f64,
    /// Damping, 1/Q.
    pub k: f64,
}

impl SvfCoeffs {
    /// `fc` is clamped below Nyquist so `tan` stays finite.
    pub fn new(fc: f64, k: f64, sample_rate: f64) -> Self {
        let fc = fc.clamp(1.0, 0.49 * sample_rate);
        Self::with_g((PI * fc / sample_rate).tan(), k)
    }

    /// From the prewarped integrator gain `g = tan(π·fc/fs)` directly; the
    /// shelves scale `g` by √A rather than the cutoff.
    pub fn with_g(g: f64, k: f64) -> Self {
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        Self {
            a1,
            a2,
            a3: g * a2,
            k,
        }
    }
}

/// One channel's integrator state.
#[derive(Debug, Clone, Copy, Default)]
pub struct SvfState {
    /// Integrator states `ic1eq`, `ic2eq`.
    ic: [f64; 2],
}

/// The three simultaneous outputs. `band` is not normalised: its peak gain
/// is Q, so `k * band` has unity peak and `low + k * band + high == input`.
#[derive(Debug, Clone, Copy)]
pub struct SvfOut {
    pub low: f64,
    pub band: f64,
    pub high: f64,
}

impl SvfState {
    #[inline]
    pub fn tick(&mut self, c: &SvfCoeffs, x: f64) -> SvfOut {
        let [ic1, ic2] = self.ic;
        let v3 = x - ic2;
        let v1 = c.a1 * ic1 + c.a2 * v3;
        let v2 = ic2 + c.a2 * ic1 + c.a3 * v3;
        self.ic = [2.0 * v1 - ic1, 2.0 * v2 - ic2];
        flush(&mut self.ic);
        SvfOut {
            low: v2,
            band: v1,
            high: x - c.k * v1 - v2,
        }
    }
}
