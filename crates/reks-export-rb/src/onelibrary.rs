//! Seam to the OneLibrary writer (Phase 4 task 8, D14): the planner asks a
//! [`OneLibraryExport`] what writing `PIONEER/rekordbox/exportLibrary.db`
//! next to the legacy export would do, so the dry run lists it, and
//! [`crate::apply`] has it write the file once the new `export.pdb` is in
//! place ([`OneLibraryExport::write`]).
//!
//! The writer lives in `reks-export-onelibrary`, which depends on this crate
//! for [`ExportPlan`]; so this crate holds only the trait, and the app picks
//! the implementation (`reks_export_onelibrary::OneLibraryWriter`) and hands
//! it to `apply` with the key ([`crate::apply::OneLibraryEnv`]). Tests here
//! use [`NoWriter`], which plans the file but blocks the export.
//!
//! The key comes from [`resolve_key`]: `REKS_ONELIBRARY_KEY`, else the key
//! built into release builds (Phase 4 key policy). It is never logged and
//! never part of a plan.

use std::path::{Path, PathBuf};

use reks_core::history::SourceSession;
use reks_core::OneLibraryKey;
use serde::{Deserialize, Serialize};

use crate::plan::{on_stick, ExportPlan, FileOp, OpReason, PlanError, Problem, Warning};

/// The OneLibrary database, from the stick root.
pub const DB_PATH: &str = "/PIONEER/rekordbox/exportLibrary.db";

/// The environment variable that overrides the built-in key (CI, testing,
/// builds from source).
pub const KEY_ENV: &str = "REKS_ONELIBRARY_KEY";

/// What the OneLibrary step of an export does. Part of the dry run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OneLibraryPlan {
    /// `exportLibrary.db` itself: `dst_rel` [`DB_PATH`], `bytes` an estimate
    /// (not counted in [`ExportPlan::bytes_to_write`], like `export.pdb`),
    /// reason [`OpReason::OneLibrary`].
    pub write: FileOp,
    /// rekordbox's auxiliary files on the stick that the write removes
    /// ([`auxiliary_files`]), reason [`OpReason::OneLibraryAuxiliary`].
    pub removes: Vec<FileOp>,
    /// The `exportLibrary.db` already on the stick, which the write replaces.
    pub existing: ExistingLibrary,
    pub warnings: Vec<Warning>,
    /// Blockers; the planner adds them to [`ExportPlan::errors`].
    pub errors: Vec<Problem>,
}

/// The `exportLibrary.db` on the stick before the export.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ExistingLibrary {
    Absent,
    /// Opened with the key: the play history players logged in it.
    /// Sessions the library has not imported make the export guard ask for
    /// a choice; Import now imports them (Phase 4 task 11).
    Read {
        sessions: u32,
        entries: u32,
        /// The sessions themselves ([`plan_onelibrary`](crate::plan::plan_onelibrary)
        /// scans them). Not serialised, like [`ExportPlan::history_import`].
        #[serde(skip)]
        history: Vec<SourceSession>,
    },
    /// Present but not read (no reader, wrong key, corrupt). It may hold
    /// history: the Phase 2 safeguard backs it up by default.
    NotRead {
        reason: String,
    },
}

/// Plans and writes the OneLibrary database for an export. Implemented by
/// the `reks-export-onelibrary` writer and by [`NoWriter`].
pub trait OneLibraryExport: Send + Sync {
    /// What writing `exportLibrary.db` for `plan` would do. Reads the stick
    /// only; `key` opens the existing database, if any.
    fn plan(&self, plan: &ExportPlan, key: &OneLibraryKey) -> Result<OneLibraryPlan, PlanError>;

    /// The `exportLibrary.db` under `device_root`, opened with `key`, for
    /// the export guard when this export writes no new one (D23: its
    /// history is scanned whenever a key resolves). Reads the stick only.
    fn existing(
        &self,
        device_root: &Path,
        key: &OneLibraryKey,
    ) -> Result<ExistingLibrary, PlanError>;

    /// Writes `exportLibrary.db` for `plan`, encrypted with `key`, at
    /// [`DB_PATH`] under `device_root`. Only [`crate::apply`] calls it, after
    /// the new `export.pdb` is in place and rekordbox's auxiliary files are
    /// gone, so the two libraries describe the same tracks.
    ///
    /// Writes nothing else on the stick but a temp file beside [`DB_PATH`],
    /// and replaces the file atomically: on `Err` the stick has no new
    /// database (baken removed the old one with the new `export.pdb`). Like
    /// baken for its own files, it leaves no macOS `._` AppleDouble file
    /// beside it (baken issues #192, #196). Errors never contain the key.
    fn write(
        &self,
        plan: &ExportPlan,
        key: &OneLibraryKey,
        device_root: &Path,
    ) -> Result<OneLibraryWritten, OneLibraryWriteError>;
}

/// What [`OneLibraryExport::write`] put on the stick.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OneLibraryWritten {
    /// Tracks in the new `exportLibrary.db`.
    pub tracks: usize,
    /// Size of the file.
    pub bytes: u64,
}

