//! Folder scan: walk library roots, add new audio files, refresh changed ones,
//! follow moved files by content hash, and flag files that disappeared.
//!
//! Conservative by design (a wrong "missing" or a clobbered edit is worse
//! than a stale row):
//! - The DB is the source of truth for metadata. Re-scanning a changed file
//!   refreshes technical fields (size, duration, …) but only fills tag fields
//!   that are still empty.
//! - Nothing is marked missing if the scan was cancelled, for roots that are
//!   absent (e.g. an unplugged drive), or under folders that could not be read.
//! - Rows are never deleted; missing files keep their cues, grids and playlists.
//! - Originals the converter replaced (`superseded_file`) are not added back.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use rusqlite::named_params;
use serde::Serialize;
use walkdir::WalkDir;

use crate::db::Library;
use crate::error::Result;
use crate::fileinfo::{
    self, bpm_cols, content_hash, key_cols, library_path, probe_cols, FileInfo, ProbeCols,
};
use crate::{pool, timefmt};

/// Row id in the `track` table.
pub type TrackId = i64;

/// File extensions the scan picks up (lower case). Must match what
/// `reks-audio` can probe.
const AUDIO_EXTENSIONS: &[&str] = &["mp3", "m4a", "aac", "flac", "wav", "wave", "aif", "aiff"];

/// A [`ScanEvent::Progress`] goes out every this many files (and after the last).
const PROGRESS_EVERY: usize = 50;

/// Cooperative cancellation (scan, import, export, convert). Clones share
/// one flag. One type for the whole stack, so a token passes straight
/// through to `reks_audio::convert`.
pub use reks_audio::CancelToken;

/// Progress reported while scanning (sent to the UI over a Tauri channel).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ScanEvent {
    /// `done` of `total` found files processed. Sent every
    /// [`PROGRESS_EVERY`] files and after the last.
    Progress {
        done: usize,
        total: usize,
    },
    Added {
        id: TrackId,
        path: String,
    },
    /// File changed on disk, or reappeared after being missing.
    Updated {
        id: TrackId,
        path: String,
    },
    Moved {
        id: TrackId,
        from: String,
        to: String,
    },
    Missing {
        id: TrackId,
        path: String,
    },
    /// A file or folder could not be read; the scan carries on.
    Error {
        path: String,
        message: String,
    },
}

/// One problem met during a scan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScanError {
    pub path: String,
    pub message: String,
}

/// Totals for a finished (or cancelled) scan.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ScanReport {
    pub found: usize,
    pub added: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub moved: usize,
    pub missing: usize,
    pub errors: Vec<ScanError>,
    pub cancelled: bool,
}

