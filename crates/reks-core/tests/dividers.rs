//! Playlist dividers (Phase 2.5 task 2, D22): insert and rename ops, the
//! label rules, remove/reorder/duplicates with dividers present, the playlist
//! query (dividers listed only in plain playlist order, never in search or
//! counts), restart, and rekordbox XML export (omitted) and re-import (kept).

#![allow(clippy::unwrap_used, clippy::panic)]

use std::path::PathBuf;

use reks_core::rbxml::{self, export, ImportOptions, PrefixRemap, Selection};
use reks_core::{
    DividerRow, DuplicateMode, Error, Filter, Library, PlaylistOp, PlaylistOutcome, Sort,
    SortColumn, TrackQuery, MAX_DIVIDER_LABEL_CHARS,
};
use rusqlite::Connection;

const OLD: &str = "2026-01-01T00:00:00.000Z";

/// A `playlist_track` row plus its playlist's `updated_at`.
type SnapshotRow = (i64, Option<i64>, i64, String, Option<String>, String);

struct Env {
    dir: tempfile::TempDir,
    lib: Library,
}

/// One entry as the tests see it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum E {
    T(i64),
    D(&'static str),
}

impl Env {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let lib = Self::open(&dir);
        Env { dir, lib }
    }

    fn open(dir: &tempfile::TempDir) -> Library {
        Library::open(&dir.path().join("reks.sqlite"), &dir.path().join("cache")).unwrap()
    }

    fn db(&self) -> Connection {
        Connection::open(self.dir.path().join("reks.sqlite")).unwrap()
    }

    /// `n` tracks titled "Track <i>", ids in order.
    fn tracks(&self, n: usize) -> Vec<i64> {
        let c = self.db();
        (0..n)
            .map(|i| {
                c.execute(
                    "INSERT INTO track (path, file_size, file_mtime, kind, title, date_added,
                                        created_at, updated_at)
                     VALUES (?1, 1, ?2, 'wav', ?3, '2026-01-01', ?2, ?2)",
                    rusqlite::params![format!("/music/{i}.wav"), OLD, format!("Track {i}")],
                )
                .unwrap();
                c.last_insert_rowid()
            })
            .collect()
    }

    fn mutate(&self, op: PlaylistOp) -> PlaylistOutcome {
        self.lib.playlist_mutate(op).unwrap()
    }

    fn playlist(&self, name: &str, folder: bool) -> i64 {
        let op = if folder {
            PlaylistOp::CreateFolder {
                parent_id: None,
                name: name.into(),
                position: None,
            }
        } else {
            PlaylistOp::CreatePlaylist {
                parent_id: None,
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

    fn add(&self, playlist: i64, tracks: &[i64]) {
        self.mutate(PlaylistOp::AddTracks {
            playlist_id: playlist,
            track_ids: tracks.to_vec(),
            before: None,
            duplicates: DuplicateMode::Allow,
        });
    }

    fn divider(&self, playlist: i64, position: Option<u32>, label: &str) {
        assert_eq!(
            self.mutate(PlaylistOp::InsertDivider {
                playlist_id: playlist,
                position,
                label: label.into(),
            }),
            done()
        );
    }

    /// Raw rows in position order; asserts positions are `0..n`.
    fn rows(&self, playlist: i64) -> Vec<(Option<i64>, String, Option<String>)> {
        let c = self.db();
        let mut stmt = c
            .prepare(
                "SELECT position, track_id, item_kind, label FROM playlist_track
                 WHERE playlist_id = ?1 ORDER BY position",
            )
            .unwrap();
        let rows: Vec<(i64, Option<i64>, String, Option<String>)> = stmt
            .query_map([playlist], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        for (i, row) in rows.iter().enumerate() {
            assert_eq!(row.0, i as i64, "positions of {playlist} not dense");
        }
        rows.into_iter().map(|(_, t, k, l)| (t, k, l)).collect()
    }

    /// Entries as `E`s; labels must be among `labels`.
    fn entries(&self, playlist: i64, labels: &[&'static str]) -> Vec<E> {
        self.rows(playlist)
            .into_iter()
            .map(|(t, kind, label)| match (kind.as_str(), t, label) {
                ("track", Some(t), None) => E::T(t),
                ("divider", None, Some(l)) => E::D(
                    labels
                        .iter()
                        .find(|x| **x == l)
                        .copied()
                        .unwrap_or_else(|| panic!("unexpected label {l:?}")),
                ),
                other => panic!("bad row {other:?}"),
            })
            .collect()
    }

    fn snapshot(&self) -> Vec<SnapshotRow> {
        self.db()
            .prepare(
                "SELECT pt.playlist_id, pt.track_id, pt.position, pt.item_kind, pt.label,
                        p.updated_at
                 FROM playlist_track pt JOIN playlist p ON p.id = pt.playlist_id
                 ORDER BY pt.playlist_id, pt.position",
            )
            .unwrap()
            .query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            })
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    fn rejects(&self, op: PlaylistOp, check: impl Fn(&Error) -> bool) {
        let before = self.snapshot();
        let err = self.lib.playlist_mutate(op.clone()).unwrap_err();
        assert!(check(&err), "{op:?} gave {err:?}");
        assert_eq!(self.snapshot(), before, "{op:?} changed the DB");
    }

    fn entry_count(&self, playlist: i64) -> u32 {
        let tree = self.lib.playlists().unwrap();
        tree.nodes
            .iter()
            .find(|n| n.id == playlist)
            .unwrap()
            .entries
    }
}

fn done() -> PlaylistOutcome {
    PlaylistOutcome::Done {
        created_id: None,
        added: 0,
        skipped: 0,
    }
}

fn in_order(playlist: i64) -> TrackQuery {
    TrackQuery {
        filters: vec![Filter::Playlist { id: playlist }],
        sort: Sort {
            column: SortColumn::PlaylistPosition,
            descending: false,
        },
        limit: 1000,
        ..TrackQuery::default()
    }
}

fn div(position: i64, label: &str) -> DividerRow {
    DividerRow {
        position,
        label: label.into(),
    }
}

#[test]
fn insert_at_position_and_append() {
    let env = Env::new();
    let t = env.tracks(3);
    let p = env.playlist("Set", false);
    env.add(p, &t);

    env.divider(p, Some(0), "  ---- WARM UP ----  ");
    env.divider(p, Some(2), "PEAK");
    env.divider(p, None, "END");
    env.divider(p, Some(6), "AFTER END");
    let l = ["---- WARM UP ----", "PEAK", "END", "AFTER END"];
    assert_eq!(
        env.entries(p, &l),
        vec![
            E::D(l[0]),
            E::T(t[0]),
            E::D(l[1]),
            E::T(t[1]),
            E::T(t[2]),
            E::D(l[2]),
            E::D(l[3]),
        ]
    );
    // Two dividers with the same label are fine.
    env.divider(p, Some(0), "PEAK");
    assert_eq!(env.entries(p, &l)[..2], [E::D("PEAK"), E::D(l[0])]);
}

#[test]
fn rename_divider() {
    let env = Env::new();
    let t = env.tracks(2);
    let p = env.playlist("Set", false);
    env.add(p, &t);
    env.divider(p, Some(1), "old");
    env.db()
        .execute("UPDATE playlist SET updated_at = ?1", [OLD])
        .unwrap();

    assert_eq!(
        env.mutate(PlaylistOp::RenameDivider {
            playlist_id: p,
            position: 1,
            label: " new ".into(),
        }),
        done()
    );
    assert_eq!(
        env.entries(p, &["new"]),
        vec![E::T(t[0]), E::D("new"), E::T(t[1])]
    );
    let updated: String = env
        .db()
        .query_row("SELECT updated_at FROM playlist WHERE id = ?1", [p], |r| {
            r.get(0)
        })
        .unwrap();
    assert_ne!(updated, OLD);

    // A track entry is not a divider; positions are entry positions.
    env.rejects(
        PlaylistOp::RenameDivider {
            playlist_id: p,
            position: 0,
            label: "x".into(),
        },
        |e| {
            matches!(
                e,
                Error::NotADivider {
                    playlist,
                    position: 0
                } if *playlist == p
            )
        },
    );
    env.rejects(
        PlaylistOp::RenameDivider {
            playlist_id: p,
            position: 3,
            label: "x".into(),
        },
        |e| matches!(e, Error::IndexOutOfRange { index: 3, len: 3 }),
    );
}

#[test]
fn label_rules() {
    let env = Env::new();
    let p = env.playlist("Set", false);
    let f = env.playlist("Folder", true);
    env.divider(p, None, "keep");

    let max = "é".repeat(MAX_DIVIDER_LABEL_CHARS);
    let too_long = "-".repeat(MAX_DIVIDER_LABEL_CHARS + 1);
    for bad in ["", "   ", "a\nb", "a\u{7}b", too_long.as_str()] {
        env.rejects(
            PlaylistOp::InsertDivider {
                playlist_id: p,
                position: None,
                label: bad.into(),
            },
            |e| matches!(e, Error::InvalidField { field: "label", .. }),
        );
        env.rejects(
            PlaylistOp::RenameDivider {
                playlist_id: p,
                position: 0,
                label: bad.into(),
            },
            |e| matches!(e, Error::InvalidField { field: "label", .. }),
        );
    }
    // The limit counts characters, not bytes; padding is trimmed first.
    env.divider(p, None, &format!("  {max}  "));
    let rows = env.rows(p);
    assert_eq!(rows[1].2.as_deref(), Some(max.as_str()));

    env.rejects(
        PlaylistOp::InsertDivider {
            playlist_id: f,
            position: None,
            label: "x".into(),
        },
        |e| matches!(e, Error::NotAPlaylist(id) if *id == f),
    );
    env.rejects(
        PlaylistOp::InsertDivider {
            playlist_id: 999,
            position: None,
            label: "x".into(),
        },
        |e| matches!(e, Error::PlaylistNotFound(999)),
    );
    env.rejects(
        PlaylistOp::InsertDivider {
            playlist_id: p,
            position: Some(3),
            label: "x".into(),
        },
        |e| matches!(e, Error::IndexOutOfRange { index: 3, len: 2 }),
    );
}

#[test]
fn remove_reorder_and_duplicates_with_dividers() {
    let env = Env::new();
    let t = env.tracks(3);
    let p = env.playlist("Set", false);
    env.add(p, &t[..2]);
    env.divider(p, Some(1), "A");
    env.divider(p, None, "B");
    let l = ["A", "B"];
    // t0 A t1 B
    env.mutate(PlaylistOp::ReorderEntries {
        playlist_id: p,
        positions: vec![3],
        before: 0,
    });
    assert_eq!(
        env.entries(p, &l),
        vec![E::D("B"), E::T(t[0]), E::D("A"), E::T(t[1])]
    );
    env.mutate(PlaylistOp::ReorderEntries {
        playlist_id: p,
        positions: vec![1, 2],
        before: 4,
    });
    assert_eq!(
        env.entries(p, &l),
        vec![E::D("B"), E::T(t[1]), E::T(t[0]), E::D("A")]
    );

    // Dividers are never duplicates: Ask only flags the repeated track.
    let out = env.mutate(PlaylistOp::AddTracks {
        playlist_id: p,
        track_ids: vec![t[2], t[0]],
        before: Some(0),
        duplicates: DuplicateMode::Ask,
    });
    assert_eq!(
        out,
        PlaylistOutcome::Duplicates {
            playlist_id: p,
            track_ids: vec![t[0]],
            duplicate_count: 1,
            requested: 2,
        }
    );
    env.mutate(PlaylistOp::AddTracks {
        playlist_id: p,
        track_ids: vec![t[2]],
        before: Some(1),
        duplicates: DuplicateMode::Ask,
    });
    assert_eq!(
        env.entries(p, &l),
        vec![E::D("B"), E::T(t[2]), E::T(t[1]), E::T(t[0]), E::D("A")]
    );

    env.mutate(PlaylistOp::RemoveEntries {
        playlist_id: p,
        positions: vec![0, 2],
    });
    assert_eq!(env.entries(p, &l), vec![E::T(t[2]), E::T(t[0]), E::D("A")]);
}

#[test]
fn query_lists_dividers_only_in_plain_playlist_order() {
    let env = Env::new();
    let t = env.tracks(3);
    let p = env.playlist("Set", false);
    let f = env.playlist("Folder", true);
    env.mutate(PlaylistOp::Move {
        id: p,
        parent_id: Some(f),
        position: 0,
    });
    env.add(p, &t);
    env.divider(p, Some(0), "OPEN Track");
    env.divider(p, Some(2), "PEAK");

    let page = env.lib.tracks(&in_order(p)).unwrap();
    assert_eq!(page.total, 3);
    let rows: Vec<(i64, Option<i64>)> =
        page.rows.iter().map(|r| (r.id, r.entry_position)).collect();
    assert_eq!(
        rows,
        vec![(t[0], Some(1)), (t[1], Some(3)), (t[2], Some(4))]
    );
    assert_eq!(page.dividers, vec![div(0, "OPEN Track"), div(2, "PEAK")]);

    // Every page carries every divider; paging and total count tracks only.
    let page = env
        .lib
        .tracks(&TrackQuery {
            offset: 2,
            limit: 1,
            ..in_order(p)
        })
        .unwrap();
    assert_eq!((page.total, page.rows.len()), (3, 1));
    assert_eq!(page.rows[0].entry_position, Some(4));
    assert_eq!(page.dividers.len(), 2);

    // Sorted, descending, searched or filtered views hide them.
    let hidden = [
        TrackQuery {
            sort: Sort::default(),
            ..in_order(p)
        },
        TrackQuery {
            sort: Sort {
                column: SortColumn::PlaylistPosition,
                descending: true,
            },
            ..in_order(p)
        },
        TrackQuery {
            text: Some("track".into()),
            ..in_order(p)
        },
        TrackQuery {
            filters: vec![
                Filter::Playlist { id: p },
                Filter::Missing { missing: false },
            ],
            ..in_order(p)
        },
        in_order(f),
    ];
    for q in hidden {
        let page = env.lib.tracks(&q).unwrap();
        assert_eq!(page.total, 3, "{q:?}");
        assert!(page.dividers.is_empty(), "{q:?}");
    }
    // Search text with no words is no search.
    let page = env
        .lib
        .tracks(&TrackQuery {
            text: Some(" - ".into()),
            ..in_order(p)
        })
        .unwrap();
    assert_eq!(page.dividers.len(), 2);

    // Dividers are not tracks: not in the library, search or counts.
    let all = env.lib.tracks(&TrackQuery::default()).unwrap();
    assert_eq!(all.total, 3);
    assert!(all.dividers.is_empty());
    let search = |text: &str| {
        env.lib
            .tracks(&TrackQuery {
                text: Some(text.into()),
                ..TrackQuery::default()
            })
            .unwrap()
            .total
    };
    assert_eq!(search("peak"), 0);
    assert_eq!(search("open"), 0);
    assert_eq!(search("track"), 3);
    assert_eq!(env.entry_count(p), 3);
    assert_eq!(env.entry_count(f), 0);
    let facets = env.lib.facets().unwrap();
    let node = facets.playlists.iter().find(|n| n.id == p).unwrap();
    assert_eq!(node.entries, 3);
}

#[test]
fn dividers_survive_a_restart() {
    let env = Env::new();
    let t = env.tracks(1);
    let p = env.playlist("Set", false);
    env.add(p, &t);
    env.divider(p, Some(0), "PEAK");
    let Env { dir, lib } = env;
    drop(lib);
    let env = Env {
        lib: Env::open(&dir),
        dir,
    };
    assert_eq!(env.entries(p, &["PEAK"]), vec![E::D("PEAK"), E::T(t[0])]);
    assert_eq!(
        env.lib.tracks(&in_order(p)).unwrap().dividers,
        vec![div(0, "PEAK")]
    );
}

#[test]
fn serde_shapes() {
    let op: PlaylistOp = serde_json::from_str(
        r#"{"type": "insertDivider", "playlistId": 3, "position": 1, "label": "PEAK"}"#,
    )
    .unwrap();
    assert_eq!(
        op,
        PlaylistOp::InsertDivider {
            playlist_id: 3,
            position: Some(1),
            label: "PEAK".into(),
        }
    );
    let op: PlaylistOp =
        serde_json::from_str(r#"{"type": "insertDivider", "playlistId": 3, "label": "END"}"#)
            .unwrap();
    assert!(matches!(
        op,
        PlaylistOp::InsertDivider { position: None, .. }
    ));
    let op: PlaylistOp = serde_json::from_str(
        r#"{"type": "renameDivider", "playlistId": 3, "position": 0, "label": "X"}"#,
    )
    .unwrap();
    assert!(matches!(op, PlaylistOp::RenameDivider { position: 0, .. }));
    assert_eq!(
        serde_json::to_value(div(2, "PEAK")).unwrap(),
        serde_json::json!({"position": 2, "label": "PEAK"})
    );
}

// rekordbox XML: export leaves dividers out; re-import keeps them in place.

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn small_collection() -> rbxml::Collection {
    let f = std::fs::File::open(fixtures().join("rekordbox/collection-small.xml")).unwrap();
    rbxml::parse(std::io::BufReader::new(f)).unwrap()
}

fn import_fixture(lib: &Library) {
    let opts = ImportOptions {
        remap: vec![PrefixRemap {
            from: "/Users/reks/Music/reks fixtures".into(),
            to: std::fs::canonicalize(fixtures().join("audio")).unwrap(),
        }],
        ..ImportOptions::default()
    };
    rbxml::import(&small_collection(), lib, opts, |_| {}).unwrap();
}

fn node(lib: &Library, name: &str) -> i64 {
    let tree = lib.playlists().unwrap();
    tree.nodes.iter().find(|n| n.name == name).unwrap().id
}

#[test]
fn xml_export_omits_dividers() {
    let env = Env::new();
    import_fixture(&env.lib);
    let all = node(&env.lib, "All fixtures");
    let before = export(
        &env.lib,
        &Selection {
            playlists: vec![all],
        },
    )
    .unwrap();
    env.divider(all, Some(0), "OPEN");
    env.divider(all, Some(2), "PEAK");
    env.divider(all, None, "END");
    let after = export(
        &env.lib,
        &Selection {
            playlists: vec![all],
        },
    )
    .unwrap();
    assert_eq!(after.collection, before.collection);
    assert_eq!(after.collection.tracks.len(), 3);
}

#[test]
fn xml_reimport_keeps_dividers_in_place() {
    let env = Env::new();
    import_fixture(&env.lib);
    let all = node(&env.lib, "All fixtures");
    env.divider(all, Some(0), "OPEN");
    env.divider(all, Some(2), "PEAK");
    env.divider(all, None, "END");
    let l = ["OPEN", "PEAK", "END"];
    let before = env.entries(all, &l);

    // Unchanged tracks: nothing is rewritten.
    let snap = env.snapshot();
    import_fixture(&env.lib);
    assert_eq!(env.snapshot(), snap);

    // A reorder in reks is undone by the XML order; dividers keep their
    // positions.
    let tracks: Vec<i64> = before
        .iter()
        .filter_map(|e| match e {
            E::T(t) => Some(*t),
            E::D(_) => None,
        })
        .collect();
    env.mutate(PlaylistOp::ReorderEntries {
        playlist_id: all,
        positions: vec![4],
        before: 3,
    });
    assert_ne!(env.entries(all, &l), before);
    import_fixture(&env.lib);
    assert_eq!(env.entries(all, &l), before);
    assert_eq!(
        before,
        vec![
            E::D("OPEN"),
            E::T(tracks[0]),
            E::D("PEAK"),
            E::T(tracks[1]),
            E::T(tracks[2]),
            E::D("END"),
        ]
    );
}
