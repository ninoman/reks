//! Smart playlists (D29 task 3): create / set rule / rename / move / delete,
//! the entry ops they refuse, the cycle check (on save and at query time),
//! folders holding smart lists, and their order everywhere a playlist's
//! contents are read.

// Test helpers outside `#[test]` fns are not covered by clippy's allow-unwrap-in-tests.
#![allow(clippy::unwrap_used, clippy::panic)]

use reks_core::{
    DuplicateMode, Error, Filter, Library, Match, PlaylistId, PlaylistOp, PlaylistOutcome,
    RuleNode, Sort, SortColumn, TrackQuery,
};
use rusqlite::Connection;

const OLD: &str = "2026-01-01T00:00:00.000Z";

struct Env {
    dir: tempfile::TempDir,
    lib: Library,
}

/// Tracks 1..=4: (title, genre, date added).
const TRACKS: [(&str, Option<&str>, &str); 4] = [
    ("Bravo", Some("House"), "2026-01-02"),
    ("Alpha", Some("Techno"), "2026-01-04"),
    ("Delta", Some("house"), "2026-01-01"),
    ("Charlie", None, "2026-01-03"),
];

impl Env {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let lib =
            Library::open(&dir.path().join("reks.sqlite"), &dir.path().join("cache")).unwrap();
        let env = Env { dir, lib };
        let c = env.db();
        for (i, (title, genre, added)) in TRACKS.iter().enumerate() {
            c.execute(
                "INSERT INTO track (path, file_size, file_mtime, kind, title, genre, date_added,
                                    created_at, updated_at)
                 VALUES (?1, ?2, ?3, 'wav', ?4, ?5, ?6, ?3, ?3)",
                rusqlite::params![
                    format!("/music/{i}.wav"),
                    100 * (i as i64 + 1),
                    OLD,
                    title,
                    genre,
                    added
                ],
            )
            .unwrap();
        }
        env
    }

    fn db(&self) -> Connection {
        Connection::open(self.dir.path().join("reks.sqlite")).unwrap()
    }

    fn mutate(&self, op: PlaylistOp) -> reks_core::Result<PlaylistOutcome> {
        self.lib.playlist_mutate(op)
    }

    fn created(&self, op: PlaylistOp) -> PlaylistId {
        match self.mutate(op).unwrap() {
            PlaylistOutcome::Done {
                created_id: Some(id),
                ..
            } => id,
            other => panic!("unexpected {other:?}"),
        }
    }

    fn folder(&self, parent: Option<PlaylistId>, name: &str) -> PlaylistId {
        self.created(PlaylistOp::CreateFolder {
            parent_id: parent,
            name: name.into(),
            position: None,
        })
    }

    fn playlist(&self, parent: Option<PlaylistId>, name: &str, tracks: &[i64]) -> PlaylistId {
        let id = self.created(PlaylistOp::CreatePlaylist {
            parent_id: parent,
            name: name.into(),
            position: None,
        });
        self.mutate(PlaylistOp::AddTracks {
            playlist_id: id,
            track_ids: tracks.to_vec(),
            before: None,
            duplicates: DuplicateMode::Allow,
        })
        .unwrap();
        id
    }

    fn smart(
        &self,
        parent: Option<PlaylistId>,
        name: &str,
        rule: RuleNode,
        sort: Option<Sort>,
    ) -> reks_core::Result<PlaylistId> {
        match self.mutate(PlaylistOp::CreateSmart {
            parent_id: parent,
            name: name.into(),
            rule,
            sort,
            position: None,
        })? {
            PlaylistOutcome::Done {
                created_id: Some(id),
                ..
            } => Ok(id),
            other => panic!("unexpected {other:?}"),
        }
    }

    /// Rows of `Library::tracks` for these filters, in playlist order.
    fn listed(&self, filters: Vec<Filter>, descending: bool) -> Vec<i64> {
        let page = self
            .lib
            .tracks(&TrackQuery {
                filters,
                sort: Sort {
                    column: SortColumn::PlaylistPosition,
                    descending,
                },
                ..TrackQuery::default()
            })
            .unwrap();
        assert!(!page.divider_view);
        assert!(page.dividers.is_empty());
        assert!(page.rows.iter().all(|r| r.entry_position.is_none()));
        page.rows.iter().map(|r| r.id).collect()
    }

    fn in_list(&self, id: PlaylistId) -> Vec<i64> {
        self.listed(vec![Filter::Playlist { id }], false)
    }

    fn node(&self, id: PlaylistId) -> reks_core::PlaylistNode {
        self.lib
            .playlists()
            .unwrap()
            .nodes
            .into_iter()
            .find(|n| n.id == id)
            .unwrap()
    }
}

