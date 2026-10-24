//! Relocate missing files and change a library folder, on a temp dir.

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use lofty::config::WriteOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::tag::{Accessor, Tag};
use reks_core::manifest::{ManifestItemKind, ManifestRow};
use reks_core::relocate::{self, RelocateChoices, RelocatePlan, RelocateScope, ReviewBasis};
use reks_core::{scan, CancelToken, Error, Library};
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
        std::fs::create_dir_all(dir.path().join("music")).unwrap();
        Env { dir, lib }
    }

    fn root(&self) -> PathBuf {
        self.dir.path().join("music")
    }

    /// `elsewhere/`, outside the library folder.
    fn other(&self) -> PathBuf {
        self.dir.path().join("elsewhere")
    }

    fn fixture(ext: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/audio")
            .join(format!("sine-click-128.{ext}"))
    }

    /// Copies a fixture to `dir/rel`.
    fn put(dir: &Path, ext: &str, rel: &str) -> PathBuf {
        let dst = dir.join(rel);
        std::fs::create_dir_all(dst.parent().unwrap()).unwrap();
        std::fs::copy(Self::fixture(ext), &dst).unwrap();
        dst
    }

    /// Scans `music/` holding `sine.{wav,flac,mp3}`.
    fn populated() -> Self {
        let env = Self::new();
        for ext in ["wav", "flac", "mp3"] {
            Self::put(&env.root(), ext, &format!("sine.{ext}"));
        }
        env.lib.add_library_root(&env.root()).unwrap();
        env.scan();
        env
    }

    fn scan(&self) {
        scan(&[self.root()], &self.lib, &CancelToken::new(), |_| {}).unwrap();
    }

    fn db(&self) -> Connection {
        Connection::open(self.dir.path().join("reks.sqlite")).unwrap()
    }

    fn id(&self, name: &str) -> i64 {
        self.db()
            .query_row(
                "SELECT id FROM track WHERE path LIKE '%/' || ?1",
                [name],
                |r| r.get(0),
            )
            .unwrap()
    }

    fn row(&self, id: i64) -> (String, bool, i64, String) {
        self.db()
            .query_row(
                "SELECT path, missing, file_size, file_mtime FROM track WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap()
    }

    /// Moves `music/<name>` to `to` and rescans: the track is missing.
    fn lose(&self, name: &str, to: &Path) -> i64 {
        let id = self.id(name);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::rename(self.root().join(name), to).unwrap();
        self.scan();
        assert!(self.row(id).1, "{name} should be missing");
        id
    }

    fn plan(&self, scope: &RelocateScope, folder: &Path) -> RelocatePlan {
        relocate::plan(&self.lib, scope, folder, &CancelToken::new(), |_| {}).unwrap()
    }

    fn apply(
        &self,
        plan: &RelocatePlan,
        choices: &RelocateChoices,
    ) -> reks_core::relocate::RelocateReport {
        relocate::apply(&self.lib, plan, choices, &CancelToken::new(), |_| {}).unwrap()
    }
}

fn retag(path: &Path, title: &str) {
    let mut file = lofty::read_from_path(path).unwrap();
    if file.primary_tag().is_none() {
        file.insert_tag(Tag::new(file.primary_tag_type()));
    }
    file.primary_tag_mut().unwrap().set_title(title.into());
    file.save_to_path(path, WriteOptions::default()).unwrap();
}

fn path_str(p: &Path) -> String {
    p.components()
        .collect::<PathBuf>()
        .to_str()
        .unwrap()
        .to_owned()
}

