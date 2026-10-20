//! Play history in an `exportLibrary.db` (Phase 4 task 11, D23 H2): the
//! `history` and `history_content` rows players logged, as
//! `reks_core::history` sessions with [`HistoryLibrary::OneLibrary`].
//!
//! Columns are the rekordbox 7.2.19 reference's (`data/7.2.19/schema.sql`):
//! `history(history_id, sequenceNo, name, attribute, history_id_parent)` and
//! `history_content(history_id, content_id, sequenceNo)`. The reference's
//! history is empty, so how players fill them is **UNVERIFIED**; the reader
//! assumes what `playlist` / `playlist_content` show:
//!
//! - Entries play in `sequenceNo` order (row order breaks ties); each joins
//!   `content` on `content_id` for path, title, length (whole seconds, 0 =
//!   unknown) and `artist_id_artist`'s name. A missing `content` row leaves
//!   the entry's fields `None` (unmatched later), as in `export.pdb`.
//! - A `history` row with no entries that is a folder (`attribute` 1, as for
//!   playlists, or another row's `history_id_parent`) is not a session. Any
//!   row with entries is one, so no logged play is dropped.
//! - Sessions come in `history_id` order, named as stored (`HISTORY #<id>`
//!   when NULL). Folder names are not part of the name.
//! - No column holds a date, so every session is undated.
//!
//! Reading a stick never writes it: [`OneLibraryReader`] opens a temp copy.
//! A Phase 2 backup is read only with its `manifest.json` (serial from
//! there) and when the database and its `-wal`/`-shm` hash as recorded.
//!
//! [`read_both`] reads a stick's two libraries for a history import (the
//! CLI and the app): `export.pdb` through the Phase 2.5 reader, then this
//! database when there is a key. The export guard reads this one through
//! the writer's plan instead (`OneLibraryExport::plan`).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use reks_core::history::{HistoryLibrary, SourceEntry, SourceHistory, SourceSession};
use reks_export_rb::history::{
    check_backup_copy, read_backup_manifest, BackupReport, HistoryError, HistoryReadOptions,
    HistorySource,
};
use reks_export_rb::onelibrary::DB_PATH;
use reks_export_rb::validate::locate;
use rusqlite::Connection;
use serde::Serialize;

use crate::error::{Error, Result};
use crate::reader::OneLibraryReader;

/// A `history` row.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Head {
    id: i64,
    name: Option<String>,
    attribute: Option<i64>,
    parent: Option<i64>,
}

/// A `history_content` row joined to its track, in play order.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Played {
    history_id: i64,
    entry: SourceEntry,
}

/// Why [`read_play_history`] read nothing.
#[derive(Debug, thiserror::Error)]
pub enum HistoryReadError {
    #[error(transparent)]
    Database(#[from] Error),
    /// The backup's manifest is missing or does not vouch for the file.
    #[error(transparent)]
    Backup(#[from] HistoryError),
}

/// What became of the source's `exportLibrary.db` in [`read_both`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "status"
)]
pub enum OneLibraryStatus {
    Absent,
    Read {
        sessions: usize,
    },
    /// No key, a key that does not open it, a backup copy that does not
    /// match its manifest, a damaged file: its sessions are left out.
    NotRead {
        reason: String,
    },
}

impl OneLibraryReader {
    /// The sessions in this database (module docs), in `history_id` order.
    pub fn play_history(&self) -> Result<Vec<SourceSession>> {
        read_sessions(self.conn())
    }
}

/// Reads the play history in `src`'s `exportLibrary.db` with `key`. No
/// database → no sessions. `stick_serial` is a stick's volume serial; a
/// backup's comes from its manifest.
pub fn read_play_history(
    src: &HistorySource,
    key: &str,
    stick_serial: Option<String>,
) -> Result<SourceHistory, HistoryReadError> {
    let (root, volume_serial, manifest) = match src {
        HistorySource::Stick(root) => (root, stick_serial, None),
        HistorySource::Backup(dir) => {
            let report = read_backup_manifest(dir)?;
            (dir, report.volume_serial.clone(), Some(report))
        }
    };
    let mut history = SourceHistory {
        volume_serial,
        sessions: Vec::new(),
    };
    let Some(db) = locate(root, DB_PATH).map_err(|source| Error::Io {
        path: root.clone(),
        source,
    })?
    else {
        return Ok(history);
    };
    if let Some(report) = &manifest {
        check_backup(report, root, &db)?;
    }
    history.sessions = OneLibraryReader::open(&db, key)?.play_history()?;
    Ok(history)
}

