//! Reading imported sessions for the History UI (Phase 2.5 task 9): the
//! session list, one session in play order, and the sessions a track was
//! played in.

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use super::{EntryKind, SessionId};
use crate::db::Library;
use crate::error::{Error, Result};
use crate::scan::TrackId;

/// Which sessions [`Library::sessions`] lists.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SessionQuery {
    /// `Some(true)`: practice (excluded) only; `Some(false)`: counted only.
    pub excluded: Option<bool>,
}

/// One session in the list. Newest first: by `played_on`, undated last
/// (newest import first among those).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRow {
    pub id: SessionId,
    pub name: String,
    /// As on the stick, e.g. `HISTORY 001`.
    pub source_name: String,
    /// `YYYY-MM-DD`; `None` until HIST1 is decoded.
    pub played_on: Option<String>,
    pub venue: Option<String>,
    /// Practice: left out of play counts.
    pub excluded: bool,
    /// `user` or `pre_baseline`.
    pub excluded_reason: Option<String>,
    /// Track entries (dividers left out).
    pub tracks: u32,
    /// Track entries with no library track.
    pub unmatched: u32,
    pub imported_at: String,
}

/// One session with its entries in play order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionDetail {
    pub session: SessionRow,
    pub notes: Option<String>,
    pub volume_serial: Option<String>,
    pub entries: Vec<SessionEntryRow>,
}

/// One entry. Matched entries show the library track's fields; the stick's
/// own (`raw_*`) are kept for unmatched ones and for "Locate…".
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionEntryRow {
    pub position: u32,
    pub kind: EntryKind,
    pub track_id: Option<TrackId>,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub duration_ms: Option<i64>,
    pub bpm_x100: Option<i64>,
    pub key: Option<String>,
    pub raw_title: Option<String>,
    pub raw_artist: Option<String>,
    pub raw_path: Option<String>,
    pub raw_duration_ms: Option<i64>,
}

/// A session a track was played in ("Played in" on the track panel).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRef {
    pub id: SessionId,
    pub name: String,
    pub played_on: Option<String>,
    pub venue: Option<String>,
    /// Practice: these plays are not counted.
    pub excluded: bool,
    /// Times the track appears in the session.
    pub plays: u32,
}

const ROW_SQL: &str = "
SELECT s.id, s.name, s.source_name, s.played_on, s.venue, s.excluded, s.excluded_reason,
       (SELECT count(*) FROM play_session_entry e
         WHERE e.session_id = s.id AND e.item_kind = 'track'),
       (SELECT count(*) FROM play_session_entry e
         WHERE e.session_id = s.id AND e.item_kind = 'track' AND e.track_id IS NULL),
       s.imported_at, s.notes, s.volume_serial
FROM play_session s";

const ORDER_SQL: &str =
    "ORDER BY s.played_on IS NULL, s.played_on DESC, s.imported_at DESC, s.id DESC";

fn session_row(r: &Row<'_>) -> rusqlite::Result<SessionRow> {
    Ok(SessionRow {
        id: r.get(0)?,
        name: r.get(1)?,
        source_name: r.get(2)?,
        played_on: r.get(3)?,
        venue: r.get(4)?,
        excluded: r.get(5)?,
        excluded_reason: r.get(6)?,
        tracks: r.get(7)?,
        unmatched: r.get(8)?,
        imported_at: r.get(9)?,
    })
}

fn entry_kind(s: &str) -> EntryKind {
    if s == "divider" {
        EntryKind::Divider
    } else {
        EntryKind::Track
    }
}

fn entries(conn: &Connection, id: SessionId) -> Result<Vec<SessionEntryRow>> {
    Ok(conn
        .prepare(
            "SELECT e.position, e.item_kind, e.track_id, t.title, t.artist, t.duration_ms,
                    t.bpm_x100, t.key, e.raw_title, e.raw_artist, e.raw_path, e.raw_duration_ms
             FROM play_session_entry e LEFT JOIN track t ON t.id = e.track_id
             WHERE e.session_id = ?1 ORDER BY e.position",
        )?
        .query_map([id], |r| {
            let kind: String = r.get(1)?;
            Ok(SessionEntryRow {
                position: r.get(0)?,
                kind: entry_kind(&kind),
                track_id: r.get(2)?,
                title: r.get(3)?,
                artist: r.get(4)?,
                duration_ms: r.get(5)?,
                bpm_x100: r.get(6)?,
                key: r.get(7)?,
                raw_title: r.get(8)?,
                raw_artist: r.get(9)?,
                raw_path: r.get(10)?,
                raw_duration_ms: r.get(11)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?)
}

impl Library {
    /// Imported sessions, newest first.
    pub fn sessions(&self, q: &SessionQuery) -> Result<Vec<SessionRow>> {
        let conn = self.conn()?;
        let rows = conn
            .prepare(&format!(
                "{ROW_SQL} WHERE ?1 IS NULL OR s.excluded = ?1 {ORDER_SQL}"
            ))?
            .query_map([q.excluded], session_row)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// One session and its entries in play order.
    pub fn session(&self, id: SessionId) -> Result<SessionDetail> {
        let conn = self.conn()?;
        let head = conn
            .query_row(&format!("{ROW_SQL} WHERE s.id = ?1"), [id], |r| {
                Ok((session_row(r)?, r.get(10)?, r.get(11)?))
            })
            .optional()?;
        let Some((session, notes, volume_serial)) = head else {
            return Err(Error::SessionNotFound(id));
        };
        Ok(SessionDetail {
            session,
            notes,
            volume_serial,
            entries: entries(&conn, id)?,
        })
    }

    /// The sessions `id` was played in, newest first, practice ones included.
    pub fn track_sessions(&self, id: TrackId) -> Result<Vec<SessionRef>> {
        let conn = self.conn()?;
        let rows = conn
            .prepare(&format!(
                "SELECT s.id, s.name, s.played_on, s.venue, s.excluded, count(*)
                 FROM play_session s JOIN play_session_entry e ON e.session_id = s.id
                 WHERE e.track_id = ?1 AND e.item_kind = 'track'
                 GROUP BY s.id {ORDER_SQL}"
            ))?
            .query_map(params![id], |r| {
                Ok(SessionRef {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    played_on: r.get(2)?,
                    venue: r.get(3)?,
                    excluded: r.get(4)?,
                    plays: r.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }
}