fn leaf(filter: Filter) -> RuleNode {
    RuleNode::Filter { not: false, filter }
}

fn not(filter: Filter) -> RuleNode {
    RuleNode::Filter { not: true, filter }
}

fn house() -> RuleNode {
    leaf(Filter::Genre {
        genre: "house".into(),
    })
}

fn by(column: SortColumn, descending: bool) -> Option<Sort> {
    Some(Sort { column, descending })
}

#[test]
fn create_set_rule_rename_move_delete() {
    let env = Env::new();
    let id = env.smart(None, "House", house(), None).unwrap();
    let n = env.node(id);
    assert!(n.smart);
    assert!(!n.is_folder);
    assert_eq!(n.entries, 2, "the tracks the rule matches now");
    // Default order: date added, newest first.
    assert_eq!(env.in_list(id), [1, 3]);
    assert_eq!(
        env.listed(vec![Filter::Playlist { id }], true),
        [3, 1],
        "descending reverses the list's own order"
    );
    // ANDed with other filters, and it never has entries of its own.
    assert_eq!(
        env.listed(
            vec![Filter::Playlist { id }, Filter::Missing { missing: false }],
            false
        ),
        [1, 3]
    );
    let count: i64 = env
        .db()
        .query_row(
            "SELECT count(*) FROM playlist_track WHERE playlist_id = ?1",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);

    // A new rule; `sort` None keeps the sort, Some replaces it.
    env.mutate(PlaylistOp::SetRule {
        id,
        rule: not(Filter::Genre {
            genre: "techno".into(),
        }),
        sort: None,
    })
    .unwrap();
    assert_eq!(env.in_list(id), [4, 1, 3]);
    env.mutate(PlaylistOp::SetRule {
        id,
        rule: not(Filter::Genre {
            genre: "techno".into(),
        }),
        sort: by(SortColumn::Title, false),
    })
    .unwrap();
    assert_eq!(env.in_list(id), [1, 4, 3]);
    assert_eq!(env.node(id).entries, 3);

    env.mutate(PlaylistOp::Rename {
        id,
        name: "Not techno".into(),
    })
    .unwrap();
    let folder = env.folder(None, "Gigs");
    env.mutate(PlaylistOp::Move {
        id,
        parent_id: Some(folder),
        position: 0,
    })
    .unwrap();
    let n = env.node(id);
    assert_eq!(
        (n.name.as_str(), n.parent_id, n.smart),
        ("Not techno", Some(folder), true)
    );
    env.mutate(PlaylistOp::Delete { id }).unwrap();
    assert_eq!(env.lib.playlists().unwrap().nodes.len(), 1);
}

#[test]
fn the_saved_rule_reads_back() {
    let env = Env::new();
    let id = env.smart(None, "House", house(), None).unwrap();
    assert_eq!(env.lib.playlist_rule(id).unwrap(), house());
    let fixed = env.playlist(None, "Fixed", &[]);
    assert!(matches!(
        env.lib.playlist_rule(fixed),
        Err(Error::NotASmartPlaylist(p)) if p == fixed
    ));
}

#[test]
fn bad_rules_and_sorts_are_rejected() {
    let env = Env::new();
    let err = env
        .smart(None, "X", house(), by(SortColumn::PlaylistPosition, false))
        .unwrap_err();
    assert!(matches!(err, Error::InvalidField { field: "sort", .. }));
    let mut deep = house();
    for _ in 0..8 {
        deep = RuleNode::Group {
            r#match: Match::All,
            not: false,
            rules: vec![deep],
        };
    }
    let err = env.smart(None, "X", deep, None).unwrap_err();
    assert!(matches!(err, Error::InvalidField { field: "rule", .. }));
    assert!(
        env.lib.playlists().unwrap().nodes.is_empty(),
        "nothing written"
    );

    let plain = env.playlist(None, "Plain", &[1]);
    let err = env
        .mutate(PlaylistOp::SetRule {
            id: plain,
            rule: house(),
            sort: None,
        })
        .unwrap_err();
    assert!(matches!(err, Error::NotASmartPlaylist(p) if p == plain));
}

