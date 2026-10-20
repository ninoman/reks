//! Divider export (Phase 2.5 task 4) end to end on a temp-dir stick: a
//! playlist of 30 tracks and 3 dividers planned with `export.dividers`,
//! written by baken through `apply`, read back with rekordcrate (the
//! validator's reader), then exported again unchanged and with a divider
//! removed.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use reks_core::rbxml::{self, ImportOptions, PrefixRemap};
use reks_core::{fsname, CancelToken, Library, ManifestItemKind, PlaylistId, PlaylistOp};
use reks_export_rb::apply::{apply, ApplyEnv, ExportReport, ExportStatus, StickCheck};
use reks_export_rb::device::{DeviceInfo, FsKind};
use reks_export_rb::divider::{self, DIVIDER_DIR};
use reks_export_rb::plan::{
    self, anlz_dir, AnlzSource, ExportOptions, ExportPlan, MissingAnalysis, NoAnlz, OpReason,
    Selection,
};
use reks_export_rb::validate;
use reks_export_rb::writer::BakenWriter;

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/audio/sine-click-128.mp3"
);
const XML_PREFIX: &str = "/Users/reks/Music/reks dividers";
const SERIAL: &str = "4993-1CFB";
const TRACKS: usize = 30;
const LABELS: [&str; 3] = ["---- OPEN ----", "---- PEAK ----", "---- CLOSE ----"];

struct World {
    tmp: tempfile::TempDir,
    lib: Library,
    stick: PathBuf,
    playlist: PlaylistId,
}

impl World {
    /// Playlist `Gig` with tracks `T01`..`T30` and dividers before T01, before
    /// T11 and after T30.
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let music = tmp.path().join("music");
        let stick = tmp.path().join("stick");
        fs::create_dir_all(&music).unwrap();
        fs::create_dir_all(&stick).unwrap();
        for i in 1..=TRACKS {
            fs::copy(FIXTURE, music.join(format!("t{i:02}.mp3"))).unwrap();
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
        let playlist = lib
            .playlists()
            .unwrap()
            .nodes
            .iter()
            .find(|n| n.name == "Gig")
            .unwrap()
            .id;
        let w = World {
            tmp,
            lib,
            stick,
            playlist,
        };
        w.insert(Some(0), LABELS[0]);
        w.insert(Some(11), LABELS[1]); // before T11: 10 tracks and a divider above it
        w.insert(None, LABELS[2]);
        w
    }

    fn insert(&self, position: Option<u32>, label: &str) {
        self.lib
            .playlist_mutate(PlaylistOp::InsertDivider {
                playlist_id: self.playlist,
                position,
                label: label.into(),
            })
            .unwrap();
    }

    fn sel(&self) -> Selection {
        Selection {
            playlists: vec![self.playlist],
        }
    }

    fn dev(&self) -> DeviceInfo {
        DeviceInfo {
            mount_point: self.stick.clone(),
            fs: FsKind::Fat32,
            fs_name: "msdos".into(),
            removable: true,
            total_bytes: 32 << 30,
            free_bytes: 16 << 30,
            label: "REKS".into(),
            os_name: "disk9s1".into(),
        }
    }

    /// `plan::plan` with a known serial (a temp dir has none).
    fn plan(&self, dividers: bool) -> ExportPlan {
        let opts = ExportOptions {
            missing_analysis: MissingAnalysis::Generate,
            dividers,
            ..ExportOptions::default()
        };
        let mut sources = plan::load(&self.lib, &self.sel()).unwrap();
        if dividers {
            divider::expand(&mut sources, &self.lib.cache_dir().join(DIVIDER_DIR)).unwrap();
        }
        let p = plan::plan_sources(
            sources,
            &self.dev(),
            Some(SERIAL.into()),
            &opts,
            &NoAnlz,
            &NoAnlz,
        )
        .unwrap();
        assert!(!p.is_blocked(), "{:?}", p.errors);
        p
    }

    fn apply(&self, plan: &ExportPlan) -> ExportReport {
        let work = self.tmp.path().join("work");
        let _ = fs::remove_dir_all(&work);
        let writer = BakenWriter::prepare(&self.lib, &self.sel(), plan, &work).unwrap();
        let data = self.tmp.path().join("data");
        let env = ApplyEnv {
            library: &self.lib,
            app_data_dir: &data,
            stick: StickCheck::Directory,
            onelibrary: None,
        };
        let report = apply(plan, &writer, &env, &|_| {}, &CancelToken::new()).unwrap();
        assert_eq!(
            report.status,
            ExportStatus::Success,
            "{:?} {:?}",
            report.findings(),
            report.errors
        );
        report
    }

