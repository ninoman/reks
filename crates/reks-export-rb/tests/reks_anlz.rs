//! reks-built ANLZ in the legacy export (Phase 3 task 10), end to end on a
//! temp-dir stick: which source the planner picks under each
//! `AnlzPreference`, then an export through `apply` whose ANLZ comes from
//! reks's analysis cache. Every file on the stick passes
//! `reks_anlz::validate_set` and the read-back validator; files without
//! point cues also parse with rekordcrate (0.3.0 can't read point cues, D8).
//! The cache entries are synthetic (known columns), not real analysis: what
//! is tested is the export path, not the analyser.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use binrw::BinRead;
use reks_analysis::beats::BeatList;
use reks_analysis::key::KeyAnalysis;
use reks_analysis::waveform::{WaveformColumns, MONO_MAX};
use reks_analysis::{cache, TrackAnalysis};
use reks_anlz::grid::pqtz_beats_from_anchors;
use reks_anlz::{read, Expect};
use reks_core::analysis::file_content_hash;
use reks_core::rbxml::{self, ImportOptions, PrefixRemap};
use reks_core::{AnlzPreference, CancelToken, GridEdit, Library, TrackId};
use reks_export_rb::apply::{apply, ApplyEnv, ExportStatus, StickCheck};
use reks_export_rb::device::{DeviceInfo, FsKind};
use reks_export_rb::dryrun::DryRun;
use reks_export_rb::onelibrary::NoWriter;
use reks_export_rb::plan::{
    self, anlz_dir, AnlzOp, AnlzSource, ExportOptions, ExportPlan, NoAnlz, ReksCache, Selection,
    SourceTrack, Warning,
};
use reks_export_rb::reksanlz;
use reks_export_rb::writer::{BakenWriter, WriterError};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/audio");
const XML_PREFIX: &str = "/Users/reks/Music/reks anlz";
const SERIAL: &str = "4993-1CFB";

/// (fixture, file name, XML children). `a`: a grid, no cues; `b`: a grid,
/// a hot cue and a memory loop; `c`: neither; `d`: never analysed by reks.
const TRACKS: [(&str, &str, &str); 4] = [
    (
        "sine-click-128.mp3",
        "a.mp3",
        r#"<TEMPO Inizio="0.000" Bpm="128.00" Metro="4/4" Battito="1"/>"#,
    ),
    (
        "sine-click-128.flac",
        "b.flac",
        r#"<TEMPO Inizio="0.025" Bpm="128.00" Metro="4/4" Battito="3"/>
  <POSITION_MARK Name="Drop" Type="0" Start="7.500" Num="0" Red="230" Green="40" Blue="40"/>
  <POSITION_MARK Name="" Type="4" Start="1.000" End="2.875" Num="-1"/>"#,
    ),
    ("sine-click-128.wav", "c.wav", ""),
    ("sine-click-128.aiff", "d.aiff", ""),
];

struct World {
    tmp: tempfile::TempDir,
    lib: Library,
    stick: PathBuf,
    sel: Selection,
    /// Library track id by file name.
    ids: HashMap<String, TrackId>,
}

