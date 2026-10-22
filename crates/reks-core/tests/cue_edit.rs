//! Cue editing from a deck (Phase 5 task 17): set, rename and delete round
//! trip through the library; refused edits write nothing.

#![allow(clippy::unwrap_used)]

use reks_core::{Error, Library, HOT_CUE_PALETTE, MAX_CUE_NAME_CHARS};
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
        Env { dir, lib }
    }

    fn db(&self) -> Connection {
        Connection::open(self.dir.path().join("reks.sqlite")).unwrap()
    }

    /// A 5-minute track at `path`.
    fn track(&self, path: &str) -> i64 {
        let c = self.db();
        c.execute(
            "INSERT INTO track (path, file_size, file_mtime, kind, title, duration_ms, date_added,
                                created_at, updated_at)
             VALUES (?1, 1, 'x', 'mp3', 'A', 300000, '2026-01-01', 'old', 'old')",
            [path],
        )
        .unwrap();
        c.last_insert_rowid()
    }

    fn sources(&self, id: i64) -> Vec<String> {
        let c = self.db();
        let mut s = c
            .prepare("SELECT source FROM cue WHERE track_id = ?1 ORDER BY id")
            .unwrap();
        s.query_map([id], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }
}

#[test]
fn set_rename_and_delete_round_trip() {
    let env = Env::new();
    let id = env.track("/a.mp3");

    let a = env.lib.set_hot_cue(id, 0, 1_000.25).unwrap();
    let c = env.lib.set_hot_cue(id, 2, 4_000.0).unwrap();
    assert_eq!(
        (a.slot, a.kind, a.start_ms, a.end_ms),
        (Some(0), 0, 1_000.25, None)
    );
    assert_eq!(a.color_rgb, Some(HOT_CUE_PALETTE[0]));
    assert_eq!(c.color_rgb, Some(HOT_CUE_PALETTE[1]), "next unused colour");
    let m = env.lib.add_memory_cue(id, 2_500.0).unwrap();
    assert_eq!((m.slot, m.kind, m.color_rgb), (None, 0, None));
    assert_eq!(
        env.lib.add_memory_cue(id, 2_500.4).unwrap(),
        m,
        "a second press at the same place adds nothing"
    );

    env.lib.rename_cue(id, c.id, Some("  Drop  ")).unwrap();
    env.lib.rename_cue(id, m.id, Some("Break")).unwrap();

    let detail = env.lib.track(id).unwrap();
    let names: Vec<_> = detail
        .cues
        .iter()
        .map(|c| (c.slot, c.start_ms, c.name.as_deref()))
        .collect();
    assert_eq!(
        names,
        [
            (Some(0), 1_000.25, None),
            (Some(2), 4_000.0, Some("Drop")),
            (None, 2_500.0, Some("Break")),
        ]
    );
    assert_ne!(detail.track.updated_at, "old");
    assert_eq!(env.sources(id), ["manual"; 3]);

    env.lib.rename_cue(id, c.id, Some("   ")).unwrap();
    env.lib.delete_cue(id, a.id).unwrap();
    let cues = env.lib.track(id).unwrap().cues;
    assert_eq!(cues.len(), 2);
    assert_eq!(cues[0].name, None, "a blank name clears it");
    // Slot A is free again and takes the first colour back.
    let a2 = env.lib.set_hot_cue(id, 0, 500.0).unwrap();
    assert_eq!(a2.color_rgb, Some(HOT_CUE_PALETTE[0]));
}

#[test]
fn refused_edits_write_nothing() {
    let env = Env::new();
    let id = env.track("/a.mp3");
    let other = env.track("/b.mp3");
    let a = env.lib.set_hot_cue(id, 0, 1_000.0).unwrap();

    let invalid = |e: Error| matches!(e, Error::InvalidField { .. });
    assert!(
        invalid(env.lib.set_hot_cue(id, 0, 2_000.0).unwrap_err()),
        "slot taken"
    );
    assert!(
        invalid(env.lib.set_hot_cue(id, 8, 2_000.0).unwrap_err()),
        "no slot I"
    );
    assert!(invalid(env.lib.set_hot_cue(id, 1, -1.0).unwrap_err()));
    assert!(
        invalid(env.lib.set_hot_cue(id, 1, 300_001.0).unwrap_err()),
        "past the end"
    );
    assert!(invalid(env.lib.add_memory_cue(id, f64::NAN).unwrap_err()));
    let long = "x".repeat(MAX_CUE_NAME_CHARS + 1);
    assert!(invalid(
        env.lib.rename_cue(id, a.id, Some(&long)).unwrap_err()
    ));
    assert!(matches!(
        env.lib.rename_cue(other, a.id, Some("x")).unwrap_err(),
        Error::CueNotFound(_)
    ));
    assert!(matches!(
        env.lib.delete_cue(other, a.id).unwrap_err(),
        Error::CueNotFound(_)
    ));
    assert!(matches!(
        env.lib.set_hot_cue(9_999, 1, 0.0).unwrap_err(),
        Error::TrackNotFound(9_999)
    ));
    assert_eq!(env.lib.track(id).unwrap().cues, [a]);
}
