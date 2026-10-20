//! Beatgrid builder: turns a tracked beat list into a constant or dynamic grid.
//!
//! [`retrieve_const_regions`], [`make_const_bpm`], [`round_bpm_within_range`]
//! and [`adjust_phase`] are a Rust port of Mixxx's `BeatUtils`
//! (`src/track/beatutils.cpp`, functions of the same names), Copyright (C)
//! the Mixxx Development Team, licensed GPL-2.0-or-later and used here under
//! GPL-3.0-or-later (decision D10). Differences from Mixxx:
//!
//! - Positions are `i64` samples, not floating frame positions.
//! - [`round_bpm_within_range`] range-checks every rung of an
//!   integer → 0.5 → 0.1 → 0.01 ladder. Mixxx returns its 1/2 and 2/3
//!   multiplier guesses unchecked and uses a 1/12 BPM rung; PQTZ stores
//!   centi-BPM, so 0.01 is the finest step worth keeping.
//! - [`make_const_bpm`] also reports the span of the merged region, so
//!   [`build_grid`] can anchor beat 1 on a downbeat inside it.
//!
//! [`build_grid`] adds what Mixxx does elsewhere or not at all: the
//! whole-track tempo search ([`fit_tempo`], Mixxx's tempo is one estimate),
//! the bar ([`crate::bar`]), the fallback to a dynamic grid only on a tempo
//! change ([`tempo_change`]), the half/double tempo rule, and the choice of
//! phase by the kick ([`kick_phase`], task 12a).

use crate::bar::{downbeat_line, BarRule, Lines};
use crate::onset::KickPulse;

/// Phase error inaudible in a mix; also the snapping tolerance (Mixxx: 25 ms).
pub const MAX_PHASE_ERROR_S: f64 = 0.025;
/// A region drifting further than this in one direction is not constant.
const MAX_PHASE_ERROR_SUM_S: f64 = 0.1;
/// Beats allowed off the ironed grid inside one region.
const MAX_OUTLIERS: usize = 1;
/// Regions shorter than this are too unstable to extend the main region.
const MIN_REGION_BEATS: usize = 16;
/// [`fit_tempo`]: a beat within this distance of the grid counts as on it.
pub const FIT_TOLERANCE_S: f64 = 0.015;
/// [`fit_tempo`]: tempos searched around each estimate (fraction either way).
const FIT_SPAN: f64 = 0.03;
/// [`fit_tempo`] histogram bin, seconds.
const FIT_BIN_S: f64 = 0.001;
/// [`fit_tempo`] ignores estimates outside this range (BPM): a broken
/// beat list must not make it search thousands of tempos.
const FIT_ESTIMATES: std::ops::RangeInclusive<f64> = 20.0..=720.0;

/// One beat from the tracker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrackedBeat {
    /// Position in samples (per channel) at [`BeatList::sample_rate`].
    pub position: i64,
    /// The tracker thinks this beat starts a bar.
    pub downbeat: bool,
}

/// Input to [`build_grid`]: beats in strictly increasing order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeatList {
    pub sample_rate: u32,
    pub beats: Vec<TrackedBeat>,
}

/// One beat of a dynamic grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridBeat {
    /// Position in samples at the input's sample rate.
    pub position: i64,
    /// 1–4 (PQTZ `beat_number`).
    pub beat_in_bar: u8,
}

/// A beatgrid; positions are samples at the input's sample rate.
#[derive(Debug, Clone, PartialEq)]
pub enum Grid {
    /// One tempo for the whole track.
    Constant {
        /// Snapped per [`round_bpm_within_range`], then octave-folded.
        bpm: f64,
        /// The earliest grid beat at or after sample 0 (or the anchor
        /// downbeat itself when the beats start before 0).
        first_beat: i64,
        /// Beat number (1–4) of `first_beat`; the anchor downbeat is 1.
        first_beat_in_bar: u8,
    },
    /// The tracker's beats, kept because no constant tempo fits them.
    Dynamic { beats: Vec<GridBeat> },
}

/// Tuning knobs; [`Default`] holds the Phase 3 values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridOptions {
    /// Fraction of beats that must sit on the constant grid (within
    /// [`MAX_PHASE_ERROR_S`]) for it to be used.
    pub min_coverage: f64,
    /// A tempo change: a constant region of at least this many beats ...
    pub change_beats: usize,
    /// ... whose tempo differs from the constant grid's by at least this
    /// fraction (and is not a 2:1 or 3:2 tracker error).
    pub change_ratio: f64,
    /// [`kick_phase`] search radius, ms; 0 turns it off.
    pub kick_phase_ms: u32,
    /// [`kick_phase`] moves the grid only for this much more kick attack.
    pub kick_phase_gain: f64,
    /// How a constant grid's beat 1 is chosen ([`downbeat_line`]).
    pub bar: BarRule,
    /// Preferred tempo range; tempos outside it are doubled or halved.
    pub min_bpm: f64,
    pub max_bpm: f64,
}

