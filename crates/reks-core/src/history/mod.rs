//! Play history import (Phase 2.5 task 6, D23): stick sessions → `play_session`.
//!
//! [`scan`] is the dry run: it matches every entry to a library track (see
//! `matcher`) and decides per session, by fingerprint, whether it is new,
//! already imported (skip) or a longer version of an imported one (extend). It
//! writes nothing. [`import`] applies a plan in one transaction.
//!
//! The stick is read elsewhere: reks-core can't depend on `reks-export-rb`,
//! which owns the PDB reader, so the caller converts its `PlayHistory` into a
//! [`SourceHistory`] (`PlayHistory::to_source_history`).
//!
//! Fingerprint: SHA-256 over source, volume serial, source name and the
//! ordered entry stick paths, each length-prefixed. Same fingerprint: skip.
//! Same source, serial (both absent counts as same) and name with the stored
//! entry paths a strict prefix of the new ones: extend that session. Whether
//! players append to a history playlist across mounts is **UNVERIFIED**
//! (HIST3); the rule only avoids counting the first part twice if they do.
//!
//! Exclusion defaults (D23): a session dated on or before the baseline date,
//! or with no date (HIST1 off), is preselected as excluded (`pre_baseline`);
//! [`ImportChoices`] overrides that per session (`user`). Play counts are not
//! recomputed here; that is task 7.

mod matcher;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::db::Library;
use crate::error::{Error, Result};
use crate::scan::TrackId;
use crate::timefmt::now_utc;

use matcher::Matcher;

/// `play_session.source` for legacy `export.pdb` history (H1).
pub const SOURCE_DEVICE_LIBRARY: &str = "device_library";

/// `play_session.id`.
pub type SessionId = i64;

/// The history on one stick or backup, as the reader hands it in.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceHistory {
    /// `None` when the OS gave no serial: no manifest matches, fallbacks only.
    pub volume_serial: Option<String>,
    /// In stick order.
    pub sessions: Vec<SourceSession>,
}

/// One history playlist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceSession {
    /// As on the stick, e.g. `HISTORY 001`.
    pub source_name: String,
    /// `YYYY-MM-DD`, `None` while HIST1 is undecoded.
    pub played_on: Option<String>,
    /// In player order.
    pub entries: Vec<SourceEntry>,
}

/// One history entry's track fields; all `None` when the stick's track row
/// is missing.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceEntry {
    /// `/Contents/...`, as in the PDB track row.
    pub stick_path: Option<String>,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub duration_ms: Option<i64>,
}

/// `play_session_entry.item_kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EntryKind {
    Track,
    /// A reks divider (known from the export manifest); never counted.
    Divider,
}

impl EntryKind {
    fn as_sql(self) -> &'static str {
        match self {
            Self::Track => "track",
            Self::Divider => "divider",
        }
    }
}

/// How an entry found its track.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MatchKind {
    /// `export_manifest` (volume serial, stick path): exact.
    Manifest,
    /// The only library track whose path ends like the stick path.
    PathSuffix,
    /// The only library track with this artist, title and duration ±1 s.
    Metadata,
}

/// One entry of a planned session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedEntry {
    /// 0-based, in player order.
    pub position: u32,
    pub kind: EntryKind,
    /// `None`: unmatched (kept, never counted, linkable later) or a divider.
    pub track_id: Option<TrackId>,
    pub matched_by: Option<MatchKind>,
    pub raw: SourceEntry,
}

/// Why a session imports excluded unless the user says otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DefaultExclusion {
    /// Played on or before the baseline date: may be in rekordbox's count.
    PreBaseline,
    /// No date (HIST1): it may be pre-baseline too.
    UnknownDate,
}

/// Why a session is not imported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum SkipReason {
    /// The library holds a session with this fingerprint.
    AlreadyImported { session_id: SessionId },
    /// An earlier session of the same source has this fingerprint.
    DuplicateInSource,
}

