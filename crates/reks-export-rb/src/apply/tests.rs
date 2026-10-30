//! `apply` end to end on temp-dir sticks: fixture audio in a library imported
//! from XML, planned with generated analysis, written by baken, read back.

use std::collections::BTreeMap;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use reks_core::rbxml::{self, ImportOptions, PrefixRemap};
use reks_core::{CancelToken, Library};

use super::*;
use crate::device::{DeviceInfo, FsKind};
use crate::onelibrary::NoWriter;
use crate::plan::{self, ExportOptions, MissingAnalysis, NoAnlz, Selection};
use crate::testutil;
use crate::writer::BakenWriter;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/audio");
const XML_PREFIX: &str = "/Users/reks/Music/reks apply";
pub(crate) const SERIAL: &str = "4993-1CFB";

/// (fixture, file name, title, artist)
const TRACKS: [(&str, &str, &str, &str); 3] = [
    ("sine-click-128.mp3", "one.mp3", "One", "Alpha"),
    ("sine-click-128.flac", "two.flac", "Two", "Beta"),
    ("sine-click-128.wav", "three.wav", "Three", "Gamma"),
];

pub(crate) struct World {
    pub(crate) tmp: tempfile::TempDir,
    pub(crate) lib: Library,
    pub(crate) stick: PathBuf,
    pub(crate) data: PathBuf,
}

impl World {
    /// [`TRACKS`] in `Set/Peak` (1, 2) and `Set/Chill` (3, 2); an empty stick.
    pub(crate) fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let music = tmp.path().join("music");
        let stick = tmp.path().join("stick");
        let data = tmp.path().join("data");
        for d in [&music, &stick, &data] {
            fs::create_dir_all(d).unwrap();
        }
        for (fixture, name, ..) in TRACKS {
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
        World {
            tmp,
            lib,
            stick,
            data,
        }
    }

    /// The `Set` folder: all three tracks.
    pub(crate) fn sel(&self) -> Selection {
        let tree = self.lib.playlists().unwrap();
        Selection {
            playlists: vec![tree.nodes.iter().find(|n| n.name == "Set").unwrap().id],
        }
    }

    pub(crate) fn dev(&self) -> DeviceInfo {
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

    fn plan_with(&self, serial: Option<&str>) -> ExportPlan {
        let sources = plan::load(&self.lib, &self.sel()).unwrap();
        // The Device Library only: the OneLibrary step has its own tests.
        let opts = ExportOptions {
            missing_analysis: MissingAnalysis::Generate,
            onelibrary: false,
            ..ExportOptions::default()
        };
        let mut p = plan::plan_sources(
            sources,
            &self.dev(),
            serial.map(str::to_owned),
            &opts,
            &NoAnlz,
            &NoAnlz,
        )
        .unwrap();
        plan::plan_onelibrary(&mut p, &NoWriter, None, &self.lib).unwrap();
        assert!(!p.is_blocked(), "{:?}", p.errors);
        p
    }

    pub(crate) fn plan(&self) -> ExportPlan {
        self.plan_with(Some(SERIAL))
    }

    fn writer(&self, plan: &ExportPlan) -> BakenWriter {
        BakenWriter::prepare(&self.lib, &self.sel(), plan, &self.tmp.path().join("work")).unwrap()
    }

    pub(crate) fn env(&self) -> ApplyEnv<'_> {
        ApplyEnv {
            library: &self.lib,
            app_data_dir: &self.data,
            stick: StickCheck::Directory,
            onelibrary: None,
        }
    }

    /// Runs `apply` with `env`; `on_event` sees each event (and the token).
    fn apply_with(
        &self,
        plan: &ExportPlan,
        env: &ApplyEnv<'_>,
        on_event: impl Fn(&ApplyEvent, &CancelToken) + Sync,
    ) -> (Result<ExportReport, ApplyError>, Vec<ApplyEvent>) {
        let cancel = CancelToken::new();
        let events = Mutex::new(Vec::new());
        let progress = |e: ApplyEvent| {
            on_event(&e, &cancel);
            events.lock().unwrap().push(e);
        };
        let result = apply(plan, &self.writer(plan), env, &progress, &cancel);
        (result, events.into_inner().unwrap())
    }

