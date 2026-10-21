//! Import a parsed rekordbox collection into the library.
//!
//! Rules (details in `docs/phases/phase-1.md`, task 7 notes):
//! - A track is matched to an existing row by path, then by the XML
//!   `Location` stored at a previous import (`rb_location`), then by
//!   (artist, title, size) when exactly one row fits. Unmatched tracks get a
//!   new row; a file that can't be found is still stored, with `missing = 1`.
//! - XML values win for the fields the XML carries; an absent or empty
//!   attribute never clears a value. File tags only fill what is still empty.
//!   Exception (D23): `PlayCount` and `LastPlayed` set the frozen play
//!   baseline of tracks this import creates and are ignored for existing
//!   ones; the cached play count and last played are then recomputed.
//! - Grids and cues with `source = 'rekordbox'` are replaced; rows from any
//!   other source are left alone. Playlists are matched by name within their
//!   folder; tracks a user added in reks stay after the XML's entries.
//! - Files are read before the database is locked; every track, grid, cue
//!   and playlist write happens in one transaction.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use rusqlite::types::Value;
use rusqlite::{named_params, params, Transaction};
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

use super::colour::colour_name;
use super::{location_to_path, Collection, NodeKind, Tempo, XmlNode, XmlTrack};
use crate::db::Library;
use crate::error::Result;
use crate::fileinfo::{self, kind_from_extension, library_path, probe_cols, FileInfo};
use crate::keys::Key;
use crate::playlist::{self, Entry};
use crate::scan::{CancelToken, TrackId};
use crate::tag::{self, TagId};
use crate::timefmt;

/// Grid and cue rows written by this import carry this `source`.
const SOURCE: &str = "rekordbox";

/// The category that tags read from Comments are created in.
pub const IMPORTED_TAG_CATEGORY: &str = "Imported";

/// Replace a path prefix in every `Location` (the music folder moved since
/// rekordbox exported the XML). Compared component-wise.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PrefixRemap {
    pub from: PathBuf,
    /// Made absolute against the current directory if relative.
    pub to: PathBuf,
}

#[derive(Debug, Clone, Default)]
pub struct ImportOptions {
    /// Tried in order; the first matching prefix wins.
    pub remap: Vec<PrefixRemap>,
    /// Checked between tracks while files are read. Once the database
    /// stage starts (one transaction) the import runs to the end.
    pub cancel: Option<CancelToken>,
}

/// Which pass a [`ImportEvent::Progress`] belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ImportStage {
    /// Matching tracks and reading their files.
    Files,
    /// Writing tracks, grids and cues.
    Database,
}

/// Progress reported while importing (sent to the UI over a Tauri channel).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ImportEvent {
    Progress {
        stage: ImportStage,
        done: usize,
        total: usize,
    },
    Unmatched(Unmatched),
}

/// An XML track that has no file in the library, with the reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Unmatched {
    /// `TrackID` in the XML.
    pub track_id: u32,
    pub name: Option<String>,
    pub artist: Option<String>,
    pub location: Option<String>,
    pub reason: UnmatchedReason,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum UnmatchedReason {
    /// No `Location`; nothing stored.
    NoLocation,
    /// Not a local file (e.g. a streaming-service entry); nothing stored.
    NotAFile,
    /// A file URL we can't turn into a path; nothing stored.
    InvalidLocation { message: String },
    /// Stored with `missing = 1` (cues, grid and playlists kept).
    FileNotFound { path: String },
    /// The file exists but couldn't be read as audio; stored with the XML's
    /// technical fields.
    Unreadable { path: String, message: String },
    /// Resolves to the same library row as an earlier XML track; this
    /// entry's metadata, grid and cues are not imported.
    DuplicateOf { track_id: u32 },
}

/// A value the import ignored or replaced with a default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportWarning {
    /// XML `TrackID`, if the warning is about a track.
    pub track_id: Option<u32>,
    pub message: String,
}

/// What an import did. Every XML track is counted in exactly one of
/// `matched_*`, `added` or (via `unmatched`) not stored at all.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    /// Tracks in the XML.
    pub total: usize,
    /// Existing rows matched by path.
    pub matched_by_path: usize,
    /// Existing rows matched by the `Location` of an earlier import.
    pub matched_by_location: usize,
    /// Existing rows matched by (artist, title, size).
    pub matched_by_fallback: usize,
    /// New rows.
    pub added: usize,
    /// Rows (new or matched) whose file wasn't found; also in `unmatched`.
    pub missing: usize,
    /// Every track without a readable file, with the reason.
    pub unmatched: Vec<Unmatched>,
    pub warnings: Vec<ImportWarning>,
    pub beatgrid_rows: usize,
    pub cue_rows: usize,
    pub playlists_created: usize,
    pub playlists_matched: usize,
    pub playlist_entries: usize,
    /// Playlist entries whose track has no library row.
    pub playlist_entries_skipped: usize,
    /// My Tags created from `/* … */` in Comments (D29), in the
    /// [`IMPORTED_TAG_CATEGORY`] category.
    pub tags_created: usize,
    /// Track–tag links added from Comments.
    pub tags_assigned: usize,
    /// Deepest folder containing every file not found, if that is deeper than
    /// a filesystem root: the `from` to suggest for a [`PrefixRemap`] when the
    /// music folder has moved.
    pub missing_root: Option<String>,
    pub elapsed_ms: u64,
    /// Stopped before the database stage: nothing was written, and only the
    /// counts up to that point are filled in.
    pub cancelled: bool,
}

/// What an import of a collection would find, before anything is written.
/// Computed from the XML, the library rows and one `stat` per track; no file
/// is read.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    /// Tracks in the XML.
    pub total: usize,
    /// Playlists and folders below `ROOT`.
    pub playlists: usize,
    pub folders: usize,
    pub playlist_entries: usize,
    /// Tracks with at least one `TEMPO` / `POSITION_MARK`.
    pub with_grid: usize,
    pub with_cues: usize,
    /// Tracks whose file (after the remap) exists.
    pub files_found: usize,
    pub files_not_found: usize,
    /// Tracks that won't be stored at all: no `Location`, not a local file,
    /// or a `Location` that can't be decoded.
    pub no_location: usize,
    pub not_a_file: usize,
    pub invalid_location: usize,
    /// Tracks that resolve to a library row by path or by the `Location` of an
    /// earlier import. An estimate: the import also matches by (artist, title,
    /// size) and reports a second XML track on one row as a duplicate.
    pub in_library: usize,
    /// As [`ImportReport::missing_root`], over the files not found.
    pub missing_root: Option<String>,
    /// Up to [`PREVIEW_SAMPLE`] paths of files not found, in XML order.
    pub not_found_sample: Vec<String>,
}

