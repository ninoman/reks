//! The review's plain-language parts (D42): the overview tiles, the track
//! issues with their fixes, and the one-line settings in use. Track problems
//! never block; each [`Issue`] says which tracks are left out (or get
//! approximate analysis) and which fixes apply, so the UI only lays it out.

use std::collections::{BTreeMap, HashSet};

use reks_core::{AnlzPreference, PlaylistId, TrackId};
use serde::{Deserialize, Serialize};

use super::{count, target_label, track_label, TrackLabels, FILE_LIST_CAP};
use crate::plan::{human_bytes, AnlzOp, ExportPlan, MissingAnalysis, SkipReason, Warning};

/// The review's headline and three numbers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    /// `Ready to export to Club stick`, `Ready to export · 12 tracks will be left out`.
    pub headline: String,
    /// `3 playlists`, `5 playlists in 2 folders`.
    pub playlists_text: String,
    /// `54 tracks to copy`.
    pub copy_text: String,
    /// `1.2 GB · 3 already on the stick`.
    pub copy_detail: String,
    /// `16.1 GB free after export`, or how much is missing.
    pub space_text: String,
    /// `frees 640 MB`, empty when nothing is deleted.
    pub space_detail: String,
    /// The new files fit in today's free space (stale files are deleted
    /// only after writing).
    pub fits: bool,
    /// Library tracks the export writes (dividers not counted).
    pub tracks_exported: usize,
    /// Those plus the tracks left out.
    pub tracks_total: usize,
    /// The primary button: `Export`, or `Export 412 of 424 tracks`.
    pub export_label: String,
}

/// What an issue is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IssueKind {
    /// Left out: the file is gone or moved.
    Missing,
    /// Left out: the file can't be read.
    Unreadable,
    /// Left out: the stick's players can't play it.
    Unsupported,
    /// Left out: no analysis, and the settings leave such tracks out.
    NoAnalysis,
    /// Written with approximate (generated) analysis.
    ApproximateAnalysis,
}

/// A fix the issue card offers. Each one replans afterwards.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum IssueAction {
    /// The find-missing-files flow.
    FindFiles,
    /// The converter's Older players preset, for these tracks.
    Convert { track_ids: Vec<TrackId> },
    /// Run the full analysis on the issue's tracks.
    Analyse,
    /// Leave tracks without analysis out of this export.
    LeaveOut,
    /// Remove the ticked tracks from this stick's (non-smart) playlists.
    RemoveFromPlaylists,
}

/// One card in the review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub kind: IssueKind,
    /// `12 tracks are missing`.
    pub title: String,
    /// One or two sentences: what happens to them.
    pub detail: String,
    /// The line the card collapses to after "Keep as it is".
    pub kept_text: String,
    /// Every track of the issue.
    pub track_ids: Vec<TrackId>,
    /// The first [`FILE_LIST_CAP`] of them, for display.
    pub tracks: Vec<IssueTrack>,
    pub actions: Vec<IssueAction>,
}

/// A track named in an issue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueTrack {
    pub track_id: TrackId,
    /// `Artist – Title`.
    pub label: String,
    /// The last known path, or the check that failed.
    pub detail: String,
    /// The library playlists on this stick it can be removed from (not smart).
    pub playlists: Vec<PlaylistId>,
    /// Names of the stick's playlists that hold it, smart ones marked.
    pub in_text: String,
}

const KEEP_IN_PLAYLISTS: &str = "They are left out of this export and stay in your playlists.";

