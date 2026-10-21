//! `Library::tags` and `Library::tag_mutate`: every op, each rejected case,
//! dense sort orders, cascades, `TrackRow::tags` and `Filter::TagAny`.

// Test helpers outside `#[test]` fns are not covered by clippy's allow-unwrap-in-tests.
#![allow(clippy::unwrap_used, clippy::panic)]

use reks_core::{Error, Filter, Library, TagOp, TagOutcome, TagTree, TrackQuery};
use rusqlite::Connection;

/// Every `tag` row (id, parent, name, order, updated) and every `track_tag` link.
type Snapshot = (
    Vec<(i64, Option<i64>, String, i64, String)>,
    Vec<(i64, i64)>,
);

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

    fn mutate(&self, op: TagOp) -> TagOutcome {
        self.lib.tag_mutate(op).unwrap()
    }

    fn category(&self, name: &str) -> i64 {
        self.mutate(TagOp::CreateCategory {
            name: name.into(),
            position: None,
        })
        .created_id
        .unwrap()
    }

    fn tag(&self, category: i64, name: &str) -> i64 {
        self.mutate(TagOp::CreateTag {
            category_id: category,
            name: name.into(),
            position: None,
        })
        .created_id
        .unwrap()
    }

    fn tree(&self) -> TagTree {
        self.lib.tags().unwrap()
    }

    /// Category names, each with its tags' names and counts.
    fn outline(&self) -> Vec<(String, Vec<(String, u32)>)> {
        self.tree()
            .categories
            .into_iter()
            .map(|c| {
                (
                    c.name,
                    c.tags
                        .into_iter()
                        .map(|t| (t.name, t.track_count))
                        .collect(),
                )
            })
            .collect()
    }

    /// Every `tag` and `track_tag` row, for "rejected ops change nothing".
    fn snapshot(&self) -> Snapshot {
        let c = self.db();
        let tags = c
            .prepare("SELECT id, parent_id, name, sort_order, updated_at FROM tag ORDER BY id")
            .unwrap()
            .query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        let links = c
            .prepare("SELECT track_id, tag_id FROM track_tag ORDER BY track_id, tag_id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        (tags, links)
    }

    fn rejects(&self, op: TagOp) -> Error {
        let before = self.snapshot();
        let err = self.lib.tag_mutate(op).unwrap_err();
        assert_eq!(self.snapshot(), before, "a rejected op wrote something");
        err
    }

    fn sort_orders(&self) -> Vec<(Option<i64>, Vec<i64>)> {
        let c = self.db();
        let parents: Vec<Option<i64>> = c
            .prepare("SELECT DISTINCT parent_id FROM tag ORDER BY parent_id")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        parents
            .into_iter()
            .map(|p| {
                let orders = c
                    .prepare("SELECT sort_order FROM tag WHERE parent_id IS ?1 ORDER BY sort_order")
                    .unwrap()
                    .query_map([p], |r| r.get(0))
                    .unwrap()
                    .collect::<Result<_, _>>()
                    .unwrap();
                (p, orders)
            })
            .collect()
    }

    fn assert_dense(&self) {
        for (parent, orders) in self.sort_orders() {
            let want: Vec<i64> = (0..orders.len() as i64).collect();
            assert_eq!(orders, want, "sort_order under {parent:?}");
        }
    }
}

fn names(outline: &[(String, Vec<(String, u32)>)]) -> Vec<&str> {
    outline.iter().map(|(n, _)| n.as_str()).collect()
}

#[test]
fn starts_with_the_rekordbox_categories() {
    let env = Env::new();
    let outline = env.outline();
    assert_eq!(names(&outline), ["Genre", "Components", "Situation"]);
    assert!(outline.iter().all(|(_, tags)| tags.is_empty()));
}

#[test]
fn creates_categories_and_tags_at_positions() {
    let env = Env::new();
    let mood = env.category("Mood");
    let first = env.mutate(TagOp::CreateCategory {
        name: "Energy".into(),
        position: Some(0),
    });
    assert!(first.created_id.is_some());
    env.tag(mood, "Dark");
    env.mutate(TagOp::CreateTag {
        category_id: mood,
        name: "Happy".into(),
        position: Some(0),
    });
    let outline = env.outline();
    assert_eq!(
        names(&outline),
        ["Energy", "Genre", "Components", "Situation", "Mood"]
    );
    assert_eq!(
        outline[4].1,
        [("Happy".to_owned(), 0), ("Dark".to_owned(), 0)]
    );
    env.assert_dense();
}

