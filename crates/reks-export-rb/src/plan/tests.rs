use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use reks_core::{CancelToken, Library, PlaylistOp, PlaylistOutcome};

use super::*;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/audio");

struct World {
    _tmp: tempfile::TempDir,
    music: PathBuf,
    cache: PathBuf,
    stick: PathBuf,
}

impl World {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let w = World {
            music: tmp.path().join("music"),
            cache: tmp.path().join("cache"),
            stick: tmp.path().join("stick"),
            _tmp: tmp,
        };
        for d in [&w.music, &w.cache, &w.stick] {
            fs::create_dir_all(d).unwrap();
        }
        w
    }

    /// Copies a fixture into the music folder as `name`.
    fn add(&self, fixture: &str, name: &str) -> PathBuf {
        let dst = self.music.join(name);
        fs::copy(Path::new(FIXTURES).join(fixture), &dst).unwrap();
        dst
    }

    fn dev(&self) -> DeviceInfo {
        DeviceInfo {
            mount_point: self.stick.clone(),
            fs: FsKind::Fat32,
            fs_name: "msdos".into(),
            removable: true,
            total_bytes: 32 << 30,
            free_bytes: 1 << 30,
            label: "REKS".into(),
            os_name: "disk9s1".into(),
        }
    }

    /// A rekordbox cache entry for every track: `<cache>/<id>/ANLZ0000.{DAT,EXT}`.
    fn locator(&self) -> impl Fn(&SourceTrack) -> Option<PathBuf> + '_ {
        move |t: &SourceTrack| {
            let dir = self.cache.join(t.track_id.to_string());
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("ANLZ0000.DAT"), [1u8; 100]).unwrap();
            fs::write(dir.join("ANLZ0000.EXT"), [2u8; 50]).unwrap();
            Some(dir.join("ANLZ0000.DAT"))
        }
    }

    fn stick_file(&self, rel: &str, bytes: &[u8]) {
        let p = on_stick(&self.stick, rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, bytes).unwrap();
    }

    fn plan(&self, sources: &Sources, opts: &ExportOptions) -> ExportPlan {
        plan_sources(sources.clone(), &self.dev(), opts, &self.locator()).unwrap()
    }

    /// What a writer would do with `plan` (audio, ANLZ slots, prune).
    fn apply(&self, plan: &ExportPlan) {
        for c in &plan.copies {
            let dst = on_stick(&self.stick, &c.dst_rel);
            fs::create_dir_all(dst.parent().unwrap()).unwrap();
            fs::copy(c.src.as_ref().unwrap(), dst).unwrap();
        }
        let mut slots: BTreeMap<String, u16> = BTreeMap::new();
        for t in &plan.tracks {
            let dir = anlz_dir(&t.dst_rel);
            let n = slots.entry(dir.clone()).or_default();
            self.stick_file(&format!("{dir}/ANLZ{:04}.DAT", n), b"anlz");
            self.stick_file(&format!("{dir}/ANLZ{:04}.EXT", n), b"anlz");
            *n += 1;
        }
        for d in &plan.deletes {
            fs::remove_file(on_stick(&self.stick, &d.dst_rel)).unwrap();
        }
        self.stick_file("/PIONEER/rekordbox/export.pdb", b"pdb");
    }

    /// Modification time of every file on the stick.
    fn mtimes(&self) -> BTreeMap<PathBuf, SystemTime> {
        fn walk(dir: &Path, out: &mut BTreeMap<PathBuf, SystemTime>) {
            for e in fs::read_dir(dir).unwrap() {
                let e = e.unwrap();
                let m = e.metadata().unwrap();
                out.insert(e.path(), m.modified().unwrap());
                if m.is_dir() {
                    walk(&e.path(), out);
                }
            }
        }
        let mut out = BTreeMap::new();
        walk(&self.stick, &mut out);
        out
    }
}

