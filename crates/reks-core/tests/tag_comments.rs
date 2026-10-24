//! My Tags ↔ Comments (D36): reading tags from the library's comments,
//! `TagOp::Merge`, duplicate groups and the write-to-Comments setting.

// Test helpers outside `#[test]` fns are not covered by clippy's allow-unwrap-in-tests.
#![allow(clippy::unwrap_used, clippy::panic)]

use reks_core::{Error, Library, SettingsPatch, TagOp, TagOutcome, TrackPatch};
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

    /// One track per comment.
    fn tracks(&self, comments: &[Option<&str>]) -> Vec<i64> {
        let c = Connection::open(self.dir.path().join("reks.sqlite")).unwrap();
        comments
            .iter()
            .enumerate()
            .map(|(i, comment)| {
                c.execute(
                    "INSERT INTO track (path, file_size, file_mtime, kind, title, comment,
                                        date_added, created_at, updated_at)
                     VALUES (?1, 1, ?2, 'wav', ?1, ?3, '2026-01-01', ?2, ?2)",
                    rusqlite::params![format!("/music/{i}.wav"), OLD, comment],
                )
                .unwrap();
                c.last_insert_rowid()
            })
            .collect()
    }

    fn mutate(&self, op: TagOp) -> TagOutcome {
        self.lib.tag_mutate(op).unwrap()
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

    fn category(&self, i: usize) -> i64 {
        self.lib.tags().unwrap().categories[i].id
    }

    fn comment(&self, track: i64) -> Option<String> {
        self.lib.track(track).unwrap().track.row.comment
    }

    fn tags_of(&self, track: i64) -> Vec<i64> {
        self.lib.track(track).unwrap().track.row.tags
    }

    fn write_back(&self, on: bool) {
        self.lib
            .update_settings(SettingsPatch {
                tags_in_comments: Some(on),
                ..SettingsPatch::default()
            })
            .unwrap();
    }
}

#[test]
fn library_comments_are_read_into_tags() {
    let env = Env::new();
    let deep = env.tag(env.category(0), "Deep");
    let t = env.tracks(&[
        Some("ok /* deep / Peak */"),
        Some(" /* Peak / Warm\u{2010}up */"),
        Some("no block"),
        None,
    ]);
    let preview = env.lib.comment_tags_preview().unwrap();
    assert_eq!(
        (preview.tracks, preview.names, preview.new_names),
        (2, 3, 2)
    );

    let report = env.lib.comment_tags_import().unwrap();
    assert_eq!((report.tracks, report.created, report.assigned), (2, 2, 4));
    assert_eq!(report.tag_ids.len(), 3);
    assert!(report.tag_ids.contains(&deep));
    assert_eq!(env.tags_of(t[0])[0], deep);
    assert!(env.tags_of(t[2]).is_empty());
    // Comments are left as they are while write-back is off.
    assert_eq!(env.comment(t[0]).as_deref(), Some("ok /* deep / Peak */"));

    let again = env.lib.comment_tags_import().unwrap();
    assert_eq!((again.created, again.assigned), (0, 0));
}

#[test]
fn merge_moves_links_and_deletes_the_others() {
    let env = Env::new();
    let (genre, situation) = (env.category(0), env.category(2));
    let prog = env.tag(genre, "Prog Trance");
    let prog2 = env.tag(situation, "prog-trance");
    let prog3 = env.tag(genre, "ProgTrance");
    let peak = env.tag(genre, "Peak");
    let t = env.tracks(&[None, None, None]);
    env.mutate(TagOp::Assign {
        track_ids: vec![t[0], t[1]],
        tag_ids: vec![prog2],
    });
    env.mutate(TagOp::Assign {
        track_ids: vec![t[1], t[2]],
        tag_ids: vec![prog],
    });

    assert_eq!(
        env.lib.tag_duplicates().unwrap(),
        vec![vec![prog, prog3, prog2]]
    );
    let out = env.mutate(TagOp::Merge {
        ids: vec![prog2, prog3],
        into: prog,
    });
    // t[1] already had `prog`.
    assert_eq!(out.changed, 1);
    for track in &t {
        assert_eq!(env.tags_of(*track), vec![prog]);
    }
    let tree = env.lib.tags().unwrap();
    let names: Vec<&str> = tree.categories[0]
        .tags
        .iter()
        .map(|g| g.name.as_str())
        .collect();
    assert_eq!(names, ["Prog Trance", "Peak"]);
    assert!(tree.categories[2].tags.is_empty());
    assert!(env.lib.tag_duplicates().unwrap().is_empty());

    for (ids, into) in [
        (vec![], peak),
        (vec![peak], peak),
        (vec![peak], genre),
        (vec![genre], peak),
        (vec![999], peak),
    ] {
        let err = env.lib.tag_mutate(TagOp::Merge { ids, into }).unwrap_err();
        assert!(
            matches!(
                err,
                Error::InvalidField { .. } | Error::NotATag(_) | Error::TagNotFound(_)
            ),
            "{err:?}"
        );
    }
    assert_eq!(env.lib.tags().unwrap().categories[0].tags.len(), 2);
}