impl Default for GridOptions {
    fn default() -> Self {
        Self {
            min_coverage: 0.3,
            change_beats: 32,
            change_ratio: 0.005,
            kick_phase_ms: 60,
            kick_phase_gain: 1.5,
            bar: BarRule::VoteKick,
            min_bpm: 70.0,
            max_bpm: 180.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GridError {
    #[error("sample rate is zero")]
    ZeroSampleRate,
    #[error("need at least 2 beats to build a grid, got {0}")]
    TooFewBeats(usize),
    #[error("beat positions are not strictly increasing at beat {index}")]
    UnorderedBeats { index: usize },
}

/// A stretch of beats with one tempo; it ends where the next region starts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConstRegion {
    /// Position of the region's first beat (an input beat).
    pub first_beat: i64,
    /// Mean beat length in samples; 0 for the end marker.
    pub beat_length: f64,
}

/// Result of [`make_const_bpm`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConstTempo {
    /// Snapped tempo, not octave-folded.
    pub bpm: f64,
    /// First beat of the merged constant region.
    pub region_start: i64,
    /// First beat after the merged constant region.
    pub region_end: i64,
}

/// Splits `beats` (sorted sample positions) into constant-tempo regions.
///
/// Greedy from the left: take the longest run whose beats stay within
/// [`MAX_PHASE_ERROR_S`] of a straight line (one outlier allowed, never the
/// first beat), then continue from its last beat. The list ends with a
/// zero-length marker at the last beat. Fewer than 2 beats give no regions.
pub fn retrieve_const_regions(beats: &[i64], sample_rate: u32) -> Vec<ConstRegion> {
    let n = beats.len();
    if n < 2 {
        return Vec::new();
    }
    let sr = f64::from(sample_rate);
    let max_err = MAX_PHASE_ERROR_S * sr;
    let max_err_sum = MAX_PHASE_ERROR_SUM_S * sr;
    let pos = |i: usize| beats[i] as f64;

    let mut regions = Vec::new();
    let mut left = 0;
    let mut right = n - 1;
    while left < n - 1 {
        let mean = (pos(right) - pos(left)) / (right - left) as f64;
        let mut outliers = 0;
        let mut ironed = pos(left);
        let mut err_sum = 0.0;
        let mut i = left + 1;
        while i <= right {
            ironed += mean;
            let err = ironed - pos(i);
            err_sum += err;
            if err.abs() > max_err {
                outliers += 1;
                if outliers > MAX_OUTLIERS || i == left + 1 {
                    break;
                }
            }
            if err_sum.abs() > max_err_sum {
                break;
            }
            i += 1;
        }
        if i > right {
            // First and last beat lengths both off in the same direction would
            // bend the mean away from the true tempo.
            let border_err = if right > left + 2 {
                let first = pos(left + 1) - pos(left);
                let last = pos(right) - pos(right - 1);
                (first + last - 2.0 * mean).abs()
            } else {
                0.0
            };
            if border_err < max_err / 2.0 {
                regions.push(ConstRegion {
                    first_beat: beats[left],
                    beat_length: mean,
                });
                left = right;
                right = n - 1;
                continue;
            }
        }
        // A two-beat run always succeeds above, so `right > left + 1` here.
        right -= 1;
    }
    regions.push(ConstRegion {
        first_beat: beats[n - 1],
        beat_length: 0.0,
    });
    regions
}

fn beats_in(length: f64, beat_length: f64) -> usize {
    (length / beat_length + 0.5) as usize
}

/// Beat-length window that keeps `beats` beats within the phase tolerance.
fn length_window(beat_length: f64, beats: usize, max_err: f64) -> (f64, f64) {
    let slack = max_err / beats.max(1) as f64;
    (beat_length - slack, beat_length + slack)
}

/// Finds the track's constant tempo from [`retrieve_const_regions`] output.
///
/// Starts from the longest region and extends it to the earliest, then the
/// latest, region of at least 16 beats with a compatible tempo and phase.
/// The tempo is snapped with [`round_bpm_within_range`]. `None` when there is
/// no region with a positive length.
pub fn make_const_bpm(regions: &[ConstRegion], sample_rate: u32) -> Option<ConstTempo> {
    let sr = f64::from(sample_rate);
    let max_err = MAX_PHASE_ERROR_S * sr;
    let last = regions.len().checked_sub(1)?;
    let span = |from: usize, to: usize| (regions[to].first_beat - regions[from].first_beat) as f64;

    let mut mid = 0;
    let mut longest_len = 0.0;
    let mut beat_len = 0.0;
    for (i, pair) in regions.windows(2).enumerate() {
        let len = (pair[1].first_beat - pair[0].first_beat) as f64;
        if len > longest_len {
            longest_len = len;
            beat_len = pair[0].beat_length;
            mid = i;
        }
    }
    if longest_len <= 0.0 || beat_len <= 0.0 {
        return None;
    }
    let mut n_beats = beats_in(longest_len, beat_len);
    let (mut len_min, mut len_max) = length_window(beat_len, n_beats, max_err);

    // Tries to merge region `i` into the span `from..to` (region indices,
    // `to` exclusive); returns the merged beat length and count.
    let try_merge =
        |i: usize, from: usize, to: usize, beat_len: f64, len_min: f64, len_max: f64| {
            let this_len = regions[i].beat_length;
            let this_beats = beats_in(span(i, i + 1), this_len);
            if this_beats < MIN_REGION_BEATS {
                return None;
            }
            let (this_min, this_max) = length_window(this_len, this_beats, max_err);
            if !(beat_len > this_min && beat_len < this_max) {
                return None;
            }
            // Same phase: the merged span must hold an unambiguous beat count.
            let merged = span(from, to);
            let most = (merged / len_min.max(this_min)).round();
            let fewest = (merged / len_max.min(this_max)).round();
            if most != fewest || fewest < 1.0 {
                return None;
            }
            let merged_len = merged / fewest;
            (merged_len > len_min && merged_len < len_max).then_some((merged_len, fewest as usize))
        };

    let mut start = mid;
    for i in 0..mid {
        if let Some((len, count)) = try_merge(i, i, mid + 1, beat_len, len_min, len_max) {
            beat_len = len;
            n_beats = count;
            (len_min, len_max) = length_window(beat_len, n_beats, max_err);
            start = i;
            break;
        }
    }
    let mut end = mid;
    for i in (mid + 1..last).rev() {
        if let Some((len, count)) = try_merge(i, start, i + 1, beat_len, len_min, len_max) {
            beat_len = len;
            n_beats = count;
            end = i;
            break;
        }
    }
    (len_min, len_max) = length_window(beat_len, n_beats, max_err);

    let bpm = round_bpm_within_range(
        60.0 * sr / len_max,
        60.0 * sr / beat_len,
        60.0 * sr / len_min,
    );
    Some(ConstTempo {
        bpm,
        region_start: regions[start].first_beat,
        region_end: regions[end + 1].first_beat,
    })
}

/// Snaps `center` to the coarsest step (1, 0.5, 0.1, 0.01 BPM) that stays
/// strictly inside `(min, max)`; returns `center` when none does.
pub fn round_bpm_within_range(min: f64, center: f64, max: f64) -> f64 {
    for per_bpm in [1.0, 2.0, 10.0, 100.0] {
        let snapped = (center * per_bpm).round() / per_bpm;
        if snapped > min && snapped < max {
            return snapped;
        }
    }
    center
}

/// Shifts `first_beat` by the mean phase error of the beats within
/// [`MAX_PHASE_ERROR_S`] of a grid at `bpm` through it.
pub fn adjust_phase(first_beat: f64, bpm: f64, sample_rate: u32, beats: &[i64]) -> f64 {
    adjust_phase_within(
        first_beat,
        bpm,
        sample_rate,
        beats,
        MAX_PHASE_ERROR_S * f64::from(sample_rate),
    )
}

/// [`adjust_phase`] over the beats within `max_err` samples of the grid.
fn adjust_phase_within(
    first_beat: f64,
    bpm: f64,
    sample_rate: u32,
    beats: &[i64],
    max_err: f64,
) -> f64 {
    let sr = f64::from(sample_rate);
    let beat_len = 60.0 * sr / bpm;
    let origin = first_beat.rem_euclid(beat_len);
    let (mut sum, mut count) = (0.0, 0usize);
    for &beat in beats {
        let mut offset = (beat as f64 - origin).rem_euclid(beat_len);
        if offset > beat_len / 2.0 {
            offset -= beat_len;
        }
        if offset.abs() < max_err {
            sum += offset;
            count += 1;
        }
    }
    if count == 0 {
        first_beat
    } else {
        first_beat + sum / count as f64
    }
}

/// Result of [`fit_tempo`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TempoFit {
    /// A multiple of 0.01 BPM.
    pub bpm: f64,
    /// Position of a grid line, samples (`0..beat length`).
    pub phase: f64,
    /// Beats within [`FIT_TOLERANCE_S`] of the grid.
    pub on_grid: usize,
}

