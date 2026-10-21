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
    M::up(include_str!("../migrations/0005_dividers_history.sql")),
    M::up(include_str!("../migrations/0006_grid_lock.sql")),
    M::up(include_str!("../migrations/0007_analysis.sql")),
    M::up(include_str!("../migrations/0009_my_tags.sql")),
    M::up(include_str!("../migrations/0010_smart_playlists.sql")),
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
            ("table", "play_session"),
            ("table", "play_session_entry"),
            ("index", "track_artist_idx"),
            ("index", "track_bpm_idx"),
            ("index", "track_key_idx"),
            ("index", "beatgrid_track_idx"),
            ("index", "cue_track_idx"),
            ("index", "playlist_track_track_idx"),
            ("index", "export_manifest_track_idx"),
            ("index", "superseded_file_track_idx"),
            ("index", "play_session_entry_track_idx"),
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

    /// A connection at schema version `v` with foreign keys on, as `init` sets it.
    fn conn_at(v: usize) -> Connection {
        let mut c = Connection::open_in_memory().unwrap();
        c.pragma_update(None, "foreign_keys", "ON").unwrap();
        MIGRATIONS.to_version(&mut c, v).unwrap();
        c
    }

    type EntryRow = (i64, Option<i64>, i64, String, Option<String>);

    fn entry_rows(c: &Connection) -> Vec<EntryRow> {
        c.prepare(
            "SELECT playlist_id, track_id, position, item_kind, label FROM playlist_track
             ORDER BY playlist_id, position",
        )
        .unwrap()
        .query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
    }

    fn baseline_date(c: &Connection) -> String {
        c.query_row(
            "SELECT value FROM setting WHERE key = 'history.baseline_date'",
            [],
            |r| r.get(0),
        )
        .unwrap()
    }

    #[test]
    fn migration_5_keeps_entries_and_copies_the_baseline() {
        let mut c = conn_at(4);
        let a = insert_track(&c, "/music/a.wav", "Alpha");
        let b = insert_track(&c, "/music/b.wav", "Beta");
        c.execute(
            "UPDATE track SET play_count = 7, last_played = '2026-05-01' WHERE id = ?1",
            [a],
        )
        .unwrap();
        c.execute_batch(
            "INSERT INTO playlist (id, name, sort_order, created_at, updated_at) VALUES
               (1, 'Set', 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
               (2, 'Other', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');",
        )
        .unwrap();
        for (p, t, pos) in [(1, a, 0), (1, b, 1), (1, a, 2), (2, b, 0)] {
            c.execute(
                "INSERT INTO playlist_track (playlist_id, track_id, position) VALUES (?1, ?2, ?3)",
                (p, t, pos),
            )
            .unwrap();
        }

        MIGRATIONS.to_latest(&mut c).unwrap();
        let track = |p, t, pos| (p, Some(t), pos, "track".to_owned(), None);
        assert_eq!(
            entry_rows(&c),
            vec![
                track(1, a, 0),
                track(1, b, 1),
                track(1, a, 2),
                track(2, b, 0)
            ]
        );
        let baselines: Vec<(i64, i64, Option<String>)> = c
            .prepare("SELECT id, baseline_play_count, baseline_last_played FROM track ORDER BY id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(
            baselines,
            vec![(a, 7, Some("2026-05-01".into())), (b, 0, None)]
        );
        let today: String = c.query_row("SELECT date('now')", [], |r| r.get(0)).unwrap();
        assert_eq!(baseline_date(&c), format!("\"{today}\""));

        // The rebuilt table still cascades and is indexed.
        c.execute("DELETE FROM track WHERE id = ?1", [b]).unwrap();
        assert_eq!(entry_rows(&c), vec![track(1, a, 0), track(1, a, 2)]);
        let idx: i64 = c
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name = 'playlist_track_track_idx'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(idx, 1);
    }

    #[test]
    fn migration_9_seeds_categories_and_cascades_tags() {
        let mut c = conn_at(8);
        let t = insert_track(&c, "/music/a.wav", "Alpha");
        MIGRATIONS.to_latest(&mut c).unwrap();
        let cats: Vec<String> = c
            .prepare("SELECT name FROM tag WHERE parent_id IS NULL ORDER BY sort_order")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(cats, ["Genre", "Components", "Situation"]);
        let genre: i64 = c
            .query_row("SELECT id FROM tag WHERE name = 'Genre'", [], |r| r.get(0))
            .unwrap();
        c.execute(
            "INSERT INTO tag (parent_id, name, sort_order, created_at, updated_at)
             VALUES (?1, 'Deep', 0, 'x', 'x')",
            [genre],
        )
        .unwrap();
        let dup = c.execute(
            "INSERT INTO tag (parent_id, name, sort_order, created_at, updated_at)
             VALUES (?1, 'DEEP', 1, 'x', 'x')",
            [genre],
        );
        assert!(dup.is_err(), "names are unique per parent, any case");
        let dup_cat = c.execute(
            "INSERT INTO tag (parent_id, name, sort_order, created_at, updated_at)
             VALUES (NULL, 'genre', 3, 'x', 'x')",
            [],
        );
        assert!(dup_cat.is_err(), "category names are unique too");
        c.execute(
            "INSERT INTO track_tag (track_id, tag_id) SELECT ?1, id FROM tag WHERE name = 'Deep'",
            [t],
        )
        .unwrap();
        c.execute("DELETE FROM tag WHERE id = ?1", [genre]).unwrap();
        assert_eq!(count(&c, "track_tag"), 0);
        assert_eq!(count(&c, "tag"), 2);
    }

    #[test]
    fn migration_10_adds_smart_columns_and_keeps_playlists() {
        let mut c = conn_at(9);
        let t = insert_track(&c, "/music/a.wav", "Alpha");
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
        c.execute(
            "INSERT INTO track_tag (track_id, tag_id) SELECT ?1, id FROM tag WHERE name = 'Genre'",
            [t],
        )
        .unwrap();
        MIGRATIONS.to_latest(&mut c).unwrap();
        let row: (String, Option<String>, Option<String>) = c
            .query_row(
                "SELECT name, smart_rule, smart_sort FROM playlist WHERE id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(row, ("Set".to_owned(), None, None));
        assert_eq!(count(&c, "playlist_track"), 1);
        assert_eq!(count(&c, "track_tag"), 1);
    }

    #[test]
    fn entry_check_rejects_invalid_rows() {
        let c = conn_at(MIGRATION_LIST.len());
        let t = insert_track(&c, "/music/a.wav", "Alpha");
        c.execute(
            "INSERT INTO playlist (id, name, sort_order, created_at, updated_at)
             VALUES (1, 'Set', 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        let insert = |kind: &str, track: Option<i64>, label: Option<&str>| {
            c.execute(
                "INSERT INTO playlist_track (playlist_id, track_id, position, item_kind, label)
                 VALUES (1, ?1, (SELECT count(*) FROM playlist_track), ?2, ?3)",
                (track, kind, label),
            )
        };
        assert!(
            insert("track", None, None).is_err(),
            "track without track_id"
        );
        assert!(
            insert("track", Some(t), Some("x")).is_err(),
            "track with label"
        );
        assert!(
            insert("divider", Some(t), Some("x")).is_err(),
            "divider with track_id"
        );
        assert!(
            insert("divider", None, None).is_err(),
            "divider without label"
        );
        assert!(insert("cue", None, Some("x")).is_err(), "unknown kind");
        insert("track", Some(t), None).unwrap();
        insert("divider", None, Some("PEAK")).unwrap();
        assert_eq!(count(&c, "playlist_track"), 2);
    }

    #[test]
    fn reopening_after_migration_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("reks.sqlite");
        let cache = dir.path().join("cache");
        {
            let lib = Library::open(&db, &cache).unwrap();
            let c = lib.conn().unwrap();
            let t = insert_track(&c, "/music/a.wav", "Alpha");
            // The baseline is frozen: later cache writes and a moved date stay.
            c.execute(
                "UPDATE track SET play_count = 9, baseline_play_count = 4 WHERE id = ?1",
                [t],
            )
            .unwrap();
            c.execute(
                "UPDATE setting SET value = '\"2026-01-02\"' WHERE key = 'history.baseline_date'",
                [],
            )
            .unwrap();
        }
        let lib = Library::open(&db, &cache).unwrap();
        let mut c = lib.conn().unwrap();
        MIGRATIONS.to_latest(&mut c).unwrap();
        let counts: (i64, i64) = c
            .query_row(
                "SELECT play_count, baseline_play_count FROM track",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(counts, (9, 4));
        assert_eq!(baseline_date(&c), "\"2026-01-02\"");
        let version: i64 = c
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
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
