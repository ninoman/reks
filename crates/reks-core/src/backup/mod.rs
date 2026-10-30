//! Automatic library backups and restore (D36).
//!
//! The library DB holds the only copy of cues, grids, tags, playlists and
//! history, so it is snapshotted with SQLite's online backup API (never by
//! copying the file while WAL is live) into `<app data>/library-backups/`:
//! on app start (at most daily), before a migration, before bulk operations
//! (which call `Library::snapshot_before` themselves) and on request. [`Library::restore_backup`] takes a
//! snapshot of the current library first, so a restore can be undone.

mod file;
mod name;
mod retention;

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db::{self, create_dir, Library, SCHEMA_VERSION};
use crate::error::{Error, Result};
use crate::settings;
use crate::timefmt;

/// Folder name inside the app data dir.
pub const BACKUP_DIR: &str = "library-backups";

/// Snapshots newer than this make the start-up snapshot unnecessary.
const DAILY_MS: i64 = 86_400_000;

/// Default for the `backup.keep` setting: how many recent snapshots stay.
pub const DEFAULT_KEEP: u32 = 10;
/// Bounds of the `backup.keep` setting.
pub const KEEP_RANGE: std::ops::RangeInclusive<u32> = 1..=100;

/// Why a snapshot was taken; part of its file name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BackupReason {
    /// App start, at most once a day.
    Startup,
    /// Before a schema migration changed the library.
    Migration,
    /// The user asked ("Back up now").
    Manual,
    /// Before a restore replaced the library: the way back.
    BeforeRestore,
    /// Before a rekordbox XML import.
    XmlImport,
    /// Before a file conversion that replaces the originals.
    ConvertReplace,
    /// Before a play history import.
    HistoryImport,
    /// Before one metadata edit applied to many tracks.
    BulkEdit,
    /// Before adopting a foreign stick's playlists.
    Adopt,
    /// Before relocating missing files.
    Relocate,
    /// Before reading My Tags from comments or rewriting comments from tags.
    CommentTags,
}

impl BackupReason {
    const ALL: [Self; 11] = [
        Self::Startup,
        Self::Migration,
        Self::Manual,
        Self::BeforeRestore,
        Self::XmlImport,
        Self::ConvertReplace,
        Self::HistoryImport,
        Self::BulkEdit,
        Self::Adopt,
        Self::Relocate,
        Self::CommentTags,
    ];

    pub(crate) fn slug(self) -> &'static str {
        match self {
            Self::Startup => "startup",
            Self::Migration => "migration",
            Self::Manual => "manual",
            Self::BeforeRestore => "before-restore",
            Self::XmlImport => "xml-import",
            Self::ConvertReplace => "convert-replace",
            Self::HistoryImport => "history-import",
            Self::BulkEdit => "bulk-edit",
            Self::Adopt => "adopt",
            Self::Relocate => "relocate",
            Self::CommentTags => "comment-tags",
        }
    }

    pub(crate) fn from_slug(slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.slug() == slug)
    }
}

/// What a checked snapshot holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupContents {
    /// SQLite `user_version` of the snapshot.
    pub schema_version: i64,
    pub tracks: i64,
    /// Playlists, not counting folders.
    pub playlists: i64,
}

/// One snapshot in the backup folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    /// File name; the id [`Library::restore_backup`] takes.
    pub name: String,
    /// UTC ISO-8601.
    pub taken_at: String,
    pub reason: BackupReason,
    pub size_bytes: u64,
    /// `None` when `problem` is set.
    pub contents: Option<BackupContents>,
    /// Why the snapshot cannot be restored (failed its check); `None` if it can.
    pub problem: Option<String>,
    /// Made by a newer reks than this one: listed, not restorable.
    pub too_new: bool,
}

/// The newest backup that failed, until one succeeds again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupFailure {
    pub reason: BackupReason,
    /// UTC ISO-8601.
    pub at: String,
    pub message: String,
}

/// Backup state for the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupStatus {
    pub dir: Option<PathBuf>,
    pub last_failure: Option<BackupFailure>,
}

/// Where backups go and how the last attempt fared; owned by [`Library`].
#[derive(Debug)]
pub(crate) struct BackupState {
    dir: Option<PathBuf>,
    last_failure: Mutex<Option<BackupFailure>>,
}

impl BackupState {
    /// `dir` is created when the first snapshot is written; `None` for
    /// libraries that cannot be backed up (in memory).
    pub(crate) fn new(dir: Option<PathBuf>) -> Self {
        Self {
            dir,
            last_failure: Mutex::new(None),
        }
    }

