//! A 4/4 drum pattern with a bar structure, for downbeat tests (phase 3
//! task 2): the click track in the crate root has no accents, so its bar
//! phase is undefined.
//!
//! Every beat has a kick (a 150 → 50 Hz sweep, 120 ms, with a 2 ms beater
//! click); beats 2 and 4 add a
//! noise snare; every off-beat a short hi-hat; and a bass note changes on
//! each downbeat, cycling A2, F2, G2, E2 (110, 87, 98, 82 Hz, integer so the
//! phase stays exact). Mono `f64`, full scale ±1.0, no clipping. Built from
//! basic IEEE operations only, like the rest of this crate, so it is the same
//! on every platform. Not checked in; tests render it.

use crate::{beat_onset_at, sin_turns};

/// Bass notes, one per bar, cycling.
pub const BAR_ROOTS_HZ: [u32; 4] = [110, 87, 98, 82];
/// Beats per bar.
pub const BEATS_PER_BAR: usize = 4;

const KICK_MS: u32 = 120;
const SNARE_MS: u32 = 90;
const HAT_MS: u32 = 25;

/// Renders `bars` bars of the pattern at `rate` Hz and [`crate::BPM`]. Beat `n`
/// starts at [`beat_onset_at`]`(rate, n)`; beats `0, 4, 8, …` are downbeats.
pub fn render_bars(rate: u32, bars: usize) -> Vec<f64> {
    let beats = bars * BEATS_PER_BAR;
    let frames = beat_onset_at(rate, beats);
    let mut mix = vec![0.0f64; frames];

    // Bass: one sustained note per bar, faded in and out over 5 ms.
    let fade = (rate / 200) as usize;
    for bar in 0..bars {
        let start = beat_onset_at(rate, bar * BEATS_PER_BAR);
        let end = beat_onset_at(rate, (bar + 1) * BEATS_PER_BAR);
        let hz = BAR_ROOTS_HZ[bar % BAR_ROOTS_HZ.len()];
        for (k, slot) in mix[start..end].iter_mut().enumerate() {
            let edge = k.min(end - start - 1 - k).min(fade) as f64 / fade as f64;
            let turns = (u64::from(hz) * k as u64 % u64::from(rate)) as f64 / f64::from(rate);
            *slot += 0.2 * edge * sin_turns(turns);
        }
    }

    let mut noise = Lcg(0x5EED_1234);
    for n in 0..beats {
        let onset = beat_onset_at(rate, n);
        let accent = if n % BEATS_PER_BAR == 0 { 0.45 } else { 0.35 };
        add_kick(&mut mix[onset..], rate, accent);
        if n % 2 == 1 {
            add_noise(&mut mix[onset..], rate, SNARE_MS, 0.2, &mut noise);
        }
        let off = (onset + beat_onset_at(rate, n + 1)) / 2;
        add_noise(&mut mix[off..], rate, HAT_MS, 0.06, &mut noise);
    }
    mix
}

/// Downward sine sweep with a squared-linear decay, and a 2 ms 3 kHz
/// beater click on top as real kicks have.
fn add_kick(out: &mut [f64], rate: u32, amplitude: f64) {
    let click = (rate / 500) as usize;
    for (k, slot) in out.iter_mut().take(click).enumerate() {
        let left = (click - k) as f64 / click as f64;
        let turns = (3_000 * k as u64 % u64::from(rate)) as f64 / f64::from(rate);
        *slot += 0.5 * amplitude * left * sin_turns(turns);
    }
    let len = (rate * KICK_MS / 1000) as usize;
    let mut turns = 0.0f64;
    for (k, slot) in out.iter_mut().take(len).enumerate() {
        let left = (len - k) as f64 / len as f64;
        *slot += amplitude * left * left * sin_turns(turns);
        let hz = 50.0 + 100.0 * left * left;
        turns += hz / f64::from(rate);
        if turns >= 1.0 {
            turns -= 1.0;
        }
    }
}

/// White-noise burst with a squared-linear decay.
fn add_noise(out: &mut [f64], rate: u32, ms: u32, amplitude: f64, noise: &mut Lcg) {
    let len = (rate * ms / 1000) as usize;
    for (k, slot) in out.iter_mut().take(len).enumerate() {
        let left = (len - k) as f64 / len as f64;
        *slot += amplitude * left * left * noise.next();
    }
}

/// Numerical Recipes LCG: deterministic noise in `[-1, 1)`.
struct Lcg(u32);

impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        f64::from(self.0) / f64::from(u32::MAX / 2 + 1) - 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length_and_headroom() {
        let mix = render_bars(44_100, 16);
        // 16 bars of 4/4 at 128 BPM are exactly 30 s.
        assert_eq!(mix.len(), 1_323_000);
        let peak = mix.iter().fold(0.0f64, |m, s| m.max(s.abs()));
        assert!((0.5..1.0).contains(&peak), "peak {peak}");
    }

    #[test]
    fn kicks_start_on_the_beat() {
        let mix = render_bars(44_100, 2);
        for n in 0..8 {
            let onset = beat_onset_at(44_100, n);
            let before = mix[onset.saturating_sub(300)..onset.saturating_sub(100)]
                .iter()
                .fold(0.0f64, |m, s| m.max(s.abs()));
            let after = mix[onset..onset + 200]
                .iter()
                .fold(0.0f64, |m, s| m.max(s.abs()));
            assert!(after > before + 0.1, "beat {n}: {before} → {after}");
        }
    }

    #[test]
    fn deterministic() {
        assert_eq!(render_bars(22_050, 1), render_bars(22_050, 1));
    }
}