/// The issues of `plan`, in a fixed order: missing, unreadable, unsupported,
/// no analysis, approximate analysis.
pub fn issues(plan: &ExportPlan, labels: &TrackLabels) -> Vec<Issue> {
    let mut left_out: BTreeMap<SkipReason, Vec<(TrackId, &str)>> = BTreeMap::new();
    for w in &plan.warnings {
        if let Warning::Skipped {
            track_id,
            reason,
            detail,
        } = w
        {
            left_out
                .entry(*reason)
                .or_default()
                .push((*track_id, detail.as_str()));
        }
    }
    let target = target_label(plan.options.target);
    let convertible: HashSet<TrackId> = plan.convertible.iter().copied().collect();
    let mut out = Vec::new();
    for (reason, tracks) in &left_out {
        let n = tracks.len();
        let ids: Vec<TrackId> = tracks.iter().map(|(id, _)| *id).collect();
        let (kind, title, detail, mut actions) = match reason {
            SkipReason::Missing => (
                IssueKind::Missing,
                format!("{} missing", count(n, "track is", "tracks are")),
                format!("Their files are gone or moved. {KEEP_IN_PLAYLISTS}"),
                vec![IssueAction::FindFiles],
            ),
            SkipReason::Unreadable => (
                IssueKind::Unreadable,
                format!("{} can't be read", count(n, "track", "tracks")),
                format!("reks can't open or check their files. {KEEP_IN_PLAYLISTS}"),
                vec![IssueAction::FindFiles],
            ),
            SkipReason::Unsupported => {
                let fixable: Vec<TrackId> = ids
                    .iter()
                    .copied()
                    .filter(|id| convertible.contains(id))
                    .collect();
                let players = if plan.options.target == crate::TargetProfile::Generic {
                    "every player".to_owned()
                } else {
                    format!("{target} and newer")
                };
                (
                    IssueKind::Unsupported,
                    format!(
                        "{} won't play on this stick's players",
                        count(n, "track", "tracks")
                    ),
                    format!("The stick must play on {players}. {KEEP_IN_PLAYLISTS}"),
                    if fixable.is_empty() {
                        Vec::new()
                    } else {
                        vec![IssueAction::Convert { track_ids: fixable }]
                    },
                )
            }
            SkipReason::NoAnalysis => (
                IssueKind::NoAnalysis,
                format!("{} no analysis", count(n, "track has", "tracks have")),
                format!(
                    "Neither reks nor rekordbox has analysed them, and Settings › Export \
                     leaves such tracks out. {KEEP_IN_PLAYLISTS}"
                ),
                vec![IssueAction::Analyse],
            ),
        };
        actions.push(IssueAction::RemoveFromPlaylists);
        let kept_text = format!(
            "{} left out, still in your playlists",
            count(n, "track", "tracks")
        );
        out.push(Issue {
            kind,
            title,
            detail,
            kept_text,
            tracks: tracks
                .iter()
                .take(FILE_LIST_CAP)
                .map(|(id, detail)| issue_track(plan, labels, *id, detail))
                .collect(),
            track_ids: ids,
            actions,
        });
    }
    if let Some(issue) = approximate(plan, labels) {
        out.push(issue);
    }
    out
}

/// Tracks written with generated analysis ([`MissingAnalysis::Generate`]);
/// dividers always are and are not counted.
fn approximate(plan: &ExportPlan, labels: &TrackLabels) -> Option<Issue> {
    let dividers: HashSet<TrackId> = plan
        .tracks
        .iter()
        .filter(|t| t.is_divider())
        .map(|t| t.track_id)
        .collect();
    let ids: Vec<TrackId> = plan
        .anlz
        .iter()
        .filter_map(|a| match a {
            AnlzOp::Generate { track_id } if !dividers.contains(track_id) => Some(*track_id),
            _ => None,
        })
        .collect();
    if ids.is_empty() {
        return None;
    }
    let n = ids.len();
    Some(Issue {
        kind: IssueKind::ApproximateAnalysis,
        title: format!(
            "{} approximate analysis",
            count(n, "track gets", "tracks get")
        ),
        detail: "Neither reks nor rekordbox has analysed them; a quick waveform and beatgrid \
                 are generated while writing."
            .to_owned(),
        kept_text: format!(
            "{} with approximate analysis",
            count(n, "track goes on the stick", "tracks go on the stick")
        ),
        tracks: ids
            .iter()
            .take(FILE_LIST_CAP)
            .map(|id| IssueTrack {
                track_id: *id,
                label: track_label(labels, *id),
                detail: String::new(),
                playlists: Vec::new(),
                in_text: String::new(),
            })
            .collect(),
        track_ids: ids,
        actions: vec![IssueAction::Analyse, IssueAction::LeaveOut],
    })
}

fn issue_track(plan: &ExportPlan, labels: &TrackLabels, id: TrackId, detail: &str) -> IssueTrack {
    let mut playlists = Vec::new();
    let mut names = Vec::new();
    for p in plan.playlists.iter().filter(|p| p.left_out.contains(&id)) {
        if p.smart {
            names.push(format!("{} (smart)", p.name));
        } else {
            playlists.push(p.id);
            names.push(p.name.clone());
        }
    }
    IssueTrack {
        track_id: id,
        label: track_label(labels, id),
        detail: detail.to_owned(),
        playlists,
        in_text: if names.is_empty() {
            String::new()
        } else {
            format!("In: {}", names.join(", "))
        },
    }
}

