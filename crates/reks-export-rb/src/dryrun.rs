//! The dry run as the export UI shows it (phase 2 task 10): [`DryRun`] is an
//! [`ExportPlan`] turned into display data (grouped notices, byte totals,
//! history and backup text, every string already worded), so the UI only
//! lays it out. The CLI prints the plan itself (`reks rb plan`); both use the
//! `Display` wording of [`Warning`] and [`Problem`] for anything not reworded
//! here.
//!
//! [`plan_at`] is the UI's entry point: the volume mounted exactly at a mount
//! point, rekordbox's analysis from its default folders, then [`plan`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use reks_core::{Library, PlaylistId, TrackId};
use serde::{Deserialize, Serialize};

use crate::device::{self, DeviceError, DeviceInfo, FsKind};
use crate::history::HistoryPolicy;
use crate::plan::{
    self, human_bytes, AnlzOp, CacheLocator, ExportOptions, ExportPlan, FileOp, PlanError, Problem,
    Selection, Warning,
};
use crate::rbcache::{self, RbCache};
use crate::TargetProfile;

#[derive(Debug, thiserror::Error)]
pub enum DryRunError {
    #[error(transparent)]
    Device(#[from] DeviceError),
    #[error("cannot read rekordbox's analysis folders: {0}")]
    Cache(#[from] rbcache::Error),
    #[error(transparent)]
    Plan(#[from] PlanError),
    #[error(transparent)]
    Library(#[from] reks_core::Error),
}

/// Plans exporting `sel` to the volume mounted at exactly `mount_point`
/// (not a folder inside one: the plan treats the device root as the stick),
/// with ANLZ from rekordbox's default analysis folders. Reads only; it scans
/// the analysis cache and samples file bytes, so it can take a while.
pub fn plan_at(
    lib: &Library,
    sel: &Selection,
    mount_point: &Path,
    opts: &ExportOptions,
) -> Result<ExportPlan, DryRunError> {
    let dev = device::list_devices()
        .into_iter()
        .find(|d| d.mount_point == mount_point)
        .ok_or_else(|| DeviceError::NotFound(mount_point.to_path_buf()))?;
    let cache = RbCache::scan(&rbcache::default_roots())?;
    let locator = CacheLocator::for_selection(lib, sel, cache)?;
    Ok(plan::plan(lib, sel, &dev, opts, &locator)?)
}

/// What the dry-run step shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DryRun {
    /// No blocking problems: the export may run.
    pub ready: bool,
    /// One sentence on the outcome.
    pub status: String,
    pub device: DeviceView,
    /// The target player family, e.g. `CDJ-2000NXS2`.
    pub target: String,
    /// Tracks, playlists, analysis, copies and deletes in one line each.
    pub summary: Vec<Fact>,
    /// The exported tree, depth first.
    pub playlists: Vec<PlaylistRow>,
    pub copies: FileGroup,
    pub deletes: FileGroup,
    pub space: SpaceView,
    pub history: HistoryView,
    /// Blockers, grouped by kind in order of first appearance.
    pub problems: Vec<NoticeGroup>,
    /// Everything else to know, grouped the same way. The history warning is
    /// in [`Self::history`] instead.
    pub warnings: Vec<NoticeGroup>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceView {
    pub mount_point: PathBuf,
    /// The label, else the mount point's last component.
    pub name: String,
    /// `FAT32 (msdos)`.
    pub fs_text: String,
    /// `18.4 GB free of 32.0 GB`.
    pub space_text: String,
    pub volume_serial: Option<String>,
}

/// A label and its value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fact {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistRow {
    pub id: PlaylistId,
    pub name: String,
    pub depth: usize,
    pub is_folder: bool,
    /// `12 tracks`; empty for folders.
    pub detail: String,
}

/// Files to copy, delete or back up.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileGroup {
    pub count: usize,
    pub bytes: u64,
    /// `12 files, 1.2 GB`, or `None` when empty.
    pub total_text: String,
    pub files: Vec<FileRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRow {
    /// Path on the stick.
    pub path: String,
    /// Where a copy comes from.
    pub from: Option<String>,
    pub size_text: String,
}

/// Byte totals against the stick's space.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceView {
    /// The new files fit in today's free space (stale files are deleted
    /// only after writing).
    pub fits: bool,
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub write_bytes: u64,
    pub delete_bytes: u64,
    /// Negative when the export does not fit.
    pub free_after_bytes: i64,
    /// Used space after the export, for a capacity bar (0 when it does not fit).
    pub used_after_bytes: u64,
    /// Free now, writes, deletes, free after.
    pub facts: Vec<Fact>,
}

/// The play history players left on the stick (D23) and its backup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryView {
    /// The stick holds, or may hold, history this export erases.
    pub at_risk: bool,
    pub policy: HistoryPolicy,
    pub title: String,
    pub text: String,
    /// What happens to it: the backup, or that it is discarded.
    pub action_text: Option<String>,
    /// Stick files copied to the app data dir before any write.
    pub backups: FileGroup,
}

/// Notices of one kind: a title and, for per-track or per-playlist kinds, one
/// item each.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoticeGroup {
    /// The `Warning`/`Problem` variant (`skipped`, `sourceMissing`, ...).
    pub kind: String,
    pub title: String,
    pub items: Vec<String>,
}

