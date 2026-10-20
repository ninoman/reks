//! The OneLibrary step on temp-dir sticks: the toggle, the key, the stub,
//! the export guard over the stick's `exportLibrary.db`, and `apply` writing
//! it after `export.pdb`.

use std::fs;
use std::path::Path;
use std::sync::Mutex;

use reks_core::{CancelToken, OneLibraryKey};

use super::*;
use crate::apply::tests::{World, SERIAL};
use crate::apply::{
    apply, ApplyEnv, ApplyError, ApplyEvent, ApplyStage, ExportReport, ExportStatus,
    ManifestStatus, OneLibraryEnv,
};
use crate::dryrun::{DryRun, TrackLabels};
use crate::history::HistoryPolicy;
use crate::onelibrary::{
    auxiliary_files, db_write, NoWriter, OneLibraryPlan, OneLibraryWriteError, OneLibraryWritten,
    DB_PATH,
};
use crate::plan::{
    self, guard_history, on_stick, ExportOptions, MissingAnalysis, NoAnlz, OpReason,
};
use crate::report::ReportView;
use crate::writer::BakenWriter;

const FAKE_DB: &[u8] = b"fake exportLibrary.db";

/// Stands in for the writer: plans the file and reports `existing`; `write`
/// records the files in `PIONEER/rekordbox` it found, then writes
/// [`FAKE_DB`] (or fails, with `fail`).
struct Fake {
    existing: ExistingLibrary,
    fail: bool,
    seen: Mutex<Option<Vec<String>>>,
}

impl Fake {
    fn new(existing: ExistingLibrary) -> Self {
        Fake {
            existing,
            fail: false,
            seen: Mutex::new(None),
        }
    }

    fn failing() -> Self {
        Fake {
            fail: true,
            ..Fake::new(ExistingLibrary::Absent)
        }
    }

    fn seen(&self) -> Option<Vec<String>> {
        self.seen.lock().unwrap().clone()
    }
}

impl OneLibraryExport for Fake {
    fn plan(&self, plan: &ExportPlan, _key: &OneLibraryKey) -> Result<OneLibraryPlan, PlanError> {
        Ok(OneLibraryPlan {
            write: db_write(40_960),
            removes: auxiliary_files(&plan.device.mount_point)?,
            existing: self.existing.clone(),
            warnings: Vec::new(),
            errors: Vec::new(),
        })
    }

    fn write(
        &self,
        plan: &ExportPlan,
        key: &OneLibraryKey,
        device_root: &Path,
    ) -> Result<OneLibraryWritten, OneLibraryWriteError> {
        assert_eq!(key.expose(), "test-key");
        *self.seen.lock().unwrap() = Some(rekordbox_files(device_root));
        if self.fail {
            return Err(OneLibraryWriteError::Build("integrity check failed".into()));
        }
        let path = on_stick(device_root, DB_PATH);
        fs::write(&path, FAKE_DB).map_err(|source| OneLibraryWriteError::Io { path, source })?;
        Ok(OneLibraryWritten {
            tracks: plan.tracks.len(),
            bytes: FAKE_DB.len() as u64,
        })
    }
}

