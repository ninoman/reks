//! Export orchestration (Phase 2 task 9): [`apply`] is the only function in
//! reks that writes to a stick. It runs a planned export in this order and
//! stops at the first step that fails:
//!
//! 1. **Pre-flight**: the plan is not blocked and has tracks; the stick is
//!    still there ([`StickCheck`]): mounted at the plan's root, with the same
//!    volume serial; enough free space for [`ExportPlan::bytes_to_write`];
//!    the history on the stick is what the dry run showed (a stick played
//!    since needs a new plan, or its new history would go without backup). A
//!    running rekordbox is only a warning ([`ApplyWarning::RekordboxRunning`]).
//! 2. **History backup** (D23, task 12): [`history::run_backups`] on
//!    [`ExportPlan::backup_plan`]. Any error aborts with the stick untouched.
//! 3. **Write** through the [`RekordboxWriter`], which gets the cancel token.
//! 4. **fsync** of the database, the ANLZ files and the copied audio.
//! 5. **Validate**: [`validate`] reads the stick back.
//! 6. **Manifest** (task 13): `Library::replace_manifest` with the read-back
//!    rows, only when the export is clean (no failed track, no finding, no
//!    error after the database was written).
//!
//! What the result means for the stick:
//!
//! - `Err(ApplyError)`: the stick's `export.pdb` is the old one (refused in
//!   pre-flight, backup failed, or the writer stopped before replacing it;
//!   [`crate::writer`] says what a writer error leaves behind).
//! - `Ok` with [`ExportStatus::Cancelled`]: the old `export.pdb` too. The
//!   token is checked before the backup, before the write and by the writer
//!   between tracks; once the new database is written the run goes on to
//!   validate and record the manifest.
//! - `Ok` with [`ExportStatus::Success`] or [`ExportStatus::Problems`]: the
//!   stick has a new `export.pdb`. `Problems` lists what went wrong: tracks
//!   that failed alone (the writer goes on without them), validator findings,
//!   errors after the database was replaced (prune, fsync, validation I/O,
//!   manifest).

use std::fs;
use std::path::{Path, PathBuf};

use reks_core::{CancelToken, Library};
use serde::Serialize;

use crate::device::{self, DeviceError};
use crate::history::{self, BackupReport, HistoryError, StickHistory};
use crate::plan::{anlz_dir, on_stick, AnlzSource, ExportPlan};
use crate::validate::{self, ValidationReport};
use crate::writer::{ExportEvent, RekordboxWriter, WriterError, WriterReport};

/// How [`apply`] makes sure the plan's stick is still there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StickCheck {
    /// The plan's root must still be a mounted volume, and its volume serial
    /// the plan's (another stick with the same label mounts at the same
    /// path). Free space comes from that volume. For real sticks.
    Mounted,
    /// The plan's root is a directory standing in for a stick (tests,
    /// `reks rb export` on a folder): it must exist; free space comes from
    /// the volume that holds it; there is no serial to compare.
    Directory,
}

/// What [`apply`] needs besides the plan and the writer.
#[derive(Debug, Clone, Copy)]
pub struct ApplyEnv<'a> {
    /// Receives the export manifest.
    pub library: &'a Library,
    /// Stick backups go under `<app_data_dir>/stick-backups/` (D19, D23).
    pub app_data_dir: &'a Path,
    pub stick: StickCheck,
}

/// Steps of [`apply`], in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ApplyStage {
    Preflight,
    /// Copying the stick's database files to the app data dir.
    Backup,
    /// The writer runs; [`ApplyEvent::Writer`] events follow.
    Write,
    /// Flushing written files to the stick.
    Sync,
    /// Reading the stick back.
    Validate,
    /// Recording the export manifest.
    Manifest,
}

/// Progress of [`apply`], for the UI. JSON:
/// `{"type": "stage", "stage": "backup"}`,
/// `{"type": "writer", "event": {"type": "trackDone", "done": 3, "total": 40, "trackId": 7}}`,
/// `{"type": "warning", "warning": {"type": "rekordboxRunning"}}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ApplyEvent {
    /// A step starts.
    Stage { stage: ApplyStage },
    /// Progress from the writer, during [`ApplyStage::Write`].
    Writer { event: ExportEvent },
    /// Something the user should know; the export goes on.
    Warning { warning: ApplyWarning },
}

