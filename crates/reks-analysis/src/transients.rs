//! Transients for the grid editor's snap (phase 3 task 18): where the
//! signal's attacks are, in whole ms on reks's timeline (D4), so a marker
//! dragged near a kick lands on it.
//!
//! Same measure as the beat snap ([`crate::beats::snap_to_onset`]): the
//! energy of the first difference (a +6 dB/octave tilt, so attacks count
//! and sustained bass does not) over the next 5 ms against the previous
//! 20 ms, per ms. A transient is a rise of at least 2× that is the largest
//! within ±[`MIN_GAP_MS`] and louder than −60 dBFS. Computed on demand from
//! the audio and cached beside the columns (`transients.json`, keyed like
//! `columns.bin`).

use std::path::{Path, PathBuf};

use reks_audio::{decode_f32, Layout};
use reks_core::{Library, TrackId};
use serde::{Deserialize, Serialize};

/// Energy window after a candidate, ms.
const AFTER_MS: usize = 5;
/// Energy window before a candidate, ms.
const BEFORE_MS: usize = 20;
/// Smallest rise (after / before energy) that counts.
const MIN_RISE: f64 = 2.0;
/// Quietest attack that counts: −60 dBFS mean square of the difference.
const MIN_ENERGY: f64 = 1e-6;
/// Two transients are at least this far apart, ms.
pub const MIN_GAP_MS: usize = 50;
/// A snapped position moves at most this far, ms.
pub const SNAP_WINDOW_MS: i64 = 40;
/// Floor so silence cannot divide by zero (−80 dBFS).
const FLOOR: f64 = 1e-8;

#[derive(Debug, thiserror::Error)]
pub enum TransientsError {
    #[error(transparent)]
    Library(#[from] reks_core::Error),
    #[error(transparent)]
    Audio(#[from] reks_audio::Error),
}

/// Transients of mono `samples` at `rate`, ms from the first sample.
pub fn detect(samples: &[f32], rate: u32) -> Vec<u32> {
    // Mean square of the first difference per ms (bin k: samples from
    // k × rate / 1000, so 44.1 kHz bins stay on the ms).
    let bin_start = |k: usize| k * rate as usize / 1000;
    let mut energy = Vec::new();
    let mut prev = 0.0f64;
    let mut k = 0;
    while bin_start(k) < samples.len() {
        let bin = &samples[bin_start(k)..bin_start(k + 1).min(samples.len())];
        let mut sum = 0.0;
        for &x in bin {
            let d = f64::from(x) - prev;
            prev = f64::from(x);
            sum += d * d;
        }
        energy.push(sum / bin.len().max(1) as f64);
        k += 1;
    }
    let mut prefix = Vec::with_capacity(energy.len() + 1);
    prefix.push(0.0);
    for e in &energy {
        prefix.push(prefix.last().copied().unwrap_or(0.0) + e);
    }
    let mean = |from: usize, to: usize| (prefix[to] - prefix[from]) / (to - from).max(1) as f64;
    let n = energy.len();
    let rise: Vec<f64> = (0..n)
        .map(|k| {
            let after = mean(k, (k + AFTER_MS).min(n));
            if after < MIN_ENERGY {
                return 0.0;
            }
            (after + FLOOR) / (mean(k.saturating_sub(BEFORE_MS), k) + FLOOR)
        })
        .collect();
    let mut out = Vec::new();
    for k in 0..n {
        if rise[k] < MIN_RISE {
            continue;
        }
        let lo = k.saturating_sub(MIN_GAP_MS);
        let hi = (k + MIN_GAP_MS).min(n - 1);
        // The first of equal peaks wins.
        let peak = (lo..=hi).all(|j| rise[j] < rise[k] || (rise[j] == rise[k] && j >= k));
        if peak {
            out.push(k as u32);
        }
    }
    out
}

/// The transient nearest `at_ms` within [`SNAP_WINDOW_MS`], else `at_ms`.
/// `transients` sorted.
pub fn snap(transients: &[u32], at_ms: i64) -> i64 {
    let i = transients.partition_point(|&t| i64::from(t) < at_ms);
    [i.checked_sub(1), Some(i)]
        .into_iter()
        .flatten()
        .filter_map(|j| transients.get(j))
        .map(|&t| i64::from(t))
        .filter(|t| (t - at_ms).abs() <= SNAP_WINDOW_MS)
        .min_by_key(|t| (t - at_ms).abs())
        .unwrap_or(at_ms)
}

#[derive(Serialize, Deserialize)]
struct Cached {
    key: String,
    ms: Vec<u32>,
}

fn cache_path(cache_dir: &Path, id: TrackId) -> PathBuf {
    crate::cache::entry_dir(cache_dir, id).join("transients.json")
}

/// Track `id`'s transients from the cache or, on a miss, from its audio
/// (blocking: decodes the whole file). A failed cache write is logged.
pub fn track_transients(lib: &Library, id: TrackId) -> Result<Vec<u32>, TransientsError> {
    let (path, key) = lib.track_file_key(id)?;
    let file = cache_path(&lib.cache_dir(), id);
    if let Some(c) = std::fs::read(&file)
        .ok()
        .and_then(|b| serde_json::from_slice::<Cached>(&b).ok())
        .filter(|c| c.key == key)
    {
        return Ok(c.ms);
    }
    let pcm = decode_f32(&path, Layout::Mono)?;
    let ms = detect(&pcm.samples, pcm.sample_rate);
    let written = serde_json::to_vec(&Cached {
        key,
        ms: ms.clone(),
    })
    .map_err(std::io::Error::other)
    .and_then(|json| {
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&file, json)
    });
    if let Err(e) = written {
        tracing::warn!(track = id, error = %e, "transients cache not written");
    }
    Ok(ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 10 ms 2 kHz bursts at `at_ms`, over a quiet 100 Hz hum.
    fn clicks(rate: u32, at_ms: &[u32], len_ms: u32) -> Vec<f32> {
        let n = (len_ms * rate / 1000) as usize;
        let mut x: Vec<f32> = (0..n)
            .map(|i| 0.05 * (2.0 * std::f32::consts::PI * 100.0 * i as f32 / rate as f32).sin())
            .collect();
        for &t in at_ms {
            let start = (t * rate / 1000) as usize;
            for k in 0..(rate / 100) as usize {
                let p = 2.0 * std::f32::consts::PI * 2_000.0 * k as f32 / rate as f32;
                x[start + k] += 0.5 * (1.0 - k as f32 * 100.0 / rate as f32) * p.cos();
            }
        }
        x
    }

    #[test]
    fn finds_each_click_to_the_ms() {
        let at = [250, 719, 1_188, 1_656, 2_125];
        let found = detect(&clicks(44_100, &at, 2_500), 44_100);
        assert_eq!(found.len(), at.len(), "{found:?}");
        for (f, a) in found.iter().zip(at) {
            assert!(f.abs_diff(a) <= 1, "{f} vs {a}");
        }
        assert!(detect(&vec![0.0; 44_100], 44_100).is_empty());
        assert!(detect(&[], 48_000).is_empty());
    }

    #[test]
    fn snap_takes_the_nearest_within_the_window() {
        let t = [100, 500, 540];
        assert_eq!(snap(&t, 470), 500);
        assert_eq!(snap(&t, 525), 540);
        assert_eq!(snap(&t, 300), 300);
        assert_eq!(snap(&t, 60), 100);
        assert_eq!(snap(&[], 60), 60);
    }
}
