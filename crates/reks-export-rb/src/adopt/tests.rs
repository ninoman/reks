//! Adopt on a synthetic foreign stick (stick-profiles §8): golden plan,
//! stick bytes unchanged, Link only on an exact ordered match.
//!
//! The stick (made with `testutil`, as rekordbox would lay it out):
//! tracks 1 Alpha.mp3, 2 Beta.flac, 3 Gamma.m4a (all in the library) and
//! 4 Unknown.wav (not); folder `Sets` holding `Warmup` [1, 2, 3] and
//! `Peak` [2, 1]; top-level `Odd` [3, 4]; one history session [1, 2].
//! The library has `Warmup` [A, B, G] (exact: Link offered) and `Peak`
//! [A, B] (wrong order: not offered).

use std::fs;
use std::path::Path;

use reks_core::history::SessionAction;
use reks_core::{CancelToken, Library, NewDevice, PlaylistOp, PlaylistOutcome, TrackQuery};

use super::*;
use crate::history::tests::{snapshot, stick};
use crate::testutil::*;

const PDB: &str = "PIONEER/rekordbox/export.pdb";
const SERIAL: &str = "4993-1CFB";
const GOLDEN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/adopt/golden_plan.json");

fn stick_pdb() -> Vec<u8> {
    let mut tables = vec![
        (
            T_TRACKS,
            vec![vec![
                track_row(1, 1, 30, "Alpha", "/Contents/Artist A/x/Alpha.mp3"),
                track_row(2, 1, 30, "Beta", "/Contents/Artist A/y/Beta.flac"),
                track_row(3, 2, 30, "Gamma", "/Contents/Artist B/z/Gamma.m4a"),
                track_row(4, 2, 200, "Unknown", "/Contents/Artist B/q/Unknown.wav"),
            ]],
        ),
        (
            T_ARTISTS,
            vec![vec![artist_row(1, "Artist A"), artist_row(2, "Artist B")]],
        ),
        (
            T_PLAYLIST_TREE,
            vec![vec![
                playlist_node(10, 0, 0, true, "Sets"),
                playlist_node(11, 10, 1, false, "Peak"),
                playlist_node(12, 10, 0, false, "Warmup"),
                playlist_node(13, 0, 1, false, "Odd"),
            ]],
        ),
        (
            T_PLAYLIST_ENTRIES,
            vec![vec![
                playlist_entry(1, 2, 11),
                playlist_entry(2, 1, 11),
                playlist_entry(1, 1, 12),
                playlist_entry(2, 2, 12),
                playlist_entry(3, 3, 12),
                playlist_entry(1, 3, 13),
                playlist_entry(2, 4, 13),
            ]],
        ),
        (
            T_HISTORY_PLAYLISTS,
            vec![vec![history_playlist(1, "HISTORY 001")]],
        ),
        (
            T_HISTORY_ENTRIES,
            vec![vec![history_entry(1, 1, 1), history_entry(2, 1, 2)]],
        ),
    ];
    // Every other table present and empty, as on a real stick.
    for t in 0..crate::validate::TABLE_COUNT {
        if !tables.iter().any(|(ty, _)| *ty == t) {
            tables.push((t, vec![vec![]]));
        }
    }
    synthetic_pdb(&tables)
}

struct Env {
    stick: tempfile::TempDir,
    _music: tempfile::TempDir,
    lib: Library,
    /// Alpha, Beta, Gamma.
    tracks: [TrackId; 3],
    warmup: PlaylistId,
    peak: PlaylistId,
    device: DeviceId,
}

fn create_playlist(lib: &Library, name: &str, tracks: &[TrackId]) -> PlaylistId {
    let PlaylistOutcome::Done {
        created_id: Some(id),
        ..
    } = lib
        .playlist_mutate(PlaylistOp::CreatePlaylist {
            parent_id: None,
            name: name.into(),
            position: None,
        })
        .unwrap()
    else {
        panic!("not created")
    };
    lib.playlist_mutate(PlaylistOp::AddTracks {
        playlist_id: id,
        track_ids: tracks.to_vec(),
        before: None,
        duplicates: Default::default(),
    })
    .unwrap();
    id
}

