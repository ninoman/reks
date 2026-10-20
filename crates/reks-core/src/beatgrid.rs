//! Beatgrid editing (Phase 3 task 13): the grid editor's maths and storage.
//!
//! A grid is its `beatgrid` rows (tempo anchors). Positions are whole
//! milliseconds here: the editor's smallest step is 1 ms, rekordbox XML and
//! PQTZ store whole ms, and reading rounds the `REAL` column. Tempo is
//! `bpm_x100` as stored. Every edit replaces the track's rows with
//! `source = 'manual'`; re-analysis must leave such grids alone, and locked
//! ones too ([`Library::grid_is_user_owned`]). A constant grid's tempo also
//! becomes the track's BPM (`bpm_source = 'manual'`).
//!
//! Only nudge and lock apply to a dynamic grid (more than one anchor); the
//! other edits need a constant grid (phase 3 non-goal: dynamic-grid editing).

use std::collections::HashSet;

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::Library;
use crate::error::{Error, Result};
use crate::scan::TrackId;

/// SQL predicate on a `track` row: its grid is locked or edited by hand.
const USER_OWNED: &str = "(grid_locked OR EXISTS (SELECT 1 FROM beatgrid
                                                  WHERE beatgrid.track_id = track.id
                                                    AND beatgrid.source = 'manual'))";

/// Slowest tempo an edit may produce, ×100.
pub const MIN_BPM_X100: i64 = 3000;
/// Fastest tempo an edit may produce, ×100.
pub const MAX_BPM_X100: i64 = 30000;
/// A pause longer than this between taps starts a new tap run, ms.
pub const TAP_RESET_MS: i64 = 2_000;
/// Taps (the newest ones) that count towards the tempo.
pub const TAP_WINDOW: usize = 16;

/// One tempo anchor: the beat at `position_ms` is beat `beat_in_bar` of a bar
/// of `beats_per_bar`, and the tempo is `bpm_x100` until the next anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GridAnchor {
    pub position_ms: i64,
    pub bpm_x100: i64,
    /// 1..=`beats_per_bar`.
    pub beat_in_bar: u8,
    pub beats_per_bar: u8,
}

impl GridAnchor {
    /// Milliseconds per beat.
    fn period_ms(&self) -> f64 {
        6_000_000.0 / self.bpm_x100 as f64
    }

    /// Bar position of the beat `n` beats after this anchor (`n` may be negative), 1-based.
    fn beat_in_bar_after(&self, n: i64) -> u8 {
        let bpb = i64::from(self.beats_per_bar.max(1));
        ((i64::from(self.beat_in_bar) - 1 + n).rem_euclid(bpb) + 1) as u8
    }
}

/// One beat line for display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BeatMark {
    /// Microseconds from the start of the track.
    pub position_us: i64,
    /// 1 = downbeat.
    pub beat_in_bar: u8,
}

/// A track's grid as the editor shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Beatgrid {
    /// Sorted by position; empty when the track has no grid.
    pub anchors: Vec<GridAnchor>,
    /// `rekordbox`, `analysis` or `manual` (the first row's; edits make all `manual`).
    pub source: Option<String>,
    pub locked: bool,
    /// Every beat from the first anchor to the end of the track.
    pub beats: Vec<BeatMark>,
}

/// An edit from the grid editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase", tag = "op")]
pub enum GridEdit {
    /// Move every anchor by `delta_ms` (±1, ±10 in the UI).
    Nudge { delta_ms: i64 },
    /// Make the beat nearest `at_ms` beat 1 of its bar. Beats do not move.
    SetDownbeat { at_ms: i64 },
    /// Twice the tempo; downbeats stay where they are.
    Double,
    /// Half the tempo; every other downbeat stays.
    Halve,
    /// Set the tempo, keeping the anchor. With no grid yet, creates one with
    /// beat 1 at `anchor_ms`.
    SetBpm { bpm_x100: i64, anchor_ms: i64 },
    /// Lock or unlock the grid (no grid needed).
    SetLocked { locked: bool },
}

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