/// Whole-track tempo search: of the tempos in 0.01 BPM steps within
/// ±3% of each of `estimates` (BPM, 20–720), the one whose grid, at its best phase,
/// has the most beats within [`FIT_TOLERANCE_S`]; ties go to the roundest
/// tempo (1, 0.5, 0.1, then 0.01 BPM), then to the one nearest an estimate.
///
/// Mixxx's estimate comes from one stretch of steady beats, so a noisy beat
/// list (fills, breakdowns, the tracker hopping onto hi-hats) leaves it a
/// few hundredths of a BPM off, which drifts a beat over a track. Counting
/// beats on the grid over the whole track pins the tempo down to 0.01 BPM
/// (0.01 BPM over 800 beats is 25 ms at 140 BPM).
pub fn fit_tempo(beats: &[i64], sample_rate: u32, estimates: &[f64]) -> Option<TempoFit> {
    let sr = f64::from(sample_rate);
    let mut candidates: Vec<i64> = estimates
        .iter()
        .filter(|e| FIT_ESTIMATES.contains(*e))
        .flat_map(|&e| {
            let lo = (e * (1.0 - FIT_SPAN) * 100.0).ceil() as i64;
            let hi = (e * (1.0 + FIT_SPAN) * 100.0).floor() as i64;
            lo.max(1)..=hi
        })
        .collect();
    candidates.sort_unstable();
    candidates.dedup();
    let roundness = |centi: i64| match centi {
        c if c % 100 == 0 => 0,
        c if c % 50 == 0 => 1,
        c if c % 10 == 0 => 2,
        _ => 3,
    };
    let distance = |centi: i64| {
        estimates
            .iter()
            .map(|e| (centi as f64 / 100.0 - e).abs())
            .fold(f64::INFINITY, f64::min)
    };
    let mut best: Option<(i64, TempoFit)> = None;
    for centi in candidates {
        let bpm = centi as f64 / 100.0;
        let (phase, on_grid) = best_phase(beats, 60.0 * sr / bpm, sr);
        let better = match &best {
            None => true,
            Some((b, f)) => {
                (on_grid, std::cmp::Reverse(roundness(centi)))
                    > (f.on_grid, std::cmp::Reverse(roundness(*b)))
                    || (on_grid == f.on_grid
                        && roundness(centi) == roundness(*b)
                        && distance(centi) < distance(*b))
            }
        };
        if better {
            best = Some((
                centi,
                TempoFit {
                    bpm,
                    phase,
                    on_grid,
                },
            ));
        }
    }
    best.map(|(_, f)| f)
}

/// The grid phase (samples, `0..beat_len`) that puts the most beats within
/// [`FIT_TOLERANCE_S`] of a grid line (the mean of those beats), and that
/// count.
fn best_phase(beats: &[i64], beat_len: f64, sr: f64) -> (f64, usize) {
    let bin = FIT_BIN_S * sr;
    let bins = (beat_len / bin).ceil().max(1.0) as usize;
    let mut hist = vec![0usize; bins];
    for &b in beats {
        let k = ((b as f64).rem_euclid(beat_len) / bin) as usize;
        hist[k.min(bins - 1)] += 1;
    }
    let half = ((FIT_TOLERANCE_S / FIT_BIN_S).round() as usize).min((bins - 1) / 2);
    // Circular window sum of 2 * half + 1 bins centred on each bin.
    let mut sum: usize = (0..=2 * half).map(|i| hist[(bins - half + i) % bins]).sum();
    let mut best = (0, sum);
    for centre in 1..bins {
        sum += hist[(centre + half) % bins];
        sum -= hist[(centre + bins - half - 1) % bins];
        if sum > best.1 {
            best = (centre, sum);
        }
    }
    // The window's beats, averaged: the middle of a plateau of equal sums.
    let centre = (best.0 as f64 + 0.5) * bin;
    let reach = (half as f64 + 0.5) * bin;
    let (mut sum_off, mut n) = (0.0, 0usize);
    for &b in beats {
        let mut off = (b as f64 - centre).rem_euclid(beat_len);
        if off > beat_len / 2.0 {
            off -= beat_len;
        }
        if off.abs() <= reach {
            sum_off += off;
            n += 1;
        }
    }
    let phase = if n == 0 {
        centre
    } else {
        (centre + sum_off / n as f64).rem_euclid(beat_len)
    };
    (phase, best.1)
}

/// Two clusters of beats closer than this are one ([`beat_clusters`]).
const CLUSTER_MS: f64 = 15.0;
/// A cluster needs this share of the beats to be a candidate phase.
const CLUSTER_MIN_SHARE: f64 = 0.05;
/// [`kick_phase`] scores a phase by the best kick attack within this many
/// ms of it: a pulse frame is 5 ms, and the attack sits a few ms from the
/// beat rekordbox marks.
const KICK_SLACK_MS: i64 = 5;

/// Clusters of beats around a grid of `beat_len` samples: (phase, beats),
/// the phase being the mean of the beats within half [`CLUSTER_MS`] of the
/// densest point; clusters are at least [`CLUSTER_MS`] apart and hold at
/// least [`CLUSTER_MIN_SHARE`] of the beats.
fn beat_clusters(beats: &[i64], beat_len: f64, sr: f64) -> Vec<(f64, usize)> {
    let bin = FIT_BIN_S * sr;
    let bins = (beat_len / bin).ceil().max(1.0) as usize;
    let mut hist = vec![0usize; bins];
    for &b in beats {
        let k = ((b as f64).rem_euclid(beat_len) / bin) as usize;
        hist[k.min(bins - 1)] += 1;
    }
    let half = ((CLUSTER_MS / 2.0 / 1000.0 / FIT_BIN_S) as usize).min((bins - 1) / 2);
    let window: Vec<usize> = (0..bins)
        .map(|c| {
            (0..=2 * half)
                .map(|i| hist[(c + bins - half + i) % bins])
                .sum()
        })
        .collect();
    let min_count = ((beats.len() as f64 * CLUSTER_MIN_SHARE).ceil() as usize).max(2);
    let mut peaks: Vec<(usize, usize)> = Vec::new();
    for c in 0..bins {
        let w = window[c];
        if w < min_count {
            continue;
        }
        // A peak: no neighbour within the cluster distance is higher, and
        // of equal ones the first.
        let reach = 2 * half;
        let is_peak = (1..=reach.min(bins / 2)).all(|d| {
            let (l, r) = (window[(c + bins - d) % bins], window[(c + d) % bins]);
            w > l && w >= r
        });
        if is_peak {
            peaks.push((c, w));
        }
    }
    peaks
        .into_iter()
        .map(|(c, count)| {
            let centre = (c as f64 + 0.5) * bin;
            let reach = (half as f64 + 0.5) * bin;
            let (mut sum, mut n) = (0.0, 0usize);
            for &b in beats {
                let mut off = (b as f64 - centre).rem_euclid(beat_len);
                if off > beat_len / 2.0 {
                    off -= beat_len;
                }
                if off.abs() <= reach {
                    sum += off;
                    n += 1;
                }
            }
            let phase = centre + if n > 0 { sum / n as f64 } else { 0.0 };
            (phase.rem_euclid(beat_len), count)
        })
        .collect()
}