/// Found while applying; does not stop the export.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ApplyWarning {
    /// rekordbox is running. It may touch the stick while reks writes it
    /// (UNVERIFIED; process name `rekordbox` / `rekordbox.exe`).
    RekordboxRunning,
}

impl std::fmt::Display for ApplyWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RekordboxRunning => f.write_str(
                "rekordbox is running; quit it while exporting so it does not touch the stick",
            ),
        }
    }
}

/// How the run ended (module docs say what each leaves on the stick).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportStatus {
    /// New `export.pdb`, every planned track written, no validator finding.
    Success,
    /// New `export.pdb`, with failed tracks, findings or errors in the report.
    Problems,
    /// Stopped by the cancel token; the old `export.pdb` is in place.
    Cancelled,
}

/// What happened to the export manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ManifestStatus {
    /// The stick's rows were replaced.
    Recorded,
    /// The OS gave no volume serial: nothing stored (the export still counts).
    NoVolumeSerial,
    /// Not recorded: the export was cancelled or has problems.
    Skipped,
    /// Storing failed (see [`ExportReport::errors`]); the old rows stay.
    Failed,
}

/// Result of [`apply`]; serialisable for the UI's report step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportReport {
    pub status: ExportStatus,
    pub volume_serial: Option<String>,
    pub warnings: Vec<ApplyWarning>,
    /// The history backup, when the plan had one.
    pub backup: Option<BackupReport>,
    /// Folder of [`Self::backup`] (`BackupReport` leaves it out of its JSON).
    pub backup_dir: Option<PathBuf>,
    /// What the writer did; `None` when cancelled before writing, or when
    /// the writer failed after replacing `export.pdb` (then in [`Self::errors`]).
    pub writer: Option<WriterReport>,
    /// The read-back; `None` when cancelled or when reading failed.
    pub validation: Option<ValidationReport>,
    pub manifest: ManifestStatus,
    /// Manifest rows stored (0 unless [`ManifestStatus::Recorded`]).
    pub manifest_rows: usize,
    /// Errors after the new `export.pdb` was written.
    pub errors: Vec<String>,
}

impl ExportReport {
    fn new(volume_serial: Option<String>, warnings: Vec<ApplyWarning>) -> Self {
        Self {
            status: ExportStatus::Cancelled,
            volume_serial,
            warnings,
            backup: None,
            backup_dir: None,
            writer: None,
            validation: None,
            manifest: ManifestStatus::Skipped,
            manifest_rows: 0,
            errors: Vec::new(),
        }
    }

    /// Validator findings (none when not validated).
    pub fn findings(&self) -> &[validate::Finding] {
        self.validation.as_ref().map_or(&[], |v| &v.findings)
    }
}