#[test]
fn write_back_keeps_comment_blocks_in_step_with_tags() {
    let env = Env::new();
    let (genre, comps) = (env.category(0), env.category(1));
    let deep = env.tag(genre, "Deep");
    let vocal = env.tag(comps, "Vocal");
    let t = env.tracks(&[Some("Visit x /* Old */"), None, Some("mine /* keep */")]);

    // Off: nothing is written.
    let out = env.mutate(TagOp::Assign {
        track_ids: vec![t[0]],
        tag_ids: vec![vocal, deep],
    });
    assert_eq!(out.comments_updated, 0);
    assert_eq!(env.comment(t[0]).as_deref(), Some("Visit x /* Old */"));

    // On: turning it on rewrites nothing; sync-all counts and rewrites tagged
    // tracks only (t[2] keeps its untagged block).
    env.write_back(true);
    assert_eq!(env.comment(t[0]).as_deref(), Some("Visit x /* Old */"));
    assert_eq!(env.lib.comment_tags_sync_all(false).unwrap(), 1);
    assert_eq!(env.comment(t[0]).as_deref(), Some("Visit x /* Old */"));
    assert_eq!(env.lib.comment_tags_sync_all(true).unwrap(), 1);
    assert_eq!(
        env.comment(t[0]).as_deref(),
        Some("Visit x /* Deep / Vocal */")
    );
    assert_eq!(env.comment(t[2]).as_deref(), Some("mine /* keep */"));

    // Every op that changes a track's tags, names or their order.
    let out = env.mutate(TagOp::Assign {
        track_ids: vec![t[1]],
        tag_ids: vec![deep],
    });
    assert_eq!(out.comments_updated, 1);
    assert_eq!(env.comment(t[1]).as_deref(), Some(" /* Deep */"));
    env.mutate(TagOp::Rename {
        id: deep,
        name: "Deep House".into(),
    });
    assert_eq!(
        env.comment(t[0]).as_deref(),
        Some("Visit x /* Deep House / Vocal */")
    );
    env.mutate(TagOp::Move {
        id: comps,
        category_id: None,
        position: 0,
    });
    assert_eq!(
        env.comment(t[0]).as_deref(),
        Some("Visit x /* Vocal / Deep House */")
    );
    let house = env.tag(genre, "House");
    let out = env.mutate(TagOp::Merge {
        ids: vec![deep],
        into: house,
    });
    assert_eq!(out.comments_updated, 2);
    assert_eq!(
        env.comment(t[0]).as_deref(),
        Some("Visit x /* Vocal / House */")
    );
    env.mutate(TagOp::Delete { id: comps });
    assert_eq!(env.comment(t[0]).as_deref(), Some("Visit x /* House */"));
    env.mutate(TagOp::Unassign {
        track_ids: vec![t[0], t[1]],
        tag_ids: vec![house],
    });
    assert_eq!(env.comment(t[0]).as_deref(), Some("Visit x"));
    assert_eq!(env.comment(t[1]), None);

    // Editing a tagged track's comment keeps the block from its tags.
    env.mutate(TagOp::Assign {
        track_ids: vec![t[0]],
        tag_ids: vec![house],
    });
    env.lib
        .update_track(
            t[0],
            &TrackPatch {
                comment: Some(Some("new text /* typed */".into())),
                ..TrackPatch::default()
            },
        )
        .unwrap();
    assert_eq!(env.comment(t[0]).as_deref(), Some("new text /* House */"));
}