/// Display names for tracks named in notices: `Artist – Title`.
pub type TrackLabels = HashMap<TrackId, String>;

/// Labels for every track a warning or problem of `plan` names, from `lib`.
/// Tracks no longer in the library are left out (shown as `track <id>`).
pub fn track_labels(lib: &Library, plan: &ExportPlan) -> TrackLabels {
    labels_for(
        lib,
        plan.warnings
            .iter()
            .filter_map(warning_track)
            .chain(plan.errors.iter().filter_map(problem_track)),
    )
}

/// Labels for every track of `plan`, for the export's progress and report.
pub fn plan_track_labels(lib: &Library, plan: &ExportPlan) -> TrackLabels {
    labels_for(lib, plan.tracks.iter().map(|t| t.track_id))
}

fn labels_for(lib: &Library, ids: impl Iterator<Item = TrackId>) -> TrackLabels {
    let mut ids: Vec<TrackId> = ids.collect();
    ids.sort_unstable();
    ids.dedup();
    ids.into_iter()
        .filter_map(|id| {
            let d = lib.track(id).ok()?;
            let row = &d.track.row;
            let file = || {
                Path::new(&d.track.path)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
            };
            let label = match (row.artist.as_deref(), row.title.as_deref()) {
                (Some(a), Some(t)) if !a.is_empty() && !t.is_empty() => format!("{a} – {t}"),
                (_, Some(t)) if !t.is_empty() => t.to_owned(),
                _ => file()?,
            };
            Some((id, label))
        })
        .collect()
}

/// `labels[id]`, else `track <id>`.
pub(crate) fn track_label(labels: &TrackLabels, id: TrackId) -> String {
    labels
        .get(&id)
        .cloned()
        .unwrap_or_else(|| format!("track {id}"))
}

impl DryRun {
    /// [`Self::new`] with labels from `lib`.
    pub fn build(lib: &Library, plan: &ExportPlan) -> Self {
        Self::new(plan, &track_labels(lib, plan))
    }

    pub fn new(plan: &ExportPlan, labels: &TrackLabels) -> Self {
        let track = |id: TrackId| track_label(labels, id);
        let playlist_names: HashMap<PlaylistId, &str> = plan
            .playlists
            .iter()
            .map(|p| (p.id, p.name.as_str()))
            .collect();
        let playlist = |id: PlaylistId| {
            playlist_names
                .get(&id)
                .map_or_else(|| format!("playlist {id}"), |n| (*n).to_owned())
        };

        let mut problems = Groups::default();
        for p in &plan.errors {
            problems.push(problem_kind(p), problem_item(p, &track));
        }
        let mut warnings = Groups::default();
        for w in &plan.warnings {
            if let Warning::StickHistory { .. } = w {
                continue;
            }
            warnings.push(warning_kind(w), warning_item(w, &track, &playlist));
        }

        let ready = !plan.is_blocked();
        let n_problems = plan.errors.len();
        let status = if ready {
            "Ready to export. Nothing has been written to the stick yet.".to_owned()
        } else {
            format!(
                "Blocked by {}. Fix {} or change the options; nothing has been written.",
                count(n_problems, "problem", "problems"),
                if n_problems == 1 { "it" } else { "them" }
            )
        };

        Self {
            ready,
            status,
            device: device_view(&plan.device, plan.history.volume_serial.clone()),
            target: target_label(plan.options.target).to_owned(),
            summary: summary(plan),
            playlists: playlist_rows(plan),
            copies: file_group(&plan.copies, true),
            deletes: file_group(&plan.deletes, false),
            space: space_view(plan),
            history: history_view(plan),
            problems: problems.finish(problem_title),
            warnings: warnings.finish(warning_title),
        }
    }
}

