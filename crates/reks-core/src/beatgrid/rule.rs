//! The beat rule: which beats a list of anchors stands for (D10, "Grid
//! editing with tempo sections"). The editor, `PQTZ` and the decks all take
//! their beats from [`grid_beats`], so they agree beat for beat.
//!
//! A section runs from its anchor to the next at the anchor's tempo; the
//! first also runs back to 0 ms. Its beats are `anchor + n × period` while a
//! beat is at least half a period before the next anchor, so a section holds
//! round(gap / period) beats (halves round up) and the interval into the next
//! marker is ½–1½ beats. rekordbox's edited grids put markers within ±2 ms
//! of a whole beat; the old "up to the next marker" rule doubled the beat
//! there.

use super::GridAnchor;

/// Slowest tempo a grid may have, ×100.
pub const MIN_BPM_X100: i64 = 3000;
/// Fastest tempo a grid may have, ×100.
pub const MAX_BPM_X100: i64 = 30000;
/// Most beats per bar; `PQTZ` numbers beats 1–4, XML `Metro` is free.
pub const MAX_BEATS_PER_BAR: u8 = 16;

/// `bpm_x100` × ms per beat.
const MS_X100_PER_MINUTE: i64 = 6_000_000;

/// One beat of a grid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridBeat {
    /// Unrounded, from the start of the track.
    pub position_ms: f64,
    /// 1 = downbeat.
    pub beat_in_bar: u8,
    pub beats_per_bar: u8,
    /// The tempo of its section.
    pub bpm_x100: i64,
    /// Index of its anchor.
    pub section: usize,
    /// Beats after the section's anchor (negative before the first anchor).
    pub n: i64,
}

impl GridAnchor {
    /// Milliseconds per beat.
    pub(crate) fn period_ms(&self) -> f64 {
        MS_X100_PER_MINUTE as f64 / self.bpm_x100 as f64
    }

    /// Position of the beat `n` beats after this anchor.
    pub(crate) fn beat_ms(&self, n: i64) -> f64 {
        self.position_ms as f64 + n as f64 * self.period_ms()
    }

    /// Bar position of the beat `n` beats after this anchor, 1-based.
    pub(crate) fn beat_in_bar_after(&self, n: i64) -> u8 {
        let bpb = i64::from(self.beats_per_bar.max(1));
        ((i64::from(self.beat_in_bar) - 1 + n).rem_euclid(bpb) + 1) as u8
    }

    /// `beat_in_bar` that makes the beat `n` beats after this anchor beat
    /// `in_bar` (1-based) of its bar.
    pub(crate) fn beat_in_bar_for(&self, n: i64, in_bar: u8) -> u8 {
        let bpb = i64::from(self.beats_per_bar.max(1));
        ((i64::from(in_bar) - 1 - n).rem_euclid(bpb) + 1) as u8
    }
}

/// Beats of the section at `a` that the rule keeps before `next`:
/// round(gap / period), halves up, in integers so every consumer counts the
/// same. Zero or less when `next` is under half a beat away.
pub fn section_beats(a: &GridAnchor, next: &GridAnchor) -> i64 {
    let gap = next.position_ms - a.position_ms;
    (2 * gap * a.bpm_x100 + MS_X100_PER_MINUTE).div_euclid(2 * MS_X100_PER_MINUTE)
}

/// Every beat of `anchors` (sorted by position) in `0..end_ms`. Anchors
/// with a tempo of zero or less have no beats.
pub fn grid_beats(anchors: &[GridAnchor], end_ms: f64) -> Vec<GridBeat> {
    let mut out = Vec::new();
    for (i, a) in anchors.iter().enumerate() {
        if a.bpm_x100 <= 0 {
            continue;
        }
        let period = a.period_ms();
        // The first section reaches back to 0 ms.
        let first = if i == 0 {
            -(a.position_ms as f64 / period).floor() as i64
        } else {
            0
        };
        let count = anchors.get(i + 1).map(|next| section_beats(a, next));
        let mut n = first;
        while count.is_none_or(|c| n < c) {
            let position_ms = a.beat_ms(n);
            if position_ms >= end_ms {
                break;
            }
            if position_ms >= 0.0 {
                out.push(GridBeat {
                    position_ms,
                    beat_in_bar: a.beat_in_bar_after(n),
                    beats_per_bar: a.beats_per_bar,
                    bpm_x100: a.bpm_x100,
                    section: i,
                    n,
                });
            }
            n += 1;
        }
    }
    out
}

/// Index of the section that holds `at_ms`: the last anchor at or before
/// it, else the first. `None` without anchors.
pub fn section_at(anchors: &[GridAnchor], at_ms: i64) -> Option<usize> {
    if anchors.is_empty() {
        return None;
    }
    Some(
        anchors
            .partition_point(|a| a.position_ms <= at_ms)
            .saturating_sub(1),
    )
}

