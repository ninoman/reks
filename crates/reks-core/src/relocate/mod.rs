//! Relocate missing files and change a library folder (Phase 1 task 15, D36).
//!
//! The folder scan already follows moves *inside* the library folders. This
//! covers the rest: find missing tracks in any folder ([`plan`] → dry run →
//! [`apply`], like exports), and rewrite the path prefix of a whole library
//! folder ([`plan_remap`], [`remap`]). Only `track.path` (and `missing`,
//! `file_mtime`) change: cues, grids, tags, playlists, history and
//! `export_manifest` hang off the track id and stay.

mod apply;
mod matching;
mod plan;
mod remap;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::scan::TrackId;
use crate::settings::LibraryRoot;

pub use apply::apply;
pub use plan::plan;
pub use remap::{plan_remap, remap};

/// Which missing tracks to look for.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RelocateScope {
    /// The missing ones among these tracks; others are ignored.
    Tracks {
        ids: Vec<TrackId>,
    },
    AllMissing,
}

/// Progress while planning or applying (sent to the UI over a Tauri channel).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RelocateEvent {
    /// Walking the folder; `found` audio files so far.
    Walking { found: usize },
    /// Hashing files whose size matches a missing track.
    Hashing { done: usize, total: usize },
    /// Reading names, tags and durations for possible matches.
    Reading { done: usize, total: usize },
    /// Checking and relinking.
    Applying { done: usize, total: usize },
}

/// A missing track, as the dry run lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackRef {
    pub id: TrackId,
    /// Where the library last had the file.
    pub path: String,
    /// `Artist – Title`, or the file name for an untagged track.
    pub label: String,
}

/// A file in the chosen folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundFile {
    pub path: String,
    pub size: i64,
    pub duration_ms: Option<i64>,
    /// Same byte size as the missing file (a retagged copy differs).
    pub same_size: bool,
}

/// One sure match: the file's content hash equals the track's, and no other
/// file in the folder shares it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SureMatch {
    pub track: TrackRef,
    pub file: FoundFile,
}

/// Why a track needs the user's pick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ReviewBasis {
    /// Several files in the folder have the track's exact content.
    SameContent,
    /// No file has its content; these share its name or tags and length.
    /// Never relinked unless the user picks one.
    Possible,
}

/// A track with one or more candidates that the user picks from or skips.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewItem {
    pub track: TrackRef,
    pub basis: ReviewBasis,
    /// Best first.
    pub candidates: Vec<FoundFile>,
}

/// A file or folder that could not be read; the plan carries on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RelocateProblem {
    pub path: String,
    pub message: String,
}

/// The dry run: nothing is written until [`apply`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelocatePlan {
    /// The folder searched, in library path form.
    pub folder: String,
    /// The folder is inside a library folder, so the scan keeps it current.
    pub inside_root: bool,
    /// Missing tracks looked for.
    pub looked_for: usize,
    /// Audio files in the folder that are not in the library yet.
    pub files_seen: usize,
    pub sure: Vec<SureMatch>,
    pub review: Vec<ReviewItem>,
    pub not_found: Vec<TrackRef>,
    pub problems: Vec<RelocateProblem>,
    /// Stopped early; there is nothing to apply.
    pub cancelled: bool,
}

/// The user's decisions on a dry run.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelocateChoices {
    /// Track id → the candidate path picked from its [`ReviewItem`]. Review
    /// tracks not listed are skipped.
    pub picks: BTreeMap<TrackId, String>,
    /// Add the folder as a library folder (scan it afterwards).
    pub add_root: bool,
}

/// A track that was not relinked, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Skipped {
    pub track: TrackRef,
    pub reason: String,
}

/// What [`apply`] did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelocateReport {
    pub relinked: usize,
    pub skipped: Vec<Skipped>,
    /// Set when `add_root` added the folder (or it already was one).
    pub root: Option<LibraryRoot>,
    /// Cancelled before anything was written; the library is unchanged.
    pub cancelled: bool,
}

/// One track's old and new path in a folder change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RemapExample {
    pub from: String,
    pub to: String,
}

/// The dry run of a library folder change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemapPlan {
    pub root_id: i64,
    pub old_path: String,
    pub new_path: String,
    /// Tracks whose path changes.
    pub tracks: usize,
    /// Library folders inside this one that move with it.
    pub nested_roots: usize,
    /// First few paths, old and new.
    pub examples: Vec<RemapExample>,
    /// Files looked for at the new place, spread over the tracks.
    pub sampled: usize,
    /// Of those, how many exist there.
    pub sample_found: usize,
    /// New paths another track already has; the change is refused then.
    pub collisions: usize,
}

/// What [`remap`] did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemapReport {
    pub tracks: usize,
    pub roots: usize,
}