    fn apply(&self, plan: &ExportPlan) -> (Result<ExportReport, ApplyError>, Vec<ApplyEvent>) {
        self.apply_with(plan, &self.env(), |_, _| {})
    }

    pub(crate) fn export(&self) -> ExportReport {
        let report = self.apply(&self.plan()).0.unwrap();
        assert_eq!(report.status, ExportStatus::Success, "{report:#?}");
        report
    }

    pub(crate) fn put_played_pdb(&self) {
        let rb = self.stick.join("PIONEER/rekordbox");
        fs::create_dir_all(&rb).unwrap();
        fs::write(rb.join("export.pdb"), testutil::played_pdb()).unwrap();
    }

    pub(crate) fn pdb(&self) -> Vec<u8> {
        fs::read(self.stick.join("PIONEER/rekordbox/export.pdb")).unwrap()
    }
}

fn collection_xml() -> String {
    let prefix = XML_PREFIX.replace(' ', "%20");
    let mut tracks = String::new();
    for (i, (_, name, title, artist)) in TRACKS.iter().enumerate() {
        tracks += &format!(
            r#"<TRACK TrackID="{id}" Name="{title}" Artist="{artist}" Album="Album" Genre="Techno" Kind="" TotalTime="30" AverageBpm="128.00" Tonality="Am" Location="file://localhost{prefix}/{name}">
  <TEMPO Inizio="0.000" Bpm="128.00" Metro="4/4" Battito="1"/>
  <POSITION_MARK Name="Drop" Type="0" Start="7.500" Num="0" Red="230" Green="40" Blue="40"/>
</TRACK>
"#,
            id = i + 1,
        );
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<DJ_PLAYLISTS Version="1.0.0">
<PRODUCT Name="rekordbox" Version="7.0.0" Company="AlphaTheta"/>
<COLLECTION Entries="3">
{tracks}</COLLECTION>
<PLAYLISTS>
<NODE Type="0" Name="ROOT" Count="1">
  <NODE Type="0" Name="Set" Count="2">
    <NODE Name="Peak" Type="1" KeyType="0" Entries="2"><TRACK Key="1"/><TRACK Key="2"/></NODE>
    <NODE Name="Chill" Type="1" KeyType="0" Entries="2"><TRACK Key="3"/><TRACK Key="2"/></NODE>
  </NODE>
</NODE>
</PLAYLISTS>
</DJ_PLAYLISTS>
"#
    )
}

/// Every file and folder under `root`: kind, length, mtime and, for files,
/// the bytes.
type Snapshot = BTreeMap<PathBuf, (bool, u64, SystemTime, Option<Vec<u8>>)>;

fn snapshot(root: &Path) -> Snapshot {
    fn walk(dir: &Path, out: &mut Snapshot) {
        for e in fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            let m = fs::metadata(&p).unwrap();
            let bytes = m.is_file().then(|| fs::read(&p).unwrap());
            out.insert(
                p.clone(),
                (m.is_dir(), m.len(), m.modified().unwrap(), bytes),
            );
            if m.is_dir() {
                walk(&p, out);
            }
        }
    }
    let mut out = Snapshot::new();
    walk(root, &mut out);
    out
}

fn stages(events: &[ApplyEvent]) -> Vec<ApplyStage> {
    events
        .iter()
        .filter_map(|e| match e {
            ApplyEvent::Stage { stage } => Some(*stage),
            _ => None,
        })
        .collect()
}

// --- tests -----------------------------------------------------------------

