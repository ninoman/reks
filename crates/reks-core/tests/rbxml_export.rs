//! Export of a playlist selection to rekordbox XML (Phase 2 task 6): the
//! file holds only the selection, parses with our parser and with baken's
//! `collection` module, and maps to baken's `Options.playlists`.

#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use reks_core::rbxml::{self, export, export_file, ImportOptions, PrefixRemap, Selection};
use reks_core::{Error, Library, PlaylistId, PlaylistOp, PlaylistOutcome, HOT_CUE_PALETTE};

const XML_PREFIX: &str = "/Users/reks/Music/reks fixtures";

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

struct Env {
    dir: tempfile::TempDir,
    lib: Library,
    audio: PathBuf,
}

impl Env {
    /// A library with the small fixture imported (its audio remapped to
    /// `fixtures/audio`).
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let lib =
            Library::open(&dir.path().join("reks.sqlite"), &dir.path().join("cache")).unwrap();
        let audio = std::fs::canonicalize(fixtures().join("audio")).unwrap();
        let f = std::fs::File::open(fixtures().join("rekordbox/collection-small.xml")).unwrap();
        let c = rbxml::parse(std::io::BufReader::new(f)).unwrap();
        let opts = ImportOptions {
            remap: vec![PrefixRemap {
                from: XML_PREFIX.into(),
                to: audio.clone(),
            }],
            ..ImportOptions::default()
        };
        rbxml::import(&c, &lib, opts, |_| {}).unwrap();
        Env { dir, lib, audio }
    }

    fn node(&self, name: &str) -> PlaylistId {
        let tree = self.lib.playlists().unwrap();
        tree.nodes.iter().find(|n| n.name == name).unwrap().id
    }

    fn create(&self, op: PlaylistOp) -> PlaylistId {
        match self.lib.playlist_mutate(op).unwrap() {
            PlaylistOutcome::Done {
                created_id: Some(id),
                ..
            } => id,
            other => unreachable!("{other:?}"),
        }
    }

    fn sel(&self, names: &[&str]) -> Selection {
        Selection {
            playlists: names.iter().map(|n| self.node(n)).collect(),
        }
    }
}

#[test]
fn one_playlist_exports_only_its_tracks_and_folders() {
    let env = Env::new();
    // An unselected top-level playlist must not appear.
    env.create(PlaylistOp::CreatePlaylist {
        parent_id: None,
        name: "Other".into(),
        position: None,
    });
    let out = export(&env.lib, &env.sel(&["Lossless"])).unwrap();
    let c = &out.collection;

    assert_eq!(c.tracks.len(), 1);
    let t = &c.tracks[0];
    assert_eq!(t.name.as_deref(), Some("Sine Click 128 (FLAC)"));
    assert_eq!(t.kind.as_deref(), Some("FLAC File"));
    assert_eq!(t.rating, Some(153));
    assert_eq!(t.colour, Some(0x00FF00));
    assert_eq!(t.tonality.as_deref(), Some("Am"));
    assert_eq!(t.average_bpm, Some(128.0));
    assert_eq!(t.tempos.len(), 1);
    assert_eq!(t.tempos[0].inizio, 0.0);
    assert_eq!(t.tempos[0].metro.as_deref(), Some("4/4"));
    assert_eq!(t.position_marks.len(), 2);
    assert_eq!(t.position_marks[0].num, -1);
    assert_eq!(t.position_marks[0].start, 3.75);
    assert_eq!(t.position_marks[1].name.as_deref(), Some("Drop"));
    assert_eq!(t.position_marks[1].rgb, Some((230, 40, 40)));

    assert_eq!(
        out.playlists,
        vec![(env.node("Lossless"), "reks fixtures/Lossless".to_owned())]
    );
    assert_eq!(out.baken_playlists(), vec!["reks fixtures/Lossless"]);
    let rbxml::NodeKind::Folder(top) = &c.playlists.kind else {
        panic!("ROOT is a folder")
    };
    assert_eq!(top.len(), 1);
    assert_eq!(top[0].name, "reks fixtures");
}