/// The beat nearest `at_ms`: its section and its `n` in that section. A
/// time past a section's last beat may be nearest the next marker.
pub fn nearest_beat(anchors: &[GridAnchor], at_ms: i64) -> Option<(usize, i64)> {
    let i = section_at(anchors, at_ms)?;
    let a = &anchors[i];
    let mut n = ((at_ms - a.position_ms) as f64 / a.period_ms()).round() as i64;
    if let Some(next) = anchors.get(i + 1) {
        let last = section_beats(a, next) - 1;
        if n > last {
            let past_last = (at_ms as f64 - a.beat_ms(last)).abs();
            if (next.position_ms - at_ms) as f64 <= past_last {
                return Some((i + 1, 0));
            }
            n = last;
        }
    }
    Some((i, n))
}

/// Tempo of the section that covers the most of a track `duration_ms` long
/// (the first one on a tie): what the track's BPM shows.
pub fn main_tempo(anchors: &[GridAnchor], duration_ms: i64) -> Option<i64> {
    let mut best: Option<(i64, i64)> = None;
    for (i, a) in anchors.iter().enumerate() {
        let start = if i == 0 { 0 } else { a.position_ms };
        let end = anchors
            .get(i + 1)
            .map_or(duration_ms.max(start), |n| n.position_ms);
        let span = end - start;
        if best.is_none_or(|(s, _)| span > s) {
            best = Some((span, a.bpm_x100));
        }
    }
    best.map(|(_, bpm)| bpm)
}

/// Why a grid is degenerate.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GridProblem {
    #[error("the grid has no markers")]
    Empty,
    #[error("marker {index} is before the start of the track")]
    BeforeStart { index: usize },
    #[error("marker {index} is not after the one before it")]
    NotAfterPrevious { index: usize },
    #[error("marker {index} is less than half a beat after the one before it")]
    TooClose { index: usize },
    #[error(
        "marker {index}: {:.2} BPM is outside {}–{} BPM",
        *bpm_x100 as f64 / 100.0,
        MIN_BPM_X100 / 100,
        MAX_BPM_X100 / 100
    )]
    Tempo { index: usize, bpm_x100: i64 },
    #[error("marker {index}: the bar position is outside its bar")]
    Meter { index: usize },
}