#[test]
fn played_stick_is_backed_up_written_validated_and_recorded() {
    let w = World::new();
    w.put_played_pdb();
    let plan = w.plan();
    assert_eq!(plan.backups.len(), 1, "{:?}", plan.backups);

    let (result, events) = w.apply(&plan);
    let report = result.unwrap();
    assert_eq!(report.status, ExportStatus::Success, "{report:#?}");
    assert!(report.findings().is_empty());
    assert!(report.errors.is_empty());
    assert_eq!(report.volume_serial.as_deref(), Some(SERIAL));

    // The old database was copied off, verified, before anything was written.
    let dir = report.backup_dir.clone().unwrap();
    assert!(dir.starts_with(w.data.join("stick-backups").join(SERIAL)));
    assert!(dir.join("manifest.json").is_file());
    assert_eq!(
        fs::read(dir.join("PIONEER/rekordbox/export.pdb")).unwrap(),
        testutil::played_pdb()
    );
    assert_eq!(report.backup.as_ref().unwrap().files.len(), 1);

    // New database, every track written.
    let writer = report.writer.as_ref().unwrap();
    assert_eq!(writer.tracks_in_database, 3);
    assert_eq!(writer.audio_copied, 3);
    assert_ne!(w.pdb(), testutil::played_pdb());

    // Manifest: one row per track, matched to the library.
    assert_eq!(report.manifest, ManifestStatus::Recorded);
    assert_eq!(report.manifest_rows, 3);
    let rows = w.lib.manifest(SERIAL).unwrap();
    assert_eq!(rows.len(), 3);
    for t in &plan.tracks {
        let row = w.lib.manifest_entry(SERIAL, &t.dst_rel).unwrap().unwrap();
        assert_eq!(row.track_id, Some(t.track_id));
    }

    assert_eq!(
        stages(&events),
        [
            ApplyStage::Preflight,
            ApplyStage::Backup,
            ApplyStage::Write,
            ApplyStage::Sync,
            ApplyStage::Validate,
            ApplyStage::Manifest,
        ]
    );
    let done = events
        .iter()
        .filter(|e| {
            matches!(
                e,
                ApplyEvent::Writer {
                    event: ExportEvent::TrackDone { .. }
                }
            )
        })
        .count();
    assert_eq!(done, 3);
    assert!(events.contains(&ApplyEvent::Writer {
        event: ExportEvent::Finished
    }));
}

#[test]
fn second_identical_export_copies_nothing() {
    let w = World::new();
    let first = w.export();
    assert_eq!(first.writer.unwrap().audio_copied, 3);
    // Our own fresh export holds no history: no backup.
    assert!(first.backup.is_none());

    let plan = w.plan();
    assert!(plan.copies.is_empty() && plan.deletes.is_empty());
    assert!(plan.backups.is_empty());
    let report = w.apply(&plan).0.unwrap();
    assert_eq!(report.status, ExportStatus::Success, "{report:#?}");
    let writer = report.writer.unwrap();
    assert_eq!((writer.audio_copied, writer.audio_kept), (0, 3));
    assert_eq!(writer.pruned, 0);
    assert_eq!(report.manifest_rows, 3);
    assert_eq!(w.lib.manifest(SERIAL).unwrap().len(), 3);
}

#[test]
fn failing_backup_leaves_the_stick_untouched_and_no_manifest() {
    let w = World::new();
    w.put_played_pdb();
    let plan = w.plan();
    assert!(!plan.backups.is_empty());
    // The app data dir is a file: the backup folder can't be made.
    let data = w.tmp.path().join("data-file");
    fs::write(&data, b"not a folder").unwrap();
    let env = ApplyEnv {
        app_data_dir: &data,
        ..w.env()
    };
    let before = snapshot(&w.stick);
    let (result, events) = w.apply_with(&plan, &env, |_, _| {});
    assert!(matches!(result, Err(ApplyError::Backup(_))), "{result:?}");
    assert_eq!(snapshot(&w.stick), before);
    assert!(w.lib.manifest(SERIAL).unwrap().is_empty());
    assert_eq!(stages(&events), [ApplyStage::Preflight, ApplyStage::Backup]);
}

