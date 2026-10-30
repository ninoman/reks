//! Read-back validator (Phase 2 task 8): after the writer ran, read the stick
//! with a parser independent of the writer (reks's own [`crate::pdb`], not
//! baken's code) and check it against the [`ExportPlan`], so a broken stick is caught on the
//! computer instead of on a CDJ.
//!
//! [`validate`] checks:
//!
//! - `PIONEER/rekordbox/export.pdb` parses and holds all 20 tables (types
//!   0–19, each once, R§1), and every table's page chain reads.
//! - One track row per planned track, matched on `file_path` = `dst_rel`; no
//!   extra or duplicate rows.
//! - Every `file_path` exists on the stick.
//! - Every `analyze_path` exists, sits in the `USBANLZ` folder players derive
//!   from `file_path` ([`anlz_dir`], R§4), parses, and its `PPTH` equals
//!   `file_path`; the same for the `.EXT` beside it when there is one.
//! - For tracks with analysis ([`AnlzSource`] not `None`): `PQTZ` holds beats
//!   where the plan says `has_grid`, and the cue count of the `.DAT` (`PCOB`)
//!   and, when present, the `.EXT` (`PCO2`) equals the plan's `cue_count`.
//! - For tracks whose ANLZ reks built ([`AnlzSource::Reks`]): the `.DAT`,
//!   `.EXT` and `.2EX` as baken left them pass [`reks_anlz::validate_set`]
//!   (section lists and order, lengths, beats, cues, waveform sizes).
//! - The playlist tree (names, folder/playlist kind, sibling order) and each
//!   playlist's entries in order match the plan.
//!
//! Every problem is a [`Finding`]; the stick is good when there are none.
//! `Err` is kept for I/O failures that stop the validation itself. The
//! report also carries the read-back track rows ([`StickTrack`]), from which
//! [`ValidationReport::manifest_rows`] builds the export manifest (task 13).
//!
//! ANLZ files are read with [`reks_anlz::read`] (section framing per the
//! crate-digger spec), not rekordcrate: its 0.3.0 ANLZ parser rejects point
//! cues (type 1), so it fails on every real cue list (D8).
//!
//! `export.pdb` is read with [`crate::pdb`], bounds-checked at the offsets of
//! the crate-digger spec (`analysis.adoc`, "Track Rows"), page chains bounded
//! by the file size and cycle-checked, so a damaged file can neither hang nor
//! abort the validator (review F03). rekordcrate is not used here: its 0.3.0
//! parser panics on some damaged strings (D8, amended 2026-10-30); it stays
//! the independent parser of the writer's round-trip tests.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use reks_anlz::read::Summary;
use reks_core::{ManifestItemKind, ManifestRow, PlaylistId, TrackId};
use serde::{Deserialize, Serialize};

use crate::pdb::rows::{
    self as pdb_rows, RowError, STR_ANALYZE_PATH, STR_FILE_PATH, STR_TITLE, TRACK_ID_AT,
};
use crate::pdb::{u32_at, Pdb, Table};
use crate::plan::{anlz_dir, AnlzSource, ExportPlan, PlannedPlaylist};

/// `export.pdb`, relative to the stick root.
pub const PDB_PATH: &str = "/PIONEER/rekordbox/export.pdb";
/// Tables a Device Library `export.pdb` holds: types 0 to 19 (R§1).
pub const TABLE_COUNT: u32 = 20;

#[derive(Debug, thiserror::Error)]
pub enum ValidateError {
    #[error("cannot read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// Result of reading a stick back.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationReport {
    /// Track rows of `export.pdb`, in table order.
    pub tracks: Vec<StickTrack>,
    /// Playlist tree rows of `export.pdb`, in table order.
    pub playlists: Vec<StickPlaylist>,
    /// Empty when the stick matches the plan.
    pub findings: Vec<Finding>,
}

impl ValidationReport {
    /// True when nothing was found.
    pub fn is_valid(&self) -> bool {
        self.findings.is_empty()
    }

    /// Findings about one planned track.
    pub fn track_findings(&self, track_id: TrackId) -> impl Iterator<Item = &Finding> {
        self.findings
            .iter()
            .filter(move |f| f.track_id() == Some(track_id))
    }