/// Why [`apply`] stopped with the stick's old `export.pdb` in place.
#[derive(Debug, thiserror::Error)]
pub enum ApplyError {
    #[error("the export plan has {0} blocking problems")]
    Blocked(usize),
    #[error("the export plan has no tracks")]
    NothingToExport,
    #[error("the stick is no longer mounted at {}", .0.display())]
    NotMounted(PathBuf),
    /// A different volume is mounted at the plan's root.
    #[error("another stick is mounted at {}: volume serial {found:?}, the plan is for {planned:?}", path.display())]
    DifferentStick {
        path: PathBuf,
        planned: Option<String>,
        found: Option<String>,
    },
    #[error("cannot check the stick: {0}")]
    Device(#[from] DeviceError),
    #[error("not enough space on the stick: {needed} bytes needed, {available} free")]
    NotEnoughSpace { needed: u64, available: u64 },
    /// The stick changed since the dry run; plan again.
    #[error("the stick changed since the dry run ({0}); plan the export again")]
    PlanOutOfDate(String),
    /// Backing up the stick's history failed; nothing was written.
    #[error("backing up the stick's play history failed, nothing was written: {0}")]
    Backup(#[source] HistoryError),
    /// The writer stopped before replacing `export.pdb`.
    #[error(transparent)]
    Writer(WriterError),
}

/// Runs `plan` on its stick (module docs). `progress` gets [`ApplyEvent`]s,
/// from the writer's thread too; `cancel` is passed to the writer.
pub fn apply(
    plan: &ExportPlan,
    writer: &dyn RekordboxWriter,
    env: &ApplyEnv<'_>,
    progress: &(dyn Fn(ApplyEvent) + Sync),
    cancel: &CancelToken,
) -> Result<ExportReport, ApplyError> {
    let root = plan.device.mount_point.as_path();
    let serial = plan.history.volume_serial.clone();
    tracing::info!(root = %root.display(), tracks = plan.tracks.len(), "applying export");

    let stage = |stage| progress(ApplyEvent::Stage { stage });
    stage(ApplyStage::Preflight);
    let warnings = preflight(plan, env.stick)?;
    for w in &warnings {
        tracing::warn!(warning = %w, "export pre-flight");
        progress(ApplyEvent::Warning { warning: w.clone() });
    }
    let mut report = ExportReport::new(serial.clone(), warnings);
    if cancel.is_cancelled() {
        return Ok(report);
    }

    stage(ApplyStage::Backup);
    if let Some(b) =
        history::run_backups(&plan.backup_plan(), env.app_data_dir).map_err(ApplyError::Backup)?
    {
        report.backup_dir = Some(b.dir.clone());
        report.backup = Some(b);
    }
    if cancel.is_cancelled() {
        return Ok(report);
    }

    stage(ApplyStage::Write);
    let forward = |event| progress(ApplyEvent::Writer { event });
    match writer.write(plan, root, &forward, cancel) {
        Ok(w) if w.cancelled => {
            tracing::info!("export cancelled; the old export.pdb stays");
            report.writer = Some(w);
            return Ok(report);
        }
        Ok(w) => report.writer = Some(w),
        Err(WriterError::AfterDatabase(message)) => {
            report
                .errors
                .push(format!("export.pdb was written, then: {message}"));
        }
        Err(e) => return Err(ApplyError::Writer(e)),
    }

    stage(ApplyStage::Sync);
    if let Err(e) = sync_written(plan) {
        report.errors.push(e);
    }

    stage(ApplyStage::Validate);
    match validate::validate(root, plan) {
        Ok(v) => report.validation = Some(v),
        Err(e) => report.errors.push(format!("validation: {e}")),
    }

    let failures = report.writer.as_ref().map_or(0, |w| w.failures.len());
    let clean = failures == 0
        && report.errors.is_empty()
        && report.validation.as_ref().is_some_and(|v| v.is_valid());
    report.status = if clean {
        ExportStatus::Success
    } else {
        ExportStatus::Problems
    };

    if !clean {
        tracing::warn!(
            failures,
            findings = report.findings().len(),
            errors = report.errors.len(),
            "export has problems; manifest not recorded"
        );
    } else if let Some(v) = &report.validation {
        stage(ApplyStage::Manifest);
        match env
            .library
            .replace_manifest(serial.as_deref(), &v.manifest_rows())
        {
            Ok(rows) if serial.is_some() => {
                report.manifest = ManifestStatus::Recorded;
                report.manifest_rows = rows;
            }
            Ok(_) => report.manifest = ManifestStatus::NoVolumeSerial,
            Err(e) => {
                report.manifest = ManifestStatus::Failed;
                report.errors.push(format!("export manifest: {e}"));
                report.status = ExportStatus::Problems;
            }
        }
    }
    tracing::info!(status = ?report.status, manifest_rows = report.manifest_rows, "export done");
    Ok(report)
}

/// Pre-flight checks (module docs, step 1).
fn preflight(plan: &ExportPlan, check: StickCheck) -> Result<Vec<ApplyWarning>, ApplyError> {
    if plan.is_blocked() {
        return Err(ApplyError::Blocked(plan.errors.len()));
    }
    if plan.tracks.is_empty() {
        return Err(ApplyError::NothingToExport);
    }
    let root = plan.device.mount_point.as_path();
    let free = match check {
        StickCheck::Mounted => {
            let dev = device::list_devices()
                .into_iter()
                .find(|d| d.mount_point == root)
                .ok_or_else(|| ApplyError::NotMounted(root.to_path_buf()))?;
            let found = device::volume_serial(&dev);
            if found != plan.history.volume_serial {
                return Err(ApplyError::DifferentStick {
                    path: root.to_path_buf(),
                    planned: plan.history.volume_serial.clone(),
                    found,
                });
            }
            dev.free_bytes
        }
        StickCheck::Directory => {
            if !root.is_dir() {
                return Err(ApplyError::NotMounted(root.to_path_buf()));
            }
            device::device_for_path(root)?.free_bytes
        }
    };
    if plan.bytes_to_write > free {
        return Err(ApplyError::NotEnoughSpace {
            needed: plan.bytes_to_write,
            available: free,
        });
    }
    let now = history::read_history(root, plan.history.volume_serial.clone());
    if now != plan.history {
        return Err(ApplyError::PlanOutOfDate(history_change(
            &plan.history,
            &now,
        )));
    }
    let mut warnings = Vec::new();
    if rekordbox_running() {
        warnings.push(ApplyWarning::RekordboxRunning);
    }
    Ok(warnings)
}

fn history_change(planned: &StickHistory, now: &StickHistory) -> String {
    format!(
        "play history was {} sessions / {} entries{}{}, now {} / {}{}{}",
        planned.pdb_sessions,
        planned.pdb_entries,
        if planned.pdb_unreadable {
            ", unreadable"
        } else {
            ""
        },
        if planned.onelibrary_db {
            ", exportLibrary.db"
        } else {
            ""
        },
        now.pdb_sessions,
        now.pdb_entries,
        if now.pdb_unreadable {
            ", unreadable"
        } else {
            ""
        },
        if now.onelibrary_db {
            ", exportLibrary.db"
        } else {
            ""
        },
    )
}

/// True when a process named `rekordbox` runs (not `rekordboxAgent`, which
/// stays in the background on macOS).
fn rekordbox_running() -> bool {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
    let mut sys = System::new();
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    sys.processes()
        .values()
        .any(|p| is_rekordbox(&p.name().to_string_lossy()))
}

fn is_rekordbox(process_name: &str) -> bool {
    process_name.eq_ignore_ascii_case("rekordbox")
        || process_name.eq_ignore_ascii_case("rekordbox.exe")
}

/// Flushes what the writer wrote: everything under `PIONEER/rekordbox` (and
/// `PIONEER/Artwork` with artwork on), each track's ANLZ files, and audio
/// copied this run. Directory syncs are best effort (some file systems
/// refuse them); a file that won't sync is an error.
fn sync_written(plan: &ExportPlan) -> Result<(), String> {
    let root = plan.device.mount_point.as_path();
    let mut files = Vec::new();
    let mut dirs = vec![on_stick(root, "/PIONEER/rekordbox")];
    if plan.options.artwork {
        dirs.push(on_stick(root, "/PIONEER/Artwork"));
    }
    let mut i = 0;
    while let Some(dir) = dirs.get(i).cloned() {
        i += 1;
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            match e.file_type() {
                Ok(t) if t.is_dir() => dirs.push(e.path()),
                Ok(t) if t.is_file() => files.push(e.path()),
                _ => {}
            }
        }
    }
    for t in &plan.tracks {
        if !t.on_stick {
            files.push(on_stick(root, &t.dst_rel));
        }
        if t.anlz_source != AnlzSource::None {
            let anlz = on_stick(root, &anlz_dir(&t.dst_rel));
            for ext in ["DAT", "EXT", "2EX"] {
                let f = anlz.join(format!("ANLZ0000.{ext}"));
                if f.is_file() {
                    files.push(f);
                }
            }
            dirs.push(anlz);
        }
    }
    let mut failed = Vec::new();
    for f in &files {
        // Write access: Windows' FlushFileBuffers needs it. Nothing is written.
        let synced = fs::OpenOptions::new()
            .write(true)
            .open(f)
            .and_then(|file| file.sync_all());
        if let Err(e) = synced {
            // A failed track's audio may be missing; the validator reports it.
            if e.kind() != std::io::ErrorKind::NotFound {
                failed.push(format!("{}: {e}", f.display()));
            }
        }
    }
    for d in &dirs {
        if let Err(e) = fs::File::open(d).and_then(|dir| dir.sync_all()) {
            tracing::debug!(dir = %d.display(), error = %e, "directory sync refused");
        }
    }
    tracing::debug!(files = files.len(), "stick files synced");
    match failed.as_slice() {
        [] => Ok(()),
        [one] => Err(format!("fsync failed: {one}")),
        [first, rest @ ..] => Err(format!(
            "fsync failed for {} files, first {first}",
            rest.len() + 1
        )),
    }
}

#[cfg(test)]
pub(crate) mod tests;
