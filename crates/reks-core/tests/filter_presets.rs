//! `Library::filter_presets` and `Library::filter_preset_mutate`: every op,
//! each rejected case, dense positions, the rule JSON round trip and
//! references to deleted tags and playlists.

// Test helpers outside `#[test]` fns are not covered by clippy's allow-unwrap-in-tests.
#![allow(clippy::unwrap_used, clippy::panic)]

use reks_core::{
    Error, Filter, FilterPreset, FilterPresetOp, Library, Match, PlaylistOp, PlaylistOutcome,
    RuleNode, TagOp, TrackQuery, MAX_RULE_DEPTH,
};
use rusqlite::Connection;

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

    fn db(&self) -> Connection {
        Connection::open(self.dir.path().join("reks.sqlite")).unwrap()
    }

    fn mutate(&self, op: FilterPresetOp) -> reks_core::Result<Option<i64>> {
        self.lib.filter_preset_mutate(op).map(|o| o.created_id)
    }

    fn create(&self, name: &str, rule: RuleNode) -> i64 {
        self.mutate(FilterPresetOp::Create {
            name: name.into(),
            rule,
            position: None,
        })
        .unwrap()
        .unwrap()
    }

    fn list(&self) -> Vec<FilterPreset> {
        self.lib.filter_presets().unwrap()
    }

    fn names(&self) -> Vec<String> {
        self.list().into_iter().map(|p| p.name).collect()
    }

    /// (name, position) of every row, by position: positions stay dense.
    fn positions(&self) -> Vec<(String, i64)> {
        self.db()
            .prepare("SELECT name, position FROM filter_preset ORDER BY position")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    }

    /// Backdates every row, so a later op's touches show.
    fn age(&self) {
        self.db()
            .execute(
                "UPDATE filter_preset SET created_at = ?1, updated_at = ?1",
                [OLD],
            )
            .unwrap();
    }

    fn updated(&self) -> Vec<(String, bool)> {
        self.list()
            .into_iter()
            .map(|p| (p.name, p.updated_at != OLD))
            .collect()
    }

    fn track(&self) -> i64 {
        let c = self.db();
        c.execute(
            "INSERT INTO track (path, file_size, file_mtime, kind, title, date_added,
                                created_at, updated_at)
             VALUES ('/music/a.wav', 1, ?1, 'wav', 'a', '2026-01-01', ?1, ?1)",
            [OLD],
        )
        .unwrap();
        c.last_insert_rowid()
    }
}

fn leaf(filter: Filter) -> RuleNode {
    RuleNode::Filter { not: false, filter }
}

fn all(rules: Vec<RuleNode>) -> RuleNode {
    RuleNode::Group {
        r#match: Match::All,
        not: false,
        rules,
    }
}

fn techno() -> RuleNode {
    all(vec![leaf(Filter::Genre {
        genre: "Techno".into(),
    })])
}

#[test]
fn create_lists_in_position_order() {
    let env = Env::new();
    assert!(env.list().is_empty());
    let a = env.create("Peak", techno());
    env.create("Warm-up", techno());
    let c = env
        .mutate(FilterPresetOp::Create {
            name: "  Closing  ".into(),
            rule: techno(),
            position: Some(1),
        })
        .unwrap()
        .unwrap();
    assert_eq!(env.names(), ["Peak", "Closing", "Warm-up"]);
    assert_eq!(
        env.positions(),
        [
            ("Peak".into(), 0),
            ("Closing".into(), 1),
            ("Warm-up".into(), 2)
        ]
    );
    let first = &env.list()[0];
    assert_eq!(first.id, a);
    assert_eq!(first.rule, Some(techno()));
    assert_eq!(first.created_at, first.updated_at);
    assert!(first.created_at.ends_with('Z'));
    assert_ne!(a, c);

    env.mutate(FilterPresetOp::Create {
        name: "First".into(),
        rule: techno(),
        position: Some(0),
    })
    .unwrap();
    assert_eq!(env.names(), ["First", "Peak", "Closing", "Warm-up"]);
    assert!(matches!(
        env.mutate(FilterPresetOp::Create {
            name: "Late".into(),
            rule: techno(),
            position: Some(5),
        }),
        Err(Error::IndexOutOfRange { index: 5, len: 4 })
    ));
}

