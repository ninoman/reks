//! Progress and report wording, end to end on the temp-dir sticks of
//! `apply`'s tests and on reports edited by hand for the rarer outcomes.

use std::path::PathBuf;
use std::sync::Mutex;

use reks_core::CancelToken;

use super::*;
use crate::apply::tests::World;
use crate::apply::ApplyWarning;
use crate::dryrun::plan_track_labels;
use crate::history::HistoryError;
use crate::writer::TrackFailure;

fn fact<'a>(facts: &'a [Fact], label: &str) -> &'a str {
    &facts
        .iter()
        .find(|f| f.label == label)
        .unwrap_or_else(|| panic!("no fact {label}: {facts:?}"))
        .value
}

fn labels() -> TrackLabels {
    [(1, "Alpha – One".to_owned()), (2, "Beta – Two".to_owned())].into()
}

/// Runs [`export`] on `w`'s stick, cancelling after `cancel_after` tracks.
fn run(
    w: &World,
    plan: &ExportPlan,
    cancel_after: Option<usize>,
) -> (ReportView, Vec<ProgressView>) {
    run_for(w, plan, cancel_after, None)
}

/// [`run`] for the stick profile `profile`.
fn run_for(
    w: &World,
    plan: &ExportPlan,
    cancel_after: Option<usize>,
    profile: Option<DeviceId>,
) -> (ReportView, Vec<ProgressView>) {
    let cancel = CancelToken::new();
    let views = Mutex::new(Vec::new());
    let progress = |v: ProgressView| {
        if cancel_after.is_some_and(|n| v.done == n && v.detail.is_some()) {
            cancel.cancel();
        }
        views.lock().unwrap().push(v);
    };
    let labels = plan_track_labels(&w.lib, plan);
    let report = export(
        &w.env(),
        plan,
        &w.sel(),
        labels,
        &progress,
        &cancel,
        profile,
    );
    (report, views.into_inner().unwrap())
}

#[test]
fn progress_follows_the_events() {
    let t = ProgressTracker::new(labels());
    let v = t.update(&ApplyEvent::Stage {
        stage: ApplyStage::Preflight,
    });
    assert_eq!(v.title, "Checking the stick");
    assert!(v.cancellable);
    assert_eq!((v.done, v.total), (0, 0));

    let v = t.update(&ApplyEvent::Warning {
        warning: ApplyWarning::RekordboxRunning,
    });
    assert_eq!(
        v.warnings,
        ["rekordbox is running; quit it while exporting so it does not touch the stick."]
    );

    t.update(&ApplyEvent::Stage {
        stage: ApplyStage::Backup,
    });
    let v = t.update(&ApplyEvent::Stage {
        stage: ApplyStage::Write,
    });
    assert_eq!(v.title, "Writing tracks");
    let v = t.update(&ApplyEvent::Writer {
        event: ExportEvent::Started { total: 3 },
    });
    assert_eq!((v.done, v.total), (0, 3));
    let v = t.update(&ApplyEvent::Writer {
        event: ExportEvent::TrackDone {
            done: 1,
            total: 3,
            track_id: Some(2),
        },
    });
    assert_eq!(v.detail.as_deref(), Some("Track 1 of 3: Beta – Two"));
    assert_eq!((v.done, v.total), (1, 3));
    assert!(v.cancellable);
    let v = t.update(&ApplyEvent::Writer {
        event: ExportEvent::TrackDone {
            done: 2,
            total: 3,
            track_id: Some(9),
        },
    });
    assert_eq!(v.detail.as_deref(), Some("Track 2 of 3: track 9"));
    let v = t.update(&ApplyEvent::Writer {
        event: ExportEvent::TrackDone {
            done: 3,
            total: 3,
            track_id: None,
        },
    });
    assert_eq!(v.detail.as_deref(), Some("Track 3 of 3"));

    // Once the writer replaces the database, cancelling is over.
    let v = t.update(&ApplyEvent::Writer {
        event: ExportEvent::Finished,
    });
    assert!(!v.cancellable);
    assert_eq!(
        v.title,
        "Writing the stick's database and removing stale files"
    );
    assert_eq!(v.detail, None);
    let v = t.update(&ApplyEvent::Stage {
        stage: ApplyStage::Validate,
    });
    assert_eq!(v.title, "Reading the stick back to check it");
    assert!(!v.cancellable);
    assert_eq!((v.done, v.total), (0, 0));
    // Warnings stay for the rest of the run.
    assert_eq!(v.warnings.len(), 1);

    let json = serde_json::to_value(&v).unwrap();
    assert_eq!(json["stage"], "validate");
    assert_eq!(json["cancellable"], false);
}