impl World {
    /// The four tracks in playlist `Set`; reks analysis cached for a, b, c.
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let music = tmp.path().join("music");
        let stick = tmp.path().join("stick");
        fs::create_dir_all(&music).unwrap();
        fs::create_dir_all(&stick).unwrap();
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
        let set = lib
            .playlists()
            .unwrap()
            .nodes
            .iter()
            .find(|n| n.name == "Set")
            .unwrap()
            .id;
        let sel = Selection {
            playlists: vec![set],
        };
        let ids = plan::load(&lib, &sel)
            .unwrap()
            .tracks
            .iter()
            .map(|t| (file_name(&t.src), t.track_id))
            .collect();
        let w = World {
            tmp,
            lib,
            stick,
            sel,
            ids,
        };
        for name in ["a.mp3", "b.flac", "c.wav"] {
            w.cache_analysis(name);
        }
        w
    }

    /// A synthetic cache entry for `name`, current for its file.
    fn cache_analysis(&self, name: &str) {
        let src = self.src(name);
        let duration_ms = reks_audio::probe(&src).unwrap().duration_ms;
        let n = usize::try_from((duration_ms * 150).div_ceil(1000)).unwrap();
        let ramp = |k: usize| (0..n).map(|i| ((i * k) % 256) as u8).collect::<Vec<_>>();
        let analysis = TrackAnalysis {
            duration_ms,
            beats: BeatList {
                sample_rate: 44_100,
                beats: Vec::new(),
                confidence: 0.0,
            },
            grid: None,
            key: KeyAnalysis {
                key: None,
                confidence: 0.0,
                similarity: 0.0,
                chroma: [0.0; 12],
            },
            columns: WaveformColumns {
                low: ramp(3),
                mid: ramp(5),
                high: ramp(7),
                mono: (0..n)
                    .map(|i| (i % (usize::from(MONO_MAX) + 1)) as u8)
                    .collect(),
            },
        };
        let hash = file_content_hash(&src).unwrap();
        cache::write(&self.lib.cache_dir(), self.ids[name], &analysis, &hash).unwrap();
    }

    fn src(&self, name: &str) -> PathBuf {
        self.tmp.path().join("music").join(name)
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

    /// Plans with rekordbox's cache holding only `a.mp3` (a stand-in file).
    fn plan(&self, pref: AnlzPreference) -> ExportPlan {
        let fake_dat = self.tmp.path().join("rb/ANLZ0000.DAT");
        fs::create_dir_all(fake_dat.parent().unwrap()).unwrap();
        fs::write(&fake_dat, b"rekordbox").unwrap();
        let a = self.ids["a.mp3"];
        let rekordbox = move |t: &SourceTrack| (t.track_id == a).then(|| fake_dat.clone());
        self.plan_with(pref, &rekordbox)
    }

    fn plan_with(&self, pref: AnlzPreference, rekordbox: &dyn plan::AnlzLocator) -> ExportPlan {
        let opts = ExportOptions {
            anlz_preference: pref,
            onelibrary: false,
            ..ExportOptions::default()
        };
        let sources = plan::load(&self.lib, &self.sel).unwrap();
        let reks = ReksCache::new(self.lib.cache_dir());
        let mut p = plan::plan_sources(
            sources,
            &self.dev(),
            Some(SERIAL.into()),
            &opts,
            rekordbox,
            &reks,
        )
        .unwrap();
        plan::plan_onelibrary(&mut p, &NoWriter, None, &self.lib).unwrap();
        p
    }

    fn sources(&self, plan: &ExportPlan) -> HashMap<String, AnlzSource> {
        plan.tracks
            .iter()
            .map(|t| (file_name(&t.src), t.anlz_source.clone()))
            .collect()
    }
}

fn file_name(p: &Path) -> String {
    p.file_name().unwrap().to_string_lossy().into_owned()
}

