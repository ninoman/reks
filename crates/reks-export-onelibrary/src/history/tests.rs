//! The history reader on a reference database with history: the rekordbox
//! 7.2.19 reference rows (built from its `export.pdb`), plus `history` rows
//! in the reference's columns, left in `-wal` as rekordbox leaves its rows.

use std::fs;
use std::path::Path;

use reks_core::history::{self as library_history, ImportChoices, SessionAction};
use reks_core::Library;
use reks_export_rb::history::{plan_backups, read_history, run_backups, HistoryPolicy};

use super::*;
use crate::testutil::{reference_stick, TEST_KEY};
use crate::writer::{build_file, OneLibraryWriter};
use crate::RbVersion;

const SERIAL: &str = "4993-1CFB";

/// `history` (id, sequenceNo, name, attribute, parent) and
/// `history_content` (history id, content id, sequenceNo) rows. Content ids
/// are the reference's: 2–4 and 7 by "reks fixtures", 7 with length 0.
const HEADS: &str = "
INSERT INTO history VALUES (1, 0, '2026', 1, 0);
INSERT INTO history VALUES (2, 0, 'HISTORY 001', 0, 1);
INSERT INTO history VALUES (3, 1, 'HISTORY 002', 0, 1);
INSERT INTO history VALUES (4, 2, 'HISTORY 003', 0, 1);";
/// Out of row order on purpose: entries play in `sequenceNo` order.
const ENTRIES: &str = "
INSERT INTO history_content VALUES (2, 1, 3);
INSERT INTO history_content VALUES (2, 3, 1);
INSERT INTO history_content VALUES (2, 6, 2);
INSERT INTO history_content VALUES (3, 7, 1);
INSERT INTO history_content VALUES (3, 99, 2);";

/// The reference stick with an `exportLibrary.db` of its rows plus `sql`,
/// whose last writes are only in `exportLibrary.db-wal`.
fn stick_with(sql: &str) -> tempfile::TempDir {
    let stick = reference_stick();
    let lib = OneLibraryWriter::default().rows(stick.path()).unwrap();
    let work = tempfile::tempdir().unwrap();
    let db = work.path().join("exportLibrary.db");
    build_file(&db, TEST_KEY, RbVersion::LATEST, &lib).unwrap();
    let conn = crate::cipher::open_read_write(&db, TEST_KEY, crate::schema::PROFILE).unwrap();
    conn.pragma_update(None, "wal_autocheckpoint", 0).unwrap();
    conn.execute_batch(sql).unwrap();
    // Copied while the connection is open, so the rows stay in -wal.
    let rb = stick.path().join("PIONEER/rekordbox");
    for name in ["exportLibrary.db", "exportLibrary.db-wal"] {
        fs::copy(work.path().join(name), rb.join(name)).unwrap();
    }
    drop(conn);
    stick
}

fn played_stick() -> tempfile::TempDir {
    stick_with(&format!("{HEADS}{ENTRIES}"))
}

fn read_stick(root: &Path) -> SourceHistory {
    read_play_history(
        &HistorySource::Stick(root.to_owned()),
        TEST_KEY,
        Some(SERIAL.into()),
    )
    .unwrap()
}

fn entry(path: &str, title: &str, artist: Option<&str>, secs: Option<i64>) -> SourceEntry {
    SourceEntry {
        stick_path: Some(path.into()),
        title: Some(title.into()),
        artist: artist.map(str::to_owned),
        duration_ms: secs.map(|s| s * 1000),
    }
}

#[test]
fn reads_sessions_in_order_with_their_tracks() {
    let stick = played_stick();
    let h = read_stick(stick.path());
    assert_eq!(h.volume_serial.as_deref(), Some(SERIAL));
    let session = |name: &str, entries| SourceSession {
        library: HistoryLibrary::OneLibrary,
        source_name: name.into(),
        played_on: None,
        entries,
    };
    assert_eq!(
        h.sessions,
        [
            session(
                "HISTORY 001",
                vec![
                    entry(
                        "/Contents/reks fixtures/UnknownAlbum/sine-click-128.m4a",
                        "Sine Click 128",
                        Some("reks fixtures"),
                        Some(30)
                    ),
                    entry(
                        "/Contents/UnknownArtist/UnknownAlbum/sine-click-128-lame.mp3",
                        "sine-click-128-lame",
                        None,
                        Some(30)
                    ),
                    entry(
                        "/Contents/UnknownArtist/UnknownAlbum/sine-click-128.wav",
                        "sine-click-128",
                        None,
                        Some(29)
                    ),
                ]
            ),
            session(
                "HISTORY 002",
                vec![
                    entry(
                        "/Contents/reks fixtures/UnknownAlbum/sine-click-128-hires.m4a",
                        "Sine Click 128",
                        Some("reks fixtures"),
                        None
                    ),
                    // No content row 99.
                    SourceEntry::default(),
                ]
            ),
            // An empty playlist-like row is a session; the folder is not.
            session("HISTORY 003", Vec::new()),
        ]
    );
}