/// How many not-found paths [`preview`] lists.
pub const PREVIEW_SAMPLE: usize = 20;

/// Counts what [`import`] would do with `c`, without writing anything.
pub fn preview(c: &Collection, lib: &Library, opts: &ImportOptions) -> Result<ImportPreview> {
    let remap = prepare_remap(&opts.remap);
    let ex = existing_rows(lib)?;
    let mut p = ImportPreview {
        total: c.tracks.len(),
        ..ImportPreview::default()
    };
    count_nodes(&c.playlists, &mut p, true);
    let mut not_found: Vec<String> = Vec::new();
    for t in &c.tracks {
        p.with_grid += usize::from(!t.tempos.is_empty());
        p.with_cues += usize::from(!t.position_marks.is_empty());
        let path = match resolve(&remap, t) {
            Ok(path) => path,
            Err(UnmatchedReason::NoLocation) => {
                p.no_location += 1;
                continue;
            }
            Err(UnmatchedReason::NotAFile) => {
                p.not_a_file += 1;
                continue;
            }
            Err(_) => {
                p.invalid_location += 1;
                continue;
            }
        };
        let known = ex.by_path.contains_key(&path)
            || t.location
                .as_ref()
                .is_some_and(|l| ex.by_location.contains_key(l));
        p.in_library += usize::from(known);
        if Path::new(&path).exists() {
            p.files_found += 1;
        } else {
            p.files_not_found += 1;
            not_found.push(path);
        }
    }
    p.missing_root = missing_root(not_found.iter().map(String::as_str));
    not_found.truncate(PREVIEW_SAMPLE);
    p.not_found_sample = not_found;
    Ok(p)
}

fn count_nodes(node: &XmlNode, p: &mut ImportPreview, root: bool) {
    match &node.kind {
        NodeKind::Folder(children) => {
            p.folders += usize::from(!root);
            for child in children {
                count_nodes(child, p, false);
            }
        }
        NodeKind::Playlist { keys, .. } => {
            p.playlists += 1;
            p.playlist_entries += keys.len();
        }
    }
}

/// Imports `c` into `lib`. Safe to repeat: a second import of the same XML
/// updates the rows the first one wrote.
pub fn import(
    c: &Collection,
    lib: &Library,
    opts: ImportOptions,
    progress: impl Fn(&ImportEvent),
) -> Result<ImportReport> {
    let started = Instant::now();
    let mut imp = Import {
        lib,
        remap: prepare_remap(&opts.remap),
        cancel: opts.cancel,
        progress: &progress,
        report: ImportReport {
            total: c.tracks.len(),
            ..ImportReport::default()
        },
    };
    let existing = existing_rows(lib)?;
    let plans = imp.plan_tracks(&c.tracks, &existing);
    if imp.cancelled() {
        imp.report.cancelled = true;
        imp.report.elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        tracing::info!(read = plans.len(), "rekordbox XML import cancelled");
        return Ok(imp.report);
    }

    let mut conn = lib.conn()?;
    let tx = conn.transaction()?;
    let mut row_of_xml: HashMap<u32, TrackId> = HashMap::new();
    let mut row_of_location: HashMap<&str, TrackId> = HashMap::new();
    let total = plans.len();
    for (i, (t, plan)) in c.tracks.iter().zip(&plans).enumerate() {
        if let Some(id) = imp.write_track(&tx, t, plan, &existing)? {
            row_of_xml.entry(t.track_id).or_insert(id);
            if let Some(loc) = t.location.as_deref() {
                row_of_location.entry(loc).or_insert(id);
            }
        }
        if (i + 1) % 250 == 0 || i + 1 == total {
            imp.emit(&ImportEvent::Progress {
                stage: ImportStage::Database,
                done: i + 1,
                total,
            });
        }
    }
    let rows = TrackRows {
        by_xml: row_of_xml,
        by_location: row_of_location,
    };
    imp.write_playlists(&tx, &c.playlists, &rows)?;
    imp.write_comment_tags(&tx, &c.tracks, &rows.by_xml)?;
    // The baseline is frozen (D23): PlayCount and LastPlayed set it only
    // for tracks this import created; every written track's cache follows.
    let mut written: Vec<TrackId> = rows.by_xml.values().copied().collect();
    written.sort_unstable();
    written.dedup();
    crate::history::recompute(&tx, &crate::history::TrackSet::Some(written))?;
    tx.commit()?;
    drop(conn);

    imp.report.missing_root =
        missing_root(imp.report.unmatched.iter().filter_map(|u| match &u.reason {
            UnmatchedReason::FileNotFound { path } => Some(path.as_str()),
            _ => None,
        }));
    imp.report.elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    tracing::info!(
        total = imp.report.total,
        added = imp.report.added,
        matched = imp.report.matched_by_path
            + imp.report.matched_by_location
            + imp.report.matched_by_fallback,
        unmatched = imp.report.unmatched.len(),
        elapsed_ms = imp.report.elapsed_ms,
        "rekordbox XML imported"
    );
    Ok(imp.report)
}

/// Deepest common folder of the paths of files not found; `None` if there
/// are none or they only share a root (`/`, `C:\\`), which is too broad to remap.
fn missing_root<'a>(paths: impl IntoIterator<Item = &'a str>) -> Option<String> {
    let mut paths = paths.into_iter().map(Path::new);
    let mut common: PathBuf = paths.next()?.parent()?.to_path_buf();
    for p in paths {
        while !p.starts_with(&common) {
            common = common.parent()?.to_path_buf();
        }
    }
    let named = common
        .components()
        .filter(|c| matches!(c, std::path::Component::Normal(_)))
        .count();
    (named > 0).then(|| common.to_string_lossy().into_owned())
}

struct Import<'a> {
    lib: &'a Library,
    remap: Vec<(String, PathBuf)>,
    cancel: Option<CancelToken>,
    progress: &'a dyn Fn(&ImportEvent),
    report: ImportReport,
}

/// A library row as it was before the import.
struct Row {
    id: TrackId,
    path: String,
    file_size: i64,
    file_mtime: String,
}

