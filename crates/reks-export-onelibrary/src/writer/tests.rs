use super::*;
use crate::diff::{diff, DiffOptions, Difference};
use crate::testutil::{reference_dump, reference_stick, TEST_KEY};
use crate::validate::validate;

fn files(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_owned()];
    while let Some(dir) = stack.pop() {
        for e in std::fs::read_dir(dir).unwrap() {
            let e = e.unwrap();
            if e.file_type().unwrap().is_dir() {
                stack.push(e.path());
            } else {
                out.push(e.file_name().to_string_lossy().into_owned());
            }
        }
    }
    out.sort();
    out
}

#[test]
fn writes_one_encrypted_file_beside_export_pdb() {
    let stick = reference_stick();
    let before = files(stick.path());
    let w = OneLibraryWriter::default()
        .write_db(TEST_KEY, stick.path())
        .unwrap();
    assert_eq!(w.tracks, 7);
    let db = stick.path().join("PIONEER/rekordbox/exportLibrary.db");
    let bytes = std::fs::read(&db).unwrap();
    assert_eq!(w.bytes, bytes.len() as u64);
    assert!(!bytes.starts_with(b"SQLite format 3\0"));
    // Only the database is new: no temp file, -wal, -shm or `._` file.
    let mut expected = before;
    expected.push("exportLibrary.db".into());
    expected.sort();
    assert_eq!(files(stick.path()), expected);
}

#[test]
fn written_database_validates_and_matches_the_reference() {
    let stick = reference_stick();
    OneLibraryWriter::default()
        .write_db(TEST_KEY, stick.path())
        .unwrap();
    assert_eq!(validate(stick.path(), TEST_KEY).unwrap(), []);

    let db = stick.path().join("PIONEER/rekordbox/exportLibrary.db");
    let reader = OneLibraryReader::open(&db, TEST_KEY).unwrap();
    let journal: String = reader_conn_pragma(&db, "journal_mode");
    assert_eq!(journal, "wal");
    let ours = reader.dump().unwrap();
    // Default diff (ids and dates ignored): only the documented My Tag
    // difference (the reference holds its user's column names).
    let diffs = diff(&ours, &reference_dump(), DiffOptions::default());
    let unexpected: Vec<String> = diffs
        .iter()
        .filter(|d| !matches!(d, Difference::Row { table, .. } if table == "myTag"))
        .filter(
            |d| !matches!(d, Difference::Property { column, .. } if column == "myTagMasterDBID"),
        )
        .map(ToString::to_string)
        .collect();
    assert!(unexpected.is_empty(), "{unexpected:#?}");
}

fn reader_conn_pragma(db: &Path, pragma: &str) -> String {
    let dir = tempfile::tempdir().unwrap();
    let copy = dir.path().join("x.db");
    std::fs::copy(db, &copy).unwrap();
    let conn = cipher::open_read_write(&copy, TEST_KEY, PROFILE).unwrap();
    conn.query_row(&format!("PRAGMA {pragma}"), [], |r| r.get(0))
        .unwrap()
}

#[test]
fn rewriting_replaces_the_file() {
    let stick = reference_stick();
    let w = OneLibraryWriter::default();
    w.write_db(TEST_KEY, stick.path()).unwrap();
    let first = std::fs::read(stick.path().join("PIONEER/rekordbox/exportLibrary.db")).unwrap();
    w.write_db(TEST_KEY, stick.path()).unwrap();
    let second = std::fs::read(stick.path().join("PIONEER/rekordbox/exportLibrary.db")).unwrap();
    // A new salt each time, same content.
    assert_ne!(first, second);
    assert_eq!(validate(stick.path(), TEST_KEY).unwrap(), []);
}

#[test]
fn no_export_pdb_writes_nothing() {
    let stick = tempfile::tempdir().unwrap();
    let err = OneLibraryWriter::default()
        .write_db(TEST_KEY, stick.path())
        .unwrap_err();
    assert!(matches!(err, OneLibraryWriteError::Build(_)), "{err:?}");
    assert!(files(stick.path()).is_empty());
}

#[test]
fn errors_never_contain_the_key() {
    let stick = tempfile::tempdir().unwrap();
    let err = OneLibraryWriter::default()
        .write_db(TEST_KEY, stick.path())
        .unwrap_err();
    assert!(!format!("{err} {err:?}").contains(TEST_KEY));
}

#[test]
fn existing_database_is_read_with_the_key() {
    let stick = reference_stick();
    assert_eq!(
        existing(stick.path(), TEST_KEY).unwrap(),
        ExistingLibrary::Absent
    );
    OneLibraryWriter::default()
        .write_db(TEST_KEY, stick.path())
        .unwrap();
    assert_eq!(
        existing(stick.path(), TEST_KEY).unwrap(),
        ExistingLibrary::Read {
            sessions: 0,
            entries: 0,
            history: Vec::new(),
        }
    );
    assert!(matches!(
        existing(stick.path(), "another-key").unwrap(),
        ExistingLibrary::NotRead { .. }
    ));
}

#[test]
fn estimate_is_close_to_the_built_size() {
    let stick = reference_stick();
    let w = OneLibraryWriter::default()
        .write_db(TEST_KEY, stick.path())
        .unwrap();
    let est = estimated_bytes(7, 7);
    // Within a few pages either way.
    assert!(
        w.bytes.abs_diff(est) <= 4 * 4096,
        "built {} est {est}",
        w.bytes
    );
}
