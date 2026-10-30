//! Editing a track's cues from a deck (Phase 5 task 17): set a hot cue in an
//! empty slot, add a memory cue or memory loop, rename or delete any of
//! them, and mark a memory loop active (playback enters it by itself).
//!
//! Rows written here use the model the import and the exporters already use
//! ([`Cue`]): `kind` 0 (a cue) or 4 (a loop, with `end_ms`), `slot` 0..7 for
//! hot cues A–H and NULL for memory cues and loops, `color_rgb` from
//! [`HOT_CUE_PALETTE`] (hot cues only; the rekordbox XML has no memory cue
//! colour), `source = 'manual'`, so a rekordbox XML re-import leaves them
//! alone. A rename or the active flag keeps a row's source: an imported cue
//! stays the import's (and a re-import replaces it, flag included).

use rusqlite::{params, OptionalExtension, Transaction};

use crate::db::Library;
use crate::error::{Error, Result};
use crate::scan::TrackId;
use crate::source::{CueKind, DataSource};
use crate::track::Cue;

/// Hot cue slots A–H.
pub const HOT_CUE_SLOTS: u8 = 8;

/// Longest cue name in characters. rekordbox's limit is not documented; the
/// name goes to the stick as the ANLZ cue comment, so keep it short.
pub const MAX_CUE_NAME_CHARS: usize = 64;

/// A memory cue this close (ms) to an existing one is the same cue.
const SAME_CUE_MS: f64 = 1.0;

/// Colours new hot cues take, in order: entries of rekordbox's 16-colour hot
/// cue palette (the RGB rekordbox shows), so an export maps each one to its
/// own player colour (`reks_anlz::cue::hot_colour`).
pub const HOT_CUE_PALETTE: [u32; 8] = [
    0x28E214, // green
    0x305AFF, // blue
    0xE62828, // red
    0xC3AF04, // yellow
    0xB432FF, // purple
    0x00E0FF, // cyan
    0xE0641B, // orange
    0xFF127B, // pink
];

/// The colour for a new hot cue in `slot`: the first palette colour no other
/// hot cue on the track has, else the slot's own.
pub fn next_hot_cue_colour(slot: u8, used: &[Option<u32>]) -> u32 {
    HOT_CUE_PALETTE
        .iter()
        .copied()
        .find(|c| !used.contains(&Some(*c)))
        .unwrap_or(HOT_CUE_PALETTE[usize::from(slot) % HOT_CUE_PALETTE.len()])
}

/// A cue name as stored: trimmed, `None` when empty.
fn clean_name(name: Option<&str>) -> Result<Option<String>> {
    let Some(t) = name.map(str::trim).filter(|t| !t.is_empty()) else {
        return Ok(None);
    };
    if t.chars().count() > MAX_CUE_NAME_CHARS {
        return Err(Error::InvalidField {
            field: "name",
            reason: format!("longer than {MAX_CUE_NAME_CHARS} characters"),
        });
    }
    if t.chars().any(char::is_control) {
        return Err(Error::InvalidField {
            field: "name",
            reason: "contains a control character".into(),
        });
    }
    Ok(Some(t.to_owned()))
}

/// Checks `start_ms` lies in the track; returns nothing to write otherwise.
fn check_start(tx: &Transaction, id: TrackId, start_ms: f64) -> Result<()> {
    let duration: Option<i64> = tx
        .query_row("SELECT duration_ms FROM track WHERE id = ?1", [id], |r| {
            r.get(0)
        })
        .optional()?
        .ok_or(Error::TrackNotFound(id))?;
    let past_end = duration.is_some_and(|d| start_ms > d as f64);
    if !start_ms.is_finite() || start_ms < 0.0 || past_end {
        return Err(Error::InvalidField {
            field: "position",
            reason: format!("{start_ms} ms is outside the track"),
        });
    }
    Ok(())
}