#[test]
fn rename_move_and_delete() {
    let env = Env::new();
    let genre = env.tree().categories[0].id;
    let situation = env.tree().categories[2].id;
    let deep = env.tag(genre, "Deep");
    let peak = env.tag(genre, "Peak");
    env.tag(genre, "Tool");

    env.mutate(TagOp::Rename {
        id: deep,
        name: "DEEP".into(),
    });
    // Within a category.
    env.mutate(TagOp::Move {
        id: peak,
        category_id: Some(genre),
        position: 0,
    });
    // To another category.
    env.mutate(TagOp::Move {
        id: deep,
        category_id: Some(situation),
        position: 0,
    });
    // A category among categories.
    env.mutate(TagOp::Move {
        id: situation,
        category_id: None,
        position: 0,
    });
    let outline = env.outline();
    assert_eq!(names(&outline), ["Situation", "Genre", "Components"]);
    assert_eq!(outline[0].1, [("DEEP".to_owned(), 0)]);
    assert_eq!(
        outline[1].1,
        [("Peak".to_owned(), 0), ("Tool".to_owned(), 0)]
    );
    env.assert_dense();

    env.mutate(TagOp::Delete { id: peak });
    env.mutate(TagOp::Delete { id: situation });
    let outline = env.outline();
    assert_eq!(names(&outline), ["Genre", "Components"]);
    assert_eq!(outline[0].1, [("Tool".to_owned(), 0)]);
    env.assert_dense();
}

#[test]
fn assign_and_unassign_count_links() {
    let env = Env::new();
    let t = env.tracks(3);
    let genre = env.tree().categories[0].id;
    let deep = env.tag(genre, "Deep");
    let peak = env.tag(genre, "Peak");

    let out = env.mutate(TagOp::Assign {
        track_ids: vec![t[0], t[1], t[0]],
        tag_ids: vec![deep, peak],
    });
    assert_eq!(out.changed, 4);
    let again = env.mutate(TagOp::Assign {
        track_ids: vec![t[0]],
        tag_ids: vec![deep],
    });
    assert_eq!(again.changed, 0, "assign is idempotent");
    assert_eq!(
        env.outline()[0].1,
        [("Deep".to_owned(), 2), ("Peak".to_owned(), 2)]
    );

    let out = env.mutate(TagOp::Unassign {
        track_ids: vec![t[1], t[2]],
        tag_ids: vec![deep],
    });
    assert_eq!(out.changed, 1);
    assert_eq!(
        env.outline()[0].1,
        [("Deep".to_owned(), 1), ("Peak".to_owned(), 2)]
    );

    // Deleting a tag or a track removes its links.
    env.mutate(TagOp::Delete { id: peak });
    env.db()
        .execute("DELETE FROM track WHERE id = ?1", [t[0]])
        .unwrap();
    assert!(env.snapshot().1.is_empty());
}

#[test]
fn rejects_invalid_ops() {
    let env = Env::new();
    let t = env.tracks(1);
    let genre = env.tree().categories[0].id;
    let components = env.tree().categories[1].id;
    let deep = env.tag(genre, "Deep");
    env.tag(components, "Vocal");

    let err = env.rejects(TagOp::CreateCategory {
        name: "  ".into(),
        position: None,
    });
    assert!(
        matches!(err, Error::InvalidField { field: "name", .. }),
        "{err}"
    );
    let err = env.rejects(TagOp::CreateCategory {
        name: "genre".into(),
        position: None,
    });
    assert!(
        matches!(err, Error::InvalidField { field: "name", .. }),
        "{err}"
    );
    let err = env.rejects(TagOp::CreateTag {
        category_id: genre,
        name: "deep".into(),
        position: None,
    });
    assert!(
        matches!(err, Error::InvalidField { field: "name", .. }),
        "{err}"
    );
    let err = env.rejects(TagOp::CreateTag {
        category_id: deep,
        name: "Nested".into(),
        position: None,
    });
    assert!(
        matches!(err, Error::NotATagCategory(id) if id == deep),
        "{err}"
    );
    let err = env.rejects(TagOp::CreateTag {
        category_id: genre,
        name: "Late".into(),
        position: Some(9),
    });
    assert!(matches!(err, Error::IndexOutOfRange { .. }), "{err}");
    let err = env.rejects(TagOp::Rename {
        id: 999,
        name: "X".into(),
    });
    assert!(matches!(err, Error::TagNotFound(999)), "{err}");
    let err = env.rejects(TagOp::Rename {
        id: genre,
        name: "Components".into(),
    });
    assert!(matches!(err, Error::InvalidField { .. }), "{err}");
    let err = env.rejects(TagOp::Move {
        id: genre,
        category_id: Some(components),
        position: 0,
    });
    assert!(
        matches!(
            err,
            Error::InvalidField {
                field: "categoryId",
                ..
            }
        ),
        "{err}"
    );
    let err = env.rejects(TagOp::Move {
        id: deep,
        category_id: None,
        position: 0,
    });
    assert!(
        matches!(
            err,
            Error::InvalidField {
                field: "categoryId",
                ..
            }
        ),
        "{err}"
    );
    // "Deep" can't move into a category that has a tag of that name.
    env.tag(components, "DEEP");
    let err = env.rejects(TagOp::Move {
        id: deep,
        category_id: Some(components),
        position: 0,
    });
    assert!(
        matches!(err, Error::InvalidField { field: "name", .. }),
        "{err}"
    );
    let err = env.rejects(TagOp::Assign {
        track_ids: t.clone(),
        tag_ids: vec![genre],
    });
    assert!(matches!(err, Error::NotATag(id) if id == genre), "{err}");
    let err = env.rejects(TagOp::Assign {
        track_ids: vec![999],
        tag_ids: vec![deep],
    });
    assert!(matches!(err, Error::TrackNotFound(999)), "{err}");
}