/// Existing rows and the lookups the matcher uses.
#[derive(Default)]
struct Existing {
    rows: Vec<Row>,
    by_path: HashMap<String, usize>,
    by_location: HashMap<String, Vec<usize>>,
    by_fallback: HashMap<(String, String, i64), Vec<usize>>,
}

/// What to do with one XML track.
enum Plan {
    /// Nothing stored (reason already reported).
    Skip,
    /// Write to an existing row (index into `Existing::rows`) or a new one.
    Write {
        target: Target,
        /// Path the row ends up with.
        path: String,
        file: FileState,
    },
}

enum Target {
    Existing(usize),
    New,
}

enum FileState {
    /// File not found (or not stat-able): `missing = 1`.
    Missing,
    /// Same size and mtime as the row already records: keep its fields.
    Unchanged,
    /// Read now.
    Read(Box<FileInfo>),
    /// Exists but isn't readable audio.
    Unreadable { size: i64, mtime: String },
}

struct TrackRows<'a> {
    by_xml: HashMap<u32, TrackId>,
    by_location: HashMap<&'a str, TrackId>,
}

impl Import<'_> {
    fn cancelled(&self) -> bool {
        self.cancel.as_ref().is_some_and(CancelToken::is_cancelled)
    }

    fn emit(&self, event: &ImportEvent) {
        (self.progress)(event);
    }

    fn warn(&mut self, track_id: Option<u32>, message: String) {
        tracing::debug!(?track_id, %message, "import warning");
        self.report
            .warnings
            .push(ImportWarning { track_id, message });
    }

    fn unmatched(&mut self, t: &XmlTrack, reason: UnmatchedReason) {
        let u = Unmatched {
            track_id: t.track_id,
            name: t.name.clone(),
            artist: t.artist.clone(),
            location: t.location.clone(),
            reason,
        };
        self.emit(&ImportEvent::Unmatched(u.clone()));
        self.report.unmatched.push(u);
    }

    /// Resolves, matches and reads the file of every XML track. No DB lock
    /// is held (artwork storage takes it briefly).
    fn plan_tracks(&mut self, tracks: &[XmlTrack], ex: &Existing) -> Vec<Plan> {
        // Row (existing index or new path) → XML TrackID that claimed it.
        let mut claimed_rows: HashMap<usize, u32> = HashMap::new();
        let mut claimed_paths: HashMap<String, u32> = HashMap::new();
        let total = tracks.len();
        let mut plans = Vec::with_capacity(total);
        for (i, t) in tracks.iter().enumerate() {
            if self.cancelled() {
                break;
            }
            let plan = self.plan_track(t, ex, &mut claimed_rows, &mut claimed_paths);
            plans.push(plan);
            if (i + 1) % 50 == 0 || i + 1 == total {
                self.emit(&ImportEvent::Progress {
                    stage: ImportStage::Files,
                    done: i + 1,
                    total,
                });
            }
        }
        plans
    }

    fn plan_track(
        &mut self,
        t: &XmlTrack,
        ex: &Existing,
        claimed_rows: &mut HashMap<usize, u32>,
        claimed_paths: &mut HashMap<String, u32>,
    ) -> Plan {
        let xml_path = match resolve(&self.remap, t) {
            Ok(p) => p,
            Err(reason) => {
                self.unmatched(t, reason);
                return Plan::Skip;
            }
        };
        let xml_stat = fileinfo::stat(&xml_path);

        // Match: path, then earlier Location, then (artist, title, size).
        let free = |i: &usize| !claimed_rows.contains_key(i);
        let (row, how) = if let Some(&i) = ex.by_path.get(&xml_path) {
            (Some(i), Match::Path)
        } else if let Some(i) = t
            .location
            .as_ref()
            .and_then(|l| ex.by_location.get(l))
            .and_then(|v| v.iter().copied().find(free))
        {
            (Some(i), Match::Location)
        } else {
            let key = t.size.and_then(|s| {
                fallback_key(
                    t.artist.as_deref(),
                    t.name.as_deref(),
                    i64::try_from(s).ok()?,
                )
            });
            let fits: Vec<usize> = key
                .and_then(|k| ex.by_fallback.get(&k))
                .map(|v| v.iter().copied().filter(free).collect())
                .unwrap_or_default();
            match fits.as_slice() {
                [i] => (Some(*i), Match::Fallback),
                _ => (None, Match::Path),
            }
        };

        if let Some(i) = row {
            if let Some(&first) = claimed_rows.get(&i) {
                self.unmatched(t, UnmatchedReason::DuplicateOf { track_id: first });
                return Plan::Skip;
            }
        } else if let Some(&first) = claimed_paths.get(&xml_path) {
            self.unmatched(t, UnmatchedReason::DuplicateOf { track_id: first });
            return Plan::Skip;
        }

        let (target, path, stat) = match row {
            Some(i) => {
                let r = &ex.rows[i];
                // Move the row to the XML path only when its own file is gone
                // and the XML's file is there (a relink); never take a path
                // another row or XML track already has.
                let relink = r.path != xml_path
                    && xml_stat.is_ok()
                    && !ex.by_path.contains_key(&xml_path)
                    && !claimed_paths.contains_key(&xml_path)
                    && !Path::new(&r.path).exists();
                claimed_rows.insert(i, t.track_id);
                match how {
                    Match::Path => self.report.matched_by_path += 1,
                    Match::Location => self.report.matched_by_location += 1,
                    Match::Fallback => self.report.matched_by_fallback += 1,
                }
                if relink {
                    claimed_paths.insert(xml_path.clone(), t.track_id);
                    (Target::Existing(i), xml_path, xml_stat)
                } else if r.path == xml_path {
                    (Target::Existing(i), xml_path, xml_stat)
                } else {
                    let stat = fileinfo::stat(&r.path);
                    (Target::Existing(i), r.path.clone(), stat)
                }
            }
            None => {
                claimed_paths.insert(xml_path.clone(), t.track_id);
                self.report.added += 1;
                (Target::New, xml_path, xml_stat)
            }
        };

        let file = match stat {
            Err(e) => {
                self.report.missing += 1;
                let reason = if e.kind() == std::io::ErrorKind::NotFound {
                    UnmatchedReason::FileNotFound { path: path.clone() }
                } else {
                    UnmatchedReason::Unreadable {
                        path: path.clone(),
                        message: e.to_string(),
                    }
                };
                self.unmatched(t, reason);
                FileState::Missing
            }
            Ok((size, mtime)) => {
                let unchanged = match &target {
                    Target::Existing(i) => {
                        let r = &ex.rows[*i];
                        r.path == path && r.file_size == size && r.file_mtime == mtime
                    }
                    Target::New => false,
                };
                if unchanged {
                    FileState::Unchanged
                } else {
                    match fileinfo::read_file(self.lib, &path, size, mtime.clone()) {
                        Ok(info) => FileState::Read(Box::new(info)),
                        Err(message) => {
                            tracing::warn!(path, %message, "import: unreadable file");
                            self.unmatched(
                                t,
                                UnmatchedReason::Unreadable {
                                    path: path.clone(),
                                    message,
                                },
                            );
                            FileState::Unreadable { size, mtime }
                        }
                    }
                }
            }
        };
        Plan::Write { target, path, file }
    }

    /// Writes one track with its grid and cues; returns its row id.
    fn write_track(
        &mut self,
        tx: &Transaction<'_>,
        t: &XmlTrack,
        plan: &Plan,
        ex: &Existing,
    ) -> Result<Option<TrackId>> {
        let Plan::Write { target, path, file } = plan else {
            return Ok(None);
        };
        let now = timefmt::now_utc();
        let date_added = self.date(t.track_id, "DateAdded", t.date_added.as_deref());
        let id = match target {
            Target::Existing(i) => ex.rows[*i].id,
            Target::New => {
                let (size, mtime) = match file {
                    FileState::Read(info) => (info.size, info.mtime.clone()),
                    FileState::Unreadable { size, mtime } => (*size, mtime.clone()),
                    FileState::Missing | FileState::Unchanged => (
                        t.size.and_then(|s| i64::try_from(s).ok()).unwrap_or(0),
                        String::new(),
                    ),
                };
                let kind = match file {
                    FileState::Read(info) => info.probe.kind.as_str(),
                    _ => kind_from_extension(Path::new(path))
                        .or_else(|| t.kind.as_deref().and_then(kind_from_xml))
                        .unwrap_or("unknown"),
                };
                tx.prepare_cached(
                    "INSERT INTO track (path, file_size, file_mtime, kind, date_added,
                                        created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
                )?
                .execute(params![
                    path,
                    size,
                    mtime,
                    kind,
                    date_added.clone().unwrap_or_else(timefmt::today_utc),
                    now,
                ])?;
                tx.last_insert_rowid()
            }
        };
        let created = matches!(target, Target::New);
        self.update_track(tx, id, t, path, file, date_added, created, &now)?;
        self.write_grid(tx, id, t)?;
        self.write_cues(tx, id, t)?;
        Ok(Some(id))
    }

    #[allow(clippy::too_many_arguments)]
    fn update_track(
        &mut self,
        tx: &Transaction<'_>,
        id: TrackId,
        t: &XmlTrack,
        path: &str,
        file: &FileState,
        date_added: Option<String>,
        created: bool,
        now: &str,
    ) -> Result<()> {
        let tid = Some(t.track_id);
        let key = match nz(&t.tonality) {
            None => None,
            Some(s) => match Key::parse(s) {
                Ok(k) => Some(k.canonical()),
                Err(_) => {
                    self.warn(
                        tid,
                        format!("unrecognised Tonality {s:?}; key not imported"),
                    );
                    None
                }
            },
        };
        let color = match t.colour {
            None => None,
            Some(rgb) => {
                let name = colour_name(rgb);
                if name.is_none() {
                    self.warn(tid, format!("Colour 0x{rgb:06X} is not a rekordbox colour"));
                }
                name
            }
        };
        let last_played = self.date(t.track_id, "LastPlayed", t.last_played.as_deref());
        let (info, missing, size, mtime) = match file {
            FileState::Read(info) => (Some(&**info), false, Some(info.size), Some(&info.mtime)),
            FileState::Unchanged => (None, false, None, None),
            FileState::Unreadable { size, mtime } => (None, false, Some(*size), Some(mtime)),
            FileState::Missing => (None, true, None, None),
        };
        let probe = info.map(|i| probe_cols(&i.probe));
        let tags = info.map(|i| &i.tags);
        let tag = |f: fn(&reks_audio::Tags) -> &Option<String>| tags.and_then(|t| f(t).as_deref());
        tx.prepare_cached(
            "UPDATE track SET
               path = :path, missing = :missing,
               file_size = coalesce(:size, file_size), file_mtime = coalesce(:mtime, file_mtime),
               content_hash = coalesce(:hash, content_hash), kind = coalesce(:kind, kind),
               duration_ms = coalesce(:duration_ms, duration_ms, :xml_duration_ms),
               sample_rate = coalesce(:sample_rate, sample_rate, :xml_sample_rate),
               bit_depth = coalesce(:bit_depth, bit_depth),
               bit_rate_kbps = coalesce(:bit_rate_kbps, bit_rate_kbps, :xml_bit_rate),
               channels = coalesce(:channels, channels),
               title = coalesce(:title, title, :tag_title),
               artist = coalesce(:artist, artist, :tag_artist),
               album = coalesce(:album, album, :tag_album),
               album_artist = coalesce(album_artist, :tag_album_artist),
               genre = coalesce(:genre, genre, :tag_genre),
               composer = coalesce(:composer, composer, :tag_composer),
               remixer = coalesce(:remixer, remixer, :tag_remixer),
               label = coalesce(:label, label, :tag_label),
               mix = coalesce(:mix, mix),
               grouping = coalesce(:grouping, grouping, :tag_grouping),
               track_no = coalesce(:track_no, track_no, :tag_track_no),
               disc_no = coalesce(:disc_no, disc_no, :tag_disc_no),
               year = coalesce(:year, year, :tag_year),
               comment = coalesce(:comment, comment, :tag_comment),
               rating = coalesce(:rating, rating),
               color = coalesce(:color, color),
               key_source = CASE WHEN :key IS NULL THEN key_source ELSE 'rekordbox' END,
               key = coalesce(:key, key),
               bpm_source = CASE WHEN :bpm IS NULL THEN bpm_source ELSE 'rekordbox' END,
               bpm_x100 = coalesce(:bpm, bpm_x100),
               baseline_play_count = CASE WHEN :created
                 THEN coalesce(:play_count, baseline_play_count) ELSE baseline_play_count END,
               date_added = coalesce(:date_added, date_added),
               baseline_last_played = CASE WHEN :created
                 THEN coalesce(:last_played, baseline_last_played) ELSE baseline_last_played END,
               artwork_id = coalesce(artwork_id, :artwork_id),
               rb_track_id = :rb_track_id, rb_location = coalesce(:rb_location, rb_location),
               updated_at = :now
             WHERE id = :id",
        )?
        .execute(named_params! {
            ":id": id, ":path": path, ":missing": missing, ":size": size, ":mtime": mtime,
            ":hash": info.map(|i| &i.hash), ":kind": probe.as_ref().map(|p| p.kind),
            ":duration_ms": probe.as_ref().map(|p| p.duration_ms),
            ":xml_duration_ms": t.total_time.map(|s| i64::from(s) * 1000),
            ":sample_rate": probe.as_ref().map(|p| p.sample_rate),
            ":xml_sample_rate": t.sample_rate.filter(|&r| r > 0),
            ":bit_depth": probe.as_ref().and_then(|p| p.bit_depth),
            ":bit_rate_kbps": probe.as_ref().and_then(|p| p.bit_rate_kbps),
            ":xml_bit_rate": t.bit_rate.filter(|&r| r > 0),
            ":channels": probe.as_ref().map(|p| p.channels),
            ":title": nz(&t.name), ":tag_title": tag(|t| &t.title),
            ":artist": nz(&t.artist), ":tag_artist": tag(|t| &t.artist),
            ":album": nz(&t.album), ":tag_album": tag(|t| &t.album),
            ":tag_album_artist": tag(|t| &t.album_artist),
            ":genre": nz(&t.genre), ":tag_genre": tag(|t| &t.genre),
            ":composer": nz(&t.composer), ":tag_composer": tag(|t| &t.composer),
            ":remixer": nz(&t.remixer), ":tag_remixer": tag(|t| &t.remixer),
            ":label": nz(&t.label), ":tag_label": tag(|t| &t.label),
            ":mix": nz(&t.mix),
            ":grouping": nz(&t.grouping), ":tag_grouping": tag(|t| &t.grouping),
            ":track_no": t.track_number.filter(|&n| n > 0),
            ":tag_track_no": tags.and_then(|t| t.track_no),
            ":disc_no": t.disc_number.filter(|&n| n > 0),
            ":tag_disc_no": tags.and_then(|t| t.disc_no),
            ":year": t.year.filter(|&y| y > 0), ":tag_year": tags.and_then(|t| t.year),
            ":comment": nz(&t.comments), ":tag_comment": tag(|t| &t.comment),
            ":rating": t.rating.map(rating_stars), ":color": color, ":key": key,
            ":bpm": t.average_bpm.and_then(bpm_x100),
            ":play_count": t.play_count, ":date_added": date_added, ":last_played": last_played,
            ":created": created,
            ":artwork_id": info.and_then(|i| i.artwork_id),
            ":rb_track_id": t.track_id, ":rb_location": t.location, ":now": now,
        })?;
        Ok(())
    }

    /// Tags named in each track's Comments: an existing tag of that name
    /// (any category) or a new one under [`IMPORTED_TAG_CATEGORY`]. The
    /// comment itself is stored as it is.
    fn write_comment_tags(
        &mut self,
        tx: &Transaction<'_>,
        tracks: &[XmlTrack],
        rows: &HashMap<u32, TrackId>,
    ) -> Result<()> {
        let now = timefmt::now_utc();
        let mut category: Option<TagId> = None;
        for t in tracks {
            let Some(&id) = rows.get(&t.track_id) else {
                continue;
            };
            let names = tag::parse_comment_tags(t.comments.as_deref().unwrap_or(""));
            let mut ids = Vec::with_capacity(names.len());
            for name in &names {
                let found = tag::find_by_name(tx, name)?;
                let tag_id = match found {
                    Some(g) => g,
                    None => {
                        let cat = match category {
                            Some(c) => c,
                            None => {
                                let c =
                                    tag::find_or_create_category(tx, IMPORTED_TAG_CATEGORY, &now)?;
                                category = Some(c);
                                c
                            }
                        };
                        self.report.tags_created += 1;
                        tag::create_tag(tx, cat, name, &now)?
                    }
                };
                ids.push(tag_id);
            }
            if !ids.is_empty() {
                self.report.tags_assigned +=
                    usize::try_from(tag::assign(tx, &[id], &ids)?).unwrap_or(0);
            }
        }
        Ok(())
    }

    fn write_grid(&mut self, tx: &Transaction<'_>, id: TrackId, t: &XmlTrack) -> Result<()> {
        let mut rows = Vec::with_capacity(t.tempos.len());
        for tempo in &t.tempos {
            let Some(row) = grid_row(tempo) else {
                self.warn(
                    Some(t.track_id),
                    format!(
                        "TEMPO Inizio={} Bpm={} is out of range; skipped",
                        tempo.inizio, tempo.bpm
                    ),
                );
                continue;
            };
            if row.meter_defaulted {
                self.warn(
                    Some(t.track_id),
                    format!("TEMPO Metro={:?} not understood; using 4/4", tempo.metro),
                );
            }
            rows.push(vec![
                // REAL column: compared with the stored value on re-import.
                Value::Real(row.position_ms as f64),
                Value::Integer(row.bpm_x100),
                Value::Integer(i64::from(row.meter_num)),
                Value::Integer(i64::from(row.meter_den)),
                Value::Integer(i64::from(row.beat_in_bar)),
            ]);
        }
        self.report.beatgrid_rows += rows.len();
        replace_rows(
            tx,
            "beatgrid",
            "position_ms, bpm_x100, meter_num, meter_den, beat_in_bar",
            id,
            &rows,
        )
    }

    fn write_cues(&mut self, tx: &Transaction<'_>, id: TrackId, t: &XmlTrack) -> Result<()> {
        let mut rows = Vec::with_capacity(t.position_marks.len());
        for m in &t.position_marks {
            let Some(slot) = cue_slot(m.num) else {
                self.warn(
                    Some(t.track_id),
                    format!("POSITION_MARK Num={} is not -1 or 0..7; skipped", m.num),
                );
                continue;
            };
            if m.kind == 4 && m.end.is_none() {
                self.warn(
                    Some(t.track_id),
                    format!("loop at {} s has no End", m.start),
                );
            }
            rows.push(vec![
                slot.map_or(Value::Null, |s| Value::Integer(i64::from(s))),
                Value::Integer(i64::from(m.kind)),
                Value::Real(m.start * 1000.0),
                m.end.map_or(Value::Null, |e| Value::Real(e * 1000.0)),
                nz(&m.name).map_or(Value::Null, |n| Value::Text(n.to_owned())),
                m.rgb
                    .map_or(Value::Null, |c| Value::Integer(i64::from(rgb_int(c)))),
            ]);
        }
        self.report.cue_rows += rows.len();
        replace_rows(
            tx,
            "cue",
            "slot, kind, start_ms, end_ms, name, color_rgb",
            id,
            &rows,
        )
    }

    /// Mirrors the XML playlist tree under `ROOT` into the library.
    fn write_playlists(
        &mut self,
        tx: &Transaction<'_>,
        root: &XmlNode,
        rows: &TrackRows<'_>,
    ) -> Result<()> {
        let NodeKind::Folder(children) = &root.kind else {
            self.warn(
                None,
                "PLAYLISTS root is not a folder; no playlists imported".into(),
            );
            return Ok(());
        };
        let now = timefmt::now_utc();
        self.write_nodes(tx, None, children, rows, &now)
    }

    fn write_nodes(
        &mut self,
        tx: &Transaction<'_>,
        parent: Option<i64>,
        nodes: &[XmlNode],
        rows: &TrackRows<'_>,
        now: &str,
    ) -> Result<()> {
        // Existing children of `parent`, in display order.
        let existing: Vec<(i64, bool, String)> = tx
            .prepare_cached(
                "SELECT id, is_folder, name FROM playlist WHERE parent_id IS ?1
                 ORDER BY sort_order, id",
            )?
            .query_map([parent], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let mut next_order: i64 = tx
            .prepare_cached(
                "SELECT coalesce(max(sort_order) + 1, 0) FROM playlist WHERE parent_id IS ?1",
            )?
            .query_row([parent], |r| r.get(0))?;
        let mut seen: HashMap<(bool, &str), usize> = HashMap::new();
        for node in nodes {
            let is_folder = matches!(node.kind, NodeKind::Folder(_));
            // The n-th XML sibling with this name and kind matches the n-th
            // existing one, so same-named playlists stay distinct.
            let n = seen.entry((is_folder, node.name.as_str())).or_insert(0);
            let found = existing
                .iter()
                .filter(|(_, f, name)| *f == is_folder && *name == node.name)
                .nth(*n)
                .map(|(id, _, _)| *id);
            *n += 1;
            let id = match found {
                Some(id) => {
                    self.report.playlists_matched += 1;
                    id
                }
                None => {
                    tx.prepare_cached(
                        "INSERT INTO playlist (parent_id, is_folder, name, sort_order,
                                               created_at, updated_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                    )?
                    .execute(params![parent, is_folder, node.name, next_order, now])?;
                    next_order += 1;
                    self.report.playlists_created += 1;
                    tx.last_insert_rowid()
                }
            };
            match &node.kind {
                NodeKind::Folder(children) => {
                    self.write_nodes(tx, Some(id), children, rows, now)?
                }
                NodeKind::Playlist { key_type, keys } => {
                    self.write_entries(tx, id, &node.name, *key_type, keys, rows, now)?;
                }
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn write_entries(
        &mut self,
        tx: &Transaction<'_>,
        playlist: i64,
        name: &str,
        key_type: u8,
        keys: &[String],
        rows: &TrackRows<'_>,
        now: &str,
    ) -> Result<()> {
        let mut tracks: Vec<TrackId> = Vec::with_capacity(keys.len());
        for key in keys {
            let id = match key_type {
                0 => key
                    .trim()
                    .parse::<u32>()
                    .ok()
                    .and_then(|k| rows.by_xml.get(&k)),
                1 => rows.by_location.get(key.as_str()),
                _ => None,
            };
            match id {
                Some(&id) => tracks.push(id),
                None => self.report.playlist_entries_skipped += 1,
            }
        }
        if key_type > 1 {
            self.warn(
                None,
                format!("playlist {name:?} has unknown KeyType {key_type}; entries skipped"),
            );
        }
        self.report.playlist_entries += tracks.len();

        let current = playlist::entries(tx, playlist)?;
        // XML entries first, then entries added in reks that the XML lacks.
        // Counted per track, so a second copy added in reks is kept.
        let mut from_xml: HashMap<TrackId, usize> = HashMap::new();
        for id in &tracks {
            *from_xml.entry(*id).or_default() += 1;
        }
        let mut wanted = tracks;
        let current_tracks: Vec<TrackId> = current.iter().filter_map(Entry::track).collect();
        for id in &current_tracks {
            match from_xml.get_mut(id) {
                Some(n) if *n > 0 => *n -= 1,
                _ => wanted.push(*id),
            }
        }
        if wanted == current_tracks {
            return Ok(());
        }
        // Dividers (reks-only, D22) keep their positions, clamped to the end.
        let mut new: Vec<Entry> = wanted.into_iter().map(Entry::Track).collect();
        for (pos, e) in current.iter().enumerate() {
            if let Entry::Divider(_) = e {
                new.insert(pos.min(new.len()), e.clone());
            }
        }
        playlist::write_entries(tx, playlist, &current, &new, now)
    }

    /// A `yyyy-mm-dd` date attribute, or `None` (with a warning if present
    /// but malformed).
    fn date(&mut self, track_id: u32, attr: &str, value: Option<&str>) -> Option<String> {
        let v = value.map(str::trim).filter(|v| !v.is_empty())?;
        if is_date(v) {
            Some(v.to_owned())
        } else {
            self.warn(
                Some(track_id),
                format!("{attr}={v:?} is not yyyy-mm-dd; ignored"),
            );
            None
        }
    }
}

/// Makes a track's `source = 'rekordbox'` rows in `table` equal `rows`
/// (values for `columns`, in order). Unchanged rows are left alone so a
/// re-import keeps their ids.
fn replace_rows(
    tx: &Transaction<'_>,
    table: &str,
    columns: &str,
    track: TrackId,
    rows: &[Vec<Value>],
) -> Result<()> {
    let current: Vec<Vec<Value>> = {
        let mut stmt = tx.prepare_cached(&format!(
            "SELECT {columns} FROM {table} WHERE track_id = ?1 AND source = ?2 ORDER BY id"
        ))?;
        let n = stmt.column_count();
        let rows = stmt.query_map(params![track, SOURCE], |r| {
            (0..n).map(|i| r.get::<_, Value>(i)).collect()
        })?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    if current == rows {
        return Ok(());
    }
    tx.prepare_cached(&format!(
        "DELETE FROM {table} WHERE track_id = ?1 AND source = ?2"
    ))?
    .execute(params![track, SOURCE])?;
    let width = rows.first().map_or(0, Vec::len);
    let marks = vec!["?"; width + 2].join(", ");
    let mut insert = tx.prepare_cached(&format!(
        "INSERT INTO {table} (track_id, source, {columns}) VALUES ({marks})"
    ))?;
    for row in rows {
        let values = [Value::Integer(track), Value::Text(SOURCE.into())]
            .into_iter()
            .chain(row.iter().cloned());
        insert.execute(rusqlite::params_from_iter(values))?;
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Match {
    Path,
    Location,
    Fallback,
}

/// Lookup key for the (artist, title, size) fallback: trimmed, case-folded.
fn fallback_key(
    artist: Option<&str>,
    title: Option<&str>,
    size: i64,
) -> Option<(String, String, i64)> {
    let norm = |s: Option<&str>| {
        let s = s?.trim();
        (!s.is_empty()).then(|| s.nfc().collect::<String>().to_lowercase())
    };
    Some((norm(artist)?, norm(title)?, size))
}

/// Library rows before the import, with the matcher's lookups.
fn existing_rows(lib: &Library) -> Result<Existing> {
    let conn = lib.conn()?;
    let mut stmt = conn.prepare(
        "SELECT id, path, file_size, file_mtime, rb_location, artist, title
         FROM track ORDER BY id",
    )?;
    let mut rows = stmt.query([])?;
    let mut ex = Existing::default();
    while let Some(r) = rows.next()? {
        let i = ex.rows.len();
        let row = Row {
            id: r.get(0)?,
            path: r.get(1)?,
            file_size: r.get(2)?,
            file_mtime: r.get(3)?,
        };
        let location: Option<String> = r.get(4)?;
        let artist: Option<String> = r.get(5)?;
        let title: Option<String> = r.get(6)?;
        ex.by_path.insert(row.path.clone(), i);
        if let Some(loc) = location {
            ex.by_location.entry(loc).or_default().push(i);
        }
        if let Some(key) = fallback_key(artist.as_deref(), title.as_deref(), row.file_size) {
            ex.by_fallback.entry(key).or_default().push(i);
        }
        ex.rows.push(row);
    }
    Ok(ex)
}

/// The library path an XML `Location` points at, after the remap.
fn resolve(
    remap: &[(String, PathBuf)],
    t: &XmlTrack,
) -> std::result::Result<String, UnmatchedReason> {
    let url = match t.location.as_deref() {
        Some(u) if !u.trim().is_empty() => u,
        _ => return Err(UnmatchedReason::NoLocation),
    };
    if !is_file_url(url) {
        return Err(UnmatchedReason::NotAFile);
    }
    let path = location_to_path(url).map_err(|e| UnmatchedReason::InvalidLocation {
        message: e.to_string(),
    })?;
    // Re-collecting the components gives native separators, so a
    // Windows Location (`C:/Music/x.mp3`) matches the scan's `C:\Music\x.mp3`.
    let path: PathBuf = apply_remap(remap, &path).components().collect();
    let Some(nfc) = path.to_str() else {
        return Err(UnmatchedReason::InvalidLocation {
            message: "remapped path is not valid UTF-8".into(),
        });
    };
    // location_to_path gives NFC; a file stored NFD on a filesystem that
    // doesn't normalise (Linux) is only found under its NFD name.
    if !Path::new(nfc).exists() {
        let nfd: String = nfc.nfd().collect();
        if nfd != nfc && Path::new(&nfd).exists() {
            return Ok(nfd);
        }
    }
    Ok(library_path(Path::new(nfc)).unwrap_or_else(|| nfc.to_owned()))
}

/// Remaps with `from` normalised to NFC (like decoded Locations) and `to`
/// made absolute.
fn prepare_remap(remap: &[PrefixRemap]) -> Vec<(String, PathBuf)> {
    remap
        .iter()
        .map(|r| {
            let from: String = r.from.to_string_lossy().nfc().collect();
            let to = std::path::absolute(&r.to).unwrap_or_else(|_| r.to.clone());
            (from, to)
        })
        .collect()
}

fn apply_remap(remap: &[(String, PathBuf)], path: &Path) -> PathBuf {
    for (from, to) in remap {
        if let Ok(rest) = path.strip_prefix(from) {
            return if rest.as_os_str().is_empty() {
                to.clone()
            } else {
                to.join(rest)
            };
        }
    }
    path.to_path_buf()
}

/// `file://localhost/…` or `file:///…`; anything else (streaming entries
/// such as `file://localhostsoundcloud:…`) is not a local file.
fn is_file_url(url: &str) -> bool {
    let lower = url.get(..17).unwrap_or(url).to_ascii_lowercase();
    lower.starts_with("file://localhost/") || lower.starts_with("file:///")
}

/// `track.kind` from the XML `Kind` text (used when the file can't be probed
/// and its extension says nothing).
fn kind_from_xml(kind: &str) -> Option<&'static str> {
    Some(match kind.trim() {
        "MP3 File" => "mp3",
        "M4A File" | "AAC File" => "aac",
        "ALAC File" => "alac",
        "FLAC File" => "flac",
        "WAV File" => "wav",
        "AIFF File" => "aiff",
        _ => return None,
    })
}

/// A non-blank text attribute.
fn nz(s: &Option<String>) -> Option<&str> {
    s.as_deref().filter(|s| !s.trim().is_empty())
}

/// XML rating 0/51/102/153/204/255 → 0..5 stars (nearest).
fn rating_stars(r: u8) -> u8 {
    ((u16::from(r) + 25) / 51).min(5) as u8
}

/// BPM × 100, rounded; `None` for 0 (unanalysed) or nonsense.
fn bpm_x100(bpm: f64) -> Option<i64> {
    let v = (bpm * 100.0).round();
    ((1.0..=100_000.0).contains(&v)).then_some(v as i64)
}

fn rgb_int((r, g, b): (u8, u8, u8)) -> u32 {
    u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b)
}

/// `Num` → hot-cue slot: −1 memory cue (`Some(None)`), 0..7 hot cue;
/// anything else `None` (skipped).
fn cue_slot(num: i32) -> Option<Option<u8>> {
    match num {
        -1 => Some(None),
        0..=7 => Some(Some(num as u8)),
        _ => None,
    }
}

struct GridRow {
    /// Whole ms, like every beatgrid position (D10).
    position_ms: i64,
    bpm_x100: i64,
    meter_num: u8,
    meter_den: u8,
    beat_in_bar: u8,
    meter_defaulted: bool,
}

fn grid_row(t: &Tempo) -> Option<GridRow> {
    let position_ms = (t.inizio * 1000.0).round();
    if !position_ms.is_finite() || position_ms < 0.0 || position_ms > i64::MAX as f64 {
        return None;
    }
    let position_ms = position_ms as i64;
    let bpm_x100 = bpm_x100(t.bpm)?;
    let meter = t.metro.as_deref().map(parse_metro);
    let (meter_num, meter_den) = meter.flatten().unwrap_or((4, 4));
    Some(GridRow {
        position_ms,
        bpm_x100,
        meter_num,
        meter_den,
        beat_in_bar: t.battito.filter(|&b| b >= 1).unwrap_or(1),
        meter_defaulted: matches!(meter, Some(None)),
    })
}

/// `"4/4"` → `(4, 4)`.
fn parse_metro(s: &str) -> Option<(u8, u8)> {
    let (n, d) = s.trim().split_once('/')?;
    let n: u8 = n.trim().parse().ok()?;
    let d: u8 = d.trim().parse().ok()?;
    (n > 0 && d > 0).then_some((n, d))
}

fn is_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_root_is_the_deepest_common_folder() {
        let root = |paths: &[&str]| missing_root(paths.iter().copied()).map(PathBuf::from);
        assert_eq!(root(&[]), None);
        assert_eq!(root(&["/m/a/x.mp3"]), Some(PathBuf::from("/m/a")));
        assert_eq!(
            root(&["/m/a/x.mp3", "/m/a/b/y.mp3", "/m/a/c/z.mp3"]),
            Some(PathBuf::from("/m/a"))
        );
        // Sibling names that share a string prefix are not a common folder.
        assert_eq!(
            root(&["/m/ab/x.mp3", "/m/ac/y.mp3"]),
            Some(PathBuf::from("/m"))
        );
        // Only the filesystem root in common: too broad to suggest.
        assert_eq!(root(&["/a/x.mp3", "/b/y.mp3"]), None);
    }

    #[test]
    fn ratings_round_to_stars() {
        for (r, s) in [
            (0, 0),
            (51, 1),
            (102, 2),
            (153, 3),
            (204, 4),
            (255, 5),
            (100, 2),
            (25, 0),
            (26, 1),
        ] {
            assert_eq!(rating_stars(r), s, "rating {r}");
        }
    }

    #[test]
    fn metro_and_grid_rows() {
        assert_eq!(parse_metro("4/4"), Some((4, 4)));
        assert_eq!(parse_metro(" 3 / 4 "), Some((3, 4)));
        assert_eq!(parse_metro("0/4"), None);
        assert_eq!(parse_metro("four"), None);
        let tempo = |metro: Option<&str>, battito| Tempo {
            inizio: 0.025,
            bpm: 127.995,
            metro: metro.map(Into::into),
            battito,
            extra: Default::default(),
        };
        let g = grid_row(&tempo(Some("7/8"), Some(3))).unwrap();
        assert_eq!(g.position_ms, 25);
        // Sub-ms XML values round to whole ms (D10).
        let odd = Tempo {
            inizio: 0.0256,
            ..tempo(None, None)
        };
        assert_eq!(grid_row(&odd).unwrap().position_ms, 26);
        assert_eq!(
            (g.bpm_x100, g.meter_num, g.meter_den, g.beat_in_bar),
            (12800, 7, 8, 3)
        );
        assert!(!g.meter_defaulted);
        let g = grid_row(&tempo(Some("x"), None)).unwrap();
        assert_eq!((g.meter_num, g.meter_den, g.beat_in_bar), (4, 4, 1));
        assert!(g.meter_defaulted);
        assert!(!grid_row(&tempo(None, Some(0))).unwrap().meter_defaulted);
    }

    #[test]
    fn cue_slots() {
        assert_eq!(cue_slot(-1), Some(None));
        assert_eq!(cue_slot(0), Some(Some(0)));
        assert_eq!(cue_slot(7), Some(Some(7)));
        assert_eq!(cue_slot(8), None);
        assert_eq!(cue_slot(-2), None);
    }

    #[test]
    fn bpm_values() {
        assert_eq!(bpm_x100(128.0), Some(12800));
        assert_eq!(bpm_x100(127.996), Some(12800));
        assert_eq!(bpm_x100(0.0), None);
        assert_eq!(bpm_x100(f64::NAN), None);
    }

    #[test]
    fn urls_and_dates() {
        assert!(is_file_url("file://localhost/Users/x.mp3"));
        assert!(is_file_url("FILE:///x.mp3"));
        assert!(!is_file_url("file://localhostsoundcloud:tracks:1"));
        assert!(!is_file_url("https://example.com/x.mp3"));
        assert!(is_date("2026-10-19"));
        assert!(!is_date("2026-1-19"));
        assert!(!is_date("19/10/2026"));
    }

    #[test]
    fn remap_is_component_wise() {
        let remap = prepare_remap(&[PrefixRemap {
            from: "/Music/old".into(),
            to: "/Volumes/new".into(),
        }]);
        // `to` is made absolute: on Windows "/Volumes/new" gains a drive.
        assert_eq!(
            apply_remap(&remap, Path::new("/Music/old/a b.mp3")),
            std::path::absolute("/Volumes/new").unwrap().join("a b.mp3")
        );
        assert_eq!(
            apply_remap(&remap, Path::new("/Music/older/a.mp3")),
            Path::new("/Music/older/a.mp3")
        );
    }

    #[test]
    fn fallback_keys_fold_case_and_space() {
        assert_eq!(
            fallback_key(Some(" DJ X "), Some("Track"), 5),
            fallback_key(Some("dj x"), Some("TRACK"), 5)
        );
        assert_eq!(fallback_key(Some(""), Some("t"), 5), None);
        assert_eq!(fallback_key(None, Some("t"), 5), None);
    }
}