/// `CDJ-2000NXS2`-style name of a target.
pub fn target_label(t: TargetProfile) -> &'static str {
    match t {
        TargetProfile::Cdj2000Nxs => "CDJ-2000NXS",
        TargetProfile::Cdj2000Nxs2 => "CDJ-2000NXS2",
        TargetProfile::Cdj3000 => "CDJ-3000",
        TargetProfile::XdjXz => "XDJ-XZ",
        TargetProfile::Generic => "Any player",
    }
}

pub(crate) fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn device_view(dev: &DeviceInfo, volume_serial: Option<String>) -> DeviceView {
    let kind = match &dev.fs {
        FsKind::Fat32 => "FAT32",
        FsKind::ExFat => "exFAT",
        FsKind::Hfs => "Mac OS Extended",
        FsKind::Other(name) => name.as_str(),
    };
    DeviceView {
        mount_point: dev.mount_point.clone(),
        name: device_name(dev),
        fs_text: if dev.fs_name.is_empty() || dev.fs_name.eq_ignore_ascii_case(kind) {
            kind.to_owned()
        } else {
            format!("{kind} ({})", dev.fs_name)
        },
        space_text: format!(
            "{} free of {}",
            human_bytes(dev.free_bytes),
            human_bytes(dev.total_bytes)
        ),
        volume_serial,
    }
}

/// The label, else the mount point's last component.
pub(crate) fn device_name(dev: &DeviceInfo) -> String {
    if dev.label.is_empty() {
        dev.mount_point.file_name().map_or_else(
            || dev.mount_point.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        )
    } else {
        dev.label.clone()
    }
}

fn summary(plan: &ExportPlan) -> Vec<Fact> {
    let fact = |label: &str, value: String| Fact {
        label: label.to_owned(),
        value,
    };
    let on_stick = plan.tracks.iter().filter(|t| t.on_stick).count();
    let lists = plan.playlists.iter().filter(|p| !p.is_folder).count();
    let folders = plan.playlists.len() - lists;
    let (mut cached, mut cached_bytes, mut generated, mut without) = (0, 0u64, 0, 0);
    for a in &plan.anlz {
        match a {
            AnlzOp::CopyFromCache { bytes, .. } => {
                cached += 1;
                cached_bytes += bytes;
            }
            AnlzOp::Generate { .. } => generated += 1,
            AnlzOp::Missing { .. } => without += 1,
        }
    }
    let mut analysis = vec![format!(
        "{cached} from rekordbox ({})",
        human_bytes(cached_bytes)
    )];
    if generated > 0 {
        analysis.push(format!("{generated} generated (approximate)"));
    }
    if without > 0 {
        analysis.push(format!("{without} without (no waveform, grid or cues)"));
    }
    vec![
        fact(
            "Tracks",
            format!(
                "{}: {} to copy, {on_stick} already on the stick",
                plan.tracks.len(),
                plan.copies.len()
            ),
        ),
        fact(
            "Playlists",
            if folders > 0 {
                format!("{lists} in {}", count(folders, "folder", "folders"))
            } else {
                lists.to_string()
            },
        ),
        fact("Analysis", analysis.join(", ")),
        fact("Copy", total_text(plan.copies.len(), sum(&plan.copies))),
        fact("Delete", total_text(plan.deletes.len(), sum(&plan.deletes))),
    ]
}

fn playlist_rows(plan: &ExportPlan) -> Vec<PlaylistRow> {
    let mut depth: HashMap<PlaylistId, usize> = HashMap::new();
    plan.playlists
        .iter()
        .map(|p| {
            let d = p
                .parent_id
                .and_then(|id| depth.get(&id))
                .map_or(0, |d| d + 1);
            depth.insert(p.id, d);
            PlaylistRow {
                id: p.id,
                name: p.name.clone(),
                depth: d,
                is_folder: p.is_folder,
                detail: if p.is_folder {
                    String::new()
                } else {
                    count(p.track_ids.len(), "track", "tracks")
                },
            }
        })
        .collect()
}

fn sum(ops: &[FileOp]) -> u64 {
    ops.iter().map(|f| f.bytes).sum()
}

fn total_text(n: usize, bytes: u64) -> String {
    if n == 0 {
        "None".to_owned()
    } else {
        format!("{}, {}", count(n, "file", "files"), human_bytes(bytes))
    }
}

fn file_group(ops: &[FileOp], with_source: bool) -> FileGroup {
    let bytes = sum(ops);
    FileGroup {
        count: ops.len(),
        bytes,
        total_text: total_text(ops.len(), bytes),
        files: ops
            .iter()
            .map(|f| FileRow {
                path: f.dst_rel.clone(),
                from: if with_source {
                    f.src.as_ref().map(|p| p.display().to_string())
                } else {
                    None
                },
                size_text: human_bytes(f.bytes),
            })
            .collect(),
    }
}