fn env() -> Env {
    let stick = stick(&[(PDB, &stick_pdb())]);
    let music = tempfile::tempdir().unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/audio");
    for (fixture, rel) in [
        ("sine-click-128.mp3", "Alpha.mp3"),
        ("sine-click-128.flac", "Beta.flac"),
        ("sine-click-128.m4a", "Gamma.m4a"),
    ] {
        fs::copy(fixtures.join(fixture), music.path().join(rel)).unwrap();
    }
    let lib = Library::open_in_memory(&music.path().join("cache")).unwrap();
    reks_core::scan(
        &[music.path().to_path_buf()],
        &lib,
        &CancelToken::new(),
        |_| {},
    )
    .unwrap();
    let rows = lib.tracks(&TrackQuery::default()).unwrap().rows;
    let id_of = |kind: &str| rows.iter().find(|r| r.kind == kind).unwrap().id;
    let tracks = [id_of("mp3"), id_of("flac"), id_of("aac")];
    let warmup = create_playlist(&lib, "Warmup", &tracks);
    let peak = create_playlist(&lib, "Peak", &tracks[..2]);
    create_playlist(&lib, "Odd", &tracks[2..]);
    let device = lib
        .create_device(&NewDevice {
            volume_serial: Some(SERIAL.into()),
            label: "REKORDBOX".into(),
            size_bytes: 32 << 30,
            name: "Gig stick".into(),
            target: "Generic".into(),
        })
        .unwrap()
        .id;
    Env {
        stick,
        _music: music,
        lib,
        tracks,
        warmup,
        peak,
        device,
    }
}

impl Env {
    fn scan(&self) -> AdoptPlan {
        scan(self.stick.path(), Some(SERIAL), &self.lib).unwrap()
    }

    fn choices(&self, playlists: &[(u32, AdoptChoice)]) -> AdoptChoices {
        AdoptChoices {
            device_id: self.device,
            playlists: playlists.iter().copied().collect(),
            history: Default::default(),
        }
    }

    fn path_of(&self, id: PlaylistId) -> String {
        library_paths(&self.lib.playlists().unwrap().nodes)[&id].clone()
    }
}

fn playlist(plan: &AdoptPlan, pdb_id: u32) -> &AdoptPlaylist {
    plan.playlists.iter().find(|p| p.pdb_id == pdb_id).unwrap()
}

/// The plan as JSON, with the one field that depends on the day the test
/// runs (the library's baseline date, set by the migration) blanked.
fn plan_json(plan: &AdoptPlan) -> serde_json::Value {
    let mut v = serde_json::to_value(plan).unwrap();
    v["history"]["baselineDate"] = serde_json::Value::Null;
    v
}

/// Run with `REKS_BLESS=1` to rewrite the golden file after a deliberate change.
#[test]
fn golden_plan() {
    let e = env();
    let got = plan_json(&e.scan());
    if std::env::var_os("REKS_BLESS").is_some() {
        fs::write(GOLDEN, serde_json::to_string_pretty(&got).unwrap() + "\n").unwrap();
    }
    let want: serde_json::Value = serde_json::from_slice(&fs::read(GOLDEN).unwrap()).unwrap();
    assert_eq!(got, want, "run with REKS_BLESS=1 if the change is intended");
}

#[test]
fn plan_reads_tree_matches_and_history() {
    let e = env();
    let plan = e.scan();
    let paths: Vec<&str> = plan
        .playlists
        .iter()
        .map(|p| p.path_text.as_str())
        .collect();
    assert_eq!(paths, ["Sets / Warmup", "Sets / Peak", "Odd"]);
    let odd = playlist(&plan, 13);
    assert_eq!((odd.matched, odd.unmatched), (1, 1));
    assert_eq!(plan.unmatched.len(), 1);
    assert_eq!(plan.unmatched[0].title.as_deref(), Some("Unknown"));
    let h = plan.history.as_ref().unwrap();
    assert_eq!(h.sessions.len(), 1);
    assert_eq!(h.sessions[0].action, SessionAction::New);
    assert_eq!(plan.history_sessions, 1);
    assert!(!plan.written_by_reks);
    assert_eq!(
        plan.summary,
        "3 playlists, 7 track entries: 6 in your library, 1 not"
    );
}

#[test]
fn link_only_on_exact_ordered_match() {
    let e = env();
    let plan = e.scan();
    let warmup = playlist(&plan, 12);
    assert_eq!(
        warmup.link,
        vec![LinkCandidate {
            playlist_id: e.warmup,
            path_text: "Warmup".into()
        }]
    );
    // Same name and tracks, other order.
    assert!(playlist(&plan, 11).link.is_empty());
    // Same name and the matched tracks, but one stick track is unmatched.
    assert!(playlist(&plan, 13).link.is_empty());
    assert!(plan
        .playlists
        .iter()
        .all(|p| p.default_choice == AdoptChoice::Create));

    let refused = apply(
        &plan,
        &e.choices(&[(
            11,
            AdoptChoice::Link {
                playlist_id: e.peak,
            },
        )]),
        &e.lib,
    );
    assert!(matches!(
        refused,
        Err(AdoptError::LinkNotOffered { pdb_id: 11, .. })
    ));
    assert_eq!(
        e.lib.playlists().unwrap().nodes.len(),
        3,
        "refused apply wrote"
    );
    let h = e.scan().history.unwrap();
    assert_eq!(
        h.sessions[0].action,
        SessionAction::New,
        "refused apply imported"
    );
}