fn track(id: TrackId, src: PathBuf, artist: &str, album: &str) -> SourceTrack {
    SourceTrack {
        track_id: id,
        src,
        artist: Some(artist.into()),
        album: Some(album.into()),
        probe: None,
        has_grid: true,
        cue_count: 3,
    }
}

fn sources(tracks: Vec<SourceTrack>) -> Sources {
    Sources {
        playlists: vec![PlannedPlaylist {
            id: 1,
            parent_id: None,
            name: "Set".into(),
            is_folder: false,
            track_ids: tracks.iter().map(|t| t.track_id).collect(),
        }],
        tracks,
        problems: vec![],
    }
}

/// Alpha (WAV), Beta (MP3), Кино (AIFF).
fn three(w: &World) -> Sources {
    sources(vec![
        track(1, w.add("sine-click-128.wav", "a.wav"), "Alpha", "One"),
        track(2, w.add("sine-click-128.mp3", "b.mp3"), "Beta", "Two"),
        track(
            3,
            w.add("sine-click-128.aiff", "Песня.aiff"),
            "Кино",
            "Звезда",
        ),
    ])
}

fn dsts(plan: &ExportPlan) -> Vec<&str> {
    plan.tracks.iter().map(|t| t.dst_rel.as_str()).collect()
}

fn size(p: &Path) -> u64 {
    fs::metadata(p).unwrap().len()
}

#[test]
fn empty_stick_copies_everything() {
    let w = World::new();
    let src = three(&w);
    let plan = w.plan(&src, &ExportOptions::default());

    assert!(!plan.is_blocked(), "{:?}", plan.errors);
    assert_eq!(
        dsts(&plan),
        [
            "/Contents/Alpha/One/a.wav",
            "/Contents/Beta/Two/b.mp3",
            "/Contents/Kino/Zvezda/Pesnia.aiff",
        ]
    );
    assert_eq!(plan.copies.len(), 3);
    assert!(plan.copies.iter().all(|c| c.reason == OpReason::NotOnStick));
    assert!(plan.tracks.iter().all(|t| !t.on_stick));
    assert!(plan.deletes.is_empty());
    assert!(plan.warnings.is_empty(), "{:?}", plan.warnings);
    assert!(plan
        .anlz
        .iter()
        .all(|a| matches!(a, AnlzOp::CopyFromCache { bytes: 150, .. })));

    let audio: u64 = src.tracks.iter().map(|t| size(&t.src)).sum();
    assert_eq!(plan.bytes_to_write, audio + 3 * 150);
    assert_eq!(
        plan.bytes_free_after,
        (1i64 << 30) - plan.bytes_to_write as i64
    );
    assert_eq!(plan.playlists[0].track_ids, [1, 2, 3]);
}

#[test]
fn second_identical_plan_copies_nothing_and_keeps_paths() {
    let w = World::new();
    let src = three(&w);
    let first = w.plan(&src, &ExportOptions::default());
    w.apply(&first);

    let second = w.plan(&src, &ExportOptions::default());
    assert!(!second.is_blocked(), "{:?}", second.errors);
    assert_eq!(dsts(&second), dsts(&first));
    assert!(second.copies.is_empty(), "{:?}", second.copies);
    assert!(second.deletes.is_empty(), "{:?}", second.deletes);
    assert!(second.tracks.iter().all(|t| t.on_stick));
    // Only ANLZ from the cache is rewritten.
    assert_eq!(second.bytes_to_write, 3 * 150);
}

#[test]
fn replanning_gives_the_same_plan() {
    let w = World::new();
    let src = three(&w);
    // A previous export with an unrelated file in the way.
    w.stick_file("/Contents/Alpha/One/a.wav", b"someone else's file");
    let a = w.plan(&src, &ExportOptions::default());
    let b = w.plan(&src, &ExportOptions::default());
    assert_eq!(a, b);
    assert_eq!(a.tracks[0].dst_rel, "/Contents/Alpha/One/a-1.wav");
}

