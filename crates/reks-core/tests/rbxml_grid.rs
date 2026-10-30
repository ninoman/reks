//! Edited grids through rekordbox XML (D10, "Grid editing"): every section
//! is a `TEMPO` row that comes back exactly, and a re-import leaves a
//! user-owned grid alone.

#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use proptest::prelude::*;
use reks_core::rbxml::{self, export, ImportOptions, PrefixRemap, Selection};
use reks_core::{
    AnalysisAnchor, AnalysisRecord, CancelToken, GridAnchor, GridEdit, Library, TrackId,
};

const XML_PREFIX: &str = "/Users/reks/Music/reks fixtures";

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn library() -> (tempfile::TempDir, Library) {
    let dir = tempfile::tempdir().unwrap();
    let lib = Library::open(&dir.path().join("reks.sqlite"), &dir.path().join("cache")).unwrap();
    (dir, lib)
}

/// Imports the small fixture collection into `lib`.
fn import_fixture(lib: &Library) -> rbxml::ImportReport {
    let c = rbxml::parse_file(&fixtures().join("rekordbox/collection-small.xml")).unwrap();
    let opts = ImportOptions {
        remap: vec![PrefixRemap {
            from: XML_PREFIX.into(),
            to: std::fs::canonicalize(fixtures().join("audio")).unwrap(),
        }],
        ..ImportOptions::default()
    };
    rbxml::import(&c, lib, opts, |_| {}).unwrap()
}

/// The library's only track in the "Lossless" playlist, and that selection.
fn lossless(lib: &Library) -> (TrackId, Selection) {
    let tree = lib.playlists().unwrap();
    let node = tree.nodes.iter().find(|n| n.name == "Lossless").unwrap().id;
    let sel = Selection {
        playlists: vec![node],
    };
    let id = export(lib, &sel).unwrap().collection.tracks[0].track_id;
    (TrackId::from(id), sel)
}