#[test]
fn cancel_during_the_write_keeps_the_old_database() {
    let w = World::new();
    w.export();
    let old_pdb = w.pdb();
    let old_manifest = w.lib.manifest(SERIAL).unwrap();

    // Second run with a stick that needs copying again: cancel after track 1.
    fs::remove_dir_all(w.stick.join("Contents")).unwrap();
    let plan = w.plan();
    assert_eq!(plan.copies.len(), 3);
    let (result, events) = w.apply_with(&plan, &w.env(), |e, cancel| {
        if let ApplyEvent::Writer {
            event: ExportEvent::TrackDone { done: 1, .. },
        } = e
        {
            cancel.cancel();
        }
    });
    let report = result.unwrap();
    assert_eq!(report.status, ExportStatus::Cancelled);
    assert!(report.writer.as_ref().unwrap().cancelled);
    assert!(report.validation.is_none());
    assert_eq!(report.manifest, ManifestStatus::Skipped);
    assert_eq!(w.pdb(), old_pdb);
    assert_eq!(w.lib.manifest(SERIAL).unwrap(), old_manifest);
    assert!(!stages(&events).contains(&ApplyStage::Validate));
}

#[test]
fn cancel_before_the_write_touches_nothing() {
    let w = World::new();
    w.put_played_pdb();
    let plan = w.plan();
    let before = snapshot(&w.stick);
    let (result, events) = w.apply_with(&plan, &w.env(), |e, cancel| {
        if *e
            == (ApplyEvent::Stage {
                stage: ApplyStage::Backup,
            })
        {
            cancel.cancel();
        }
    });
    let report = result.unwrap();
    assert_eq!(report.status, ExportStatus::Cancelled);
    // The backup ran to completion; the writer never started.
    assert!(report.backup.is_some());
    assert!(report.writer.is_none());
    assert!(!stages(&events).contains(&ApplyStage::Write));
    assert_eq!(snapshot(&w.stick), before);
}

#[test]
fn dry_run_and_validation_leave_the_stick_unchanged() {
    let w = World::new();
    w.put_played_pdb();
    w.export();
    let before = snapshot(&w.stick);
    // Through the public planner (asks the OS for a serial; the fake stick has none).
    let opts = ExportOptions {
        missing_analysis: MissingAnalysis::Generate,
        ..ExportOptions::default()
    };
    let plan = plan::plan(&w.lib, &w.sel(), &w.dev(), &opts, &NoAnlz).unwrap();
    assert!(plan.copies.is_empty(), "{:?}", plan.copies);
    let report = validate::validate(&w.stick, &plan).unwrap();
    assert!(report.is_valid(), "{:?}", report.findings);
    assert_eq!(snapshot(&w.stick), before);
}

#[test]
fn no_volume_serial_succeeds_without_manifest_rows() {
    let w = World::new();
    let plan = w.plan_with(None);
    let (result, events) = w.apply(&plan);
    let report = result.unwrap();
    assert_eq!(report.status, ExportStatus::Success, "{report:#?}");
    assert_eq!(report.manifest, ManifestStatus::NoVolumeSerial);
    assert_eq!(report.manifest_rows, 0);
    assert!(report.volume_serial.is_none());
    assert!(stages(&events).contains(&ApplyStage::Manifest));
}

#[test]
fn stick_played_since_the_dry_run_is_refused() {
    let w = World::new();
    let plan = w.plan();
    assert!(plan.backups.is_empty());
    // Played between dry run and apply: its history has no planned backup.
    w.put_played_pdb();
    let before = snapshot(&w.stick);
    let result = w.apply(&plan).0;
    assert!(
        matches!(result, Err(ApplyError::PlanOutOfDate(_))),
        "{result:?}"
    );
    assert_eq!(snapshot(&w.stick), before);
    assert!(!w.data.join("stick-backups").exists());
}