/// Every beat from the first anchor up to `duration_ms`.
pub fn beat_marks(anchors: &[GridAnchor], duration_ms: i64) -> Vec<BeatMark> {
    let mut out = Vec::new();
    for (i, a) in anchors.iter().enumerate() {
        if a.bpm_x100 <= 0 {
            continue;
        }
        // Up to the next anchor (exclusive) or the end of the track (inclusive).
        let next_us = anchors.get(i + 1).map(|next| next.position_ms * 1000);
        let period_us = 6_000_000_000.0 / a.bpm_x100 as f64;
        let start_us = a.position_ms * 1000;
        for n in 0.. {
            let pos = start_us + (n as f64 * period_us).round() as i64;
            if next_us.is_some_and(|next| pos >= next) || pos > duration_ms * 1000 {
                break;
            }
            out.push(BeatMark {
                position_us: pos,
                beat_in_bar: a.beat_in_bar_after(n),
            });
        }
    }
    out
}

/// Applies `edit` to `anchors` and returns the new anchors. Pure; locking is
/// handled by the caller. Errors leave nothing changed.
pub fn apply_edit(
    track: TrackId,
    anchors: &[GridAnchor],
    edit: GridEdit,
) -> Result<Vec<GridAnchor>> {
    let constant = || match anchors {
        [] => Err(Error::NoGrid(track)),
        [a] => Ok(*a),
        _ => Err(Error::DynamicGrid(track)),
    };
    let out = match edit {
        GridEdit::Nudge { delta_ms } => {
            if anchors.is_empty() {
                return Err(Error::NoGrid(track));
            }
            let mut moved: Vec<GridAnchor> = anchors
                .iter()
                .map(|a| GridAnchor {
                    position_ms: a.position_ms + delta_ms,
                    ..*a
                })
                .collect();
            // Keep the first anchor at or after 0: step it forward by whole beats.
            if let Some(first) = moved.first_mut() {
                if first.position_ms < 0 {
                    let n = (-first.position_ms as f64 / first.period_ms()).ceil() as i64;
                    let pos = first.position_ms + (n as f64 * first.period_ms()).round() as i64;
                    *first = GridAnchor {
                        position_ms: pos,
                        beat_in_bar: first.beat_in_bar_after(n),
                        ..*first
                    };
                }
            }
            moved
        }
        GridEdit::SetDownbeat { at_ms } => {
            let a = constant()?;
            let n = ((at_ms - a.position_ms) as f64 / a.period_ms()).round() as i64;
            // Beat n becomes 1, so the anchor is 1 - n.
            let bpb = i64::from(a.beats_per_bar.max(1));
            vec![GridAnchor {
                beat_in_bar: ((-n).rem_euclid(bpb) + 1) as u8,
                ..a
            }]
        }
        GridEdit::Double => {
            let a = constant()?;
            // Old beat k of the bar is new beat 2k: downbeats stay downbeats.
            let bpb = i64::from(a.beats_per_bar.max(1));
            let k = i64::from(a.beat_in_bar) - 1;
            vec![GridAnchor {
                bpm_x100: check_bpm(a.bpm_x100 * 2)?,
                beat_in_bar: ((2 * k).rem_euclid(bpb) + 1) as u8,
                ..a
            }]
        }
        GridEdit::Halve => {
            let a = constant()?;
            let bpb = i64::from(a.beats_per_bar.max(1));
            let bpm = check_bpm((a.bpm_x100 + 1) / 2)?;
            // Surviving beats are those an even number of beats from a downbeat;
            // if the anchor is not one, move it to the next beat.
            let mut k = i64::from(a.beat_in_bar) - 1;
            let mut pos = a.position_ms;
            if k % 2 == 1 {
                pos += a.period_ms().round() as i64;
                k = (k + 1).rem_euclid(bpb);
            }
            vec![GridAnchor {
                position_ms: pos,
                bpm_x100: bpm,
                beat_in_bar: ((k / 2).rem_euclid(bpb) + 1) as u8,
                ..a
            }]
        }
        GridEdit::SetBpm {
            bpm_x100,
            anchor_ms,
        } => {
            let bpm_x100 = check_bpm(bpm_x100)?;
            match anchors {
                [] => vec![GridAnchor {
                    position_ms: anchor_ms.max(0),
                    bpm_x100,
                    beat_in_bar: 1,
                    beats_per_bar: 4,
                }],
                _ => vec![GridAnchor {
                    bpm_x100,
                    ..constant()?
                }],
            }
        }
        GridEdit::SetLocked { .. } => anchors.to_vec(),
    };
    Ok(out)
}

fn check_bpm(bpm_x100: i64) -> Result<i64> {
    if (MIN_BPM_X100..=MAX_BPM_X100).contains(&bpm_x100) {
        Ok(bpm_x100)
    } else {
        Err(Error::InvalidField {
            field: "bpm",
            reason: format!(
                "{:.2} is outside {}–{} BPM",
                bpm_x100 as f64 / 100.0,
                MIN_BPM_X100 / 100,
                MAX_BPM_X100 / 100
            ),
        })
    }
}

