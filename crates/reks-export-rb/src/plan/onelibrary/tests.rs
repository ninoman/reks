//! The OneLibrary step on temp-dir sticks: the toggle, the key, the stub and
//! the export guard over the stick's `exportLibrary.db`.

use std::fs;

use reks_core::{CancelToken, OneLibraryKey};

use super::*;
use crate::apply::tests::{World, SERIAL};
use crate::apply::{apply, ApplyError};
use crate::dryrun::{DryRun, TrackLabels};
use crate::history::HistoryPolicy;
use crate::onelibrary::{auxiliary_files, db_write, NoWriter, OneLibraryPlan, DB_PATH};
use crate::plan::{self, guard_history, ExportOptions, MissingAnalysis, NoAnlz, OpReason};
use crate::writer::BakenWriter;

/// Stands in for the writer: plans the file and reports `existing`.
struct Fake(ExistingLibrary);

impl OneLibraryExport for Fake {
    fn plan(&self, plan: &ExportPlan, _key: &OneLibraryKey) -> Result<OneLibraryPlan, PlanError> {
        Ok(OneLibraryPlan {
            write: db_write(40_960),
            removes: auxiliary_files(&plan.device.mount_point)?,
            existing: self.0.clone(),
            warnings: Vec::new(),
            errors: Vec::new(),
        })
    }
}

fn key() -> OneLibraryKey {
    OneLibraryKey::new("test-key").unwrap()
}

fn options(onelibrary: bool) -> ExportOptions {
    ExportOptions {
        missing_analysis: MissingAnalysis::Generate,
        onelibrary,
        ..ExportOptions::default()
    }
}

/// `plan::plan_sources` + the guard, as `dryrun::plan_at` runs them.
fn planned(w: &World, opts: &ExportOptions) -> ExportPlan {
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
    p
}

/// rekordbox's OneLibrary files on the test stick.
fn put_rekordbox_onelibrary(w: &World) {
    let rb = w.stick.join("PIONEER/rekordbox");
    fs::create_dir_all(&rb).unwrap();
    for name in ["exportLibrary.db", "exportLibrary.db-wal", "exportExt.pdb"] {
        fs::write(rb.join(name), name).unwrap();
    }
}

fn is_onelibrary(p: &Problem) -> bool {
    matches!(
        p,
        Problem::OneLibraryKeyMissing
            | Problem::OneLibraryUnavailable
            | Problem::OneLibraryHistoryNotImported { .. }
    )
}

#[test]
fn off_by_default_and_off_adds_nothing() {
    assert!(!ExportOptions::default().onelibrary);
    let w = World::new();
    let mut p = planned(&w, &options(false));
    let before = p.clone();
    plan_onelibrary(&mut p, &Fake(ExistingLibrary::Absent), Some(&key())).unwrap();
    assert_eq!(p, before);
    assert!(p.onelibrary.is_none());
}

#[test]
fn on_without_a_key_blocks_and_says_where_the_key_goes() {
    let w = World::new();
    let mut p = planned(&w, &options(true));
    plan_onelibrary(&mut p, &Fake(ExistingLibrary::Absent), None).unwrap();
    assert!(p.onelibrary.is_none());
    assert!(p.warnings.contains(&Warning::OneLibraryExperimental));
    assert_eq!(p.errors, [Problem::OneLibraryKeyMissing]);
    assert!(Problem::OneLibraryKeyMissing
        .to_string()
        .contains("Settings → Experimental"));
}

#[test]
fn on_with_a_key_lists_the_database_and_the_files_it_removes() {
    let w = World::new();
    put_rekordbox_onelibrary(&w);
    let mut p = planned(&w, &options(true));
    plan_onelibrary(&mut p, &Fake(ExistingLibrary::Absent), Some(&key())).unwrap();

    assert!(!p.is_blocked(), "{:?}", p.errors);
    let ol = p.onelibrary.as_ref().unwrap();
    assert_eq!(ol.write.dst_rel, DB_PATH);
    assert_eq!(ol.write.reason, OpReason::OneLibrary);
    let removes: Vec<&str> = ol.removes.iter().map(|f| f.dst_rel.as_str()).collect();
    assert_eq!(
        removes,
        [
            "/PIONEER/rekordbox/exportLibrary.db-wal",
            "/PIONEER/rekordbox/exportExt.pdb"
        ]
    );
    assert_eq!(
        p.warnings
            .iter()
            .filter(|w| **w == Warning::OneLibraryExperimental)
            .count(),
        1
    );

    // The dry run shows the file and the removals.
    let d = DryRun::new(&p, &TrackLabels::new());
    let view = d.onelibrary.unwrap();
    assert_eq!(view.write.path, DB_PATH);
    assert_eq!(view.removes.count, 2);
    assert!(d
        .warnings
        .iter()
        .any(|g| g.kind == "oneLibraryExperimental"));

    // Planning again replaces, not adds.
    let once = p.clone();
    plan_onelibrary(&mut p, &Fake(ExistingLibrary::Absent), Some(&key())).unwrap();
    assert_eq!(p, once);
}

