//! Reading an `exportLibrary.db` (task 7, and the writer's `plan`).
//!
//! rekordbox leaves its database in WAL mode with most of its content in
//! `exportLibrary.db-wal` (reference: 118 KiB `.db`, 910 KiB `-wal`).
//! Opening it in place would replay the WAL into the stick's file, so the
//! reader opens a copy of the database and its `-wal`/`-shm` in a temporary
//! directory. The stick is never written.

use std::path::Path;

use rusqlite::Connection;
use tempfile::TempDir;

use crate::cipher::{self, CipherProfile};
use crate::dump::Dump;
use crate::error::{Error, Result};
use crate::rows::OneLibrary;
use crate::schema::PROFILE;
use crate::store;

/// An unlocked copy of an `exportLibrary.db`.
#[derive(Debug)]
pub struct OneLibraryReader {
    conn: Connection,
    // Dropped after `conn` (field order), which removes the copy.
    _copy: TempDir,
}

impl OneLibraryReader {
    /// Opens `path` (and its `-wal`/`-shm` when present) with `key`.
    pub fn open(path: &Path, key: &str) -> Result<Self> {
        Self::open_with(path, key, PROFILE)
    }

    pub fn open_with(path: &Path, key: &str, profile: CipherProfile) -> Result<Self> {
        let copy = tempfile::tempdir().map_err(|source| Error::Io {
            path: std::env::temp_dir(),
            source,
        })?;
        let dst = copy.path().join("exportLibrary.db");
        for suffix in ["", "-wal", "-shm"] {
            let mut from = path.as_os_str().to_owned();
            from.push(suffix);
            let mut to = dst.as_os_str().to_owned();
            to.push(suffix);
            match std::fs::read(&from) {
                Ok(bytes) => std::fs::write(&to, bytes).map_err(|source| Error::Io {
                    path: to.into(),
                    source,
                })?,
                Err(e) if suffix.is_empty() || e.kind() != std::io::ErrorKind::NotFound => {
                    return Err(Error::Io {
                        path: from.into(),
                        source: e,
                    })
                }
                Err(_) => {}
            }
        }
        let conn = cipher::open_read_write(&dst, key, profile).map_err(|e| match e {
            Error::NotDecrypted { .. } => Error::NotDecrypted {
                path: path.to_owned(),
            },
            e => e,
        })?;
        Ok(Self { conn, _copy: copy })
    }

    /// `PRAGMA integrity_check`'s answer, `"ok"` when sound.
    pub fn integrity_check(&self) -> Result<String> {
        Ok(self
            .conn
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))?)
    }

    /// The non-seed rows.
    pub fn library(&self) -> Result<OneLibrary> {
        store::read(&self.conn)
    }

    /// Schema and every row.
    pub fn dump(&self) -> Result<Dump> {
        Dump::read(&self.conn)
    }

    /// The unlocked copy; play history reads it ([`crate::history`]).
    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }
}