fn collection_xml() -> String {
    let prefix = XML_PREFIX.replace(' ', "%20");
    let mut tracks = String::new();
    let mut keys = String::new();
    for (i, (_, name, children)) in TRACKS.iter().enumerate() {
        let id = i + 1;
        tracks += &format!(
            r#"<TRACK TrackID="{id}" Name="{name}" Artist="Artist" Album="Album" Kind="" TotalTime="30" Location="file://localhost{prefix}/{name}">
  {children}
</TRACK>
"#
        );
        keys += &format!(r#"<TRACK Key="{id}"/>"#);
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<DJ_PLAYLISTS Version="1.0.0">
<PRODUCT Name="rekordbox" Version="7.0.0" Company="AlphaTheta"/>
<COLLECTION Entries="4">
{tracks}</COLLECTION>
<PLAYLISTS>
<NODE Type="0" Name="ROOT" Count="1">
  <NODE Name="Set" Type="1" KeyType="0" Entries="4">{keys}</NODE>
</NODE>
</PLAYLISTS>
</DJ_PLAYLISTS>
"#
    )
}

#[test]
fn preference_picks_the_source() {
    let w = World::new();
    let dat = w.tmp.path().join("rb/ANLZ0000.DAT");
    let rb = AnlzSource::RekordboxCache { dat };

    let p = w.plan(AnlzPreference::RekordboxFirst);
    let s = w.sources(&p);
    assert_eq!(s["a.mp3"], rb);
    assert_eq!(s["b.flac"], AnlzSource::Reks);
    assert_eq!(s["c.wav"], AnlzSource::Reks);
    assert!(!s.contains_key("d.aiff"));
    // d has neither: skipped (the default), and the reason names both.
    let reason = p
        .warnings
        .iter()
        .find_map(|w| match w {
            Warning::Skipped { reason, .. } => Some(reason.clone()),
            _ => None,
        })
        .unwrap();
    assert!(reason.contains("reks has not analysed it"), "{reason}");

    let s = w.sources(&w.plan(AnlzPreference::ReksFirst));
    assert_eq!(s["a.mp3"], AnlzSource::Reks);
    assert_eq!(s["b.flac"], AnlzSource::Reks);

    let s = w.sources(&w.plan(AnlzPreference::RekordboxOnly));
    assert_eq!(s["a.mp3"], rb);
    assert_eq!(s.len(), 1);

    // Built ANLZ is counted in the bytes to write (estimate).
    let p = w.plan(AnlzPreference::ReksFirst);
    let built: u64 = p
        .anlz
        .iter()
        .map(|a| match a {
            AnlzOp::Build { bytes, .. } => *bytes,
            _ => 0,
        })
        .sum();
    assert!(built > 0);
    let audio: u64 = p.copies.iter().map(|c| c.bytes).sum();
    assert_eq!(p.bytes_to_write, audio + built);
}

#[test]
fn edited_grid_prefers_reks_over_rekordbox() {
    let w = World::new();
    let dat = w.tmp.path().join("rb/ANLZ0000.DAT");
    let rb = AnlzSource::RekordboxCache { dat };
    let a = w.ids["a.mp3"];
    assert_eq!(
        w.sources(&w.plan(AnlzPreference::RekordboxFirst))["a.mp3"],
        rb
    );

    // A grid-editor edit on the rekordbox-analysed track: rekordbox's cached
    // PQTZ would lose it, so the default takes reks's set (the library grid).
    w.lib
        .edit_beatgrid(
            a,
            GridEdit::Nudge {
                delta_ms: 10,
                section: None,
            },
        )
        .unwrap();
    assert_eq!(
        w.sources(&w.plan(AnlzPreference::RekordboxFirst))["a.mp3"],
        AnlzSource::Reks
    );
    assert_eq!(
        w.sources(&w.plan(AnlzPreference::ReksFirst))["a.mp3"],
        AnlzSource::Reks
    );
    // `rekordboxOnly` never writes reks's analysis.
    assert_eq!(
        w.sources(&w.plan(AnlzPreference::RekordboxOnly))["a.mp3"],
        rb
    );

    // Without reks's analysis rekordbox's set still beats skipping the track.
    cache::remove(&w.lib.cache_dir(), a).unwrap();
    assert_eq!(
        w.sources(&w.plan(AnlzPreference::RekordboxFirst))["a.mp3"],
        rb
    );
    // With rekordbox's whole set, its waveforms get the library's grid.
    let AnlzSource::RekordboxCache { dat } = &rb else {
        unreachable!()
    };
    for ext in ["EXT", "2EX"] {
        fs::write(dat.with_extension(ext), b"stand-in").unwrap();
    }
    for pref in [AnlzPreference::RekordboxFirst, AnlzPreference::ReksFirst] {
        assert_eq!(
            w.sources(&w.plan(pref))["a.mp3"],
            AnlzSource::ReksGrid { dat: dat.clone() }
        );
    }
    assert_eq!(
        w.sources(&w.plan(AnlzPreference::RekordboxOnly))["a.mp3"],
        rb
    );
}

#[test]
fn locked_grid_prefers_reks_over_rekordbox() {
    let w = World::new();
    w.lib
        .edit_beatgrid(w.ids["a.mp3"], GridEdit::SetLocked { locked: true })
        .unwrap();
    assert_eq!(
        w.sources(&w.plan(AnlzPreference::RekordboxFirst))["a.mp3"],
        AnlzSource::Reks
    );
}

#[test]
fn analysis_of_a_changed_file_is_not_used() {
    let w = World::new();
    // The file changes after analysis: its content hash no longer matches.
    let mut bytes = fs::read(w.src("c.wav")).unwrap();
    bytes.extend_from_slice(&[0; 64]);
    fs::write(w.src("c.wav"), &bytes).unwrap();
    let s = w.sources(&w.plan(AnlzPreference::ReksFirst));
    assert!(!s.contains_key("c.wav"));
    assert_eq!(s["b.flac"], AnlzSource::Reks);
}

#[test]
fn export_with_reks_anlz_validates() {
    let w = World::new();
    let plan = w.plan_with(AnlzPreference::ReksFirst, &NoAnlz);
    assert!(!plan.is_blocked(), "{:?}", plan.errors);
    assert_eq!(plan.tracks.len(), 3);
    assert!(plan
        .tracks
        .iter()
        .all(|t| t.anlz_source == AnlzSource::Reks));
    let dry = DryRun::build(&w.lib, &plan);
    let analysis = dry.summary.iter().find(|f| f.label == "Analysis").unwrap();
    assert_eq!(
        analysis.value,
        "0 from rekordbox (0 B), 3 from reks's analysis (UNVERIFIED on players)"
    );

    let work = w.tmp.path().join("work");
    let writer = BakenWriter::prepare(&w.lib, &w.sel, &plan, &work).unwrap();
    let env = ApplyEnv {
        library: &w.lib,
        app_data_dir: &w.tmp.path().join("data"),
        stick: StickCheck::Directory,
        onelibrary: None,
    };
    let report = apply(&plan, &writer, &env, &|_| {}, &CancelToken::new()).unwrap();
    // The read-back validator ran `validate_set` on every reks-built set.
    assert_eq!(
        report.status,
        ExportStatus::Success,
        "{:?} {:?}",
        report.findings(),
        report.errors
    );

    let estimate: HashMap<TrackId, u64> = plan
        .anlz
        .iter()
        .filter_map(|a| match a {
            AnlzOp::Build { track_id, bytes } => Some((*track_id, *bytes)),
            _ => None,
        })
        .collect();
    for t in &plan.tracks {
        let name = file_name(&t.src);
        let dir = w.stick.join(anlz_dir(&t.dst_rel).trim_start_matches('/'));
        let read_file = |ext: &str| fs::read(dir.join(format!("ANLZ0000.{ext}"))).unwrap();
        let (dat, ext, ex2) = (read_file("DAT"), read_file("EXT"), read_file("2EX"));
        let duration = reks_audio::probe(&t.src).unwrap().duration_ms;
        let issues = reks_anlz::validate_set(
            &dat,
            &ext,
            Some(&ex2),
            Expect {
                path: Some(&t.dst_rel),
                duration_ms: Some(duration),
            },
        );
        assert!(issues.is_empty(), "{name}: {issues:?}");
        let written = (dat.len() + ext.len() + ex2.len()) as u64;
        assert!(written <= estimate[&t.track_id], "{name}: {written} bytes");

        // rekordcrate parses what has no point cues (D8): all of a and c,
        // the .2EX of b.
        let files: Vec<&[u8]> = if name == "b.flac" {
            vec![&ex2]
        } else {
            vec![&dat, &ext, &ex2]
        };
        for bytes in files {
            rekordcrate::anlz::ANLZ::read(&mut Cursor::new(bytes))
                .unwrap_or_else(|e| panic!("{name}: rekordcrate: {e}"));
        }

        // PQTZ is the library's grid; PVBR the MP3's frames × 1152.
        let sections = read::sections(&dat).unwrap();
        let pqtz = sections.iter().find(|s| &s.tag == b"PQTZ").unwrap();
        let anchors = w.lib.beatgrid(t.track_id).unwrap().anchors;
        assert_eq!(
            read::pqtz_beats(pqtz).unwrap(),
            pqtz_beats_from_anchors(&anchors, duration).unwrap(),
            "{name}"
        );
        let pvbr = sections.iter().find(|s| &s.tag == b"PVBR").unwrap();
        let trailer = u32::from_be_bytes(pvbr.bytes[pvbr.bytes.len() - 4..].try_into().unwrap());
        let frames = reks_audio::mp3_audio_frames(&t.src).unwrap();
        match name.as_str() {
            "a.mp3" => assert_eq!(Some(trailer), frames.map(|f| f * 1152)),
            _ => assert_eq!(trailer, 0),
        }
        match name.as_str() {
            "c.wav" => assert!(anchors.is_empty()),
            _ => assert!(!read::pqtz_beats(pqtz).unwrap().is_empty()),
        }
    }

    // b's cues reached both files (baken spliced them from the XML).
    let b = plan
        .tracks
        .iter()
        .find(|t| file_name(&t.src) == "b.flac")
        .unwrap();
    let dir = w.stick.join(anlz_dir(&b.dst_rel).trim_start_matches('/'));
    let dat = read::summary(&fs::read(dir.join("ANLZ0000.DAT")).unwrap()).unwrap();
    let ext = read::summary(&fs::read(dir.join("ANLZ0000.EXT")).unwrap()).unwrap();
    assert_eq!((dat.cues, ext.extended_cues), (Some(2), Some(2)));
}

#[test]
fn stale_analysis_refuses_before_writing() {
    let w = World::new();
    let plan = w.plan_with(AnlzPreference::ReksFirst, &NoAnlz);
    // The analysis goes away between the dry run and the export.
    cache::remove(&w.lib.cache_dir(), w.ids["b.flac"]).unwrap();
    let err = BakenWriter::prepare(&w.lib, &w.sel, &plan, &w.tmp.path().join("work")).unwrap_err();
    match err {
        WriterError::ReksAnlz(e) => {
            assert_eq!(e.track_id, w.ids["b.flac"]);
            assert!(matches!(e.error, reksanlz::ReksAnlzError::Stale));
        }
        other => panic!("{other}"),
    }
    assert_eq!(fs::read_dir(&w.stick).unwrap().count(), 0);
}
