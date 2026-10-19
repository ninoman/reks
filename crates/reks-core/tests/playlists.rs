//! `Library::playlists` and `Library::playlist_mutate`: every op, each rejected
//! case, dense positions and sort orders, folder delete, the cycle check,
//! `updated_at`, the duplicate modes and the per-entry playlist view.

// Test helpers outside `#[test]` fns are not covered by clippy's allow-unwrap-in-tests.
#![allow(clippy::unwrap_used, clippy::panic)]

use reks_core::{
    DuplicateMode, Error, Filter, Library, PlaylistOp, PlaylistOutcome, Sort, SortColumn,
    TrackQuery,
};
use rusqlite::Connection;

/// Every `playlist` row (as debug strings) and every `playlist_track` row.
type Snapshot = (Vec<Vec<String>>, Vec<(i64, i64, i64)>);

const OLD: &str = "2026-01-01T00:00:00.000Z";

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

    /// A second connection to the library file for seeding and checks.
    fn db(&self) -> Connection {
        Connection::open(self.dir.path().join("reks.sqlite")).unwrap()
    }

    /// `n` tracks, ids returned in order.
    fn tracks(&self, n: usize) -> Vec<i64> {
        let c = self.db();
        (0..n)
            .map(|i| {
                c.execute(
                    "INSERT INTO track (path, file_size, file_mtime, kind, title, date_added,
                                        created_at, updated_at)
                     VALUES (?1, 1, ?2, 'wav', ?1, '2026-01-01', ?2, ?2)",
                    rusqlite::params![format!("/music/{i}.wav"), OLD],
                )
                .unwrap();
                c.last_insert_rowid()
            })
            .collect()
    }

    fn mutate(&self, op: PlaylistOp) -> PlaylistOutcome {
        self.lib.playlist_mutate(op).unwrap()
    }

    fn create(&self, parent: Option<i64>, name: &str, folder: bool) -> i64 {
        let op = if folder {
            PlaylistOp::CreateFolder {
                parent_id: parent,
                name: name.into(),
                position: None,
            }
        } else {
            PlaylistOp::CreatePlaylist {
                parent_id: parent,
                name: name.into(),
                position: None,
            }
        };
        match self.mutate(op) {
            PlaylistOutcome::Done {
                created_id: Some(id),
                ..
            } => id,
            other => panic!("unexpected {other:?}"),
        }
    }

    fn add(&self, playlist: i64, tracks: &[i64], mode: DuplicateMode) -> PlaylistOutcome {
        self.mutate(PlaylistOp::AddTracks {
            playlist_id: playlist,
            track_ids: tracks.to_vec(),
            before: None,
            duplicates: mode,
        })
    }

    /// Track ids in position order; asserts positions are `0..n`.
    fn entries(&self, playlist: i64) -> Vec<i64> {
        let c = self.db();
        let mut stmt = c
            .prepare(
                "SELECT position, track_id FROM playlist_track WHERE playlist_id = ?1
                 ORDER BY position",
            )
            .unwrap();
        let rows: Vec<(i64, i64)> = stmt
            .query_map([playlist], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        for (i, (pos, _)) in rows.iter().enumerate() {
            assert_eq!(*pos, i as i64, "positions of {playlist} not dense");
        }
        rows.into_iter().map(|(_, t)| t).collect()
    }

    /// Child ids of `parent` in order; asserts `sort_order` is `0..n`.
    fn children(&self, parent: Option<i64>) -> Vec<i64> {
        let c = self.db();
        let mut stmt = c
            .prepare(
                "SELECT sort_order, id FROM playlist WHERE parent_id IS ?1 ORDER BY sort_order, id",
            )
            .unwrap();
        let rows: Vec<(i64, i64)> = stmt
            .query_map([parent], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        for (i, (order, _)) in rows.iter().enumerate() {
            assert_eq!(*order, i as i64, "sort_order under {parent:?} not dense");
        }
        rows.into_iter().map(|(_, id)| id).collect()
    }

    fn name(&self, id: i64) -> String {
        self.db()
            .query_row("SELECT name FROM playlist WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .unwrap()
    }

    fn parent(&self, id: i64) -> Option<i64> {
        self.db()
            .query_row("SELECT parent_id FROM playlist WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .unwrap()
    }

    fn updated_at(&self, id: i64) -> String {
        self.db()
            .query_row("SELECT updated_at FROM playlist WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .unwrap()
    }

    /// Sets every playlist's timestamps to `OLD`.
    fn age(&self) {
        self.db()
            .execute(
                "UPDATE playlist SET created_at = ?1, updated_at = ?1",
                [OLD],
            )
            .unwrap();
    }

    fn snapshot(&self) -> Snapshot {
        let c = self.db();
        let nodes = c
            .prepare("SELECT * FROM playlist ORDER BY id")
            .unwrap()
            .query_map([], |r| {
                (0..7)
                    .map(|i| {
                        r.get::<_, rusqlite::types::Value>(i)
                            .map(|v| format!("{v:?}"))
                    })
                    .collect()
            })
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        let entries = c
            .prepare("SELECT * FROM playlist_track ORDER BY playlist_id, position")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        (nodes, entries)
    }

    /// Rejects `op` with an error matching `check`, and changes nothing.
    fn rejects(&self, op: PlaylistOp, check: impl Fn(&Error) -> bool) {
        let before = self.snapshot();
        let err = self.lib.playlist_mutate(op.clone()).unwrap_err();
        assert!(check(&err), "{op:?} gave {err:?}");
        assert_eq!(self.snapshot(), before, "{op:?} changed the DB");
    }

    fn view(&self, playlist: i64) -> Vec<(i64, Option<i64>)> {
        let q = TrackQuery {
            filters: vec![Filter::Playlist { id: playlist }],
            sort: Sort {
                column: SortColumn::PlaylistPosition,
                descending: false,
            },
            limit: 1000,
            ..TrackQuery::default()
        };
        let page = self.lib.tracks(&q).unwrap();
        assert_eq!(page.total, page.rows.len() as u64);
        page.rows.iter().map(|r| (r.id, r.entry_position)).collect()
    }
}

fn done() -> PlaylistOutcome {
    PlaylistOutcome::Done {
        created_id: None,
        added: 0,
        skipped: 0,
    }
}

fn added(n: u32, skipped: u32) -> PlaylistOutcome {
    PlaylistOutcome::Done {
        created_id: None,
        added: n,
        skipped,
    }
}

#[test]
fn create_at_end_and_at_position() {
    let env = Env::new();
    let a = env.create(None, "A", false);
    let f = env.create(None, "Folder", true);
    let b = env.create(Some(f), "B", false);
    let first = match env.mutate(PlaylistOp::CreatePlaylist {
        parent_id: None,
        name: "  First  ".into(),
        position: Some(0),
    }) {
        PlaylistOutcome::Done {
            created_id: Some(id),
            ..
        } => id,
        other => panic!("{other:?}"),
    };
    assert_eq!(env.children(None), vec![first, a, f]);
    assert_eq!(env.children(Some(f)), vec![b]);
    assert_eq!(env.name(first), "First");

    let tree = env.lib.playlists().unwrap().nodes;
    let shape: Vec<(i64, u32, u32, bool)> = tree
        .iter()
        .map(|n| (n.id, n.depth, n.position, n.is_folder))
        .collect();
    assert_eq!(
        shape,
        vec![
            (first, 0, 0, false),
            (a, 0, 1, false),
            (f, 0, 2, true),
            (b, 1, 0, false),
        ]
    );
}

#[test]
fn rename_and_name_rules() {
    let env = Env::new();
    let a = env.create(None, "A", false);
    assert_eq!(
        env.mutate(PlaylistOp::Rename {
            id: a,
            name: " Warm-up ".into()
        }),
        done()
    );
    assert_eq!(env.name(a), "Warm-up");

    let bad_name = |e: &Error| matches!(e, Error::InvalidField { field: "name", .. });
    for name in ["", "   ", "tab\there", "line\nbreak", "nul\u{0}"] {
        env.rejects(
            PlaylistOp::Rename {
                id: a,
                name: name.into(),
            },
            bad_name,
        );
        env.rejects(
            PlaylistOp::CreateFolder {
                parent_id: None,
                name: name.into(),
                position: None,
            },
            bad_name,
        );
    }
    env.rejects(
        PlaylistOp::Rename {
            id: 999,
            name: "X".into(),
        },
        |e| matches!(e, Error::PlaylistNotFound(999)),
    );
}

#[test]
fn create_rejections() {
    let env = Env::new();
    let p = env.create(None, "P", false);
    env.rejects(
        PlaylistOp::CreatePlaylist {
            parent_id: Some(p),
            name: "X".into(),
            position: None,
        },
        |e| matches!(e, Error::NotAFolder(id) if *id == p),
    );
    env.rejects(
        PlaylistOp::CreatePlaylist {
            parent_id: Some(999),
            name: "X".into(),
            position: None,
        },
        |e| matches!(e, Error::PlaylistNotFound(999)),
    );
    env.rejects(
        PlaylistOp::CreateFolder {
            parent_id: None,
            name: "X".into(),
            position: Some(2),
        },
        |e| matches!(e, Error::IndexOutOfRange { index: 2, len: 1 }),
    );
}

#[test]
fn delete_folder_cascades_and_renumbers() {
    let env = Env::new();
    let t = env.tracks(2);
    let a = env.create(None, "A", false);
    let f = env.create(None, "F", true);
    let sub = env.create(Some(f), "Sub", true);
    let p1 = env.create(Some(f), "P1", false);
    let p2 = env.create(Some(sub), "P2", false);
    let b = env.create(None, "B", false);
    env.add(p1, &t, DuplicateMode::Ask);
    env.add(p2, &t, DuplicateMode::Ask);
    env.add(a, &t[..1], DuplicateMode::Ask);

    assert_eq!(env.mutate(PlaylistOp::Delete { id: f }), done());
    assert_eq!(env.children(None), vec![a, b]);
    let c = env.db();
    let left: i64 = c
        .query_row(
            "SELECT count(*) FROM playlist WHERE id IN (?1, ?2, ?3, ?4)",
            [f, sub, p1, p2],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(left, 0);
    let entries: i64 = c
        .query_row("SELECT count(*) FROM playlist_track", [], |r| r.get(0))
        .unwrap();
    assert_eq!(entries, 1);
    let tracks: i64 = c
        .query_row("SELECT count(*) FROM track", [], |r| r.get(0))
        .unwrap();
    assert_eq!(tracks, 2);

    env.rejects(
        PlaylistOp::Delete { id: f },
        |e| matches!(e, Error::PlaylistNotFound(id) if *id == f),
    );
}

#[test]
fn move_within_and_across_parents() {
    let env = Env::new();
    let a = env.create(None, "A", false);
    let b = env.create(None, "B", false);
    let c = env.create(None, "C", false);
    let f = env.create(None, "F", true);

    // Indices are in the current list: "before C" is 2, also when A moves down.
    env.mutate(PlaylistOp::Move {
        id: a,
        parent_id: None,
        position: 2,
    });
    assert_eq!(env.children(None), vec![b, a, c, f]);
    env.mutate(PlaylistOp::Move {
        id: f,
        parent_id: None,
        position: 0,
    });
    assert_eq!(env.children(None), vec![f, b, a, c]);
    // To the end.
    env.mutate(PlaylistOp::Move {
        id: f,
        parent_id: None,
        position: 4,
    });
    assert_eq!(env.children(None), vec![b, a, c, f]);

    env.mutate(PlaylistOp::Move {
        id: a,
        parent_id: Some(f),
        position: 0,
    });
    env.mutate(PlaylistOp::Move {
        id: b,
        parent_id: Some(f),
        position: 0,
    });
    assert_eq!(env.children(None), vec![c, f]);
    assert_eq!(env.children(Some(f)), vec![b, a]);
    assert_eq!(env.parent(a), Some(f));

    env.mutate(PlaylistOp::Move {
        id: a,
        parent_id: None,
        position: 1,
    });
    assert_eq!(env.children(None), vec![c, a, f]);
    assert_eq!(env.children(Some(f)), vec![b]);
}

#[test]
fn move_rejections_and_cycle_check() {
    let env = Env::new();
    let f = env.create(None, "F", true);
    let sub = env.create(Some(f), "Sub", true);
    let deep = env.create(Some(sub), "Deep", true);
    let p = env.create(None, "P", false);

    for target in [f, sub, deep] {
        env.rejects(
            PlaylistOp::Move {
                id: f,
                parent_id: Some(target),
                position: 0,
            },
            |e| matches!(e, Error::MoveCycle { id, parent } if *id == f && *parent == target),
        );
    }
    env.rejects(
        PlaylistOp::Move {
            id: f,
            parent_id: Some(p),
            position: 0,
        },
        |e| matches!(e, Error::NotAFolder(id) if *id == p),
    );
    env.rejects(
        PlaylistOp::Move {
            id: 999,
            parent_id: None,
            position: 0,
        },
        |e| matches!(e, Error::PlaylistNotFound(999)),
    );
    env.rejects(
        PlaylistOp::Move {
            id: p,
            parent_id: None,
            position: 3,
        },
        |e| matches!(e, Error::IndexOutOfRange { index: 3, len: 2 }),
    );
    // Moving a sibling of `f` into `deep` is fine.
    env.mutate(PlaylistOp::Move {
        id: p,
        parent_id: Some(deep),
        position: 0,
    });
    assert_eq!(env.children(Some(deep)), vec![p]);
}

#[test]
fn add_append_and_insert() {
    let env = Env::new();
    let t = env.tracks(5);
    let p = env.create(None, "P", false);
    assert_eq!(env.add(p, &t[..2], DuplicateMode::Ask), added(2, 0));
    env.mutate(PlaylistOp::AddTracks {
        playlist_id: p,
        track_ids: vec![t[2], t[3]],
        before: Some(1),
        duplicates: DuplicateMode::Ask,
    });
    assert_eq!(env.entries(p), vec![t[0], t[2], t[3], t[1]]);
    env.mutate(PlaylistOp::AddTracks {
        playlist_id: p,
        track_ids: vec![t[4]],
        before: Some(0),
        duplicates: DuplicateMode::Ask,
    });
    assert_eq!(env.entries(p), vec![t[4], t[0], t[2], t[3], t[1]]);
    // An empty request writes nothing.
    assert_eq!(env.add(p, &[], DuplicateMode::Ask), added(0, 0));
}

#[test]
fn add_rejections() {
    let env = Env::new();
    let t = env.tracks(1);
    let f = env.create(None, "F", true);
    let p = env.create(None, "P", false);
    env.add(p, &t, DuplicateMode::Ask);
    env.rejects(
        PlaylistOp::AddTracks {
            playlist_id: f,
            track_ids: t.clone(),
            before: None,
            duplicates: DuplicateMode::Ask,
        },
        |e| matches!(e, Error::NotAPlaylist(id) if *id == f),
    );
    env.rejects(
        PlaylistOp::AddTracks {
            playlist_id: 999,
            track_ids: t.clone(),
            before: None,
            duplicates: DuplicateMode::Ask,
        },
        |e| matches!(e, Error::PlaylistNotFound(999)),
    );
    env.rejects(
        PlaylistOp::AddTracks {
            playlist_id: p,
            track_ids: vec![4242],
            before: None,
            duplicates: DuplicateMode::Allow,
        },
        |e| matches!(e, Error::TrackNotFound(4242)),
    );
    env.rejects(
        PlaylistOp::AddTracks {
            playlist_id: p,
            track_ids: t.clone(),
            before: Some(2),
            duplicates: DuplicateMode::Allow,
        },
        |e| matches!(e, Error::IndexOutOfRange { index: 2, len: 1 }),
    );
}

#[test]
fn duplicates_ask_writes_nothing_and_reports() {
    let env = Env::new();
    let t = env.tracks(4);
    let p = env.create(None, "P", false);
    env.add(p, &t[..2], DuplicateMode::Ask);
    env.age();
    let before = env.snapshot();

    // t1 is in the playlist; t2 is new but named twice.
    let out = env.add(p, &[t[2], t[1], t[2], t[3]], DuplicateMode::Ask);
    assert_eq!(
        out,
        PlaylistOutcome::Duplicates {
            playlist_id: p,
            track_ids: vec![t[1], t[2]],
            duplicate_count: 2,
            requested: 4,
        }
    );
    assert_eq!(env.snapshot(), before);
}

#[test]
fn duplicates_ask_without_duplicates_adds() {
    let env = Env::new();
    let t = env.tracks(3);
    let p = env.create(None, "P", false);
    env.add(p, &t[..1], DuplicateMode::Ask);
    assert_eq!(env.add(p, &t[1..], DuplicateMode::Ask), added(2, 0));
    assert_eq!(env.entries(p), t);
}

#[test]
fn duplicates_allow_adds_copies() {
    let env = Env::new();
    let t = env.tracks(2);
    let p = env.create(None, "P", false);
    env.add(p, &t, DuplicateMode::Ask);
    assert_eq!(env.add(p, &[t[0], t[0]], DuplicateMode::Allow), added(2, 0));
    assert_eq!(env.entries(p), vec![t[0], t[1], t[0], t[0]]);
}

#[test]
fn duplicates_skip_adds_only_new() {
    let env = Env::new();
    let t = env.tracks(4);
    let p = env.create(None, "P", false);
    env.add(p, &t[..2], DuplicateMode::Ask);
    assert_eq!(
        env.add(p, &[t[1], t[2], t[2], t[3], t[0]], DuplicateMode::Skip),
        added(2, 3)
    );
    assert_eq!(env.entries(p), t);
    // All duplicates: nothing to add, nothing written.
    env.age();
    assert_eq!(env.add(p, &t, DuplicateMode::Skip), added(0, 4));
    assert_eq!(env.updated_at(p), OLD);
}

#[test]
fn removing_one_copy_keeps_the_others() {
    let env = Env::new();
    let t = env.tracks(2);
    let p = env.create(None, "P", false);
    env.add(p, &[t[0], t[1], t[0], t[0]], DuplicateMode::Allow);
    env.mutate(PlaylistOp::RemoveEntries {
        playlist_id: p,
        positions: vec![2],
    });
    assert_eq!(env.entries(p), vec![t[0], t[1], t[0]]);
    env.mutate(PlaylistOp::RemoveEntries {
        playlist_id: p,
        positions: vec![0, 2, 0],
    });
    assert_eq!(env.entries(p), vec![t[1]]);
}

#[test]
fn remove_and_reorder_rejections() {
    let env = Env::new();
    let t = env.tracks(2);
    let f = env.create(None, "F", true);
    let p = env.create(None, "P", false);
    env.add(p, &t, DuplicateMode::Ask);
    let out_of_range = |e: &Error| matches!(e, Error::IndexOutOfRange { index: 2, len: 2 });
    env.rejects(
        PlaylistOp::RemoveEntries {
            playlist_id: p,
            positions: vec![0, 2],
        },
        out_of_range,
    );
    env.rejects(
        PlaylistOp::ReorderEntries {
            playlist_id: p,
            positions: vec![2],
            before: 0,
        },
        out_of_range,
    );
    env.rejects(
        PlaylistOp::ReorderEntries {
            playlist_id: p,
            positions: vec![0],
            before: 3,
        },
        |e| matches!(e, Error::IndexOutOfRange { index: 3, len: 2 }),
    );
    env.rejects(
        PlaylistOp::RemoveEntries {
            playlist_id: f,
            positions: vec![],
        },
        |e| matches!(e, Error::NotAPlaylist(id) if *id == f),
    );
    env.rejects(
        PlaylistOp::ReorderEntries {
            playlist_id: 999,
            positions: vec![],
            before: 0,
        },
        |e| matches!(e, Error::PlaylistNotFound(999)),
    );
}

#[test]
fn reorder_moves_a_block_before_an_entry() {
    let env = Env::new();
    let t = env.tracks(6);
    let p = env.create(None, "P", false);
    env.add(p, &t, DuplicateMode::Ask);
    // Entries 1 and 3 (in that order) go before entry 5.
    env.mutate(PlaylistOp::ReorderEntries {
        playlist_id: p,
        positions: vec![3, 1],
        before: 5,
    });
    assert_eq!(env.entries(p), vec![t[0], t[2], t[4], t[1], t[3], t[5]]);
    // To the top, and to the end.
    env.mutate(PlaylistOp::ReorderEntries {
        playlist_id: p,
        positions: vec![5],
        before: 0,
    });
    assert_eq!(env.entries(p), vec![t[5], t[0], t[2], t[4], t[1], t[3]]);
    env.mutate(PlaylistOp::ReorderEntries {
        playlist_id: p,
        positions: vec![0, 1],
        before: 6,
    });
    assert_eq!(env.entries(p), vec![t[2], t[4], t[1], t[3], t[5], t[0]]);
}

#[test]
fn positions_and_sort_orders_stay_dense_after_mixed_edits() {
    let env = Env::new();
    let t = env.tracks(8);
    let f = env.create(None, "F", true);
    let p = env.create(Some(f), "P", false);
    let q = env.create(Some(f), "Q", false);
    let r = env.create(None, "R", false);
    let mut want: Vec<i64> = Vec::new();

    env.add(p, &t[..4], DuplicateMode::Ask);
    want.extend(&t[..4]);
    env.mutate(PlaylistOp::AddTracks {
        playlist_id: p,
        track_ids: vec![t[4], t[0]],
        before: Some(2),
        duplicates: DuplicateMode::Allow,
    });
    want.splice(2..2, [t[4], t[0]]);
    env.mutate(PlaylistOp::RemoveEntries {
        playlist_id: p,
        positions: vec![1, 4],
    });
    want.remove(4);
    want.remove(1);
    env.mutate(PlaylistOp::ReorderEntries {
        playlist_id: p,
        positions: vec![0],
        before: 4,
    });
    let first = want.remove(0);
    want.push(first);
    env.mutate(PlaylistOp::AddTracks {
        playlist_id: p,
        track_ids: t[5..].to_vec(),
        before: Some(1),
        duplicates: DuplicateMode::Ask,
    });
    want.splice(1..1, t[5..].iter().copied());
    env.mutate(PlaylistOp::RemoveEntries {
        playlist_id: p,
        positions: vec![0],
    });
    want.remove(0);
    assert_eq!(env.entries(p), want);

    let s = env.create(Some(f), "S", false);
    env.mutate(PlaylistOp::Move {
        id: r,
        parent_id: Some(f),
        position: 1,
    });
    env.mutate(PlaylistOp::Delete { id: q });
    env.mutate(PlaylistOp::Move {
        id: s,
        parent_id: Some(f),
        position: 0,
    });
    assert_eq!(env.children(Some(f)), vec![s, p, r]);
    assert_eq!(env.children(None), vec![f]);
    let nodes = env.lib.playlists().unwrap().nodes;
    let positions: Vec<(i64, u32, u32)> = nodes
        .iter()
        .map(|n| (n.id, n.position, n.entries))
        .collect();
    assert_eq!(
        positions,
        vec![(f, 0, 0), (s, 0, 0), (p, 1, want.len() as u32), (r, 2, 0)]
    );
}

#[test]
fn updated_at_marks_changed_rows_only() {
    let env = Env::new();
    let t = env.tracks(2);
    let a = env.create(None, "A", false);
    let b = env.create(None, "B", false);
    let f = env.create(None, "F", true);
    let check = |changed: &[i64]| {
        for id in [a, b, f] {
            let at = env.updated_at(id);
            if changed.contains(&id) {
                assert_ne!(at, OLD, "{id} not marked");
                assert!(at.ends_with('Z') && at.contains('T'), "{at}");
            } else {
                assert_eq!(at, OLD, "{id} marked");
            }
        }
    };

    env.age();
    env.mutate(PlaylistOp::Rename {
        id: a,
        name: "A2".into(),
    });
    check(&[a]);

    env.age();
    env.mutate(PlaylistOp::Rename {
        id: a,
        name: "A2".into(),
    });
    check(&[]);

    env.age();
    env.add(b, &t, DuplicateMode::Ask);
    check(&[b]);

    env.age();
    env.mutate(PlaylistOp::ReorderEntries {
        playlist_id: b,
        positions: vec![0],
        before: 1,
    });
    check(&[]); // the same order: nothing written

    env.age();
    // B moves into F: B (parent) changes, and F (was 2, now 1) is renumbered.
    env.mutate(PlaylistOp::Move {
        id: b,
        parent_id: Some(f),
        position: 0,
    });
    check(&[b, f]);

    env.age();
    let created = env.create(None, "New", false);
    check(&[]);
    assert_ne!(env.updated_at(created), OLD);
}

#[test]
fn per_entry_view_shows_copies_and_folders_stay_distinct() {
    let env = Env::new();
    let t = env.tracks(3);
    let f = env.create(None, "F", true);
    let p = env.create(Some(f), "P", false);
    let q = env.create(Some(f), "Q", false);
    env.add(p, &[t[2], t[0], t[2]], DuplicateMode::Allow);
    env.add(q, &[t[0], t[1]], DuplicateMode::Ask);

    assert_eq!(
        env.view(p),
        vec![(t[2], Some(0)), (t[0], Some(1)), (t[2], Some(2))]
    );
    let folder = env.view(f);
    let mut ids: Vec<i64> = folder.iter().map(|(id, _)| *id).collect();
    ids.sort();
    assert_eq!(ids, t);
    assert!(folder.iter().all(|(_, pos)| pos.is_none()));

    // Other sorts keep one row per entry, copies tied by position.
    let q = TrackQuery {
        filters: vec![Filter::Playlist { id: p }],
        limit: 1000,
        ..TrackQuery::default()
    };
    let rows: Vec<Option<i64>> = env
        .lib
        .tracks(&q)
        .unwrap()
        .rows
        .iter()
        .map(|r| r.entry_position)
        .collect();
    assert_eq!(rows, vec![Some(1), Some(0), Some(2)]);
}

#[test]
fn serde_shapes() {
    let op: PlaylistOp = serde_json::from_str(
        r#"{"type": "addTracks", "playlistId": 3, "trackIds": [1, 2], "before": 0}"#,
    )
    .unwrap();
    assert_eq!(
        op,
        PlaylistOp::AddTracks {
            playlist_id: 3,
            track_ids: vec![1, 2],
            before: Some(0),
            duplicates: DuplicateMode::Ask,
        }
    );
    let op: PlaylistOp =
        serde_json::from_str(r#"{"type": "move", "id": 3, "parentId": null, "position": 1}"#)
            .unwrap();
    assert_eq!(
        op,
        PlaylistOp::Move {
            id: 3,
            parent_id: None,
            position: 1
        }
    );
    let op: PlaylistOp = serde_json::from_str(r#"{"type": "createFolder", "name": "F"}"#).unwrap();
    assert!(matches!(
        op,
        PlaylistOp::CreateFolder {
            parent_id: None,
            ..
        }
    ));
    let skip: PlaylistOp = serde_json::from_str(
        r#"{"type": "addTracks", "playlistId": 3, "trackIds": [], "duplicates": "skip"}"#,
    )
    .unwrap();
    assert!(matches!(
        skip,
        PlaylistOp::AddTracks {
            duplicates: DuplicateMode::Skip,
            ..
        }
    ));
    assert!(serde_json::from_str::<PlaylistOp>(
        r#"{"type": "delete", "id": 1, "recursive": true}"#
    )
    .is_err());

    let out = serde_json::to_value(PlaylistOutcome::Duplicates {
        playlist_id: 3,
        track_ids: vec![7],
        duplicate_count: 2,
        requested: 5,
    })
    .unwrap();
    assert_eq!(
        out,
        serde_json::json!({
            "type": "duplicates", "playlistId": 3, "trackIds": [7],
            "duplicateCount": 2, "requested": 5
        })
    );
    let out = serde_json::to_value(added(1, 0)).unwrap();
    assert_eq!(
        out,
        serde_json::json!({"type": "done", "createdId": null, "added": 1, "skipped": 0})
    );
}