#[test]
fn names_are_checked_and_unique_in_any_case() {
    let env = Env::new();
    let peak = env.create("Peak techno", techno());
    let warm = env.create("Warm-up", techno());
    for name in ["Peak techno", "PEAK TECHNO", " peak techno "] {
        assert!(
            matches!(
                env.mutate(FilterPresetOp::Create {
                    name: name.into(),
                    rule: techno(),
                    position: None,
                }),
                Err(Error::FilterPresetNameTaken(n)) if n.eq_ignore_ascii_case("peak techno")
            ),
            "{name}"
        );
    }
    assert!(matches!(
        env.mutate(FilterPresetOp::Create {
            name: "   ".into(),
            rule: techno(),
            position: None,
        }),
        Err(Error::InvalidField { field: "name", .. })
    ));
    assert!(matches!(
        env.mutate(FilterPresetOp::Rename {
            id: warm,
            name: "peak TECHNO".into(),
        }),
        Err(Error::FilterPresetNameTaken(_))
    ));
    // Its own name in another case is a rename, not a clash.
    env.mutate(FilterPresetOp::Rename {
        id: peak,
        name: "PEAK techno".into(),
    })
    .unwrap();
    assert_eq!(env.names(), ["PEAK techno", "Warm-up"]);
}

#[test]
fn rename_and_set_rule_touch_only_their_row() {
    let env = Env::new();
    let peak = env.create("Peak", techno());
    env.create("Warm-up", techno());
    env.age();

    env.mutate(FilterPresetOp::Rename {
        id: peak,
        name: "Peak".into(),
    })
    .unwrap();
    assert_eq!(
        env.updated(),
        [("Peak".into(), false), ("Warm-up".into(), false)],
        "an unchanged name writes nothing"
    );
    env.mutate(FilterPresetOp::Rename {
        id: peak,
        name: "Peak time".into(),
    })
    .unwrap();
    assert_eq!(
        env.updated(),
        [("Peak time".into(), true), ("Warm-up".into(), false)]
    );

    env.age();
    let deep = all(vec![leaf(Filter::RatingMin { rating: 4 })]);
    env.mutate(FilterPresetOp::SetRule {
        id: peak,
        rule: deep.clone(),
    })
    .unwrap();
    assert_eq!(env.list()[0].rule, Some(deep));
    assert_eq!(env.list()[1].rule, Some(techno()));
    assert_eq!(
        env.updated(),
        [("Peak time".into(), true), ("Warm-up".into(), false)]
    );
}

#[test]
fn bad_rules_and_ids_are_rejected() {
    let env = Env::new();
    let peak = env.create("Peak", techno());
    let mut deep = leaf(Filter::NeverPlayed);
    for _ in 0..MAX_RULE_DEPTH {
        deep = all(vec![deep]);
    }
    assert!(matches!(
        env.mutate(FilterPresetOp::Create {
            name: "Deep".into(),
            rule: deep.clone(),
            position: None,
        }),
        Err(Error::InvalidField { field: "rule", .. })
    ));
    assert!(matches!(
        env.mutate(FilterPresetOp::SetRule {
            id: peak,
            rule: deep
        }),
        Err(Error::InvalidField { field: "rule", .. })
    ));
    for op in [
        FilterPresetOp::Rename {
            id: 99,
            name: "X".into(),
        },
        FilterPresetOp::SetRule {
            id: 99,
            rule: techno(),
        },
        FilterPresetOp::Duplicate { id: 99 },
        FilterPresetOp::Delete { id: 99 },
        FilterPresetOp::Move {
            id: 99,
            position: 0,
        },
    ] {
        assert!(
            matches!(env.mutate(op.clone()), Err(Error::FilterPresetNotFound(99))),
            "{op:?}"
        );
    }
    assert_eq!(env.names(), ["Peak"]);
    assert_eq!(env.list()[0].rule, Some(techno()));
}