#[test]
fn dry_run_leaves_stick_mtimes_unchanged() {
    let w = World::new();
    let src = three(&w);
    let first = w.plan(&src, &ExportOptions::default());
    w.apply(&first);
    w.stick_file("/Contents/Old/Gone/x.mp3", b"stale");
    w.stick_file("/PIONEER/rekordbox/exportExt.pdb", b"ext");

    let before = w.mtimes();
    // Let a careless write show up as a different time.
    std::thread::sleep(Duration::from_millis(20));
    let plan = w.plan(&src, &ExportOptions::default());
    assert_eq!(plan.deletes.len(), 1);
    assert_eq!(w.mtimes(), before);
}

#[test]
fn previous_export_stale_files_are_deleted_and_nothing_else() {
    let w = World::new();
    let src = three(&w);
    let first = w.plan(&src, &ExportOptions::default());
    w.apply(&first);
    // Leftovers of a track that is no longer exported, and files outside the
    // three export folders.
    let old = "/Contents/Gone/Album/old.mp3";
    w.stick_file(old, b"old audio");
    let old_anlz = format!("{}/ANLZ0000.DAT", anlz_dir(old));
    w.stick_file(&old_anlz, b"old anlz");
    w.stick_file("/PIONEER/Artwork/00001/a1.jpg", b"jpg");
    w.stick_file("/PIONEER/rekordbox/export.pdb", b"pdb");
    w.stick_file("/PIONEER/MYSETTING.DAT", b"settings");
    w.stick_file("/notes.txt", b"mine");
    w.stick_file("/Contents/._a.wav", b"appledouble");

    // Drop Beta from the selection too.
    let mut fewer = src.clone();
    fewer.tracks.retain(|t| t.track_id != 2);
    fewer.playlists[0].track_ids.retain(|&id| id != 2);
    let plan = w.plan(&fewer, &ExportOptions::default());

    assert!(plan.copies.is_empty());
    let beta = &first.tracks[1].dst_rel;
    let mut deletes: Vec<&str> = plan.deletes.iter().map(|d| d.dst_rel.as_str()).collect();
    deletes.sort_unstable();
    let beta_dat = format!("{}/ANLZ0000.DAT", anlz_dir(beta));
    let beta_ext = format!("{}/ANLZ0000.EXT", anlz_dir(beta));
    let mut want = vec![
        old,
        &old_anlz,
        beta,
        &beta_dat,
        &beta_ext,
        "/PIONEER/Artwork/00001/a1.jpg",
    ];
    want.sort_unstable();
    assert_eq!(deletes, want);
    assert!(plan.deletes.iter().all(|d| d.reason == OpReason::Stale));
    let freed: u64 = plan.deletes.iter().map(|d| d.bytes).sum();
    assert_eq!(
        plan.bytes_free_after,
        (1i64 << 30) + freed as i64 - plan.bytes_to_write as i64
    );

    // With artwork on, the writer owns the artwork folder.
    let opts = ExportOptions {
        artwork: true,
        ..ExportOptions::default()
    };
    let plan = w.plan(&fewer, &opts);
    assert!(!plan.deletes.iter().any(|d| d.dst_rel.contains("Artwork")));

    // keep_stale: nothing deleted, but the user is told.
    let opts = ExportOptions {
        keep_stale: true,
        ..ExportOptions::default()
    };
    let plan = w.plan(&fewer, &opts);
    assert!(plan.deletes.is_empty());
    assert!(plan
        .warnings
        .iter()
        .any(|w| matches!(w, Warning::StaleKept { files: 6, .. })));
}

