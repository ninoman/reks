//! Stick profiles against a real library and temp-dir sticks: identity,
//! profile → selection → plan, and a real baken sync recorded and compared,
//! read from the stick and from the record. The pure status rules have their
//! property tests in `status.rs`.

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use reks_core::rbxml::{self, ImportOptions, PrefixRemap};
use reks_core::{CancelToken, DuplicateMode, Library, ManifestItemKind, ManifestRow, PlaylistOp};

use super::*;
use crate::apply::{apply, ApplyEnv, StickCheck};
use crate::device::FsKind;
use crate::plan::{self, ExportOptions, MissingAnalysis, NoAnlz};
use crate::testutil;
use crate::writer::BakenWriter;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/audio");
const XML_PREFIX: &str = "/Users/reks/Music/reks profiles";
const SERIAL: &str = "4993-1CFB";

/// (fixture, file name, title)
const TRACKS: [(&str, &str, &str); 3] = [
    ("sine-click-128.mp3", "one.mp3", "One"),
    ("sine-click-128.flac", "two.flac", "Two"),
    ("sine-click-128.wav", "three.wav", "Three"),
];

fn info(stick: &Path, label: &str, total_bytes: u64) -> DeviceInfo {
    DeviceInfo {
        mount_point: stick.to_path_buf(),
        fs: FsKind::Fat32,
        fs_name: "msdos".into(),
        removable: true,
        total_bytes,
        free_bytes: total_bytes / 2,
        label: label.into(),
        os_name: "disk9s1".into(),
    }
}

/// A library with `Set/Peak` (One, Two), `Set/Chill` (Three, Two) and `Solo`
/// (One), and an empty stick.
struct World {
    tmp: tempfile::TempDir,
    lib: Library,
    stick: PathBuf,
}

impl World {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let music = tmp.path().join("music");
        let stick = tmp.path().join("stick");
        for d in [&music, &stick] {
            fs::create_dir_all(d).unwrap();
        }
        for (fixture, name, _) in TRACKS {
            fs::copy(Path::new(FIXTURES).join(fixture), music.join(name)).unwrap();
        }
        let lib =
            Library::open(&tmp.path().join("reks.sqlite"), &tmp.path().join("cache")).unwrap();
        let c = rbxml::parse(Cursor::new(collection_xml())).unwrap();
        let opts = ImportOptions {
            remap: vec![PrefixRemap {
                from: XML_PREFIX.into(),
                to: music,
            }],
            ..ImportOptions::default()
        };
        rbxml::import(&c, &lib, opts, |_| {}).unwrap();
        World { tmp, lib, stick }
    }

    fn info(&self) -> DeviceInfo {
        info(&self.stick, "REKS", 32 << 30)
    }

    fn playlist(&self, name: &str) -> PlaylistId {
        let tree = self.lib.playlists().unwrap();
        tree.nodes.iter().find(|n| n.name == name).unwrap().id
    }

    fn entries(&self, name: &str) -> Vec<TrackId> {
        let id = self.playlist(name);
        self.lib.playlist_entry_sizes(&[id]).unwrap()[&id]
            .iter()
            .map(|(t, _)| *t)
            .collect()
    }

    fn size(&self, track: TrackId) -> u64 {
        self.lib.track(track).unwrap().track.file_size as u64
    }

    fn device(&self) -> Device {
        create(
            &self.lib,
            &self.info(),
            Some(SERIAL),
            "Main",
            TargetProfile::Cdj3000,
        )
        .unwrap()
    }

    fn plan(&self, sel: &Selection) -> ExportPlan {
        let opts = ExportOptions {
            missing_analysis: MissingAnalysis::Generate,
            ..ExportOptions::default()
        };
        let sources = plan::load(&self.lib, sel).unwrap();
        plan::plan_sources(
            sources,
            &self.info(),
            Some(SERIAL.into()),
            &opts,
            &NoAnlz,
            &NoAnlz,
        )
        .unwrap()
    }

    /// A Sync: the profile's plan written by baken through `apply`.
    fn sync(&self, dev: DeviceId) -> (ExportPlan, ExportReport) {
        let sel = selection(&self.lib, dev).unwrap();
        let plan = self.plan(&sel);
        assert!(!plan.is_blocked(), "{:?}", plan.errors);
        let work = self.tmp.path().join("work");
        let _ = fs::remove_dir_all(&work);
        let writer = BakenWriter::prepare(&self.lib, &sel, &plan, &work).unwrap();
        let env = ApplyEnv {
            library: &self.lib,
            app_data_dir: &self.tmp.path().join("data"),
            stick: StickCheck::Directory,
        };
        let report = apply(&plan, &writer, &env, &|_| {}, &CancelToken::new()).unwrap();
        assert_eq!(report.status, ExportStatus::Success, "{report:#?}");
        (plan, report)
    }

    fn status(&self, dev: DeviceId, mounted: bool) -> ProfileStatus {
        profile_status(&self.lib, dev, mounted.then_some(self.stick.as_path())).unwrap()
    }
}

