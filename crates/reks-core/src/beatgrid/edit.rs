//! Grid edits as pure functions over anchors (phase 3 tasks 13 and 15).
//! Every result passes [`validate_anchors`]; an edit that would make a
//! degenerate grid fails and changes nothing.

use serde::Deserialize;

use super::rule::{
    grid_beats, nearest_beat, section_at, section_beats, validate_anchors, GridProblem,
};
use super::GridAnchor;
use crate::error::{Error, Result};
use crate::scan::TrackId;

/// An edit from the grid editor. `section` is an anchor's index.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase", tag = "op")]
pub enum GridEdit {
    /// Move every marker by `delta_ms` (shift the whole grid).
    Nudge { delta_ms: i64 },
    /// Make the beat nearest `at_ms` beat 1 of its bar, in its section only.
    /// Beats do not move.
    SetDownbeat { at_ms: i64 },
    /// Twice the tempo of `section` (every section with `None`); downbeats
    /// stay where they are.
    Double { section: Option<usize> },
    /// Half the tempo of `section` (every section with `None`); every other
    /// downbeat stays.
    Halve { section: Option<usize> },
    /// Set the tempo of the section at `anchor_ms`. With no grid yet, creates
    /// one with beat 1 at `anchor_ms`.
    SetBpm { bpm_x100: i64, anchor_ms: i64 },
    /// Set the tempo of `section` (typed or tapped); its marker stays.
    SetSectionBpm { section: usize, bpm_x100: i64 },
    /// A marker at `at_ms`: the tempo of the section there, and the bar
    /// position of the beat nearest it.
    AddMarker { at_ms: i64 },
    /// Move marker `section` to `to_ms` with its tempo and bar phase.
    MoveMarker { section: usize, to_ms: i64 },
    /// Remove marker `section`; the section before runs on (or, for the
    /// first marker, the next one runs back to the start).
    DeleteMarker { section: usize },
    /// Retempo `section` so its beat nearest `beat_ms` lands on `to_ms`.
    Stretch {
        section: usize,
        beat_ms: i64,
        to_ms: i64,
    },
    /// Back to the grid from before the first edit: `section` only, or
    /// all of it with `None`.
    Reset { section: Option<usize> },
    /// Replace the anchors (the editor's undo and redo).
    SetAnchors { anchors: Vec<GridAnchor> },
    /// Lock or unlock the grid (no grid needed).
    SetLocked { locked: bool },
}

impl GridEdit {
    /// The positions this edit places, for snapping to transients.
    pub fn snap_target(&mut self) -> Option<&mut i64> {
        match self {
            Self::AddMarker { at_ms } => Some(at_ms),
            Self::MoveMarker { to_ms, .. } | Self::Stretch { to_ms, .. } => Some(to_ms),
            _ => None,
        }
    }
}