/// Refuses degenerate grids: no markers, a marker before 0 ms or not after
/// the previous one, one less than half a beat after the previous one (so
/// every section has a beat), a tempo outside [`MIN_BPM_X100`]..=
/// [`MAX_BPM_X100`], or a bar position outside its bar.
pub fn validate_anchors(anchors: &[GridAnchor]) -> Result<(), GridProblem> {
    if anchors.is_empty() {
        return Err(GridProblem::Empty);
    }
    for (index, a) in anchors.iter().enumerate() {
        if !(MIN_BPM_X100..=MAX_BPM_X100).contains(&a.bpm_x100) {
            return Err(GridProblem::Tempo {
                index,
                bpm_x100: a.bpm_x100,
            });
        }
        if !(1..=MAX_BEATS_PER_BAR).contains(&a.beats_per_bar)
            || !(1..=a.beats_per_bar).contains(&a.beat_in_bar)
        {
            return Err(GridProblem::Meter { index });
        }
        if a.position_ms < 0 {
            return Err(GridProblem::BeforeStart { index });
        }
        if let Some(prev) = index.checked_sub(1).map(|p| &anchors[p]) {
            if a.position_ms <= prev.position_ms {
                return Err(GridProblem::NotAfterPrevious { index });
            }
            if section_beats(prev, a) < 1 {
                return Err(GridProblem::TooClose { index });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use proptest::prelude::*;

    pub(crate) fn anchor(position_ms: i64, bpm_x100: i64, beat_in_bar: u8) -> GridAnchor {
        GridAnchor {
            position_ms,
            bpm_x100,
            beat_in_bar,
            beats_per_bar: 4,
        }
    }

    fn ms(beats: &[GridBeat]) -> Vec<f64> {
        beats.iter().map(|b| b.position_ms).collect()
    }

    #[test]
    fn a_marker_just_after_a_beat_replaces_it() {
        // rekordbox's edited grids: the next marker 1 ms after beat 4.
        let g = [anchor(0, 12000, 1), anchor(2_001, 12000, 1)];
        assert_eq!(
            ms(&grid_beats(&g, 3_000.0)),
            [0.0, 500.0, 1_000.0, 1_500.0, 2_001.0, 2_501.0]
        );
        // And 1 ms before it.
        let g = [anchor(0, 12000, 1), anchor(1_999, 12000, 1)];
        assert_eq!(grid_beats(&g, 2_100.0).len(), 5);
    }

    #[test]
    fn sections_end_half_a_beat_before_the_next_marker() {
        // 2.5 beats: three beats (halves round up); 2.4 beats: two.
        assert_eq!(
            section_beats(&anchor(0, 12000, 1), &anchor(1_250, 12000, 1)),
            3
        );
        assert_eq!(
            section_beats(&anchor(0, 12000, 1), &anchor(1_200, 12000, 1)),
            2
        );
        assert_eq!(
            section_beats(&anchor(0, 12000, 1), &anchor(249, 12000, 1)),
            0
        );
        let g = [anchor(0, 12000, 1), anchor(1_200, 6000, 3)];
        let b = grid_beats(&g, 3_200.0);
        assert_eq!(ms(&b), [0.0, 500.0, 1_200.0, 2_200.0]);
        assert_eq!(
            b.iter()
                .map(|b| (b.beat_in_bar, b.section))
                .collect::<Vec<_>>(),
            [(1, 0), (2, 0), (3, 1), (4, 1)]
        );
    }

    #[test]
    fn the_first_section_runs_back_to_zero() {
        let b = grid_beats(&[anchor(1_100, 12000, 1)], 2_000.0);
        assert_eq!(ms(&b), [100.0, 600.0, 1_100.0, 1_600.0]);
        assert_eq!(b[0].beat_in_bar, 3);
        assert_eq!(b[0].n, -2);
        assert!(grid_beats(&[], 1_000.0).is_empty());
        assert!(grid_beats(&[anchor(0, 0, 1)], 1_000.0).is_empty());
    }

    #[test]
    fn nearest_beat_and_section() {
        let g = [anchor(0, 12000, 1), anchor(1_200, 6000, 1)];
        assert_eq!(section_at(&g, -5), Some(0));
        assert_eq!(section_at(&g, 1_199), Some(0));
        assert_eq!(section_at(&g, 1_200), Some(1));
        assert_eq!(nearest_beat(&g, 480), Some((0, 1)));
        // Past the last beat of section 0 (500 ms) but nearer the marker.
        assert_eq!(nearest_beat(&g, 900), Some((1, 0)));
        assert_eq!(nearest_beat(&g, 800), Some((0, 1)));
        assert_eq!(nearest_beat(&g, 3_100), Some((1, 2)));
        assert_eq!(nearest_beat(&[], 3), None);
    }

    #[test]
    fn main_tempo_is_the_longest_section() {
        let g = [anchor(300, 14000, 1), anchor(60_000, 17400, 1)];
        assert_eq!(main_tempo(&g, 200_000), Some(17400));
        assert_eq!(main_tempo(&g, 100_000), Some(14000));
        assert_eq!(main_tempo(&[], 1), None);
    }

    #[test]
    fn degenerate_grids_are_refused() {
        use GridProblem::*;
        let ok = [anchor(0, 14000, 1), anchor(60_000, 17400, 1)];
        assert_eq!(validate_anchors(&ok), Ok(()));
        let cases: [(Vec<GridAnchor>, GridProblem); 7] = [
            (vec![], Empty),
            (vec![anchor(-1, 12000, 1)], BeforeStart { index: 0 }),
            (
                vec![anchor(0, 12000, 1), anchor(0, 12000, 1)],
                NotAfterPrevious { index: 1 },
            ),
            (
                vec![anchor(0, 12000, 1), anchor(249, 12000, 1)],
                TooClose { index: 1 },
            ),
            (
                vec![anchor(0, 2999, 1)],
                Tempo {
                    index: 0,
                    bpm_x100: 2999,
                },
            ),
            (vec![anchor(0, 12000, 5)], Meter { index: 0 }),
            (vec![anchor(0, 12000, 0)], Meter { index: 0 }),
        ];
        for (g, problem) in cases {
            assert_eq!(validate_anchors(&g), Err(problem), "{g:?}");
        }
        // Exactly half a beat is a beat.
        assert_eq!(
            validate_anchors(&[anchor(0, 12000, 1), anchor(250, 12000, 1)]),
            Ok(())
        );
    }

    /// Valid grids: 1–6 sections, 60–200 BPM, gaps of ½–64 beats.
    pub(crate) fn valid_grid() -> impl Strategy<Value = Vec<GridAnchor>> {
        (
            0i64..2_000,
            prop::collection::vec((6_000i64..20_000, 1u8..=4, 0.5f64..64.0), 1..6),
        )
            .prop_map(|(start, sections)| {
                let mut out: Vec<GridAnchor> = Vec::new();
                let mut pos = start;
                for (bpm, beat, beats) in sections {
                    if let Some(prev) = out.last() {
                        pos += (beats * prev.period_ms()).ceil() as i64;
                    }
                    out.push(anchor(pos, bpm, beat));
                }
                out
            })
    }

    proptest! {
        #[test]
        fn valid_grids_give_strictly_increasing_whole_ms_beats(g in valid_grid()) {
            prop_assert_eq!(validate_anchors(&g), Ok(()));
            let end = g.last().map_or(0, |a| a.position_ms) as f64 + 10_000.0;
            let beats = grid_beats(&g, end);
            prop_assert!(!beats.is_empty());
            for pair in beats.windows(2) {
                prop_assert!(pair[1].position_ms.round() > pair[0].position_ms.round());
                // Boundary intervals are ½–1½ beats of the earlier section.
                let period = 6_000_000.0 / pair[0].bpm_x100 as f64;
                let gap = pair[1].position_ms - pair[0].position_ms;
                prop_assert!(gap >= period / 2.0 - 1e-6 && gap <= 1.5 * period + 1e-6);
            }
            // Every marker is a beat.
            for a in &g {
                prop_assert!(beats.iter().any(|b| b.position_ms == a.position_ms as f64));
            }
        }
    }
}
