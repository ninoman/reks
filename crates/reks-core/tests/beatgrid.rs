//! Grid editor storage: edits write `manual` rows, lock blocks edits, and
//! both mark the grid as the user's.

// Test helpers outside `#[test]` fns are not covered by clippy's allow-unwrap-in-tests.
#![allow(clippy::unwrap_used)]

use reks_core::{Error, GridEdit, Library};
use rusqlite::Connection;

fn setup() -> (tempfile::TempDir, Library, i64) {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("reks.sqlite");
    let lib = Library::open(&db, &dir.path().join("cache")).unwrap();
    let c = Connection::open(&db).unwrap();
    c.execute(
        "INSERT INTO track (path, file_size, file_mtime, kind, duration_ms, date_added,
                            created_at, updated_at)
         VALUES ('/m/a.wav', 1, ?1, 'wav', 4000, ?1, ?1, ?1)",
        ["2026-01-01T00:00:00Z"],
    )
    .unwrap();
    let id = c.last_insert_rowid();
    c.execute(
        "INSERT INTO beatgrid (track_id, position_ms, bpm_x100, beat_in_bar, source)
         VALUES (?1, 20.4, 12000, 1, 'analysis')",
        [id],
    )
    .unwrap();
    (dir, lib, id)
}

fn sources(lib_dir: &tempfile::TempDir) -> Vec<String> {
    let c = Connection::open(lib_dir.path().join("reks.sqlite")).unwrap();
    let mut s = c.prepare("SELECT source FROM beatgrid").unwrap();
    s.query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

#[test]
fn reading_rounds_to_whole_ms_and_lists_beats() {
    let (_dir, lib, id) = setup();
    let g = lib.beatgrid(id).unwrap();
    assert_eq!(g.anchors[0].position_ms, 20);
    assert_eq!(g.source.as_deref(), Some("analysis"));
    assert!(!g.locked);
    // 20 ms + k × 500 ms up to 4 s.
    assert_eq!(g.beats.len(), 8);
    assert_eq!(g.beats[7].position_us, 3_520_000);
    assert!(!lib.grid_is_user_owned(id).unwrap());
    assert!(lib.user_owned_grids().unwrap().is_empty());
}

#[test]
fn edits_write_manual_rows() {
    let (dir, lib, id) = setup();
    let g = lib
        .edit_beatgrid(id, GridEdit::Nudge { delta_ms: 10 })
        .unwrap();
    assert_eq!(g.anchors[0].position_ms, 30);
    assert_eq!(g.source.as_deref(), Some("manual"));
    assert_eq!(sources(&dir), ["manual"]);
    assert!(lib.grid_is_user_owned(id).unwrap());
    assert_eq!(lib.user_owned_grids().unwrap(), [id].into());
    let g = lib.edit_beatgrid(id, GridEdit::Double).unwrap();
    assert_eq!(g.anchors[0].bpm_x100, 24_000);
    // The track detail summary reads the same rows.
    assert_eq!(
        lib.track(id).unwrap().beatgrid.unwrap().bpm_max_x100,
        24_000
    );
}

#[test]
fn lock_blocks_edits_until_unlocked() {
    let (dir, lib, id) = setup();
    let g = lib
        .edit_beatgrid(id, GridEdit::SetLocked { locked: true })
        .unwrap();
    assert!(g.locked);
    // Locking alone keeps the rows' source but protects them.
    assert_eq!(sources(&dir), ["analysis"]);
    assert!(lib.grid_is_user_owned(id).unwrap());
    assert_eq!(lib.user_owned_grids().unwrap(), [id].into());
    assert!(matches!(
        lib.edit_beatgrid(id, GridEdit::Halve),
        Err(Error::GridLocked(t)) if t == id
    ));
    assert_eq!(lib.beatgrid(id).unwrap().anchors[0].bpm_x100, 12_000);
    lib.edit_beatgrid(id, GridEdit::SetLocked { locked: false })
        .unwrap();
    assert!(!lib.grid_is_user_owned(id).unwrap());
    assert_eq!(
        lib.edit_beatgrid(id, GridEdit::Halve).unwrap().anchors[0].bpm_x100,
        6_000
    );
}

#[test]
fn tap_creates_a_grid_and_errors_change_nothing() {
    let (_dir, lib, id) = setup();
    let c = Connection::open(_dir.path().join("reks.sqlite")).unwrap();
    c.execute("DELETE FROM beatgrid", []).unwrap();
    assert!(matches!(
        lib.edit_beatgrid(id, GridEdit::Nudge { delta_ms: 1 }),
        Err(Error::NoGrid(_))
    ));
    let g = lib
        .edit_beatgrid(
            id,
            GridEdit::SetBpm {
                bpm_x100: 12_800,
                anchor_ms: 100,
            },
        )
        .unwrap();
    assert_eq!(
        (g.anchors[0].position_ms, g.anchors[0].bpm_x100),
        (100, 12_800)
    );
    assert!(lib
        .edit_beatgrid(
            id,
            GridEdit::SetBpm {
                bpm_x100: 1,
                anchor_ms: 0
            }
        )
        .is_err());
    assert_eq!(lib.beatgrid(id).unwrap().anchors[0].bpm_x100, 12_800);
    assert!(matches!(lib.beatgrid(999), Err(Error::TrackNotFound(999))));
}