/// The file names in the stick's `PIONEER/rekordbox`, sorted.
fn rekordbox_files(root: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(root.join("PIONEER/rekordbox"))
        .map(|dir| {
            dir.flatten()
                .filter(|e| e.path().is_file())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
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
    plan_onelibrary(&mut p, &Fake::new(ExistingLibrary::Absent), Some(&key())).unwrap();
    assert_eq!(p, before);
    assert!(p.onelibrary.is_none());
}

#[test]
fn on_without_a_key_blocks_and_says_where_the_key_goes() {
    let w = World::new();
    let mut p = planned(&w, &options(true));
    plan_onelibrary(&mut p, &Fake::new(ExistingLibrary::Absent), None).unwrap();
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
    plan_onelibrary(&mut p, &Fake::new(ExistingLibrary::Absent), Some(&key())).unwrap();

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
    plan_onelibrary(&mut p, &Fake::new(ExistingLibrary::Absent), Some(&key())).unwrap();
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
    let read = Fake::new(ExistingLibrary::Read {
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

/// A plan with the option on and the key, on a stick that has rekordbox's
/// `exportLibrary.db`, its `-wal` and `exportExt.pdb`.
fn onelibrary_plan(w: &World, fake: &Fake) -> ExportPlan {
    put_rekordbox_onelibrary(w);
    let mut p = planned(w, &options(true));
    plan_onelibrary(&mut p, fake, Some(&key())).unwrap();
    assert!(!p.is_blocked(), "{:?}", p.errors);
    p
}

/// Runs `apply` on `p` with `fake` and the key (`None`: no OneLibrary
/// writer); returns the result and the stages in order.
fn apply_onelibrary(
    w: &World,
    p: &ExportPlan,
    fake: Option<&Fake>,
) -> (Result<ExportReport, ApplyError>, Vec<ApplyStage>) {
    let key = key();
    let env = ApplyEnv {
        onelibrary: fake.map(|f| OneLibraryEnv {
            export: f,
            key: &key,
        }),
        ..w.env()
    };
    let writer = BakenWriter::prepare(&w.lib, &w.sel(), p, &w.tmp.path().join("work")).unwrap();
    let stages = Mutex::new(Vec::new());
    let progress = |e: ApplyEvent| {
        if let ApplyEvent::Stage { stage } = e {
            stages.lock().unwrap().push(stage);
        }
    };
    let result = apply(p, &writer, &env, &progress, &CancelToken::new());
    (result, stages.into_inner().unwrap())
}

#[test]
fn apply_writes_exportlibrary_after_export_pdb_with_the_auxiliary_files_gone() {
    let w = World::new();
    let fake = Fake::new(ExistingLibrary::NotRead {
        reason: "test".into(),
    });
    let p = onelibrary_plan(&w, &fake);
    let (result, stages) = apply_onelibrary(&w, &p, Some(&fake));
    let report = result.unwrap();

    // When the writer ran, the new export.pdb was there and rekordbox's
    // OneLibrary files were gone (baken removes them with export.pdb).
    assert_eq!(fake.seen().unwrap(), ["export.pdb"]);
    let rb = w.stick.join("PIONEER/rekordbox");
    assert_eq!(fs::read(rb.join("exportLibrary.db")).unwrap(), FAKE_DB);
    for gone in ["exportLibrary.db-wal", "exportExt.pdb"] {
        assert!(!rb.join(gone).exists(), "{gone}");
    }
    // The step runs after the write, before the sync and the read-back.
    let at = |s| stages.iter().position(|x| *x == s).unwrap();
    assert_eq!(at(ApplyStage::OneLibrary), at(ApplyStage::Write) + 1);
    assert_eq!(at(ApplyStage::Sync), at(ApplyStage::OneLibrary) + 1);

    assert_eq!(report.status, ExportStatus::Success, "{:?}", report.errors);
    let ol = report.onelibrary.as_ref().unwrap();
    assert_eq!(ol.written.tracks, p.tracks.len());
    assert_eq!(ol.auxiliary_removed, 0);
    // rekordbox's database is in the backup (not read: Phase 2 safeguard).
    assert!(report
        .backup_dir
        .as_ref()
        .unwrap()
        .join("PIONEER/rekordbox/exportLibrary.db")
        .exists());

    let view = ReportView::new(&p, &Ok(report), &TrackLabels::new());
    let fact = view
        .facts
        .iter()
        .find(|f| f.label == "OneLibrary (experimental)")
        .unwrap();
    assert_eq!(fact.value, "exportLibrary.db written: 3 tracks, 21 B");
}

#[test]
fn a_failed_onelibrary_write_keeps_export_pdb_and_reports_it() {
    let w = World::new();
    let fake = Fake::failing();
    let p = onelibrary_plan(&w, &fake);
    let (result, _) = apply_onelibrary(&w, &p, Some(&fake));
    let report = result.unwrap();

    let rb = w.stick.join("PIONEER/rekordbox");
    assert!(rb.join("export.pdb").exists());
    assert!(!rb.join("exportLibrary.db").exists());
    assert_eq!(report.status, ExportStatus::Problems);
    assert_eq!(report.manifest, ManifestStatus::Skipped);
    assert!(report.onelibrary.is_none());
    assert_eq!(
        report.errors,
        ["exportLibrary.db not written: building exportLibrary.db failed: integrity check failed"]
    );
    let view = ReportView::new(&p, &Ok(report), &TrackLabels::new());
    assert!(view
        .facts
        .iter()
        .any(|f| f.value == "exportLibrary.db not written (see the problems)"));
}

#[test]
fn apply_refuses_a_onelibrary_plan_without_a_writer_or_its_step() {
    let w = World::new();
    let fake = Fake::new(ExistingLibrary::Absent);
    let p = onelibrary_plan(&w, &fake);
    let before = rekordbox_files(&w.stick);

    let (result, _) = apply_onelibrary(&w, &p, None);
    assert!(
        matches!(result, Err(ApplyError::NoOneLibraryWriter)),
        "{result:?}"
    );
    // A plan read back from JSON without its OneLibrary step.
    let mut stripped = p.clone();
    stripped.onelibrary = None;
    let (result, _) = apply_onelibrary(&w, &stripped, Some(&fake));
    assert!(
        matches!(result, Err(ApplyError::NoOneLibraryPlan)),
        "{result:?}"
    );

    assert_eq!(rekordbox_files(&w.stick), before);
    assert!(fake.seen().is_none());
}

#[test]
fn the_stub_writes_nothing() {
    let w = World::new();
    let p = planned(&w, &options(true));
    let err = NoWriter.write(&p, &key(), &w.stick).unwrap_err();
    assert!(matches!(err, OneLibraryWriteError::Unavailable));
    assert!(rekordbox_files(&w.stick).is_empty());
}
