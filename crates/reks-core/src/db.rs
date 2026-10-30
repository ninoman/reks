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

use crate::backup::{self, BackupReason, BackupState};
use crate::error::{Error, Result};
use crate::settings;

/// Ordered schema migrations. Append only; never edit a released migration.
/// Each runs with foreign keys off ([`migrate`]) and is checked with
/// `PRAGMA foreign_key_check` before its transaction commits.
const MIGRATION_LIST: &[M<'static>] = &[
    up(include_str!("../migrations/0001_init.sql")),
    up(include_str!("../migrations/0002_export_manifest.sql")),
    up(include_str!("../migrations/0003_device_profiles.sql")),
    up(include_str!("../migrations/0004_superseded_file.sql")),
    up(include_str!("../migrations/0005_dividers_history.sql")),
    up(include_str!("../migrations/0006_grid_lock.sql")),
    up(include_str!("../migrations/0007_analysis.sql")),
    up(include_str!("../migrations/0008_analysis_source.sql")),
    up(include_str!("../migrations/0009_my_tags.sql")),
    up(include_str!("../migrations/0010_smart_playlists.sql")),
    up(include_str!("../migrations/0011_cue_active_loop.sql")),
    up(include_str!("../migrations/0012_filter_presets.sql")),
    up(include_str!("../migrations/0013_grid_base.sql")),
    up(include_str!("../migrations/0014_drop_mixed_grids.sql")),
    up(include_str!("../migrations/0015_one_hot_cue_per_slot.sql")),
];
const MIGRATIONS: Migrations<'static> = Migrations::from_slice(MIGRATION_LIST);

const fn up(sql: &'static str) -> M<'static> {
    M::up(sql).foreign_key_check()
}

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
    /// Where snapshots go and how the last one fared (`backup` module).
    pub(crate) backup: BackupState,
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
        // Snapshots sit next to the DB: the data dir, not the cache (D36).
        let backup_dir = db_path.parent().map(|dir| dir.join(backup::BACKUP_DIR));
        Self::init(conn, cache_dir, backup_dir)
    }

    /// Opens a fresh in-memory library (tests and headless tools). It cannot
    /// be backed up.
    pub fn open_in_memory(cache_dir: &Path) -> Result<Self> {
        Self::init(Connection::open_in_memory()?, cache_dir, None)
    }

    fn init(mut conn: Connection, cache_dir: &Path, backup_dir: Option<PathBuf>) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(BUSY_TIMEOUT)?;
        let before: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        // A library with data and an older schema is about to be rewritten:
        // keep its last good state. A failed snapshot is reported, not fatal.
        let mut backup_failure = None;
        if let Some(dir) = backup_dir
            .as_deref()
            .filter(|_| 0 < before && before < SCHEMA_VERSION)
        {
            let keep = settings::backup_keep(&conn);
            if let Err(err) = backup::take(&conn, dir, BackupReason::Migration, keep) {
                tracing::warn!(%err, "pre-migration library backup failed");
                backup_failure = Some(backup::failure_of(BackupReason::Migration, &err));
            }
        }
        migrate(&mut conn)?;
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
            backup: BackupState::new(backup_dir).with_failure(backup_failure),
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

/// Brings the schema to the latest version.
pub(crate) fn migrate(conn: &mut Connection) -> Result<()> {
    migrate_with(conn, &MIGRATIONS)
}