/// Scans `roots` recursively and brings the library in line with them.
/// `progress` is called on the scanning thread for every event.
pub fn scan(
    roots: &[PathBuf],
    lib: &Library,
    cancel: &CancelToken,
    progress: impl Fn(&ScanEvent),
) -> Result<ScanReport> {
    let mut s = Scan {
        lib,
        cancel,
        progress: &progress,
        report: ScanReport::default(),
    };
    let walk = s.walk(roots);
    let existing = s.existing_tracks()?;
    let superseded = s.superseded_paths()?;

    // Rows whose file was not found: candidates for "moved" and "missing".
    let mut gone: HashMap<TrackId, &ExistingTrack> = existing
        .values()
        .filter(|t| !walk.files.contains(&t.path) && (t.missing || walk.covers(&t.path)))
        .map(|t| (t.id, t))
        .collect();
    let mut by_hash: HashMap<(String, i64), Vec<TrackId>> = HashMap::new();
    for t in gone.values() {
        if let Some(h) = &t.content_hash {
            by_hash
                .entry((h.clone(), t.file_size))
                .or_default()
                .push(t.id);
        }
    }

    // Files are read on a pool (stat, hash, probe, tags, artwork) and written
    // here one by one, in walk order. A new file whose hash could be a move is
    // only hashed there, so a move reads no tags and stores no artwork.
    let files: Vec<&str> = walk.files.iter().map(String::as_str).collect();
    let move_keys: HashSet<(String, i64)> = by_hash.keys().cloned().collect();
    let lib = s.lib;
    let read = |path: &&str| prepare(lib, path, &existing, &superseded, &move_keys);
    let total = files.len();
    pool::for_each_ordered(&files, &|| cancel.is_cancelled(), read, |i, prepared| {
        if s.cancelled() {
            return Ok(());
        }
        let path = files[i];
        match prepared {
            Prepared::Superseded => tracing::debug!(path, "skipping converted-away original"),
            Prepared::Failed(message) => s.error(path, message),
            Prepared::Unchanged(t) => s.unchanged(t, path)?,
            Prepared::Changed(t, info) => {
                if let Some(info) = s.checked(path, info) {
                    s.refresh(t, path, &info)?;
                }
            }
            Prepared::New(info) => {
                if let Some(info) = s.checked(path, info) {
                    s.add(path, &info)?;
                }
            }
            Prepared::MaybeMoved { size, mtime, hash } => {
                let moved_from = by_hash.get_mut(&(hash.clone(), size)).and_then(|ids| {
                    let pos = ids.iter().position(|id| {
                        // A row that was already missing may live outside the
                        // scanned roots; only claim it if its old file is gone.
                        gone.get(id).is_some_and(|t| !Path::new(&t.path).exists())
                    })?;
                    Some(ids.swap_remove(pos))
                });
                if let Some(id) = moved_from {
                    s.moved(id, path, &mtime)?;
                    gone.remove(&id);
                } else {
                    let info = fileinfo::read_file_hashed(lib, path, size, mtime, hash);
                    if let Some(info) = s.checked(path, info) {
                        s.add(path, &info)?;
                    }
                }
            }
        }
        if (i + 1) % PROGRESS_EVERY == 0 || i + 1 == total {
            s.emit(ScanEvent::Progress { done: i + 1, total });
        }
        Ok::<(), crate::error::Error>(())
    })?;

    if s.cancelled() {
        s.report.cancelled = true;
        return Ok(s.report);
    }
    let mut newly_missing: Vec<&ExistingTrack> =
        gone.into_values().filter(|t| !t.missing).collect();
    newly_missing.sort_by_key(|t| t.id);
    for t in newly_missing {
        s.lib.conn()?.execute(
            "UPDATE track SET missing = 1, updated_at = ?2 WHERE id = ?1",
            (t.id, timefmt::now_utc()),
        )?;
        s.report.missing += 1;
        s.emit(ScanEvent::Missing {
            id: t.id,
            path: t.path.clone(),
        });
    }
    let now = timefmt::now_utc();
    for root in &walk.roots {
        s.lib.conn()?.execute(
            "UPDATE library_root SET last_scan = ?2 WHERE path = ?1",
            (root, &now),
        )?;
    }
    Ok(s.report)
}

struct Scan<'a> {
    lib: &'a Library,
    cancel: &'a CancelToken,
    progress: &'a dyn Fn(&ScanEvent),
    report: ScanReport,
}

/// Result of walking the roots.
#[derive(Default)]
struct Walk {
    /// Library paths of audio files found, sorted.
    files: BTreeSet<String>,
    /// Roots that were walked (library path form).
    roots: Vec<String>,
    /// Folders that could not be read; nothing under them is marked missing.
    failed: Vec<String>,
}

impl Walk {
    /// True if a missing file at `path` would have been seen by this walk.
    fn covers(&self, path: &str) -> bool {
        self.roots.iter().any(|r| is_under(path, r))
            && !self.failed.iter().any(|f| is_under(path, f))
    }
}