#[test]
fn preflight_refusals_write_nothing() {
    let w = World::new();
    let plan = w.plan();

    // A plain directory is not a mounted volume.
    let env = ApplyEnv {
        stick: StickCheck::Mounted,
        onelibrary: None,
        ..w.env()
    };
    let result = w.apply_with(&plan, &env, |_, _| {}).0;
    assert!(
        matches!(result, Err(ApplyError::NotMounted(_))),
        "{result:?}"
    );

    let mut big = plan.clone();
    big.bytes_to_write = u64::MAX;
    let result = w.apply(&big).0;
    assert!(
        matches!(result, Err(ApplyError::NotEnoughSpace { .. })),
        "{result:?}"
    );

    let mut blocked = plan.clone();
    blocked.errors.push(plan::Problem::NothingSelected);
    assert!(matches!(w.apply(&blocked).0, Err(ApplyError::Blocked(1))));

    let mut empty = plan.clone();
    empty.tracks.clear();
    assert!(matches!(
        w.apply(&empty).0,
        Err(ApplyError::NothingToExport)
    ));

    let mut gone = plan;
    gone.device.mount_point = w.tmp.path().join("unplugged");
    assert!(matches!(w.apply(&gone).0, Err(ApplyError::NotMounted(_))));

    assert!(snapshot(&w.stick).is_empty());
}

#[test]
fn writer_refusal_keeps_the_old_database() {
    let w = World::new();
    w.put_played_pdb();
    let plan = w.plan();
    // The source vanished after planning: the writer refuses before writing.
    let src = plan.tracks[0].src.clone();
    fs::remove_file(&src).unwrap();
    let result = w.apply(&plan).0;
    assert!(
        matches!(
            result,
            Err(ApplyError::Writer(WriterError::PlanMismatch { .. }))
        ),
        "{result:?}"
    );
    assert_eq!(w.pdb(), testutil::played_pdb());
    assert!(!w.stick.join("Contents").exists());
    assert!(w.lib.manifest(SERIAL).unwrap().is_empty());
}

#[test]
fn rekordbox_process_names() {
    for name in ["rekordbox", "rekordbox.exe", "REKORDBOX.EXE"] {
        assert!(is_rekordbox(name), "{name}");
    }
    for name in ["rekordboxAgent", "rekordbox_helper", "reks", ""] {
        assert!(!is_rekordbox(name), "{name}");
    }
}

#[test]
fn events_and_report_json() {
    let e = ApplyEvent::Writer {
        event: ExportEvent::TrackDone {
            done: 3,
            total: 40,
            track_id: Some(7),
        },
    };
    assert_eq!(
        serde_json::to_value(&e).unwrap(),
        serde_json::json!({"type": "writer", "event": {"type": "trackDone", "done": 3, "total": 40, "trackId": 7}})
    );
    assert_eq!(
        serde_json::to_value(ApplyEvent::Stage {
            stage: ApplyStage::Backup
        })
        .unwrap(),
        serde_json::json!({"type": "stage", "stage": "backup"})
    );
    assert_eq!(
        serde_json::to_value(ApplyEvent::Warning {
            warning: ApplyWarning::RekordboxRunning
        })
        .unwrap(),
        serde_json::json!({"type": "warning", "warning": {"type": "rekordboxRunning"}})
    );
    let r = ExportReport::new(None, Vec::new());
    let json = serde_json::to_value(&r).unwrap();
    assert_eq!(json["status"], "cancelled");
    assert_eq!(json["manifest"], "skipped");
    assert_eq!(json["manifestRows"], 0);
}

#[test]
fn remove_auxiliary_removes_what_is_left_and_skips_what_is_gone() {
    let dir = tempfile::tempdir().unwrap();
    let rb = dir.path().join("PIONEER/rekordbox");
    fs::create_dir_all(&rb).unwrap();
    fs::write(rb.join("exportLibrary.db-wal"), b"wal").unwrap();
    let op = |name: &str| FileOp {
        src: None,
        dst_rel: format!("/PIONEER/rekordbox/{name}"),
        bytes: 3,
        track_id: None,
        reason: crate::plan::OpReason::OneLibraryAuxiliary,
    };
    let removes = [op("exportLibrary.db-wal"), op("exportLibrary.db-shm")];
    assert_eq!(remove_auxiliary(dir.path(), &removes), Ok(1));
    assert!(!rb.join("exportLibrary.db-wal").exists());
}