#[test]
fn imports_entries_in_order_and_reimport_adds_nothing() {
    let stick = played_stick();
    let before = fs::read(stick.path().join("PIONEER/rekordbox/exportLibrary.db-wal")).unwrap();
    let lib = Library::open_in_memory(Path::new("cache-unused")).unwrap();

    let plan = library_history::scan(&read_stick(stick.path()), &lib).unwrap();
    assert!(plan.sessions.iter().all(|s| s.action == SessionAction::New));
    let report = library_history::import(&plan, &ImportChoices::default(), &lib).unwrap();
    assert_eq!((report.created.len(), report.entries), (3, 5));
    let first = lib.session(report.created[0]).unwrap();
    assert_eq!(first.session.library, HistoryLibrary::OneLibrary);
    assert_eq!(first.volume_serial.as_deref(), Some(SERIAL));
    let paths: Vec<&str> = first
        .entries
        .iter()
        .filter_map(|e| e.raw_path.as_deref())
        .collect();
    assert_eq!(
        paths,
        [
            "/Contents/reks fixtures/UnknownAlbum/sine-click-128.m4a",
            "/Contents/UnknownArtist/UnknownAlbum/sine-click-128-lame.mp3",
            "/Contents/UnknownArtist/UnknownAlbum/sine-click-128.wav",
        ]
    );

    let again = library_history::scan(&read_stick(stick.path()), &lib).unwrap();
    assert!(again
        .sessions
        .iter()
        .all(|s| matches!(s.action, SessionAction::Skip(_))));
    let report = library_history::import(&again, &ImportChoices::default(), &lib).unwrap();
    assert!(report.created.is_empty() && report.extended.is_empty());
    assert_eq!(report.entries, 0);
    assert_eq!(lib.sessions(&Default::default()).unwrap().len(), 3);

    // The stick was read from a copy.
    assert_eq!(
        fs::read(stick.path().join("PIONEER/rekordbox/exportLibrary.db-wal")).unwrap(),
        before
    );
}

#[test]
fn reads_a_backup_with_its_serial_and_refuses_a_changed_one() {
    let stick = played_stick();
    let data = tempfile::tempdir().unwrap();
    let h = read_history(stick.path(), Some(SERIAL.into()));
    let plan = plan_backups(stick.path(), &h, HistoryPolicy::BackUp, false).unwrap();
    let backup = run_backups(&plan, data.path()).unwrap().unwrap();
    let src = HistorySource::Backup(backup.dir.clone());

    let read = read_play_history(&src, TEST_KEY, None).unwrap();
    assert_eq!(read, read_stick(stick.path()));

    let wal = backup.dir.join("PIONEER/rekordbox/exportLibrary.db-wal");
    let mut bytes = fs::read(&wal).unwrap();
    bytes[100] ^= 0xff;
    fs::write(&wal, &bytes).unwrap();
    let err = read_play_history(&src, TEST_KEY, None).unwrap_err();
    assert!(
        matches!(
            err,
            HistoryReadError::Backup(HistoryError::HashMismatch { .. })
        ),
        "{err:?}"
    );

    fs::remove_file(&wal).unwrap();
    let err = read_play_history(&src, TEST_KEY, None).unwrap_err();
    assert!(err.to_string().contains("missing"), "{err}");
}

#[test]
fn no_database_no_sessions_and_a_wrong_key_is_an_error() {
    let stick = reference_stick();
    assert!(read_stick(stick.path()).sessions.is_empty());

    let stick = played_stick();
    let err = read_play_history(
        &HistorySource::Stick(stick.path().to_owned()),
        "wrong",
        None,
    )
    .unwrap_err();
    assert!(
        matches!(err, HistoryReadError::Database(Error::NotDecrypted { .. })),
        "{err:?}"
    );
}

#[test]
fn folders_are_not_sessions_unless_they_hold_entries() {
    let head = |id, name: &str, attribute, parent| Head {
        id,
        name: Some(name.into()),
        attribute: Some(attribute),
        parent: Some(parent),
    };
    let played = |history_id| Played {
        history_id,
        entry: SourceEntry::default(),
    };
    let heads = vec![
        head(1, "parent", 0, 0),
        head(2, "child", 0, 1),
        head(3, "folder with plays", 1, 0),
        head(4, "empty folder", 1, 0),
    ];
    let got = sessions(heads, vec![played(9), played(3), played(2)]);
    let names: Vec<(&str, usize)> = got
        .iter()
        .map(|s| (s.source_name.as_str(), s.entries.len()))
        .collect();
    assert_eq!(
        names,
        [("child", 1), ("folder with plays", 1), ("HISTORY #9", 1)]
    );
}