/// What the pool learned about one walked file, before the DB is touched.
enum Prepared<'a> {
    /// A converted-away original; never added back.
    Superseded,
    /// Stat or hash failed; reported as a scan error.
    Failed(String),
    /// Known path with the size and mtime its row records.
    Unchanged(&'a ExistingTrack),
    /// Known path whose file changed. `Err`: not playable audio.
    Changed(&'a ExistingTrack, std::result::Result<FileInfo, String>),
    /// New path whose hash matches no row that could have moved.
    New(std::result::Result<FileInfo, String>),
    /// New path whose (hash, size) matches a row whose file is gone.
    MaybeMoved {
        size: i64,
        mtime: String,
        hash: String,
    },
}

/// Runs on the pool: everything the scan needs from the file itself.
fn prepare<'a>(
    lib: &Library,
    path: &str,
    existing: &'a HashMap<String, ExistingTrack>,
    superseded: &HashSet<String>,
    move_keys: &HashSet<(String, i64)>,
) -> Prepared<'a> {
    let known = existing.get(path);
    if known.is_none() && superseded.contains(path) {
        return Prepared::Superseded;
    }
    let cannot_read = |e: std::io::Error| Prepared::Failed(format!("cannot read file: {e}"));
    let (size, mtime) = match fileinfo::stat(path) {
        Ok(s) => s,
        Err(e) => return cannot_read(e),
    };
    if let Some(t) = known {
        if size == t.file_size && mtime == t.file_mtime {
            return Prepared::Unchanged(t);
        }
        return Prepared::Changed(t, fileinfo::read_file(lib, path, size, mtime));
    }
    let hash = match content_hash(Path::new(path)) {
        Ok(h) => h,
        Err(e) => return cannot_read(e),
    };
    if move_keys.contains(&(hash.clone(), size)) {
        return Prepared::MaybeMoved { size, mtime, hash };
    }
    Prepared::New(fileinfo::read_file_hashed(lib, path, size, mtime, hash))
}

#[derive(Debug)]
struct ExistingTrack {
    id: TrackId,
    path: String,
    file_size: i64,
    file_mtime: String,
    content_hash: Option<String>,
    missing: bool,
}

