//! Mixer DSP: pure, real-time-safe building blocks for the Phase 5 mixer
//! (D21). Every `process` works in place on interleaved stereo `f32` and
//! never allocates, locks or does I/O; buffers are sized in `new`.
//! Formulas, sources and tolerances: `docs/mixer-dsp.md`.

pub mod biquad;
pub mod eq;
pub mod filter;
pub mod smooth;
pub mod svf;

#[cfg(test)]
mod testutil;

/// Flushes a filter's two state values to zero together once both are far
/// below hearing (−400 dB), so state decaying in silence never turns
/// subnormal (slow on x86) and reaches exact silence. Flushing them one at a
/// time can leave a limit cycle around the threshold.
#[inline]
pub(crate) fn flush(s: &mut [f64; 2]) {
    if s[0].abs() < 1e-20 && s[1].abs() < 1e-20 {
        *s = [0.0; 2];
    }
}