/// Library tracks the export leaves out, once each.
fn left_out_count(plan: &ExportPlan) -> usize {
    plan.warnings
        .iter()
        .filter_map(|w| match w {
            Warning::Skipped { track_id, .. } => Some(*track_id),
            _ => None,
        })
        .collect::<HashSet<_>>()
        .len()
}

pub fn overview(plan: &ExportPlan, device_name: &str) -> Overview {
    let exported = plan.tracks.iter().filter(|t| !t.is_divider()).count();
    let left_out = left_out_count(plan);
    let total = exported + left_out;
    let lists = plan.playlists.iter().filter(|p| !p.is_folder).count();
    let folders = plan.playlists.len() - lists;
    let dividers: HashSet<TrackId> = plan
        .tracks
        .iter()
        .filter(|t| t.is_divider())
        .map(|t| t.track_id)
        .collect();
    let copies: Vec<_> = plan
        .copies
        .iter()
        .filter(|c| !c.track_id.is_some_and(|id| dividers.contains(&id)))
        .collect();
    let copy_bytes: u64 = copies.iter().map(|c| c.bytes).sum();
    let on_stick = plan
        .tracks
        .iter()
        .filter(|t| !t.is_divider() && t.on_stick)
        .count();
    let freed: u64 = plan.deletes.iter().map(|d| d.bytes).sum();
    let headline = if plan.is_blocked() {
        format!("Can't export to {device_name} yet")
    } else if left_out > 0 {
        format!(
            "Ready to export · {} will be left out",
            count(left_out, "track", "tracks")
        )
    } else {
        format!("Ready to export to {device_name}")
    };
    Overview {
        headline,
        playlists_text: if folders > 0 {
            format!(
                "{} in {}",
                count(lists, "playlist", "playlists"),
                count(folders, "folder", "folders")
            )
        } else {
            count(lists, "playlist", "playlists")
        },
        copy_text: format!("{} to copy", count(copies.len(), "track", "tracks")),
        copy_detail: if on_stick > 0 {
            format!(
                "{} · {on_stick} already on the stick",
                human_bytes(copy_bytes)
            )
        } else {
            human_bytes(copy_bytes)
        },
        space_text: if plan.bytes_free_after < 0 {
            format!(
                "{} short",
                human_bytes(plan.bytes_free_after.unsigned_abs())
            )
        } else {
            format!(
                "{} free after export",
                human_bytes(plan.bytes_free_after.unsigned_abs())
            )
        },
        space_detail: if freed > 0 {
            format!("frees {}", human_bytes(freed))
        } else {
            String::new()
        },
        fits: plan.bytes_to_write <= plan.device.free_bytes,
        tracks_exported: exported,
        tracks_total: total,
        export_label: if left_out > 0 {
            format!("Export {exported} of {}", count(total, "track", "tracks"))
        } else {
            "Export".to_owned()
        },
    }
}

/// The settings this export is written with, one short phrase each.
pub fn settings_line(plan: &ExportPlan) -> Vec<String> {
    let o = &plan.options;
    let mut out = vec![target_label(o.target).to_owned()];
    out.push(
        match o.anlz_preference {
            AnlzPreference::RekordboxFirst => "rekordbox analysis, then reks",
            AnlzPreference::ReksFirst => "reks analysis, then rekordbox",
            AnlzPreference::RekordboxOnly => "rekordbox analysis only",
        }
        .to_owned(),
    );
    out.push(
        match o.missing_analysis {
            MissingAnalysis::Skip => "Unanalysed tracks left out",
            MissingAnalysis::Generate => "Unanalysed tracks approximated",
            MissingAnalysis::ExportWithout => "Unanalysed tracks without analysis",
        }
        .to_owned(),
    );
    if o.artwork {
        out.push("Artwork".to_owned());
    }
    out.push(
        if o.keep_stale {
            "Keeps unused files"
        } else {
            "Cleans up unused files"
        }
        .to_owned(),
    );
    if o.dividers {
        out.push("Dividers".to_owned());
    }
    if o.active_loops {
        out.push("First loop starts by itself".to_owned());
    }
    if o.onelibrary {
        out.push("OneLibrary".to_owned());
    }
    out
}