/// Runs `migrations` with foreign keys off, as SQLite's table-rebuild recipe
/// requires: with them on, a `DROP TABLE` of a parent cascade-deletes its
/// children (cues, grids, entries, tags). The pragma is a no-op inside a
/// transaction, so it is switched here, around the migration transactions.
/// Each step's `foreign_key_check` stands in for the constraints meanwhile.
fn migrate_with(conn: &mut Connection, migrations: &Migrations<'_>) -> Result<()> {
    conn.pragma_update(None, "foreign_keys", "OFF")?;
    let migrated = migrations.to_latest(conn);
    conn.pragma_update(None, "foreign_keys", "ON")?;
    migrated?;
    Ok(())
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
            ("table", "filter_preset"),
            ("index", "track_artist_idx"),
            ("index", "track_bpm_idx"),
            ("index", "track_key_idx"),
            ("index", "beatgrid_track_idx"),
            ("index", "cue_track_idx"),
            ("index", "playlist_track_track_idx"),
            ("index", "export_manifest_track_idx"),
            ("index", "superseded_file_track_idx"),
            ("index", "play_session_entry_track_idx"),
            ("index", "filter_preset_name_idx"),
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
    fn migration_12_adds_filter_presets_and_keeps_playlists() {
        let mut c = conn_at(11);
        c.execute(
            "INSERT INTO playlist (id, name, sort_order, created_at, updated_at, smart_rule)
             VALUES (1, 'Smart', 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z',
                     '{\"type\":\"group\",\"match\":\"all\"}')",
            [],
        )
        .unwrap();
        MIGRATIONS.to_latest(&mut c).unwrap();
        assert_eq!(count(&c, "playlist"), 1);
        assert_eq!(count(&c, "filter_preset"), 0);
        let insert = |name: &str, position: i64| {
            c.execute(
                "INSERT INTO filter_preset (name, position, rule, created_at, updated_at)
                 VALUES (?1, ?2, '{}', 'x', 'x')",
                (name, position),
            )
        };
        insert("Peak techno", 0).unwrap();
        assert!(
            insert("PEAK TECHNO", 1).is_err(),
            "names are unique, any case"
        );
        insert("Warm-up", 1).unwrap();
        assert_eq!(count(&c, "filter_preset"), 2);
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
    fn migrating_snapshots_the_old_library_first() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("reks.sqlite");
        let cache = dir.path().join("cache");

        // A fresh library has nothing to protect.
        drop(Library::open(&db, &cache).unwrap());
        assert!(!dir.path().join(backup::BACKUP_DIR).exists());

        // One left at schema 11 holds a track that migration 12 must not endanger.
        std::fs::remove_file(&db).unwrap();
        {
            let mut c = Connection::open(&db).unwrap();
            MIGRATIONS.to_version(&mut c, 11).unwrap();
            insert_track(&c, "/music/a.wav", "Alpha");
        }
        let lib = Library::open(&db, &cache).unwrap();
        let backups = lib.backups().unwrap();
        assert_eq!(backups.len(), 1);
        assert_eq!(backups[0].reason, BackupReason::Migration);
        let contents = backups[0].contents.unwrap();
        assert_eq!((contents.schema_version, contents.tracks), (11, 1));
        assert_eq!(lib.backup_status().last_failure, None);
        drop(lib);

        // Up to date: reopening adds nothing.
        let lib = Library::open(&db, &cache).unwrap();
        assert_eq!(lib.backups().unwrap().len(), 1);
    }

    #[test]
    fn a_failed_pre_migration_snapshot_does_not_stop_the_migration() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("reks.sqlite");
        {
            let mut c = Connection::open(&db).unwrap();
            MIGRATIONS.to_version(&mut c, 11).unwrap();
        }
        std::fs::write(dir.path().join(backup::BACKUP_DIR), b"in the way").unwrap();
        let lib = Library::open(&db, &dir.path().join("cache")).unwrap();
        let failure = lib.backup_status().last_failure.unwrap();
        assert_eq!(failure.reason, BackupReason::Migration);
        let version: i64 = lib
            .conn()
            .unwrap()
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
    }

    #[test]
    fn migration_14_drops_analysis_rows_beside_rekordbox_ones() {
        let mut c = Connection::open_in_memory().unwrap();
        MIGRATIONS.to_version(&mut c, 13).unwrap();
        let mixed = insert_track(&c, "/music/a.wav", "Mixed");
        let analysed = insert_track(&c, "/music/b.wav", "Analysed");
        let grid = |t: i64, pos: i64, source: &str| {
            c.execute(
                "INSERT INTO beatgrid (track_id, position_ms, bpm_x100, source)
                 VALUES (?1, ?2, 12800, ?3)",
                rusqlite::params![t, pos, source],
            )
            .unwrap();
        };
        grid(mixed, 0, "rekordbox");
        grid(mixed, 7, "analysis");
        grid(analysed, 7, "analysis");
        MIGRATIONS.to_version(&mut c, 14).unwrap();
        let sources = |t: i64| -> Vec<String> {
            let mut stmt = c
                .prepare("SELECT source FROM beatgrid WHERE track_id = ?1 ORDER BY id")
                .unwrap();
            stmt.query_map([t], |r| r.get(0))
                .unwrap()
                .collect::<std::result::Result<_, _>>()
                .unwrap()
        };
        assert_eq!(sources(mixed), ["rekordbox"]);
        assert_eq!(sources(analysed), ["analysis"]);
    }

    #[test]
    fn migration_15_keeps_one_hot_cue_per_slot() {
        let mut c = Connection::open_in_memory().unwrap();
        MIGRATIONS.to_version(&mut c, 14).unwrap();
        let t = insert_track(&c, "/music/a.wav", "Alpha");
        let cue = |slot: Option<i64>, start: i64, source: &str| {
            c.execute(
                "INSERT INTO cue (track_id, slot, kind, start_ms, source)
                 VALUES (?1, ?2, 0, ?3, ?4)",
                rusqlite::params![t, slot, start, source],
            )
            .unwrap();
        };
        cue(Some(0), 1, "rekordbox");
        cue(Some(0), 2, "manual");
        cue(Some(1), 3, "rekordbox");
        cue(Some(1), 4, "rekordbox");
        cue(None, 5, "rekordbox");
        cue(None, 6, "rekordbox");
        MIGRATIONS.to_version(&mut c, 15).unwrap();
        let mut stmt = c
            .prepare("SELECT slot, start_ms FROM cue ORDER BY start_ms")
            .unwrap();
        let rows: Vec<(Option<i64>, f64)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<std::result::Result<_, _>>()
            .unwrap();
        assert_eq!(
            rows,
            [(Some(0), 2.0), (Some(1), 3.0), (None, 5.0), (None, 6.0)]
        );
        // The index refuses a second hot cue in a slot, not a memory cue.
        assert!(c
            .execute(
                "INSERT INTO cue (track_id, slot, kind, start_ms, source)
                 VALUES (?1, 0, 0, 9, 'manual')",
                [t],
            )
            .is_err());
        c.execute(
            "INSERT INTO cue (track_id, slot, kind, start_ms, source)
             VALUES (?1, NULL, 0, 9, 'manual')",
            [t],
        )
        .unwrap();
    }

    /// The migrations plus `extra`, as a later release would ship them.
    fn with_extra(extra: &'static str) -> Migrations<'static> {
        let mut list = MIGRATION_LIST.to_vec();
        list.push(up(extra));
        Migrations::new(list)
    }

    #[test]
    fn a_table_rebuild_keeps_child_rows() {
        let lib = Library::open_in_memory(Path::new("cache-unused")).unwrap();
        let mut c = lib.conn().unwrap();
        let t = insert_track(&c, "/music/a.wav", "Alpha");
        // SQLite's recipe for a schema change ALTER TABLE can't do.
        let rebuild = with_extra(
            "CREATE TABLE tag_new (
               id INTEGER PRIMARY KEY,
               parent_id INTEGER REFERENCES tag(id) ON DELETE CASCADE,
               name TEXT NOT NULL, sort_order INTEGER NOT NULL,
               created_at TEXT NOT NULL, updated_at TEXT NOT NULL);
             INSERT INTO tag_new SELECT id, parent_id, name, sort_order, created_at, updated_at
               FROM tag;
             DROP TABLE tag;
             ALTER TABLE tag_new RENAME TO tag;",
        );
        let tagged: i64 = {
            let cat: i64 = c
                .query_row(
                    "SELECT id FROM tag WHERE parent_id IS NULL LIMIT 1",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            c.execute(
                "INSERT INTO tag (parent_id, name, sort_order, created_at, updated_at)
                 VALUES (?1, 'Peak', 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                [cat],
            )
            .unwrap();
            let tag = c.last_insert_rowid();
            c.execute(
                "INSERT INTO track_tag (track_id, tag_id) VALUES (?1, ?2)",
                [t, tag],
            )
            .unwrap();
            count(&c, "track_tag")
        };
        let tags = count(&c, "tag");
        migrate_with(&mut c, &rebuild).unwrap();
        assert_eq!(count(&c, "tag"), tags);
        assert_eq!(count(&c, "track_tag"), tagged);
        let fk: i64 = c
            .pragma_query_value(None, "foreign_keys", |r| r.get(0))
            .unwrap();
        assert_eq!(fk, 1);
    }

    #[test]
    fn a_migration_that_orphans_rows_is_rolled_back() {
        let lib = Library::open_in_memory(Path::new("cache-unused")).unwrap();
        let mut c = lib.conn().unwrap();
        let t = insert_track(&c, "/music/a.wav", "Alpha");
        c.execute(
            "INSERT INTO cue (track_id, kind, start_ms, source) VALUES (?1, 0, 0, 'manual')",
            [t],
        )
        .unwrap();
        let err = migrate_with(&mut c, &with_extra("DELETE FROM track;")).unwrap_err();
        assert!(
            matches!(
                err,
                Error::Migration(rusqlite_migration::Error::ForeignKeyCheck(_))
            ),
            "{err}"
        );
        assert_eq!(count(&c, "track"), 1);
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