impl Scan<'_> {
    fn emit(&self, event: ScanEvent) {
        (self.progress)(&event);
    }

    fn cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    fn error(&mut self, path: &str, message: String) {
        tracing::warn!(path, %message, "scan error");
        self.report.errors.push(ScanError {
            path: path.to_owned(),
            message: message.clone(),
        });
        self.emit(ScanEvent::Error {
            path: path.to_owned(),
            message,
        });
    }

    fn walk(&mut self, roots: &[PathBuf]) -> Walk {
        let mut walk = Walk::default();
        for root in roots {
            let root = std::path::absolute(root).unwrap_or_else(|_| root.clone());
            let Some(root_key) = library_path(&root) else {
                self.error(
                    &root.to_string_lossy(),
                    "folder name is not valid UTF-8".into(),
                );
                continue;
            };
            if !root.is_dir() {
                self.error(&root_key, "folder not found (not mounted?)".into());
                continue;
            }
            walk.roots.push(root_key);
            let entries = WalkDir::new(&root)
                .follow_links(false)
                .into_iter()
                .filter_entry(|e| e.depth() == 0 || !is_hidden(e.file_name()));
            for entry in entries {
                if self.cancelled() {
                    return walk;
                }
                let entry = match entry {
                    Ok(e) => e,
                    Err(e) => {
                        let path = e.path().unwrap_or(&root).to_path_buf();
                        let key = library_path(&path)
                            .unwrap_or_else(|| path.to_string_lossy().into_owned());
                        walk.failed.push(key.clone());
                        self.error(&key, e.to_string());
                        continue;
                    }
                };
                if !entry.file_type().is_file() || !is_audio(entry.path()) {
                    continue;
                }
                match library_path(entry.path()) {
                    Some(path) => {
                        if walk.files.insert(path) {
                            self.report.found += 1;
                        }
                    }
                    None => self.error(
                        &entry.path().to_string_lossy(),
                        "file name is not valid UTF-8".into(),
                    ),
                }
            }
        }
        walk
    }

    fn existing_tracks(&self) -> Result<HashMap<String, ExistingTrack>> {
        let conn = self.lib.conn()?;
        let mut stmt = conn
            .prepare("SELECT id, path, file_size, file_mtime, content_hash, missing FROM track")?;
        let rows = stmt.query_map([], |r| {
            Ok(ExistingTrack {
                id: r.get(0)?,
                path: r.get(1)?,
                file_size: r.get(2)?,
                file_mtime: r.get(3)?,
                content_hash: r.get(4)?,
                missing: r.get(5)?,
            })
        })?;
        let mut map = HashMap::new();
        for t in rows {
            let t = t?;
            map.insert(t.path.clone(), t);
        }
        Ok(map)
    }

    /// Originals the converter replaced; the scan never adds them back.
    fn superseded_paths(&self) -> Result<HashSet<String>> {
        let conn = self.lib.conn()?;
        let mut stmt = conn.prepare("SELECT old_path FROM superseded_file")?;
        let paths = stmt
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(paths)
    }

    /// A known path whose file is as its row records; back if it was missing.
    fn unchanged(&mut self, t: &ExistingTrack, path: &str) -> Result<()> {
        if t.missing {
            self.lib.conn()?.execute(
                "UPDATE track SET missing = 0, updated_at = ?2 WHERE id = ?1",
                (t.id, timefmt::now_utc()),
            )?;
            self.updated(t.id, path);
        } else {
            self.report.unchanged += 1;
        }
        Ok(())
    }

    /// A known path whose file changed on disk.
    fn refresh(&mut self, t: &ExistingTrack, path: &str, info: &FileInfo) -> Result<()> {
        let ProbeCols {
            kind,
            duration_ms,
            sample_rate,
            bit_depth,
            bit_rate_kbps,
            channels,
        } = probe_cols(&info.probe);
        let tags = &info.tags;
        let (key, key_source) = key_cols(tags);
        let (bpm, bpm_source) = bpm_cols(tags);
        self.lib.conn()?.execute(
            "UPDATE track SET
               file_size = :size, file_mtime = :mtime, content_hash = :hash, missing = 0,
               kind = :kind, duration_ms = :duration_ms, sample_rate = :sample_rate,
               bit_depth = :bit_depth, bit_rate_kbps = :bit_rate_kbps, channels = :channels,
               title = coalesce(title, :title), artist = coalesce(artist, :artist),
               album = coalesce(album, :album), album_artist = coalesce(album_artist, :album_artist),
               genre = coalesce(genre, :genre), composer = coalesce(composer, :composer),
               remixer = coalesce(remixer, :remixer), label = coalesce(label, :label),
               grouping = coalesce(grouping, :grouping), track_no = coalesce(track_no, :track_no),
               disc_no = coalesce(disc_no, :disc_no), year = coalesce(year, :year),
               comment = coalesce(comment, :comment),
               key_source = CASE WHEN key IS NULL THEN :key_source ELSE key_source END,
               key = coalesce(key, :key),
               bpm_source = CASE WHEN bpm_x100 IS NULL THEN :bpm_source ELSE bpm_source END,
               bpm_x100 = coalesce(bpm_x100, :bpm),
               artwork_id = coalesce(artwork_id, :artwork_id),
               updated_at = :now
             WHERE id = :id",
            named_params! {
                ":id": t.id, ":size": info.size, ":mtime": info.mtime, ":hash": info.hash,
                ":kind": kind, ":duration_ms": duration_ms, ":sample_rate": sample_rate,
                ":bit_depth": bit_depth, ":bit_rate_kbps": bit_rate_kbps, ":channels": channels,
                ":title": tags.title, ":artist": tags.artist, ":album": tags.album,
                ":album_artist": tags.album_artist, ":genre": tags.genre,
                ":composer": tags.composer, ":remixer": tags.remixer, ":label": tags.label,
                ":grouping": tags.grouping, ":track_no": tags.track_no,
                ":disc_no": tags.disc_no, ":year": tags.year, ":comment": tags.comment,
                ":key": key, ":key_source": key_source, ":bpm": bpm, ":bpm_source": bpm_source,
                ":artwork_id": info.artwork_id, ":now": timefmt::now_utc(),
            },
        )?;
        self.updated(t.id, path);
        Ok(())
    }

    /// A row whose file moved to `path` (same content hash and size).
    fn moved(&mut self, id: TrackId, path: &str, mtime: &str) -> Result<()> {
        let conn = self.lib.conn()?;
        let from: String =
            conn.query_row("SELECT path FROM track WHERE id = ?1", [id], |r| r.get(0))?;
        conn.execute(
            "UPDATE track SET path = ?2, file_mtime = ?3, missing = 0, updated_at = ?4
             WHERE id = ?1",
            (id, path, mtime, timefmt::now_utc()),
        )?;
        drop(conn);
        self.report.moved += 1;
        self.emit(ScanEvent::Moved {
            id,
            from,
            to: path.to_owned(),
        });
        Ok(())
    }

    /// A new track.
    fn add(&mut self, path: &str, info: &FileInfo) -> Result<()> {
        let ProbeCols {
            kind,
            duration_ms,
            sample_rate,
            bit_depth,
            bit_rate_kbps,
            channels,
        } = probe_cols(&info.probe);
        let tags = &info.tags;
        let (key, key_source) = key_cols(tags);
        let (bpm, bpm_source) = bpm_cols(tags);
        let now = timefmt::now_utc();
        let conn = self.lib.conn()?;
        conn.execute(
            "INSERT INTO track (
               path, file_size, file_mtime, content_hash, kind, duration_ms, sample_rate,
               bit_depth, bit_rate_kbps, channels, title, artist, album, album_artist, genre,
               composer, remixer, label, grouping, track_no, disc_no, year, comment,
               key, key_source, bpm_x100, bpm_source, artwork_id, date_added,
               created_at, updated_at)
             VALUES (
               :path, :size, :mtime, :hash, :kind, :duration_ms, :sample_rate,
               :bit_depth, :bit_rate_kbps, :channels, :title, :artist, :album, :album_artist,
               :genre, :composer, :remixer, :label, :grouping, :track_no, :disc_no, :year,
               :comment, :key, :key_source, :bpm, :bpm_source, :artwork_id, :date_added,
               :now, :now)",
            named_params! {
                ":path": path, ":size": info.size, ":mtime": info.mtime, ":hash": info.hash,
                ":kind": kind, ":duration_ms": duration_ms, ":sample_rate": sample_rate,
                ":bit_depth": bit_depth, ":bit_rate_kbps": bit_rate_kbps, ":channels": channels,
                ":title": tags.title, ":artist": tags.artist, ":album": tags.album,
                ":album_artist": tags.album_artist, ":genre": tags.genre,
                ":composer": tags.composer, ":remixer": tags.remixer, ":label": tags.label,
                ":grouping": tags.grouping, ":track_no": tags.track_no,
                ":disc_no": tags.disc_no, ":year": tags.year, ":comment": tags.comment,
                ":key": key, ":key_source": key_source, ":bpm": bpm, ":bpm_source": bpm_source,
                ":artwork_id": info.artwork_id, ":date_added": timefmt::today_utc(), ":now": now,
            },
        )?;
        let id = conn.last_insert_rowid();
        drop(conn);
        self.report.added += 1;
        self.emit(ScanEvent::Added {
            id,
            path: path.to_owned(),
        });
        Ok(())
    }

    fn updated(&mut self, id: TrackId, path: &str) {
        self.report.updated += 1;
        self.emit(ScanEvent::Updated {
            id,
            path: path.to_owned(),
        });
    }

    /// `None` after reporting a file that is not playable audio.
    fn checked(
        &mut self,
        path: &str,
        info: std::result::Result<FileInfo, String>,
    ) -> Option<FileInfo> {
        info.map_err(|message| self.error(path, message)).ok()
    }
}

fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| AUDIO_EXTENSIONS.iter().any(|a| a.eq_ignore_ascii_case(e)))
}

/// Dot-files and dot-folders: macOS AppleDouble `._*` files, `.Trashes`,
/// `.Spotlight-V100`, etc.
fn is_hidden(name: &std::ffi::OsStr) -> bool {
    name.to_str().is_some_and(|n| n.starts_with('.'))
}

/// `path` is `dir` or inside it (component-wise, not a bare string prefix).
fn is_under(path: &str, dir: &str) -> bool {
    Path::new(path).starts_with(dir)
}
