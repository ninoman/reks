use std::fs;
use std::path::{Path, PathBuf};

use reks_core::{CancelToken, TrackPatch};

use super::*;
use crate::plan::{
    plan_sources, MissingAnalysis, NoAnlz, PlannedPlaylist, SkipReason, SourceTrack, Sources,
};

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
            fs: crate::device::FsKind::Fat32,
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
        cues: Default::default(),
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
                left_out: Vec::new(),
            },
            PlannedPlaylist {
                id: 11,
                parent_id: Some(10),
                name: "Friday".into(),
                is_folder: false,
                track_ids: tracks.iter().map(|t| t.track_id).collect(),
                smart: false,
                left_out: Vec::new(),
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
    let o = &dry.overview;
    assert_eq!(o.headline, "Ready to export to REKS");
    assert_eq!(o.playlists_text, "1 playlist in 1 folder");
    assert_eq!(o.copy_text, "2 tracks to copy");
    assert_eq!(o.copy_detail, human_bytes(audio));
    assert!(o.fits);
    let free_after = (1u64 << 30) - (audio + 2000);
    assert_eq!(
        o.space_text,
        format!("{} free after export", human_bytes(free_after))
    );
    assert_eq!(o.space_detail, "");
    assert_eq!((o.tracks_exported, o.tracks_total), (2, 2));
    assert_eq!(o.export_label, "Export");
    assert!(dry.issues.is_empty());
    assert_eq!(
        dry.settings,
        [
            "Any player",
            "rekordbox analysis, then reks",
            "Unanalysed tracks approximated",
            "Cleans up unused files",
            "Dividers"
        ]
    );

    assert!(!dry.history.at_risk);
    assert_eq!(dry.history.summary, "");
    assert!(dry.problems.is_empty());
    assert!(dry.warnings.is_empty(), "{:?}", dry.warnings);
}

#[test]
fn stale_files_free_space() {
    let w = World::new();
    w.stick_file("/Contents/Old/Gone/old.mp3", &[0u8; 2500]);
    let a = w.add("sine-click-128.wav", "a.wav");
    let plan = w.plan(sources(vec![track(1, a)]), &ExportOptions::default(), &[1]);
    let dry = DryRun::new(&plan, &labels());

    assert_eq!(dry.overview.space_detail, "frees 2.5 KB");
}

#[test]
fn missing_tracks_are_issues_and_space_still_blocks() {
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
    assert_eq!(kinds(&dry.problems), ["notEnoughSpace"]);
    let missing = &dry.issues[0];
    assert_eq!(missing.kind, IssueKind::Missing);
    assert_eq!(missing.title, "2 tracks are missing");
    assert_eq!(missing.track_ids, [1, 2]);
    assert_eq!(
        missing.actions,
        [IssueAction::FindFiles, IssueAction::RemoveFromPlaylists]
    );
    let rows: Vec<(String, String)> = missing
        .tracks
        .iter()
        .map(|t| (t.label.clone(), t.detail.clone()))
        .collect();
    assert_eq!(
        rows,
        [
            (
                "Alpha – First".to_owned(),
                w.music.join("gone-1.mp3").display().to_string()
            ),
            (
                "Alpha – Second".to_owned(),
                w.music.join("gone-2.mp3").display().to_string()
            ),
        ]
    );
    assert_eq!(missing.tracks[0].playlists, [11]);
    assert_eq!(dry.overview.export_label, "Export 1 of 3 tracks");
    assert_eq!(dry.overview.headline, "Can't export to REKS yet");
    let space = &dry.problems[0];
    assert!(space.items.is_empty());
    assert!(
        space
            .title
            .starts_with("Not enough space: the export writes "),
        "{}",
        space.title
    );
    assert!(space.title.ends_with("the stick has 10 B free."));

    assert!(!dry.overview.fits);
    assert!(
        dry.overview.space_text.ends_with(" short"),
        "{}",
        dry.overview.space_text
    );
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
        left_out: Vec::new(),
    });
    let plan = plan_sources(
        src,
        &w.dev(),
        None,
        &ExportOptions {
            missing_analysis: MissingAnalysis::Skip,
            ..ExportOptions::default()
        },
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
    // No volume serial is a technical note: not in the review.
    assert!(plan.warnings.contains(&Warning::NoVolumeSerial));
    assert_eq!(kinds(&dry.warnings), ["emptyPlaylist"]);

    let skipped = &dry.issues[0];
    assert_eq!(skipped.kind, IssueKind::NoAnalysis);
    assert_eq!(skipped.title, "2 tracks have no analysis");
    let rows: Vec<(&str, &str, &str)> = skipped
        .tracks
        .iter()
        .map(|t| (t.label.as_str(), t.detail.as_str(), t.in_text.as_str()))
        .collect();
    let why = "no rekordbox analysis found; reks has not analysed it yet";
    assert_eq!(
        rows,
        [
            ("Alpha – Second", why, "In: Friday"),
            ("track 7", why, "In: Friday, Saturday"),
        ]
    );
    assert_eq!(dry.overview.export_label, "Export 1 of 3 tracks");
    assert_eq!(
        dry.warnings[0].title,
        "1 playlist ends up empty after leaving tracks out"
    );
    assert_eq!(dry.warnings[0].items, ["Saturday"]);
    assert_eq!(dry.overview.copy_text, "1 track to copy");
}