#[test]
fn played_stick_reports_success_and_the_backup() {
    let w = World::new();
    w.put_played_pdb();
    let plan = w.plan();
    let (r, views) = run(&w, &plan, None);

    assert_eq!(r.outcome, Outcome::Success, "{r:#?}");
    assert_eq!(r.title, "Export finished");
    assert!(r
        .status
        .starts_with("reks wrote the stick and read it back"));
    assert_eq!(r.device_name, "REKS");
    assert_eq!(r.mount_point, w.stick);
    assert!(r.problems.is_empty());

    assert_eq!(fact(&r.facts, "Tracks"), "3 in the stick's database");
    assert_eq!(fact(&r.facts, "Audio"), "3 copied, 0 already on the stick");
    assert!(fact(&r.facts, "Analysis").ends_with("; 3 tracks generated (approximate)"));
    assert!(fact(&r.facts, "Read back").starts_with("3 tracks and "));
    assert_eq!(
        fact(&r.facts, "Library record"),
        "3 tracks recorded as on this stick"
    );

    let h = &r.history;
    assert_eq!(h.title, "Play history backed up");
    assert!(h
        .text
        .starts_with("Before writing, reks copied 1 database file ("));
    assert!(h
        .text
        .ends_with("The stick's new database starts without history."));
    let folder = h.folder.clone().unwrap();
    assert!(folder.starts_with(w.data.join("stick-backups")));
    assert!(folder.join("PIONEER/rekordbox/export.pdb").is_file());
    assert_eq!(h.files, ["/PIONEER/rekordbox/export.pdb"]);

    let titles: Vec<&str> = views.iter().map(|v| v.title.as_str()).collect();
    assert_eq!(titles.first(), Some(&"Checking the stick"));
    assert_eq!(titles.last(), Some(&"Recording what is on the stick"));
    assert!(views
        .iter()
        .any(|v| v.detail.as_deref() == Some("Track 3 of 3: Gamma – Three")));
}

#[test]
fn cancelled_export_keeps_the_history_on_the_stick() {
    let w = World::new();
    w.put_played_pdb();
    let old_pdb = w.pdb();
    let plan = w.plan();
    let (r, views) = run(&w, &plan, Some(1));

    assert_eq!(r.outcome, Outcome::Cancelled, "{r:#?}");
    assert_eq!(r.title, "Export cancelled");
    assert!(r
        .status
        .starts_with("The stick keeps its previous database."));
    assert!(r.facts.is_empty());
    assert_eq!(r.history.title, "Play history backed up");
    assert!(r
        .history
        .text
        .ends_with("The stick still has them too: its database was not replaced."));
    assert!(r.history.folder.is_some());
    assert_eq!(w.pdb(), old_pdb);
    assert!(views.iter().all(|v| v.cancellable));
}

/// A stick profile for `w`'s stick with volume ID `serial`, holding `w.sel()`.
fn profile(w: &World, serial: &str) -> DeviceId {
    let dev = devices::create(
        &w.lib,
        &w.dev(),
        Some(serial),
        "Main",
        crate::TargetProfile::Generic,
    )
    .unwrap();
    devices::set_selection(&w.lib, dev.id, &w.sel().playlists).unwrap();
    dev.id
}

/// Warnings about the profile record (the machine may add others, such as
/// rekordbox running).
fn profile_warnings(r: &ReportView) -> Vec<&NoticeGroup> {
    r.warnings
        .iter()
        .filter(|g| g.kind == "profileNotRecorded")
        .collect()
}

#[test]
fn export_records_the_sync_in_the_stick_profile() {
    let w = World::new();
    let dev = profile(&w, crate::apply::tests::SERIAL);
    let plan = w.plan();
    let (r, _) = run_for(&w, &plan, None, Some(dev));

    assert_eq!(r.outcome, Outcome::Success, "{r:#?}");
    assert_eq!(
        fact(&r.facts, "Stick profile"),
        "2 playlists recorded for the Devices panel"
    );
    assert!(profile_warnings(&r).is_empty(), "{:?}", r.warnings);
    let synced = w.lib.device_synced(dev).unwrap();
    let paths: Vec<_> = synced.iter().map(|s| s.playlist.path.join("/")).collect();
    assert_eq!(paths, ["Set/Peak", "Set/Chill"]);
    assert!(synced.iter().all(|s| s.playlist.playlist_id.is_some()));
    assert!(w.lib.device(dev).unwrap().last_synced_at.is_some());
}

