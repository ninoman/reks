use std::fs;
use std::path::{Path, PathBuf};

use reks_core::{CancelToken, TrackPatch};

use super::*;
use crate::plan::{plan_sources, MissingAnalysis, NoAnlz, PlannedPlaylist, SourceTrack, Sources};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/audio");

/// A temp music folder, analysis cache and stick.
struct World {
    _tmp: tempfile::TempDir,
    music: PathBuf,
    cache: PathBuf,
    stick: PathBuf,
    free_bytes: u64,
}

impl World {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let w = World {
            music: tmp.path().join("music"),
            cache: tmp.path().join("cache"),
            stick: tmp.path().join("stick"),
            free_bytes: 1 << 30,
            _tmp: tmp,
        };
        for d in [&w.music, &w.cache, &w.stick] {
            fs::create_dir_all(d).unwrap();
        }
        w
    }

    fn add(&self, fixture: &str, name: &str) -> PathBuf {
        let dst = self.music.join(name);
        fs::copy(Path::new(FIXTURES).join(fixture), &dst).unwrap();
        dst
    }

    fn stick_file(&self, rel: &str, bytes: &[u8]) {
        let p = rel
            .trim_start_matches('/')
            .split('/')
            .fold(self.stick.clone(), |p, c| p.join(c));
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, bytes).unwrap();
    }

    fn dev(&self) -> DeviceInfo {
        DeviceInfo {
            mount_point: self.stick.clone(),
            fs: FsKind::Fat32,
            fs_name: "msdos".into(),
            removable: true,
            total_bytes: 32_000_000_000,
            free_bytes: self.free_bytes,
            label: "REKS".into(),
            os_name: "disk9s1".into(),
        }
    }

    /// Plans with a rekordbox cache entry for every track id in `analysed`.
    fn plan(&self, sources: Sources, opts: &ExportOptions, analysed: &[TrackId]) -> ExportPlan {
        let locate = |t: &SourceTrack| {
            if !analysed.contains(&t.track_id) {
                return None;
            }
            let dir = self.cache.join(t.track_id.to_string());
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("ANLZ0000.DAT"), [1u8; 1000]).unwrap();
            Some(dir.join("ANLZ0000.DAT"))
        };
        plan_sources(sources, &self.dev(), serial(), opts, &locate, &NoAnlz).unwrap()
    }
}

fn serial() -> Option<String> {
    Some("4993-1CFB".into())
}

fn track(id: TrackId, src: PathBuf) -> SourceTrack {
    SourceTrack {
        track_id: id,
        src,
        artist: Some("Alpha".into()),
        album: Some("One".into()),
        probe: None,
        has_grid: true,
        grid_user_owned: false,
        cue_count: 0,
        divider: None,
    }
}

/// Folder "Gigs" with playlist "Friday" holding `tracks`.
fn sources(tracks: Vec<SourceTrack>) -> Sources {
    Sources {
        playlists: vec![
            PlannedPlaylist {
                id: 10,
                parent_id: None,
                name: "Gigs".into(),
                is_folder: true,
                track_ids: vec![],
                smart: false,
            },
            PlannedPlaylist {
                id: 11,
                parent_id: Some(10),
                name: "Friday".into(),
                is_folder: false,
                track_ids: tracks.iter().map(|t| t.track_id).collect(),
                smart: false,
            },
        ],
        tracks,
        problems: vec![],
        dividers: vec![],
    }
}

fn labels() -> TrackLabels {
    [
        (1, "Alpha – First".to_owned()),
        (2, "Alpha – Second".to_owned()),
    ]
    .into()
}

fn fact<'a>(facts: &'a [Fact], label: &str) -> &'a str {
    &facts.iter().find(|f| f.label == label).unwrap().value
}

fn kinds(groups: &[NoticeGroup]) -> Vec<&str> {
    groups.iter().map(|g| g.kind.as_str()).collect()
}

