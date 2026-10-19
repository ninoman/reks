//! The export's progress and report as the UI shows them (phase 2 task 10),
//! worded here like [`crate::dryrun`]: [`ProgressTracker`] turns
//! [`ApplyEvent`]s into [`ProgressView`]s, [`ReportView`] turns the result
//! of [`apply`] into the report step (outcome, what was written, the history
//! backup, problems grouped by kind). The CLI prints [`ExportReport`] itself
//! (`reks rb export`).
//!
//! [`export`] is the UI's entry point: it runs the plan the dry run showed
//! through [`apply`] with a fresh [`BakenWriter`], records the sync in the
//! stick's profile ([`devices::record_sync`], D27) and returns the report.

use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

use reks_core::{CancelToken, Library};
use serde::Serialize;

use crate::apply::{
    apply, ApplyEnv, ApplyError, ApplyEvent, ApplyStage, ExportReport, ExportStatus, ManifestStatus,
};
use crate::devices::{self, DeviceId, DevicesError};
use crate::dryrun::{
    capitalise, count, device_name, track_label, Fact, Groups, NoticeGroup, TrackLabels,
};
use crate::history::HistoryPolicy;
use crate::plan::{human_bytes, ExportPlan, Selection};
use crate::validate::Finding;
use crate::writer::{BakenWriter, ExportEvent, WriterError, WriterReport};

/// Runs `plan` (made from `sel`, as the dry run showed it) on its stick:
/// prepares a [`BakenWriter`] in a temp folder, then [`apply`] with `env`.
/// `progress` gets a [`ProgressView`] per event; `labels` names the plan's
/// tracks ([`crate::dryrun::plan_track_labels`]). `profile`: the stick's
/// [`devices`] row; when the stick got a new database (not cancelled, not
/// refused), [`devices::record_sync`] records it and the report says so.
/// Never fails: a refused export is a [`ReportView`] with
/// [`Outcome::Failed`].
pub fn export(
    env: &ApplyEnv<'_>,
    plan: &ExportPlan,
    sel: &Selection,
    labels: TrackLabels,
    progress: &(dyn Fn(ProgressView) + Sync),
    cancel: &CancelToken,
    profile: Option<DeviceId>,
) -> ReportView {
    let tracker = ProgressTracker::new(labels);
    let result = writer_for(env.library, sel).and_then(|(writer, _work)| {
        apply(
            plan,
            &writer,
            env,
            &|e| progress(tracker.update(&e)),
            cancel,
        )
    });
    if let Err(e) = &result {
        tracing::warn!(error = %e, "export refused");
    }
    let mut view = ReportView::new(plan, &result, &tracker.labels);
    if let (Some(dev), Ok(report)) = (profile, &result) {
        if report.status != ExportStatus::Cancelled {
            let recorded = devices::record_sync(env.library, dev, plan, report);
            if let Err(e) = &recorded {
                tracing::warn!(error = %e, device = dev, "stick profile not recorded");
            }
            view.add_profile_record(&recorded);
        }
    }
    view
}

/// A writer for `sel` and the temp folder it works in (removed on drop).
fn writer_for(
    lib: &Library,
    sel: &Selection,
) -> Result<(BakenWriter, tempfile::TempDir), ApplyError> {
    let work = tempfile::Builder::new()
        .prefix("reks-export-")
        .tempdir()
        .map_err(|source| {
            ApplyError::Writer(WriterError::Io {
                path: std::env::temp_dir(),
                source,
            })
        })?;
    let writer = BakenWriter::prepare(lib, sel, work.path()).map_err(ApplyError::Writer)?;
    Ok((writer, work))
}

/// Where the export is, for the progress step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressView {
    pub stage: ApplyStage,
    /// What runs now: `Writing tracks`.
    pub title: String,
    /// `Track 3 of 40: Alpha – One`.
    pub detail: Option<String>,
    /// Tracks done and planned while writing; `total` 0 = no count (an
    /// indeterminate bar).
    pub done: usize,
    pub total: usize,
    /// Cancelling still keeps the old database: false once the writer
    /// replaces `export.pdb`.
    pub cancellable: bool,
    /// Warnings so far, one sentence each.
    pub warnings: Vec<String>,
}

/// Folds [`ApplyEvent`]s into the current [`ProgressView`]. Shared by the
/// writer's thread and the caller's, so it locks.
#[derive(Debug)]
pub struct ProgressTracker {
    labels: TrackLabels,
    view: Mutex<ProgressView>,
}