#[test]
fn the_stub_plans_the_file_but_blocks() {
    let w = World::new();
    put_rekordbox_onelibrary(&w);
    let mut p = planned(&w, &options(true));
    plan_onelibrary(&mut p, &NoWriter, Some(&key())).unwrap();
    assert_eq!(p.errors, [Problem::OneLibraryUnavailable]);
    let ol = p.onelibrary.as_ref().unwrap();
    assert_eq!(ol.write.dst_rel, DB_PATH);
    assert!(matches!(ol.existing, ExistingLibrary::NotRead { .. }));
    // Not read: the Phase 2 safeguard backs the database up, no guard choice.
    assert_eq!(p.guard.onelibrary_sessions, 0);
    assert!(p
        .backups
        .iter()
        .any(|b| b.dst_rel == "/PIONEER/rekordbox/exportLibrary.db"));

    // Turned off again, everything the step added goes.
    p.options.onelibrary = false;
    plan_onelibrary(&mut p, &NoWriter, Some(&key())).unwrap();
    assert!(p.onelibrary.is_none());
    assert!(!p.errors.iter().any(is_onelibrary));
    assert!(!p.warnings.contains(&Warning::OneLibraryExperimental));
}

#[test]
fn history_in_the_existing_database_needs_a_choice() {
    let w = World::new();
    put_rekordbox_onelibrary(&w);
    let read = Fake(ExistingLibrary::Read {
        sessions: 2,
        entries: 5,
    });

    let mut p = planned(&w, &options(true));
    plan_onelibrary(&mut p, &read, Some(&key())).unwrap();
    assert_eq!(
        p.errors,
        [Problem::OneLibraryHistoryNotImported { sessions: 2 }]
    );
    assert_eq!(p.guard.onelibrary_sessions, 2);
    assert!(p.guard.needs_choice());
    let g = DryRun::new(&p, &TrackLabels::new()).history.guard.unwrap();
    assert_eq!(g.title, "2 sessions on this stick are not in your library");
    assert_eq!(g.sessions, ["exportLibrary.db: 2 sessions"]);
    assert!(g.text.contains("can't be imported yet"), "{}", g.text);

    // The export guard running again keeps the count and adds no
    // export.pdb problem (that stick has no export.pdb sessions).
    guard_history(&mut p, &w.lib).unwrap();
    assert_eq!(p.guard.onelibrary_sessions, 2);
    assert_eq!(
        p.errors,
        [Problem::OneLibraryHistoryNotImported { sessions: 2 }]
    );

    // apply refuses it, and the stick keeps its database.
    let writer = BakenWriter::prepare(&w.lib, &w.sel(), &p, &w.tmp.path().join("work")).unwrap();
    let result = apply(&p, &writer, &w.env(), &|_| {}, &CancelToken::new());
    assert!(matches!(result, Err(ApplyError::Blocked(1))), "{result:?}");
    assert!(w.stick.join("PIONEER/rekordbox/exportLibrary.db").exists());

    // Back up and continue: no blocker, and the database is backed up.
    let opts = ExportOptions {
        history: HistoryPolicy::BackUp,
        history_chosen: true,
        ..options(true)
    };
    let mut p = planned(&w, &opts);
    plan_onelibrary(&mut p, &read, Some(&key())).unwrap();
    assert!(!p.is_blocked(), "{:?}", p.errors);
    assert!(p
        .backups
        .iter()
        .any(|b| b.dst_rel == "/PIONEER/rekordbox/exportLibrary.db"));
}

#[test]
fn apply_refuses_a_onelibrary_plan_until_the_writer_is_wired() {
    let w = World::new();
    let mut p = planned(&w, &options(true));
    plan_onelibrary(&mut p, &Fake(ExistingLibrary::Absent), Some(&key())).unwrap();
    assert!(!p.is_blocked(), "{:?}", p.errors);
    let writer = BakenWriter::prepare(&w.lib, &w.sel(), &p, &w.tmp.path().join("work")).unwrap();
    let result = apply(&p, &writer, &w.env(), &|_| {}, &CancelToken::new());
    assert!(
        matches!(result, Err(ApplyError::OneLibraryNotWired)),
        "{result:?}"
    );
    assert!(!w.stick.join("PIONEER/rekordbox/export.pdb").exists());
}