#[test]
fn ready_plan_on_an_empty_stick() {
    let w = World::new();
    let a = w.add("sine-click-128.wav", "a.wav");
    let b = w.add("sine-click-128.mp3", "b.mp3");
    let audio = fs::metadata(&a).unwrap().len() + fs::metadata(&b).unwrap().len();
    let plan = w.plan(
        sources(vec![track(1, a), track(2, b)]),
        &ExportOptions::default(),
        &[1, 2],
    );
    let dry = DryRun::new(&plan, &labels());

    assert!(dry.ready);
    assert_eq!(
        dry.status,
        "Ready to export. Nothing has been written to the stick yet."
    );
    assert_eq!(dry.device.name, "REKS");
    assert_eq!(dry.device.fs_text, "FAT32 (msdos)");
    assert_eq!(dry.device.space_text, "1.1 GB free of 32.0 GB");
    assert_eq!(dry.device.volume_serial.as_deref(), Some("4993-1CFB"));
    assert_eq!(dry.target, "Any player");

    assert_eq!(
        fact(&dry.summary, "Tracks"),
        "2: 2 to copy, 0 already on the stick"
    );
    assert_eq!(fact(&dry.summary, "Playlists"), "1 in 1 folder");
    assert_eq!(fact(&dry.summary, "Analysis"), "2 from rekordbox (2.0 KB)");
    assert_eq!(
        fact(&dry.summary, "Copy"),
        format!("2 files, {}", human_bytes(audio))
    );
    assert_eq!(fact(&dry.summary, "Delete"), "None");

    let rows: Vec<(&str, usize, &str)> = dry
        .playlists
        .iter()
        .map(|p| (p.name.as_str(), p.depth, p.detail.as_str()))
        .collect();
    assert_eq!(rows, [("Gigs", 0, ""), ("Friday", 1, "2 tracks")]);

    assert_eq!(dry.copies.count, 2);
    assert_eq!(dry.copies.bytes, audio);
    assert_eq!(dry.copies.files[0].path, "/Contents/Alpha/One/a.wav");
    assert_eq!(
        dry.copies.files[0].from.as_deref(),
        Some(w.music.join("a.wav").display().to_string().as_str())
    );
    assert_eq!(dry.deletes.count, 0);

    assert!(dry.space.fits);
    assert_eq!(dry.space.write_bytes, audio + 2000);
    assert_eq!(
        dry.space.free_after_bytes,
        (1i64 << 30) - (audio + 2000) as i64
    );
    assert_eq!(
        dry.space.used_after_bytes,
        32_000_000_000 - dry.space.free_after_bytes as u64
    );
    assert_eq!(fact(&dry.space.facts, "Free now"), "1.1 GB");

    assert!(!dry.history.at_risk);
    assert_eq!(dry.history.title, "No play history on this stick");
    assert!(dry.history.action_text.is_none());
    assert!(dry.problems.is_empty());
    assert!(dry.warnings.is_empty(), "{:?}", dry.warnings);
}

#[test]
fn stale_files_are_listed_as_deletes() {
    let w = World::new();
    w.stick_file("/Contents/Old/Gone/old.mp3", &[0u8; 2500]);
    let a = w.add("sine-click-128.wav", "a.wav");
    let plan = w.plan(sources(vec![track(1, a)]), &ExportOptions::default(), &[1]);
    let dry = DryRun::new(&plan, &labels());

    assert_eq!(dry.deletes.count, 1);
    assert_eq!(dry.deletes.total_text, "1 file, 2.5 KB");
    assert_eq!(dry.deletes.files[0].path, "/Contents/Old/Gone/old.mp3");
    assert_eq!(dry.deletes.files[0].from, None);
    assert_eq!(fact(&dry.space.facts, "Deletes free"), "2.5 KB");
}

#[test]
fn blocked_plan_groups_problems_with_track_names() {
    let mut w = World::new();
    w.free_bytes = 10;
    let a = w.add("sine-click-128.wav", "a.wav");
    let plan = w.plan(
        sources(vec![
            track(1, w.music.join("gone-1.mp3")),
            track(2, w.music.join("gone-2.mp3")),
            track(3, a),
        ]),
        &ExportOptions::default(),
        &[1, 2, 3],
    );
    assert!(plan.is_blocked());
    let dry = DryRun::new(&plan, &labels());

    assert!(!dry.ready);
    assert_eq!(
        dry.status,
        "Blocked by 3 problems. Fix them or change the options; nothing has been written."
    );
    assert_eq!(kinds(&dry.problems), ["sourceMissing", "notEnoughSpace"]);
    let missing = &dry.problems[0];
    assert_eq!(missing.title, "2 tracks: file not found");
    assert_eq!(
        missing.items,
        [
            format!("Alpha – First: {}", w.music.join("gone-1.mp3").display()),
            format!("Alpha – Second: {}", w.music.join("gone-2.mp3").display()),
        ]
    );
    let space = &dry.problems[1];
    assert!(space.items.is_empty());
    assert!(
        space
            .title
            .starts_with("Not enough space: the export writes "),
        "{}",
        space.title
    );
    assert!(space.title.ends_with("the stick has 10 B free."));

    assert!(!dry.space.fits);
    assert!(dry.space.free_after_bytes < 0);
    assert_eq!(dry.space.used_after_bytes, 0);
    assert!(fact(&dry.space.facts, "Free after").ends_with(" short"));
}