    /// Titles of `Gig`'s entries on the stick, in order (rekordcrate).
    fn stick_entries(&self) -> Vec<String> {
        let back = validate::read(&self.stick).unwrap();
        assert!(back.findings.is_empty(), "{:?}", back.findings);
        let titles: HashMap<u32, &str> = back
            .tracks
            .iter()
            .map(|t| (t.pdb_id, t.title.as_str()))
            .collect();
        let gig = back.playlists.iter().find(|p| p.name == "Gig").unwrap();
        gig.entries.iter().map(|id| titles[id].to_owned()).collect()
    }

    fn stick_files(&self) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        walk(&self.stick, &self.stick.join("Contents"), &mut out);
        walk(&self.stick, &self.stick.join("PIONEER/USBANLZ"), &mut out);
        out
    }
}

fn walk(root: &Path, dir: &Path, out: &mut BTreeSet<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for e in entries {
        let p = e.unwrap().path();
        if p.is_dir() {
            walk(root, &p, out);
        } else {
            let rel = p.strip_prefix(root).unwrap().to_string_lossy();
            out.insert(format!("/{}", rel.replace('\\', "/")));
        }
    }
}

fn collection_xml() -> String {
    let prefix = XML_PREFIX.replace(' ', "%20");
    let mut tracks = String::new();
    let mut keys = String::new();
    for i in 1..=TRACKS {
        tracks += &format!(
            r#"<TRACK TrackID="{i}" Name="T{i:02}" Artist="Artist {i:02}" Album="Album" Kind="MP3 File" TotalTime="30" Location="file://localhost{prefix}/t{i:02}.mp3"/>
"#
        );
        keys += &format!(r#"<TRACK Key="{i}"/>"#);
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<DJ_PLAYLISTS Version="1.0.0">
<PRODUCT Name="rekordbox" Version="7.0.0" Company="AlphaTheta"/>
<COLLECTION Entries="{TRACKS}">
{tracks}</COLLECTION>
<PLAYLISTS>
<NODE Type="0" Name="ROOT" Count="1">
  <NODE Name="Gig" Type="1" KeyType="0" Entries="{TRACKS}">{keys}</NODE>
</NODE>
</PLAYLISTS>
</DJ_PLAYLISTS>
"#
    )
}

/// The expected entry titles with `labels` at positions 0, 11 and the end.
fn expected(labels: &[Option<&str>; 3]) -> Vec<String> {
    let mut out: Vec<String> = (1..=TRACKS).map(|i| format!("T{i:02}")).collect();
    if let Some(l) = labels[2] {
        out.push(l.to_owned());
    }
    if let Some(l) = labels[1] {
        out.insert(10, l.to_owned());
    }
    if let Some(l) = labels[0] {
        out.insert(0, l.to_owned());
    }
    out
}

fn divider_paths(plan: &ExportPlan) -> Vec<String> {
    plan.tracks
        .iter()
        .filter(|t| t.is_divider())
        .map(|t| t.dst_rel.clone())
        .collect()
}

#[test]
fn dividers_export_reexport_and_removal() {
    let w = World::new();

    // Off (the default): dividers are left out.
    let off = w.plan(false);
    assert_eq!(off.tracks.len(), TRACKS);
    assert!(off.tracks.iter().all(|t| !t.is_divider()));

    // On: 30 tracks and 3 dividers, 33 entries in order.
    let first = w.plan(true);
    assert_eq!(first.tracks.len(), TRACKS + 3);
    let dividers: Vec<_> = first.tracks.iter().filter(|t| t.is_divider()).collect();
    assert_eq!(dividers.len(), 3);
    for (d, label) in dividers.iter().zip(LABELS) {
        let info = d.divider.as_ref().unwrap();
        assert_eq!(info.label, label);
        assert_eq!(d.anlz_source, AnlzSource::Generated);
        assert_eq!(d.kind, reks_audio::Kind::Wav);
        assert!(d.src.starts_with(w.lib.cache_dir().join(DIVIDER_DIR)));
        assert_eq!(
            d.dst_rel,
            fsname::contents_path("", "", &divider::file_name(w.playlist, info.ordinal))
        );
    }
    assert_eq!(first.playlists[0].track_ids.len(), TRACKS + 3);

    let report = w.apply(&first);
    let validation = report.validation.as_ref().unwrap();
    assert!(validation.findings.is_empty());
    assert_eq!(
        w.stick_entries(),
        expected(&[Some(LABELS[0]), Some(LABELS[1]), Some(LABELS[2])])
    );

    // 3 WAVs on the stick, each with its ANLZ.
    for dst in divider_paths(&first) {
        let wav = w.stick.join(dst.trim_start_matches('/'));
        assert_eq!(fs::read(&wav).unwrap(), divider::wav_bytes(), "{dst}");
        let anlz = w.stick.join(anlz_dir(&dst).trim_start_matches('/'));
        assert!(anlz.join("ANLZ0000.DAT").is_file(), "{dst}");
        assert!(anlz.join("ANLZ0000.EXT").is_file(), "{dst}");
    }

    // The manifest marks them as dividers, without a library track.
    let manifest = w.lib.manifest(SERIAL).unwrap();
    assert_eq!(manifest.len(), TRACKS + 3);
    let marked: BTreeSet<String> = manifest
        .iter()
        .filter(|m| m.item_kind == ManifestItemKind::Divider)
        .map(|m| {
            assert_eq!(m.track_id, None);
            m.stick_path.clone()
        })
        .collect();
    assert_eq!(marked, divider_paths(&first).into_iter().collect());

    // A second identical export copies nothing.
    let files = w.stick_files();
    let again = w.plan(true);
    assert!(again.copies.is_empty(), "{:?}", again.copies);
    assert!(again.deletes.is_empty(), "{:?}", again.deletes);
    let report = w.apply(&again);
    let wr = report.writer.as_ref().unwrap();
    assert_eq!((wr.audio_copied, wr.audio_kept), (0, TRACKS + 3));
    assert_eq!(w.stick_files(), files);

    // Remove the middle divider: exactly one WAV and its ANLZ go. Dividers
    // are numbered per playlist, so the last file is the one freed.
    let middle = 11; // entry position of "---- PEAK ----"
    w.lib
        .playlist_mutate(PlaylistOp::RemoveEntries {
            playlist_id: w.playlist,
            positions: vec![middle],
        })
        .unwrap();
    let fewer = w.plan(true);
    assert!(fewer.copies.is_empty(), "{:?}", fewer.copies);
    let freed = fsname::contents_path("", "", &divider::file_name(w.playlist, 3));
    let anlz = anlz_dir(&freed);
    let planned: BTreeSet<String> = fewer.deletes.iter().map(|d| d.dst_rel.clone()).collect();
    assert!(fewer.deletes.iter().all(|d| d.reason == OpReason::Stale));
    let mut want: BTreeSet<String> = files
        .iter()
        .filter(|f| f.starts_with(&format!("{anlz}/")))
        .cloned()
        .collect();
    assert!(want.len() >= 2, "{want:?}");
    want.insert(freed.clone());
    assert_eq!(planned, want);

    w.apply(&fewer);
    let after = w.stick_files();
    let removed: BTreeSet<String> = files.difference(&after).cloned().collect();
    assert_eq!(removed, want);
    assert!(after.difference(&files).next().is_none());
    assert_eq!(
        w.stick_entries(),
        expected(&[Some(LABELS[0]), None, Some(LABELS[2])])
    );
}

#[test]
fn temp_xml_rows_for_dividers() {
    let w = World::new();
    let plan = w.plan(true);
    let work = w.tmp.path().join("xml");
    let writer = BakenWriter::prepare(&w.lib, &w.sel(), &plan, &work).unwrap();
    let c = rbxml::parse_file(writer.xml()).unwrap();
    assert_eq!(c.tracks.len(), TRACKS + 3);
    for d in plan.tracks.iter().filter(|t| t.is_divider()) {
        let row = c
            .tracks
            .iter()
            .find(|t| i64::from(t.track_id) == d.track_id)
            .unwrap();
        assert_eq!(
            row.name.as_deref(),
            Some(d.divider.as_ref().unwrap().label.as_str())
        );
        assert_eq!(row.artist, None);
        assert_eq!(row.album, None);
        assert_eq!(row.genre, None);
        assert_eq!(row.tonality, None);
        assert_eq!(row.average_bpm, None);
        assert!(row.tempos.is_empty() && row.position_marks.is_empty());
        assert_eq!(row.kind.as_deref(), Some("WAV File"));
        assert_eq!(row.total_time, Some(divider::DIVIDER_SECONDS));
    }
    let keys = match &c.playlists.kind {
        rbxml::NodeKind::Folder(children) => match &children[0].kind {
            rbxml::NodeKind::Playlist { keys, .. } => keys.clone(),
            _ => panic!("not a playlist"),
        },
        _ => panic!("not a folder"),
    };
    let ids: Vec<String> = plan.playlists[0]
        .track_ids
        .iter()
        .map(|id| id.to_string())
        .collect();
    assert_eq!(keys, ids);
}

#[test]
fn plan_writes_divider_files_into_the_cache_dir() {
    let w = World::new();
    let opts = ExportOptions {
        missing_analysis: MissingAnalysis::Generate,
        dividers: true,
        ..ExportOptions::default()
    };
    let p = plan::plan(&w.lib, &w.sel(), &w.dev(), &opts, &NoAnlz).unwrap();
    assert_eq!(p.tracks.iter().filter(|t| t.is_divider()).count(), 3);
    let dir = w.lib.cache_dir().join(DIVIDER_DIR);
    let mut names: Vec<String> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    let want: Vec<String> = (1..=3).map(|n| divider::file_name(w.playlist, n)).collect();
    assert_eq!(names, want);
}