/// Picks the grid's phase among the clusters of beats ([`beat_clusters`])
/// by the kick: a constant grid (`origin`, `bpm`) moves to another cluster
/// within `radius_ms` when that cluster's lines, over the whole track, sit
/// on at least `gain` times as much kick attack ([`KickPulse::at`]) as the
/// cluster the grid is on. The phase always comes from the beats; the kick
/// only chooses (a hi-hat or bass note the tracker took for the beat, or an
/// average of two clusters a few ms apart). Unchanged otherwise.
pub fn kick_phase(
    origin: f64,
    bpm: f64,
    sample_rate: u32,
    beats: &[i64],
    pulse: &KickPulse,
    radius_ms: u32,
    gain: f64,
) -> f64 {
    let sr = f64::from(sample_rate);
    let beat_len = 60.0 * sr / bpm;
    let ms = sr / 1000.0;
    let end = pulse.len_samples();
    let line_sum = |o: f64| -> f64 {
        let mut line = o.rem_euclid(beat_len);
        let mut sum = 0.0;
        while line < end {
            sum += f64::from(pulse.at(line));
            line += beat_len;
        }
        sum
    };
    let score = |o: f64| {
        (-KICK_SLACK_MS..=KICK_SLACK_MS)
            .map(|d| line_sum(o + d as f64 * ms))
            .fold(0.0, f64::max)
    };
    // Signed distance from the grid to `phase`, samples.
    let from_origin = |phase: f64| {
        let d = (phase - origin).rem_euclid(beat_len);
        if d > beat_len / 2.0 {
            d - beat_len
        } else {
            d
        }
    };
    let clusters: Vec<(f64, f64)> = beat_clusters(beats, beat_len, sr)
        .into_iter()
        .map(|(phase, _)| (from_origin(phase), phase))
        .filter(|(d, _)| d.abs() <= f64::from(radius_ms) * ms)
        .collect();
    // The cluster the grid is on: the nearest within the cluster distance.
    let current = clusters
        .iter()
        .filter(|(d, _)| d.abs() < CLUSTER_MS * ms)
        .min_by(|a, b| a.0.abs().total_cmp(&b.0.abs()));
    let here = current.map_or_else(|| score(origin), |&(_, p)| score(p));
    let best = clusters
        .iter()
        .filter(|(d, _)| current.is_none_or(|c| (d - c.0).abs() >= CLUSTER_MS * ms))
        .map(|&(d, p)| (d, p, score(p)))
        .filter(|&(_, _, s)| s > 0.0 && s >= here * gain)
        .max_by(|a, b| a.2.total_cmp(&b.2).then(b.0.abs().total_cmp(&a.0.abs())));
    match best {
        Some((d, _, _)) => origin + d,
        None => origin,
    }
}

/// Tempo of the median gap between beats, BPM.
fn median_gap_bpm(beats: &[i64], sample_rate: u32) -> Option<f64> {
    let mut gaps: Vec<i64> = beats.windows(2).map(|w| w[1] - w[0]).collect();
    if gaps.is_empty() {
        return None;
    }
    let mid = gaps.len() / 2;
    let gap = *gaps.select_nth_unstable(mid).1;
    (gap > 0).then(|| 60.0 * f64::from(sample_rate) / gap as f64)
}

/// Builds the grid for a tracked beat list.
///
/// The constant tempo is [`fit_tempo`] around [`make_const_bpm`]'s tempo
/// and the median beat gap. The grid is constant when it puts at least
/// `opts.min_coverage` of all beats within [`MAX_PHASE_ERROR_S`] of a grid
/// line and the beats show no tempo change ([`tempo_change`]); beat 1 is
/// then chosen by `opts.bar` ([`downbeat_line`]), falling back to the
/// flagged downbeat nearest the start of [`make_const_bpm`]'s merged
/// constant region (its first beat when no downbeat is flagged).
/// Otherwise dynamic, numbered from the downbeat flags. Either way a tempo
/// outside `opts.min_bpm..=opts.max_bpm` is doubled or halved into it.
pub fn build_grid(input: &BeatList, opts: &GridOptions) -> Result<Grid, GridError> {
    build_grid_with_pulse(input, None, opts)
}

