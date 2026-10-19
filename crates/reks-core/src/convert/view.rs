//! The converter's dry run and report as the UI and the CLI show them
//! (phase 2.6 tasks 6–7): every string final, tracks named `Artist – Title`.

use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;

use super::{
    AudioFormat, ConvertPlan, ConvertReport, Destination, LibraryMode, OriginalFate, Originals,
    SkipReason, Warning,
};
use crate::db::Library;
use crate::scan::TrackId;

/// What the dry run shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanView {
    /// At least one file converts.
    pub ready: bool,
    /// One sentence on the outcome.
    pub status: String,
    /// Settings and totals, one line each.
    pub facts: Vec<Fact>,
    pub items: Vec<ItemView>,
    pub skipped: Vec<SkipView>,
    pub warnings: Vec<String>,
}

/// A label and its value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Fact {
    pub label: String,
    pub value: String,
}

/// One file the plan converts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemView {
    pub track_id: TrackId,
    pub label: String,
    pub source: String,
    pub dest: String,
    /// `FLAC · 24-bit · 96 kHz · stereo`.
    pub from: String,
    pub to: String,
    /// `41.2 MB → 23.0 MB`.
    pub size_text: String,
}

/// One selected track that is not converted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkipView {
    pub track_id: TrackId,
    pub label: String,
    pub reason: String,
}

/// How a run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Outcome {
    Success,
    Problems,
    Cancelled,
}

/// What the report shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportView {
    pub outcome: Outcome,
    pub title: String,
    pub status: String,
    pub facts: Vec<Fact>,
    pub converted: Vec<ConvertedView>,
    pub failed: Vec<FailedView>,
    pub skipped: Vec<SkipView>,
}

/// One finished conversion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConvertedView {
    pub track_id: TrackId,
    pub new_track_id: TrackId,
    pub label: String,
    pub dest: String,
    /// Where the original is now: `Kept: <path>`, `Moved to the Trash`, …
    pub original: String,
}

/// One failed conversion; its track is unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FailedView {
    pub track_id: TrackId,
    pub label: String,
    pub message: String,
}

/// The dry run of `plan`, with track names from `lib`.
pub fn plan_view(lib: &Library, plan: &ConvertPlan) -> PlanView {
    let labels = labels(
        lib,
        plan.items
            .iter()
            .map(|i| i.track_id)
            .chain(plan.skipped.iter().map(|s| s.0)),
    );
    let label = |id| label_of(&labels, id);
    let n = plan.items.len();
    let status = if n == 0 {
        "Nothing to convert. Nothing has been written.".to_owned()
    } else {
        format!(
            "Ready to convert {}. Nothing has been written yet.",
            count(n, "file", "files")
        )
    };
    let o = &plan.options;
    let mut facts = vec![
        fact(
            "Files",
            format!("{n} to convert, {} skipped", plan.skipped.len()),
        ),
        fact(
            "Size",
            format!(
                "{} → about {}",
                human_bytes(plan.bytes_before),
                human_bytes(plan.bytes_after_est)
            ),
        ),
        fact(
            "Destination",
            match &o.destination {
                Destination::NextToOriginal => "Next to each original".to_owned(),
                Destination::Folder(dir) => dir.display().to_string(),
            },
        ),
        fact(
            "Library",
            match o.library {
                LibraryMode::Replace => {
                    "Replace: each track points at its new file and keeps its cues, grid, \
                     playlists and history"
                }
                LibraryMode::AddAsNew => "Add as new tracks (metadata, cues and grid copied)",
            },
        ),
        fact(
            "Originals",
            match o.originals {
                Originals::Keep => "Kept where they are",
                Originals::Trash => "Moved to the Trash after a verified conversion",
            },
        ),
    ];
    if o.cap_rate_48k {
        facts.push(fact(
            "Sample rate",
            "Above 48 kHz goes to 44.1 or 48 kHz (Older players)",
        ));
    }
    PlanView {
        ready: n > 0,
        status,
        facts,
        items: plan
            .items
            .iter()
            .map(|i| ItemView {
                track_id: i.track_id,
                label: label(i.track_id),
                source: i.source.clone(),
                dest: i.dest.clone(),
                from: format_text(&i.from),
                to: format_text(&i.to),
                size_text: format!(
                    "{} → {}{}",
                    human_bytes(i.bytes_before),
                    if i.to.kind == "flac" { "about " } else { "" },
                    human_bytes(i.bytes_after_est)
                ),
            })
            .collect(),
        skipped: skip_views(&plan.skipped, &label),
        warnings: plan
            .warnings
            .iter()
            .map(|w| warning_text(w, &label))
            .collect(),
    }
}