impl Library {
    /// The grid of track `id`, with every beat up to the track's end.
    pub fn beatgrid(&self, id: TrackId) -> Result<Beatgrid> {
        let conn = self.conn()?;
        read_grid(&conn, id)
    }

    /// Applies `edit` to track `id`'s grid and stores the result as `manual`
    /// rows. Fails with [`Error::GridLocked`] on a locked grid (except unlocking).
    pub fn edit_beatgrid(&self, id: TrackId, edit: GridEdit) -> Result<Beatgrid> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        let before = read_grid(&tx, id)?;
        if let GridEdit::SetLocked { locked } = edit {
            tx.execute(
                "UPDATE track SET grid_locked = ?2 WHERE id = ?1",
                params![id, locked],
            )?;
        } else {
            if before.locked {
                return Err(Error::GridLocked(id));
            }
            let anchors = apply_edit(id, &before.anchors, edit)?;
            tx.execute("DELETE FROM beatgrid WHERE track_id = ?1", [id])?;
            let mut ins = tx.prepare(
                "INSERT INTO beatgrid (track_id, position_ms, bpm_x100, meter_num, meter_den,
                                       beat_in_bar, source)
                 VALUES (?1, ?2, ?3, ?4, 4, ?5, 'manual')",
            )?;
            for a in &anchors {
                ins.execute(params![
                    id,
                    a.position_ms as f64,
                    a.bpm_x100,
                    a.beats_per_bar,
                    a.beat_in_bar
                ])?;
            }
            drop(ins);
            // The track's display BPM follows a constant grid's tempo.
            if let [a] = anchors.as_slice() {
                tx.execute(
                    "UPDATE track SET bpm_x100 = ?2, bpm_source = 'manual', updated_at = ?3
                     WHERE id = ?1 AND bpm_x100 IS NOT ?2",
                    params![id, a.bpm_x100, crate::timefmt::now_utc()],
                )?;
            }
        }
        let after = read_grid(&tx, id)?;
        tx.commit()?;
        Ok(after)
    }

    /// True when re-analysis must keep track `id`'s grid: it is locked or
    /// was edited by hand (any `source = 'manual'` row).
    pub fn grid_is_user_owned(&self, id: TrackId) -> Result<bool> {
        let conn = self.conn()?;
        conn.query_row(
            &format!("SELECT {USER_OWNED} FROM track WHERE id = ?1"),
            [id],
            |r| r.get(0),
        )
        .optional()?
        .ok_or(Error::TrackNotFound(id))
    }

    /// Every track whose grid [`Library::grid_is_user_owned`], in one query
    /// (the USB export planner asks for a whole selection at once).
    pub fn user_owned_grids(&self) -> Result<HashSet<TrackId>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(&format!("SELECT id FROM track WHERE {USER_OWNED}"))?;
        let ids = stmt
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(ids)
    }
}