fn collection_xml() -> String {
    let prefix = XML_PREFIX.replace(' ', "%20");
    let mut tracks = String::new();
    for (i, (_, name, title)) in TRACKS.iter().enumerate() {
        tracks += &format!(
            r#"<TRACK TrackID="{id}" Name="{title}" Artist="Artist" Album="Album" Kind="" TotalTime="30" AverageBpm="128.00" Location="file://localhost{prefix}/{name}">
  <TEMPO Inizio="0.000" Bpm="128.00" Metro="4/4" Battito="1"/>
</TRACK>
"#,
            id = i + 1,
        );
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<DJ_PLAYLISTS Version="1.0.0">
<PRODUCT Name="rekordbox" Version="7.0.0" Company="AlphaTheta"/>
<COLLECTION Entries="3">
{tracks}</COLLECTION>
<PLAYLISTS>
<NODE Type="0" Name="ROOT" Count="2">
  <NODE Type="0" Name="Set" Count="2">
    <NODE Name="Peak" Type="1" KeyType="0" Entries="2"><TRACK Key="1"/><TRACK Key="2"/></NODE>
    <NODE Name="Chill" Type="1" KeyType="0" Entries="2"><TRACK Key="3"/><TRACK Key="2"/></NODE>
  </NODE>
  <NODE Name="Solo" Type="1" KeyType="0" Entries="1"><TRACK Key="1"/></NODE>
</NODE>
</PLAYLISTS>
</DJ_PLAYLISTS>
"#
    )
}

fn paths(s: &ProfileStatus) -> Vec<(String, &SyncState)> {
    s.playlists
        .iter()
        .map(|p| (p.path_text.clone(), &p.state))
        .collect()
}

fn changed(added: u32, removed: u32) -> SyncState {
    SyncState::Changed {
        added,
        removed,
        reordered: false,
        moved_from: None,
    }
}

// ---- identity -------------------------------------------------------------

#[test]
fn serial_hit_needs_no_prompt() {
    let w = World::new();
    let new = identify_serial(&w.info(), Some(SERIAL), &w.lib).unwrap();
    assert!(
        matches!(&new, DeviceMatch::New { volume_serial: Some(s), suggested_name, .. }
            if s == SERIAL && suggested_name == "REKS"),
        "{new:?}"
    );
    let dev = w.device();
    // Label and size don't matter once the ID is known.
    let renamed = info(&w.stick, "OTHER", 8 << 30);
    assert_eq!(
        identify_serial(&renamed, Some(SERIAL), &w.lib).unwrap(),
        DeviceMatch::Known { device: dev }
    );
}