#[test]
fn warnings_are_grouped_and_unknown_tracks_get_their_id() {
    let w = World::new();
    let a = w.add("sine-click-128.wav", "a.wav");
    let b = w.add("sine-click-128.mp3", "b.mp3");
    let c = w.add("sine-click-128.aiff", "c.aiff");
    let mut src = sources(vec![track(1, a.clone()), track(2, b), track(7, c)]);
    // A second playlist holding only the track without analysis.
    src.playlists.push(PlannedPlaylist {
        id: 12,
        parent_id: None,
        name: "Saturday".into(),
        is_folder: false,
        track_ids: vec![7],
        smart: false,
    });
    let plan = plan_sources(
        src,
        &w.dev(),
        None,
        &ExportOptions::default(),
        &|t: &SourceTrack| {
            (t.track_id == 1).then(|| {
                fs::write(w.cache.join("1.DAT"), [1u8; 10]).unwrap();
                w.cache.join("1.DAT")
            })
        },
        &NoAnlz,
    )
    .unwrap();
    let dry = DryRun::new(&plan, &labels());

    assert!(dry.ready);
    assert_eq!(
        kinds(&dry.warnings),
        ["noVolumeSerial", "skipped", "emptyPlaylist"]
    );
    let serial = &dry.warnings[0];
    assert!(serial.items.is_empty());
    assert!(
        serial.title.starts_with("The OS gave no volume serial"),
        "{}",
        serial.title
    );
    assert!(serial.title.ends_with('.'));

    let skipped = &dry.warnings[1];
    assert_eq!(skipped.title, "2 tracks left out");
    assert_eq!(
        skipped.items,
        [
            "Alpha – Second: no rekordbox analysis found; reks has not analysed it yet",
            "track 7: no rekordbox analysis found; reks has not analysed it yet",
        ]
    );
    assert_eq!(
        dry.warnings[2].title,
        "1 playlist ends up empty after leaving tracks out"
    );
    assert_eq!(dry.warnings[2].items, ["Saturday"]);
    assert_eq!(
        fact(&dry.summary, "Tracks"),
        "1: 1 to copy, 0 already on the stick"
    );
}

#[test]
fn generated_and_missing_analysis_are_counted() {
    let w = World::new();
    let a = w.add("sine-click-128.wav", "a.wav");
    let plan = w.plan(
        sources(vec![track(1, a)]),
        &ExportOptions {
            missing_analysis: MissingAnalysis::Generate,
            ..ExportOptions::default()
        },
        &[],
    );
    let dry = DryRun::new(&plan, &labels());
    assert_eq!(
        fact(&dry.summary, "Analysis"),
        "0 from rekordbox (0 B), 1 generated (approximate)"
    );
}

#[test]
fn played_stick_shows_history_and_backups() {
    let w = World::new();
    w.stick_file(
        "/PIONEER/rekordbox/export.pdb",
        &crate::testutil::played_pdb(),
    );
    w.stick_file("/PIONEER/rekordbox/exportLibrary.db", b"sqlite");
    let a = w.add("sine-click-128.wav", "a.wav");
    let plan = w.plan(sources(vec![track(1, a)]), &ExportOptions::default(), &[1]);
    let dry = DryRun::new(&plan, &labels());

    assert!(dry.ready);
    let h = &dry.history;
    assert!(h.at_risk);
    assert_eq!(h.policy, HistoryPolicy::BackUp);
    assert_eq!(h.title, "This stick holds play history");
    assert_eq!(
        h.text,
        "The stick's export.pdb holds 2 history sessions with 3 played tracks; an \
         exportLibrary.db (OneLibrary) is present and players may have logged plays there. \
         Exporting rebuilds the stick's database, which erases it."
    );
    let pdb = fs::metadata(w.stick.join("PIONEER/rekordbox/export.pdb"))
        .unwrap()
        .len();
    assert_eq!(
        h.action_text.as_deref(),
        Some(
            format!(
                "Before writing anything, reks copies 2 database files ({}) to its data \
                 folder and checks the copies; if the backup fails, nothing is written. The \
                 report after the export shows where the copies are.",
                human_bytes(pdb + 6)
            )
            .as_str()
        )
    );
    let backed_up: Vec<&str> = h.backups.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(
        backed_up,
        [
            "/PIONEER/rekordbox/export.pdb",
            "/PIONEER/rekordbox/exportLibrary.db"
        ]
    );
    // The history warning is shown in `history`, not twice; rekordbox's own
    // stick marker is still a warning.
    assert!(!kinds(&dry.warnings).contains(&"stickHistory"));
    assert_eq!(kinds(&dry.warnings), ["rekordboxStick"]);
    assert_eq!(
        dry.warnings[0].title,
        "rekordbox wrote this stick (/PIONEER/rekordbox/exportLibrary.db). reks rewrites its \
         Device Library completely, and rekordbox will show its \"out of matching\" caution \
         afterwards."
    );
}