    /// Export manifest rows (task 13): one per track row on the stick, with
    /// the planned track it belongs to (`None` for a row the plan doesn't
    /// know). Feed to `Library::replace_manifest` after a valid export.
    pub fn manifest_rows(&self) -> Vec<ManifestRow> {
        let mut seen = HashSet::new();
        self.tracks
            .iter()
            .filter(|t| seen.insert(t.file_path.as_str()))
            .map(|t| ManifestRow {
                stick_path: t.file_path.clone(),
                item_kind: if t.divider {
                    ManifestItemKind::Divider
                } else {
                    ManifestItemKind::Track
                },
                track_id: if t.divider { None } else { t.track_id },
            })
            .collect()
    }
}

/// A track row as read back from `export.pdb`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StickTrack {
    /// Row id in `export.pdb` (not stable across exports).
    pub pdb_id: u32,
    pub title: String,
    /// `/Contents/...`
    pub file_path: String,
    /// `/PIONEER/USBANLZ/.../ANLZ0000.DAT`, empty without analysis.
    pub analyze_path: String,
    /// The planned track with this `file_path` (a synthetic id for a divider).
    pub track_id: Option<TrackId>,
    /// The planned track is a playlist divider (D22).
    #[serde(default)]
    pub divider: bool,
}

/// A playlist tree row as read back from `export.pdb`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StickPlaylist {
    pub pdb_id: u32,
    /// 0 at the root.
    pub parent_id: u32,
    pub sort_order: u32,
    pub name: String,
    pub is_folder: bool,
    /// Track row ids in entry order.
    pub entries: Vec<u32>,
}

/// One problem found on the stick.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Finding {
    /// No `export.pdb`: players show no Device Library.
    PdbMissing,
    /// `export.pdb` header or a page does not parse.
    PdbUnreadable {
        message: String,
    },
    /// A table type 0–19 is absent from the header.
    TableMissing {
        page_type: u32,
    },
    /// A table type appears more than once, or is outside 0–19.
    TableUnexpected {
        page_type: u32,
    },
    TrackCount {
        expected: usize,
        found: usize,
    },
    /// A planned track has no row in `export.pdb`.
    TrackNotInPdb {
        track_id: TrackId,
        path: String,
    },
    /// A track row the plan does not know.
    UnexpectedTrack {
        pdb_id: u32,
        path: String,
    },
    /// More than one track row points at the same file.
    DuplicateTrackRow {
        path: String,
    },
    /// A track row whose strings do not decode.
    TrackRowUnreadable {
        message: String,
    },
    /// `file_path` does not exist on the stick.
    AudioMissing {
        track_id: TrackId,
        path: String,
    },
    /// The plan has analysis for the track, the row no `analyze_path`.
    AnlzPathEmpty {
        track_id: TrackId,
    },
    /// `analyze_path` (or the `.EXT` beside it) is not on the stick.
    AnlzMissing {
        track_id: TrackId,
        path: String,
    },
    /// `analyze_path` is not in the folder players derive from `file_path`,
    /// so players won't find the analysis.
    AnlzDirMismatch {
        track_id: TrackId,
        expected: String,
        found: String,
    },
    AnlzUnreadable {
        track_id: TrackId,
        path: String,
        message: String,
    },
    /// The ANLZ file's `PPTH` is absent or names another file.
    PpthMismatch {
        track_id: TrackId,
        path: String,
        expected: String,
        found: Option<String>,
    },
    /// The plan has a beatgrid, `PQTZ` holds no beats.
    GridEmpty {
        track_id: TrackId,
        path: String,
    },
    CueCount {
        track_id: TrackId,
        path: String,
        expected: u8,
        found: u32,
    },
    /// An ANLZ set reks built fails [`reks_anlz::validate_set`] on the stick.
    AnlzInvalid {
        track_id: TrackId,
        path: String,
        issues: Vec<String>,
    },
    /// A planned playlist or folder (`Folder/Name` path) is absent.
    PlaylistMissing {
        path: String,
    },
    /// A playlist or folder the plan does not have.
    UnexpectedPlaylist {
        path: String,
    },
    /// Folder on one side, playlist on the other.
    PlaylistKind {
        path: String,
        expected_folder: bool,
    },
    /// Siblings under `parent` (empty at the root) are in another order.
    PlaylistOrder {
        parent: String,
        expected: Vec<String>,
        found: Vec<String>,
    },
    /// A playlist's entries (stick paths) differ from the plan.
    PlaylistEntries {
        path: String,
        expected_len: usize,
        found_len: usize,
        /// First position where they differ.
        first_difference: usize,
    },
}