#[test]
fn temp_file_parses_with_ours_and_with_baken() {
    let env = Env::new();
    let path = env.dir.path().join("collection.xml");
    let out = export_file(&env.lib, &env.sel(&["reks fixtures"]), &path).unwrap();

    // Our parser reads back exactly what we built.
    let f = std::fs::File::open(&path).unwrap();
    let ours = rbxml::parse(std::io::BufReader::new(f)).unwrap();
    assert_eq!(ours, out.collection);
    assert_eq!(ours.tracks.len(), 3);

    // baken reads the same tracks, playlists and paths.
    let lib = baken_export::collection::Library::load(&path).unwrap();
    assert_eq!(lib.tracks.len(), 3);
    for path in out.baken_playlists() {
        assert!(lib.playlist(&path).is_some(), "{path}");
    }
    let all = lib.playlist("reks fixtures/All fixtures").unwrap();
    assert_eq!(all.key_type, "0");
    let ids: Vec<u64> = out
        .collection
        .tracks
        .iter()
        .map(|t| t.track_id.into())
        .collect();
    assert_eq!(all.track_ids, ids);
    assert_eq!(
        lib.playlist("reks fixtures/Lossless").unwrap().track_ids,
        vec![ids[1]]
    );

    let mp3 = lib.track(ids[0]).unwrap();
    let on_disk = env.audio.join("sine-click-128.mp3");
    assert_eq!(mp3.location, on_disk.to_str().unwrap());
    assert_eq!(mp3.total_time, 30);
    assert_eq!(mp3.tempos.len(), 1);
    assert_eq!(mp3.tempos[0].bpm, 128.0);
    assert_eq!(mp3.cues.len(), 5);
    let hot: Vec<_> = mp3.cues.iter().filter(|c| c.is_hot()).collect();
    assert_eq!(hot.len(), 3);
    assert!(mp3
        .cues
        .iter()
        .any(|c| c.is_loop() && c.end == Some(16.875)));
    assert_eq!(mp3.tonality, "Am");
    assert_eq!(mp3.stars(), 5);
}

#[test]
fn ambiguous_paths_fall_back_to_every_playlist() {
    let env = Env::new();
    let folder = env.node("reks fixtures");
    // A second "Lossless" next to the first, and a top-level name that reads
    // like the folder path.
    let dup = env.create(PlaylistOp::CreatePlaylist {
        parent_id: Some(folder),
        name: "Lossless".into(),
        position: None,
    });
    let slash = env.create(PlaylistOp::CreatePlaylist {
        parent_id: None,
        name: "reks fixtures/All fixtures".into(),
        position: None,
    });
    let unique = export(&env.lib, &env.sel(&["All fixtures"])).unwrap();
    assert_eq!(unique.baken_playlists().len(), 1);

    // Only one of the two "Lossless" selected: still unique in the file.
    let one = Selection {
        playlists: vec![dup],
    };
    assert_eq!(export(&env.lib, &one).unwrap().baken_playlists().len(), 1);

    let all = env.node("All fixtures");
    for (sel, n) in [(vec![folder], 3), (vec![all, slash], 2)] {
        let out = export(&env.lib, &Selection { playlists: sel }).unwrap();
        assert_eq!(out.playlists.len(), n);
        assert!(out.baken_playlists().is_empty());
    }
}

#[test]
fn bad_selections_are_errors() {
    let env = Env::new();
    assert!(matches!(
        export(
            &env.lib,
            &Selection {
                playlists: vec![9999]
            }
        ),
        Err(Error::PlaylistNotFound(9999))
    ));
    assert!(matches!(
        export(&env.lib, &Selection::default()),
        Err(Error::EmptySelection)
    ));
    let empty = env.create(PlaylistOp::CreateFolder {
        parent_id: None,
        name: "Empty".into(),
        position: None,
    });
    assert!(matches!(
        export(
            &env.lib,
            &Selection {
                playlists: vec![empty]
            }
        ),
        Err(Error::EmptySelection)
    ));
    // An empty playlist is a valid selection.
    let p = env.create(PlaylistOp::CreatePlaylist {
        parent_id: Some(empty),
        name: "Nothing yet".into(),
        position: None,
    });
    let out = export(&env.lib, &Selection { playlists: vec![p] }).unwrap();
    assert!(out.collection.tracks.is_empty());
    assert_eq!(out.baken_playlists(), vec!["Empty/Nothing yet"]);
}