#[test]
fn unknown_serial_with_known_label_and_size_prompts_and_relink_replaces_the_id() {
    let w = World::new();
    let dev = w.device();
    let m = identify_serial(&w.info(), Some("ABCD-0001"), &w.lib).unwrap();
    let DeviceMatch::Ambiguous {
        volume_serial,
        reason,
        candidates,
        prompt,
        ..
    } = m
    else {
        panic!("{m:?}")
    };
    assert_eq!(volume_serial.as_deref(), Some("ABCD-0001"));
    assert_eq!(reason, AmbiguousReason::SerialChanged);
    assert_eq!(candidates, std::slice::from_ref(&dev));
    assert_eq!(prompt, "Is this 'Main'?");

    // "Yes": the stored ID is replaced; the stick is known from now on.
    let linked = link(&w.lib, dev.id, &w.info(), volume_serial.as_deref()).unwrap();
    assert_eq!(linked.volume_serial.as_deref(), Some("ABCD-0001"));
    assert!(matches!(
        identify_serial(&w.info(), Some("ABCD-0001"), &w.lib).unwrap(),
        DeviceMatch::Known { device } if device.id == dev.id
    ));
    // The old ID is gone (one column, no aliases: D28).
    assert!(matches!(
        identify_serial(&w.info(), Some(SERIAL), &w.lib).unwrap(),
        DeviceMatch::Ambiguous { .. }
    ));

    // Another stick with the same label and size: the prompt lists both.
    create(
        &w.lib,
        &w.info(),
        Some("EEEE-0002"),
        "Backup",
        TargetProfile::Cdj3000,
    )
    .unwrap();
    let m = identify_serial(&w.info(), Some("FFFF-0003"), &w.lib).unwrap();
    assert!(
        matches!(&m, DeviceMatch::Ambiguous { candidates, prompt, .. }
            if candidates.len() == 2 && prompt == "Which of your sticks is this?"),
        "{m:?}"
    );
    // Linking an ID another stick owns is refused.
    assert!(link(&w.lib, dev.id, &w.info(), Some("EEEE-0002")).is_err());
}

#[test]
fn no_serial_falls_back_to_label_and_size_and_asks_every_time() {
    let w = World::new();
    let dev = w.device();
    for _ in 0..2 {
        let m = identify_serial(&w.info(), None, &w.lib).unwrap();
        let DeviceMatch::Ambiguous {
            volume_serial,
            reason,
            candidates,
            ..
        } = m
        else {
            panic!("{m:?}")
        };
        assert_eq!(volume_serial, None);
        assert_eq!(reason, AmbiguousReason::NoSerial);
        assert_eq!(candidates[0].id, dev.id);
        // Confirming keeps the stored ID.
        let linked = link(&w.lib, dev.id, &w.info(), None).unwrap();
        assert_eq!(linked.volume_serial.as_deref(), Some(SERIAL));
    }
    // A different size is a different stick.
    assert!(matches!(
        identify_serial(&info(&w.stick, "REKS", 16 << 30), None, &w.lib).unwrap(),
        DeviceMatch::New { .. }
    ));
}

#[test]
fn a_library_reks_did_not_write_is_foreign() {
    let w = World::new();
    let pdb = w.stick.join("PIONEER/rekordbox");
    fs::create_dir_all(&pdb).unwrap();
    fs::write(pdb.join("export.pdb"), testutil::unplayed_pdb()).unwrap();
    let blank = info(&w.stick, "", 32 << 30);
    let m = identify_serial(&blank, Some(SERIAL), &w.lib).unwrap();
    assert!(
        matches!(&m, DeviceMatch::Foreign { suggested_name, .. } if suggested_name == "USB stick"),
        "{m:?}"
    );
    assert!(matches!(
        identify_serial(&blank, None, &w.lib).unwrap(),
        DeviceMatch::Foreign { .. }
    ));

    // A stick reks exported to before profiles existed has a manifest.
    w.lib
        .replace_manifest(
            Some(SERIAL),
            &[ManifestRow {
                stick_path: "/Contents/a.mp3".into(),
                item_kind: ManifestItemKind::Track,
                track_id: None,
            }],
        )
        .unwrap();
    assert!(matches!(
        identify_serial(&blank, Some(SERIAL), &w.lib).unwrap(),
        DeviceMatch::New { .. }
    ));
}

#[test]
fn rename_and_target() {
    let w = World::new();
    let dev = w.device();
    assert_eq!(target(&dev), TargetProfile::Cdj3000);
    assert_eq!(rename(&w.lib, dev.id, "Gig").unwrap().name, "Gig");
    let dev = set_target(&w.lib, dev.id, TargetProfile::Cdj2000Nxs2).unwrap();
    assert_eq!(dev.target, "Cdj2000Nxs2");
    assert_eq!(target(&dev), TargetProfile::Cdj2000Nxs2);
    let mut odd = dev;
    odd.target = "Cdj9000".into();
    assert_eq!(target(&odd), TargetProfile::Generic);
}

// ---- profile → selection → plan -------------------------------------------