fn space_view(plan: &ExportPlan) -> SpaceView {
    let dev = &plan.device;
    let delete_bytes = sum(&plan.deletes);
    let free_after = plan.bytes_free_after;
    let fits = plan.bytes_to_write <= dev.free_bytes;
    let fact = |label: &str, value: String| Fact {
        label: label.to_owned(),
        value,
    };
    let free_after_text = if free_after < 0 {
        format!("{} short", human_bytes(free_after.unsigned_abs()))
    } else {
        human_bytes(free_after.unsigned_abs())
    };
    SpaceView {
        fits,
        total_bytes: dev.total_bytes,
        free_bytes: dev.free_bytes,
        write_bytes: plan.bytes_to_write,
        delete_bytes,
        free_after_bytes: free_after,
        used_after_bytes: u64::try_from(free_after)
            .map_or(0, |free| dev.total_bytes.saturating_sub(free)),
        facts: vec![
            fact("Free now", human_bytes(dev.free_bytes)),
            fact("Writes", human_bytes(plan.bytes_to_write)),
            fact("Deletes free", human_bytes(delete_bytes)),
            fact("Free after", free_after_text),
        ],
    }
}

fn history_view(plan: &ExportPlan) -> HistoryView {
    let h = &plan.history;
    let policy = plan.options.history;
    let backups = file_group(&plan.backups, false);
    if !h.may_hold_history() {
        return HistoryView {
            at_risk: false,
            policy,
            title: "No play history on this stick".to_owned(),
            text: "Players have not logged any sessions here, so the export loses nothing."
                .to_owned(),
            action_text: None,
            backups,
        };
    }
    let mut found = Vec::new();
    if h.pdb_sessions > 0 || h.pdb_entries > 0 {
        found.push(format!(
            "the stick's export.pdb holds {} with {}",
            count(
                h.pdb_sessions as usize,
                "history session",
                "history sessions"
            ),
            count(h.pdb_entries as usize, "played track", "played tracks")
        ));
    }
    if h.pdb_unreadable {
        found.push("the stick's export.pdb can't be read, so it may hold play history".to_owned());
    }
    if h.onelibrary_db {
        found.push(
            "an exportLibrary.db (OneLibrary) is present and players may have logged plays there"
                .to_owned(),
        );
    }
    let text = format!(
        "{}. Exporting rebuilds the stick's database, which erases it.",
        capitalise(&found.join("; "))
    );
    let confirmed = plan.options.discard_history_confirmed;
    let action_text = match policy {
        HistoryPolicy::BackUp => format!(
            "Before writing anything, reks copies {} ({}) to its data folder and checks the \
             copies; if the backup fails, nothing is written. The report after the export \
             shows where the copies are.",
            count(backups.count, "database file", "database files"),
            human_bytes(backups.bytes)
        ),
        HistoryPolicy::Discard if confirmed => {
            "You chose to discard it: it is erased without a backup.".to_owned()
        }
        HistoryPolicy::Discard => {
            "Discarding it needs your confirmation in the options.".to_owned()
        }
    };
    HistoryView {
        at_risk: true,
        policy,
        title: "This stick holds play history".to_owned(),
        text,
        action_text: Some(action_text),
        backups,
    }
}

pub(crate) fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// Groups notices by kind, keeping the order kinds first appear in.
#[derive(Default)]
pub(crate) struct Groups {
    order: Vec<&'static str>,
    items: HashMap<&'static str, Vec<String>>,
}

impl Groups {
    pub(crate) fn push(&mut self, kind: &'static str, item: String) {
        let items = self.items.entry(kind).or_insert_with(|| {
            self.order.push(kind);
            Vec::new()
        });
        items.push(item);
    }

    /// `title(kind, items)` titles a list of items; for kinds where it gives
    /// `None`, each item is a sentence and they become the title.
    pub(crate) fn finish(
        mut self,
        title: impl Fn(&str, &[String]) -> Option<String>,
    ) -> Vec<NoticeGroup> {
        self.order
            .iter()
            .map(|&kind| {
                let items = self.items.remove(kind).unwrap_or_default();
                match title(kind, &items) {
                    Some(title) => NoticeGroup {
                        kind: kind.to_owned(),
                        title,
                        items,
                    },
                    // One sentence per notice: no list.
                    None => NoticeGroup {
                        kind: kind.to_owned(),
                        title: items.join(" "),
                        items: Vec::new(),
                    },
                }
            })
            .collect()
    }
}

fn warning_track(w: &Warning) -> Option<TrackId> {
    match w {
        Warning::Audio { track_id, .. } | Warning::Skipped { track_id, .. } => Some(*track_id),
        _ => None,
    }
}