/// Valid grids: 1–6 sections, 60–200 BPM, gaps of ½–64 beats.
fn valid_grid() -> impl Strategy<Value = Vec<GridAnchor>> {
    (
        0i64..2_000,
        prop::collection::vec((6_000i64..20_000, 1u8..=4, 0.5f64..64.0), 1..6),
    )
        .prop_map(|(start, sections)| {
            let mut out: Vec<GridAnchor> = Vec::new();
            let mut pos = start;
            for (bpm_x100, beat_in_bar, beats) in sections {
                if let Some(prev) = out.last() {
                    pos += (beats * 6_000_000.0 / prev.bpm_x100 as f64).ceil() as i64;
                }
                out.push(GridAnchor {
                    position_ms: pos,
                    bpm_x100,
                    beat_in_bar,
                    beats_per_bar: 4,
                });
            }
            out
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    #[test]
    fn every_section_round_trips_through_xml(anchors in valid_grid()) {
        let (_a, lib) = library();
        import_fixture(&lib);
        let (id, sel) = lossless(&lib);
        lib.edit_beatgrid(id, GridEdit::SetAnchors { anchors: anchors.clone() }).unwrap();

        let out = export(&lib, &sel).unwrap();
        prop_assert_eq!(out.collection.tracks[0].tempos.len(), anchors.len());
        let mut xml = Vec::new();
        rbxml::write(&out.collection, &mut xml).unwrap();
        let parsed = rbxml::parse(xml.as_slice()).unwrap();

        // Into a fresh library: the rows come back as rekordbox's.
        let (other_dir, other) = library();
        rbxml::import(&parsed, &other, ImportOptions::default(), |_| {}).unwrap();
        let g = other.beatgrid(only_track(&other_dir)).unwrap();
        prop_assert_eq!(g.anchors, anchors);
        prop_assert_eq!(g.source.as_deref(), Some("rekordbox"));
    }
}

/// The only track of the library in `dir`.
fn only_track(dir: &tempfile::TempDir) -> TrackId {
    rusqlite::Connection::open(dir.path().join("reks.sqlite"))
        .unwrap()
        .query_row("SELECT id FROM track", [], |r| r.get(0))
        .unwrap()
}

#[test]
fn a_reimport_keeps_a_user_owned_grid() {
    let (_d, lib) = library();
    import_fixture(&lib);
    let (id, _) = lossless(&lib);
    let edited = lib
        .edit_beatgrid(id, GridEdit::AddMarker { at_ms: 15_000 })
        .unwrap();
    let report = import_fixture(&lib);
    assert_eq!(lib.beatgrid(id).unwrap(), edited);
    assert!(report
        .warnings
        .iter()
        .any(|w| w.message.contains("TEMPO rows are left out")));
    // A locked grid too: rekordbox's rows stay as they are.
    lib.edit_beatgrid(id, GridEdit::Reset { section: None })
        .unwrap();
    lib.edit_beatgrid(id, GridEdit::SetLocked { locked: true })
        .unwrap();
    let before = lib.beatgrid(id).unwrap();
    import_fixture(&lib);
    assert_eq!(lib.beatgrid(id).unwrap(), before);
}

/// "Add folder", the queue analyses, then "import collection.xml" (F01):
/// the XML's grid replaces the analysis grid instead of interleaving with it.
#[test]
fn an_import_replaces_an_analysis_grid() {
    let (_d, lib) = library();
    let audio = std::fs::canonicalize(fixtures().join("audio")).unwrap();
    reks_core::scan(&[audio], &lib, &CancelToken::new(), |_| {}).unwrap();
    let ids = lib.analysis_pending(u32::MAX).unwrap();
    assert!(!ids.is_empty());
    let record = AnalysisRecord {
        version: 1,
        content_hash: "h".into(),
        grid: vec![AnalysisAnchor {
            position_ms: 7,
            bpm_x100: 12_799,
            beat_in_bar: 1,
        }],
        bpm_x100: Some(12_799),
        key: None,
        beat_confidence: 1.0,
        key_confidence: 1.0,
    };
    for job in &ids {
        lib.store_analysis(job.track_id, &record).unwrap();
    }

    import_fixture(&lib);
    let (id, _) = lossless(&lib);
    let g = lib.beatgrid(id).unwrap();
    assert_eq!(g.source.as_deref(), Some("rekordbox"));
    assert_eq!(
        g.anchors,
        [GridAnchor {
            position_ms: 0,
            bpm_x100: 12_800,
            beat_in_bar: 1,
            beats_per_bar: 4,
        }]
    );
    // A later analysis run keeps the rekordbox grid (D10).
    lib.store_analysis(id, &record).unwrap();
    assert_eq!(lib.beatgrid(id).unwrap(), g);
}

/// The library's BPM and its source for track `id`.
fn bpm(lib: &Library, id: TrackId) -> (Option<i64>, Option<String>) {
    let t = lib.track(id).unwrap().track;
    (t.row.bpm_x100, t.bpm_source)
}

/// F09: the BPM of an edited grid, or one set by hand, survives a re-import.
#[test]
fn a_reimport_keeps_a_manual_bpm() {
    let (dir, lib) = library();
    import_fixture(&lib);
    let (id, _) = lossless(&lib);
    assert_eq!(bpm(&lib, id), (Some(12_800), Some("rekordbox".into())));
    lib.edit_beatgrid(
        id,
        GridEdit::SetAnchors {
            anchors: vec![GridAnchor {
                position_ms: 0,
                bpm_x100: 6_400,
                beat_in_bar: 1,
                beats_per_bar: 4,
            }],
        },
    )
    .unwrap();
    assert_eq!(bpm(&lib, id), (Some(6_400), Some("manual".into())));
    import_fixture(&lib);
    assert_eq!(bpm(&lib, id), (Some(6_400), Some("manual".into())));

    // A manual BPM beside a rekordbox grid stays too; the grid is replaced.
    lib.edit_beatgrid(id, GridEdit::Reset { section: None })
        .unwrap();
    rusqlite::Connection::open(dir.path().join("reks.sqlite"))
        .unwrap()
        .execute(
            "UPDATE track SET bpm_x100 = 12900, bpm_source = 'manual' WHERE id = ?1",
            [id],
        )
        .unwrap();
    import_fixture(&lib);
    assert_eq!(bpm(&lib, id), (Some(12_900), Some("manual".into())));
    assert_eq!(
        lib.beatgrid(id).unwrap().source.as_deref(),
        Some("rekordbox")
    );
}