#[test]
fn profile_of_another_stick_is_a_warning_not_a_failure() {
    let w = World::new();
    let dev = profile(&w, "0000-0000");
    let plan = w.plan();
    let (r, _) = run_for(&w, &plan, None, Some(dev));

    assert_eq!(r.outcome, Outcome::Success, "{r:#?}");
    assert_eq!(
        fact(&r.facts, "Stick profile"),
        "Not recorded (see the warnings)"
    );
    let warned = profile_warnings(&r);
    assert_eq!(warned.len(), 1);
    assert!(warned[0].title.contains("went to another stick"));
    assert!(w.lib.device_synced(dev).unwrap().is_empty());
}

#[test]
fn cancelled_export_records_nothing_in_the_profile() {
    let w = World::new();
    let dev = profile(&w, crate::apply::tests::SERIAL);
    let plan = w.plan();
    let (r, _) = run_for(&w, &plan, Some(1), Some(dev));

    assert_eq!(r.outcome, Outcome::Cancelled, "{r:#?}");
    assert!(r.facts.is_empty());
    assert!(profile_warnings(&r).is_empty());
    assert!(w.lib.device_synced(dev).unwrap().is_empty());
}

#[test]
fn refused_export_fails_with_the_reason() {
    let w = World::new();
    let plan = w.plan();
    // Played between the dry run and Export.
    w.put_played_pdb();
    let (r, _) = run(&w, &plan, None);

    assert_eq!(r.outcome, Outcome::Failed);
    assert_eq!(r.title, "Export failed");
    assert!(
        r.status
            .starts_with("The stick changed since the dry run (play history was 0 sessions"),
        "{}",
        r.status
    );
    assert!(r
        .status
        .ends_with("; plan the export again. The stick keeps its previous database."));
    assert!(r.facts.is_empty());
    assert_eq!(r.history.title, "No play history on this stick");
}

#[test]
fn backup_failure_says_nothing_was_written() {
    let w = World::new();
    w.put_played_pdb();
    let plan = w.plan();
    let err = ApplyError::Backup(HistoryError::Io {
        path: PathBuf::from("/data/stick-backups"),
        source: std::io::Error::other("disk full"),
    });
    let r = ReportView::new(&plan, &Err(err), &labels());
    assert_eq!(r.outcome, Outcome::Failed);
    assert!(r
        .status
        .starts_with("Backing up the stick's play history failed, nothing was written: "));
    assert_eq!(r.history.title, "Play history backup failed");
    assert_eq!(r.history.folder, None);
}

#[test]
fn problems_are_grouped_with_track_names() {
    let w = World::new();
    let plan = w.plan();
    let mut report = w.export();
    let id = plan.tracks[0].track_id;
    let writer = report.writer.as_mut().unwrap();
    writer.failures = vec![
        TrackFailure {
            track_id: Some(id),
            title: "One".into(),
            message: "source unreadable".into(),
        },
        TrackFailure {
            track_id: None,
            title: "Dup".into(),
            message: "title matched two tracks".into(),
        },
    ];
    writer.prune_skipped = true;
    let v = report.validation.as_mut().unwrap();
    v.findings = vec![
        Finding::TrackNotInPdb {
            track_id: id,
            path: "/Contents/Alpha/one.mp3".into(),
        },
        Finding::AnlzMissing {
            track_id: id,
            path: "/PIONEER/USBANLZ/P000/0000/ANLZ0000.DAT".into(),
        },
        Finding::PlaylistMissing {
            path: "Set/Peak".into(),
        },
        Finding::PdbMissing,
    ];
    report.errors = vec!["fsync failed: /x: busy".into()];
    report.status = ExportStatus::Problems;
    report.manifest = ManifestStatus::Skipped;
    report.manifest_rows = 0;
    let labels = [(id, "Alpha – One".to_owned())].into();
    let r = ReportView::new(&plan, &Ok(report), &labels);

    assert_eq!(r.outcome, Outcome::Problems);
    assert_eq!(r.title, "Export finished with problems");
    let groups: Vec<(&str, &str, Vec<&str>)> = r
        .problems
        .iter()
        .map(|g| {
            (
                g.kind.as_str(),
                g.title.as_str(),
                g.items.iter().map(String::as_str).collect(),
            )
        })
        .collect();
    assert_eq!(
        groups,
        [
            (
                "trackFailed",
                "2 tracks could not be written",
                vec![
                    "Alpha – One: source unreadable",
                    "Dup: title matched two tracks"
                ]
            ),
            (
                "trackFinding",
                "Read back: 2 track problems",
                vec![
                    "Alpha – One: /Contents/Alpha/one.mp3 is not in the stick's database",
                    "Alpha – One: analysis /PIONEER/USBANLZ/P000/0000/ANLZ0000.DAT is not on \
                     the stick",
                ]
            ),
            (
                "playlistFinding",
                "Read back: 1 playlist problem",
                vec!["Playlist Set/Peak is not on the stick"]
            ),
            (
                "databaseFinding",
                "Read back: 1 problem with the database",
                vec!["No export.pdb on the stick"]
            ),
            (
                "error",
                "1 error after the new database was written",
                vec!["Fsync failed: /x: busy"]
            ),
        ]
    );
    assert_eq!(
        fact(&r.facts, "Stale files"),
        "Kept: tracks failed, so nothing was removed"
    );
    assert_eq!(fact(&r.facts, "Read back"), "4 problems found");
    assert_eq!(
        fact(&r.facts, "Library record"),
        "Not recorded: the export has problems"
    );
}

