//! The OneLibrary writer (task 6): [`OneLibraryWriter`] implements
//! [`OneLibraryExport`], which the planner and `apply` call.
//!
//! `write` reads the `export.pdb` baken just wrote, builds the database in a
//! local temp directory (encrypted from the start, WAL journal like
//! rekordbox's), checkpoints and closes it so it is one file, reopens it to
//! run `PRAGMA integrity_check` and compare every row with what was meant,
//! then copies it to a temp file beside `exportLibrary.db` on the stick,
//! fsyncs and renames it into place. The bytes are written with a plain
//! `write`, not `fs::copy`: macOS copies extended attributes, which FAT
//! keeps as `._` AppleDouble files (baken #192, #196).

use std::fs::File;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use reks_core::OneLibraryKey;
use reks_export_rb::onelibrary::{
    auxiliary_files, db_write, ExistingLibrary, OneLibraryExport, OneLibraryPlan,
    OneLibraryWriteError, OneLibraryWritten, DB_PATH,
};
use reks_export_rb::pdbread;
use reks_export_rb::plan::{ExportPlan, PlanError};
use reks_export_rb::validate::{locate, PDB_PATH};
use rusqlite::Connection;

use crate::build;
use crate::cipher;
use crate::error::{Error, Result};
use crate::reader::OneLibraryReader;
use crate::rows::OneLibrary;
use crate::schema::{self, RbVersion, PROFILE};
use crate::store;

/// Writes `exportLibrary.db` in rekordbox `version`'s layout.
#[derive(Debug, Clone, Copy)]
pub struct OneLibraryWriter {
    pub version: RbVersion,
}

impl Default for OneLibraryWriter {
    fn default() -> Self {
        Self {
            version: RbVersion::LATEST,
        }
    }
}

/// Size estimate for the dry run: the empty database with seed rows, plus
/// per track and per playlist entry (measured on our builds; see tests).
pub fn estimated_bytes(tracks: usize, entries: usize) -> u64 {
    const EMPTY: u64 = 26 * 4096;
    const PER_TRACK: u64 = 600;
    const PER_ENTRY: u64 = 16;
    EMPTY + PER_TRACK * tracks as u64 + PER_ENTRY * entries as u64
}

/// Builds `lib` into a new encrypted database file at `path` (which must
/// not exist), checkpointed into one file, and checks it reads back as
/// `lib`. Returns the file's size.
pub fn build_file(path: &Path, key: &str, version: RbVersion, lib: &OneLibrary) -> Result<u64> {
    {
        let mut conn = cipher::open_read_write(path, key, PROFILE)?;
        // rekordbox's file is in WAL mode; the header says so on ours too.
        let mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))?;
        if mode != "wal" {
            return Err(Error::Check(format!("journal mode {mode}, not wal")));
        }
        build_into(&mut conn, version, lib)?;
        conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
    }
    // Closing the last connection removes `-wal` and `-shm`.
    for suffix in ["-wal", "-shm"] {
        let mut aux = path.as_os_str().to_owned();
        aux.push(suffix);
        if Path::new(&aux).exists() {
            return Err(Error::Check(format!("{suffix} file left after close")));
        }
    }
    verify(path, key, lib)?;
    std::fs::metadata(path)
        .map(|m| m.len())
        .map_err(|source| Error::Io {
            path: path.to_owned(),
            source,
        })
}

/// Schema, seed rows and `lib` into an empty database.
pub fn build_into(conn: &mut Connection, version: RbVersion, lib: &OneLibrary) -> Result<()> {
    schema::create_empty(conn, version)?;
    store::insert(conn, lib)
}

fn verify(path: &Path, key: &str, lib: &OneLibrary) -> Result<()> {
    let reader = OneLibraryReader::open(path, key)?;
    let check = reader.integrity_check()?;
    if check != "ok" {
        return Err(Error::Check(format!("integrity check: {check}")));
    }
    if reader.library()? != *lib {
        return Err(Error::Check(
            "rows read back differ from rows written".into(),
        ));
    }
    Ok(())
}

impl OneLibraryWriter {
    /// The rows for the stick under `device_root`, from its `export.pdb`.
    pub fn rows(&self, device_root: &Path) -> Result<OneLibrary> {
        let pdb = pdbread::read_stick(device_root).map_err(|e| Error::Pdb(e.to_string()))?;
        Ok(build::from_pdb(&pdb, &reks_core::timefmt::today_utc()))
    }

