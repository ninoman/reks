//! Sine-measurement helpers shared by the DSP tests.

use std::f64::consts::PI;

/// Sample rates every response test runs at.
pub const RATES: [u32; 3] = [44_100, 48_000, 96_000];

/// Test-tone amplitude: low enough that +26 dB resonance peaks stay finite
/// and readable.
const AMP: f64 = 0.05;

/// Frequencies from `lo` to `hi` (capped at 0.45 × the rate), `per_octave`
/// steps per octave.
pub fn log_sweep(lo: f64, hi: f64, rate: u32, per_octave: u32) -> Vec<f64> {
    let hi = hi.min(0.45 * f64::from(rate));
    let steps = ((hi / lo).log2() * f64::from(per_octave)).floor() as u32;
    (0..=steps)
        .map(|i| lo * 2f64.powf(f64::from(i) / f64::from(per_octave)))
        .collect()
}

/// Steady-state gain in dB of `process` (interleaved stereo, in place) for a
/// sine at `hz`, measured on the left channel by projection onto sine and
/// cosine over a whole number of cycles after a settling time.
pub fn gain_db(rate: u32, hz: f64, process: impl FnMut(&mut [f32])) -> f64 {
    gain_db_settled(rate, hz, 0.2, process)
}

/// [`gain_db`] with an explicit settling time in seconds (high-Q filters at
/// low cutoffs ring for longer).
pub fn gain_db_settled(
    rate: u32,
    hz: f64,
    settle_s: f64,
    mut process: impl FnMut(&mut [f32]),
) -> f64 {
    let fs = f64::from(rate);
    let settle = (settle_s * fs) as usize;
    let cycles = (hz * 0.1).max(50.0).round();
    let measure = (cycles * fs / hz).round() as usize;
    let total = settle + measure;
    let mut buf: Vec<f32> = (0..total)
        .flat_map(|n| {
            let s = (AMP * (2.0 * PI * hz * n as f64 / fs).sin()) as f32;
            [s, s]
        })
        .collect();
    for chunk in buf.chunks_mut(512) {
        process(chunk);
    }
    let (mut s, mut c) = (0.0, 0.0);
    for n in settle..total {
        let y = f64::from(buf[2 * n]);
        let ph = 2.0 * PI * hz * n as f64 / fs;
        s += y * ph.sin();
        c += y * ph.cos();
    }
    let amp = 2.0 * (s * s + c * c).sqrt() / measure as f64;
    20.0 * (amp / AMP).log10()
}
