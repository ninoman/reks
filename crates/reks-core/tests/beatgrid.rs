//! Grid editor storage: edits write `manual` rows, lock blocks edits, and
//! both mark the grid as the user's.

// Test helpers outside `#[test]` fns are not covered by clippy's allow-unwrap-in-tests.
#![allow(clippy::unwrap_used)]

use reks_core::beatgrid::GridProblem;
use reks_core::DataSource;
use reks_core::{Error, GridAnchor, GridEdit, Library};
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
    assert_eq!(g.source, Some(DataSource::Analysis));
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
        .edit_beatgrid(
            id,
            GridEdit::Nudge {
                delta_ms: 10,
                section: None,
            },
        )
        .unwrap();
    assert_eq!(g.anchors[0].position_ms, 30);
    assert_eq!(g.source, Some(DataSource::Manual));
    assert_eq!(sources(&dir), ["manual"]);
    assert!(lib.grid_is_user_owned(id).unwrap());
    assert_eq!(lib.user_owned_grids().unwrap(), [id].into());
    let g = lib
        .edit_beatgrid(id, GridEdit::Double { section: None })
        .unwrap();
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
        lib.edit_beatgrid(id, GridEdit::Halve { section: None }),
        Err(Error::GridLocked(t)) if t == id
    ));
    assert_eq!(lib.beatgrid(id).unwrap().anchors[0].bpm_x100, 12_000);
    lib.edit_beatgrid(id, GridEdit::SetLocked { locked: false })
        .unwrap();
    assert!(!lib.grid_is_user_owned(id).unwrap());
    assert_eq!(
        lib.edit_beatgrid(id, GridEdit::Halve { section: None })
            .unwrap()
            .anchors[0]
            .bpm_x100,
        6_000
    );
}

#[test]
fn tap_creates_a_grid_and_errors_change_nothing() {
    let (_dir, lib, id) = setup();
    let c = Connection::open(_dir.path().join("reks.sqlite")).unwrap();
    c.execute("DELETE FROM beatgrid", []).unwrap();
    assert!(matches!(
        lib.edit_beatgrid(
            id,
            GridEdit::Nudge {
                delta_ms: 1,
                section: None
            }
        ),
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

fn anchor(position_ms: i64, bpm_x100: i64) -> GridAnchor {
    GridAnchor {
        position_ms,
        bpm_x100,
        beat_in_bar: 1,
        beats_per_bar: 4,
    }
}

fn track_bpm(dir: &tempfile::TempDir, id: i64) -> (i64, String) {
    let c = Connection::open(dir.path().join("reks.sqlite")).unwrap();
    c.query_row(
        "SELECT bpm_x100, bpm_source FROM track WHERE id = ?1",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .unwrap()
}

#[test]
fn sections_are_edited_and_the_bpm_follows_the_longest() {
    let (dir, lib, id) = setup();
    // 4 s track at 120 BPM from 20 ms; a second section from 1520 ms.
    let g = lib
        .edit_beatgrid(id, GridEdit::AddMarker { at_ms: 1_520 })
        .unwrap();
    assert_eq!(g.anchors.len(), 2);
    let g = lib
        .edit_beatgrid(
            id,
            GridEdit::SetSectionBpm {
                section: 1,
                bpm_x100: 15_000,
            },
        )
        .unwrap();
    assert_eq!(g.anchors[1].bpm_x100, 15_000);
    // Beats: 20, 520, 1020 at 120; 1520, 1920, ... at 150.
    let ms: Vec<i64> = g.beats.iter().map(|b| b.position_us / 1000).collect();
    assert_eq!(
        ms,
        [20, 520, 1020, 1520, 1920, 2320, 2720, 3120, 3520, 3920]
    );
    // The 150 BPM section covers 2.48 of the 4 s.
    assert_eq!(track_bpm(&dir, id), (15_000, "manual".into()));
    assert_eq!(sources(&dir), ["manual", "manual"]);
}

#[test]
fn degenerate_edits_change_nothing() {
    let (dir, lib, id) = setup();
    let before = lib.beatgrid(id).unwrap();
    let err = lib
        .edit_beatgrid(
            id,
            GridEdit::SetAnchors {
                anchors: vec![anchor(0, 12_000), anchor(100, 12_000)],
            },
        )
        .unwrap_err();
    assert!(matches!(
        err,
        Error::InvalidGrid {
            problem: GridProblem::TooClose { index: 1 },
            ..
        }
    ));
    assert_eq!(lib.beatgrid(id).unwrap(), before);
    assert_eq!(sources(&dir), ["analysis"]);
}

#[test]
fn reset_brings_back_the_grid_from_before_the_first_edit() {
    let (dir, lib, id) = setup();
    lib.edit_beatgrid(
        id,
        GridEdit::Nudge {
            delta_ms: 10,
            section: None,
        },
    )
    .unwrap();
    lib.edit_beatgrid(id, GridEdit::Double { section: None })
        .unwrap();
    let g = lib
        .edit_beatgrid(id, GridEdit::Reset { section: None })
        .unwrap();
    assert_eq!(g.anchors, [anchor(20, 12_000)]);
    assert_eq!(g.source, Some(DataSource::Analysis));
    assert!(!lib.grid_is_user_owned(id).unwrap());
    assert_eq!(track_bpm(&dir, id), (12_000, "analysis".into()));
    // The base went with the reset; the next edit keeps a new one.
    lib.edit_beatgrid(
        id,
        GridEdit::Nudge {
            delta_ms: 5,
            section: None,
        },
    )
    .unwrap();
    let g = lib
        .edit_beatgrid(id, GridEdit::Reset { section: Some(0) })
        .unwrap();
    assert_eq!(g.anchors, [anchor(20, 12_000)]);
    assert_eq!(g.source, Some(DataSource::Manual));
}

#[test]
fn a_grid_created_from_nothing_resets_to_nothing() {
    let (dir, lib, id) = setup();
    let c = Connection::open(dir.path().join("reks.sqlite")).unwrap();
    c.execute("DELETE FROM beatgrid", []).unwrap();
    lib.edit_beatgrid(
        id,
        GridEdit::SetBpm {
            bpm_x100: 12_800,
            anchor_ms: 0,
        },
    )
    .unwrap();
    let g = lib
        .edit_beatgrid(id, GridEdit::Reset { section: None })
        .unwrap();
    assert!(g.anchors.is_empty());
    assert!(sources(&dir).is_empty());
}

#[test]
fn grids_edited_before_the_base_reset_to_the_analysis() {
    let (dir, lib, id) = setup();
    // As an edit before migration 0013 left it: manual rows, no base.
    let c = Connection::open(dir.path().join("reks.sqlite")).unwrap();
    c.execute("UPDATE beatgrid SET source = 'manual'", [])
        .unwrap();
    assert!(matches!(
        lib.edit_beatgrid(id, GridEdit::Reset { section: None }),
        Err(Error::NoGridBase(_))
    ));
    let g = lib
        .edit_beatgrid_with(id, GridEdit::Reset { section: None }, || {
            Some(vec![anchor(33, 12_100)])
        })
        .unwrap();
    assert_eq!(g.anchors, [anchor(33, 12_100)]);
    assert_eq!(g.source, Some(DataSource::Analysis));
    // The analysis is only asked for a reset.
    lib.edit_beatgrid_with(
        id,
        GridEdit::Nudge {
            delta_ms: 1,
            section: None,
        },
        || panic!("not a reset"),
    )
    .unwrap();
}