#[test]
fn profile_selection_plans_like_the_hand_built_selection() {
    let w = World::new();
    let dev = w.device();
    let (set, solo, peak) = (w.playlist("Set"), w.playlist("Solo"), w.playlist("Peak"));
    set_selection(&w.lib, dev.id, &[solo, set, peak]).unwrap();

    let sel = selection(&w.lib, dev.id).unwrap();
    assert_eq!(sel.playlists, [set, peak, solo]); // library tree order
    let hand = Selection {
        playlists: vec![solo, peak, set],
    };
    let from_profile = w.plan(&sel);
    assert!(!from_profile.is_blocked(), "{:?}", from_profile.errors);
    assert_eq!(from_profile, w.plan(&hand));

    // Empty profile: an empty selection, which the planner refuses as usual.
    set_selection(&w.lib, dev.id, &[]).unwrap();
    assert!(selection(&w.lib, dev.id).unwrap().playlists.is_empty());
}

// ---- sync, record, status -------------------------------------------------

#[test]
fn never_synced_profile_is_all_new() {
    let w = World::new();
    let dev = w.device();
    set_selection(&w.lib, dev.id, &[w.playlist("Set")]).unwrap();
    let s = w.status(dev.id, false);
    assert_eq!(s.source, StatusSource::NeverSynced);
    assert_eq!(s.source_text, "Not synced yet");
    assert_eq!(
        paths(&s),
        [
            ("Set / Peak".to_owned(), &SyncState::New),
            ("Set / Chill".to_owned(), &SyncState::New)
        ]
    );
    assert_eq!(s.summary, "2 new");
    let all: u64 = w
        .entries("Peak")
        .iter()
        .chain(&w.entries("Chill"))
        .collect::<HashSet<_>>()
        .into_iter()
        .map(|t| w.size(*t))
        .sum();
    assert_eq!(s.planned_bytes, all);
    assert_eq!(s.entries.len(), 1);
    assert_eq!((s.entries[0].playlists, s.entries[0].tracks), (2, 3));
    assert_eq!(s.entries[0].frees_bytes, all);
    assert_eq!(s.capacity_bytes, 32 << 30);
    assert!(!s.over_capacity);

    // Mounted but empty: read from the stick, still all new.
    let s = w.status(dev.id, true);
    assert_eq!(s.source, StatusSource::Stick);
    assert_eq!(s.notes, ["The stick holds no rekordbox library yet."]);
    assert_eq!(s.summary, "2 new");
}

#[test]
fn synced_profile_tracks_library_edits() {
    let w = World::new();
    let dev = w.device();
    let (set, solo) = (w.playlist("Set"), w.playlist("Solo"));
    set_selection(&w.lib, dev.id, &[set]).unwrap();

    let (plan, report) = w.sync(dev.id);
    assert_eq!(record_sync(&w.lib, dev.id, &plan, &report).unwrap(), 2);
    let recorded = w.lib.device_synced(dev.id).unwrap();
    assert_eq!(recorded[0].playlist.path, ["Set", "Peak"]);
    assert_eq!(recorded[0].playlist.playlist_id, Some(w.playlist("Peak")));
    assert_eq!(
        recorded[1].playlist.entries,
        w.entries("Chill").into_iter().map(Some).collect::<Vec<_>>()
    );

    for mounted in [false, true] {
        let s = w.status(dev.id, mounted);
        assert_eq!(
            s.source,
            if mounted {
                StatusSource::Stick
            } else {
                StatusSource::LastSync
            }
        );
        assert_eq!(s.summary, "In sync", "mounted {mounted}: {s:#?}");
        assert!(s.notes.is_empty(), "{:?}", s.notes);
    }

    // Library edits: a track added to Chill, Solo ticked.
    let one = w.entries("Solo")[0];
    w.lib
        .playlist_mutate(PlaylistOp::AddTracks {
            playlist_id: w.playlist("Chill"),
            track_ids: vec![one],
            before: None,
            duplicates: DuplicateMode::Allow,
        })
        .unwrap();
    set_selection(&w.lib, dev.id, &[set, solo]).unwrap();
    for mounted in [false, true] {
        let s = w.status(dev.id, mounted);
        assert_eq!(
            paths(&s),
            [
                ("Set / Peak".to_owned(), &SyncState::InSync),
                ("Set / Chill".to_owned(), &changed(1, 0)),
                ("Solo".to_owned(), &SyncState::New),
            ],
            "mounted {mounted}"
        );
        assert_eq!(s.playlists[1].text, "1 track added");
        assert_eq!(s.summary, "1 new, 1 changed");
        // Solo's only track is in Set too: unticking Solo frees nothing;
        // unticking Set frees Two and Three (One stays for Solo).
        let two_three: u64 = w
            .entries("Chill")
            .iter()
            .filter(|t| **t != one)
            .collect::<HashSet<_>>()
            .into_iter()
            .map(|t| w.size(*t))
            .sum();
        assert_eq!(s.entries[0].frees_bytes, two_three);
        assert_eq!(s.entries[1].frees_bytes, 0);
        assert!(s.entries[1]
            .frees_text
            .starts_with("Unticking frees nothing"));
    }

    // Peak deleted from the library: dropped from the profile, removed from
    // the stick at the next sync, never silently.
    w.lib
        .playlist_mutate(PlaylistOp::Delete {
            id: w.playlist("Peak"),
        })
        .unwrap();
    for mounted in [false, true] {
        let s = w.status(dev.id, mounted);
        let peak = s.playlists.last().unwrap();
        assert_eq!(peak.path_text, "Set / Peak", "mounted {mounted}");
        assert_eq!(peak.state, SyncState::WillRemove);
        assert_eq!(peak.playlist_id, None);
        assert_eq!(peak.tracks, 2);
        assert_eq!(s.summary, "1 new, 1 changed, 1 to remove");
    }

    // Sync again: in sync, Peak gone from the record.
    let (plan, report) = w.sync(dev.id);
    assert_eq!(record_sync(&w.lib, dev.id, &plan, &report).unwrap(), 2);
    for mounted in [false, true] {
        let s = w.status(dev.id, mounted);
        assert_eq!(s.summary, "In sync", "mounted {mounted}: {s:#?}");
    }
}

