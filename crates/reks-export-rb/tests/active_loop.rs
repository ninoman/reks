//! Phase 5 task 17: a memory loop marked active in the library reaches baken
//! as its active loop only with the `export.active_loops` flag (the temp
//! XML's `[active]` name marker; baken writes `PCPT` status 4, which its own
//! tests match against rekordbox bytes). Without the flag, or in an XML for
//! rekordbox, it is a plain memory loop.

#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use reks_core::rbxml::{self, ActiveLoops, ImportOptions, PrefixRemap, Selection};
use reks_core::Library;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

/// The small fixture imported, with two active memory loops on the
/// `Lossless` playlist's track; the selection of that playlist.
fn library(dir: &tempfile::TempDir) -> (Library, Selection) {
    let lib = Library::open(&dir.path().join("reks.sqlite"), &dir.path().join("cache")).unwrap();
    let f = std::fs::File::open(fixtures().join("rekordbox/collection-small.xml")).unwrap();
    let c = rbxml::parse(std::io::BufReader::new(f)).unwrap();
    let opts = ImportOptions {
        remap: vec![PrefixRemap {
            from: "/Users/reks/Music/reks fixtures".into(),
            to: std::fs::canonicalize(fixtures().join("audio")).unwrap(),
        }],
        ..ImportOptions::default()
    };
    rbxml::import(&c, &lib, opts, |_| {}).unwrap();
    let tree = lib.playlists().unwrap();
    let lossless = tree.nodes.iter().find(|n| n.name == "Lossless").unwrap().id;
    let id = lib.playlist_track_ids(&[lossless]).unwrap()[&lossless][0];
    for (start, end) in [(6_000.0, 7_000.0), (2_000.0, 2_500.0)] {
        let lp = lib.add_memory_cue(id, start, Some(end)).unwrap();
        lib.set_active_loop(id, lp.id, true).unwrap();
    }
    lib.rename_cue(
        id,
        lib.track(id)
            .unwrap()
            .cues
            .iter()
            .find(|c| c.start_ms == 2_000.0)
            .unwrap()
            .id,
        Some("Break"),
    )
    .unwrap();
    (
        lib,
        Selection {
            playlists: vec![lossless],
        },
    )
}

/// The collection as baken parses our XML.
fn as_baken_reads(
    lib: &Library,
    sel: &Selection,
    mode: ActiveLoops,
) -> baken_export::collection::Library {
    let out = rbxml::export_with_dividers(lib, sel, &[], mode).unwrap();
    let mut bytes = Vec::new();
    rbxml::write(&out.collection, &mut bytes).unwrap();
    baken_export::collection::Library::parse(&bytes).unwrap()
}

#[test]
fn our_marker_is_bakens() {
    assert_eq!(
        rbxml::BAKEN_ACTIVE_LOOP_MARKER,
        baken_export::collection::ACTIVE_LOOP_MARKER
    );
}

#[test]
fn the_earliest_active_memory_loop_reaches_baken_only_with_the_marker() {
    let dir = tempfile::tempdir().unwrap();
    let (lib, sel) = library(&dir);

    let marked = as_baken_reads(&lib, &sel, ActiveLoops::BakenMarker);
    let cues = &marked.tracks[0].cues;
    let active = baken_export::collection::active_loop(cues).unwrap();
    assert_eq!(
        (active.start, active.end, active.name.as_str()),
        (2.0, Some(2.5), "Break")
    );
    assert_eq!(cues.iter().filter(|c| c.marked).count(), 1, "one per track");

    let plain = as_baken_reads(&lib, &sel, ActiveLoops::Leave);
    assert!(baken_export::collection::active_loop(&plain.tracks[0].cues).is_none());
    assert!(plain.tracks[0].cues.iter().any(|c| c.name == "Break"));
}