    pub(crate) fn write_db(
        &self,
        key: &str,
        device_root: &Path,
    ) -> Result<OneLibraryWritten, OneLibraryWriteError> {
        let build_err = |e: Error| OneLibraryWriteError::Build(e.to_string());
        let lib = self.rows(device_root).map_err(build_err)?;
        let work = tempfile::tempdir().map_err(|source| OneLibraryWriteError::Io {
            path: std::env::temp_dir(),
            source,
        })?;
        let built = work.path().join("exportLibrary.db");
        build_file(&built, key, self.version, &lib).map_err(build_err)?;
        let bytes = std::fs::read(&built).map_err(|source| OneLibraryWriteError::Io {
            path: built.clone(),
            source,
        })?;
        let dst = destination(device_root)?;
        replace(&dst, &bytes)?;
        tracing::info!(
            tracks = lib.contents.len(),
            bytes = bytes.len(),
            "wrote exportLibrary.db"
        );
        Ok(OneLibraryWritten {
            tracks: lib.contents.len(),
            bytes: bytes.len() as u64,
        })
    }
}

/// `exportLibrary.db` beside the stick's `export.pdb` (same folder,
/// whatever its case on FAT).
fn destination(device_root: &Path) -> Result<PathBuf, OneLibraryWriteError> {
    let io = |source| OneLibraryWriteError::Io {
        path: device_root.to_owned(),
        source,
    };
    let pdb = locate(device_root, PDB_PATH).map_err(io)?.ok_or_else(|| {
        OneLibraryWriteError::Build("the stick has no export.pdb to describe".into())
    })?;
    let name = DB_PATH.rsplit('/').next().unwrap_or("exportLibrary.db");
    let dir = pdb.parent().unwrap_or(device_root);
    Ok(dir.join(name))
}

/// Writes `bytes` to a temp file beside `dst`, fsyncs it and renames it over
/// `dst`. On error the temp file is removed and `dst` is untouched.
fn replace(dst: &Path, bytes: &[u8]) -> Result<(), OneLibraryWriteError> {
    let mut tmp = dst.as_os_str().to_owned();
    tmp.push(".reks-tmp");
    let tmp = PathBuf::from(tmp);
    let io = |path: &Path| {
        let path = path.to_owned();
        move |source| OneLibraryWriteError::Io { path, source }
    };
    let result = (|| {
        let mut f = File::create(&tmp).map_err(io(&tmp))?;
        f.write_all(bytes).map_err(io(&tmp))?;
        f.sync_all().map_err(io(&tmp))?;
        drop(f);
        std::fs::rename(&tmp, dst).map_err(io(dst))?;
        sync_dir(dst)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Makes the rename durable (Unix; Windows has no directory handles to sync).
fn sync_dir(path: &Path) -> Result<(), OneLibraryWriteError> {
    #[cfg(unix)]
    if let Some(dir) = path.parent() {
        File::open(dir)
            .and_then(|d| d.sync_all())
            .map_err(|source| OneLibraryWriteError::Io {
                path: dir.to_owned(),
                source,
            })?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

/// What the stick's current `exportLibrary.db` holds, opened with `key`:
/// its play history sessions ([`crate::history`]).
pub fn existing(device_root: &Path, key: &str) -> Result<ExistingLibrary, PlanError> {
    let Some(path) = locate(device_root, DB_PATH).map_err(|source| PlanError::Stick {
        path: device_root.to_owned(),
        source,
    })?
    else {
        return Ok(ExistingLibrary::Absent);
    };
    Ok(
        match OneLibraryReader::open(&path, key).and_then(|r| r.play_history()) {
            Ok(history) => ExistingLibrary::Read {
                sessions: u32::try_from(history.len()).unwrap_or(u32::MAX),
                entries: u32::try_from(history.iter().map(|s| s.entries.len()).sum::<usize>())
                    .unwrap_or(u32::MAX),
                history,
            },
            Err(Error::NotDecrypted { .. }) => ExistingLibrary::NotRead {
                reason: "the OneLibrary key does not open it".into(),
            },
            Err(e) => ExistingLibrary::NotRead {
                reason: e.to_string(),
            },
        },
    )
}

impl OneLibraryExport for OneLibraryWriter {
    fn plan(&self, plan: &ExportPlan, key: &OneLibraryKey) -> Result<OneLibraryPlan, PlanError> {
        let root = plan.device.mount_point.as_path();
        let entries = plan.playlists.iter().map(|p| p.track_ids.len()).sum();
        Ok(OneLibraryPlan {
            write: db_write(estimated_bytes(plan.tracks.len(), entries)),
            removes: auxiliary_files(root)?,
            existing: existing(root, key.expose())?,
            warnings: Vec::new(),
            errors: Vec::new(),
        })
    }

    fn existing(
        &self,
        device_root: &Path,
        key: &OneLibraryKey,
    ) -> Result<ExistingLibrary, PlanError> {
        existing(device_root, key.expose())
    }

    fn write(
        &self,
        _plan: &ExportPlan,
        key: &OneLibraryKey,
        device_root: &Path,
    ) -> Result<OneLibraryWritten, OneLibraryWriteError> {
        self.write_db(key.expose(), device_root)
    }
}

#[cfg(test)]
mod tests;