#[test]
fn discard_needs_confirmation_before_the_plan_is_ready() {
    let w = World::new();
    w.stick_file(
        "/PIONEER/rekordbox/export.pdb",
        &crate::testutil::played_pdb(),
    );
    let a = w.add("sine-click-128.wav", "a.wav");
    let discard = ExportOptions {
        history: HistoryPolicy::Discard,
        ..ExportOptions::default()
    };

    let plan = w.plan(sources(vec![track(1, a.clone())]), &discard, &[1]);
    let dry = DryRun::new(&plan, &labels());
    assert!(!dry.ready);
    assert_eq!(kinds(&dry.problems), ["discardNotConfirmed"]);
    assert!(dry.problems[0]
        .title
        .contains("tick the box in the options"));
    assert_eq!(
        dry.history.action_text.as_deref(),
        Some("Discarding it needs your confirmation in the options.")
    );
    assert_eq!(dry.history.backups.count, 0);

    let confirmed = ExportOptions {
        discard_history_confirmed: true,
        ..discard
    };
    let plan = w.plan(sources(vec![track(1, a)]), &confirmed, &[1]);
    let dry = DryRun::new(&plan, &labels());
    assert!(dry.ready);
    assert_eq!(
        dry.history.action_text.as_deref(),
        Some("You chose to discard it: it is erased without a backup.")
    );
    assert_eq!(dry.history.backups.count, 0);
}

#[test]
fn json_is_camel_case() {
    let w = World::new();
    let a = w.add("sine-click-128.wav", "a.wav");
    let plan = w.plan(sources(vec![track(1, a)]), &ExportOptions::default(), &[1]);
    let dry = DryRun::new(&plan, &labels());
    let json = serde_json::to_value(&dry).unwrap();
    for key in [
        "ready",
        "status",
        "device",
        "target",
        "summary",
        "playlists",
        "copies",
        "deletes",
        "space",
        "history",
        "problems",
        "warnings",
    ] {
        assert!(json.get(key).is_some(), "{key}");
    }
    assert!(json["space"]["freeAfterBytes"].is_i64());
    assert!(json["copies"]["totalText"].is_string());
    assert!(json["device"]["mountPoint"].is_string());
    assert_eq!(json["history"]["policy"], "backUp");
    assert_eq!(json["playlists"][1]["isFolder"], false);
    let back: DryRun = serde_json::from_value(json).unwrap();
    assert_eq!(back, dry);
}

#[test]
fn track_labels_come_from_the_library() {
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
    let rows = lib.tracks(&reks_core::TrackQuery::default()).unwrap().rows;
    let (first, second) = (rows[0].id, rows[1].id);
    lib.update_track(
        first,
        &TrackPatch {
            artist: Some(Some("Кино".into())),
            title: Some(Some("Звезда".into())),
            ..TrackPatch::default()
        },
    )
    .unwrap();
    lib.update_track(
        second,
        &TrackPatch {
            artist: Some(None),
            title: Some(None),
            ..TrackPatch::default()
        },
    )
    .unwrap();
    let second_file = Path::new(&lib.track(second).unwrap().track.path)
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();

    let mut plan = w.plan(sources(vec![]), &ExportOptions::default(), &[]);
    plan.warnings = vec![
        Warning::Skipped {
            track_id: first,
            reason: "x".into(),
        },
        Warning::Audio {
            track_id: 999,
            message: "y".into(),
        },
    ];
    plan.errors = vec![Problem::Unsupported {
        track_id: second,
        message: "z".into(),
    }];
    let labels = track_labels(&lib, &plan);
    assert_eq!(labels.len(), 2);
    assert_eq!(labels[&first], "Кино – Звезда");
    assert_eq!(labels[&second], second_file);

    let dry = DryRun::build(&lib, &plan);
    assert_eq!(dry.warnings[0].items, ["Кино – Звезда: x"]);
    assert_eq!(dry.warnings[1].items, ["track 999: y"]);
    assert_eq!(dry.problems[0].items, [format!("{second_file}: z")]);
    assert!(dry.convertible.is_empty());
    assert_eq!(dry.convert_text, None);

    plan.convertible = vec![first, second];
    let dry = DryRun::build(&lib, &plan);
    assert_eq!(dry.convertible, [first, second]);
    assert!(dry
        .convert_text
        .as_deref()
        .is_some_and(|t| t.starts_with("2 tracks can be converted")));
}