#[test]
fn exact_hash_match_relinks_and_keeps_everything() {
    let env = Env::populated();
    let new = env.other().join("tidied/renamed track.flac");
    let id = env.lose("sine.flac", &new);

    // Things that hang off the track and must survive.
    env.db()
        .execute(
            "INSERT INTO cue (track_id, slot, kind, start_ms, source) VALUES (?1, 0, 0, 1500, 'manual')",
            [id],
        )
        .unwrap();
    env.lib
        .replace_manifest(
            Some("SERIAL-1"),
            &[ManifestRow {
                stick_path: "/Contents/a/sine.flac".into(),
                item_kind: ManifestItemKind::Track,
                track_id: Some(id),
            }],
        )
        .unwrap();

    let plan = env.plan(&RelocateScope::AllMissing, &env.other());
    assert_eq!((plan.looked_for, plan.files_seen), (1, 1));
    assert!(plan.review.is_empty() && plan.not_found.is_empty());
    assert!(!plan.inside_root);
    assert_eq!(plan.sure.len(), 1);
    assert_eq!(plan.sure[0].track.id, id);
    assert_eq!(plan.sure[0].file.path, path_str(&new));

    let report = env.apply(
        &plan,
        &RelocateChoices {
            add_root: true,
            ..Default::default()
        },
    );
    assert_eq!(report.relinked, 1);
    assert!(report.skipped.is_empty());
    assert_eq!(report.root.unwrap().path, path_str(&env.other()));

    let (path, missing, ..) = env.row(id);
    assert_eq!((path, missing), (path_str(&new), false));
    let cues: i64 = env
        .db()
        .query_row("SELECT count(*) FROM cue WHERE track_id = ?1", [id], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(cues, 1);
    let entry = env
        .lib
        .manifest_entry("SERIAL-1", "/Contents/a/sine.flac")
        .unwrap();
    assert_eq!(entry.unwrap().track_id, Some(id));
    assert_eq!(env.lib.library_roots().unwrap().len(), 2);

    // The scan, which now covers the new folder, finds nothing to change.
    scan(
        &[env.root(), env.other()],
        &env.lib,
        &CancelToken::new(),
        |_| {},
    )
    .unwrap();
    assert_eq!(env.row(id).0, path_str(&new));
}

#[test]
fn folder_inside_a_root_is_not_offered_as_a_root() {
    let env = Env::populated();
    let id = env.lose("sine.wav", &env.dir.path().join("away/sine.wav"));
    let inside = env.root().join("found");
    std::fs::create_dir_all(&inside).unwrap();
    std::fs::rename(
        env.dir.path().join("away/sine.wav"),
        inside.join("sine.wav"),
    )
    .unwrap();
    // Not scanned: the track is still flagged missing.
    let plan = env.plan(&RelocateScope::Tracks { ids: vec![id] }, &inside);
    assert!(plan.inside_root);
    let report = env.apply(
        &plan,
        &RelocateChoices {
            add_root: true,
            ..Default::default()
        },
    );
    assert_eq!((report.relinked, report.root), (1, None));
    assert_eq!(env.lib.library_roots().unwrap().len(), 1);
}

#[test]
fn identical_copies_need_a_pick() {
    let env = Env::populated();
    let id = env.lose("sine.mp3", &env.dir.path().join("trash/sine.mp3"));
    Env::put(&env.other(), "mp3", "a/copy.mp3");
    Env::put(&env.other(), "mp3", "b/copy.mp3");

    let plan = env.plan(&RelocateScope::AllMissing, &env.other());
    assert!(plan.sure.is_empty());
    let [item] = plan.review.as_slice() else {
        panic!("{plan:?}")
    };
    assert_eq!((item.track.id, item.basis), (id, ReviewBasis::SameContent));
    assert_eq!(item.candidates.len(), 2);

    // Skipped by default.
    let report = env.apply(&plan, &RelocateChoices::default());
    assert_eq!(report.relinked, 0);
    assert!(env.row(id).1);

    let pick = item.candidates[1].path.clone();
    let choices = RelocateChoices {
        picks: [(id, pick.clone())].into(),
        add_root: false,
    };
    assert_eq!(env.apply(&plan, &choices).relinked, 1);
    assert_eq!(env.row(id).0, pick);
}

#[test]
fn name_alone_is_never_relinked() {
    let env = Env::populated();
    let id = env.lose("sine.wav", &env.dir.path().join("trash/sine.wav"));
    let (_, _, size_before, mtime_before) = env.row(id);

    // Same name, same length, but not the same file (retagged).
    let look_alike = Env::put(&env.other(), "wav", "sine.wav");
    retag(&look_alike, "Another Edit");
    // Unrelated name and format: no candidate.
    Env::put(&env.other(), "flac", "other.flac");

    let plan = env.plan(&RelocateScope::AllMissing, &env.other());
    assert!(plan.sure.is_empty(), "{plan:?}");
    let [item] = plan.review.as_slice() else {
        panic!("{plan:?}")
    };
    assert_eq!(item.basis, ReviewBasis::Possible);
    assert_eq!(item.candidates.len(), 1);
    assert_eq!(item.candidates[0].path, path_str(&look_alike));

    // Applying without a pick changes nothing.
    let report = env.apply(&plan, &RelocateChoices::default());
    assert_eq!(report.relinked, 0);
    assert!(env.row(id).1);

    // A pick relinks, keeping the stored size and mtime so the next scan
    // refreshes the track from the new file.
    let choices = RelocateChoices {
        picks: [(id, path_str(&look_alike))].into(),
        add_root: false,
    };
    assert_eq!(env.apply(&plan, &choices).relinked, 1);
    let (path, missing, size, mtime) = env.row(id);
    assert_eq!((path, missing), (path_str(&look_alike), false));
    assert_eq!((size, mtime), (size_before, mtime_before));
}

#[test]
fn tags_make_a_candidate_with_a_different_file_name() {
    let env = Env::populated();
    let id = env.lose("sine.flac", &env.dir.path().join("trash/x.flac"));
    // The scan read these tags from the fixture.
    let (artist, title): (Option<String>, Option<String>) = env
        .db()
        .query_row("SELECT artist, title FROM track WHERE id = ?1", [id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert!(artist.is_some() && title.is_some(), "fixture has no tags");
    let moved = Env::put(&env.other(), "flac", "completely different name.flac");
    retag(&moved, title.as_deref().unwrap());

    let plan = env.plan(&RelocateScope::AllMissing, &env.other());
    let [item] = plan.review.as_slice() else {
        panic!("{plan:?}")
    };
    assert_eq!(item.basis, ReviewBasis::Possible);
    assert_eq!(item.candidates[0].path, path_str(&moved));
}

#[test]
fn nothing_in_the_folder_is_not_found() {
    let env = Env::populated();
    let id = env.lose("sine.wav", &env.dir.path().join("trash/sine.wav"));
    std::fs::create_dir_all(env.other()).unwrap();
    Env::put(&env.other(), "flac", "unrelated.flac");
    let plan = env.plan(&RelocateScope::AllMissing, &env.other());
    assert_eq!(plan.not_found.len(), 1);
    assert_eq!(plan.not_found[0].id, id);
    assert!(plan.sure.is_empty() && plan.review.is_empty());
}

#[test]
fn scope_limits_which_tracks_are_looked_for() {
    let env = Env::populated();
    let wav = env.lose("sine.wav", &env.other().join("sine.wav"));
    let mp3 = env.lose("sine.mp3", &env.other().join("sine.mp3"));
    let present = env.id("sine.flac");

    let plan = env.plan(
        &RelocateScope::Tracks {
            ids: vec![wav, present],
        },
        &env.other(),
    );
    assert_eq!(plan.looked_for, 1);
    assert_eq!(
        plan.sure.iter().map(|m| m.track.id).collect::<Vec<_>>(),
        [wav]
    );
    assert_ne!(wav, mp3);
}

#[test]
fn files_the_library_already_has_are_not_candidates() {
    let env = Env::populated();
    env.lose("sine.wav", &env.dir.path().join("trash/sine.wav"));
    // Searching the library folder itself: its files are all known.
    let plan = env.plan(&RelocateScope::AllMissing, &env.root());
    assert_eq!(plan.files_seen, 0);
    assert_eq!(plan.not_found.len(), 1);
}

#[test]
fn a_pick_outside_the_dry_run_is_refused() {
    let env = Env::populated();
    let id = env.lose("sine.wav", &env.other().join("sine.wav"));
    let plan = env.plan(&RelocateScope::AllMissing, &env.other());
    let choices = RelocateChoices {
        picks: [(id, "/etc/passwd.wav".to_owned())].into(),
        add_root: false,
    };
    let err = relocate::apply(&env.lib, &plan, &choices, &CancelToken::new(), |_| {}).unwrap_err();
    assert!(matches!(err, Error::RelocateRefused(_)), "{err}");
    assert!(env.row(id).1);
}

#[test]
fn a_file_changed_since_the_dry_run_is_skipped() {
    let env = Env::populated();
    let id = env.lose("sine.wav", &env.other().join("sine.wav"));
    let plan = env.plan(&RelocateScope::AllMissing, &env.other());
    std::fs::write(env.other().join("sine.wav"), b"short").unwrap();
    let report = env.apply(&plan, &RelocateChoices::default());
    assert_eq!(report.relinked, 0);
    assert_eq!(report.skipped.len(), 1);
    assert!(env.row(id).1);
}

#[test]
fn cancelled_plan_cannot_be_applied_and_cancelled_apply_changes_nothing() {
    let env = Env::populated();
    let id = env.lose("sine.wav", &env.other().join("sine.wav"));

    let cancel = CancelToken::new();
    cancel.cancel();
    let stopped = relocate::plan(
        &env.lib,
        &RelocateScope::AllMissing,
        &env.other(),
        &cancel,
        |_| {},
    )
    .unwrap();
    assert!(stopped.cancelled);
    let err = relocate::apply(
        &env.lib,
        &stopped,
        &RelocateChoices::default(),
        &CancelToken::new(),
        |_| {},
    )
    .unwrap_err();
    assert!(matches!(err, Error::RelocateRefused(_)));

    let plan = env.plan(&RelocateScope::AllMissing, &env.other());
    assert_eq!(plan.sure.len(), 1);
    let before = env.row(id);
    let report = relocate::apply(
        &env.lib,
        &plan,
        &RelocateChoices {
            add_root: true,
            ..Default::default()
        },
        &cancel,
        |_| {},
    )
    .unwrap();
    assert!(report.cancelled);
    assert_eq!((report.relinked, report.root), (0, None));
    assert_eq!(env.row(id), before);
    assert_eq!(env.lib.library_roots().unwrap().len(), 1);
}

// --- change library folder ---

fn paths(env: &Env) -> Vec<String> {
    let db = env.db();
    let mut stmt = db.prepare("SELECT path FROM track ORDER BY path").unwrap();
    let rows = stmt.query_map([], |r| r.get(0)).unwrap();
    rows.map(Result::unwrap).collect()
}

#[test]
fn changing_a_library_folder_round_trips() {
    let env = Env::populated();
    let root = env.lib.add_library_root(&env.root()).unwrap();
    let original = paths(&env);
    let new = env.dir.path().join("Volumes/music");
    std::fs::create_dir_all(new.parent().unwrap()).unwrap();
    std::fs::rename(env.root(), &new).unwrap();

    let plan = relocate::plan_remap(&env.lib, root.id, &new).unwrap();
    assert_eq!(
        (
            plan.tracks,
            plan.collisions,
            plan.sampled,
            plan.sample_found
        ),
        (3, 0, 3, 3)
    );
    assert_eq!(plan.examples.len(), 3);
    assert!(plan.examples[0].to.starts_with(&plan.new_path));

    let report = relocate::remap(&env.lib, root.id, &new).unwrap();
    assert_eq!((report.tracks, report.roots), (3, 1));
    let moved = paths(&env);
    assert!(
        moved.iter().all(|p| p.starts_with(&path_str(&new))),
        "{moved:?}"
    );
    assert_eq!(env.lib.library_roots().unwrap()[0].path, path_str(&new));
    assert_eq!(
        env.db()
            .query_row("SELECT count(*) FROM track WHERE missing = 1", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );

    // And back again.
    std::fs::rename(&new, env.root()).unwrap();
    relocate::remap(&env.lib, root.id, &env.root()).unwrap();
    assert_eq!(paths(&env), original);
    assert_eq!(
        env.lib.library_roots().unwrap()[0].path,
        path_str(&env.root())
    );
}

#[test]
fn remap_moves_library_folders_inside_and_leaves_other_tracks_alone() {
    let env = Env::populated();
    let outside = Env::put(&env.other(), "wav", "keep.wav");
    scan(
        &[env.root(), env.other()],
        &env.lib,
        &CancelToken::new(),
        |_| {},
    )
    .unwrap();
    let outer = env.lib.add_library_root(&env.root()).unwrap();
    let inner_dir = env.root().join("deep");
    std::fs::create_dir_all(&inner_dir).unwrap();
    env.lib.add_library_root(&inner_dir).unwrap();
    let new = env.dir.path().join("moved");
    std::fs::rename(env.root(), &new).unwrap();

    let report = relocate::remap(&env.lib, outer.id, &new).unwrap();
    assert_eq!((report.tracks, report.roots), (3, 2));
    let roots: Vec<String> = env
        .lib
        .library_roots()
        .unwrap()
        .into_iter()
        .map(|r| r.path)
        .collect();
    assert!(roots.contains(&path_str(&new.join("deep"))), "{roots:?}");
    assert!(paths(&env).contains(&path_str(&outside)));
}

#[test]
fn remap_refuses_when_the_files_are_not_there() {
    let env = Env::populated();
    let root = env.lib.add_library_root(&env.root()).unwrap();
    let empty = env.dir.path().join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    let before = paths(&env);
    let err = relocate::remap(&env.lib, root.id, &empty).unwrap_err();
    assert!(matches!(err, Error::RelocateRefused(_)), "{err}");
    assert_eq!(paths(&env), before);
    assert_eq!(
        env.lib.library_roots().unwrap()[0].path,
        path_str(&env.root())
    );
}

#[test]
fn remap_refuses_bad_targets() {
    let env = Env::populated();
    let root = env.lib.add_library_root(&env.root()).unwrap();
    std::fs::create_dir_all(env.other()).unwrap();
    let other_root = env.lib.add_library_root(&env.other()).unwrap();
    for target in [
        env.root(),                      // unchanged
        env.root().join("deeper"),       // inside itself (and not a folder)
        env.dir.path().join("nope"),     // not a folder
        PathBuf::from(&other_root.path), // already a library folder
    ] {
        let err = relocate::remap(&env.lib, root.id, &target).unwrap_err();
        assert!(
            matches!(err, Error::RelocateRefused(_)),
            "{target:?}: {err}"
        );
    }
    assert!(matches!(
        relocate::remap(&env.lib, 999, &env.root()),
        Err(Error::LibraryRootNotFound(999))
    ));
}

#[test]
fn remap_refuses_colliding_paths() {
    let env = Env::populated();
    let root = env.lib.add_library_root(&env.root()).unwrap();
    // Another track already sits where `sine.wav` would land.
    let target = env.dir.path().join("target");
    Env::put(&target, "wav", "sine.wav");
    env.db()
        .execute(
            "INSERT INTO track (path, file_size, file_mtime, kind, date_added, created_at, updated_at)
             VALUES (?1, 1, '', 'wav', '2026-01-01', '', '')",
            [path_str(&target.join("sine.wav"))],
        )
        .unwrap();
    let before = paths(&env);
    let err = relocate::remap(&env.lib, root.id, &target).unwrap_err();
    assert!(matches!(err, Error::RelocateRefused(_)), "{err}");
    assert_eq!(paths(&env), before);
}