impl Finding {
    /// The planned track this finding is about, if any.
    pub fn track_id(&self) -> Option<TrackId> {
        match self {
            Finding::TrackNotInPdb { track_id, .. }
            | Finding::AudioMissing { track_id, .. }
            | Finding::AnlzPathEmpty { track_id }
            | Finding::AnlzMissing { track_id, .. }
            | Finding::AnlzDirMismatch { track_id, .. }
            | Finding::AnlzUnreadable { track_id, .. }
            | Finding::PpthMismatch { track_id, .. }
            | Finding::GridEmpty { track_id, .. }
            | Finding::CueCount { track_id, .. }
            | Finding::AnlzInvalid { track_id, .. } => Some(*track_id),
            _ => None,
        }
    }
}

/// One line per finding, for the CLI and the export UI's report.
impl std::fmt::Display for Finding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PdbMissing => f.write_str("no export.pdb on the stick"),
            Self::PdbUnreadable { message } => write!(f, "export.pdb does not parse: {message}"),
            Self::TableMissing { page_type } => {
                write!(f, "export.pdb lacks table type {page_type}")
            }
            Self::TableUnexpected { page_type } => {
                write!(
                    f,
                    "export.pdb has an unexpected or repeated table type {page_type}"
                )
            }
            Self::TrackCount { expected, found } => {
                write!(f, "{found} track rows, the plan has {expected} tracks")
            }
            Self::TrackNotInPdb { track_id, path } => {
                write!(f, "track {track_id} ({path}) is not in export.pdb")
            }
            Self::UnexpectedTrack { pdb_id, path } => {
                write!(f, "track row {pdb_id} ({path}) is not in the plan")
            }
            Self::DuplicateTrackRow { path } => write!(f, "several track rows point at {path}"),
            Self::TrackRowUnreadable { message } => {
                write!(f, "a track row does not decode: {message}")
            }
            Self::AudioMissing { track_id, path } => {
                write!(f, "track {track_id}: audio {path} is not on the stick")
            }
            Self::AnlzPathEmpty { track_id } => {
                write!(f, "track {track_id}: the row has no analysis path")
            }
            Self::AnlzMissing { track_id, path } => {
                write!(f, "track {track_id}: analysis {path} is not on the stick")
            }
            Self::AnlzDirMismatch {
                track_id,
                expected,
                found,
            } => write!(
                f,
                "track {track_id}: analysis is in {found}, players look in {expected}"
            ),
            Self::AnlzUnreadable {
                track_id,
                path,
                message,
            } => write!(f, "track {track_id}: {path} does not parse: {message}"),
            Self::PpthMismatch {
                track_id,
                path,
                expected,
                found,
            } => write!(
                f,
                "track {track_id}: {path} names {}, not {expected}",
                found.as_deref().unwrap_or("no file")
            ),
            Self::GridEmpty { track_id, path } => {
                write!(f, "track {track_id}: {path} has no beatgrid")
            }
            Self::CueCount {
                track_id,
                path,
                expected,
                found,
            } => write!(
                f,
                "track {track_id}: {path} has {found} cues, the plan {expected}"
            ),
            Self::AnlzInvalid {
                track_id,
                path,
                issues,
            } => write!(
                f,
                "track {track_id}: {path} fails the ANLZ check: {}",
                issues.join("; ")
            ),
            Self::PlaylistMissing { path } => write!(f, "playlist {path} is not on the stick"),
            Self::UnexpectedPlaylist { path } => {
                write!(f, "playlist {path} on the stick is not in the plan")
            }
            Self::PlaylistKind {
                path,
                expected_folder,
            } => write!(
                f,
                "{path} should be a {}",
                if *expected_folder {
                    "folder"
                } else {
                    "playlist"
                }
            ),
            Self::PlaylistOrder {
                parent,
                expected,
                found,
            } => write!(
                f,
                "playlists under {:?} are in the order {}, the plan has {}",
                parent,
                found.join(", "),
                expected.join(", ")
            ),
            Self::PlaylistEntries {
                path,
                expected_len,
                found_len,
                first_difference,
            } => write!(
                f,
                "playlist {path} has {found_len} entries, the plan {expected_len}; \
                 first difference at position {}",
                first_difference + 1
            ),
        }
    }
}