fn read_cue(tx: &Transaction, id: TrackId, cue: i64) -> Result<Cue> {
    tx.query_row(
        "SELECT id, slot, kind, start_ms, end_ms, name, color_rgb, active_loop FROM cue
         WHERE id = ?1 AND track_id = ?2",
        params![cue, id],
        |r| {
            Ok(Cue {
                id: r.get(0)?,
                slot: r.get(1)?,
                kind: r.get(2)?,
                start_ms: r.get(3)?,
                end_ms: r.get(4)?,
                name: r.get(5)?,
                color_rgb: r.get(6)?,
                active_loop: r.get(7)?,
            })
        },
    )
    .optional()?
    .ok_or(Error::CueNotFound(cue))
}

fn touch(tx: &Transaction, id: TrackId) -> Result<()> {
    tx.execute(
        "UPDATE track SET updated_at = ?2 WHERE id = ?1",
        params![id, crate::timefmt::now_utc()],
    )?;
    Ok(())
}

impl Library {
    /// Sets hot cue `slot` (0..7 = A..H) of track `id` at `start_ms`, in the
    /// next palette colour. Refused when the slot holds any cue already:
    /// a set hot cue is deleted first, never overwritten.
    pub fn set_hot_cue(&self, id: TrackId, slot: u8, start_ms: f64) -> Result<Cue> {
        if slot >= HOT_CUE_SLOTS {
            return Err(Error::InvalidField {
                field: "slot",
                reason: format!("{slot} is not a hot cue slot (0..{HOT_CUE_SLOTS})"),
            });
        }
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        check_start(&tx, id, start_ms)?;
        let mut stmt =
            tx.prepare("SELECT slot, color_rgb FROM cue WHERE track_id = ?1 AND slot IS NOT NULL")?;
        let hot: Vec<(u8, Option<u32>)> = stmt
            .query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        drop(stmt);
        if hot.iter().any(|(s, _)| *s == slot) {
            return Err(Error::InvalidField {
                field: "slot",
                reason: format!("hot cue {} is already set", char::from(b'A' + slot)),
            });
        }
        let used: Vec<Option<u32>> = hot.iter().map(|(_, c)| *c).collect();
        let colour = next_hot_cue_colour(slot, &used);
        tx.execute(
            "INSERT INTO cue (track_id, slot, kind, start_ms, color_rgb, source)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, slot, CueKind::Cue, start_ms, colour, DataSource::Manual],
        )?;
        let cue = read_cue(&tx, id, tx.last_insert_rowid())?;
        touch(&tx, id)?;
        tx.commit()?;
        Ok(cue)
    }