    fn dir(&self) -> Result<&Path> {
        self.dir.as_deref().ok_or(Error::NoBackupDir)
    }

    fn record(&self, reason: BackupReason, outcome: &Result<BackupInfo>) {
        let failure = outcome.as_ref().err().map(|err| BackupFailure {
            reason,
            at: timefmt::now_utc(),
            message: err.to_string(),
        });
        if let Some(f) = &failure {
            tracing::warn!(reason = reason.slug(), err = %f.message, "library backup failed");
        }
        match self.last_failure.lock() {
            Ok(mut slot) => *slot = failure,
            Err(poisoned) => *poisoned.into_inner() = failure,
        }
    }

    /// Notes the outcome of a snapshot taken before this state was attached.
    pub(crate) fn with_failure(self, failure: Option<BackupFailure>) -> Self {
        if let Ok(mut slot) = self.last_failure.lock() {
            *slot = failure;
        }
        self
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_millis()).ok())
        .unwrap_or(0)
}

/// Writes a verified snapshot of `conn` into `dir` and prunes old ones.
/// A snapshot that fails its check is deleted, never left among the good ones.
pub(crate) fn take(
    conn: &Connection,
    dir: &Path,
    reason: BackupReason,
    keep: u32,
) -> Result<BackupInfo> {
    create_dir(dir)?;
    // Names carry milliseconds; step past one that is taken (two snapshots
    // in the same millisecond) rather than overwrite it.
    let mut taken_ms = now_ms();
    while dir.join(name::file_name(taken_ms, reason)).exists() {
        taken_ms += 1;
    }
    let name = name::file_name(taken_ms, reason);
    let partial = dir.join(format!(".{name}.partial"));
    let target = dir.join(&name);

    let contents = file::write(conn, &partial)
        .and_then(|()| file::check(&partial, &name, file::Depth::Full))
        .inspect_err(|_| {
            if let Err(err) = file::remove(&partial) {
                tracing::debug!(%err, "partial snapshot not removed");
            }
        })?;
    file::rename(&partial, &target)?;
    let info = BackupInfo {
        taken_at: iso(taken_ms),
        reason,
        size_bytes: file::size(&target)?,
        contents: Some(contents.into()),
        problem: None,
        too_new: false,
        name,
    };
    if let Err(err) = prune(dir, keep, taken_ms) {
        tracing::warn!(%err, "old library backups not pruned");
    }
    Ok(info)
}

impl From<file::Contents> for BackupContents {
    fn from(c: file::Contents) -> Self {
        Self {
            schema_version: c.schema_version,
            tracks: c.tracks,
            playlists: c.playlists,
        }
    }
}

fn iso(ms: i64) -> String {
    timefmt::iso8601_utc(UNIX_EPOCH + std::time::Duration::from_millis(ms.unsigned_abs()))
}

/// The snapshots in `dir` as `(file name, parsed name)`, newest first.
fn snapshot_files(dir: &Path) -> Result<Vec<(String, name::ParsedName)>> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(Error::Io {
                path: dir.to_path_buf(),
                source,
            })
        }
    };
    let mut out: Vec<_> = entries
        .flatten()
        .filter_map(|e| {
            let file_name = e.file_name().into_string().ok()?;
            let parsed = name::parse(&file_name)?;
            Some((file_name, parsed))
        })
        .collect();
    out.sort_by_key(|(_, p)| std::cmp::Reverse(p.taken_ms));
    Ok(out)
}

fn prune(dir: &Path, keep: u32, now_ms: i64) -> Result<()> {
    let files = snapshot_files(dir)?;
    let taken: Vec<i64> = files.iter().map(|(_, p)| p.taken_ms).collect();
    for i in retention::expired(&taken, now_ms, keep as usize) {
        file::remove(&dir.join(&files[i].0))?;
    }
    Ok(())
}

impl Library {
    /// Snapshots the library now, for `reason`. Call before a bulk operation.
    /// Records a failure for [`Library::backup_status`] and returns it;
    /// whether a failed snapshot stops the operation is the caller's call.
    pub fn snapshot(&self, reason: BackupReason) -> Result<BackupInfo> {
        let outcome = self.snapshot_inner(reason);
        self.backup.record(reason, &outcome);
        outcome
    }