/// What [`import`] does with a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum SessionAction {
    New,
    Skip(SkipReason),
    /// Append the entries from `existing_entries` on to `session_id`.
    Extend {
        session_id: SessionId,
        existing_entries: u32,
    },
}

/// One stick session in the dry run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedSession {
    pub source_name: String,
    pub played_on: Option<String>,
    /// Hex SHA-256; the key of [`ImportChoices::excluded`].
    pub fingerprint: String,
    pub action: SessionAction,
    /// For [`SessionAction::New`]; an extended session keeps its state.
    pub default_exclusion: Option<DefaultExclusion>,
    /// Every entry for New and Extend (Extend adds those from
    /// `existing_entries` on); empty for Skip.
    pub entries: Vec<PlannedEntry>,
}

impl PlannedSession {
    /// Entries [`import`] would add.
    pub fn new_entries(&self) -> &[PlannedEntry] {
        match self.action {
            SessionAction::New => &self.entries,
            SessionAction::Skip(_) => &[],
            SessionAction::Extend {
                existing_entries, ..
            } => self
                .entries
                .get(existing_entries as usize..)
                .unwrap_or_default(),
        }
    }
}

/// The dry run of a history import. Pure data; nothing is written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryImportPlan {
    pub source: String,
    pub volume_serial: Option<String>,
    /// `history.baseline_date` when the plan was made.
    pub baseline_date: Option<String>,
    pub sessions: Vec<PlannedSession>,
}

/// The user's choices in the import dialog.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportChoices {
    /// Fingerprint → Practice (excluded). Absent: the plan's default.
    pub excluded: HashMap<String, bool>,
}

/// What [`import`] did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub created: Vec<SessionId>,
    pub extended: Vec<SessionId>,
    pub skipped: usize,
    /// Entries written, all sessions.
    pub entries: usize,
    /// Of those, track entries with no library track.
    pub unmatched: usize,
}

/// The dry run: matches every entry and decides each session's action.
/// Read-only.
pub fn scan(history: &SourceHistory, lib: &Library) -> Result<HistoryImportPlan> {
    let conn = lib.conn()?;
    let matcher = Matcher::load(&conn)?;
    let serial = history.volume_serial.as_deref().filter(|s| !s.is_empty());
    let baseline_date = baseline_date(&conn)?;
    let mut seen = HashSet::new();
    let mut sessions = Vec::with_capacity(history.sessions.len());
    for s in &history.sessions {
        let fingerprint = fingerprint(SOURCE_DEVICE_LIBRARY, serial, s);
        let action = if !seen.insert(fingerprint.clone()) {
            SessionAction::Skip(SkipReason::DuplicateInSource)
        } else if let Some(id) = session_by_fingerprint(&conn, &fingerprint)? {
            SessionAction::Skip(SkipReason::AlreadyImported { session_id: id })
        } else if let Some((id, n)) = extendable(&conn, serial, s)? {
            SessionAction::Extend {
                session_id: id,
                existing_entries: n,
            }
        } else {
            SessionAction::New
        };
        let entries = if matches!(action, SessionAction::Skip(_)) {
            Vec::new()
        } else {
            let mut out = Vec::with_capacity(s.entries.len());
            for (i, e) in s.entries.iter().enumerate() {
                let m = matcher.find(&conn, serial, e)?;
                out.push(PlannedEntry {
                    position: u32::try_from(i).unwrap_or(u32::MAX),
                    kind: m.kind,
                    track_id: m.track_id,
                    matched_by: m.matched_by,
                    raw: e.clone(),
                });
            }
            out
        };
        let default_exclusion = match (&action, &s.played_on) {
            (SessionAction::New, None) => Some(DefaultExclusion::UnknownDate),
            (SessionAction::New, Some(d))
                if baseline_date.as_deref().is_some_and(|b| d.as_str() <= b) =>
            {
                Some(DefaultExclusion::PreBaseline)
            }
            _ => None,
        };
        sessions.push(PlannedSession {
            source_name: s.source_name.clone(),
            played_on: s.played_on.clone(),
            fingerprint,
            action,
            default_exclusion,
            entries,
        });
    }
    Ok(HistoryImportPlan {
        source: SOURCE_DEVICE_LIBRARY.into(),
        volume_serial: serial.map(str::to_owned),
        baseline_date,
        sessions,
    })
}