fn problem_track(p: &Problem) -> Option<TrackId> {
    match p {
        Problem::SourceMissing { track_id, .. }
        | Problem::SourceUnreadable { track_id, .. }
        | Problem::Unsupported { track_id, .. }
        | Problem::BadPath { track_id, .. } => Some(*track_id),
        _ => None,
    }
}

fn warning_kind(w: &Warning) -> &'static str {
    match w {
        Warning::ExFat { .. } => "exFat",
        Warning::Hfs => "hfs",
        Warning::NotRemovable => "notRemovable",
        Warning::RekordboxStick { .. } => "rekordboxStick",
        Warning::Audio { .. } => "audio",
        Warning::Skipped { .. } => "skipped",
        Warning::StaleKept { .. } => "staleKept",
        Warning::EmptyPlaylist { .. } => "emptyPlaylist",
        Warning::StickHistory { .. } => "stickHistory",
        Warning::NoVolumeSerial => "noVolumeSerial",
    }
}

fn warning_item(
    w: &Warning,
    track: &impl Fn(TrackId) -> String,
    playlist: &impl Fn(PlaylistId) -> String,
) -> String {
    match w {
        Warning::Audio { track_id, message } => format!("{}: {message}", track(*track_id)),
        Warning::Skipped { track_id, reason } => format!("{}: {reason}", track(*track_id)),
        Warning::EmptyPlaylist { id } => playlist(*id),
        Warning::ExFat { target } => format!(
            "The stick is exFAT, which older players don't read; FAT32 is the safe choice \
             (target: {}).",
            target_label(*target)
        ),
        Warning::RekordboxStick { found } => format!(
            "rekordbox wrote this stick ({}). reks rewrites its Device Library completely, and \
             rekordbox will show its \"out of matching\" caution afterwards.",
            found.join(", ")
        ),
        other => capitalise(&format!("{other}.")),
    }
}

fn warning_title(kind: &str, items: &[String]) -> Option<String> {
    let n = items.len();
    match kind {
        "audio" => Some(format!(
            "{} the target player may not handle well",
            count(n, "track", "tracks")
        )),
        "skipped" => Some(format!("{} left out", count(n, "track", "tracks"))),
        "emptyPlaylist" => Some(format!(
            "{} empty after leaving tracks out",
            count(n, "playlist ends up", "playlists end up")
        )),
        _ => None,
    }
}

fn problem_kind(p: &Problem) -> &'static str {
    match p {
        Problem::NothingSelected => "nothingSelected",
        Problem::PlaylistNotFound { .. } => "playlistNotFound",
        Problem::UnsupportedFilesystem { .. } => "unsupportedFilesystem",
        Problem::SourceMissing { .. } => "sourceMissing",
        Problem::SourceUnreadable { .. } => "sourceUnreadable",
        Problem::Unsupported { .. } => "unsupported",
        Problem::BadPath { .. } => "badPath",
        Problem::NotEnoughSpace { .. } => "notEnoughSpace",
        Problem::DiscardNotConfirmed => "discardNotConfirmed",
    }
}

fn problem_item(p: &Problem, track: &impl Fn(TrackId) -> String) -> String {
    match p {
        Problem::SourceMissing { track_id, path } => {
            format!("{}: {}", track(*track_id), path.display())
        }
        Problem::SourceUnreadable {
            track_id,
            path,
            message,
        } => format!("{}: {}: {message}", track(*track_id), path.display()),
        Problem::Unsupported { track_id, message } => format!("{}: {message}", track(*track_id)),
        Problem::BadPath {
            track_id,
            path,
            message,
        } => format!("{}: {path}: {message}", track(*track_id)),
        Problem::NotEnoughSpace { needed, free } => format!(
            "Not enough space: the export writes {}, the stick has {} free.",
            human_bytes(*needed),
            human_bytes(*free)
        ),
        Problem::DiscardNotConfirmed => "Discarding the stick's play history needs your \
                                         confirmation: tick the box in the options, or back \
                                         it up instead."
            .to_owned(),
        other => capitalise(&format!("{other}.")),
    }
}

fn problem_title(kind: &str, items: &[String]) -> Option<String> {
    let n = items.len();
    let tracks = count(n, "track", "tracks");
    match kind {
        "sourceMissing" => Some(format!("{tracks}: file not found")),
        "sourceUnreadable" => Some(format!("{tracks}: file can't be read")),
        "unsupported" => Some(format!("{tracks} the target player can't play")),
        "badPath" => Some(format!("{tracks} with a stick path players can't use")),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