/// Reads the stick at `device_root` back and checks it against `plan`.
pub fn validate(device_root: &Path, plan: &ExportPlan) -> Result<ValidationReport, ValidateError> {
    let mut report = match read_back(device_root)? {
        ReadBack::Read(report) => report,
        ReadBack::Missing => return Ok(finding_only(Finding::PdbMissing)),
        ReadBack::Unreadable(message) => {
            return Ok(finding_only(Finding::PdbUnreadable { message }))
        }
    };
    let planned: HashMap<&str, (TrackId, bool)> = plan
        .tracks
        .iter()
        .map(|t| (t.dst_rel.as_str(), (t.track_id, t.is_divider())))
        .collect();
    for t in &mut report.tracks {
        let found = planned.get(t.file_path.as_str()).copied();
        t.track_id = found.map(|(id, _)| id);
        t.divider = found.is_some_and(|(_, d)| d);
    }
    check_tracks(device_root, plan, &report.tracks, &mut report.findings)?;
    check_playlists(
        plan,
        &report.tracks,
        &report.playlists,
        &mut report.findings,
    );
    Ok(report)
}

/// Reads the stick's `export.pdb` without a plan: its track rows (with no
/// `track_id`), its playlist tree, and findings only for damage
/// ([`Finding::PdbMissing`] when there is no database). Read-only; stick
/// profiles use it to see what a mounted stick holds.
pub fn read(device_root: &Path) -> Result<ValidationReport, ValidateError> {
    Ok(match read_back(device_root)? {
        ReadBack::Read(report) => report,
        ReadBack::Missing => finding_only(Finding::PdbMissing),
        ReadBack::Unreadable(message) => finding_only(Finding::PdbUnreadable { message }),
    })
}

enum ReadBack {
    Missing,
    /// The header itself is unusable.
    Unreadable(String),
    /// Rows, tree and entries; damage to tables or rows in `findings`.
    Read(ValidationReport),
}

fn finding_only(f: Finding) -> ValidationReport {
    ValidationReport {
        findings: vec![f],
        ..ValidationReport::default()
    }
}

fn read_back(device_root: &Path) -> Result<ReadBack, ValidateError> {
    let Some(pdb_path) = locate(device_root, PDB_PATH).map_err(io_err(device_root))? else {
        return Ok(ReadBack::Missing);
    };
    let bytes = fs::read(&pdb_path).map_err(io_err(&pdb_path))?;
    let PdbContents {
        table_types,
        tracks,
        playlists,
        entries,
        errors,
    } = match read_pdb(&bytes) {
        Ok(pdb) => pdb,
        Err(message) => return Ok(ReadBack::Unreadable(message)),
    };
    let mut report = ValidationReport {
        tracks,
        playlists,
        findings: errors,
    };
    check_tables(&table_types, &mut report.findings);
    for (playlist_id, list) in entries {
        if let Some(p) = report
            .playlists
            .iter_mut()
            .find(|p| p.pdb_id == playlist_id)
        {
            p.entries = list;
        }
    }
    Ok(ReadBack::Read(report))
}

fn check_tables(types: &[u32], findings: &mut Vec<Finding>) {
    let mut seen = HashSet::new();
    for &t in types {
        if t >= TABLE_COUNT || !seen.insert(t) {
            findings.push(Finding::TableUnexpected { page_type: t });
        }
    }
    for t in 0..TABLE_COUNT {
        if !seen.contains(&t) {
            findings.push(Finding::TableMissing { page_type: t });
        }
    }
}