/// Applies `plan` in one transaction. A plan the library has moved past
/// (an extended session changed since the scan) fails with
/// [`Error::StaleHistoryPlan`] and writes nothing; a new session whose
/// fingerprint was imported meanwhile is skipped.
pub fn import(
    plan: &HistoryImportPlan,
    choices: &ImportChoices,
    lib: &Library,
) -> Result<ImportReport> {
    let now = now_utc();
    let mut report = ImportReport::default();
    let mut conn = lib.conn()?;
    let tx = conn.transaction()?;
    for s in &plan.sessions {
        match s.action {
            SessionAction::Skip(_) => report.skipped += 1,
            SessionAction::New => {
                if session_by_fingerprint(&tx, &s.fingerprint)?.is_some() {
                    report.skipped += 1;
                    continue;
                }
                let excluded = choices
                    .excluded
                    .get(&s.fingerprint)
                    .copied()
                    .unwrap_or(s.default_exclusion.is_some());
                let reason = match (excluded, s.default_exclusion) {
                    (false, _) => None,
                    (true, Some(_)) => Some("pre_baseline"),
                    (true, None) => Some("user"),
                };
                tx.execute(
                    "INSERT INTO play_session (source, fingerprint, volume_serial, source_name,
                         name, played_on, excluded, excluded_reason, imported_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?4, ?5, ?6, ?7, ?8, ?8)",
                    params![
                        plan.source,
                        s.fingerprint,
                        plan.volume_serial,
                        s.source_name,
                        s.played_on,
                        excluded,
                        reason,
                        now
                    ],
                )?;
                let id = tx.last_insert_rowid();
                insert_entries(&tx, id, s.new_entries(), &mut report)?;
                report.created.push(id);
            }
            SessionAction::Extend {
                session_id,
                existing_entries,
            } => {
                let stored = stored_paths(&tx, session_id)?;
                if stored.len() != existing_entries as usize {
                    return Err(Error::StaleHistoryPlan(format!(
                        "session {session_id} has {} entries, the plan expected {existing_entries}",
                        stored.len()
                    )));
                }
                tx.execute(
                    "UPDATE play_session SET fingerprint = ?2, updated_at = ?3 WHERE id = ?1",
                    params![session_id, s.fingerprint, now],
                )?;
                insert_entries(&tx, session_id, s.new_entries(), &mut report)?;
                report.extended.push(session_id);
            }
        }
    }
    tx.commit()?;
    tracing::info!(
        created = report.created.len(),
        extended = report.extended.len(),
        skipped = report.skipped,
        entries = report.entries,
        unmatched = report.unmatched,
        "play history imported"
    );
    Ok(report)
}

impl Library {
    /// Links one session entry to a library track ("Locate…"). Dividers
    /// can't be linked.
    pub fn session_link_entry(
        &self,
        session: SessionId,
        position: u32,
        track: TrackId,
    ) -> Result<()> {
        let conn = self.conn()?;
        let kind: Option<String> = conn
            .query_row(
                "SELECT item_kind FROM play_session_entry WHERE session_id = ?1 AND position = ?2",
                params![session, position],
                |r| r.get(0),
            )
            .optional()?;
        match kind.as_deref() {
            None => return Err(Error::SessionEntryNotFound { session, position }),
            Some("track") => {}
            Some(_) => return Err(Error::NotATrackEntry { session, position }),
        }
        let exists: bool = conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM track WHERE id = ?1)",
            [track],
            |r| r.get(0),
        )?;
        if !exists {
            return Err(Error::TrackNotFound(track));
        }
        conn.execute(
            "UPDATE play_session_entry SET track_id = ?3 WHERE session_id = ?1 AND position = ?2",
            params![session, position, track],
        )?;
        conn.execute(
            "UPDATE play_session SET updated_at = ?2 WHERE id = ?1",
            params![session, now_utc()],
        )?;
        Ok(())
    }
}