#[test]
fn rename_to_another_case_of_itself_is_allowed() {
    let env = Env::new();
    let genre = env.tree().categories[0].id;
    env.mutate(TagOp::Rename {
        id: genre,
        name: "GENRE".into(),
    });
    assert_eq!(env.outline()[0].0, "GENRE");
}

#[test]
fn track_rows_carry_tags_and_filter_by_any_tag() {
    let env = Env::new();
    let t = env.tracks(4);
    let genre = env.tree().categories[0].id;
    let situation = env.tree().categories[2].id;
    let warmup = env.tag(situation, "Warm-up");
    let deep = env.tag(genre, "Deep");
    let peak = env.tag(genre, "Peak");
    env.mutate(TagOp::Assign {
        track_ids: vec![t[0]],
        tag_ids: vec![warmup, peak, deep],
    });
    env.mutate(TagOp::Assign {
        track_ids: vec![t[1]],
        tag_ids: vec![peak],
    });
    env.mutate(TagOp::Assign {
        track_ids: vec![t[2]],
        tag_ids: vec![warmup],
    });

    let ids = |filters: Vec<Filter>| -> Vec<i64> {
        let page = env
            .lib
            .tracks(&TrackQuery {
                filters,
                ..TrackQuery::default()
            })
            .unwrap();
        page.rows.iter().map(|r| r.id).collect()
    };
    let page = env.lib.tracks(&TrackQuery::default()).unwrap();
    // Category order, then tag order.
    assert_eq!(page.rows[0].tags, vec![deep, peak, warmup]);
    assert_eq!(page.rows[3].tags, Vec::<i64>::new());
    assert_eq!(
        env.lib.track(t[0]).unwrap().track.row.tags,
        vec![deep, peak, warmup]
    );

    assert_eq!(ids(vec![Filter::TagAny { ids: vec![peak] }]), [t[0], t[1]]);
    assert_eq!(
        ids(vec![Filter::TagAny {
            ids: vec![deep, warmup]
        }]),
        [t[0], t[2]]
    );
    assert!(ids(vec![Filter::TagAny { ids: vec![] }]).is_empty());
    // Several TagAny filters are ANDed.
    assert_eq!(
        ids(vec![
            Filter::TagAny { ids: vec![peak] },
            Filter::TagAny { ids: vec![warmup] }
        ]),
        [t[0]]
    );
}

#[test]
fn serde_shapes() {
    let op: TagOp =
        serde_json::from_str(r#"{"type":"createTag","categoryId":1,"name":"Deep"}"#).unwrap();
    assert_eq!(
        op,
        TagOp::CreateTag {
            category_id: 1,
            name: "Deep".into(),
            position: None
        }
    );
    let op: TagOp = serde_json::from_str(r#"{"type":"move","id":5,"position":0}"#).unwrap();
    assert_eq!(
        op,
        TagOp::Move {
            id: 5,
            category_id: None,
            position: 0
        }
    );
    let op: TagOp =
        serde_json::from_str(r#"{"type":"assign","trackIds":[1],"tagIds":[2]}"#).unwrap();
    assert!(matches!(op, TagOp::Assign { .. }));
    assert!(serde_json::from_str::<TagOp>(r#"{"type":"delete","id":1,"x":0}"#).is_err());

    let env = Env::new();
    let genre = env.tree().categories[0].id;
    let out = serde_json::to_value(env.mutate(TagOp::CreateTag {
        category_id: genre,
        name: "Deep".into(),
        position: None,
    }))
    .unwrap();
    assert_eq!(out["changed"], 0);
    assert!(out["createdId"].is_i64());
    let tree = serde_json::to_value(env.tree()).unwrap();
    assert_eq!(tree["categories"][0]["name"], "Genre");
    assert_eq!(tree["categories"][0]["tags"][0]["name"], "Deep");
    assert_eq!(tree["categories"][0]["tags"][0]["trackCount"], 0);
}