/// A smart playlist is written as a plain playlist of the tracks it matches
/// now, in its own sort (D29).
#[test]
fn smart_playlist_exports_as_a_plain_playlist() {
    let env = Env::new();
    let folder = env.node("reks fixtures");
    let mut ids: Vec<i64> = env
        .lib
        .tracks(&reks_core::TrackQuery::default())
        .unwrap()
        .rows
        .iter()
        .map(|r| r.id)
        .collect();
    ids.sort_unstable();
    let smart = env.create(PlaylistOp::CreateSmart {
        parent_id: Some(folder),
        name: "Smart".into(),
        rule: reks_core::RuleNode::Group {
            r#match: reks_core::Match::All,
            not: false,
            rules: vec![],
        },
        sort: Some(reks_core::Sort {
            column: reks_core::SortColumn::Title,
            descending: true,
        }),
        position: None,
    });
    let out = export(
        &env.lib,
        &Selection {
            playlists: vec![smart],
        },
    )
    .unwrap();
    let want = env.lib.playlist_track_ids(&[smart]).unwrap()[&smart].clone();
    let mut sorted = want.clone();
    sorted.sort_unstable();
    assert_eq!(sorted, ids, "every track: an empty \"all\" group");
    let titles: Vec<String> = want
        .iter()
        .map(|&id| {
            env.lib
                .track(id)
                .unwrap()
                .track
                .row
                .title
                .unwrap_or_default()
        })
        .collect();
    let mut by_title = titles.clone();
    by_title.sort_unstable_by_key(|t| std::cmp::Reverse(t.to_lowercase()));
    assert_eq!(titles, by_title, "title, descending");

    let rbxml::NodeKind::Folder(top) = &out.collection.playlists.kind else {
        panic!("ROOT is a folder")
    };
    let rbxml::NodeKind::Folder(inner) = &top[0].kind else {
        panic!("the parent folder")
    };
    let rbxml::NodeKind::Playlist { keys, .. } = &inner[0].kind else {
        panic!("a plain playlist")
    };
    assert_eq!(inner[0].name, "Smart");
    let want_keys: Vec<String> = want.iter().map(i64::to_string).collect();
    assert_eq!(keys, &want_keys);
    assert_eq!(out.collection.tracks.len(), ids.len());
}

/// Phase 5 task 17: cues set, renamed and deleted from a deck are what the
/// XML (and through it the USB export) carries, and a re-import of the
/// rekordbox XML leaves the manual ones alone.
#[test]
fn cues_edited_on_a_deck_reach_the_export() {
    let env = Env::new();
    let lossless = env.node("Lossless");
    let id = env.lib.playlist_track_ids(&[lossless]).unwrap()[&lossless][0];
    let before = env.lib.track(id).unwrap().cues;
    let drop = before
        .iter()
        .find(|c| c.name.as_deref() == Some("Drop"))
        .unwrap();
    let free = (0..8)
        .find(|s| before.iter().all(|c| c.slot != Some(*s)))
        .unwrap();

    let hot = env.lib.set_hot_cue(id, free, 1_500.0).unwrap();
    env.lib.rename_cue(id, hot.id, Some(" Break ")).unwrap();
    env.lib.add_memory_cue(id, 9_000.0).unwrap();
    env.lib.delete_cue(id, drop.id).unwrap();

    let out = export(&env.lib, &env.sel(&["Lossless"])).unwrap();
    let marks = &out.collection.tracks[0].position_marks;
    let [r, g, b] = [16, 8, 0].map(|s| (HOT_CUE_PALETTE[0] >> s) as u8);
    let new_hot = marks.iter().find(|m| m.num == i32::from(free)).unwrap();
    assert_eq!(
        (
            new_hot.name.as_deref(),
            new_hot.start,
            new_hot.kind,
            new_hot.rgb
        ),
        (Some("Break"), 1.5, 0, Some((r, g, b)))
    );
    assert!(marks.iter().any(|m| m.num == -1 && m.start == 9.0));
    assert!(marks.iter().all(|m| m.name.as_deref() != Some("Drop")));
}