fn read_grid(conn: &rusqlite::Connection, id: TrackId) -> Result<Beatgrid> {
    let (locked, duration_ms): (bool, Option<i64>) = conn
        .query_row(
            "SELECT grid_locked, duration_ms FROM track WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .ok_or(Error::TrackNotFound(id))?;
    let mut stmt = conn.prepare(
        "SELECT position_ms, bpm_x100, meter_num, beat_in_bar, source
         FROM beatgrid WHERE track_id = ?1 ORDER BY position_ms, id",
    )?;
    let mut source = None;
    let anchors = stmt
        .query_map([id], |r| {
            let bpb: i64 = r.get(2)?;
            let bpb = u8::try_from(bpb).ok().filter(|&b| b > 0).unwrap_or(4);
            let beat: i64 = r.get(3)?;
            Ok((
                GridAnchor {
                    position_ms: r.get::<_, f64>(0)?.round() as i64,
                    bpm_x100: r.get(1)?,
                    beat_in_bar: u8::try_from(beat)
                        .ok()
                        .filter(|b| (1..=bpb).contains(b))
                        .unwrap_or(1),
                    beats_per_bar: bpb,
                },
                r.get::<_, String>(4)?,
            ))
        })?
        .map(|row| {
            row.map(|(a, s)| {
                source.get_or_insert(s);
                a
            })
        })
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let beats = beat_marks(&anchors, duration_ms.unwrap_or(0));
    Ok(Beatgrid {
        anchors,
        source,
        locked,
        beats,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: TrackId = 1;

    fn grid(position_ms: i64, bpm_x100: i64, beat_in_bar: u8) -> Vec<GridAnchor> {
        vec![GridAnchor {
            position_ms,
            bpm_x100,
            beat_in_bar,
            beats_per_bar: 4,
        }]
    }

    fn one(anchors: Vec<GridAnchor>) -> GridAnchor {
        assert_eq!(anchors.len(), 1);
        anchors[0]
    }

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

    #[test]
    fn nudge_moves_every_anchor() {
        let anchors = vec![
            GridAnchor {
                position_ms: 100,
                bpm_x100: 12000,
                beat_in_bar: 1,
                beats_per_bar: 4,
            },
            GridAnchor {
                position_ms: 60_100,
                bpm_x100: 12100,
                beat_in_bar: 3,
                beats_per_bar: 4,
            },
        ];
        let out = apply_edit(T, &anchors, GridEdit::Nudge { delta_ms: -10 }).unwrap();
        assert_eq!(out[0].position_ms, 90);
        assert_eq!(out[1].position_ms, 60_090);
        assert_eq!(out[1].beat_in_bar, 3);
        let out = apply_edit(T, &out, GridEdit::Nudge { delta_ms: 1 }).unwrap();
        assert_eq!(out[0].position_ms, 91);
    }

    #[test]
    fn nudge_below_zero_steps_the_anchor_a_beat_forward() {
        let a = one(apply_edit(T, &grid(5, 12000, 1), GridEdit::Nudge { delta_ms: -10 }).unwrap());
        assert_eq!(a.position_ms, 495);
        assert_eq!(a.beat_in_bar, 2);
    }

    #[test]
    fn set_downbeat_rephases_without_moving_beats() {
        // 120 BPM from 100 ms: beats at 100, 600, 1100, 1600...
        let a = one(apply_edit(
            T,
            &grid(100, 12000, 1),
            GridEdit::SetDownbeat { at_ms: 1080 },
        )
        .unwrap());
        assert_eq!(a.position_ms, 100);
        // Beat 2 after the anchor (1100 ms) is now 1, so the anchor is beat 3.
        assert_eq!(a.beat_in_bar, 3);
        let beats = beat_marks(&[a], 2_000);
        assert_eq!(beats[2].position_us, 1_100_000);
        assert_eq!(beats[2].beat_in_bar, 1);
        // Before the anchor works too.
        let a = one(apply_edit(
            T,
            &grid(2000, 12000, 1),
            GridEdit::SetDownbeat { at_ms: 1490 },
        )
        .unwrap());
        assert_eq!(a.beat_in_bar, 2);
    }

    #[test]
    fn double_keeps_downbeats() {
        for b in 1..=4u8 {
            let before = grid(1000, 6400, b);
            let after = apply_edit(T, &before, GridEdit::Double).unwrap();
            assert_eq!(after[0].bpm_x100, 12800);
            let old: Vec<i64> = beat_marks(&before, 30_000)
                .iter()
                .filter(|m| m.beat_in_bar == 1)
                .map(|m| m.position_us)
                .collect();
            let new = beat_marks(&after, 30_000);
            for p in old {
                assert!(
                    new.iter().any(|m| m.position_us == p && m.beat_in_bar == 1),
                    "beat {b}: old downbeat {p} lost"
                );
            }
        }
    }

    #[test]
    fn halve_keeps_every_other_downbeat() {
        for b in 1..=4u8 {
            let before = grid(1000, 12800, b);
            let after = apply_edit(T, &before, GridEdit::Halve).unwrap();
            assert_eq!(after[0].bpm_x100, 6400);
            let old_down: Vec<i64> = beat_marks(&before, 30_000)
                .iter()
                .filter(|m| m.beat_in_bar == 1)
                .map(|m| m.position_us)
                .collect();
            let new_down: Vec<i64> = beat_marks(&after, 30_000)
                .iter()
                .filter(|m| m.beat_in_bar == 1)
                .map(|m| m.position_us)
                .collect();
            assert!(!new_down.is_empty());
            for p in &new_down {
                assert!(
                    old_down.iter().any(|q| (q - p).abs() <= 1_000),
                    "beat {b}: new downbeat {p} was not an old one"
                );
            }
        }
        // Odd tempo rounds half up.
        assert_eq!(
            apply_edit(T, &grid(0, 12801, 1), GridEdit::Halve).unwrap()[0].bpm_x100,
            6401
        );
    }

    #[test]
    fn tempo_limits() {
        assert!(matches!(
            apply_edit(T, &grid(0, 17400, 1), GridEdit::Double),
            Err(Error::InvalidField { field: "bpm", .. })
        ));
        assert!(apply_edit(T, &grid(0, 5000, 1), GridEdit::Halve).is_err());
        assert!(apply_edit(
            T,
            &[],
            GridEdit::SetBpm {
                bpm_x100: 500,
                anchor_ms: 0
            }
        )
        .is_err());
    }

    #[test]
    fn set_bpm_creates_or_retempos() {
        let a = one(apply_edit(
            T,
            &[],
            GridEdit::SetBpm {
                bpm_x100: 12600,
                anchor_ms: 250,
            },
        )
        .unwrap());
        assert_eq!((a.position_ms, a.bpm_x100, a.beat_in_bar), (250, 12600, 1));
        let a = one(apply_edit(
            T,
            &grid(40, 12000, 3),
            GridEdit::SetBpm {
                bpm_x100: 12450,
                anchor_ms: 9,
            },
        )
        .unwrap());
        assert_eq!((a.position_ms, a.bpm_x100, a.beat_in_bar), (40, 12450, 3));
    }

    #[test]
    fn dynamic_grids_only_nudge() {
        let mut anchors = grid(0, 12000, 1);
        anchors.extend(grid(10_000, 12200, 1));
        assert!(apply_edit(T, &anchors, GridEdit::Nudge { delta_ms: 1 }).is_ok());
        for edit in [
            GridEdit::Double,
            GridEdit::Halve,
            GridEdit::SetDownbeat { at_ms: 0 },
            GridEdit::SetBpm {
                bpm_x100: 12000,
                anchor_ms: 0,
            },
        ] {
            assert!(matches!(
                apply_edit(T, &anchors, edit),
                Err(Error::DynamicGrid(T))
            ));
        }
        assert!(matches!(
            apply_edit(T, &[], GridEdit::Double),
            Err(Error::NoGrid(T))
        ));
    }

    #[test]
    fn serde_shapes_match_ipc_ts() {
        let edit: GridEdit = serde_json::from_str(r#"{"op":"nudge","deltaMs":-10}"#).unwrap();
        assert_eq!(edit, GridEdit::Nudge { delta_ms: -10 });
        let edit: GridEdit =
            serde_json::from_str(r#"{"op":"setBpm","bpmX100":12800,"anchorMs":5}"#).unwrap();
        assert_eq!(
            edit,
            GridEdit::SetBpm {
                bpm_x100: 12800,
                anchor_ms: 5
            }
        );
        for (json, edit) in [
            (
                r#"{"op":"setDownbeat","atMs":1}"#,
                GridEdit::SetDownbeat { at_ms: 1 },
            ),
            (r#"{"op":"double"}"#, GridEdit::Double),
            (r#"{"op":"halve"}"#, GridEdit::Halve),
            (
                r#"{"op":"setLocked","locked":true}"#,
                GridEdit::SetLocked { locked: true },
            ),
        ] {
            assert_eq!(serde_json::from_str::<GridEdit>(json).unwrap(), edit);
        }
        let grid = Beatgrid {
            anchors: grid(1, 12000, 2),
            source: Some("manual".into()),
            locked: false,
            beats: vec![BeatMark {
                position_us: 1000,
                beat_in_bar: 2,
            }],
        };
        assert_eq!(
            serde_json::to_string(&grid).unwrap(),
            r#"{"anchors":[{"positionMs":1,"bpmX100":12000,"beatInBar":2,"beatsPerBar":4}],"source":"manual","locked":false,"beats":[{"positionUs":1000,"beatInBar":2}]}"#
        );
    }

    #[test]
    fn beat_marks_span_anchors() {
        let anchors = vec![
            GridAnchor {
                position_ms: 0,
                bpm_x100: 12000,
                beat_in_bar: 1,
                beats_per_bar: 4,
            },
            GridAnchor {
                position_ms: 1_200,
                bpm_x100: 6000,
                beat_in_bar: 1,
                beats_per_bar: 4,
            },
        ];
        let us: Vec<i64> = beat_marks(&anchors, 3_200)
            .iter()
            .map(|m| m.position_us / 1000)
            .collect();
        assert_eq!(us, [0, 500, 1000, 1200, 2200, 3200]);
        assert!(beat_marks(&[], 1000).is_empty());
    }
}