/// Why [`OneLibraryExport::write`] wrote no database.
#[derive(Debug, thiserror::Error)]
pub enum OneLibraryWriteError {
    /// This build has no OneLibrary writer ([`NoWriter`]).
    #[error("this build cannot write exportLibrary.db")]
    Unavailable,
    /// Building the database failed (schema, rows, encryption, integrity check).
    #[error("building exportLibrary.db failed: {0}")]
    Build(String),
    /// The stick refused the write.
    #[error("cannot write {}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// The key the export uses: `REKS_ONELIBRARY_KEY`, else the built-in one
/// ([`reks_core::secret::builtin`]).
pub fn resolve_key() -> Option<OneLibraryKey> {
    let env = std::env::var(KEY_ENV).ok().map(zeroize::Zeroizing::new);
    pick_key(
        env.as_deref().map(String::as_str),
        reks_core::secret::builtin::key,
    )
}

/// [`resolve_key`]'s order, on values already read.
pub fn pick_key(
    env: Option<&str>,
    builtin: impl FnOnce() -> Option<OneLibraryKey>,
) -> Option<OneLibraryKey> {
    env.and_then(OneLibraryKey::new).or_else(builtin)
}

/// rekordbox's files next to `exportLibrary.db` that a new one must not sit
/// beside ([`baken_export::ONELIBRARY_FILES`] without the database itself:
/// its `-wal`/`-shm`, which SQLite would replay into ours, and
/// `exportExt.pdb`), as found on the stick under `root`.
pub fn auxiliary_files(root: &Path) -> Result<Vec<FileOp>, PlanError> {
    let mut found = library_files(root, OpReason::OneLibraryAuxiliary)?;
    found.retain(|f| f.dst_rel != DB_PATH);
    Ok(found)
}

/// Every OneLibrary file on the stick under `root`
/// ([`baken_export::ONELIBRARY_FILES`]: `exportLibrary.db`, its
/// `-wal`/`-shm` and `exportExt.pdb`), as deletes for `reason`. baken
/// removes them whenever it writes no `exportLibrary.db` itself, which is
/// always (D14).
pub fn library_files(root: &Path, reason: OpReason) -> Result<Vec<FileOp>, PlanError> {
    let rb_dir = DB_PATH.rsplit_once('/').map_or("", |(dir, _)| dir);
    let mut found = Vec::new();
    for name in baken_export::ONELIBRARY_FILES {
        let rel = format!("{rb_dir}/{name}");
        let path = on_stick(root, &rel);
        match std::fs::metadata(&path) {
            Ok(meta) if meta.is_file() => found.push(FileOp {
                src: None,
                dst_rel: rel,
                bytes: meta.len(),
                track_id: None,
                reason,
            }),
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => return Err(PlanError::Stick { path, source }),
        }
    }
    Ok(found)
}

/// The planned `exportLibrary.db` write for `plan`, with `bytes` as given.
pub fn db_write(bytes: u64) -> FileOp {
    FileOp {
        src: None,
        dst_rel: DB_PATH.to_owned(),
        bytes,
        track_id: None,
        reason: OpReason::OneLibrary,
    }
}

/// Stand-in for a writer, for tests and plans that write no OneLibrary:
/// lists the file and what it would remove, and blocks the export with
/// [`Problem::OneLibraryUnavailable`]; `write` refuses.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoWriter;

impl OneLibraryExport for NoWriter {
    fn plan(&self, plan: &ExportPlan, _key: &OneLibraryKey) -> Result<OneLibraryPlan, PlanError> {
        let root = plan.device.mount_point.as_path();
        let existing = if plan.history.onelibrary_db {
            ExistingLibrary::NotRead {
                reason: "this build has no OneLibrary reader".to_owned(),
            }
        } else {
            ExistingLibrary::Absent
        };
        Ok(OneLibraryPlan {
            write: db_write(0),
            removes: auxiliary_files(root)?,
            existing,
            warnings: Vec::new(),
            errors: vec![Problem::OneLibraryUnavailable],
        })
    }

    fn existing(
        &self,
        _device_root: &Path,
        _key: &OneLibraryKey,
    ) -> Result<ExistingLibrary, PlanError> {
        Ok(ExistingLibrary::NotRead {
            reason: "this build has no OneLibrary reader".to_owned(),
        })
    }

    fn write(
        &self,
        _plan: &ExportPlan,
        _key: &OneLibraryKey,
        _device_root: &Path,
    ) -> Result<OneLibraryWritten, OneLibraryWriteError> {
        Err(OneLibraryWriteError::Unavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environment_wins_over_the_builtin() {
        let builtin = || OneLibraryKey::new("builtin");
        let none = || None;
        assert_eq!(pick_key(Some(" env "), builtin).unwrap().expose(), "env");
        assert_eq!(pick_key(Some("  "), builtin).unwrap().expose(), "builtin");
        assert_eq!(pick_key(None, builtin).unwrap().expose(), "builtin");
        assert!(pick_key(Some("  "), none).is_none());
        assert!(pick_key(None, none).is_none());
    }

    #[test]
    fn auxiliary_files_lists_what_is_there_but_not_the_database() {
        let dir = tempfile::tempdir().unwrap();
        let rb = dir.path().join("PIONEER/rekordbox");
        std::fs::create_dir_all(&rb).unwrap();
        for (name, body) in [
            ("exportLibrary.db", &b"db"[..]),
            ("exportLibrary.db-wal", b"wal!"),
            ("exportExt.pdb", b"ext"),
        ] {
            std::fs::write(rb.join(name), body).unwrap();
        }
        let found = auxiliary_files(dir.path()).unwrap();
        let rels: Vec<(&str, u64)> = found
            .iter()
            .map(|f| (f.dst_rel.as_str(), f.bytes))
            .collect();
        assert_eq!(
            rels,
            [
                ("/PIONEER/rekordbox/exportLibrary.db-wal", 4),
                ("/PIONEER/rekordbox/exportExt.pdb", 3)
            ]
        );
        assert!(found
            .iter()
            .all(|f| f.reason == OpReason::OneLibraryAuxiliary));
        assert!(auxiliary_files(&dir.path().join("empty"))
            .unwrap()
            .is_empty());
    }
}