#[test]
fn generated_analysis_is_an_issue() {
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
    let issue = &dry.issues[0];
    assert_eq!(issue.kind, IssueKind::ApproximateAnalysis);
    assert_eq!(issue.title, "1 track gets approximate analysis");
    assert_eq!(issue.actions, [IssueAction::Analyse, IssueAction::LeaveOut]);
    // Approximate analysis leaves nothing out.
    assert_eq!(dry.overview.export_label, "Export");
}

#[test]
fn played_stick_says_history_is_saved_first() {
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
    // The history warning is shown in `history`, not twice; rekordbox's own
    // stick marker is a technical note, not in the review.
    assert!(plan
        .warnings
        .iter()
        .any(|w| matches!(w, Warning::RekordboxStick { .. })));
    assert!(dry.warnings.is_empty());
    assert_eq!(
        h.summary,
        "Your stick's play history is saved to your library before anything is written."
    );
    // Backups are still planned (in the plan, not the review).
    assert_eq!(plan.backups.len(), 2);
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
        .contains("confirmation under the play history"));
    assert_eq!(
        dry.history.summary,
        "Discarding your stick's play history needs your confirmation."
    );
    assert!(plan.backups.is_empty());

    let confirmed = ExportOptions {
        discard_history_confirmed: true,
        ..discard
    };
    let plan = w.plan(sources(vec![track(1, a)]), &confirmed, &[1]);
    let dry = DryRun::new(&plan, &labels());
    assert!(dry.ready);
    assert_eq!(
        dry.history.summary,
        "Your stick's play history will be erased, as you chose."
    );
    assert!(plan.backups.is_empty());
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
        "overview",
        "issues",
        "settings",
        "history",
        "problems",
        "warnings",
        "convertible",
    ] {
        assert!(json.get(key).is_some(), "{key}");
    }
    assert!(json["overview"]["exportLabel"].is_string());
    assert!(json["overview"]["tracksExported"].is_u64());
    assert_eq!(json["history"]["policy"], "backUp");
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
            reason: SkipReason::Unsupported,
            detail: "x".into(),
        },
        Warning::Audio {
            track_id: 999,
            message: "y".into(),
        },
    ];
    plan.errors = vec![Problem::BadPath {
        track_id: second,
        path: "/p".into(),
        message: "z".into(),
    }];
    let labels = track_labels(&lib, &plan);
    assert_eq!(labels.len(), 2);
    assert_eq!(labels[&first], "Кино – Звезда");
    assert_eq!(labels[&second], second_file);

    let dry = DryRun::build(&lib, &plan);
    // Left-out tracks are issues; audio notes are not in the review.
    assert!(dry.warnings.is_empty(), "{:?}", dry.warnings);
    assert_eq!(dry.issues[0].kind, IssueKind::Unsupported);
    assert_eq!(dry.issues[0].tracks[0].label, "Кино – Звезда");
    assert_eq!(dry.issues[0].tracks[0].detail, "x");
    assert_eq!(dry.problems[0].items, [format!("{second_file}: /p: z")]);
    assert!(dry.convertible.is_empty());
    assert_eq!(dry.convert_text, None);

    // A left-out track's Convert is on its issue, not in the general offer.
    plan.convertible = vec![first, second];
    let dry = DryRun::build(&lib, &plan);
    assert_eq!(dry.convertible, [second]);
    assert!(dry
        .convert_text
        .as_deref()
        .is_some_and(|t| t.starts_with("1 track may not play on older players")));
    // A stick for newer players only gets no "older players" offer.
    plan.options.target = TargetProfile::Cdj2000Nxs2;
    let dry = DryRun::build(&lib, &plan);
    assert_eq!(dry.convert_text, None);
    assert_eq!(dry.convertible, [second]);
    assert_eq!(
        dry.issues[0].actions,
        [
            IssueAction::Convert {
                track_ids: vec![first]
            },
            IssueAction::RemoveFromPlaylists
        ]
    );
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
    assert_eq!(plan.device.mount_point, stick);
    assert!(plan.history.volume_serial.is_some(), "{:?}", plan.device);
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

    // Dividers are not tracks in the overview, nor approximate-analysis issues.
    let dry = DryRun::new(&plan, &labels());
    assert_eq!(dry.overview.copy_text, "2 tracks to copy");
    assert_eq!(
        (dry.overview.tracks_exported, dry.overview.tracks_total),
        (2, 2)
    );
    assert!(dry.issues.is_empty(), "{:?}", dry.issues);
    assert_eq!(plan.copies.len(), 4);

    let labels: TrackLabels = divider_labels(&plan).collect();
    let d = plan.tracks.iter().find(|t| t.is_divider()).unwrap();
    assert_eq!(labels[&d.track_id], "Divider \"PEAK\"");
}

#[test]
fn file_group_lists_the_first_rows_but_counts_all() {
    let n = FILE_LIST_CAP + 50;
    let ops: Vec<FileOp> = (0..n)
        .map(|i| FileOp {
            src: None,
            dst_rel: format!("/Contents/{i}.mp3"),
            bytes: 10,
            track_id: None,
            reason: crate::plan::OpReason::NotOnStick,
        })
        .collect();
    let g = file_group(&ops);
    assert_eq!(g.files.len(), FILE_LIST_CAP);
    assert_eq!(g.files[0].path, "/Contents/0.mp3");
    assert_eq!(g.count, n);
    assert_eq!(g.bytes, 10 * n as u64);
    assert_eq!(g.total_text, total_text(n, 10 * n as u64));
}