impl ProgressTracker {
    pub fn new(labels: TrackLabels) -> Self {
        Self {
            labels,
            view: Mutex::new(ProgressView {
                stage: ApplyStage::Preflight,
                title: stage_title(ApplyStage::Preflight).to_owned(),
                detail: None,
                done: 0,
                total: 0,
                cancellable: true,
                warnings: Vec::new(),
            }),
        }
    }

    /// Applies `event` and returns the new view.
    pub fn update(&self, event: &ApplyEvent) -> ProgressView {
        let mut v = self.view.lock().unwrap_or_else(PoisonError::into_inner);
        match event {
            ApplyEvent::Stage { stage } => {
                v.stage = *stage;
                v.title = stage_title(*stage).to_owned();
                v.detail = None;
                if *stage != ApplyStage::Write {
                    v.done = 0;
                    v.total = 0;
                }
                // From the sync on, the new database is on the stick.
                v.cancellable = matches!(
                    stage,
                    ApplyStage::Preflight | ApplyStage::Backup | ApplyStage::Write
                );
            }
            ApplyEvent::Writer { event } => match event {
                ExportEvent::Started { total } => {
                    v.done = 0;
                    v.total = *total;
                }
                ExportEvent::TrackDone {
                    done,
                    total,
                    track_id,
                } => {
                    v.done = *done;
                    v.total = *total;
                    v.detail = Some(match track_id {
                        Some(id) => format!(
                            "Track {done} of {total}: {}",
                            track_label(&self.labels, *id)
                        ),
                        None => format!("Track {done} of {total}"),
                    });
                }
                ExportEvent::Finished => {
                    v.title = "Writing the stick's database and removing stale files".to_owned();
                    v.detail = None;
                    v.cancellable = false;
                }
            },
            ApplyEvent::Warning { warning } => {
                // Not capitalised: the text starts with "rekordbox".
                v.warnings.push(format!("{warning}."));
            }
        }
        v.clone()
    }
}

fn stage_title(stage: ApplyStage) -> &'static str {
    match stage {
        ApplyStage::Preflight => "Checking the stick",
        ApplyStage::Backup => "Backing up the stick's play history",
        ApplyStage::Write => "Writing tracks",
        ApplyStage::Sync => "Flushing the written files to the stick",
        ApplyStage::Validate => "Reading the stick back to check it",
        ApplyStage::Manifest => "Recording what is on the stick",
    }
}

/// How the export ended, for the report's tone and buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Outcome {
    /// New database, read back clean.
    Success,
    /// New database with failed tracks, findings or errors.
    Problems,
    /// Cancelled; the old database is in place.
    Cancelled,
    /// Refused or stopped before the database was replaced.
    Failed,
}

/// What the report step shows; every string is final.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportView {
    pub outcome: Outcome,
    pub title: String,
    /// What the stick holds now, in a sentence or two.
    pub status: String,
    /// For Eject.
    pub mount_point: PathBuf,
    pub device_name: String,
    /// What was written and checked; empty when nothing was.
    pub facts: Vec<Fact>,
    pub history: HistoryOutcome,
    /// Failed tracks, read-back findings and errors, grouped by kind.
    pub problems: Vec<NoticeGroup>,
    pub warnings: Vec<NoticeGroup>,
}

/// What happened to the stick's play history (D23).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryOutcome {
    pub title: String,
    pub text: String,
    /// The backup folder in the app data dir.
    pub folder: Option<PathBuf>,
    /// Stick paths of the backed-up files.
    pub files: Vec<String>,
}

impl ReportView {
    pub fn new(
        plan: &ExportPlan,
        result: &Result<ExportReport, ApplyError>,
        labels: &TrackLabels,
    ) -> Self {
        let device_name = device_name(&plan.device);
        let mut view = Self {
            outcome: Outcome::Failed,
            title: String::new(),
            status: String::new(),
            mount_point: plan.device.mount_point.clone(),
            device_name,
            facts: Vec::new(),
            history: history_outcome(plan, result),
            problems: Vec::new(),
            warnings: Vec::new(),
        };
        let r = match result {
            Ok(r) => r,
            Err(e) => {
                view.title = "Export failed".to_owned();
                view.status = format!(
                    "{}. The stick keeps its previous database.",
                    capitalise(&e.to_string())
                );
                return view;
            }
        };
        let (outcome, title, status) = match r.status {
            ExportStatus::Success => (
                Outcome::Success,
                "Export finished",
                "reks wrote the stick and read it back: it matches the plan. Eject it before \
                 unplugging.",
            ),
            ExportStatus::Problems => (
                Outcome::Problems,
                "Export finished with problems",
                "The stick has a new database, but it doesn't match the plan in every point. \
                 See the problems below; plan again after fixing them.",
            ),
            ExportStatus::Cancelled => (
                Outcome::Cancelled,
                "Export cancelled",
                "The stick keeps its previous database. Audio and analysis copied before you \
                 cancelled stay on the stick; the old database doesn't list them.",
            ),
        };
        view.outcome = outcome;
        view.title = title.to_owned();
        view.status = status.to_owned();
        if r.status != ExportStatus::Cancelled {
            view.facts = facts(plan, r);
        }
        view.problems = problems(r, labels);
        let mut warnings = Groups::default();
        for w in &r.warnings {
            warnings.push("applyWarning", format!("{w}."));
        }
        view.warnings = warnings.finish(|_, _| None);
        view
    }

