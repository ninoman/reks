//! rekordbox's analysis mapped into reks's cache (D10, `rbmap`), end to
//! end: an XML import with `TEMPO` markers gives rekordbox grid rows, the
//! rekordbox 6 reference analysis (`fixtures/rekordbox/ref-rb6-demo`) is
//! found for one track and mapped, the track leaves the analysis queue's
//! pending list, and reks-built ANLZ for it carries rekordbox's waveform
//! bytes with the library's grid, including after a grid edit.
//!
//! The audio files are stand-ins (the reference export has no audio): the
//! import takes the length from `TotalTime`, and nothing here decodes.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use reks_analysis::{cache, ANALYSER_VERSION};
use reks_anlz::read::{self, PQTZ};
use reks_anlz::write::{PWAV, PWV2, PWV3, PWV4, PWV5, PWV6, PWV7, PWVC};
use reks_anlz::AnlzTarget;
use reks_audio::Kind;
use reks_core::analysis::file_content_hash;
use reks_core::rbxml::{self, ImportOptions, PrefixRemap};
use reks_core::{GridEdit, Library, TrackId};
use reks_export_rb::rbcache::{Missing, RbCache};
use reks_export_rb::rbmap::{map_pending, Skip};
use reks_export_rb::reksanlz::{self, ReksTrack};

const REF: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/rekordbox/ref-rb6-demo/PIONEER/USBANLZ"
);
/// Demo Track 1's analysis: 368 beats at 128 BPM, 25,866 columns.
const DEMO1: &str = "P016/0000875E/ANLZ0000";
const XML_PREFIX: &str = "/Users/reks/Music/rb";

fn ref_file(ext: &str) -> Vec<u8> {
    fs::read(Path::new(REF).join(format!("{DEMO1}.{ext}"))).unwrap()
}

fn section(bytes: &[u8], tag: [u8; 4]) -> Vec<u8> {
    read::sections(bytes)
        .unwrap()
        .into_iter()
        .find(|s| s.tag == tag)
        .unwrap_or_else(|| panic!("no {}", String::from_utf8_lossy(&tag)))
        .bytes
        .to_vec()
}

fn pqtz_times(dat: &[u8]) -> Vec<u32> {
    let s = read::sections(dat).unwrap();
    let pqtz = s.iter().find(|s| s.tag == PQTZ).unwrap();
    read::pqtz_beats(pqtz)
        .unwrap()
        .iter()
        .map(|b| b.time_ms)
        .collect()
}

struct World {
    _tmp: tempfile::TempDir,
    lib: Library,
    music: PathBuf,
    rb_root: PathBuf,
    ids: Vec<(String, TrackId)>,
}

/// rekordbox's first beat of Demo Track 1, as XML `Inizio` seconds.
fn demo1_first_beat_s() -> f64 {
    f64::from(pqtz_times(&ref_file("DAT"))[0]) / 1000.0
}