/// [`build_grid`], then a constant grid's phase is lined up with the kicks
/// ([`kick_phase`]) when `pulse` is given and `opts.kick_phase_ms > 0`; the
/// pulse also places the bar ([`BarRule::VoteKick`]).
pub fn build_grid_with_pulse(
    input: &BeatList,
    pulse: Option<&KickPulse>,
    opts: &GridOptions,
) -> Result<Grid, GridError> {
    if input.sample_rate == 0 {
        return Err(GridError::ZeroSampleRate);
    }
    if input.beats.len() < 2 {
        return Err(GridError::TooFewBeats(input.beats.len()));
    }
    if let Some(index) = input
        .beats
        .windows(2)
        .position(|w| w[1].position <= w[0].position)
    {
        return Err(GridError::UnorderedBeats { index: index + 1 });
    }

    let sr = f64::from(input.sample_rate);
    let positions: Vec<i64> = input.beats.iter().map(|b| b.position).collect();
    let regions = retrieve_const_regions(&positions, input.sample_rate);
    let mixxx = make_const_bpm(&regions, input.sample_rate);
    let estimates: Vec<f64> = mixxx
        .map(|t| t.bpm)
        .into_iter()
        .chain(median_gap_bpm(&positions, input.sample_rate))
        .collect();
    if let Some(fit) = fit_tempo(&positions, input.sample_rate, &estimates) {
        let (region_start, region_end) = mixxx
            .map_or((positions[0], positions[positions.len() - 1]), |t| {
                (t.region_start, t.region_end)
            });
        let tempo = ConstTempo {
            bpm: fit.bpm,
            region_start,
            region_end,
        };
        let beat_len = 60.0 * sr / tempo.bpm;
        // Phase from the steady region (a noisy track's beats can cluster
        // off the beat too); the whole-track best phase without one.
        let start = mixxx.map_or(fit.phase, |t| t.region_start as f64);
        let origin = adjust_phase(start, tempo.bpm, input.sample_rate, &positions);
        let origin = match pulse {
            Some(p) if opts.kick_phase_ms > 0 => kick_phase(
                origin,
                tempo.bpm,
                input.sample_rate,
                &positions,
                p,
                opts.kick_phase_ms,
                opts.kick_phase_gain,
            ),
            _ => origin,
        };
        // How steady the beat is, at the tempo's best phase.
        let best = adjust_phase(fit.phase, tempo.bpm, input.sample_rate, &positions);
        let coverage = on_grid_fraction(&positions, best, beat_len, MAX_PHASE_ERROR_S * sr);
        let change = tempo_change(&regions, beat_len, input.sample_rate, opts);
        tracing::debug!(bpm = tempo.bpm, coverage, change, "constant grid candidate");
        if coverage >= opts.min_coverage && !change {
            let fallback = nearest_downbeat(&input.beats, tempo.region_start, tempo.region_end)
                .unwrap_or(tempo.region_start);
            let downbeats: Vec<i64> = input
                .beats
                .iter()
                .filter(|b| b.downbeat)
                .map(|b| b.position)
                .collect();
            let anchor = downbeat_line(
                Lines { origin, beat_len },
                &downbeats,
                pulse,
                fallback as f64,
                opts.bar,
            );

            let bpm = tempo.bpm * octave_factor(tempo.bpm, opts);
            let beat_len = 60.0 * sr / bpm;
            let back = (anchor / beat_len).floor().max(0.0);
            let first_beat = anchor - back * beat_len;
            return Ok(Grid::Constant {
                bpm,
                first_beat: first_beat.round() as i64,
                first_beat_in_bar: beat_in_bar(-(back as i64)),
            });
        }
    }
    Ok(dynamic_grid(&input.beats, input.sample_rate, opts))
}

/// Ratios of tracked to grid beat length that are tracker errors (half or
/// double time, triplet feel), not tempo changes.
const TRACKER_RATIOS: [f64; 4] = [0.5, 2.0, 2.0 / 3.0, 1.5];

/// Evidence of a real tempo change against a constant grid of `beat_len`
/// samples: a region from [`retrieve_const_regions`] of at least
/// `opts.change_beats` beats whose tempo differs by at least
/// `opts.change_ratio` and drifts off the grid by more than twice
/// [`MAX_PHASE_ERROR_S`] over its length. Noisy beats break into short
/// regions or ones close to the grid's tempo; a track that really changes
/// tempo holds the other tempo for many bars.
pub fn tempo_change(
    regions: &[ConstRegion],
    beat_len: f64,
    sample_rate: u32,
    opts: &GridOptions,
) -> bool {
    let max_drift = 2.0 * MAX_PHASE_ERROR_S * f64::from(sample_rate);
    regions.windows(2).any(|pair| {
        let len = pair[0].beat_length;
        if len <= 0.0 {
            return false;
        }
        let beats = beats_in((pair[1].first_beat - pair[0].first_beat) as f64, len);
        let ratio = len / beat_len;
        beats >= opts.change_beats
            && (ratio - 1.0).abs() >= opts.change_ratio
            && (len - beat_len).abs() * beats as f64 > max_drift
            && !TRACKER_RATIOS
                .iter()
                .any(|r| (ratio / r - 1.0).abs() < opts.change_ratio)
    })
}

/// Beat number (1–4) of the beat `index` beats after a downbeat.
fn beat_in_bar(index: i64) -> u8 {
    // rem_euclid(4) is 0..=3, so the cast cannot truncate.
    (index.rem_euclid(4) + 1) as u8
}

fn on_grid_fraction(beats: &[i64], origin: f64, beat_len: f64, max_err: f64) -> f64 {
    let on = beats
        .iter()
        .filter(|&&b| {
            let offset = (b as f64 - origin).rem_euclid(beat_len);
            offset.min(beat_len - offset) <= max_err
        })
        .count();
    on as f64 / beats.len().max(1) as f64
}

/// Flagged downbeat nearest `start`, preferring those inside `start..=end`.
fn nearest_downbeat(beats: &[TrackedBeat], start: i64, end: i64) -> Option<i64> {
    let downbeats = || beats.iter().filter(|b| b.downbeat).map(|b| b.position);
    let distance = |p: &i64| p.abs_diff(start);
    downbeats()
        .filter(|&p| p >= start && p <= end)
        .min_by_key(distance)
        .or_else(|| downbeats().min_by_key(distance))
}

/// Power of two that moves `bpm` into the preferred range; 1 when it is in
/// range or cannot be moved without leaving the range on the other side.
fn octave_factor(bpm: f64, opts: &GridOptions) -> f64 {
    let (mut bpm, mut factor) = (bpm, 1.0);
    while bpm < opts.min_bpm && bpm * 2.0 <= opts.max_bpm {
        bpm *= 2.0;
        factor *= 2.0;
    }
    while bpm > opts.max_bpm && bpm / 2.0 >= opts.min_bpm {
        bpm /= 2.0;
        factor /= 2.0;
    }
    factor
}

fn dynamic_grid(beats: &[TrackedBeat], sample_rate: u32, opts: &GridOptions) -> Grid {
    let mut intervals: Vec<i64> = beats
        .windows(2)
        .map(|w| w[1].position - w[0].position)
        .collect();
    let mid = intervals.len() / 2;
    let median = intervals.select_nth_unstable(mid).1;
    let factor = octave_factor(60.0 * f64::from(sample_rate) / *median as f64, opts);

    let folded = if factor > 1.0 {
        double_beats(beats, factor as usize)
    } else if factor < 1.0 {
        halve_beats(beats, (1.0 / factor) as usize)
    } else {
        beats.to_vec()
    };
    Grid::Dynamic {
        beats: number_beats(&folded),
    }
}

/// Inserts `factor - 1` evenly spaced beats between neighbours.
fn double_beats(beats: &[TrackedBeat], factor: usize) -> Vec<TrackedBeat> {
    let mut out = Vec::with_capacity(beats.len() * factor);
    for w in beats.windows(2) {
        out.push(w[0]);
        let step = (w[1].position - w[0].position) as f64 / factor as f64;
        out.extend((1..factor).map(|k| TrackedBeat {
            position: w[0].position + (step * k as f64).round() as i64,
            downbeat: false,
        }));
    }
    out.extend(beats.last().copied());
    out
}

