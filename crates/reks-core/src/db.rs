//! The library database: opening, connection settings and migrations.
//!
//! Schema: `migrations/` (spec in `docs/phases/phase-1.md`). One writer
//! connection behind a mutex (decision D3); callers run queries off the UI
//! thread.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, RwLock};
use std::time::Duration;

use rusqlite::Connection;
use rusqlite_migration::{Migrations, M};

use crate::error::{Error, Result};

/// Ordered schema migrations. Append only; never edit a released migration.
const MIGRATION_LIST: &[M<'static>] = &[
    M::up(include_str!("../migrations/0001_init.sql")),
    M::up(include_str!("../migrations/0002_export_manifest.sql")),
    M::up(include_str!("../migrations/0003_device_profiles.sql")),
    M::up(include_str!("../migrations/0004_superseded_file.sql")),
];
const MIGRATIONS: Migrations<'static> = Migrations::from_slice(MIGRATION_LIST);

/// Schema version after all migrations (SQLite `user_version`).
pub const SCHEMA_VERSION: i64 = MIGRATION_LIST.len() as i64;

/// How long a statement waits on a locked database before failing.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// The reks music library (SQLite).
#[derive(Debug)]
pub struct Library {
    conn: Mutex<Connection>,
    /// Changed only by `use_cache_dir` / `move_cache_dir` (settings.rs).
    pub(crate) cache_dir: RwLock<PathBuf>,
}

impl Library {
    /// Opens (creating if needed) the library at `db_path`, enables WAL and
    /// foreign keys, and migrates the schema to the latest version.
    pub fn open(db_path: &Path, cache_dir: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            create_dir(parent)?;
        }
        create_dir(cache_dir)?;

        let conn = Connection::open(db_path)?;
        let mode: String =
            conn.pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))?;
        if !mode.eq_ignore_ascii_case("wal") {
            return Err(Error::JournalMode(mode));
        }
        // NORMAL is durable across application crashes in WAL mode; only a
        // power loss can drop the last commits.
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        Self::init(conn, cache_dir)
    }

    /// Opens a fresh in-memory library (tests and headless tools).
    pub fn open_in_memory(cache_dir: &Path) -> Result<Self> {
        Self::init(Connection::open_in_memory()?, cache_dir)
    }

    fn init(mut conn: Connection, cache_dir: &Path) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(BUSY_TIMEOUT)?;
        let before: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        MIGRATIONS.to_latest(&mut conn)?;
        if before != SCHEMA_VERSION {
            tracing::info!(
                from = before,
                to = SCHEMA_VERSION,
                "library schema migrated"
            );
        }
        Ok(Self {
            conn: Mutex::new(conn),
            cache_dir: RwLock::new(cache_dir.to_path_buf()),
        })
    }

    /// Directory for derived files (artwork, analysis caches).
    pub fn cache_dir(&self) -> PathBuf {
        match self.cache_dir.read() {
            Ok(dir) => dir.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    /// Locks the connection. Crate-internal: service modules build on this.
    pub(crate) fn conn(&self) -> Result<MutexGuard<'_, Connection>> {
        self.conn.lock().map_err(|_| Error::LockPoisoned)
    }
}

