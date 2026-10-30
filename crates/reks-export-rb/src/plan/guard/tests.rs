//! The export guard on synthetic played sticks (`testutil::played_pdb`: two
//! undated sessions, 2 + 1 entries), planned and applied end to end.

use std::fs;
use std::path::PathBuf;

use reks_core::history::{self as library_history, ImportChoices, SessionAction};
use reks_core::CancelToken;

use super::*;
use crate::apply::tests::{World, SERIAL};
use crate::apply::{apply, ApplyError, ApplyEvent, ApplyStage, ExportReport, ExportStatus};
use crate::dryrun::{DryRun, TrackLabels};
use crate::history::{read_play_history, HistoryPolicy, HistoryReadOptions, HistorySource};
use crate::onelibrary::NoWriter;
use crate::plan::{self, ExportOptions, MissingAnalysis, NoAnlz};
use crate::report::ReportView;
use crate::testutil;
use crate::writer::BakenWriter;

fn options(history: HistoryPolicy, chosen: bool) -> ExportOptions {
    ExportOptions {
        missing_analysis: MissingAnalysis::Generate,
        history,
        history_chosen: chosen,
        discard_history_confirmed: history == HistoryPolicy::Discard,
        onelibrary: false,
        ..ExportOptions::default()
    }
}

/// What `plan::plan` does, with the test stick's serial.
fn plan_guarded(w: &World, opts: &ExportOptions) -> ExportPlan {
    let sources = plan::load(&w.lib, &w.sel()).unwrap();
    let mut p = plan::plan_sources(
        sources,
        &w.dev(),
        Some(SERIAL.into()),
        opts,
        &NoAnlz,
        &NoAnlz,
    )
    .unwrap();
    guard_history(&mut p, &w.lib).unwrap();
    plan::plan_onelibrary(&mut p, &NoWriter, None, &w.lib).unwrap();
    p
}

fn run(w: &World, plan: &ExportPlan) -> (Result<ExportReport, ApplyError>, Vec<ApplyStage>) {
    let writer = BakenWriter::prepare(&w.lib, &w.sel(), plan, &w.tmp.path().join("work")).unwrap();
    let stages = std::sync::Mutex::new(Vec::new());
    let progress = |e: ApplyEvent| {
        if let ApplyEvent::Stage { stage } = e {
            stages.lock().unwrap().push(stage);
        }
    };
    let result = apply(plan, &writer, &w.env(), &progress, &CancelToken::new());
    (result, stages.into_inner().unwrap())
}

/// The scan of a backup folder: what "import later" would do.
fn scan_backup(w: &World, dir: PathBuf) -> Vec<SessionAction> {
    let h = read_play_history(&HistorySource::Backup(dir), &HistoryReadOptions::default())
        .unwrap()
        .to_source_history();
    library_history::scan(&h, &w.lib)
        .unwrap()
        .sessions
        .into_iter()
        .map(|s| s.action)
        .collect()
}

#[test]
fn unimported_sessions_block_until_a_choice() {
    let w = World::new();
    w.put_played_pdb();
    let plan = plan_guarded(&w, &options(HistoryPolicy::BackUp, false));

    assert_eq!(
        plan.guard.sessions,
        [
            UnimportedSession {
                name: "HISTORY 001".into(),
                entries: 2,
                extends: false,
            },
            UnimportedSession {
                name: "HISTORY 002".into(),
                entries: 1,
                extends: false,
            },
        ]
    );
    assert!(plan.history_import.is_some());
    assert_eq!(plan.errors, [Problem::HistoryNotImported { sessions: 2 }]);

    // apply refuses it, and the stick keeps its history.
    let (result, _) = run(&w, &plan);
    assert!(matches!(result, Err(ApplyError::Blocked(1))), "{result:?}");
    assert_eq!(w.pdb(), testutil::played_pdb());

    // The dry run words the guard and offers the three choices.
    let d = DryRun::new(&plan, &TrackLabels::new());
    assert!(!d.ready);
    let g = d.history.guard.unwrap();
    assert_eq!(g.title, "2 sessions on this stick are not in your library");
    assert!(!g.chosen && g.can_import);
    assert_eq!(g.sessions, ["HISTORY 001: 2 plays", "HISTORY 002: 1 play"]);
    assert!(!g.text.contains("import as practice"), "{}", g.text);
    assert!(
        g.text
            .contains("you can mark any of them as practice in History later"),
        "{}",
        g.text
    );
    assert_eq!(d.problems[0].kind, "historyNotImported");

    // Planning again with the guard is idempotent.
    let mut again = plan.clone();
    guard_history(&mut again, &w.lib).unwrap();
    assert_eq!(again.errors, plan.errors);
}

#[test]
fn import_now_imports_backs_up_then_writes() {
    let w = World::new();
    w.put_played_pdb();
    let plan = plan_guarded(&w, &options(HistoryPolicy::Import, true));
    assert!(!plan.is_blocked(), "{:?}", plan.errors);
    let d = DryRun::new(&plan, &TrackLabels::new());
    let action = d.history.action_text.unwrap();
    assert!(
        action.starts_with("Before writing anything, reks imports 2 sessions (3 plays)"),
        "{action}"
    );

    let (result, stages) = run(&w, &plan);
    let report = result.unwrap();
    assert_eq!(report.status, ExportStatus::Success, "{report:#?}");
    assert_eq!(
        stages[..3],
        [
            ApplyStage::Preflight,
            ApplyStage::ImportHistory,
            ApplyStage::Backup
        ]
    );
    let imported = report.history_import.as_ref().unwrap();
    assert_eq!(imported.created.len(), 2);
    assert_eq!(imported.entries, 3);

    // The backup holds the same sessions, all imported now.
    let dir = report.backup_dir.clone().unwrap();
    let actions = scan_backup(&w, dir);
    assert_eq!(actions.len(), 2);
    assert!(actions.iter().all(|a| matches!(a, SessionAction::Skip(_))));

    let view = ReportView::new(&plan, &Ok(report), &TrackLabels::new());
    assert_eq!(view.history.title, "Play history imported and backed up");
    assert!(view
        .history
        .text
        .starts_with("reks imported 2 sessions (3 plays) into your library's history."));

    // The new stick has no history: the next export needs no choice.
    let next = plan_guarded(&w, &options(HistoryPolicy::BackUp, false));
    assert!(!next.guard.needs_choice());
    assert!(!next.is_blocked(), "{:?}", next.errors);
}