#[test]
fn rekordbox_written_stick_is_detected_and_unicode_paths_not_reused() {
    let w = World::new();
    let src = three(&w);
    // rekordbox's layout: Unicode names, plus its own databases.
    let rb_path = "/Contents/Кино/Звезда/Песня.aiff";
    let dst = on_stick(&w.stick, rb_path);
    fs::create_dir_all(dst.parent().unwrap()).unwrap();
    fs::copy(&src.tracks[2].src, &dst).unwrap();
    w.stick_file("/PIONEER/rekordbox/export.pdb", b"pdb");
    w.stick_file("/PIONEER/rekordbox/exportExt.pdb", b"ext");
    w.stick_file("/PIONEER/rekordbox/exportLibrary.db", b"db");
    w.stick_file("/PIONEER/rekordbox/abc.sync", b"sync");

    let plan = w.plan(&src, &ExportOptions::default());
    assert!(!plan.is_blocked());
    assert!(plan.warnings.contains(&Warning::RekordboxStick {
        found: vec![
            "/PIONEER/rekordbox/exportLibrary.db".into(),
            "/PIONEER/rekordbox/exportExt.pdb".into(),
            "/PIONEER/rekordbox/abc.sync".into(),
        ]
    }));
    assert_eq!(plan.tracks[2].dst_rel, "/Contents/Kino/Zvezda/Pesnia.aiff");
    assert_eq!(plan.copies.len(), 3);
    assert_eq!(plan.deletes.len(), 1);
    assert_eq!(plan.deletes[0].dst_rel, rb_path);
}

#[test]
fn changed_source_gets_a_fresh_copy_and_the_old_one_goes() {
    let w = World::new();
    let src = three(&w);
    let first = w.plan(&src, &ExportOptions::default());
    w.apply(&first);

    // Same size, re-written later (a re-tag that kept the length).
    let a = &src.tracks[0].src;
    let mut bytes = fs::read(a).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xFF;
    fs::write(a, bytes).unwrap();
    let later = SystemTime::now() + Duration::from_secs(60);
    fs::File::options()
        .write(true)
        .open(a)
        .unwrap()
        .set_modified(later)
        .unwrap();

    let plan = w.plan(&src, &ExportOptions::default());
    assert_eq!(plan.copies.len(), 1);
    assert_eq!(plan.copies[0].dst_rel, "/Contents/Alpha/One/a-1.wav");
    let deletes: Vec<&str> = plan.deletes.iter().map(|d| d.dst_rel.as_str()).collect();
    assert!(
        deletes.contains(&"/Contents/Alpha/One/a.wav"),
        "{deletes:?}"
    );
}

#[test]
fn identical_files_each_get_their_own_stick_file() {
    let w = World::new();
    let src = sources(vec![
        track(1, w.add("sine-click-128.mp3", "x.mp3"), "Same", "Same"),
        track(2, w.add("sine-click-128.mp3", "X.MP3"), "Same", "Same"),
    ]);
    let src = Sources {
        tracks: {
            let mut t = src.tracks;
            t[1].src = w.music.join("sub").join("x.mp3");
            fs::create_dir_all(w.music.join("sub")).unwrap();
            fs::copy(&t[0].src, &t[1].src).unwrap();
            t
        },
        ..src
    };
    let first = w.plan(&src, &ExportOptions::default());
    assert_eq!(
        dsts(&first),
        ["/Contents/Same/Same/x.mp3", "/Contents/Same/Same/x-1.mp3"]
    );
    w.apply(&first);
    let second = w.plan(&src, &ExportOptions::default());
    assert_eq!(dsts(&second), dsts(&first));
    assert!(second.copies.is_empty());
    assert!(second.deletes.is_empty());
}