#[test]
fn duplicate_goes_after_the_original_with_a_free_name() {
    let env = Env::new();
    let peak = env.create("Peak", techno());
    env.create("Warm-up", techno());
    let copy = env
        .mutate(FilterPresetOp::Duplicate { id: peak })
        .unwrap()
        .unwrap();
    assert_eq!(env.names(), ["Peak", "Peak copy", "Warm-up"]);
    let listed = env.list();
    assert_eq!(listed[1].id, copy);
    assert_eq!(listed[1].rule, listed[0].rule);

    env.mutate(FilterPresetOp::Rename {
        id: copy,
        name: "PEAK COPY".into(),
    })
    .unwrap();
    env.mutate(FilterPresetOp::Duplicate { id: peak }).unwrap();
    env.mutate(FilterPresetOp::Duplicate { id: peak }).unwrap();
    assert_eq!(
        env.names(),
        ["Peak", "Peak copy 3", "Peak copy 2", "PEAK COPY", "Warm-up"]
    );
    assert_eq!(env.positions().last().unwrap().1, 4);
}

#[test]
fn delete_and_move_keep_positions_dense() {
    let env = Env::new();
    let ids: Vec<i64> = ["A", "B", "C", "D"]
        .iter()
        .map(|n| env.create(n, techno()))
        .collect();

    // Indices are in the list as it is now, before the move.
    env.mutate(FilterPresetOp::Move {
        id: ids[0],
        position: 3,
    })
    .unwrap();
    assert_eq!(env.names(), ["B", "C", "A", "D"]);
    env.mutate(FilterPresetOp::Move {
        id: ids[3],
        position: 0,
    })
    .unwrap();
    assert_eq!(env.names(), ["D", "B", "C", "A"]);
    env.mutate(FilterPresetOp::Move {
        id: ids[2],
        position: 4,
    })
    .unwrap();
    assert_eq!(env.names(), ["D", "B", "A", "C"]);
    env.mutate(FilterPresetOp::Move {
        id: ids[1],
        position: 1,
    })
    .unwrap();
    assert_eq!(env.names(), ["D", "B", "A", "C"], "in place");
    assert!(matches!(
        env.mutate(FilterPresetOp::Move {
            id: ids[1],
            position: 5,
        }),
        Err(Error::IndexOutOfRange { index: 5, len: 4 })
    ));

    env.mutate(FilterPresetOp::Delete { id: ids[1] }).unwrap();
    assert_eq!(
        env.positions(),
        [("D".into(), 0), ("A".into(), 1), ("C".into(), 2)]
    );
    // The name is free again.
    env.create("b", techno());
    assert_eq!(env.names(), ["D", "A", "C", "b"]);
}

#[test]
fn the_rule_json_round_trips() {
    let env = Env::new();
    let rule: RuleNode = serde_json::from_str(
        r#"{"type": "group", "match": "all", "not": false, "rules": [
            {"type": "filter", "not": false, "filter": {"type": "bpmRange", "min": 12400, "max": 13000}},
            {"type": "filter", "not": false, "filter": {"type": "keyIn", "keys": ["8A", "9A"]}},
            {"type": "filter", "not": false, "filter": {"type": "genre", "genre": "Techno"}},
            {"type": "filter", "not": true, "filter": {"type": "tagAny", "ids": [4, 5]}},
            {"type": "group", "match": "any", "not": true, "rules": [
                {"type": "filter", "not": false, "filter": {"type": "ratingMin", "rating": 4}},
                {"type": "filter", "not": false, "filter": {"type": "playlist", "id": 3}},
                {"type": "filter", "not": false, "filter": {"type": "neverPlayed"}},
                {"type": "filter", "not": false, "filter": {"type": "playedSince", "date": "2026-01-01"}},
                {"type": "filter", "not": false, "filter": {"type": "playCountRange", "min": 1, "max": 9}},
                {"type": "filter", "not": false, "filter": {"type": "color", "color": "Red"}},
                {"type": "filter", "not": false, "filter": {"type": "missing", "missing": false}}
            ]}
        ]}"#,
    )
    .unwrap();
    env.create("Everything", rule.clone());
    let listed = env.list();
    assert_eq!(listed[0].rule.as_ref(), Some(&rule));
    let stored: String = env
        .db()
        .query_row("SELECT rule FROM filter_preset", [], |r| r.get(0))
        .unwrap();
    assert_eq!(serde_json::from_str::<RuleNode>(&stored).unwrap(), rule);

    let json = serde_json::to_value(&listed[0]).unwrap();
    assert_eq!(json["name"], "Everything");
    assert_eq!(json["rule"]["rules"][4]["rules"][1]["filter"]["id"], 3);
    assert!(json["createdAt"].is_string() && json["updatedAt"].is_string());
}