fn check_tracks(
    root: &Path,
    plan: &ExportPlan,
    rows: &[StickTrack],
    findings: &mut Vec<Finding>,
) -> Result<(), ValidateError> {
    if rows.len() != plan.tracks.len() {
        findings.push(Finding::TrackCount {
            expected: plan.tracks.len(),
            found: rows.len(),
        });
    }
    let mut by_path: HashMap<&str, &StickTrack> = HashMap::new();
    for row in rows {
        if by_path.insert(row.file_path.as_str(), row).is_some() {
            findings.push(Finding::DuplicateTrackRow {
                path: row.file_path.clone(),
            });
        }
        if row.track_id.is_none() {
            findings.push(Finding::UnexpectedTrack {
                pdb_id: row.pdb_id,
                path: row.file_path.clone(),
            });
        }
    }

    for t in &plan.tracks {
        let track_id = t.track_id;
        let Some(row) = by_path.get(t.dst_rel.as_str()) else {
            findings.push(Finding::TrackNotInPdb {
                track_id,
                path: t.dst_rel.clone(),
            });
            continue;
        };
        if locate(root, &row.file_path)
            .map_err(io_err(root))?
            .is_none()
        {
            findings.push(Finding::AudioMissing {
                track_id,
                path: row.file_path.clone(),
            });
        }

        let has_analysis = !matches!(t.anlz_source, AnlzSource::None);
        if row.analyze_path.is_empty() {
            if has_analysis {
                findings.push(Finding::AnlzPathEmpty { track_id });
            }
            continue;
        }
        let expected_dir = anlz_dir(&row.file_path);
        let found_dir = row.analyze_path.rsplit_once('/').map_or("", |(dir, _)| dir);
        if !found_dir.eq_ignore_ascii_case(&expected_dir) {
            findings.push(Finding::AnlzDirMismatch {
                track_id,
                expected: expected_dir,
                found: found_dir.to_string(),
            });
        }

        let Some(dat) = read_anlz(root, track_id, &row.analyze_path, findings)? else {
            continue;
        };
        check_ppth(&dat, track_id, &row.analyze_path, &row.file_path, findings);
        if !has_analysis {
            continue;
        }
        if t.has_grid && dat.beats == 0 {
            findings.push(Finding::GridEmpty {
                track_id,
                path: row.analyze_path.clone(),
            });
        }
        let found = dat.cues.unwrap_or(0);
        if found != u32::from(t.cue_count) {
            findings.push(Finding::CueCount {
                track_id,
                path: row.analyze_path.clone(),
                expected: t.cue_count,
                found,
            });
        }

        if t.anlz_source.reks_built() {
            check_reks_set(root, track_id, &row.analyze_path, &row.file_path, findings)?;
        }

        // `.EXT` carries the colour waveform and the extended cues NXS2 and
        // newer players read; check it when the writer put one there.
        let ext_rel = with_extension(&row.analyze_path, "EXT");
        if locate(root, &ext_rel).map_err(io_err(root))?.is_some() {
            if let Some(ext) = read_anlz(root, track_id, &ext_rel, findings)? {
                check_ppth(&ext, track_id, &ext_rel, &row.file_path, findings);
                match ext.extended_cues {
                    Some(found) if found != u32::from(t.cue_count) => {
                        findings.push(Finding::CueCount {
                            track_id,
                            path: ext_rel,
                            expected: t.cue_count,
                            found,
                        });
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

/// [`reks_anlz::validate_set`] on a set reks built, as it sits on the stick.
/// A missing `.DAT`/`.EXT` is already a finding elsewhere (or the issue
/// list says so); the `.2EX` is checked when present.
fn check_reks_set(
    root: &Path,
    track_id: TrackId,
    dat_rel: &str,
    file_path: &str,
    findings: &mut Vec<Finding>,
) -> Result<(), ValidateError> {
    let read = |rel: &str| -> Result<Option<Vec<u8>>, ValidateError> {
        match locate(root, rel).map_err(io_err(root))? {
            Some(path) => fs::read(&path).map(Some).map_err(io_err(&path)),
            None => Ok(None),
        }
    };
    let (Some(dat), Some(ext)) = (read(dat_rel)?, read(&with_extension(dat_rel, "EXT"))?) else {
        findings.push(Finding::AnlzMissing {
            track_id,
            path: with_extension(dat_rel, "EXT"),
        });
        return Ok(());
    };
    let ex2 = read(&with_extension(dat_rel, "2EX"))?;
    let issues = reks_anlz::validate_set(
        &dat,
        &ext,
        ex2.as_deref(),
        reks_anlz::Expect {
            path: Some(file_path),
            duration_ms: None,
        },
    );
    if !issues.is_empty() {
        findings.push(Finding::AnlzInvalid {
            track_id,
            path: dat_rel.to_string(),
            issues: issues.iter().map(ToString::to_string).collect(),
        });
    }
    Ok(())
}

fn check_ppth(
    anlz: &Summary,
    track_id: TrackId,
    anlz_rel: &str,
    file_path: &str,
    findings: &mut Vec<Finding>,
) {
    if anlz.path.as_deref() != Some(file_path) {
        findings.push(Finding::PpthMismatch {
            track_id,
            path: anlz_rel.to_string(),
            expected: file_path.to_string(),
            found: anlz.path.clone(),
        });
    }
}

/// Reads and summarises the ANLZ file at stick path `rel`; a missing or
/// unparsable file becomes a finding and `None`.
fn read_anlz(
    root: &Path,
    track_id: TrackId,
    rel: &str,
    findings: &mut Vec<Finding>,
) -> Result<Option<Summary>, ValidateError> {
    let Some(path) = locate(root, rel).map_err(io_err(root))? else {
        findings.push(Finding::AnlzMissing {
            track_id,
            path: rel.to_string(),
        });
        return Ok(None);
    };
    let bytes = fs::read(&path).map_err(io_err(&path))?;
    match reks_anlz::read::summary(&bytes) {
        Ok(summary) => Ok(Some(summary)),
        Err(e) => {
            findings.push(Finding::AnlzUnreadable {
                track_id,
                path: rel.to_string(),
                message: e.to_string(),
            });
            Ok(None)
        }
    }
}

fn with_extension(rel: &str, ext: &str) -> String {
    match rel.rsplit_once('.') {
        Some((stem, _)) if !stem.ends_with('/') => format!("{stem}.{ext}"),
        _ => format!("{rel}.{ext}"),
    }
}

/// A node of a playlist tree, either side, for comparison.
struct Node<'a> {
    name: &'a str,
    is_folder: bool,
    children: Vec<usize>,
    /// Stick paths of the entries, in order.
    entries: Vec<String>,
}

fn check_playlists(
    plan: &ExportPlan,
    rows: &[StickTrack],
    playlists: &[StickPlaylist],
    findings: &mut Vec<Finding>,
) {
    let (expected, expected_roots) = plan_tree(plan);
    let (found, found_roots) = stick_tree(rows, playlists);
    compare_level(
        "",
        &expected,
        &expected_roots,
        &found,
        &found_roots,
        findings,
    );
}

fn plan_tree(plan: &ExportPlan) -> (Vec<Node<'_>>, Vec<usize>) {
    let paths: HashMap<TrackId, &str> = plan
        .tracks
        .iter()
        .map(|t| (t.track_id, t.dst_rel.as_str()))
        .collect();
    let index: HashMap<PlaylistId, usize> = plan
        .playlists
        .iter()
        .enumerate()
        .map(|(i, p)| (p.id, i))
        .collect();
    let mut nodes: Vec<Node<'_>> = plan
        .playlists
        .iter()
        .map(|p: &PlannedPlaylist| Node {
            name: &p.name,
            is_folder: p.is_folder,
            children: Vec::new(),
            entries: p
                .track_ids
                .iter()
                .map(|id| {
                    paths
                        .get(id)
                        .map_or_else(|| format!("<track {id}>"), |s| (*s).to_string())
                })
                .collect(),
        })
        .collect();
    let mut roots = Vec::new();
    for (i, p) in plan.playlists.iter().enumerate() {
        match p.parent_id.and_then(|id| index.get(&id)) {
            Some(&parent) if parent != i => nodes[parent].children.push(i),
            _ => roots.push(i),
        }
    }
    (nodes, roots)
}

fn stick_tree<'a>(
    rows: &[StickTrack],
    playlists: &'a [StickPlaylist],
) -> (Vec<Node<'a>>, Vec<usize>) {
    let paths: HashMap<u32, &str> = rows
        .iter()
        .map(|t| (t.pdb_id, t.file_path.as_str()))
        .collect();
    let index: HashMap<u32, usize> = playlists
        .iter()
        .enumerate()
        .map(|(i, p)| (p.pdb_id, i))
        .collect();
    let mut nodes: Vec<Node<'a>> = playlists
        .iter()
        .map(|p| Node {
            name: &p.name,
            is_folder: p.is_folder,
            children: Vec::new(),
            entries: p
                .entries
                .iter()
                .map(|id| {
                    paths
                        .get(id)
                        .map_or_else(|| format!("<pdb track {id}>"), |s| (*s).to_string())
                })
                .collect(),
        })
        .collect();
    let mut roots = Vec::new();
    // Siblings in sort order; table order breaks ties.
    let mut order: Vec<usize> = (0..playlists.len()).collect();
    order.sort_by_key(|&i| (playlists[i].sort_order, i));
    for i in order {
        let p = &playlists[i];
        match index.get(&p.parent_id) {
            Some(&parent) if p.parent_id != 0 && parent != i => nodes[parent].children.push(i),
            _ => roots.push(i),
        }
    }
    (nodes, roots)
}

/// Compares one level of the trees (children of `parent`) and recurses.
fn compare_level(
    parent: &str,
    expected: &[Node<'_>],
    exp_children: &[usize],
    found: &[Node<'_>],
    found_children: &[usize],
    findings: &mut Vec<Finding>,
) {
    let join = |name: &str| {
        if parent.is_empty() {
            name.to_string()
        } else {
            format!("{parent}/{name}")
        }
    };
    let mut used = vec![false; found_children.len()];
    let mut matched: Vec<(usize, usize)> = Vec::new(); // (expected node, position in found_children)
    for &e in exp_children {
        let en = &expected[e];
        let hit = found_children
            .iter()
            .enumerate()
            .find(|(k, &f)| !used[*k] && found[f].name == en.name);
        let Some((k, _)) = hit else {
            findings.push(Finding::PlaylistMissing {
                path: join(en.name),
            });
            continue;
        };
        used[k] = true;
        matched.push((e, k));
    }
    for (k, &f) in found_children.iter().enumerate() {
        if !used[k] {
            findings.push(Finding::UnexpectedPlaylist {
                path: join(found[f].name),
            });
        }
    }
    if matched.windows(2).any(|w| w[0].1 > w[1].1) {
        let mut by_found = matched.clone();
        by_found.sort_by_key(|&(_, k)| k);
        findings.push(Finding::PlaylistOrder {
            parent: parent.to_string(),
            expected: matched
                .iter()
                .map(|&(e, _)| expected[e].name.to_string())
                .collect(),
            found: by_found
                .iter()
                .map(|&(e, _)| expected[e].name.to_string())
                .collect(),
        });
    }
    for (e, k) in matched {
        let (en, fnode) = (&expected[e], &found[found_children[k]]);
        let path = join(en.name);
        if en.is_folder != fnode.is_folder {
            findings.push(Finding::PlaylistKind {
                path,
                expected_folder: en.is_folder,
            });
            continue;
        }
        if en.is_folder {
            compare_level(
                &path,
                expected,
                &en.children,
                found,
                &fnode.children,
                findings,
            );
        } else if en.entries != fnode.entries {
            let first_difference = en
                .entries
                .iter()
                .zip(&fnode.entries)
                .position(|(a, b)| a != b)
                .unwrap_or_else(|| en.entries.len().min(fnode.entries.len()));
            findings.push(Finding::PlaylistEntries {
                path,
                expected_len: en.entries.len(),
                found_len: fnode.entries.len(),
                first_difference,
            });
        }
    }
}

/// What [`read_pdb`] gets out of `export.pdb`.
struct PdbContents {
    table_types: Vec<u32>,
    tracks: Vec<StickTrack>,
    playlists: Vec<StickPlaylist>,
    /// Playlist id → track ids in entry order.
    entries: Vec<(u32, Vec<u32>)>,
    /// Damage found in individual tables or rows; the rest still reads.
    errors: Vec<Finding>,
}

/// Parses `export.pdb` ([`crate::pdb`]). `Err` only when the header itself
/// is unusable; a broken page chain or a row that does not decode is a
/// finding and the rest still reads.
fn read_pdb(bytes: &[u8]) -> Result<PdbContents, String> {
    let file = Pdb::parse(bytes).map_err(|e| e.to_string())?;
    let mut out = PdbContents {
        table_types: file.tables().iter().map(|t| t.table.code()).collect(),
        tracks: Vec::new(),
        playlists: Vec::new(),
        entries: Vec::new(),
        errors: Vec::new(),
    };
    let mut entries: Vec<(u32, u32, u32)> = Vec::new(); // (playlist, index, track)
    for table in file.tables() {
        let rows = match table.table {
            Table::Tracks | Table::PlaylistTree | Table::PlaylistEntries => file.rows(table),
            // Not decoded, but its page chain must read.
            _ => file.pages(table).map(|_| Vec::new()),
        };
        let rows = match rows {
            Ok(rows) => rows,
            Err(e) => {
                out.errors.push(Finding::PdbUnreadable {
                    message: e.to_string(),
                });
                continue;
            }
        };
        for r in rows {
            let decoded = match table.table {
                Table::Tracks => track_row(r).map(|t| out.tracks.push(t)),
                Table::PlaylistTree => playlist_row(r).map(|p| out.playlists.push(p)),
                _ => pdb_rows::playlist_entry(r).map(|e| entries.push(e)),
            };
            if let Err(e) = decoded {
                out.errors.push(Finding::TrackRowUnreadable {
                    message: e.to_string(),
                });
            }
        }
    }
    entries.sort_by_key(|&(playlist, index, _)| (playlist, index));
    for (playlist, _, track) in entries {
        match out.entries.last_mut() {
            Some((p, list)) if *p == playlist => list.push(track),
            _ => out.entries.push((playlist, vec![track])),
        }
    }
    Ok(out)
}

fn track_row(r: &[u8]) -> Result<StickTrack, RowError> {
    Ok(StickTrack {
        pdb_id: u32_at(r, TRACK_ID_AT).ok_or(RowError::Short("track"))?,
        title: pdb_rows::track_string(r, STR_TITLE)?,
        file_path: pdb_rows::track_string(r, STR_FILE_PATH)?,
        analyze_path: pdb_rows::track_string(r, STR_ANALYZE_PATH)?,
        track_id: None,
        divider: false,
    })
}

fn playlist_row(r: &[u8]) -> Result<StickPlaylist, RowError> {
    let p = pdb_rows::playlist_node(r)?;
    Ok(StickPlaylist {
        pdb_id: p.id,
        parent_id: p.parent_id,
        sort_order: p.sort_order,
        name: p.name,
        is_folder: p.is_folder,
        entries: Vec::new(),
    })
}

/// The file at stick path `rel` (`/`-separated), each component matched
/// ignoring ASCII case when the exact name is absent (FAT is
/// case-insensitive, a temp dir on Linux is not).
pub fn locate(root: &Path, rel: &str) -> io::Result<Option<PathBuf>> {
    let mut path = root.to_path_buf();
    for part in rel.split('/').filter(|p| !p.is_empty()) {
        let exact = path.join(part);
        if exact.try_exists()? {
            path = exact;
            continue;
        }
        let entries = match fs::read_dir(&path) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        let mut hit = None;
        for entry in entries {
            let entry = entry?;
            if entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(part)
            {
                hit = Some(entry.path());
                break;
            }
        }
        match hit {
            Some(p) => path = p,
            None => return Ok(None),
        }
    }
    Ok(path.is_file().then_some(path))
}

fn io_err(path: &Path) -> impl FnOnce(io::Error) -> ValidateError + '_ {
    move |source| ValidateError::Io {
        path: path.to_path_buf(),
        source,
    }
}

#[cfg(test)]
mod tests;
