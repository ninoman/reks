//! Folder-scan scenarios on a temp dir: add, rescan, modify, move, delete,
//! restore, cancel, unplugged root, NFC names.

// Test helpers outside `#[test]` fns are not covered by clippy's allow-unwrap-in-tests.
#![allow(clippy::unwrap_used)]

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use lofty::config::WriteOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::tag::{Accessor, Tag};
use reks_core::{scan, CancelToken, Library, ScanEvent, ScanReport};
use rusqlite::Connection;

struct Env {
    dir: tempfile::TempDir,
    lib: Library,
}

impl Env {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let lib =
            Library::open(&dir.path().join("reks.sqlite"), &dir.path().join("cache")).unwrap();
        std::fs::create_dir_all(dir.path().join("music/set")).unwrap();
        Env { dir, lib }
    }

    fn root(&self) -> PathBuf {
        self.dir.path().join("music")
    }

    /// Copies `fixtures/audio/sine-click-128.<ext>` to `music/<rel>`.
    fn add_fixture(&self, ext: &str, rel: &str) -> PathBuf {
        let src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/audio")
            .join(format!("sine-click-128.{ext}"));
        let dst = self.root().join(rel);
        std::fs::create_dir_all(dst.parent().unwrap()).unwrap();
        std::fs::copy(src, &dst).unwrap();
        dst
    }

    fn scan_with(&self, roots: &[PathBuf], cancel: &CancelToken) -> (ScanReport, Vec<ScanEvent>) {
        let events = RefCell::new(Vec::new());
        let report = scan(roots, &self.lib, cancel, |e| {
            events.borrow_mut().push(e.clone())
        })
        .unwrap();
        (report, events.into_inner())
    }

    fn scan(&self) -> (ScanReport, Vec<ScanEvent>) {
        self.scan_with(&[self.root()], &CancelToken::new())
    }

    /// Separate read connection (WAL allows it alongside the library's).
    fn db(&self) -> Connection {
        Connection::open(self.dir.path().join("reks.sqlite")).unwrap()
    }

    fn track(&self, file_name: &str) -> (i64, String, bool) {
        self.db()
            .query_row(
                "SELECT id, path, missing FROM track WHERE path LIKE '%' || ?1",
                [file_name],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap()
    }

    fn count(&self, sql: &str) -> i64 {
        self.db().query_row(sql, [], |r| r.get(0)).unwrap()
    }
}

/// `p` as the scan reports it. Re-collecting the components gives native
/// separators (`join("set/x.mp3")` keeps the `/` on Windows).
fn path_str(p: &Path) -> String {
    p.components()
        .collect::<PathBuf>()
        .to_str()
        .unwrap()
        .to_owned()
}

/// Five fixtures, plus files the scan must skip or reject.
fn populated() -> Env {
    let env = Env::new();
    for ext in ["wav", "aiff", "flac", "mp3", "m4a"] {
        env.add_fixture(ext, &format!("set/sine.{ext}"));
    }
    std::fs::write(env.root().join("notes.txt"), "not audio").unwrap();
    std::fs::write(env.root().join("set/._sine.mp3"), "AppleDouble").unwrap();
    env.add_fixture("mp3", ".hidden/sine.mp3");
    std::fs::write(env.root().join("bad.mp3"), "not really an mp3").unwrap();
    env
}

#[test]
fn first_scan_adds_audio_and_skips_the_rest() {
    let env = populated();
    let (report, events) = env.scan();
    assert_eq!(report.found, 6, "{report:?}"); // five fixtures + bad.mp3
    assert_eq!(report.added, 5);
    assert_eq!(report.errors.len(), 1);
    assert!(report.errors[0].path.ends_with("bad.mp3"));
    assert!(!report.cancelled);
    assert_eq!(env.count("SELECT count(*) FROM track"), 5);
    assert_eq!(
        env.count("SELECT count(*) FROM track WHERE content_hash IS NULL OR missing = 1"),
        0
    );

    let (kind, title, dur, added): (String, Option<String>, i64, String) = env
        .db()
        .query_row(
            "SELECT kind, title, duration_ms, date_added FROM track WHERE path LIKE '%sine.flac'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(kind, "flac");
    assert_eq!(title.as_deref(), Some("Sine Click 128"));
    assert_eq!(dur, 30_000);
    assert_eq!(added.len(), 10);

    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, ScanEvent::Added { .. }))
            .count(),
        5
    );
    assert!(matches!(
        events
            .iter()
            .rev()
            .find(|e| matches!(e, ScanEvent::Progress { .. })),
        Some(ScanEvent::Progress { done: 6, total: 6 })
    ));
}

#[test]
fn rescan_is_idempotent() {
    let env = populated();
    env.scan();
    let (report, _) = env.scan();
    assert_eq!((report.added, report.updated, report.unchanged), (0, 0, 5));
    assert_eq!((report.moved, report.missing), (0, 0));
    assert_eq!(env.count("SELECT count(*) FROM track"), 5);
}

