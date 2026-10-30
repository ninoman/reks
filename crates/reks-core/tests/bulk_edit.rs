//! `Library::bulk_update_tracks`: mixed values, untouched fields, comment
//! modes, tag add/remove and all-or-nothing rollback.

// Test helpers outside `#[test]` fns are not covered by clippy's allow-unwrap-in-tests.
#![allow(clippy::unwrap_used, clippy::panic)]

use reks_core::DataSource;
use reks_core::{BulkPatch, CommentEdit, Error, Library, TagOp};
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

    /// A track with its own title, artist and genre (so they differ) and a comment.
    fn track(&self, n: usize, comment: Option<&str>) -> i64 {
        let c = self.db();
        c.execute(
            "INSERT INTO track (path, file_size, file_mtime, kind, title, artist, genre, comment,
                                rating, color, key, key_source, date_added, created_at, updated_at)
             VALUES (?1, 1, ?2, 'mp3', ?1, ?3, ?4, ?5, ?6, 'red', 'Am', 'rekordbox',
                     '2026-01-01', ?2, ?2)",
            rusqlite::params![
                format!("/music/{n}.mp3"),
                OLD,
                format!("Artist {n}"),
                format!("Genre {n}"),
                comment,
                n as i64
            ],
        )
        .unwrap();
        c.last_insert_rowid()
    }

    fn tag(&self, name: &str) -> i64 {
        let cat = self
            .lib
            .tag_mutate(TagOp::CreateCategory {
                name: format!("cat {name}"),
                position: None,
            })
            .unwrap()
            .created_id
            .unwrap();
        self.lib
            .tag_mutate(TagOp::CreateTag {
                category_id: cat,
                name: name.into(),
                position: None,
            })
            .unwrap()
            .created_id
            .unwrap()
    }

    fn detail(&self, id: i64) -> reks_core::TrackDetail {
        self.lib.track(id).unwrap()
    }
}

#[test]
fn sets_only_the_given_fields_on_every_track() {
    let e = Env::new();
    let ids = [e.track(1, None), e.track(2, None), e.track(3, None)];
    let patch = BulkPatch {
        genre: Some(Some("House".into())),
        year: Some(Some(1999)),
        rating: Some(5),
        color: Some(None),
        ..BulkPatch::default()
    };
    let out = e.lib.bulk_update_tracks(&ids, &patch).unwrap();
    assert_eq!(out.rows.len(), 3);
    for (i, id) in ids.iter().enumerate() {
        let t = e.detail(*id).track;
        assert_eq!(t.row.genre.as_deref(), Some("House"));
        assert_eq!(t.row.year, Some(1999));
        assert_eq!(t.row.rating, 5);
        assert_eq!(t.row.color, None);
        // Untouched fields keep each track's own (mixed) value.
        assert_eq!(t.row.artist, Some(format!("Artist {}", i + 1)));
        assert_eq!(t.row.title, Some(format!("/music/{}.mp3", i + 1)));
        assert_eq!(t.row.key.as_deref(), Some("Am"));
        assert_ne!(t.updated_at, OLD);
        assert_eq!(out.rows[i].genre.as_deref(), Some("House"));
    }
}

#[test]
fn key_is_stored_canonical_and_marked_manual() {
    let e = Env::new();
    let ids = [e.track(1, None), e.track(2, None)];
    let patch = BulkPatch {
        key: Some(Some("8B".into())),
        ..BulkPatch::default()
    };
    e.lib.bulk_update_tracks(&ids, &patch).unwrap();
    for id in ids {
        let t = e.detail(id).track;
        assert_eq!(t.row.key.as_deref(), Some("C"));
        assert_eq!(t.key_source, Some(DataSource::Manual));
    }
}

#[test]
fn empty_patch_and_empty_selection_write_nothing() {
    let e = Env::new();
    let id = e.track(1, Some("x"));
    let out = e
        .lib
        .bulk_update_tracks(&[id], &BulkPatch::default())
        .unwrap();
    assert_eq!(out.rows.len(), 1);
    assert_eq!(e.detail(id).track.updated_at, OLD);
    let patch = BulkPatch {
        genre: Some(Some("X".into())),
        ..BulkPatch::default()
    };
    assert!(e
        .lib
        .bulk_update_tracks(&[], &patch)
        .unwrap()
        .rows
        .is_empty());
}

#[test]
fn comment_replace_and_append() {
    let e = Env::new();
    let ids = [
        e.track(1, Some("warm-up")),
        e.track(2, None),
        e.track(3, Some("  ")),
    ];
    let append = |text: &str| BulkPatch {
        comment: Some(CommentEdit::Append { text: text.into() }),
        ..BulkPatch::default()
    };
    e.lib.bulk_update_tracks(&ids, &append(" /x/ ")).unwrap();
    let comments: Vec<_> = ids
        .iter()
        .map(|&id| e.detail(id).track.row.comment)
        .collect();
    assert_eq!(
        comments,
        [
            Some("warm-up /x/".into()),
            Some("/x/".into()),
            Some("/x/".into())
        ]
    );

    // Appending nothing changes nothing.
    e.lib.bulk_update_tracks(&ids, &append("  ")).unwrap();
    assert_eq!(
        e.detail(ids[0]).track.row.comment.as_deref(),
        Some("warm-up /x/")
    );

    let replace = |text: Option<&str>| BulkPatch {
        comment: Some(CommentEdit::Replace {
            text: text.map(Into::into),
        }),
        ..BulkPatch::default()
    };
    e.lib
        .bulk_update_tracks(&ids, &replace(Some("new")))
        .unwrap();
    assert!(ids
        .iter()
        .all(|&id| e.detail(id).track.row.comment.as_deref() == Some("new")));
    e.lib.bulk_update_tracks(&ids, &replace(None)).unwrap();
    assert!(ids
        .iter()
        .all(|&id| e.detail(id).track.row.comment.is_none()));
}

