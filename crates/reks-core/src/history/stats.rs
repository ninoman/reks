//! Derived play counts (Phase 2.5 task 7, D23) and the session edits that
//! change them.
//!
//! ```text
//! play_count(t)  = baseline_play_count(t) + track entries of t in included sessions
//! last_played(t) = max(baseline_last_played(t), played_on of included sessions holding t)
//! ```
//!
//! Included means `excluded = 0`; divider and unmatched entries never count.
//! `track.play_count` / `last_played` are a cache of this, written only by
//! [`recompute`], so sorting and filtering keep using the existing columns.
//! Every write that changes the inputs (history import, linking an entry,
//! the practice toggle, deleting a session, a rekordbox XML import)
//! recomputes the tracks it touched in the same transaction.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::SessionId;
use crate::db::Library;
use crate::error::{Error, Result};
use crate::scan::TrackId;
use crate::timefmt::now_utc;

/// Which tracks [`Library::recompute_play_stats`] recomputes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "ids")]
pub enum TrackSet {
    All,
    Some(Vec<TrackId>),
}

/// A session edit; `None` fields are left alone.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionPatch {
    /// Trimmed; must not be empty.
    pub name: Option<String>,
    /// `Some(None)` clears it.
    pub venue: Option<Option<String>>,
    pub notes: Option<Option<String>>,
    /// Practice. Setting it stores reason `user`; clearing it clears the reason.
    pub excluded: Option<bool>,
}

/// The cache columns from the count rules, for `id = ?1` (or every track
/// when the `WHERE` is dropped). `max(a, b)` with a NULL is NULL in SQLite,
/// hence the `''` stand-in.
const RECOMPUTE_SQL: &str = "
UPDATE track SET
  play_count = baseline_play_count + (
    SELECT count(*) FROM play_session_entry e JOIN play_session s ON s.id = e.session_id
    WHERE e.track_id = track.id AND e.item_kind = 'track' AND s.excluded = 0),
  last_played = nullif(max(coalesce(baseline_last_played, ''), coalesce((
    SELECT max(s.played_on) FROM play_session_entry e JOIN play_session s ON s.id = e.session_id
    WHERE e.track_id = track.id AND e.item_kind = 'track' AND s.excluded = 0), '')), '')";

/// Rewrites the play count and last played of `ids` from the baseline and
/// the included sessions. Runs on whatever transaction `conn` is in.
pub(crate) fn recompute(conn: &Connection, ids: &TrackSet) -> Result<usize> {
    let n = match ids {
        TrackSet::All => conn.execute(RECOMPUTE_SQL, [])?,
        TrackSet::Some(ids) => {
            let mut stmt = conn.prepare_cached(&format!("{RECOMPUTE_SQL} WHERE id = ?1"))?;
            let mut n = 0;
            for id in ids {
                n += stmt.execute([id])?;
            }
            n
        }
    };
    tracing::debug!(tracks = n, "play stats recomputed");
    Ok(n)
}

/// The distinct tracks a session's entries point at.
fn session_tracks(conn: &Connection, session: SessionId) -> Result<Vec<TrackId>> {
    Ok(conn
        .prepare(
            "SELECT DISTINCT track_id FROM play_session_entry
             WHERE session_id = ?1 AND track_id IS NOT NULL",
        )?
        .query_map([session], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?)
}

fn session_exists(conn: &Connection, session: SessionId) -> Result<()> {
    let found: Option<i64> = conn
        .query_row("SELECT 1 FROM play_session WHERE id = ?1", [session], |r| {
            r.get(0)
        })
        .optional()?;
    match found {
        Some(_) => Ok(()),
        None => Err(Error::SessionNotFound(session)),
    }
}

fn trimmed(v: Option<String>) -> Option<String> {
    v.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty())
}

impl Library {
    /// Recomputes the cached play count and last played of `ids` (D23).
    pub fn recompute_play_stats(&self, ids: TrackSet) -> Result<()> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        recompute(&tx, &ids)?;
        tx.commit()?;
        Ok(())
    }

    /// Edits a session; toggling `excluded` (Practice) recomputes its tracks.
    pub fn session_update(&self, id: SessionId, patch: SessionPatch) -> Result<()> {
        let name = match patch.name {
            None => None,
            Some(n) => match trimmed(Some(n)) {
                Some(n) => Some(n),
                None => {
                    return Err(Error::InvalidField {
                        field: "name",
                        reason: "must not be empty".into(),
                    })
                }
            },
        };
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        session_exists(&tx, id)?;
        let now = now_utc();
        if let Some(name) = name {
            tx.execute(
                "UPDATE play_session SET name = ?2 WHERE id = ?1",
                params![id, name],
            )?;
        }
        if let Some(venue) = patch.venue {
            tx.execute(
                "UPDATE play_session SET venue = ?2 WHERE id = ?1",
                params![id, trimmed(venue)],
            )?;
        }
        if let Some(notes) = patch.notes {
            tx.execute(
                "UPDATE play_session SET notes = ?2 WHERE id = ?1",
                params![id, trimmed(notes)],
            )?;
        }
        if let Some(excluded) = patch.excluded {
            let changed = tx.execute(
                "UPDATE play_session SET excluded = ?2, excluded_reason = ?3
                 WHERE id = ?1 AND excluded IS NOT ?2",
                params![id, excluded, excluded.then_some("user")],
            )?;
            if changed > 0 {
                recompute(&tx, &TrackSet::Some(session_tracks(&tx, id)?))?;
            }
        }
        tx.execute(
            "UPDATE play_session SET updated_at = ?2 WHERE id = ?1",
            params![id, now],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Deletes a session and its entries; its tracks fall back to what the
    /// other sessions and the baseline give.
    pub fn session_delete(&self, id: SessionId) -> Result<()> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        session_exists(&tx, id)?;
        let tracks = session_tracks(&tx, id)?;
        tx.execute("DELETE FROM play_session_entry WHERE session_id = ?1", [id])?;
        tx.execute("DELETE FROM play_session WHERE id = ?1", [id])?;
        recompute(&tx, &TrackSet::Some(tracks))?;
        tx.commit()?;
        tracing::info!(session = id, "play session deleted");
        Ok(())
    }
}
