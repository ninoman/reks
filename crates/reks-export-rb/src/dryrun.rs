//! The export review as the UI shows it (phase 2 task 10, D42): [`DryRun`]
//! is the part of an [`ExportPlan`] a user decides on (the overview, track
//! issues, blockers, warnings that affect the export, the play-history line
//! and guard, the OneLibrary removal), every string already worded, so the UI
//! only lays it out. The full detail (files, paths, technical notes) is the
//! CLI's (`reks rb plan`); both use the `Display` wording of [`Warning`] and
//! [`Problem`] for anything not reworded here.
//!
//! [`plan_at`] is the UI's entry point: the volume mounted exactly at a mount
//! point, rekordbox's analysis from its default folders, then [`plan`] and
//! the OneLibrary step ([`plan::plan_onelibrary`]).

use std::collections::{HashMap, HashSet};
use std::path::Path;

use reks_core::{Library, PlaylistId, TrackId};
use serde::{Deserialize, Serialize};

use crate::device::{self, DeviceError, DeviceInfo};
use crate::history::HistoryPolicy;
use crate::onelibrary::{self as one, OneLibraryExport};
use crate::plan::{
    self, human_bytes, CacheLocator, ExportOptions, ExportPlan, FileOp, OpReason, PlanError,
    Problem, Selection, Warning,
};
use crate::rbcache::{self, RbCache};
use crate::TargetProfile;

mod review;

pub(crate) use review::issues;
pub use review::{Issue, IssueAction, IssueKind, IssueTrack, Overview};

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
/// with ANLZ from rekordbox's default analysis folders and the library's
/// analysis cache ([`plan::plan`]), then the OneLibrary step through
/// `onelibrary` with the key from [`one::resolve_key`]. Reads only; it scans
/// the analysis cache and samples file bytes, so it can take a while.
pub fn plan_at(
    lib: &Library,
    sel: &Selection,
    mount_point: &Path,
    opts: &ExportOptions,
    onelibrary: &dyn OneLibraryExport,
) -> Result<ExportPlan, DryRunError> {
    let dev = device::list_devices()
        .into_iter()
        .find(|d| d.mount_point == mount_point)
        .ok_or_else(|| DeviceError::NotFound(mount_point.to_path_buf()))?;
    let cache = RbCache::scan(&rbcache::default_roots())?;
    let locator = CacheLocator::for_selection(lib, sel, cache)?;
    let mut plan = plan::plan(lib, sel, &dev, opts, &locator)?;
    let key = one::resolve_key();
    plan::plan_onelibrary(&mut plan, onelibrary, key.as_ref(), lib)?;
    Ok(plan)
}

/// What the dry-run step shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DryRun {
    /// No blocking problems: the export may run.
    pub ready: bool,
    /// The headline and the three numbers (D42).
    pub overview: Overview,
    /// Track problems and what can be done about them; none blocks.
    pub issues: Vec<Issue>,
    /// The settings in use, one short phrase each.
    pub settings: Vec<String>,
    pub history: HistoryView,
    /// Blockers, grouped by kind in order of first appearance.
    pub problems: Vec<NoticeGroup>,
    /// What the user should know before exporting (file system, empty
    /// playlists, OneLibrary not written), grouped the same way. The history
    /// warning is in [`Self::history`], left-out tracks in [`Self::issues`].
    /// The rest (rekordbox wrote the stick, audio notes, smart playlists
    /// fixed, ...) is in the plan and the CLI's dry run, not here.
    pub warnings: Vec<NoticeGroup>,
    /// Exported tracks whose audio warning the converter's Older players
    /// preset fixes ([`ExportPlan::convertible`]); the view offers
    /// "Convert…". Left-out ones are in their issue's action.
    #[serde(default)]
    pub convertible: Vec<TrackId>,
    /// One sentence on them, `None` when there are none.
    #[serde(default)]
    pub convert_text: Option<String>,
    /// The stick's `exportLibrary.db` is deleted because this export writes
    /// none ([`Warning::OneLibraryRemoved`]); the view asks for
    /// [`ExportOptions::onelibrary_removal_confirmed`](crate::plan::ExportOptions::onelibrary_removal_confirmed).
    #[serde(default)]
    pub onelibrary_removal: Option<OneLibraryRemovalView>,
}