    /// Adds what [`devices::record_sync`] did: a fact, and a warning when it
    /// failed (the export itself is unaffected).
    pub fn add_profile_record(&mut self, recorded: &Result<usize, DevicesError>) {
        let value = match recorded {
            Ok(n) => format!(
                "{} recorded for the Devices panel",
                count(*n, "playlist", "playlists")
            ),
            Err(e) => {
                self.warnings.push(NoticeGroup {
                    kind: "profileNotRecorded".to_owned(),
                    title: format!(
                        "The stick's playlists were not recorded in its profile: {e}. The \
                         Devices panel reads the stick while it is plugged in."
                    ),
                    items: Vec::new(),
                });
                "Not recorded (see the warnings)".to_owned()
            }
        };
        self.facts.push(Fact {
            label: "Stick profile".to_owned(),
            value,
        });
    }
}

fn history_outcome(plan: &ExportPlan, result: &Result<ExportReport, ApplyError>) -> HistoryOutcome {
    let report = result.as_ref().ok();
    let replaced = report.is_some_and(|r| r.status != ExportStatus::Cancelled);
    let backup = report.and_then(|r| r.backup.as_ref());
    let files: Vec<String> = backup
        .map(|b| b.files.iter().map(|f| format!("/{}", f.rel_path)).collect())
        .unwrap_or_default();
    let folder = report.and_then(|r| r.backup_dir.clone());
    let outcome = |title: &str, text: String| HistoryOutcome {
        title: title.to_owned(),
        text,
        folder: folder.clone(),
        files: files.clone(),
    };

    if let Err(ApplyError::Backup(_)) = result {
        return outcome(
            "Play history backup failed",
            "Nothing was written, so the stick still holds its play history.".to_owned(),
        );
    }
    if !plan.history.may_hold_history() {
        return outcome(
            "No play history on this stick",
            "Players had not logged any sessions here, so nothing was lost.".to_owned(),
        );
    }
    if let Some(b) = backup {
        let bytes = b.files.iter().map(|f| f.bytes).sum();
        let copied = format!(
            "Before writing, reks copied {} ({}) from the stick to the folder below and checked \
             the copies. They keep the sessions players logged on this stick.",
            count(b.files.len(), "database file", "database files"),
            human_bytes(bytes)
        );
        let text = if replaced {
            format!("{copied} The stick's new database starts without history.")
        } else {
            format!("{copied} The stick still has them too: its database was not replaced.")
        };
        return outcome("Play history backed up", text);
    }
    if !replaced {
        return outcome(
            "Play history kept",
            "The stick's database was not replaced, so its play history is still there.".to_owned(),
        );
    }
    match plan.options.history {
        HistoryPolicy::Discard => outcome(
            "Play history discarded",
            "As you chose, the stick's play history was erased without a backup.".to_owned(),
        ),
        // A backup policy always backs up a stick that may hold history;
        // reaching here means the plan had no backup files.
        HistoryPolicy::BackUp => outcome(
            "Play history not backed up",
            "The plan listed no database files to back up.".to_owned(),
        ),
    }
}

fn facts(plan: &ExportPlan, r: &ExportReport) -> Vec<Fact> {
    let fact = |label: &str, value: String| Fact {
        label: label.to_owned(),
        value,
    };
    let mut out = Vec::new();
    if let Some(w) = &r.writer {
        out.extend(writer_facts(plan, w).into_iter().map(|(l, v)| fact(l, v)));
    }
    out.push(fact(
        "Read back",
        match &r.validation {
            Some(v) if v.is_valid() => format!(
                "{} and {}, as planned",
                count(v.tracks.len(), "track", "tracks"),
                count(
                    v.playlists.len(),
                    "playlist or folder",
                    "playlists and folders"
                )
            ),
            Some(v) => count(v.findings.len(), "problem found", "problems found"),
            None => "Not checked (see the problems)".to_owned(),
        },
    ));
    out.push(fact(
        "Library record",
        match r.manifest {
            ManifestStatus::Recorded => format!(
                "{} recorded as on this stick",
                count(r.manifest_rows, "track", "tracks")
            ),
            ManifestStatus::NoVolumeSerial => {
                "Not recorded: the system gave no volume ID for this stick".to_owned()
            }
            ManifestStatus::Skipped => "Not recorded: the export has problems".to_owned(),
            ManifestStatus::Failed => "Recording failed (see the problems)".to_owned(),
        },
    ));
    out
}