/// Both stick libraries' sessions in `src`: `export.pdb`'s (an error stops
/// the read, as for the Phase 2.5 import), then `exportLibrary.db`'s with
/// `key` (any failure only leaves them out, see [`OneLibraryStatus`]).
pub fn read_both(
    src: &HistorySource,
    opts: &HistoryReadOptions,
    key: Option<&str>,
) -> Result<(SourceHistory, OneLibraryStatus), HistoryReadError> {
    let mut history = reks_export_rb::history::read_play_history(src, opts)?.to_source_history();
    let root = match src {
        HistorySource::Stick(p) | HistorySource::Backup(p) => p,
    };
    let present = locate(root, DB_PATH)
        .map_err(|source| Error::Io {
            path: root.clone(),
            source,
        })?
        .is_some();
    let status = match key {
        _ if !present => OneLibraryStatus::Absent,
        None => OneLibraryStatus::NotRead {
            reason: "no OneLibrary key (Settings → Experimental)".into(),
        },
        Some(key) => match read_play_history(src, key, opts.stick_serial.clone()) {
            Ok(one) => {
                let sessions = one.sessions.len();
                history.sessions.extend(one.sessions);
                OneLibraryStatus::Read { sessions }
            }
            Err(e) => {
                tracing::warn!(error = %e, "exportLibrary.db history not read");
                OneLibraryStatus::NotRead {
                    reason: e.to_string(),
                }
            }
        },
    };
    Ok((history, status))
}

/// The database must be in the manifest; its `-wal` and `-shm` too when
/// present (rekordbox keeps most rows in `-wal`), and all must hash as
/// recorded. A listed `-wal` missing from the folder is an error.
fn check_backup(report: &BackupReport, root: &Path, db: &Path) -> Result<(), HistoryReadError> {
    for suffix in ["", "-wal", "-shm"] {
        let mut path = db.as_os_str().to_owned();
        path.push(suffix);
        let path = PathBuf::from(path);
        match std::fs::read(&path) {
            Ok(bytes) => check_backup_copy(report, root, &path, &bytes)?,
            Err(e) if !suffix.is_empty() && e.kind() == std::io::ErrorKind::NotFound => {
                let rel = backup_rel(root, &path);
                if report.files.iter().any(|f| f.rel_path == rel) {
                    return Err(HistoryError::Unreadable {
                        path,
                        message: "listed in the backup manifest but missing".into(),
                    }
                    .into());
                }
            }
            Err(source) => return Err(Error::Io { path, source }.into()),
        }
    }
    Ok(())
}

/// `path` relative to the backup folder, `/`-separated, as the manifest has it.
fn backup_rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

fn read_sessions(conn: &Connection) -> Result<Vec<SourceSession>> {
    let heads = conn
        .prepare(
            "SELECT history_id, name, attribute, history_id_parent FROM history \
             ORDER BY history_id",
        )?
        .query_map([], |r| {
            Ok(Head {
                id: r.get(0)?,
                name: r.get(1)?,
                attribute: r.get(2)?,
                parent: r.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let played = conn
        .prepare(
            "SELECT h.history_id, c.path, c.title, a.name, c.length \
             FROM history_content h \
             LEFT JOIN content c ON c.content_id = h.content_id \
             LEFT JOIN artist a ON a.artist_id = c.artist_id_artist \
             ORDER BY h.history_id, h.sequenceNo, h.rowid",
        )?
        .query_map([], |r| {
            let length: Option<i64> = r.get(4)?;
            Ok(Played {
                history_id: r.get(0)?,
                entry: SourceEntry {
                    stick_path: r.get(1)?,
                    title: r.get(2)?,
                    artist: r.get(3)?,
                    duration_ms: length.filter(|&s| s > 0).map(|s| s * 1000),
                },
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(sessions(heads, played))
}

/// Groups entries under their sessions (module docs). Entries whose
/// `history_id` has no `history` row form a session of their own.
fn sessions(mut heads: Vec<Head>, played: Vec<Played>) -> Vec<SourceSession> {
    let mut by_id: HashMap<i64, Vec<SourceEntry>> = HashMap::new();
    for p in played {
        by_id.entry(p.history_id).or_default().push(p.entry);
    }
    let parents: HashSet<i64> = heads.iter().filter_map(|h| h.parent).collect();
    let known: HashSet<i64> = heads.iter().map(|h| h.id).collect();
    heads.extend(
        by_id
            .keys()
            .filter(|id| !known.contains(id))
            .map(|&id| Head {
                id,
                name: None,
                attribute: None,
                parent: None,
            }),
    );
    heads.sort_by_key(|h| h.id);
    heads
        .into_iter()
        .filter_map(|h| {
            let entries = by_id.remove(&h.id).unwrap_or_default();
            let folder = h.attribute == Some(1) || parents.contains(&h.id);
            if entries.is_empty() && folder {
                return None;
            }
            Some(SourceSession {
                library: HistoryLibrary::OneLibrary,
                source_name: h.name.unwrap_or_else(|| format!("HISTORY #{}", h.id)),
                played_on: None,
                entries,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests;