#[test]
fn missing_analysis_skips_or_generates() {
    let w = World::new();
    let src = three(&w);
    let plan = plan_sources(src.clone(), &w.dev(), &ExportOptions::default(), &NoAnlz).unwrap();
    assert!(plan.tracks.is_empty());
    assert!(plan.copies.is_empty());
    assert_eq!(
        plan.warnings
            .iter()
            .filter(|w| matches!(w, Warning::Skipped { .. }))
            .count(),
        3
    );
    assert!(plan.warnings.contains(&Warning::EmptyPlaylist { id: 1 }));
    assert!(plan.playlists[0].track_ids.is_empty());

    for (mode, want) in [
        (MissingAnalysis::Generate, AnlzOp::Generate { track_id: 1 }),
        (
            MissingAnalysis::ExportWithout,
            AnlzOp::Missing { track_id: 1 },
        ),
    ] {
        let opts = ExportOptions {
            missing_analysis: mode,
            ..ExportOptions::default()
        };
        let plan = plan_sources(src.clone(), &w.dev(), &opts, &NoAnlz).unwrap();
        assert_eq!(plan.tracks.len(), 3);
        assert_eq!(plan.anlz[0], want);
    }
}

#[test]
fn missing_source_blocks_unless_skipped() {
    let w = World::new();
    let mut src = three(&w);
    src.tracks[1].src = w.music.join("nope.mp3");
    let plan = w.plan(&src, &ExportOptions::default());
    assert!(plan.is_blocked());
    assert_eq!(
        plan.errors,
        [Problem::SourceMissing {
            track_id: 2,
            path: w.music.join("nope.mp3")
        }]
    );

    let opts = ExportOptions {
        skip_unsupported: true,
        ..ExportOptions::default()
    };
    let plan = w.plan(&src, &opts);
    assert!(!plan.is_blocked());
    assert_eq!(plan.tracks.len(), 2);
    assert_eq!(plan.playlists[0].track_ids, [1, 3]);
    assert!(plan
        .warnings
        .iter()
        .any(|w| matches!(w, Warning::Skipped { track_id: 2, .. })));
}

#[test]
fn codec_severity_follows_the_target() {
    let w = World::new();
    let src = sources(vec![track(
        1,
        w.add("sine-click-128.flac", "f.flac"),
        "F",
        "F",
    )]);
    let nxs = ExportOptions {
        target: TargetProfile::Cdj2000Nxs,
        ..ExportOptions::default()
    };
    let plan = w.plan(&src, &nxs);
    assert!(matches!(
        plan.errors.as_slice(),
        [Problem::Unsupported { track_id: 1, .. }]
    ));
    assert!(plan.copies.is_empty());

    let nxs2 = ExportOptions {
        target: TargetProfile::Cdj2000Nxs2,
        ..ExportOptions::default()
    };
    let plan = w.plan(&src, &nxs2);
    assert!(!plan.is_blocked());
    assert!(matches!(
        plan.warnings.as_slice(),
        [Warning::Audio { track_id: 1, .. }]
    ));
}

#[test]
fn device_checks() {
    let w = World::new();
    let src = three(&w);
    let mut dev = w.dev();
    dev.free_bytes = 1_000;
    dev.removable = false;
    dev.fs = FsKind::ExFat;
    let plan = plan_sources(src.clone(), &dev, &ExportOptions::default(), &w.locator()).unwrap();
    assert!(matches!(
        plan.errors.as_slice(),
        [Problem::NotEnoughSpace { free: 1_000, .. }]
    ));
    assert!(plan.bytes_free_after < 0);
    assert!(plan.warnings.contains(&Warning::NotRemovable));
    assert!(plan.warnings.contains(&Warning::ExFat {
        target: TargetProfile::Generic
    }));

    dev.fs = FsKind::Other("apfs".into());
    dev.fs_name = "apfs".into();
    dev.free_bytes = 1 << 30;
    let plan = plan_sources(src, &dev, &ExportOptions::default(), &w.locator()).unwrap();
    assert_eq!(
        plan.errors,
        [Problem::UnsupportedFilesystem {
            fs_name: "apfs".into()
        }]
    );
}