fn insert_entries(
    tx: &Transaction<'_>,
    session: SessionId,
    entries: &[PlannedEntry],
    report: &mut ImportReport,
) -> Result<()> {
    let mut stmt = tx.prepare(
        "INSERT INTO play_session_entry (session_id, position, item_kind, track_id,
             raw_title, raw_artist, raw_path, raw_duration_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?;
    for e in entries {
        stmt.execute(params![
            session,
            e.position,
            e.kind.as_sql(),
            e.track_id,
            e.raw.title,
            e.raw.artist,
            e.raw.stick_path,
            e.raw.duration_ms
        ])?;
        report.entries += 1;
        if e.kind == EntryKind::Track && e.track_id.is_none() {
            report.unmatched += 1;
        }
    }
    Ok(())
}

/// Hex SHA-256 of source, serial, name and the ordered stick paths, each
/// length-prefixed (a missing path hashes differently from an empty one).
fn fingerprint(source: &str, serial: Option<&str>, s: &SourceSession) -> String {
    let mut h = Sha256::new();
    let mut field = |v: Option<&str>| match v {
        None => h.update([0u8]),
        Some(v) => {
            h.update([1u8]);
            h.update((v.len() as u64).to_le_bytes());
            h.update(v.as_bytes());
        }
    };
    field(Some(source));
    field(serial);
    field(Some(&s.source_name));
    for e in &s.entries {
        field(e.stick_path.as_deref());
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

fn session_by_fingerprint(conn: &Connection, fingerprint: &str) -> Result<Option<SessionId>> {
    Ok(conn
        .query_row(
            "SELECT id FROM play_session WHERE fingerprint = ?1",
            [fingerprint],
            |r| r.get(0),
        )
        .optional()?)
}

/// A stored session of the same source, serial and name whose entry paths
/// are a strict prefix of `s`'s; the longest one if several.
fn extendable(
    conn: &Connection,
    serial: Option<&str>,
    s: &SourceSession,
) -> Result<Option<(SessionId, u32)>> {
    let ids: Vec<SessionId> = conn
        .prepare(
            "SELECT id FROM play_session
             WHERE source = ?1 AND volume_serial IS ?2 AND source_name = ?3 ORDER BY id",
        )?
        .query_map(params![SOURCE_DEVICE_LIBRARY, serial, s.source_name], |r| {
            r.get(0)
        })?
        .collect::<rusqlite::Result<_>>()?;
    let new: Vec<Option<&str>> = s.entries.iter().map(|e| e.stick_path.as_deref()).collect();
    let mut best: Option<(SessionId, u32)> = None;
    for id in ids {
        let stored = stored_paths(conn, id)?;
        let is_prefix =
            stored.len() < new.len() && stored.iter().zip(&new).all(|(a, b)| a.as_deref() == *b);
        let n = u32::try_from(stored.len()).unwrap_or(u32::MAX);
        if is_prefix && best.is_none_or(|(_, b)| n > b) {
            best = Some((id, n));
        }
    }
    Ok(best)
}

fn stored_paths(conn: &Connection, session: SessionId) -> Result<Vec<Option<String>>> {
    Ok(conn
        .prepare("SELECT raw_path FROM play_session_entry WHERE session_id = ?1 ORDER BY position")?
        .query_map([session], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?)
}

fn baseline_date(conn: &Connection) -> Result<Option<String>> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT value FROM setting WHERE key = 'history.baseline_date'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    Ok(raw.and_then(|v| match serde_json::from_str::<String>(&v) {
        Ok(d) => Some(d),
        Err(e) => {
            tracing::warn!(value = %v, error = %e, "unreadable history.baseline_date");
            None
        }
    }))
}