/// The report of a run of `plan`, with track names from `lib`.
pub fn report_view(lib: &Library, plan: &ConvertPlan, report: &ConvertReport) -> ReportView {
    let labels = labels(
        lib,
        report
            .converted
            .iter()
            .map(|c| c.new_track_id)
            .chain(report.failed.iter().map(|f| f.track_id))
            .chain(report.skipped.iter().map(|s| s.0)),
    );
    let label = |id| label_of(&labels, id);
    let total = plan.items.len();
    let done = report.converted.len();
    let failed = report.failed.len();
    let (outcome, title) = if report.cancelled {
        (Outcome::Cancelled, "Conversion cancelled")
    } else if failed > 0 {
        (Outcome::Problems, "Conversion finished with problems")
    } else {
        (Outcome::Success, "Conversion finished")
    };
    let mut status = format!("Converted {done} of {}.", count(total, "file", "files"));
    if failed > 0 {
        status.push_str(&format!(
            " {} failed; {} unchanged.",
            failed,
            if failed == 1 {
                "its track is"
            } else {
                "their tracks are"
            }
        ));
    }
    if report.cancelled {
        status.push_str(" Files not started are unchanged.");
    }
    ReportView {
        outcome,
        title: title.to_owned(),
        status,
        facts: vec![fact(
            "Size",
            format!(
                "{} → {}",
                human_bytes(report.bytes_before),
                human_bytes(report.bytes_after)
            ),
        )],
        converted: report
            .converted
            .iter()
            .map(|c| ConvertedView {
                track_id: c.track_id,
                new_track_id: c.new_track_id,
                label: label(c.new_track_id),
                dest: c.dest.clone(),
                original: match &c.original {
                    OriginalFate::Kept => format!("Original kept: {}", c.source),
                    OriginalFate::Trashed => format!("Original moved to the Trash: {}", c.source),
                    OriginalFate::TrashFailed { message } => {
                        format!(
                            "Original kept ({}): couldn't move it to the Trash: {message}",
                            c.source
                        )
                    }
                },
            })
            .collect(),
        failed: report
            .failed
            .iter()
            .map(|f| FailedView {
                track_id: f.track_id,
                label: label(f.track_id),
                message: f.message.clone(),
            })
            .collect(),
        skipped: skip_views(&report.skipped, &label),
    }
}

fn skip_views(
    skipped: &[(TrackId, SkipReason)],
    label: &impl Fn(TrackId) -> String,
) -> Vec<SkipView> {
    skipped
        .iter()
        .map(|(id, reason)| SkipView {
            track_id: *id,
            label: label(*id),
            reason: skip_text(reason),
        })
        .collect()
}

/// Why a track is skipped, as a phrase.
pub fn skip_text(reason: &SkipReason) -> String {
    match reason {
        SkipReason::NotInLibrary => "not in the library".to_owned(),
        SkipReason::Duplicate => "selected more than once".to_owned(),
        SkipReason::FileMissing => "file not found".to_owned(),
        SkipReason::Unreadable { message } => format!("can't be read: {message}"),
        SkipReason::Lossy => "MP3 or AAC: only lossless files convert".to_owned(),
        SkipReason::TooManyChannels { channels } => {
            format!("{channels} channels: only mono and stereo convert")
        }
        SkipReason::AlreadyTarget => "already in that format".to_owned(),
    }
}

fn warning_text(w: &Warning, label: &impl Fn(TrackId) -> String) -> String {
    match w {
        Warning::Renamed {
            track_id,
            wanted,
            dest,
        } => format!(
            "{}: {} is taken, writing {}",
            label(*track_id),
            file_name(wanted),
            file_name(dest)
        ),
        Warning::ReducedDepth { track_id, from } => format!(
            "{}: {from}-bit to 16-bit, with dither; some precision is lost",
            label(*track_id)
        ),
        Warning::RekordboxAnalysisDropped { track_id } => format!(
            "{}: imported from rekordbox; until reusing its analysis after a conversion is \
             checked on players (CONV1, UNVERIFIED), exports use generated analysis for it",
            label(*track_id)
        ),
    }
}

/// `FLAC · 24-bit · 96 kHz · stereo`.
pub fn format_text(f: &AudioFormat) -> String {
    let channels = match f.channels {
        1 => "mono".to_owned(),
        2 => "stereo".to_owned(),
        n => format!("{n} channels"),
    };
    format!(
        "{} · {}-bit · {} · {channels}",
        f.kind.to_ascii_uppercase(),
        f.bit_depth,
        rate_text(f.sample_rate)
    )
}

/// `44.1 kHz`, `48 kHz`.
fn rate_text(hz: u32) -> String {
    if hz.is_multiple_of(1000) {
        format!("{} kHz", hz / 1000)
    } else {
        format!("{:.1} kHz", f64::from(hz) / 1000.0)
    }
}

/// `412.3 MB` (SI units, as the export dry run).
pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if bytes < 1000 {
        return format!("{bytes} B");
    }
    let mut v = bytes as f64 / 1000.0;
    let mut unit = 0;
    while v >= 1000.0 && unit + 1 < UNITS.len() {
        v /= 1000.0;
        unit += 1;
    }
    format!("{v:.1} {}", UNITS[unit])
}

fn fact(label: &str, value: impl Into<String>) -> Fact {
    Fact {
        label: label.to_owned(),
        value: value.into(),
    }
}

fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map_or_else(|| path.to_owned(), |n| n.to_string_lossy().into_owned())
}

/// `Artist – Title`, the title alone, or the file name.
fn labels(lib: &Library, ids: impl Iterator<Item = TrackId>) -> HashMap<TrackId, String> {
    let mut out = HashMap::new();
    for id in ids {
        if out.contains_key(&id) {
            continue;
        }
        let Ok(d) = lib.track(id) else { continue };
        let row = &d.track.row;
        let label = match (row.artist.as_deref(), row.title.as_deref()) {
            (Some(a), Some(t)) if !a.is_empty() && !t.is_empty() => format!("{a} – {t}"),
            (_, Some(t)) if !t.is_empty() => t.to_owned(),
            _ => file_name(&d.track.path),
        };
        out.insert(id, label);
    }
    out
}

fn label_of(labels: &HashMap<TrackId, String>, id: TrackId) -> String {
    labels
        .get(&id)
        .cloned()
        .unwrap_or_else(|| format!("track {id}"))
}