#[test]
fn a_rule_that_does_not_parse_still_lists() {
    let env = Env::new();
    let id = env.create("Future", techno());
    env.db()
        .execute(
            "UPDATE filter_preset SET rule = '{\"type\":\"filter\",\"filter\":{\"type\":\"mood\"}}'",
            [],
        )
        .unwrap();
    let listed = env.list();
    assert_eq!(listed[0].rule, None);
    assert!(serde_json::to_value(&listed[0]).unwrap()["rule"].is_null());

    let copy = env
        .mutate(FilterPresetOp::Duplicate { id })
        .unwrap()
        .unwrap();
    assert_eq!(env.list()[1].rule, None);
    env.mutate(FilterPresetOp::SetRule {
        id: copy,
        rule: techno(),
    })
    .unwrap();
    assert_eq!(env.list()[1].rule, Some(techno()));
}

#[test]
fn deleted_tags_and_playlists_leave_the_rule_as_saved() {
    let env = Env::new();
    let t = env.track();
    let cat = env
        .lib
        .tag_mutate(TagOp::CreateCategory {
            name: "Mood".into(),
            position: None,
        })
        .unwrap()
        .created_id
        .unwrap();
    let dark = env
        .lib
        .tag_mutate(TagOp::CreateTag {
            category_id: cat,
            name: "Dark".into(),
            position: None,
        })
        .unwrap()
        .created_id
        .unwrap();
    env.lib
        .tag_mutate(TagOp::Assign {
            track_ids: vec![t],
            tag_ids: vec![dark],
        })
        .unwrap();
    let PlaylistOutcome::Done {
        created_id: Some(set),
        ..
    } = env
        .lib
        .playlist_mutate(PlaylistOp::CreatePlaylist {
            parent_id: None,
            name: "Set".into(),
            position: None,
        })
        .unwrap()
    else {
        panic!("no playlist")
    };
    env.lib
        .playlist_mutate(PlaylistOp::AddTracks {
            playlist_id: set,
            track_ids: vec![t],
            before: None,
            duplicates: Default::default(),
        })
        .unwrap();

    let rule = all(vec![
        leaf(Filter::TagAny { ids: vec![dark] }),
        leaf(Filter::Playlist { id: set }),
    ]);
    env.create("Dark set", rule.clone());
    let matches = |rule: &RuleNode| {
        env.lib
            .tracks(&TrackQuery {
                rule: Some(rule.clone()),
                ..TrackQuery::default()
            })
            .unwrap()
            .total
    };
    assert_eq!(matches(&rule), 1);

    env.lib.tag_mutate(TagOp::Delete { id: dark }).unwrap();
    env.lib
        .playlist_mutate(PlaylistOp::Delete { id: set })
        .unwrap();
    let loaded = env.list()[0].rule.clone().unwrap();
    assert_eq!(loaded, rule, "ids are kept as saved");
    assert_eq!(matches(&loaded), 0, "a deleted id matches no track");
}

#[test]
fn serde_shapes() {
    let create: FilterPresetOp = serde_json::from_str(
        r#"{"type": "create", "name": "Peak", "position": 0,
            "rule": {"type": "group", "match": "all", "rules": []}}"#,
    )
    .unwrap();
    assert_eq!(
        create,
        FilterPresetOp::Create {
            name: "Peak".into(),
            rule: all(vec![]),
            position: Some(0),
        }
    );
    let set: FilterPresetOp = serde_json::from_str(
        r#"{"type": "setRule", "id": 2,
            "rule": {"type": "filter", "filter": {"type": "neverPlayed"}}}"#,
    )
    .unwrap();
    assert_eq!(
        set,
        FilterPresetOp::SetRule {
            id: 2,
            rule: leaf(Filter::NeverPlayed),
        }
    );
    let mv: FilterPresetOp =
        serde_json::from_str(r#"{"type": "move", "id": 2, "position": 1}"#).unwrap();
    assert_eq!(mv, FilterPresetOp::Move { id: 2, position: 1 });
    assert!(
        serde_json::from_str::<FilterPresetOp>(r#"{"type": "delete", "id": 2, "x": 1}"#).is_err()
    );

    let env = Env::new();
    let id = env.create("Peak", techno());
    let out = env
        .lib
        .filter_preset_mutate(FilterPresetOp::Duplicate { id })
        .unwrap();
    assert!(serde_json::to_value(out).unwrap()["createdId"].is_i64());
}
