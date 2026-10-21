//! Measures an engine's latency from its output instead of trusting its
//! report: a short tone burst goes in at a known frame, and the envelope
//! peak of the output is compared with where the burst belongs.

use std::f32::consts::PI;

/// Input frame the burst is centred on: one second in at 44.1 kHz-ish
/// rates, past every engine's start-up.
pub fn burst_centre(rate: u32) -> usize {
    rate as usize
}

/// Three seconds of stereo silence with a 10 ms Hann-windowed 2 kHz burst
/// centred on [`burst_centre`].
pub fn burst(rate: u32) -> Vec<f32> {
    let frames = rate as usize * 3;
    let len = rate as usize / 100;
    let start = burst_centre(rate) - len / 2;
    let mut out = vec![0.0f32; frames * 2];
    for i in 0..len {
        let w = 0.5 - 0.5 * (2.0 * PI * i as f32 / len as f32).cos();
        let x = 0.5 * w * (2.0 * PI * 2_000.0 * i as f32 / rate as f32).sin();
        out[(start + i) * 2] = x;
        out[(start + i) * 2 + 1] = x;
    }
    out
}

/// Frame of the largest 5 ms RMS window of the left channel, at its centre.
pub fn envelope_peak(samples: &[f32], rate: u32) -> usize {
    let win = (rate as usize / 200).max(1);
    let left: Vec<f32> = samples.iter().step_by(2).map(|x| x * x).collect();
    if left.len() < win {
        return 0;
    }
    let mut sum: f32 = left[..win].iter().sum();
    let (mut best, mut at) = (sum, 0);
    for i in win..left.len() {
        sum += left[i] - left[i - win];
        if sum > best {
            best = sum;
            at = i + 1 - win;
        }
    }
    at + win / 2
}

/// Measured delay in output frames (negative: early) of an untrimmed render
/// of [`burst`] at `ratio`.
pub fn measured_latency(untrimmed: &[f32], rate: u32, ratio: f64) -> f64 {
    let expected = burst_centre(rate) as f64 / ratio;
    envelope_peak(untrimmed, rate) as f64 - expected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_burst_where_it_was_put() {
        let b = burst(44_100);
        let at = envelope_peak(&b, 44_100) as i64;
        assert!((at - burst_centre(44_100) as i64).abs() <= 2, "{at}");
    }

    #[test]
    fn a_delayed_copy_measures_its_delay() {
        let mut d = vec![0.0; 300 * 2];
        d.extend(burst(44_100));
        let m = measured_latency(&d, 44_100, 1.0);
        assert!((m - 300.0).abs() <= 2.0, "{m}");
    }
}