fn xml(first_beat_s: f64) -> String {
    let track = |id: u32, name: &str, extra: &str| {
        format!(
            r#"<TRACK TrackID="{id}" Name="{name}" Kind="MP3 File" TotalTime="172" Tonality="Am"
  Location="file://localhost{XML_PREFIX}/{}">{extra}</TRACK>"#,
            name.replace(' ', "%20")
        )
    };
    let tempo = |s: f64, bpm: &str| {
        format!(r#"<TEMPO Inizio="{s:.3}" Bpm="{bpm}" Metro="4/4" Battito="1"/>"#)
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<DJ_PLAYLISTS Version="1.0.0">
<PRODUCT Name="rekordbox" Version="6.8.5" Company="AlphaTheta"/>
<COLLECTION Entries="4">
{}
{}
{}
{}
</COLLECTION>
<PLAYLISTS><NODE Type="0" Name="ROOT" Count="0"></NODE></PLAYLISTS>
</DJ_PLAYLISTS>"#,
        track(1, "Demo Track 1.mp3", &tempo(first_beat_s, "128.00")),
        // Same file name as rekordbox's Demo Track 2, other grid: refused.
        track(2, "Demo Track 2.mp3", &tempo(0.5, "125.00")),
        track(3, "Other.mp3", &tempo(0.1, "124.00")),
        track(4, "No Grid.mp3", ""),
    )
}

impl World {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let music = tmp.path().join("music");
        fs::create_dir_all(&music).unwrap();
        for name in [
            "Demo Track 1.mp3",
            "Demo Track 2.mp3",
            "Other.mp3",
            "No Grid.mp3",
        ] {
            fs::write(music.join(name), format!("stand-in for {name}")).unwrap();
        }
        let rb_root = tmp.path().join("rekordbox/USBANLZ");
        for dir in ["P016/0000875E", "P053/0001D21F"] {
            fs::create_dir_all(rb_root.join(dir)).unwrap();
            for ext in ["DAT", "EXT", "2EX"] {
                let name = format!("ANLZ0000.{ext}");
                fs::copy(
                    Path::new(REF).join(dir).join(&name),
                    rb_root.join(dir).join(name),
                )
                .unwrap();
            }
        }
        let lib =
            Library::open(&tmp.path().join("reks.sqlite"), &tmp.path().join("cache")).unwrap();
        let c = rbxml::parse(Cursor::new(xml(demo1_first_beat_s()))).unwrap();
        let opts = ImportOptions {
            remap: vec![PrefixRemap {
                from: XML_PREFIX.into(),
                to: music.clone(),
            }],
            ..ImportOptions::default()
        };
        rbxml::import(&c, &lib, opts, |_| {}).unwrap();
        let ids = lib
            .analysis_pending(ANALYSER_VERSION)
            .unwrap()
            .into_iter()
            .map(|j| (j.path, j.track_id))
            .collect();
        World {
            _tmp: tmp,
            lib,
            music,
            rb_root,
            ids,
        }
    }

    /// Track id by file name (every track is pending before mapping).
    fn id(&self, name: &str) -> TrackId {
        let path = self.music.join(name);
        self.ids
            .iter()
            .find(|(p, _)| Path::new(p) == path)
            .unwrap()
            .1
    }

    fn build(&self, id: TrackId) -> reks_anlz::AnlzSet {
        let src = self.music.join("Demo Track 1.mp3");
        let t = ReksTrack {
            track_id: id,
            src: &src,
            contents_path: "/Contents/Demo Track 1.mp3",
            // Not Mp3: the stand-in has no MP3 frames to count for PVBR.
            kind: Kind::Flac,
            rekordbox_dat: None,
        };
        reksanlz::build(&self.lib, &t, AnlzTarget::Cdj3000).unwrap()
    }
}

#[test]
fn import_gives_rekordbox_grid_rows() {
    let w = World::new();
    let grid = w.lib.beatgrid(w.id("Demo Track 1.mp3")).unwrap();
    assert_eq!(grid.source.as_deref(), Some("rekordbox"));
    assert_eq!(grid.anchors.len(), 1);
    assert_eq!(grid.anchors[0].bpm_x100, 12800);
    assert_eq!(
        grid.anchors[0].position_ms,
        i64::from(pqtz_times(&ref_file("DAT"))[0])
    );
    assert!(w
        .lib
        .beatgrid(w.id("No Grid.mp3"))
        .unwrap()
        .anchors
        .is_empty());
}

#[test]
fn rekordbox_analysis_is_mapped_and_skips_the_queue() {
    let w = World::new();
    let (demo1, demo2, other, no_grid) = (
        w.id("Demo Track 1.mp3"),
        w.id("Demo Track 2.mp3"),
        w.id("Other.mp3"),
        w.id("No Grid.mp3"),
    );
    let pending = |lib: &Library| -> Vec<TrackId> {
        lib.analysis_pending(ANALYSER_VERSION)
            .unwrap()
            .iter()
            .map(|j| j.track_id)
            .collect()
    };
    assert_eq!(pending(&w.lib).len(), 4);

    let rb = RbCache::scan(std::slice::from_ref(&w.rb_root)).unwrap();
    let report = map_pending(&w.lib, &rb, ANALYSER_VERSION).unwrap();
    assert_eq!(report.candidates, 3, "the grid-less track is no candidate");
    assert_eq!(report.mapped, [demo1]);
    assert_eq!(
        report.skipped,
        [
            (demo2, Skip::NotFound(Missing::Mismatch)),
            (other, Skip::NotFound(Missing::NotAnalysed)),
        ]
    );

    // The queue skips it; the rest still go through reks's analyser, the
    // grid-less one first.
    assert_eq!(pending(&w.lib), [no_grid, demo2, other]);
    let hash = file_content_hash(&w.music.join("Demo Track 1.mp3")).unwrap();
    let entry = cache::load_rekordbox(&w.lib.cache_dir(), demo1, &hash).unwrap();
    assert_eq!(entry.duration_ms, 172_000);
    assert_eq!(entry.ext, ref_file("EXT"));
    assert!(cache::load_valid(&w.lib.cache_dir(), demo1, &hash).is_none());

    // Mapping again finds nothing new.
    let again = map_pending(&w.lib, &rb, ANALYSER_VERSION).unwrap();
    assert!(again.mapped.is_empty());
}