#[test]
fn target_labels() {
    assert_eq!(target_label(TargetProfile::Cdj2000Nxs2), "CDJ-2000NXS2");
    assert_eq!(target_label(TargetProfile::Cdj3000), "CDJ-3000");
}

#[test]
fn plan_at_needs_an_exact_mount_point() {
    let w = World::new();
    let lib = Library::open_in_memory(&w.cache.join("lib")).unwrap();
    // A folder on some volume is not a stick root.
    let err = plan_at(
        &lib,
        &Selection::default(),
        &w.stick,
        &ExportOptions::default(),
        &crate::onelibrary::NoWriter,
    )
    .unwrap_err();
    assert!(
        matches!(err, DryRunError::Device(DeviceError::NotFound(_))),
        "{err}"
    );
}

/// Plans to a real mounted stick (a FAT32 disk image is fine), e.g.
/// `hdiutil create -size 64m -fs MS-DOS -volname REKSTEST /tmp/reks.dmg`,
/// `hdiutil attach /tmp/reks.dmg`, then
/// `REKS_TEST_STICK=/Volumes/REKSTEST cargo test -p reks-export-rb live_plan_at -- --ignored`.
#[test]
#[ignore = "needs a mounted stick in REKS_TEST_STICK"]
fn live_plan_at_a_mounted_stick() {
    let stick = PathBuf::from(std::env::var("REKS_TEST_STICK").unwrap());
    let tmp = tempfile::tempdir().unwrap();
    let lib = Library::open_in_memory(tmp.path()).unwrap();
    let plan = plan_at(
        &lib,
        &Selection::default(),
        &stick,
        &ExportOptions::default(),
        &crate::onelibrary::NoWriter,
    )
    .unwrap();
    let dry = DryRun::build(&lib, &plan);
    assert_eq!(dry.device.mount_point, stick);
    assert!(dry.device.volume_serial.is_some(), "{:?}", dry.device);
    assert!(!dry.ready);
    assert_eq!(kinds(&dry.problems), ["nothingSelected"]);
    assert_eq!(dry.problems[0].title, "No playlist selected.");
}

#[test]
fn dividers_are_their_own_group() {
    use crate::divider::{self, DividerInfo, SourceDivider};

    let w = World::new();
    let a = w.add("sine-click-128.mp3", "a.mp3");
    let b = w.add("sine-click-128.mp3", "b.mp3");
    let mut src = sources(vec![track(1, a), track(2, b)]);
    src.dividers = ["PEAK", "END"]
        .iter()
        .zip([1, 2])
        .map(|(label, n)| SourceDivider {
            info: DividerInfo {
                playlist_id: 11,
                ordinal: n,
                label: (*label).to_owned(),
            },
            tracks_before: if n == 1 { 1 } else { 2 },
        })
        .collect();
    divider::expand(&mut src, &w.cache.join("dividers")).unwrap();
    let opts = ExportOptions {
        dividers: true,
        ..ExportOptions::default()
    };
    let plan = w.plan(src, &opts, &[1, 2]);
    assert!(!plan.is_blocked(), "{:?}", plan.errors);
    let order: Vec<Option<&str>> = plan.playlists[1]
        .track_ids
        .iter()
        .map(|id| {
            let t = plan.tracks.iter().find(|t| t.track_id == *id).unwrap();
            t.divider.as_ref().map(|d| d.label.as_str())
        })
        .collect();
    assert_eq!(order, [None, Some("PEAK"), None, Some("END")]);

    let dry = DryRun::new(&plan, &labels());
    assert_eq!(
        fact(&dry.summary, "Tracks"),
        "2: 2 to copy, 0 already on the stick"
    );
    let size = human_bytes(2 * divider::wav_bytes().len() as u64);
    assert_eq!(
        fact(&dry.summary, "Dividers"),
        format!(
            "2 dividers, {size}: silent tracks titled with the label, analysis generated \
             (UNVERIFIED on players)"
        )
    );
    assert_eq!(fact(&dry.summary, "Analysis"), "2 from rekordbox (2.0 KB)");
    assert_eq!(dry.playlists[1].detail, "2 tracks, 2 dividers");
    assert_eq!(dry.copies.count, 4);

    let labels: TrackLabels = divider_labels(&plan).collect();
    let d = plan.tracks.iter().find(|t| t.is_divider()).unwrap();
    assert_eq!(labels[&d.track_id], "Divider \"PEAK\"");
}