#[test]
fn anlz_dir_matches_rekordbox() {
    // Pairs from a rekordbox export, as recorded by baken-export's tests.
    assert_eq!(
        anlz_dir("/Contents/XamarA/UnknownAlbum/Unreal.mp3"),
        "/PIONEER/USBANLZ/P060/00012938"
    );
    assert_eq!(
        anlz_dir("/Contents/Szeir/Stolperfalle EP/Szeir - Stolperfalle EP - 01 Oort Cloud.flac"),
        "/PIONEER/USBANLZ/P074/0001B742"
    );
}

#[test]
fn plan_serialises_and_round_trips() {
    let w = World::new();
    let plan = w.plan(&three(&w), &ExportOptions::default());
    let json = serde_json::to_string(&plan).unwrap();
    assert!(json.contains("\"dstRel\":\"/Contents/Alpha/One/a.wav\""));
    assert!(json.contains("\"type\":\"copyFromCache\""));
    let back: ExportPlan = serde_json::from_str(&json).unwrap();
    assert_eq!(back, plan);
}

#[test]
fn loads_selection_from_the_library() {
    let w = World::new();
    w.add("sine-click-128.wav", "a.wav");
    w.add("sine-click-128.mp3", "b.mp3");
    let lib = Library::open_in_memory(&w.cache.join("lib")).unwrap();
    reks_core::scan(
        std::slice::from_ref(&w.music),
        &lib,
        &CancelToken::new(),
        |_| {},
    )
    .unwrap();
    let ids: Vec<TrackId> = lib
        .tracks(&reks_core::TrackQuery::default())
        .unwrap()
        .rows
        .iter()
        .map(|r| r.id)
        .collect();
    assert_eq!(ids.len(), 2);

    let created = |op| match lib.playlist_mutate(op).unwrap() {
        PlaylistOutcome::Done { created_id, .. } => created_id.unwrap(),
        other => panic!("{other:?}"),
    };
    let folder = created(PlaylistOp::CreateFolder {
        parent_id: None,
        name: "Gigs".into(),
        position: None,
    });
    let set = created(PlaylistOp::CreatePlaylist {
        parent_id: Some(folder),
        name: "Friday".into(),
        position: None,
    });
    let other = created(PlaylistOp::CreatePlaylist {
        parent_id: None,
        name: "Other".into(),
        position: None,
    });
    for (pl, tracks) in [(set, vec![ids[1], ids[0], ids[1]]), (other, vec![ids[0]])] {
        lib.playlist_mutate(PlaylistOp::AddTracks {
            playlist_id: pl,
            track_ids: tracks,
            before: None,
            duplicates: reks_core::DuplicateMode::Allow,
        })
        .unwrap();
    }

    let sel = Selection {
        playlists: vec![set, 999],
    };
    let src = load(&lib, &sel).unwrap();
    assert_eq!(src.problems, [Problem::PlaylistNotFound { id: 999 }]);
    let names: Vec<&str> = src.playlists.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["Gigs", "Friday"]);
    assert!(src.playlists[0].track_ids.is_empty());
    assert_eq!(src.playlists[1].track_ids, [ids[1], ids[0], ids[1]]);
    let order: Vec<TrackId> = src.tracks.iter().map(|t| t.track_id).collect();
    assert_eq!(order, [ids[1], ids[0]]);
    assert!(src.tracks.iter().all(|t| t.probe.is_some()));

    // Selecting the folder takes everything below it.
    let src = load(
        &lib,
        &Selection {
            playlists: vec![folder],
        },
    )
    .unwrap();
    assert!(src.problems.is_empty());
    assert_eq!(src.playlists.len(), 2);

    let plan = plan(
        &lib,
        &Selection {
            playlists: vec![set],
        },
        &w.dev(),
        &ExportOptions::default(),
        &w.locator(),
    )
    .unwrap();
    assert!(!plan.is_blocked(), "{:?}", plan.errors);
    assert_eq!(plan.copies.len(), 2);

    let src = load(&lib, &Selection::default()).unwrap();
    assert_eq!(src.problems, [Problem::NothingSelected]);
}
