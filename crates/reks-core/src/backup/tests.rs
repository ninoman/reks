use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use rusqlite::Connection;

use super::*;
use crate::settings::SettingsPatch;

struct Fixture {
    dir: tempfile::TempDir,
    lib: Library,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let lib = Library::open(&dir.path().join("reks.sqlite"), &dir.path().join("cache")).unwrap();
    Fixture { dir, lib }
}

impl Fixture {
    fn backup_dir(&self) -> PathBuf {
        self.dir.path().join(BACKUP_DIR)
    }

    fn files(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(self.backup_dir())
            .map(|d| {
                d.flatten()
                    .map(|e| e.file_name().into_string().unwrap())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }
}

fn add_track(lib: &Library, n: usize) {
    lib.conn()
        .unwrap()
        .execute(
            "INSERT INTO track (path, file_size, file_mtime, kind, title, date_added,
                                created_at, updated_at)
             VALUES (?1, 1, '2026-01-01T00:00:00Z', 'wav', 'T', '2026-01-01',
                     '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [format!("/music/{n}.wav")],
        )
        .unwrap();
}

fn track_count(lib: &Library) -> i64 {
    lib.conn()
        .unwrap()
        .query_row("SELECT count(*) FROM track", [], |r| r.get(0))
        .unwrap()
}

#[test]
fn snapshot_is_one_verified_file_with_counts() {
    let f = fixture();
    add_track(&f.lib, 1);
    add_track(&f.lib, 2);
    let info = f.lib.snapshot(BackupReason::Manual).unwrap();

    assert_eq!(f.files(), vec![info.name.clone()], "no -wal, no partial");
    assert_eq!(info.reason, BackupReason::Manual);
    assert!(info.size_bytes > 0);
    let contents = info.contents.unwrap();
    assert_eq!((contents.tracks, contents.playlists), (2, 0));
    assert_eq!(contents.schema_version, SCHEMA_VERSION);
    assert_eq!(f.lib.backups().unwrap(), vec![info]);
    assert_eq!(f.lib.backup_status().last_failure, None);
}

#[test]
fn snapshot_while_a_writer_is_active() {
    let f = fixture();
    let lib = Arc::new(f.lib);
    let stop = Arc::new(AtomicBool::new(false));
    let writer = {
        let (lib, stop) = (Arc::clone(&lib), Arc::clone(&stop));
        std::thread::spawn(move || {
            let mut n = 0;
            while !stop.load(Ordering::Relaxed) {
                add_track(&lib, n);
                n += 1;
            }
        })
    };
    let mut seen = Vec::new();
    for _ in 0..5 {
        let info = lib.snapshot(BackupReason::Manual).unwrap();
        seen.push(info.contents.unwrap().tracks);
    }
    stop.store(true, Ordering::Relaxed);
    writer.join().unwrap();

    assert!(seen.windows(2).all(|w| w[0] <= w[1]), "{seen:?}");
    assert!(*seen.last().unwrap() <= track_count(&lib));
    assert_eq!(lib.backups().unwrap().len(), 5);
    assert!(lib.backups().unwrap().iter().all(|b| b.problem.is_none()));
}

#[test]
fn start_up_snapshot_is_due_once_a_day() {
    let f = fixture();
    let first = f.lib.snapshot_if_due().unwrap().expect("none yet: due");
    assert_eq!(first.reason, BackupReason::Startup);
    assert_eq!(f.lib.snapshot_if_due().unwrap(), None);
    assert_eq!(f.files().len(), 1);
}

#[test]
fn in_memory_libraries_cannot_be_backed_up() {
    let lib = Library::open_in_memory(Path::new("cache-unused")).unwrap();
    assert!(matches!(
        lib.snapshot(BackupReason::Manual),
        Err(Error::NoBackupDir)
    ));
    assert_eq!(lib.backup_status().dir, None);
}

#[test]
fn pruning_keeps_the_configured_count() {
    let f = fixture();
    f.lib
        .update_settings(SettingsPatch {
            backup_keep: Some(2),
            ..SettingsPatch::default()
        })
        .unwrap();
    assert_eq!(f.lib.settings().unwrap().backup_keep, 2);
    let names: Vec<String> = (0..4)
        .map(|_| f.lib.snapshot(BackupReason::Manual).unwrap().name)
        .collect();
    // All in one calendar week: the two newest stay (the weekly pick is one of them).
    assert_eq!(f.files(), names[2..].to_vec());
}

#[test]
fn keep_setting_is_clamped() {
    let f = fixture();
    let set = |n| {
        f.lib
            .update_settings(SettingsPatch {
                backup_keep: Some(n),
                ..SettingsPatch::default()
            })
            .unwrap()
            .backup_keep
    };
    assert_eq!(set(0), 1);
    assert_eq!(set(10_000), 100);
    assert_eq!(set(7), 7);
}

#[test]
fn restore_round_trip_keeps_a_way_back() {
    let f = fixture();
    add_track(&f.lib, 1);
    let good = f.lib.snapshot(BackupReason::Manual).unwrap();
    add_track(&f.lib, 2);
    add_track(&f.lib, 3);
    assert_eq!(track_count(&f.lib), 3);

    f.lib.restore_backup(&good.name).unwrap();
    assert_eq!(track_count(&f.lib), 1);
    // Search index and foreign keys came along and still work.
    add_track(&f.lib, 4);
    assert_eq!(track_count(&f.lib), 2);
    let fk: i64 = f
        .lib
        .conn()
        .unwrap()
        .pragma_query_value(None, "foreign_keys", |r| r.get(0))
        .unwrap();
    assert_eq!(fk, 1);

    let undo = f
        .lib
        .backups()
        .unwrap()
        .into_iter()
        .find(|b| b.reason == BackupReason::BeforeRestore)
        .expect("the current library was snapshotted first");
    assert_eq!(undo.contents.unwrap().tracks, 3);
    f.lib.restore_backup(&undo.name).unwrap();
    assert_eq!(track_count(&f.lib), 3);
}

#[test]
fn restore_migrates_an_older_snapshot() {
    let f = fixture();
    let old = f.lib.snapshot(BackupReason::Manual).unwrap();
    let path = f.backup_dir().join(&old.name);
    {
        // What a snapshot from before migration 12 looks like.
        let c = Connection::open(&path).unwrap();
        c.execute_batch("DROP TABLE filter_preset; PRAGMA user_version = 11;")
            .unwrap();
    }
    assert_eq!(
        f.lib.backups().unwrap()[0].contents.unwrap().schema_version,
        11
    );
    f.lib.restore_backup(&old.name).unwrap();
    let c = f.lib.conn().unwrap();
    let version: i64 = c
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(version, SCHEMA_VERSION);
    c.query_row("SELECT count(*) FROM filter_preset", [], |r| {
        r.get::<_, i64>(0)
    })
    .unwrap();
}

#[test]
fn a_snapshot_from_a_newer_schema_is_refused() {
    let f = fixture();
    add_track(&f.lib, 1);
    let snap = f.lib.snapshot(BackupReason::Manual).unwrap();
    Connection::open(f.backup_dir().join(&snap.name))
        .unwrap()
        .pragma_update(None, "user_version", SCHEMA_VERSION + 1)
        .unwrap();
    add_track(&f.lib, 2);

    let listed = &f.lib.backups().unwrap()[0];
    assert!(listed.too_new);
    let err = f.lib.restore_backup(&snap.name).unwrap_err();
    assert!(matches!(err, Error::BackupTooNew { found, .. } if found == SCHEMA_VERSION + 1));
    assert!(err.to_string().contains("update reks"));
    assert_eq!(track_count(&f.lib), 2, "library untouched");
    assert_eq!(
        f.files().len(),
        1,
        "no safety snapshot for a refused restore"
    );
}

#[test]
fn a_damaged_snapshot_is_flagged_and_never_restored() {
    let f = fixture();
    add_track(&f.lib, 1);
    let snap = f.lib.snapshot(BackupReason::Manual).unwrap();
    let path = f.backup_dir().join(&snap.name);
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[..64].fill(0xAB);
    std::fs::write(&path, bytes).unwrap();
    add_track(&f.lib, 2);

    let listed = &f.lib.backups().unwrap()[0];
    assert!(listed.problem.is_some());
    assert_eq!(listed.contents, None);
    assert!(matches!(
        f.lib.restore_backup(&snap.name),
        Err(Error::BackupCorrupt { .. })
    ));
    assert_eq!(track_count(&f.lib), 2, "library untouched");
}

#[test]
fn a_file_that_is_not_a_reks_library_is_refused() {
    let f = fixture();
    let other = f
        .backup_dir()
        .join(name::file_name(1, BackupReason::Manual));
    std::fs::create_dir_all(f.backup_dir()).unwrap();
    Connection::open(&other)
        .unwrap()
        .execute_batch("CREATE TABLE x (a); PRAGMA user_version = 3;")
        .unwrap();
    let file_name = other.file_name().unwrap().to_str().unwrap();
    assert!(matches!(
        f.lib.restore_backup(file_name),
        Err(Error::BackupNotALibrary(_))
    ));
}

#[test]
fn restore_only_takes_names_in_the_backup_folder() {
    let f = fixture();
    for name in ["../reks.sqlite", "reks.sqlite", "nope", "/etc/passwd"] {
        assert!(
            matches!(f.lib.restore_backup(name), Err(Error::BackupNotFound(_))),
            "{name}"
        );
    }
    let missing = name::file_name(5, BackupReason::Manual);
    assert!(matches!(
        f.lib.restore_backup(&missing),
        Err(Error::BackupNotFound(_))
    ));
}

#[test]
fn a_failed_snapshot_is_reported_then_cleared() {
    let f = fixture();
    // A file where the folder should be: creating it fails.
    std::fs::write(f.backup_dir(), b"in the way").unwrap();
    assert!(f.lib.snapshot(BackupReason::Startup).is_err());
    let failure = f.lib.backup_status().last_failure.unwrap();
    assert_eq!(failure.reason, BackupReason::Startup);
    assert!(!failure.message.is_empty());

    std::fs::remove_file(f.backup_dir()).unwrap();
    f.lib.snapshot(BackupReason::Manual).unwrap();
    assert_eq!(f.lib.backup_status().last_failure, None);
}

#[test]
fn unrelated_files_in_the_folder_are_left_alone() {
    let f = fixture();
    std::fs::create_dir_all(f.backup_dir()).unwrap();
    std::fs::write(f.backup_dir().join("notes.txt"), b"mine").unwrap();
    f.lib
        .update_settings(SettingsPatch {
            backup_keep: Some(1),
            ..SettingsPatch::default()
        })
        .unwrap();
    f.lib.snapshot(BackupReason::Manual).unwrap();
    f.lib.snapshot(BackupReason::Manual).unwrap();
    assert!(f.files().contains(&"notes.txt".to_owned()));
    assert_eq!(f.lib.backups().unwrap().len(), 1);
}