#[test]
fn built_anlz_has_rekordbox_waveforms_and_the_library_grid() {
    let w = World::new();
    let demo1 = w.id("Demo Track 1.mp3");
    let rb = RbCache::scan(std::slice::from_ref(&w.rb_root)).unwrap();
    map_pending(&w.lib, &rb, ANALYSER_VERSION).unwrap();

    let set = w.build(demo1);
    let ex2 = set.ex2.as_deref().unwrap();
    let (dat, ext, rb_ex2) = (ref_file("DAT"), ref_file("EXT"), ref_file("2EX"));
    for tag in [PWAV, PWV2] {
        assert_eq!(section(&set.dat, tag), section(&dat, tag));
    }
    for tag in [PWV3, PWV4, PWV5] {
        assert_eq!(section(&set.ext, tag), section(&ext, tag));
    }
    for tag in [PWV6, PWV7, PWVC] {
        assert_eq!(section(ex2, tag), section(&rb_ex2, tag));
    }
    // The grid is the library's: rekordbox's beats within 1 ms (its own
    // times are not exactly linear, phase 3 doc), up to the 172 s length.
    let ours = pqtz_times(&set.dat);
    let theirs = pqtz_times(&dat);
    assert_eq!(ours[0], theirs[0]);
    for (a, b) in ours.iter().zip(&theirs) {
        assert!(a.abs_diff(*b) <= 1, "{a} vs {b}");
    }

    // A grid edit reaches the built files; the waveforms stay rekordbox's.
    w.lib
        .edit_beatgrid(
            demo1,
            GridEdit::Nudge {
                delta_ms: 10,
                section: None,
            },
        )
        .unwrap();
    let edited = w.build(demo1);
    assert_eq!(pqtz_times(&edited.dat)[0], theirs[0] + 10);
    assert_eq!(section(&edited.ext, PWV5), section(&ext, PWV5));
}

#[test]
fn an_edited_grid_without_reks_analysis_rebuilds_rekordbox_files() {
    // Not mapped (and not analysed): only rekordbox's own cache has the
    // track, as for a grid edited before the queue reached it (D10).
    let w = World::new();
    let demo1 = w.id("Demo Track 1.mp3");
    w.lib
        .edit_beatgrid(demo1, GridEdit::AddMarker { at_ms: 60_000 })
        .unwrap();
    let g = w
        .lib
        .edit_beatgrid(
            demo1,
            GridEdit::SetSectionBpm {
                section: 1,
                bpm_x100: 13_000,
            },
        )
        .unwrap();
    assert_eq!(g.anchors.len(), 2);
    let src = w.music.join("Demo Track 1.mp3");
    let dat = w.rb_root.join(format!("{DEMO1}.DAT"));
    let t = ReksTrack {
        track_id: demo1,
        src: &src,
        contents_path: "/Contents/Demo Track 1.mp3",
        kind: Kind::Flac,
        rekordbox_dat: Some(&dat),
    };
    let set = reksanlz::build(&w.lib, &t, AnlzTarget::Cdj3000).unwrap();
    // rekordbox's waveforms, the library's two-tempo grid.
    assert_eq!(section(&set.ext, PWV5), section(&ref_file("EXT"), PWV5));
    let s = read::sections(&set.dat).unwrap();
    let beats = read::pqtz_beats(s.iter().find(|s| s.tag == PQTZ).unwrap()).unwrap();
    let marker = beats
        .iter()
        .position(|b| i64::from(b.time_ms) == g.anchors[1].position_ms)
        .unwrap();
    assert_eq!(beats[marker - 1].tempo, 12_800);
    assert!(beats[marker..].iter().all(|b| b.tempo == 13_000));
    // Without the rekordbox files there is nothing to build from.
    let none = ReksTrack {
        rekordbox_dat: None,
        ..t
    };
    assert!(reksanlz::build(&w.lib, &none, AnlzTarget::Cdj3000).is_err());
}
