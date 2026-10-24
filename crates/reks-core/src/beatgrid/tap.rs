//! Tap tempo for the grid editor.

use super::rule::{MAX_BPM_X100, MIN_BPM_X100};

/// A pause longer than this between taps starts a new tap run, ms.
pub const TAP_RESET_MS: i64 = 2_000;
/// Taps (the newest ones) that count towards the tempo.
pub const TAP_WINDOW: usize = 16;

/// Tempo of the newest run of taps (timestamps in ms, oldest first), ×100.
/// A gap over [`TAP_RESET_MS`] starts a new run and only the last
/// [`TAP_WINDOW`] taps count. The tempo is the least-squares slope through
/// the taps, so one early or late tap shifts it less than the mean interval
/// would. `None` with fewer than two taps in the run or a result outside
/// [`MIN_BPM_X100`]..=[`MAX_BPM_X100`].
pub fn tap_tempo(taps_ms: &[i64]) -> Option<i64> {
    let start = taps_ms
        .windows(2)
        .rposition(|w| w[1] - w[0] > TAP_RESET_MS || w[1] <= w[0])
        .map_or(0, |i| i + 1);
    let run = &taps_ms[start.max(taps_ms.len().saturating_sub(TAP_WINDOW))..];
    if run.len() < 2 {
        return None;
    }
    let n = run.len() as f64;
    let t0 = run[0];
    let mean_i = (n - 1.0) / 2.0;
    let mean_t = run.iter().map(|&t| (t - t0) as f64).sum::<f64>() / n;
    let (mut cov, mut var) = (0.0, 0.0);
    for (i, &t) in run.iter().enumerate() {
        let di = i as f64 - mean_i;
        cov += di * ((t - t0) as f64 - mean_t);
        var += di * di;
    }
    let period_ms = cov / var;
    if period_ms <= 0.0 {
        return None;
    }
    let bpm_x100 = (6_000_000.0 / period_ms).round() as i64;
    (MIN_BPM_X100..=MAX_BPM_X100)
        .contains(&bpm_x100)
        .then_some(bpm_x100)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tap_tempo_fits_the_newest_run() {
        // 120 BPM = 500 ms; jitter of ±5 ms costs a fraction of a BPM.
        assert_eq!(tap_tempo(&[1000, 1500, 2000, 2500]), Some(12000));
        let bpm = tap_tempo(&[1000, 1505, 1995, 2502, 3000, 3497]).unwrap();
        assert!((bpm - 12000).abs() <= 50, "{bpm}");
        // A pause resets: only the last run (125 BPM = 480 ms) counts.
        let taps = [0, 300, 600, 10_000, 10_480, 10_960, 11_440];
        assert_eq!(tap_tempo(&taps), Some(12500));
        assert_eq!(tap_tempo(&[5]), None);
        assert_eq!(tap_tempo(&[]), None);
        // Out of range (600 BPM).
        assert_eq!(tap_tempo(&[0, 100, 200]), None);
        // Out-of-order timestamps reset the run.
        assert_eq!(tap_tempo(&[1000, 500]), None);
    }

    #[test]
    fn tap_tempo_uses_a_window() {
        // 40 taps at 100 BPM then 16 at 125 BPM: the window sees only the latter.
        let mut taps: Vec<i64> = (0..40).map(|i| i * 600).collect();
        let last = *taps.last().unwrap();
        taps.extend((1..=16).map(|i| last + i * 480));
        assert_eq!(tap_tempo(&taps), Some(12500));
    }
}