#[test]
fn back_up_and_continue_leaves_a_backup_the_scan_reads() {
    let w = World::new();
    w.put_played_pdb();
    let plan = plan_guarded(&w, &options(HistoryPolicy::BackUp, true));
    assert!(!plan.is_blocked(), "{:?}", plan.errors);

    let (result, stages) = run(&w, &plan);
    let report = result.unwrap();
    assert_eq!(report.status, ExportStatus::Success, "{report:#?}");
    assert!(!stages.contains(&ApplyStage::ImportHistory));
    assert!(report.history_import.is_none());

    // Nothing imported; the backup offers both sessions for import later.
    let dir = report.backup_dir.clone().unwrap();
    assert_eq!(
        fs::read(dir.join("PIONEER/rekordbox/export.pdb")).unwrap(),
        testutil::played_pdb()
    );
    assert_eq!(
        scan_backup(&w, dir),
        [SessionAction::New, SessionAction::New]
    );
}

#[test]
fn continue_and_discard_needs_its_confirmation() {
    let w = World::new();
    w.put_played_pdb();
    let mut opts = options(HistoryPolicy::Discard, true);
    opts.discard_history_confirmed = false;
    let plan = plan_guarded(&w, &opts);
    assert_eq!(plan.errors, [Problem::DiscardNotConfirmed]);

    let plan = plan_guarded(&w, &options(HistoryPolicy::Discard, true));
    assert!(!plan.is_blocked(), "{:?}", plan.errors);
    assert!(plan.backups.is_empty());
    let (result, stages) = run(&w, &plan);
    let report = result.unwrap();
    assert_eq!(report.status, ExportStatus::Success, "{report:#?}");
    assert!(report.backup.is_none() && report.history_import.is_none());
    assert!(!stages.contains(&ApplyStage::ImportHistory));
    assert_ne!(w.pdb(), testutil::played_pdb());
}

#[test]
fn an_imported_stick_needs_no_choice() {
    let w = World::new();
    w.put_played_pdb();
    let plan = plan_guarded(&w, &options(HistoryPolicy::BackUp, false));
    library_history::import(
        plan.history_import.as_ref().unwrap(),
        &ImportChoices::default(),
        &w.lib,
    )
    .unwrap();

    let plan = plan_guarded(&w, &options(HistoryPolicy::BackUp, false));
    assert!(plan.guard.sessions.is_empty());
    assert!(!plan.is_blocked(), "{:?}", plan.errors);
    assert!(DryRun::new(&plan, &TrackLabels::new())
        .history
        .guard
        .is_none());
    // The Phase 2 backup still runs.
    assert_eq!(plan.backups.len(), 1);
}

#[test]
fn import_meanwhile_is_skipped_and_a_plan_without_its_scan_is_refused() {
    let w = World::new();
    w.put_played_pdb();
    let plan = plan_guarded(&w, &options(HistoryPolicy::Import, true));

    // The plan read back from JSON has lost its scan.
    let mut bare = plan.clone();
    bare.history_import = None;
    let (result, _) = run(&w, &bare);
    assert!(
        matches!(result, Err(ApplyError::NoHistoryScan)),
        "{result:?}"
    );
    assert_eq!(w.pdb(), testutil::played_pdb());

    // Imported in the History view between the dry run and the export.
    library_history::import(
        plan.history_import.as_ref().unwrap(),
        &ImportChoices::default(),
        &w.lib,
    )
    .unwrap();
    let (result, _) = run(&w, &plan);
    let report = result.unwrap();
    let imported = report.history_import.unwrap();
    assert_eq!((imported.created.len(), imported.skipped), (0, 2));
}

#[test]
fn a_stick_without_history_sessions_is_not_scanned() {
    let w = World::new();
    let plan = plan_guarded(&w, &options(HistoryPolicy::BackUp, false));
    assert_eq!(plan.guard, HistoryGuard::default());
    assert!(plan.history_import.is_none());
    assert!(!plan.is_blocked(), "{:?}", plan.errors);
}

/// The guard on a stick a player wrote (Phase 2.5 hardware checklist step 8:
/// after importing, the guard finds nothing).
#[test]
#[ignore = "needs fixtures/rekordbox/played-<ver>/ from the hardware checklist (step 7)"]
fn played_stick_fixture_guard() {
    let root = PathBuf::from(std::env::var("REKS_PLAYED_FIXTURE").unwrap());
    let w = World::new();
    let copy = |from: &std::path::Path, to: &std::path::Path| {
        fs::create_dir_all(to.parent().unwrap()).unwrap();
        fs::copy(from, to).unwrap();
    };
    copy(
        &root.join("PIONEER/rekordbox/export.pdb"),
        &w.stick.join("PIONEER/rekordbox/export.pdb"),
    );
    let plan = plan_guarded(&w, &options(HistoryPolicy::Import, true));
    assert!(plan.guard.sessions.len() >= 2);
    let (result, _) = run(&w, &plan);
    assert_eq!(result.unwrap().status, ExportStatus::Success);
}