    /// Adds a memory cue to track `id` at `start_ms`, or a memory loop when
    /// `end_ms` is given (MEMORY pressed while a loop plays). The same cue
    /// or loop already there (start and end within 1 ms) is returned as it
    /// is instead (pressing MEMORY twice).
    pub fn add_memory_cue(&self, id: TrackId, start_ms: f64, end_ms: Option<f64>) -> Result<Cue> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        check_start(&tx, id, start_ms)?;
        if let Some(end) = end_ms {
            check_start(&tx, id, end)?;
            if end - start_ms < SAME_CUE_MS {
                return Err(Error::InvalidField {
                    field: "position",
                    reason: format!("a loop must end after it starts ({start_ms}–{end} ms)"),
                });
            }
        }
        let kind = if end_ms.is_some() {
            CueKind::Loop
        } else {
            CueKind::Cue
        };
        let same: Option<i64> = tx
            .query_row(
                "SELECT id FROM cue WHERE track_id = ?1 AND slot IS NULL AND kind = ?4
                 AND abs(start_ms - ?2) < ?3 AND abs(coalesce(end_ms, 0) - ?5) < ?3
                 ORDER BY id LIMIT 1",
                params![id, start_ms, SAME_CUE_MS, kind, end_ms.unwrap_or(0.0)],
                |r| r.get(0),
            )
            .optional()?;
        let cue_id = match same {
            Some(c) => c,
            None => {
                tx.execute(
                    "INSERT INTO cue (track_id, slot, kind, start_ms, end_ms, source)
                     VALUES (?1, NULL, ?2, ?3, ?4, ?5)",
                    params![id, kind, start_ms, end_ms, DataSource::Manual],
                )?;
                touch(&tx, id)?;
                tx.last_insert_rowid()
            }
        };
        let cue = read_cue(&tx, id, cue_id)?;
        tx.commit()?;
        Ok(cue)
    }

    /// MEMORY pressed at `at_ms`: while a loop plays (`looping` is its
    /// `(start, end)`), it saves the loop (rekordbox), else a cue at `at_ms`.
    pub fn save_memory(&self, id: TrackId, at_ms: f64, looping: Option<(f64, f64)>) -> Result<Cue> {
        match looping {
            Some((start, end)) => self.add_memory_cue(id, start, Some(end)),
            None => self.add_memory_cue(id, at_ms, None),
        }
    }

    /// Renames cue `cue` of track `id`; `None` or blank clears the name.
    pub fn rename_cue(&self, id: TrackId, cue: i64, name: Option<&str>) -> Result<Cue> {
        let name = clean_name(name)?;
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        read_cue(&tx, id, cue)?;
        tx.execute("UPDATE cue SET name = ?2 WHERE id = ?1", params![cue, name])?;
        touch(&tx, id)?;
        let out = read_cue(&tx, id, cue)?;
        tx.commit()?;
        Ok(out)
    }

    /// Marks memory loop `cue` of track `id` active (playback enters it by
    /// itself when the playhead reaches its start) or not. Any number may be
    /// marked in the library; an export takes the earliest (players hold one
    /// per track). Refused for anything but a memory loop.
    pub fn set_active_loop(&self, id: TrackId, cue: i64, on: bool) -> Result<Cue> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        let c = read_cue(&tx, id, cue)?;
        if c.slot.is_some() || c.kind != CueKind::Loop.code() || c.end_ms.is_none() {
            return Err(Error::InvalidField {
                field: "activeLoop",
                reason: "only a memory loop can be active".into(),
            });
        }
        tx.execute(
            "UPDATE cue SET active_loop = ?2 WHERE id = ?1",
            params![cue, on],
        )?;
        touch(&tx, id)?;
        let out = read_cue(&tx, id, cue)?;
        tx.commit()?;
        Ok(out)
    }

    /// Deletes cue `cue` of track `id` (hot cue, memory cue or loop). Returns
    /// the hot cue slot it freed, if it was one.
    pub fn delete_cue(&self, id: TrackId, cue: i64) -> Result<Option<u8>> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        let slot = read_cue(&tx, id, cue)?.slot;
        tx.execute("DELETE FROM cue WHERE id = ?1", [cue])?;
        touch(&tx, id)?;
        tx.commit()?;
        Ok(slot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_hot_cues_take_the_first_unused_palette_colour() {
        assert_eq!(next_hot_cue_colour(0, &[]), HOT_CUE_PALETTE[0]);
        assert_eq!(
            next_hot_cue_colour(3, &[Some(HOT_CUE_PALETTE[0]), None, Some(0x123456)]),
            HOT_CUE_PALETTE[1]
        );
        let all: Vec<Option<u32>> = HOT_CUE_PALETTE.iter().map(|&c| Some(c)).collect();
        assert_eq!(
            next_hot_cue_colour(10, &all),
            HOT_CUE_PALETTE[2],
            "slot wraps"
        );
    }

    #[test]
    fn names_are_trimmed_and_checked() {
        assert_eq!(clean_name(Some("  Drop ")).ok(), Some(Some("Drop".into())));
        assert_eq!(clean_name(Some("   ")).ok(), Some(None));
        assert_eq!(clean_name(None).ok(), Some(None));
        assert!(clean_name(Some(&"x".repeat(MAX_CUE_NAME_CHARS + 1))).is_err());
        assert!(clean_name(Some("a\u{7}b")).is_err());
    }
}