fn writer_facts(plan: &ExportPlan, w: &WriterReport) -> Vec<(&'static str, String)> {
    let mut out = vec![
        (
            "Tracks",
            format!("{} in the stick's database", w.tracks_in_database),
        ),
        (
            "Audio",
            format!(
                "{} copied, {} already on the stick",
                w.audio_copied, w.audio_kept
            ),
        ),
    ];
    let mut analysis = format!(
        "{} written, {} unchanged",
        count(w.anlz_written, "file", "files"),
        w.anlz_unchanged
    );
    if w.anlz_generated > 0 {
        analysis.push_str(&format!(
            "; {} generated (approximate)",
            count(w.anlz_generated, "track", "tracks")
        ));
    }
    out.push(("Analysis", analysis));
    if plan.options.artwork {
        out.push(("Artwork", count(w.artwork, "track", "tracks")));
    }
    if w.prune_skipped {
        out.push((
            "Stale files",
            "Kept: tracks failed, so nothing was removed".to_owned(),
        ));
    } else if w.pruned > 0 {
        out.push(("Stale files", format!("{} removed", w.pruned)));
    }
    if w.onelibrary_removed > 0 || w.onelibrary_kept > 0 {
        let mut text = format!(
            "{} removed",
            count(w.onelibrary_removed, "OneLibrary file", "OneLibrary files")
        );
        if w.onelibrary_kept > 0 {
            text.push_str(&format!(", {} could not be removed", w.onelibrary_kept));
        }
        out.push(("rekordbox OneLibrary", text));
    }
    if w.seek_tables_rebuilt > 0 || w.seek_tables_dropped > 0 {
        out.push((
            "FLAC seek tables",
            format!(
                "{} rebuilt, {} dropped",
                w.seek_tables_rebuilt, w.seek_tables_dropped
            ),
        ));
    }
    out
}

fn problems(r: &ExportReport, labels: &TrackLabels) -> Vec<NoticeGroup> {
    let mut g = Groups::default();
    for f in r.writer.iter().flat_map(|w| &w.failures) {
        let name = f
            .track_id
            .map_or_else(|| f.title.clone(), |id| track_label(labels, id));
        g.push("trackFailed", format!("{name}: {}", f.message));
    }
    for f in r.findings() {
        g.push(finding_kind(f), finding_item(f, labels));
    }
    for e in &r.errors {
        g.push("error", capitalise(e));
    }
    g.finish(|kind, items| {
        let n = items.len();
        Some(match kind {
            "trackFailed" => format!("{} could not be written", count(n, "track", "tracks")),
            "trackFinding" => format!("Read back: {}", count(n, "track problem", "track problems")),
            "playlistFinding" => format!(
                "Read back: {}",
                count(n, "playlist problem", "playlist problems")
            ),
            "databaseFinding" => format!(
                "Read back: {} with the database",
                count(n, "problem", "problems")
            ),
            _ => format!(
                "{} after the new database was written",
                count(n, "error", "errors")
            ),
        })
    })
}

fn finding_kind(f: &Finding) -> &'static str {
    match f {
        _ if f.track_id().is_some() => "trackFinding",
        Finding::PlaylistMissing { .. }
        | Finding::UnexpectedPlaylist { .. }
        | Finding::PlaylistKind { .. }
        | Finding::PlaylistOrder { .. }
        | Finding::PlaylistEntries { .. } => "playlistFinding",
        _ => "databaseFinding",
    }
}

/// `Alpha – One: analysis … is not on the stick` for track findings (the
/// `Display` text names the track by id), the `Display` text otherwise.
fn finding_item(f: &Finding, labels: &TrackLabels) -> String {
    let Some(id) = f.track_id() else {
        return capitalise(&f.to_string());
    };
    let name = track_label(labels, id);
    if let Finding::TrackNotInPdb { path, .. } = f {
        return format!("{name}: {path} is not in the stick's database");
    }
    let text = f.to_string();
    let rest = text.strip_prefix(&format!("track {id}: ")).unwrap_or(&text);
    format!("{name}: {rest}")
}

#[cfg(test)]
mod tests;