#[test]
fn record_sync_refuses_what_it_cannot_trust() {
    let w = World::new();
    let dev = w.device();
    set_selection(&w.lib, dev.id, &[w.playlist("Solo")]).unwrap();
    let (plan, report) = w.sync(dev.id);

    let mut cancelled = report.clone();
    cancelled.status = ExportStatus::Cancelled;
    assert!(matches!(
        record_sync(&w.lib, dev.id, &plan, &cancelled),
        Err(DevicesError::NotWritten)
    ));
    let mut other = report.clone();
    other.volume_serial = Some("0000-0000".into());
    assert!(matches!(
        record_sync(&w.lib, dev.id, &plan, &other),
        Err(DevicesError::WrongStick { .. })
    ));
    let mut unread = report.clone();
    unread.validation = None;
    assert!(matches!(
        record_sync(&w.lib, dev.id, &plan, &unread),
        Err(DevicesError::NotReadBack)
    ));
    assert!(w.lib.device_synced(dev.id).unwrap().is_empty());
    assert_eq!(w.lib.device(dev.id).unwrap().last_synced_at, None);

    // Problems still record: the read-back is what the stick holds.
    let mut problems = report;
    problems.status = ExportStatus::Problems;
    assert_eq!(record_sync(&w.lib, dev.id, &plan, &problems).unwrap(), 1);
}

#[test]
fn mounted_stick_without_a_volume_id_uses_the_record() {
    let w = World::new();
    let dev = create(&w.lib, &w.info(), None, "No ID", TargetProfile::Cdj3000).unwrap();
    set_selection(&w.lib, dev.id, &[w.playlist("Solo")]).unwrap();
    let s = w.status(dev.id, true);
    assert_eq!(s.source, StatusSource::NeverSynced);
    assert_eq!(s.notes.len(), 1);
    assert_eq!(s.summary, "1 new");
}

#[test]
fn status_json_shape() {
    let w = World::new();
    let dev = w.device();
    set_selection(&w.lib, dev.id, &[w.playlist("Solo")]).unwrap();
    let v = serde_json::to_value(w.status(dev.id, false)).unwrap();
    assert_eq!(v["source"], "neverSynced");
    assert_eq!(v["playlists"][0]["state"]["state"], "new");
    assert_eq!(v["playlists"][0]["pathText"], "Solo");
    assert_eq!(v["entries"][0]["isFolder"], false);
    assert_eq!(v["device"]["volumeSerial"], SERIAL);
    let m = serde_json::to_value(identify_serial(&w.info(), None, &w.lib).unwrap()).unwrap();
    assert_eq!(m["type"], "ambiguous");
    assert_eq!(m["reason"], "noSerial");
}
