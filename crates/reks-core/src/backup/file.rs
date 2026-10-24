//! Writing, checking and reading snapshot files. Everything here works on
//! plain paths and connections; the library handle lives in `mod.rs`.

use std::path::Path;
use std::time::Duration;

use rusqlite::backup::Backup;
use rusqlite::{Connection, OpenFlags};

use crate::error::{Error, Result};

/// What a checked snapshot holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Contents {
    pub schema_version: i64,
    pub tracks: i64,
    pub playlists: i64,
}

fn io(path: &Path, source: std::io::Error) -> Error {
    Error::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// Copies `src` into a new file at `dest` with SQLite's online backup API,
/// which reads a consistent state even while WAL is live (copying the file
/// would miss what sits in `-wal`). The copy is switched to rollback-journal
/// mode so a snapshot is one self-contained file.
pub(super) fn write(src: &Connection, dest: &Path) -> Result<()> {
    let mut out = Connection::open(dest)?;
    {
        let backup = Backup::new(src, &mut out)?;
        // One step (i32::MAX pages): the source is read in a single transaction.
        backup.run_to_completion(i32::MAX, Duration::ZERO, None)?;
    }
    let mode: String = out.pragma_update_and_check(None, "journal_mode", "DELETE", |r| r.get(0))?;
    debug_assert!(mode.eq_ignore_ascii_case("delete"));
    out.close().map_err(|(_, e)| Error::from(e))?;
    Ok(())
}

/// How hard [`check`] looks for corruption.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Depth {
    /// `quick_check`: skips index-against-table comparison; for listings.
    Quick,
    /// `integrity_check`: for a snapshot just written and one about to be restored.
    Full,
}

/// Opens `path` read-only, runs the integrity pragma and reads its schema
/// version and counts. `name` is how errors name the snapshot.
pub(super) fn check(path: &Path, name: &str, depth: Depth) -> Result<Contents> {
    let corrupt = |detail: String| Error::BackupCorrupt {
        name: name.to_owned(),
        detail,
    };
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| corrupt(e.to_string()))?;

    let report = integrity_report(&conn, depth).map_err(|e| corrupt(e.to_string()))?;
    if report != "ok" {
        return Err(corrupt(report));
    }
    let schema_version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    let is_library: bool = conn.query_row(
        "SELECT count(*) = 2 FROM sqlite_master
         WHERE type = 'table' AND name IN ('track', 'playlist')",
        [],
        |r| r.get(0),
    )?;
    if schema_version == 0 || !is_library {
        return Err(Error::BackupNotALibrary(name.to_owned()));
    }
    Ok(Contents {
        schema_version,
        tracks: conn.query_row("SELECT count(*) FROM track", [], |r| r.get(0))?,
        playlists: conn.query_row(
            "SELECT count(*) FROM playlist WHERE is_folder = 0",
            [],
            |r| r.get(0),
        )?,
    })
}

/// First line of the integrity pragma ("ok", or the first problem).
fn integrity_report(conn: &Connection, depth: Depth) -> rusqlite::Result<String> {
    let pragma = match depth {
        Depth::Quick => "PRAGMA quick_check",
        Depth::Full => "PRAGMA integrity_check",
    };
    conn.query_row(pragma, [], |r| r.get(0))
}

/// Replaces everything in `live` with the contents of the snapshot at `from`
/// (online backup, so the connection stays open and keeps its pragmas).
pub(super) fn load_into(live: &mut Connection, from: &Path) -> Result<()> {
    let src = Connection::open_with_flags(from, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let backup = Backup::new(&src, live)?;
    backup.run_to_completion(i32::MAX, Duration::ZERO, None)?;
    Ok(())
}

pub(super) fn remove(path: &Path) -> Result<()> {
    std::fs::remove_file(path).map_err(|e| io(path, e))
}

pub(super) fn rename(from: &Path, to: &Path) -> Result<()> {
    std::fs::rename(from, to).map_err(|e| io(to, e))
}

pub(super) fn size(path: &Path) -> Result<u64> {
    std::fs::metadata(path)
        .map(|m| m.len())
        .map_err(|e| io(path, e))
}