    /// The snapshot every bulk operation takes before it writes (D36), so
    /// each front end gets it. A failure is recorded and logged, not
    /// returned: a full disk should not lock the user out of an import. A
    /// library with no backup folder (in memory) takes none.
    pub(crate) fn snapshot_before(&self, reason: BackupReason) {
        if self.backup.dir.is_none() {
            return;
        }
        if let Err(err) = self.snapshot(reason) {
            tracing::warn!(%err, reason = reason.slug(), "continuing without a pre-operation backup");
        }
    }

    fn snapshot_inner(&self, reason: BackupReason) -> Result<BackupInfo> {
        let dir = self.backup.dir()?;
        let keep = self.backup_keep()?;
        let conn = self.conn()?;
        take(&conn, dir, reason, keep)
    }

    /// The start-up snapshot: only when the newest one is over a day old.
    /// `Ok(None)` when none was due.
    pub fn snapshot_if_due(&self) -> Result<Option<BackupInfo>> {
        let newest = snapshot_files(self.backup.dir()?)?
            .first()
            .map(|(_, p)| p.taken_ms);
        if newest.is_some_and(|t| now_ms() - t < DAILY_MS) {
            return Ok(None);
        }
        self.snapshot(BackupReason::Startup).map(Some)
    }

    /// The snapshots, newest first. Each is opened read-only and checked
    /// (`quick_check`) for its counts; a damaged one is listed with `problem`.
    pub fn backups(&self) -> Result<Vec<BackupInfo>> {
        let dir = self.backup.dir()?;
        snapshot_files(dir)?
            .into_iter()
            .map(|(file_name, parsed)| {
                let path = dir.join(&file_name);
                let checked = file::check(&path, &file_name, file::Depth::Quick);
                let (contents, problem) = match checked {
                    Ok(c) => (Some(BackupContents::from(c)), None),
                    Err(err) => (None, Some(err.to_string())),
                };
                Ok(BackupInfo {
                    size_bytes: file::size(&path)?,
                    taken_at: iso(parsed.taken_ms),
                    reason: parsed.reason,
                    too_new: contents.is_some_and(|c| c.schema_version > SCHEMA_VERSION),
                    contents,
                    problem,
                    name: file_name,
                })
            })
            .collect()
    }

    /// Where backups live and the last failure, if any.
    pub fn backup_status(&self) -> BackupStatus {
        let last_failure = match self.backup.last_failure.lock() {
            Ok(f) => f.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        };
        BackupStatus {
            dir: self.backup.dir.clone(),
            last_failure,
        }
    }

    /// Replaces the library with snapshot `name`, after snapshotting the
    /// current one (`before-restore`). Refused, with nothing changed, when the
    /// snapshot fails its integrity check, is not a reks library, comes from a
    /// newer schema, or the safety snapshot cannot be written. A snapshot from
    /// an older schema is migrated. The connection stays open; the caller
    /// reloads whatever it holds in memory (track ids may differ).
    pub fn restore_backup(&self, name: &str) -> Result<()> {
        let dir = self.backup.dir()?;
        // `parse` only accepts our own file names, so `name` cannot be a path.
        if name::parse(name).is_none() {
            return Err(Error::BackupNotFound(name.to_owned()));
        }
        let path = dir.join(name);
        if !path.is_file() {
            return Err(Error::BackupNotFound(name.to_owned()));
        }
        let contents = file::check(&path, name, file::Depth::Full)?;
        if contents.schema_version > SCHEMA_VERSION {
            return Err(Error::BackupTooNew {
                name: name.to_owned(),
                found: contents.schema_version,
                supported: SCHEMA_VERSION,
            });
        }
        self.snapshot(BackupReason::BeforeRestore)?;

        {
            let mut conn = self.conn()?;
            file::load_into(&mut conn, &path)?;
            db::migrate(&mut conn)?;
        }
        tracing::info!(backup = name, "library restored");
        if let Err(err) = self.use_stored_cache_dir() {
            tracing::warn!(%err, "restored cache dir unusable, keeping the current one");
        }
        Ok(())
    }

    fn backup_keep(&self) -> Result<u32> {
        let conn = self.conn()?;
        Ok(settings::backup_keep(&conn))
    }
}

/// The failure to carry into a new [`Library`] when a pre-migration snapshot failed.
pub(crate) fn failure_of(reason: BackupReason, err: &Error) -> BackupFailure {
    BackupFailure {
        reason,
        at: timefmt::now_utc(),
        message: err.to_string(),
    }
}

#[cfg(test)]
mod tests;
