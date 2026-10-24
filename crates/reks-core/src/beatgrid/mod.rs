//! Beatgrids: the grid editor's model and storage (phase 3 tasks 13, 15;
//! D10, "Grid editing with tempo sections").
//!
//! A grid is its `beatgrid` rows (tempo anchors, or markers): whole ms,
//! tempo `bpm_x100`, exactly an XML `TEMPO` row. Each anchor starts a
//! section; [`rule`] says which beats the anchors stand for, [`edit`] holds
//! the edits. Every edit replaces the track's rows with `source = 'manual'`
//! and refuses a degenerate grid ([`validate_anchors`]); re-analysis leaves
//! manual and locked grids alone ([`Library::grid_is_user_owned`]). The
//! first edit keeps the grid it replaces (`track.grid_base`) for Reset. The
//! track's BPM follows the tempo of the longest section (`bpm_source =
//! 'manual'`).

mod edit;
mod rule;
mod tap;

use std::collections::HashSet;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::analysis::{SOURCE_ANALYSIS, SOURCE_MANUAL};
use crate::db::Library;
use crate::error::{Error, Result};
use crate::scan::TrackId;

pub use edit::{apply_edit, GridEdit};
pub use rule::{
    grid_beats, main_tempo, nearest_beat, section_at, section_beats, validate_anchors, GridBeat,
    GridProblem, MAX_BEATS_PER_BAR, MAX_BPM_X100, MIN_BPM_X100,
};
pub use tap::{tap_tempo, TAP_RESET_MS, TAP_WINDOW};

/// SQL predicate on a `track` row: its grid is locked or edited by hand.
pub(crate) const USER_OWNED: &str = "(grid_locked OR EXISTS (SELECT 1 FROM beatgrid
                                                  WHERE beatgrid.track_id = track.id
                                                    AND beatgrid.source = 'manual'))";

/// One tempo anchor (a marker): the beat at `position_ms` is beat
/// `beat_in_bar` of a bar of `beats_per_bar`, and the tempo is `bpm_x100`
/// until the next anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GridAnchor {
    pub position_ms: i64,
    pub bpm_x100: i64,
    /// 1..=`beats_per_bar`.
    pub beat_in_bar: u8,
    pub beats_per_bar: u8,
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
    /// Every beat up to the end of the track ([`grid_beats`]).
    pub beats: Vec<BeatMark>,
}

/// The grid a track had before its first edit (`track.grid_base`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct GridBase {
    source: Option<String>,
    anchors: Vec<GridAnchor>,
}

/// Every beat of `anchors` before `end_ms`, for display.
pub fn beat_marks(anchors: &[GridAnchor], end_ms: i64) -> Vec<BeatMark> {
    grid_beats(anchors, end_ms as f64)
        .iter()
        .map(|b| BeatMark {
            position_us: (b.position_ms * 1000.0).round() as i64,
            beat_in_bar: b.beat_in_bar,
        })
        .collect()
}

/// Where a track's grid stands, read in one go.
struct Stored {
    anchors: Vec<GridAnchor>,
    /// Per anchor.
    sources: Vec<String>,
    locked: bool,
    duration_ms: i64,
    base: Option<GridBase>,
}

impl Stored {
    fn view(&self) -> Beatgrid {
        Beatgrid {
            beats: beat_marks(&self.anchors, self.duration_ms),
            anchors: self.anchors.clone(),
            source: self.sources.first().cloned(),
            locked: self.locked,
        }
    }
}

impl Library {
    /// The grid of track `id`, with every beat up to the track's end.
    pub fn beatgrid(&self, id: TrackId) -> Result<Beatgrid> {
        let conn = self.conn()?;
        Ok(read_grid(&conn, id)?.view())
    }

    /// Applies `edit` to track `id`'s grid and stores the result as `manual`
    /// rows. Fails with [`Error::GridLocked`] on a locked grid (except
    /// unlocking). A reset without a stored base fails with
    /// [`Error::NoGridBase`]; see [`Library::edit_beatgrid_with`].
    pub fn edit_beatgrid(&self, id: TrackId, edit: GridEdit) -> Result<Beatgrid> {
        self.edit_beatgrid_with(id, edit, || None)
    }