#[test]
fn tags_are_added_and_removed_not_replaced() {
    let e = Env::new();
    let ids = [e.track(1, None), e.track(2, None)];
    let (a, b, c) = (e.tag("A"), e.tag("B"), e.tag("C"));
    let add = |tags: &[i64]| BulkPatch {
        add_tags: tags.to_vec(),
        ..BulkPatch::default()
    };
    e.lib.bulk_update_tracks(&[ids[0]], &add(&[a])).unwrap();
    let out = e.lib.bulk_update_tracks(&ids, &add(&[a, b])).unwrap();
    // A was already on track 1, so 3 links are new.
    assert_eq!(out.tags_added, 3);
    assert_eq!(e.detail(ids[0]).track.row.tags, [a, b]);
    assert_eq!(e.detail(ids[1]).track.row.tags, [a, b]);
    assert_eq!(out.rows[0].tags, [a, b]);

    let patch = BulkPatch {
        add_tags: vec![c],
        remove_tags: vec![a],
        ..BulkPatch::default()
    };
    let out = e.lib.bulk_update_tracks(&ids, &patch).unwrap();
    assert_eq!((out.tags_added, out.tags_removed), (2, 2));
    assert_eq!(e.detail(ids[0]).track.row.tags, [b, c]);
}

#[test]
fn a_tag_both_added_and_removed_is_refused() {
    let e = Env::new();
    let id = e.track(1, None);
    let a = e.tag("A");
    let patch = BulkPatch {
        add_tags: vec![a],
        remove_tags: vec![a],
        ..BulkPatch::default()
    };
    assert!(matches!(
        e.lib.bulk_update_tracks(&[id], &patch),
        Err(Error::InvalidField { field: "tags", .. })
    ));
}

/// A rejected edit leaves every track, comment and tag link as it was.
fn assert_unchanged(e: &Env, ids: &[i64], comments: &[Option<&str>]) {
    for (id, comment) in ids.iter().zip(comments) {
        let t = e.detail(*id).track;
        assert_eq!(t.updated_at, OLD);
        assert_eq!(t.row.comment.as_deref(), *comment);
        assert_eq!(t.row.genre, Some(format!("Genre {}", t.row.rating)));
        assert!(t.row.tags.is_empty());
    }
}

#[test]
fn invalid_value_rolls_everything_back() {
    let e = Env::new();
    let ids = [e.track(1, Some("a")), e.track(2, Some("b"))];
    let a = e.tag("A");
    let patch = BulkPatch {
        genre: Some(Some("House".into())),
        year: Some(Some(0)),
        add_tags: vec![a],
        ..BulkPatch::default()
    };
    assert!(matches!(
        e.lib.bulk_update_tracks(&ids, &patch),
        Err(Error::InvalidField { field: "year", .. })
    ));
    assert_unchanged(&e, &ids, &[Some("a"), Some("b")]);
}

#[test]
fn failure_on_a_later_track_rolls_back_earlier_ones() {
    let e = Env::new();
    let long = "x".repeat(1_020);
    let ids = [e.track(1, Some("a")), e.track(2, Some(&long))];
    let patch = BulkPatch {
        genre: Some(Some("House".into())),
        comment: Some(CommentEdit::Append {
            text: "more text".into(),
        }),
        ..BulkPatch::default()
    };
    assert!(matches!(
        e.lib.bulk_update_tracks(&ids, &patch),
        Err(Error::InvalidField {
            field: "comment",
            ..
        })
    ));
    assert_unchanged(&e, &ids, &[Some("a"), Some(&long)]);
}

#[test]
fn unknown_track_or_tag_changes_nothing() {
    let e = Env::new();
    let ids = [e.track(1, None), e.track(2, None)];
    let patch = BulkPatch {
        genre: Some(Some("House".into())),
        ..BulkPatch::default()
    };
    assert!(matches!(
        e.lib.bulk_update_tracks(&[ids[0], 9_999], &patch),
        Err(Error::TrackNotFound(9_999))
    ));
    let patch = BulkPatch {
        genre: Some(Some("House".into())),
        add_tags: vec![424_242],
        ..BulkPatch::default()
    };
    assert!(e.lib.bulk_update_tracks(&ids, &patch).is_err());
    assert_unchanged(&e, &ids, &[None, None]);
}

#[test]
fn patch_json_matches_the_ipc_shape() {
    let p: BulkPatch = serde_json::from_str(
        r#"{"genre": null, "rating": 4, "comment": {"mode": "append", "text": "x"},
            "addTags": [1], "removeTags": [2]}"#,
    )
    .unwrap();
    assert_eq!(p.genre, Some(None));
    assert_eq!(p.artist, None);
    assert_eq!(p.rating, Some(4));
    assert_eq!(p.add_tags, [1]);
    assert_eq!(p.remove_tags, [2]);
    // Fields that are unsafe in bulk are refused, not ignored.
    assert!(serde_json::from_str::<BulkPatch>(r#"{"title": "x"}"#).is_err());
    assert!(serde_json::from_str::<BulkPatch>(r#"{"bpmX100": 1}"#).is_err());
}