pub(crate) fn create_dir(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_temp() -> (tempfile::TempDir, Library) {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::open(
            &dir.path().join("db/reks.sqlite"),
            &dir.path().join("cache"),
        )
        .unwrap();
        (dir, lib)
    }

    fn insert_track(c: &Connection, path: &str, title: &str) -> i64 {
        c.execute(
            "INSERT INTO track (path, file_size, file_mtime, kind, title, artist,
                                date_added, created_at, updated_at)
             VALUES (?1, 1, '2026-01-01T00:00:00Z', 'wav', ?2, 'Some Artist',
                     '2026-01-01', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            (path, title),
        )
        .unwrap();
        c.last_insert_rowid()
    }

    fn fts_ids(c: &Connection, query: &str) -> Vec<i64> {
        let mut stmt = c
            .prepare("SELECT rowid FROM track_fts WHERE track_fts MATCH ?1 ORDER BY rowid")
            .unwrap();
        stmt.query_map([query], |r| r.get(0))
            .unwrap()
            .collect::<std::result::Result<_, _>>()
            .unwrap()
    }

    fn count(c: &Connection, table: &str) -> i64 {
        c.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn migrations_are_valid() {
        MIGRATIONS.validate().unwrap();
    }

    #[test]
    fn open_sets_pragmas_and_version() {
        let (dir, lib) = open_temp();
        assert!(dir.path().join("db/reks.sqlite").exists());
        assert!(lib.cache_dir().is_dir());
        let c = lib.conn().unwrap();
        let mode: String = c
            .pragma_query_value(None, "journal_mode", |r| r.get(0))
            .unwrap();
        assert_eq!(mode, "wal");
        let fk: i64 = c
            .pragma_query_value(None, "foreign_keys", |r| r.get(0))
            .unwrap();
        assert_eq!(fk, 1);
        let version: i64 = c
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
    }

    #[test]
    fn reopen_keeps_data() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("reks.sqlite");
        let cache = dir.path().join("cache");
        {
            let lib = Library::open(&db, &cache).unwrap();
            insert_track(&lib.conn().unwrap(), "/music/a.wav", "Alpha");
        }
        let lib = Library::open(&db, &cache).unwrap();
        assert_eq!(count(&lib.conn().unwrap(), "track"), 1);
    }

    #[test]
    fn schema_objects_exist() {
        let (_dir, lib) = open_temp();
        let c = lib.conn().unwrap();
        let expected = [
            ("table", "track"),
            ("table", "artwork"),
            ("table", "beatgrid"),
            ("table", "cue"),
            ("table", "playlist"),
            ("table", "playlist_track"),
            ("table", "library_root"),
            ("table", "setting"),
            ("table", "track_fts"),
            ("table", "export_manifest"),
            ("table", "superseded_file"),
            ("index", "track_artist_idx"),
            ("index", "track_bpm_idx"),
            ("index", "track_key_idx"),
            ("index", "beatgrid_track_idx"),
            ("index", "cue_track_idx"),
            ("index", "playlist_track_track_idx"),
            ("index", "export_manifest_track_idx"),
            ("index", "superseded_file_track_idx"),
            ("trigger", "track_fts_ai"),
            ("trigger", "track_fts_ad"),
            ("trigger", "track_fts_au"),
        ];
        for (kind, name) in expected {
            let n: i64 = c
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type = ?1 AND name = ?2",
                    (kind, name),
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "missing {kind} {name}");
        }
    }

    #[test]
    fn fts_follows_insert_update_delete() {
        let lib = Library::open_in_memory(Path::new("cache-unused")).unwrap();
        let c = lib.conn().unwrap();
        let a = insert_track(&c, "/music/a.wav", "Midnight Groove");
        let b = insert_track(&c, "/music/b.wav", "Sunrise Dub");
        assert_eq!(fts_ids(&c, "groove"), vec![a]);
        assert_eq!(fts_ids(&c, "artist"), vec![a, b]);

        c.execute(
            "UPDATE track SET title = 'Morning Groove' WHERE id = ?1",
            [b],
        )
        .unwrap();
        assert_eq!(fts_ids(&c, "sunrise"), Vec::<i64>::new());
        assert_eq!(fts_ids(&c, "groove"), vec![a, b]);

        // Non-indexed column updates leave the index intact.
        c.execute("UPDATE track SET play_count = 3 WHERE id = ?1", [a])
            .unwrap();
        assert_eq!(fts_ids(&c, "midnight"), vec![a]);

        c.execute("DELETE FROM track WHERE id = ?1", [a]).unwrap();
        assert_eq!(fts_ids(&c, "groove"), vec![b]);
        c.execute(
            "INSERT INTO track_fts(track_fts) VALUES ('integrity-check')",
            [],
        )
        .unwrap();
    }

    #[test]
    fn deleting_track_cascades() {
        let lib = Library::open_in_memory(Path::new("cache-unused")).unwrap();
        let c = lib.conn().unwrap();
        let t = insert_track(&c, "/music/a.wav", "Alpha");
        c.execute(
            "INSERT INTO beatgrid (track_id, position_ms, bpm_x100, source)
             VALUES (?1, 25.0, 12800, 'rekordbox')",
            [t],
        )
        .unwrap();
        c.execute(
            "INSERT INTO cue (track_id, slot, kind, start_ms, source)
             VALUES (?1, 0, 0, 25.0, 'rekordbox')",
            [t],
        )
        .unwrap();
        c.execute(
            "INSERT INTO playlist (id, name, sort_order, created_at, updated_at)
             VALUES (1, 'Set', 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        c.execute(
            "INSERT INTO playlist_track (playlist_id, track_id, position) VALUES (1, ?1, 0)",
            [t],
        )
        .unwrap();

        c.execute("DELETE FROM track WHERE id = ?1", [t]).unwrap();
        for table in ["beatgrid", "cue", "playlist_track"] {
            assert_eq!(count(&c, table), 0, "{table} not cascaded");
        }
        assert_eq!(count(&c, "playlist"), 1);
    }

    #[test]
    fn foreign_keys_are_enforced() {
        let lib = Library::open_in_memory(Path::new("cache-unused")).unwrap();
        let c = lib.conn().unwrap();
        let err = c.execute(
            "INSERT INTO cue (track_id, kind, start_ms, source) VALUES (999, 0, 0, 'manual')",
            [],
        );
        assert!(err.is_err());
    }
}