    /// [`Library::edit_beatgrid`]; `analysis` gives reks's analysed grid
    /// for a reset when the track has no stored base (edited before
    /// migration 0013). It is only called for a reset.
    pub fn edit_beatgrid_with(
        &self,
        id: TrackId,
        edit: GridEdit,
        analysis: impl FnOnce() -> Option<Vec<GridAnchor>>,
    ) -> Result<Beatgrid> {
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
            let base = match (&edit, &before.base) {
                (GridEdit::Reset { .. }, None) => analysis().map(|anchors| GridBase {
                    source: Some(SOURCE_ANALYSIS.into()),
                    anchors,
                }),
                (_, base) => base.clone(),
            };
            let after = apply_edit(
                id,
                &before.anchors,
                &edit,
                base.as_ref().map(|b| b.anchors.as_slice()),
            )?;
            match (&edit, base) {
                (GridEdit::Reset { section: None }, Some(base)) => {
                    store_rows(&tx, id, &base.anchors, base.source.as_deref())?;
                    set_base(&tx, id, None)?;
                    if let Some(source) = base.source.as_deref() {
                        store_bpm(&tx, id, &base.anchors, before.duration_ms, source)?;
                    }
                }
                _ => {
                    let edited = before.sources.iter().any(|s| s == SOURCE_MANUAL);
                    if before.base.is_none() && !edited {
                        set_base(
                            &tx,
                            id,
                            Some(&GridBase {
                                source: before.sources.first().cloned(),
                                anchors: before.anchors.clone(),
                            }),
                        )?;
                    }
                    store_rows(&tx, id, &after, Some(SOURCE_MANUAL))?;
                    store_bpm(&tx, id, &after, before.duration_ms, SOURCE_MANUAL)?;
                }
            }
        }
        let after = read_grid(&tx, id)?.view();
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

/// Replaces the track's rows; no rows for `source = None` with no anchors.
fn store_rows(
    conn: &Connection,
    id: TrackId,
    anchors: &[GridAnchor],
    source: Option<&str>,
) -> Result<()> {
    conn.execute("DELETE FROM beatgrid WHERE track_id = ?1", [id])?;
    let mut ins = conn.prepare(
        "INSERT INTO beatgrid (track_id, position_ms, bpm_x100, meter_num, meter_den,
                               beat_in_bar, source)
         VALUES (?1, ?2, ?3, ?4, 4, ?5, ?6)",
    )?;
    for a in anchors {
        ins.execute(params![
            id,
            a.position_ms as f64,
            a.bpm_x100,
            a.beats_per_bar,
            a.beat_in_bar,
            source.unwrap_or(SOURCE_MANUAL)
        ])?;
    }
    Ok(())
}

/// The track's BPM follows the grid's longest section.
fn store_bpm(
    conn: &Connection,
    id: TrackId,
    anchors: &[GridAnchor],
    duration_ms: i64,
    source: &str,
) -> Result<()> {
    if let Some(bpm) = main_tempo(anchors, duration_ms) {
        conn.execute(
            "UPDATE track SET bpm_x100 = ?2, bpm_source = ?3, updated_at = ?4
             WHERE id = ?1 AND (bpm_x100 IS NOT ?2 OR bpm_source IS NOT ?3)",
            params![id, bpm, source, crate::timefmt::now_utc()],
        )?;
    }
    Ok(())
}

fn set_base(conn: &Connection, id: TrackId, base: Option<&GridBase>) -> Result<()> {
    let json = base
        .map(serde_json::to_string)
        .transpose()
        .map_err(|e| Error::InvalidField {
            field: "grid base",
            reason: e.to_string(),
        })?;
    conn.execute(
        "UPDATE track SET grid_base = ?2 WHERE id = ?1",
        params![id, json],
    )?;
    Ok(())
}

fn read_grid(conn: &Connection, id: TrackId) -> Result<Stored> {
    let (locked, duration_ms, base): (bool, Option<i64>, Option<String>) = conn
        .query_row(
            "SELECT grid_locked, duration_ms, grid_base FROM track WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?
        .ok_or(Error::TrackNotFound(id))?;
    let base = base.and_then(|json| match serde_json::from_str(&json) {
        Ok(b) => Some(b),
        Err(e) => {
            tracing::warn!(track = id, error = %e, "unreadable grid_base; ignored");
            None
        }
    });
    let mut stmt = conn.prepare(
        "SELECT position_ms, bpm_x100, meter_num, beat_in_bar, source
         FROM beatgrid WHERE track_id = ?1 ORDER BY position_ms, id",
    )?;
    let rows = stmt
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
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let (anchors, sources) = rows.into_iter().unzip();
    Ok(Stored {
        anchors,
        sources,
        locked,
        duration_ms: duration_ms.unwrap_or(0),
        base,
    })
}

#[cfg(test)]
mod tests;