#[test]
fn discarded_history_and_warnings() {
    let w = World::new();
    let mut plan = w.plan();
    let mut report = w.export();
    // As if the stick had history and the user confirmed Discard.
    plan.history.pdb_sessions = 2;
    plan.options.history = HistoryPolicy::Discard;
    plan.options.discard_history_confirmed = true;
    report.backup = None;
    report.backup_dir = None;
    report.warnings = vec![ApplyWarning::RekordboxRunning];
    report.manifest = ManifestStatus::NoVolumeSerial;
    let r = ReportView::new(&plan, &Ok(report), &labels());

    assert_eq!(r.history.title, "Play history discarded");
    assert_eq!(
        r.history.text,
        "As you chose, the stick's play history was erased without a backup."
    );
    assert_eq!(r.history.folder, None);
    assert_eq!(
        fact(&r.facts, "Library record"),
        "Not recorded: the system gave no volume ID for this stick"
    );
    assert_eq!(r.warnings.len(), 1);
    assert_eq!(
        r.warnings[0].title,
        "rekordbox is running; quit it while exporting so it does not touch the stick."
    );

    let json = serde_json::to_value(&r).unwrap();
    assert_eq!(json["outcome"], "success");
    assert_eq!(json["history"]["folder"], serde_json::Value::Null);
    assert!(json["mountPoint"].is_string());
}

/// The UI's path on a real mounted FAT32 volume: `plan_at`, then [`export`]
/// with [`crate::StickCheck::Mounted`] (volume serial and free space from
/// the OS). Puts a played `export.pdb` on the stick first, so the backup
/// runs too. Run against a disk image:
/// `hdiutil create -size 64m -fs MS-DOS -volname REKSTEST /tmp/reks.dmg`,
/// `hdiutil attach /tmp/reks.dmg`, then
/// `REKS_TEST_STICK=/Volumes/REKSTEST cargo test -p reks-export-rb live_export -- --ignored`.
#[test]
#[ignore = "writes to the mounted stick in REKS_TEST_STICK"]
fn live_export_to_a_mounted_stick() {
    use crate::plan::{ExportOptions, MissingAnalysis};

    let stick = PathBuf::from(std::env::var("REKS_TEST_STICK").unwrap());
    let w = World::new();
    let rb = stick.join("PIONEER/rekordbox");
    std::fs::create_dir_all(&rb).unwrap();
    std::fs::write(rb.join("export.pdb"), crate::testutil::played_pdb()).unwrap();
    let opts = ExportOptions {
        missing_analysis: MissingAnalysis::Generate,
        ..ExportOptions::default()
    };
    let plan = crate::dryrun::plan_at(
        &w.lib,
        &w.sel(),
        &stick,
        &opts,
        &crate::onelibrary::NoWriter,
    )
    .unwrap();
    assert!(plan.history.volume_serial.is_some());
    let env = ApplyEnv {
        library: &w.lib,
        app_data_dir: &w.data,
        stick: crate::StickCheck::Mounted,
        onelibrary: None,
    };
    let views = Mutex::new(Vec::new());
    let labels = plan_track_labels(&w.lib, &plan);
    let r = export(
        &env,
        &plan,
        &w.sel(),
        labels,
        &|v| views.lock().unwrap().push(v),
        &CancelToken::new(),
        None,
    );
    assert_eq!(r.outcome, Outcome::Success, "{r:#?}");
    assert_eq!(r.mount_point, stick);
    assert_eq!(r.history.title, "Play history backed up");
    assert!(r
        .history
        .folder
        .as_ref()
        .unwrap()
        .join("manifest.json")
        .is_file());
    assert_eq!(
        fact(&r.facts, "Library record"),
        "3 tracks recorded as on this stick"
    );
    assert!(views
        .lock()
        .unwrap()
        .iter()
        .any(|v| v.detail.as_deref() == Some("Track 3 of 3: Gamma – Three")));
}