/// Applies `edit` to `anchors` (sorted). `base` is the grid to reset to
/// (`Some(&[])` when the track had none); `Reset` without one fails.
/// Locking is handled by the caller; `SetLocked` returns `anchors`.
pub fn apply_edit(
    track: TrackId,
    anchors: &[GridAnchor],
    edit: &GridEdit,
    base: Option<&[GridAnchor]>,
) -> Result<Vec<GridAnchor>> {
    let invalid = |problem: GridProblem| Error::InvalidGrid { track, problem };
    let need_grid = || {
        if anchors.is_empty() {
            Err(Error::NoGrid(track))
        } else {
            Ok(anchors.to_vec())
        }
    };
    let section = |i: usize| -> Result<usize> {
        if i < anchors.len() {
            Ok(i)
        } else {
            Err(Error::InvalidField {
                field: "section",
                reason: format!("no marker {i}"),
            })
        }
    };
    let out = match edit {
        GridEdit::SetLocked { .. } => return Ok(anchors.to_vec()),
        GridEdit::Nudge { delta_ms } => nudge(need_grid()?, *delta_ms),
        GridEdit::SetDownbeat { at_ms } => {
            let mut g = need_grid()?;
            let (i, n) = nearest_beat(&g, *at_ms).ok_or(Error::NoGrid(track))?;
            g[i].beat_in_bar = g[i].beat_in_bar_for(n, 1);
            g
        }
        GridEdit::Double { section: s } => {
            let mut g = need_grid()?;
            for i in sections(&g, s.map(section).transpose()?) {
                g[i] = double(g[i]);
            }
            g
        }
        GridEdit::Halve { section: s } => {
            let mut g = need_grid()?;
            for i in sections(&g, s.map(section).transpose()?) {
                g[i] = halve(g[i]);
            }
            g
        }
        GridEdit::SetBpm {
            bpm_x100,
            anchor_ms,
        } => match section_at(anchors, *anchor_ms) {
            None => vec![GridAnchor {
                position_ms: (*anchor_ms).max(0),
                bpm_x100: *bpm_x100,
                beat_in_bar: 1,
                beats_per_bar: 4,
            }],
            Some(i) => with_bpm(anchors, i, *bpm_x100),
        },
        GridEdit::SetSectionBpm {
            section: s,
            bpm_x100,
        } => with_bpm(anchors, section(*s)?, *bpm_x100),
        GridEdit::AddMarker { at_ms } => {
            let mut g = need_grid()?;
            if g.iter().any(|a| a.position_ms == *at_ms) {
                return Err(Error::InvalidField {
                    field: "marker",
                    reason: "there is a marker here already".into(),
                });
            }
            let (i, n) = nearest_beat(&g, *at_ms).ok_or(Error::NoGrid(track))?;
            let here = section_at(&g, *at_ms).unwrap_or(0);
            let marker = GridAnchor {
                position_ms: *at_ms,
                beat_in_bar: g[i].beat_in_bar_after(n),
                ..g[here]
            };
            let at = g.partition_point(|a| a.position_ms < *at_ms);
            g.insert(at, marker);
            g
        }
        GridEdit::MoveMarker { section: s, to_ms } => {
            let mut g = anchors.to_vec();
            g[section(*s)?].position_ms = *to_ms;
            g
        }
        GridEdit::DeleteMarker { section: s } => {
            let i = section(*s)?;
            if anchors.len() == 1 {
                return Err(Error::InvalidField {
                    field: "marker",
                    reason: "a grid keeps at least one marker; reset it instead".into(),
                });
            }
            let mut g = anchors.to_vec();
            g.remove(i);
            g
        }
        GridEdit::Stretch {
            section: s,
            beat_ms,
            to_ms,
        } => {
            let i = section(*s)?;
            stretch(anchors, i, *beat_ms, *to_ms).ok_or_else(|| Error::InvalidField {
                field: "stretch",
                reason: "pick a beat of the section other than its marker, and a time on the same side of the marker".into(),
            })?
        }
        GridEdit::Reset { section: s } => {
            let base = base.ok_or(Error::NoGridBase(track))?;
            match s {
                None => {
                    // Whatever the track had: rekordbox's or the analyser's grid
                    // is stored as it was, without validation.
                    return Ok(base.to_vec());
                }
                Some(s) => reset_section(anchors, section(*s)?, base),
            }
        }
        GridEdit::SetAnchors { anchors } => anchors.clone(),
    };
    // Not re-sorted: a marker moved past its neighbour is refused.
    validate_anchors(&out).map_err(invalid)?;
    Ok(out)
}

/// Every anchor moved by `delta_ms`; a first marker before 0 ms steps
/// forward by whole beats (the beats stay where they are).
fn nudge(mut g: Vec<GridAnchor>, delta_ms: i64) -> Vec<GridAnchor> {
    for a in &mut g {
        a.position_ms += delta_ms;
    }
    if let Some(first) = g.first_mut() {
        if first.position_ms < 0 && first.bpm_x100 > 0 {
            let n = (-first.position_ms as f64 / first.period_ms()).ceil() as i64;
            *first = GridAnchor {
                position_ms: first.beat_ms(n).round() as i64,
                beat_in_bar: first.beat_in_bar_after(n),
                ..*first
            };
        }
    }
    g
}

/// Indexes of `section`, or every one.
fn sections(g: &[GridAnchor], section: Option<usize>) -> Vec<usize> {
    section.map_or_else(|| (0..g.len()).collect(), |i| vec![i])
}