/// Keeps every `step`-th beat, at the phase holding the most downbeats.
fn halve_beats(beats: &[TrackedBeat], step: usize) -> Vec<TrackedBeat> {
    let step = step.max(1);
    let phase = (0..step)
        .max_by_key(|&p| {
            (
                beats
                    .iter()
                    .skip(p)
                    .step_by(step)
                    .filter(|b| b.downbeat)
                    .count(),
                usize::MAX - p,
            )
        })
        .unwrap_or(0);
    beats.iter().skip(phase).step_by(step).copied().collect()
}

/// Beat 1 at every flagged downbeat, counting 1–4 in between; beats before
/// the first downbeat count back from it. No flags: the first beat is 1.
fn number_beats(beats: &[TrackedBeat]) -> Vec<GridBeat> {
    let first_downbeat = beats.iter().position(|b| b.downbeat).unwrap_or(0);
    let mut out = Vec::with_capacity(beats.len());
    let mut current = 1u8;
    for (i, beat) in beats.iter().enumerate() {
        current = if i < first_downbeat {
            beat_in_bar(i as i64 - first_downbeat as i64)
        } else if beat.downbeat || i == first_downbeat {
            1
        } else {
            current % 4 + 1
        };
        out.push(GridBeat {
            position: beat.position,
            beat_in_bar: current,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::onset::PULSE_RATE;

    const SR: u32 = 44_100;

    /// One constant-tempo stretch of a synthetic track.
    struct Segment {
        bpm: f64,
        /// Seconds; the segment covers `[from, to)`.
        from: f64,
        to: f64,
    }

    /// Synthetic beats: piecewise-constant tempo, uniform jitter of
    /// ±`jitter_ms` (deterministic), a downbeat every `bar` beats starting
    /// at beat `downbeat_phase` (`bar == 0`: no downbeats).
    fn synth(segments: &[Segment], jitter_ms: f64, bar: usize, downbeat_phase: usize) -> BeatList {
        let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
        let mut beats = Vec::new();
        for seg in segments {
            let mut t = seg.from;
            while t < seg.to - 1e-9 {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                let unit = (seed >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0;
                let i = beats.len();
                beats.push(TrackedBeat {
                    position: ((t + unit * jitter_ms / 1000.0) * f64::from(SR)).round() as i64,
                    downbeat: bar > 0
                        && i >= downbeat_phase
                        && (i - downbeat_phase).is_multiple_of(bar),
                });
                t += 60.0 / seg.bpm;
            }
        }
        BeatList {
            sample_rate: SR,
            beats,
        }
    }

    fn one(bpm: f64, from: f64, to: f64) -> Vec<Segment> {
        vec![Segment { bpm, from, to }]
    }

    fn secs(s: f64) -> i64 {
        (s * f64::from(SR)).round() as i64
    }

    /// Beat number the constant grid gives the grid line nearest `pos`.
    fn constant_beat_in_bar(grid: &Grid, pos: i64) -> u8 {
        let Grid::Constant {
            bpm,
            first_beat,
            first_beat_in_bar,
        } = grid
        else {
            panic!("expected a constant grid, got {grid:?}");
        };
        let beat_len = 60.0 * f64::from(SR) / bpm;
        let index = ((pos - first_beat) as f64 / beat_len).round() as i64;
        beat_in_bar(index + i64::from(*first_beat_in_bar) - 1)
    }

    #[test]
    fn round_bpm_ladder() {
        let cases = [
            // (min, center, max, expected)
            (119.9, 120.03, 120.1, 120.0),
            (127.3, 127.48, 127.7, 127.5),
            (123.37, 123.41, 123.46, 123.4),
            (123.448, 123.452, 123.458, 123.45),
            (123.4561, 123.4563, 123.4566, 123.4563),
            (87.1, 87.4, 87.6, 87.5),
        ];
        for (min, center, max, want) in cases {
            let got = round_bpm_within_range(min, center, max);
            assert!(
                (got - want).abs() < 1e-9,
                "{min} {center} {max}: got {got}, want {want}"
            );
        }
    }

    #[test]
    fn const_regions() {
        let beat = 60.0 * f64::from(SR) / 120.0;
        let exact: Vec<i64> = (0..64).map(|i| (i as f64 * beat).round() as i64).collect();
        let mut outlier = exact.clone();
        outlier[30] += secs(0.04);
        let mut shifted = exact.clone();
        for p in &mut shifted[32..] {
            *p += secs(0.05);
        }
        let cases: [(&str, Vec<i64>, usize); 5] = [
            ("empty", vec![], 0),
            ("one beat", vec![1000], 0),
            ("exact tempo", exact, 2),
            ("one outlier is tolerated", outlier, 2),
            // Two long regions with a two-beat bridge across the shift.
            ("phase shift splits", shifted, 4),
        ];
        for (name, beats, want) in cases {
            let regions = retrieve_const_regions(&beats, SR);
            assert_eq!(regions.len(), want, "{name}: {regions:?}");
            if let Some(end) = regions.last() {
                assert_eq!(end.beat_length, 0.0, "{name}: end marker");
                assert_eq!(Some(&end.first_beat), beats.last(), "{name}: end marker");
            }
        }
    }

    #[test]
    fn constant_grids() {
        // (name, beats, bpm, first_beat_s, first_beat_in_bar)
        let cases: Vec<(&str, BeatList, f64, f64, u8)> = vec![
            (
                "exact 128",
                synth(&one(128.0, 0.2, 240.0), 0.0, 4, 0),
                128.0,
                0.2,
                1,
            ),
            (
                "jitter ±10 ms",
                synth(&one(124.0, 0.2, 300.0), 10.0, 4, 0),
                124.0,
                0.2,
                1,
            ),
            (
                "fractional tempo with jitter",
                synth(&one(123.4, 0.1, 300.0), 10.0, 4, 0),
                123.4,
                0.1,
                1,
            ),
            (
                "half-integer tempo",
                synth(&one(87.5, 0.3, 240.0), 5.0, 4, 0),
                87.5,
                0.3,
                1,
            ),
            (
                "downbeat on the third beat",
                synth(&one(120.0, 0.3, 200.0), 0.0, 4, 2),
                120.0,
                0.3,
                3,
            ),
            (
                "no downbeats: first beat is 1",
                synth(&one(120.0, 0.3, 200.0), 0.0, 0, 0),
                120.0,
                0.3,
                1,
            ),
            // Beats start after a beatless intro; the grid reaches back to it.
            (
                "grid extends into the intro",
                synth(&one(120.0, 10.2, 200.0), 0.0, 4, 1),
                120.0,
                0.2,
                4,
            ),
            // In range on both octaves: kept as tracked.
            (
                "174 stays (drum and bass)",
                synth(&one(174.0, 0.1, 200.0), 0.0, 4, 0),
                174.0,
                0.1,
                1,
            ),
            (
                "95 stays",
                synth(&one(95.0, 0.1, 200.0), 0.0, 4, 0),
                95.0,
                0.1,
                1,
            ),
            // Tracker locked to half or double time: folded into 70–180.
            (
                "half time doubled",
                synth(&one(64.0, 0.2, 240.0), 5.0, 2, 0),
                128.0,
                0.2,
                1,
            ),
            (
                "double time halved",
                synth(&one(256.0, 0.2, 240.0), 5.0, 8, 0),
                128.0,
                0.2,
                1,
            ),
        ];
        for (name, input, bpm, first_s, bib) in cases {
            let grid = build_grid(&input, &GridOptions::default()).unwrap();
            let Grid::Constant {
                bpm: got_bpm,
                first_beat,
                first_beat_in_bar,
            } = grid
            else {
                panic!("{name}: expected constant, got {grid:?}");
            };
            assert!((got_bpm - bpm).abs() < 1e-9, "{name}: bpm {got_bpm}");
            assert!(
                (first_beat - secs(first_s)).abs() <= secs(0.005),
                "{name}: first beat {first_beat}, want {}",
                secs(first_s)
            );
            assert_eq!(first_beat_in_bar, bib, "{name}");
        }
    }

    #[test]
    fn missing_beats_still_give_one_constant_grid() {
        let mut input = synth(&one(126.0, 0.4, 300.0), 3.0, 4, 0);
        input.beats.drain(200..206);
        let grid = build_grid(&input, &GridOptions::default()).unwrap();
        assert!(
            matches!(grid, Grid::Constant { bpm, .. } if bpm == 126.0),
            "{grid:?}"
        );
    }

    #[test]
    fn beat_one_comes_from_the_longest_region() {
        // A loose intro whose downbeat flags are a beat off, then the body.
        let mut intro = synth(&one(120.0, 0.0, 12.0), 40.0, 4, 1).beats;
        let body = synth(&one(120.0, 12.0, 240.0), 0.0, 4, 0).beats;
        let body_downbeat = body[0].position;
        intro.extend(body);
        let input = BeatList {
            sample_rate: SR,
            beats: intro,
        };
        let grid = build_grid(&input, &GridOptions::default()).unwrap();
        assert_eq!(constant_beat_in_bar(&grid, body_downbeat), 1, "{grid:?}");
    }

    #[test]
    fn dynamic_grids() {
        // (name, beats, expected beat count)
        let change = vec![
            Segment {
                bpm: 128.0,
                from: 0.5,
                to: 120.0,
            },
            Segment {
                bpm: 132.0,
                from: 120.0,
                to: 240.0,
            },
        ];
        let change_input = synth(&change, 5.0, 4, 0);
        let n = change_input.beats.len();
        let half = synth(
            &[
                Segment {
                    bpm: 60.0,
                    from: 0.5,
                    to: 120.0,
                },
                Segment {
                    bpm: 64.0,
                    from: 120.0,
                    to: 240.0,
                },
            ],
            0.0,
            2,
            0,
        );
        let half_n = half.beats.len();
        let double = synth(
            &[
                Segment {
                    bpm: 250.0,
                    from: 0.5,
                    to: 120.0,
                },
                Segment {
                    bpm: 262.0,
                    from: 120.0,
                    to: 240.0,
                },
            ],
            0.0,
            8,
            0,
        );
        let double_n = double.beats.len();
        let drift = BeatList {
            sample_rate: SR,
            // 120 → 130 BPM over six minutes, like a live drummer.
            beats: {
                let mut t = 0.5;
                let mut beats = Vec::new();
                while t < 360.0 {
                    beats.push(TrackedBeat {
                        position: secs(t),
                        downbeat: beats.len().is_multiple_of(4),
                    });
                    t += 60.0 / (120.0 + 10.0 * t / 360.0);
                }
                beats
            },
        };
        let drift_n = drift.beats.len();
        let cases = [
            ("tempo change at 2:00", change_input, n),
            ("half time doubled", half, half_n * 2 - 1),
            ("double time halved", double, double_n.div_ceil(2)),
            ("slow drift", drift, drift_n),
        ];
        for (name, input, want) in cases {
            let grid = build_grid(&input, &GridOptions::default()).unwrap();
            let Grid::Dynamic { beats } = grid else {
                panic!("{name}: expected dynamic, got {grid:?}");
            };
            assert_eq!(beats.len(), want, "{name}");
            // Input downbeats stay beat 1 and the count runs 1–4.
            assert_eq!(beats[0].beat_in_bar, 1, "{name}");
            for w in beats.windows(2) {
                assert!(w[1].position > w[0].position, "{name}: order");
                assert!(
                    w[1].beat_in_bar == 1 || w[1].beat_in_bar == w[0].beat_in_bar + 1,
                    "{name}: count"
                );
            }
        }
    }

    /// `input` with every beat whose index `i` satisfies `pick(i)` moved by
    /// `offset_ms` (a hi-hat or bass note the tracker took for the beat).
    fn displace(mut input: BeatList, offset_ms: f64, pick: impl Fn(usize) -> bool) -> BeatList {
        for (i, b) in input.beats.iter_mut().enumerate() {
            if pick(i) {
                b.position += secs(offset_ms / 1000.0);
            }
        }
        input
    }

    #[test]
    fn noisy_steady_tracks_stay_constant() {
        // (name, beats): all at 143 BPM; most used to fall back to dynamic.
        let base = || synth(&one(143.0, 0.3, 360.0), 4.0, 4, 0);
        let cases = [
            // Half the beats 45 ms late in runs of 8: no constant region is
            // long, coverage ≈ 0.5 (the old 0.8 floor went dynamic).
            ("runs off the beat", displace(base(), 45.0, |i| i % 16 >= 8)),
            // Every third beat on an off-beat hi-hat.
            ("hi-hats", displace(base(), 105.0, |i| i % 3 == 1)),
            // A 64-beat breakdown tracked in half time is a tracker error.
            ("half-time breakdown", {
                let mut b = base();
                let keep: Vec<TrackedBeat> = b
                    .beats
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| !(300..364).contains(i) || i % 2 == 0)
                    .map(|(_, b)| *b)
                    .collect();
                b.beats = keep;
                b
            }),
        ];
        for (name, input) in cases {
            let grid = build_grid(&input, &GridOptions::default()).unwrap();
            assert!(
                matches!(grid, Grid::Constant { bpm, .. } if bpm == 143.0),
                "{name}: {grid:?}"
            );
            let old = GridOptions {
                min_coverage: 0.8,
                ..GridOptions::default()
            };
            if name == "runs off the beat" {
                assert!(matches!(
                    build_grid(&input, &old).unwrap(),
                    Grid::Dynamic { .. }
                ));
            }
        }
    }

    #[test]
    fn a_real_tempo_step_is_dynamic() {
        // 3 minutes at 128, then 64 beats (30 s) at 130: 85% of the beats
        // sit on the 128 grid, but the 130 section holds for 16 bars.
        let step = vec![
            Segment {
                bpm: 128.0,
                from: 0.5,
                to: 180.5,
            },
            Segment {
                bpm: 130.0,
                from: 180.5,
                to: 180.5 + 64.0 * 60.0 / 130.0,
            },
        ];
        let input = synth(&step, 3.0, 4, 0);
        let grid = build_grid(&input, &GridOptions::default()).unwrap();
        assert!(matches!(grid, Grid::Dynamic { .. }), "{grid:?}");
        // The same change for only 16 beats is not evidence.
        let short = vec![
            Segment {
                bpm: 128.0,
                from: 0.5,
                to: 180.5,
            },
            Segment {
                bpm: 130.0,
                from: 180.5,
                to: 180.5 + 16.0 * 60.0 / 130.0,
            },
        ];
        let grid = build_grid(&synth(&short, 3.0, 4, 0), &GridOptions::default()).unwrap();
        assert!(
            matches!(grid, Grid::Constant { bpm, .. } if bpm == 128.0),
            "{grid:?}"
        );
    }

    #[test]
    fn fit_tempo_finds_the_exact_tempo() {
        let sr = f64::from(SR);
        // 137 BPM for 6 minutes, ±6 ms jitter, a third of the beats 40 ms off.
        let input = displace(synth(&one(137.0, 0.25, 360.0), 6.0, 4, 0), 40.0, |i| {
            i % 3 == 0
        });
        let beats: Vec<i64> = input.beats.iter().map(|b| b.position).collect();
        // Estimates a little off either way still land on 137.00.
        for estimate in [136.6, 137.04, 137.3] {
            let fit = fit_tempo(&beats, SR, &[estimate]).unwrap();
            assert_eq!(fit.bpm, 137.0, "from {estimate}");
            assert!(fit.on_grid >= beats.len() * 2 / 3 - 2, "{fit:?}");
            // Phase: a grid line within 6 ms of 0.25 s.
            let len = 60.0 * sr / 137.0;
            let d = (0.25 * sr - fit.phase).rem_euclid(len);
            assert!(d.min(len - d) < 0.006 * sr, "{fit:?}");
        }
        // A fractional tempo is kept to 0.01 BPM.
        let input = synth(&one(126.37, 0.1, 360.0), 3.0, 4, 0);
        let beats: Vec<i64> = input.beats.iter().map(|b| b.position).collect();
        assert_eq!(fit_tempo(&beats, SR, &[126.2]).unwrap().bpm, 126.37);
        assert_eq!(fit_tempo(&beats, SR, &[]), None);
    }

    #[test]
    fn kick_phase_moves_the_grid_onto_the_kicks() {
        // 140 BPM; the tracker took a hi-hat `hat_ms` after the kick for 60%
        // of the beats (in runs of 24), so the grid lands on the hats.
        let kicks = synth(&one(140.0, 0.5, 300.0), 0.0, 4, 0);
        let first_line = |g: &Grid| match g {
            Grid::Constant {
                bpm, first_beat, ..
            } => (*bpm, *first_beat),
            other => panic!("expected constant, got {other:?}"),
        };
        let len = 60.0 * f64::from(SR) / 140.0;
        let off_kick = |first: i64| {
            let d = (first - kicks.beats[0].position) as f64;
            let d = d.rem_euclid(len);
            d.min(len - d) / f64::from(SR) * 1000.0
        };
        // A rise curve with a 12 dB attack at every kick (5 ms frames).
        let frames = (300.5 * f64::from(PULSE_RATE)) as usize;
        let mut q = vec![0u8; frames];
        for b in &kicks.beats {
            let f = (b.position as f64 * f64::from(PULSE_RATE) / f64::from(SR)) as usize;
            for (k, slot) in q.iter_mut().skip(f).take(5).enumerate() {
                *slot = (48 - 8 * k) as u8;
            }
        }
        let pulse = KickPulse::from_quarter_db(q);
        let opts = GridOptions::default();
        let off = GridOptions {
            kick_phase_ms: 0,
            ..opts
        };
        for hat_ms in [45.0, 20.0] {
            let input = displace(kicks.clone(), hat_ms, |i| i % 40 < 24);
            // Without the kicks the grid is on the hats (45 ms) or between
            // them and the kicks (20 ms, averaged).
            let (bpm, first) = first_line(&build_grid(&input, &opts).unwrap());
            assert_eq!(bpm, 140.0);
            assert!(off_kick(first) > 10.0, "{hat_ms}: {}", off_kick(first));
            let grid = build_grid_with_pulse(&input, Some(&pulse), &opts).unwrap();
            let (bpm, first) = first_line(&grid);
            assert_eq!(bpm, 140.0);
            assert!(off_kick(first) < 2.0, "{hat_ms}: {}", off_kick(first));
            // Off: as without a pulse.
            let (_, plain) =
                first_line(&build_grid_with_pulse(&input, Some(&pulse), &off).unwrap());
            let (_, none) = first_line(&build_grid(&input, &opts).unwrap());
            assert_eq!(plain, none);
        }
    }

    #[test]
    fn dynamic_numbering_follows_downbeats() {
        let beats = [
            false, false, true, false, false, true, false, false, false, false, false,
        ]
        .iter()
        .enumerate()
        .map(|(i, &downbeat)| TrackedBeat {
            position: i as i64 * 1000,
            downbeat,
        })
        .collect::<Vec<_>>();
        let numbers: Vec<u8> = number_beats(&beats).iter().map(|b| b.beat_in_bar).collect();
        assert_eq!(numbers, [3, 4, 1, 2, 3, 1, 2, 3, 4, 1, 2]);
    }

    #[test]
    fn invalid_input() {
        let beat = |position| TrackedBeat {
            position,
            downbeat: false,
        };
        let cases = [
            (0, vec![beat(0), beat(100)], GridError::ZeroSampleRate),
            (SR, vec![], GridError::TooFewBeats(0)),
            (SR, vec![beat(5)], GridError::TooFewBeats(1)),
            (
                SR,
                vec![beat(0), beat(100), beat(100)],
                GridError::UnorderedBeats { index: 2 },
            ),
            (
                SR,
                vec![beat(0), beat(100), beat(50)],
                GridError::UnorderedBeats { index: 2 },
            ),
        ];
        for (sample_rate, beats, want) in cases {
            let input = BeatList { sample_rate, beats };
            assert_eq!(build_grid(&input, &GridOptions::default()), Err(want));
        }
    }
}