/// The stick loses its OneLibrary library: what, why, and for which players.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OneLibraryRemovalView {
    pub title: String,
    pub text: String,
    /// The OneLibrary files deleted.
    pub files: FileGroup,
    /// The confirmation's wording.
    pub confirm_text: String,
    /// The user confirmed it.
    pub confirmed: bool,
}

/// A label and its value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fact {
    pub label: String,
    pub value: String,
}

/// Files the export deletes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileGroup {
    pub count: usize,
    pub bytes: u64,
    /// `12 files, 1.2 GB`, or `None` when empty.
    pub total_text: String,
    /// The first [`FILE_LIST_CAP`] files; `count` and `bytes` cover all of them.
    pub files: Vec<FileRow>,
}

/// Rows of a [`FileGroup`] sent to the UI. A 30k-track export has tens of
/// thousands of copies, which the wizard cannot usefully list (audit P3-7).
pub const FILE_LIST_CAP: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRow {
    /// Path on the stick.
    pub path: String,
    pub size_text: String,
}

/// The play history players left on the stick (D23).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryView {
    /// The stick holds, or may hold, history this export erases.
    pub at_risk: bool,
    /// One plain sentence for the review; empty when there is nothing to say.
    #[serde(default)]
    pub summary: String,
    pub policy: HistoryPolicy,
    /// The export guard (Phase 2.5): sessions the library has not imported.
    /// `None` when there are none.
    #[serde(default)]
    pub guard: Option<GuardView>,
}

/// Sessions on the stick the library lacks, and the choice the export needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuardView {
    pub title: String,
    pub text: String,
    /// One line per session: `HISTORY 001: 12 plays`.
    pub sessions: Vec<String>,
    /// The user has chosen; [`HistoryView::policy`] is the choice.
    pub chosen: bool,
    /// "Import now" can run (the sessions could be read).
    pub can_import: bool,
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
    let mut labels = labels_for(
        lib,
        plan.warnings
            .iter()
            .filter_map(skipped_track)
            .chain(plan.errors.iter().filter_map(problem_track)),
    );
    labels.extend(divider_labels(plan));
    labels
}

/// Labels for every track of `plan` and the ones it leaves out, for the
/// export's progress and report.
pub fn plan_track_labels(lib: &Library, plan: &ExportPlan) -> TrackLabels {
    let mut labels = labels_for(
        lib,
        plan.tracks
            .iter()
            .filter(|t| !t.is_divider())
            .map(|t| t.track_id)
            .chain(plan.warnings.iter().filter_map(skipped_track)),
    );
    labels.extend(divider_labels(plan));
    labels
}