/// Twice the tempo: old beat k of the bar is new beat 2k, so downbeats stay.
fn double(a: GridAnchor) -> GridAnchor {
    let bpb = i64::from(a.beats_per_bar.max(1));
    let k = i64::from(a.beat_in_bar) - 1;
    GridAnchor {
        bpm_x100: a.bpm_x100 * 2,
        beat_in_bar: ((2 * k).rem_euclid(bpb) + 1) as u8,
        ..a
    }
}

/// Half the tempo. Beats an even number of beats from a downbeat survive;
/// if the marker is not one, it moves to the next beat.
fn halve(a: GridAnchor) -> GridAnchor {
    let bpb = i64::from(a.beats_per_bar.max(1));
    let mut k = i64::from(a.beat_in_bar) - 1;
    let mut pos = a.position_ms;
    if k % 2 == 1 {
        pos = a.beat_ms(1).round() as i64;
        k = (k + 1).rem_euclid(bpb);
    }
    GridAnchor {
        position_ms: pos,
        bpm_x100: (a.bpm_x100 + 1) / 2,
        beat_in_bar: ((k / 2).rem_euclid(bpb) + 1) as u8,
        ..a
    }
}

fn with_bpm(g: &[GridAnchor], i: usize, bpm_x100: i64) -> Vec<GridAnchor> {
    let mut g = g.to_vec();
    g[i].bpm_x100 = bpm_x100;
    g
}

/// Section `i` retempoed so its beat nearest `beat_ms` lands on `to_ms`
/// (as near as 0.01 BPM allows). `None` when that beat is the marker or
/// `to_ms` is on the other side of it.
fn stretch(g: &[GridAnchor], i: usize, beat_ms: i64, to_ms: i64) -> Option<Vec<GridAnchor>> {
    let a = g[i];
    let mut n = ((beat_ms - a.position_ms) as f64 / a.period_ms()).round() as i64;
    if let Some(next) = g.get(i + 1) {
        n = n.min(section_beats(&a, next));
    }
    let span = to_ms - a.position_ms;
    if n == 0 || span == 0 || (span > 0) != (n > 0) {
        return None;
    }
    let bpm_x100 = (n as f64 * 6_000_000.0 / span as f64).round() as i64;
    Some(with_bpm(g, i, bpm_x100))
}

/// Section `i` replaced by the base grid over the same span: a marker at
/// the base's first beat at or after the section's start (0 ms for the
/// first), then the base's markers inside the span. Base markers within
/// half a beat of the next marker are left out.
fn reset_section(g: &[GridAnchor], i: usize, base: &[GridAnchor]) -> Vec<GridAnchor> {
    let start = if i == 0 { 0 } else { g[i].position_ms };
    let end = g.get(i + 1).map(|a| a.position_ms);
    let mut span: Vec<GridAnchor> = Vec::new();
    // The base's first beat at or after `start` is under one beat (2 s at
    // the slowest tempo) after it.
    if let Some(first) = grid_beats(base, start as f64 + 3_000.0)
        .into_iter()
        .find(|b| b.position_ms.round() as i64 >= start)
    {
        let src = base[first.section];
        span.push(GridAnchor {
            position_ms: first.position_ms.round() as i64,
            beat_in_bar: first.beat_in_bar,
            ..src
        });
    }
    let after = span.first().map_or(start - 1, |f| f.position_ms);
    span.extend(
        base.iter()
            .filter(|a| a.position_ms > after)
            .filter(|a| end.is_none_or(|e| a.position_ms < e))
            .copied(),
    );
    if let Some(e) = end {
        span.retain(|a| a.position_ms < e);
        while let Some(last) = span.last() {
            let next = GridAnchor {
                position_ms: e,
                ..*last
            };
            if section_beats(last, &next) >= 1 {
                break;
            }
            span.pop();
        }
    }
    if span.is_empty() {
        // Nothing of the base fits: the section stays.
        return g.to_vec();
    }
    let mut out = g[..i].to_vec();
    out.extend(span);
    out.extend_from_slice(&g[i + 1..]);
    out
}