#[test]
fn apply_imports_history_first_then_creates_links_and_ticks() {
    let e = env();
    let before = snapshot(e.stick.path());
    let plan = e.scan();
    let report = apply(
        &plan,
        &e.choices(&[
            (
                12,
                AdoptChoice::Link {
                    playlist_id: e.warmup,
                },
            ),
            (13, AdoptChoice::Skip),
        ]),
        &e.lib,
    )
    .unwrap();
    assert_eq!(report.history.as_ref().unwrap().created.len(), 1);
    assert_eq!(report.linked, vec![e.warmup]);
    assert_eq!(report.skipped, 1);
    assert_eq!(report.created.len(), 1);
    let peak = report.created[0];
    assert_eq!(e.path_of(peak), "From stick / Gig stick / Sets / Peak");
    let [a, b, _] = e.tracks;
    assert_eq!(
        e.lib.playlist_track_ids(&[peak]).unwrap()[&peak],
        vec![b, a]
    );
    let mut ticked = e.lib.device_selection(e.device).unwrap();
    ticked.sort_unstable();
    let mut want = vec![e.warmup, peak];
    want.sort_unstable();
    assert_eq!(ticked, want);
    assert!(report.unmatched.is_empty(), "the skipped playlist held it");
    assert_eq!(
        report.summary,
        "1 playlist created, 1 linked, 1 skipped; 1 play history session imported."
    );

    // The stick is never written; a second scan finds the history imported.
    assert_eq!(snapshot(e.stick.path()), before, "adopt touched the stick");
    let again = e.scan();
    assert!(matches!(
        again.history.unwrap().sessions[0].action,
        SessionAction::Skip(_)
    ));
    assert_eq!(again.history_sessions, 0);
}

#[test]
fn default_choices_create_everything_and_list_unmatched() {
    let e = env();
    let before = snapshot(e.stick.path());
    let plan = e.scan();
    let report = apply(&plan, &e.choices(&[]), &e.lib).unwrap();
    let paths: Vec<String> = report.created.iter().map(|&id| e.path_of(id)).collect();
    assert_eq!(
        paths,
        [
            "From stick / Gig stick / Sets / Warmup",
            "From stick / Gig stick / Sets / Peak",
            "From stick / Gig stick / Odd",
        ]
    );
    let odd = report.created[2];
    assert_eq!(
        e.lib.playlist_track_ids(&[odd]).unwrap()[&odd],
        vec![e.tracks[2]]
    );
    assert_eq!(report.unmatched.len(), 1);
    assert_eq!(snapshot(e.stick.path()), before, "adopt touched the stick");
}

#[test]
fn refuses_another_stick_and_unknown_choices() {
    let e = env();
    let mut plan = e.scan();
    assert!(matches!(
        apply(&plan, &e.choices(&[(99, AdoptChoice::Skip)]), &e.lib),
        Err(AdoptError::UnknownPlaylist { pdb_id: 99 })
    ));
    plan.volume_serial = Some("0000-0000".into());
    assert!(matches!(
        apply(&plan, &e.choices(&[]), &e.lib),
        Err(AdoptError::WrongStick { .. })
    ));
    assert_eq!(e.lib.playlists().unwrap().nodes.len(), 3);
}

#[test]
fn no_library_is_an_error() {
    let lib = Library::open_in_memory(Path::new("cache-unused")).unwrap();
    let empty = tempfile::tempdir().unwrap();
    assert!(matches!(
        scan(empty.path(), None, &lib),
        Err(AdoptError::NoLibrary { .. })
    ));
    let garbage = stick(&[(PDB, b"not a database")]);
    assert!(matches!(
        scan(garbage.path(), None, &lib),
        Err(AdoptError::Unreadable { .. })
    ));
}

/// Golden test on a rekordbox-written reference stick (stick-profiles §8,
/// open item S4): `fixtures/rekordbox/ref-<ver>/` with the stick's
/// `PIONEER/` folder and the expected `adopt-plan.json` (scanned against an
/// empty library, no volume ID). Set `REKS_REF_FIXTURE` to that folder and
/// run with `--ignored`; make it a plain test once the fixture is in.
#[test]
#[ignore = "needs fixtures/rekordbox/ref-<ver>/ exported by rekordbox 7 (S4)"]
fn reference_stick_fixture_golden_plan() {
    let root = std::path::PathBuf::from(std::env::var("REKS_REF_FIXTURE").unwrap());
    let before = snapshot(&root);
    let lib = Library::open_in_memory(Path::new("cache-unused")).unwrap();
    let plan = scan(&root, None, &lib).unwrap();
    assert!(plan.notes.is_empty(), "{:?}", plan.notes);
    let want: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("adopt-plan.json")).unwrap()).unwrap();
    assert_eq!(plan_json(&plan), want);
    assert_eq!(snapshot(&root), before);
}