#[test]
fn changed_file_refreshes_but_keeps_library_metadata() {
    let env = populated();
    env.scan();
    let (id, _, _) = env.track("sine.mp3");

    let path = env.root().join("set/sine.mp3");
    let mut file = lofty::read_from_path(&path).unwrap();
    if file.primary_tag().is_none() {
        file.insert_tag(Tag::new(file.primary_tag_type()));
    }
    let tag = file.primary_tag_mut().unwrap();
    tag.set_title("Edited Title".into());
    tag.set_album("New Album".into());
    file.save_to_path(&path, WriteOptions::default()).unwrap();

    let (report, events) = env.scan();
    assert_eq!((report.updated, report.unchanged), (1, 4), "{report:?}");
    assert!(events.contains(&ScanEvent::Updated {
        id,
        path: path_str(&path)
    }));
    let (title, album): (String, String) = env
        .db()
        .query_row("SELECT title, album FROM track WHERE id = ?1", [id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(
        title, "Sine Click 128",
        "existing title must not be overwritten"
    );
    assert_eq!(album, "New Album", "empty field is filled from tags");
}

#[test]
fn moved_file_keeps_its_row() {
    let env = populated();
    env.scan();
    let (id, old, _) = env.track("sine.flac");
    let new = env.root().join("archive/renamed.flac");
    std::fs::create_dir_all(new.parent().unwrap()).unwrap();
    std::fs::rename(&old, &new).unwrap();

    let (report, events) = env.scan();
    assert_eq!(
        (report.moved, report.added, report.missing),
        (1, 0, 0),
        "{report:?}"
    );
    assert!(events.contains(&ScanEvent::Moved {
        id,
        from: old,
        to: path_str(&new)
    }));
    assert_eq!(env.track("renamed.flac"), (id, path_str(&new), false));
}

#[test]
fn deleted_file_is_marked_missing_then_restored() {
    let env = populated();
    env.scan();
    let (id, path, _) = env.track("sine.wav");
    std::fs::remove_file(&path).unwrap();

    let (report, events) = env.scan();
    assert_eq!(report.missing, 1);
    assert!(events.contains(&ScanEvent::Missing {
        id,
        path: path.clone()
    }));
    assert!(env.track("sine.wav").2);

    // Already missing: not reported again.
    assert_eq!(env.scan().0.missing, 0);

    env.add_fixture("wav", "set/sine.wav");
    let (report, _) = env.scan();
    assert_eq!((report.updated, report.added), (1, 0), "{report:?}");
    assert_eq!(env.track("sine.wav"), (id, path, false));
}

#[test]
fn file_moved_back_into_root_after_going_missing() {
    let env = populated();
    env.scan();
    let (id, path, _) = env.track("sine.aiff");
    let outside = env.dir.path().join("elsewhere.aiff");
    std::fs::rename(&path, &outside).unwrap();
    assert_eq!(env.scan().0.missing, 1);

    let back = env.root().join("returned.aiff");
    std::fs::rename(&outside, &back).unwrap();
    let (report, _) = env.scan();
    assert_eq!((report.moved, report.added), (1, 0), "{report:?}");
    assert_eq!(env.track("returned.aiff"), (id, path_str(&back), false));
}

#[test]
fn cancelled_scan_marks_nothing_missing() {
    let env = populated();
    env.scan();
    let (_, path, _) = env.track("sine.m4a");
    std::fs::remove_file(&path).unwrap();

    let cancel = CancelToken::new();
    cancel.cancel();
    let (report, _) = env.scan_with(&[env.root()], &cancel);
    assert!(report.cancelled);
    assert_eq!(report.missing, 0);
    assert_eq!(env.count("SELECT count(*) FROM track WHERE missing = 1"), 0);
}

#[test]
fn unplugged_root_marks_nothing_missing() {
    let env = populated();
    env.scan();
    std::fs::rename(env.root(), env.dir.path().join("unplugged")).unwrap();

    let (report, _) = env.scan();
    assert_eq!(report.errors.len(), 1, "{report:?}");
    assert_eq!(report.missing, 0);
    assert_eq!(env.count("SELECT count(*) FROM track WHERE missing = 1"), 0);
}

#[test]
fn records_last_scan_for_known_roots() {
    let env = populated();
    env.db()
        .execute(
            "INSERT INTO library_root (path) VALUES (?1)",
            [path_str(&env.root())],
        )
        .unwrap();
    env.scan();
    assert_eq!(
        env.count("SELECT count(*) FROM library_root WHERE last_scan IS NOT NULL"),
        1
    );
}

#[cfg(target_os = "macos")]
#[test]
fn decomposed_names_are_stored_nfc_on_macos() {
    let env = Env::new();
    env.add_fixture("wav", "Cafe\u{301}.wav"); // NFD "é"
    env.scan();
    let (_, path, _) = env.track(".wav");
    assert!(path.ends_with("Caf\u{e9}.wav"), "{path:?}");
    assert!(Path::new(&path).exists());
}