#[test]
fn entry_ops_are_refused() {
    let env = Env::new();
    let id = env.smart(None, "House", house(), None).unwrap();
    let ops = [
        PlaylistOp::AddTracks {
            playlist_id: id,
            track_ids: vec![2],
            before: None,
            duplicates: DuplicateMode::Allow,
        },
        PlaylistOp::RemoveEntries {
            playlist_id: id,
            positions: vec![0],
        },
        PlaylistOp::ReorderEntries {
            playlist_id: id,
            positions: vec![0],
            before: 1,
        },
        PlaylistOp::InsertDivider {
            playlist_id: id,
            position: None,
            label: "Peak".into(),
        },
        PlaylistOp::RenameDivider {
            playlist_id: id,
            position: 0,
            label: "Peak".into(),
        },
    ];
    for op in ops {
        let err = env.mutate(op.clone()).unwrap_err();
        assert!(
            matches!(err, Error::SmartPlaylist(p) if p == id),
            "{op:?}: {err}"
        );
    }
    assert_eq!(env.in_list(id), [1, 3]);
}

#[test]
fn cycles_are_rejected_on_save() {
    let env = Env::new();
    let a = env.smart(None, "A", house(), None).unwrap();
    let b = env
        .smart(None, "B", leaf(Filter::Playlist { id: a }), None)
        .unwrap();
    assert_eq!(env.in_list(b), [1, 3], "B is A's tracks in B's order");

    // A → B → A.
    let err = env
        .mutate(PlaylistOp::SetRule {
            id: a,
            rule: not(Filter::Playlist { id: b }),
            sort: None,
        })
        .unwrap_err();
    assert!(
        matches!(err, Error::SmartPlaylistCycle(p) if p == a),
        "{err}"
    );
    // Itself.
    let err = env
        .mutate(PlaylistOp::SetRule {
            id: a,
            rule: leaf(Filter::Playlist { id: a }),
            sort: None,
        })
        .unwrap_err();
    assert!(matches!(err, Error::SmartPlaylistCycle(_)));
    assert_eq!(env.in_list(a), [1, 3], "the old rule stays");

    // A folder that holds the list itself.
    let gigs = env.folder(None, "Gigs");
    let err = env
        .smart(Some(gigs), "C", not(Filter::Playlist { id: gigs }), None)
        .unwrap_err();
    assert!(matches!(err, Error::SmartPlaylistCycle(_)));
    // Moving a list into a folder its rule names.
    let d = env
        .smart(None, "D", leaf(Filter::Playlist { id: gigs }), None)
        .unwrap();
    let err = env
        .mutate(PlaylistOp::Move {
            id: d,
            parent_id: Some(gigs),
            position: 0,
        })
        .unwrap_err();
    assert!(matches!(err, Error::SmartPlaylistCycle(p) if p == d));
    assert_eq!(env.node(d).parent_id, None, "the move was rolled back");
}

/// A cycle that got into the database anyway matches nothing where it
/// closes, and the query still answers.
#[test]
fn a_stored_cycle_matches_nothing() {
    let env = Env::new();
    let a = env.smart(None, "A", house(), None).unwrap();
    let rule = serde_json::to_string(&RuleNode::Group {
        r#match: Match::Any,
        not: false,
        rules: vec![
            leaf(Filter::Playlist { id: a }),
            leaf(Filter::Genre {
                genre: "techno".into(),
            }),
        ],
    })
    .unwrap();
    env.db()
        .execute(
            "UPDATE playlist SET smart_rule = ?2 WHERE id = ?1",
            rusqlite::params![a, rule],
        )
        .unwrap();
    assert_eq!(env.in_list(a), [2]);
    assert_eq!(env.node(a).entries, 1);
}