/// `Divider "<label>"` for the plan's dividers, which are not library tracks.
fn divider_labels(plan: &ExportPlan) -> impl Iterator<Item = (TrackId, String)> + '_ {
    plan.tracks.iter().filter_map(|t| {
        t.divider
            .as_ref()
            .map(|d| (t.track_id, format!("Divider \"{}\"", d.label)))
    })
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
            if let Some(kind) = review_kind(w) {
                warnings.push(kind, warning_item(w, &playlist));
            }
        }
        let left_out: HashSet<TrackId> = plan.warnings.iter().filter_map(skipped_track).collect();
        let convertible: Vec<TrackId> = plan
            .convertible
            .iter()
            .copied()
            .filter(|id| !left_out.contains(id))
            .collect();
        Self {
            ready: !plan.is_blocked(),
            overview: review::overview(plan, &device_name(&plan.device)),
            issues: review::issues(plan, labels),
            settings: review::settings_line(plan),
            history: history_view(plan),
            problems: problems.finish(problem_title),
            warnings: warnings.finish(warning_title),
            onelibrary_removal: onelibrary_removal_view(plan),
            // Exported tracks only carry warnings for the target; "older
            // players" is only the user's concern when the stick must play on
            // any player.
            convert_text: (!convertible.is_empty()
                && plan.options.target == TargetProfile::Generic)
                .then(|| {
                    format!(
                        "{} may not play on older players. Converting makes a copy they can \
                     play; cues, grid and playlists stay.",
                        count(convertible.len(), "track", "tracks")
                    )
                }),
            convertible,
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

fn file_group(ops: &[FileOp]) -> FileGroup {
    let bytes = sum(ops);
    FileGroup {
        count: ops.len(),
        bytes,
        total_text: total_text(ops.len(), bytes),
        files: ops
            .iter()
            .take(FILE_LIST_CAP)
            .map(|f| FileRow {
                path: f.dst_rel.clone(),
                size_text: human_bytes(f.bytes),
            })
            .collect(),
    }
}

fn onelibrary_removal_view(plan: &ExportPlan) -> Option<OneLibraryRemovalView> {
    let no_key = plan.warnings.iter().find_map(|w| match w {
        Warning::OneLibraryRemoved { no_key, .. } => Some(*no_key),
        _ => None,
    })?;
    let removed: Vec<FileOp> = plan
        .deletes
        .iter()
        .filter(|f| f.reason == OpReason::OneLibraryRemoved)
        .cloned()
        .collect();
    let why = if no_key {
        "This build of reks has no OneLibrary key (use a release build or set \
         REKS_ONELIBRARY_KEY), so it writes only the Device Library (export.pdb)"
    } else {
        "OneLibrary is off, so this export writes only the Device Library (export.pdb)"
    };
    Some(OneLibraryRemovalView {
        title: "The stick's OneLibrary library is deleted".to_owned(),
        text: format!(
            "{why}, and the stick's exportLibrary.db is deleted with the files below. Players \
             that read only OneLibrary ({}) will find no library on this stick. Players that \
             read export.pdb are not affected.",
            plan::ONELIBRARY_ONLY_PLAYERS.join(", ")
        ),
        files: file_group(&removed),
        confirm_text: "I understand exportLibrary.db is deleted and OneLibrary-only players \
                       won't find a library on this stick."
            .to_owned(),
        confirmed: plan.options.onelibrary_removal_confirmed,
    })
}

fn history_view(plan: &ExportPlan) -> HistoryView {
    let policy = plan.options.history;
    if !plan.history.may_hold_history() {
        return HistoryView {
            at_risk: false,
            summary: String::new(),
            policy,
            guard: None,
        };
    }
    let confirmed = plan.options.discard_history_confirmed;
    let summary = match policy {
        HistoryPolicy::Import if !plan.guard.sessions.is_empty() => {
            "Your stick's play history is added to your library before anything is written."
        }
        HistoryPolicy::Import | HistoryPolicy::BackUp => {
            "Your stick's play history is saved to your library before anything is written."
        }
        HistoryPolicy::Discard if confirmed => {
            "Your stick's play history will be erased, as you chose."
        }
        HistoryPolicy::Discard => "Discarding your stick's play history needs your confirmation.",
    }
    .to_owned();
    HistoryView {
        at_risk: true,
        summary,
        policy,
        guard: guard_view(plan),
    }
}

fn guard_view(plan: &ExportPlan) -> Option<GuardView> {
    let g = &plan.guard;
    if !g.needs_choice() {
        return None;
    }
    let pdb = if g.unreadable.is_some() {
        plan.history.pdb_sessions as usize
    } else {
        g.sessions.len()
    };
    let n = pdb + g.onelibrary_sessions.len();
    let title = format!(
        "{} not in your library",
        count(n, "session on this stick is", "sessions on this stick are")
    );
    let mut text = match &g.unreadable {
        Some(message) => format!(
            "reks can't read the stick's history sessions ({message}), so it can't tell \
             whether they are imported, and can't import them now."
        ),
        None => "Exporting erases them from the stick. Import now adds them to your library's \
                 history before writing; Back up and continue keeps a copy you can import \
                 later; Continue and discard erases them."
            .to_owned(),
    };
    if g.unreadable.is_none() {
        text.push_str(
            " Imported sessions count in play counts; you can mark any of them as practice in \
             History later.",
        );
    }
    if !g.sessions.is_empty() && !g.onelibrary_sessions.is_empty() {
        text.push_str(
            " The stick's two libraries import as separate sessions: if a player logged the \
             same plays in both, mark one as practice.",
        );
    }
    let pdb_lines = g.sessions.iter().map(|s| (s, ""));
    let ol_lines = g.onelibrary_sessions.iter().map(|s| (s, " (OneLibrary)"));
    let sessions: Vec<String> = pdb_lines
        .chain(ol_lines)
        .map(|(s, library)| {
            let mut line = format!(
                "{}{library}: {}",
                s.name,
                count(
                    s.entries as usize,
                    if s.extends { "new play" } else { "play" },
                    if s.extends { "new plays" } else { "plays" }
                )
            );
            if s.extends {
                line.push_str(", adds to the imported session");
            }
            line
        })
        .collect();
    Some(GuardView {
        title,
        text,
        sessions,
        chosen: plan.options.history_chosen,
        can_import: g.unreadable.is_none(),
    })
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

/// The kind of a warning the review shows (what affects the export), `None`
/// for the rest, which only the plan and the CLI's dry run list.
fn review_kind(w: &Warning) -> Option<&'static str> {
    match w {
        Warning::ExFat { .. } => Some("exFat"),
        Warning::Hfs => Some("hfs"),
        Warning::EmptyPlaylist { .. } => Some("emptyPlaylist"),
        Warning::OneLibraryNoKey => Some("oneLibraryNoKey"),
        _ => None,
    }
}

fn skipped_track(w: &Warning) -> Option<TrackId> {
    match w {
        Warning::Skipped { track_id, .. } => Some(*track_id),
        _ => None,
    }
}

fn problem_track(p: &Problem) -> Option<TrackId> {
    match p {
        Problem::BadPath { track_id, .. } => Some(*track_id),
        _ => None,
    }
}

fn warning_item(w: &Warning, playlist: &impl Fn(PlaylistId) -> String) -> String {
    match w {
        Warning::EmptyPlaylist { id } => playlist(*id),
        Warning::ExFat { target } => format!(
            "The stick is exFAT, which older players don't read; FAT32 is the safe choice \
             (target: {}).",
            target_label(*target)
        ),
        other => capitalise(&format!("{other}.")),
    }
}

fn warning_title(kind: &str, items: &[String]) -> Option<String> {
    (kind == "emptyPlaylist").then(|| {
        format!(
            "{} empty after leaving tracks out",
            count(items.len(), "playlist ends up", "playlists end up")
        )
    })
}

fn problem_kind(p: &Problem) -> &'static str {
    match p {
        Problem::NothingSelected => "nothingSelected",
        Problem::PlaylistNotFound { .. } => "playlistNotFound",
        Problem::UnsupportedFilesystem { .. } => "unsupportedFilesystem",
        Problem::BadPath { .. } => "badPath",
        Problem::NotEnoughSpace { .. } => "notEnoughSpace",
        Problem::DiscardNotConfirmed => "discardNotConfirmed",
        Problem::HistoryNotImported { .. } => "historyNotImported",
        Problem::HistoryImportUnavailable { .. } => "historyImportUnavailable",
        Problem::OneLibraryUnavailable => "oneLibraryUnavailable",
        Problem::OneLibraryHistoryNotImported { .. } => "oneLibraryHistoryNotImported",
        Problem::OneLibraryRemovalNotConfirmed => "oneLibraryRemovalNotConfirmed",
    }
}

fn problem_item(p: &Problem, track: &impl Fn(TrackId) -> String) -> String {
    match p {
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
                                         confirmation under the play history, or back it up \
                                         instead."
            .to_owned(),
        Problem::HistoryNotImported { sessions } => format!(
            "The stick holds {} not imported into your library. Choose what happens to {} \
             under the play history.",
            count(
                *sessions as usize,
                "play history session",
                "play history sessions"
            ),
            if *sessions == 1 { "it" } else { "them" }
        ),
        Problem::HistoryImportUnavailable { message } => format!(
            "The stick's play history can't be imported ({message}). Back it up or discard it \
             instead."
        ),
        Problem::OneLibraryRemovalNotConfirmed => "Deleting the stick's exportLibrary.db needs \
                                                   your confirmation: tick the box under \
                                                   \"The stick's OneLibrary library is \
                                                   deleted\"."
            .to_owned(),
        other => capitalise(&format!("{other}.")),
    }
}

fn problem_title(kind: &str, items: &[String]) -> Option<String> {
    let n = items.len();
    let tracks = count(n, "track", "tracks");
    match kind {
        "badPath" => Some(format!("{tracks} with a stick path players can't use")),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