#[test]
fn folder_with_a_smart_child() {
    let env = Env::new();
    let gigs = env.folder(None, "Gigs");
    env.playlist(Some(gigs), "Static", &[4]);
    let sub = env.folder(Some(gigs), "Sub");
    env.smart(Some(sub), "House", house(), None).unwrap();
    let mut ids = env.listed(vec![Filter::Playlist { id: gigs }], false);
    ids.sort_unstable();
    assert_eq!(ids, [1, 3, 4], "static entries or any smart list's rule");
    // The same inside a rule, negated.
    let page = env
        .lib
        .tracks(&TrackQuery {
            rule: Some(not(Filter::Playlist { id: gigs })),
            ..TrackQuery::default()
        })
        .unwrap();
    assert_eq!(page.rows.iter().map(|r| r.id).collect::<Vec<_>>(), [2]);
    // A smart list naming the folder sees the smart child too.
    let outside = env
        .smart(None, "Outside", not(Filter::Playlist { id: gigs }), None)
        .unwrap();
    assert_eq!(env.in_list(outside), [2]);
}

/// Every reader of a playlist's contents sees a smart list's current tracks
/// in its own sort.
#[test]
fn readers_see_current_tracks_in_order() {
    let env = Env::new();
    let id = env
        .smart(None, "House", house(), by(SortColumn::Title, true))
        .unwrap();
    let plain = env.playlist(None, "Plain", &[2, 2, 1]);
    let lists = env.lib.playlist_track_ids(&[id, plain]).unwrap();
    assert_eq!(lists[&id], [3, 1], "title descending: Delta, Bravo");
    assert_eq!(lists[&plain], [2, 2, 1]);
    let sizes = env.lib.playlist_entry_sizes(&[id]).unwrap();
    assert_eq!(sizes[&id], [(3, 300), (1, 100)]);

    // A track that starts matching shows up everywhere at once.
    env.db()
        .execute("UPDATE track SET genre = 'House' WHERE id = 4", [])
        .unwrap();
    assert_eq!(env.lib.playlist_track_ids(&[id]).unwrap()[&id], [3, 4, 1]);
    assert_eq!(env.in_list(id), [3, 4, 1]);
    assert_eq!(env.node(id).entries, 3);
}

/// The JSON `src/lib/ipc.ts` sends for the new ops, and `PlaylistNode.smart`.
#[test]
fn serde_shapes() {
    let create: PlaylistOp = serde_json::from_str(
        r#"{"type": "createSmart", "parentId": 3, "name": "Peak", "position": 0,
            "rule": {"type": "filter", "not": false, "filter": {"type": "tagAny", "ids": [7]}},
            "sort": {"column": "bpm", "descending": false}}"#,
    )
    .unwrap();
    assert_eq!(
        create,
        PlaylistOp::CreateSmart {
            parent_id: Some(3),
            name: "Peak".into(),
            rule: leaf(Filter::TagAny { ids: vec![7] }),
            sort: by(SortColumn::Bpm, false),
            position: Some(0),
        }
    );
    let minimal: PlaylistOp = serde_json::from_str(
        r#"{"type": "createSmart", "name": "Peak",
            "rule": {"type": "group", "match": "any", "not": false, "rules": []}}"#,
    )
    .unwrap();
    assert!(matches!(
        minimal,
        PlaylistOp::CreateSmart {
            parent_id: None,
            sort: None,
            position: None,
            ..
        }
    ));
    let set: PlaylistOp = serde_json::from_str(
        r#"{"type": "setRule", "id": 4,
            "rule": {"type": "filter", "not": true, "filter": {"type": "playlist", "id": 2}}}"#,
    )
    .unwrap();
    assert_eq!(
        set,
        PlaylistOp::SetRule {
            id: 4,
            rule: not(Filter::Playlist { id: 2 }),
            sort: None,
        }
    );

    let env = Env::new();
    env.smart(None, "House", house(), None).unwrap();
    let tree = serde_json::to_value(env.lib.playlists().unwrap()).unwrap();
    assert_eq!(tree["nodes"][0]["smart"], true);
    assert_eq!(tree["nodes"][0]["entries"], 2);
}
