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

use std::collections::{BTreeSet, HashMap};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use rusqlite::named_params;
use serde::Serialize;
use unicode_normalization::UnicodeNormalization;
use walkdir::WalkDir;

use crate::db::Library;
use crate::error::Result;
use crate::keys::Key;
use crate::timefmt;

/// Row id in the `track` table.
pub type TrackId = i64;

/// File extensions the scan picks up (lower case). Must match what
/// `reks-audio` can probe.
const AUDIO_EXTENSIONS: &[&str] = &["mp3", "m4a", "aac", "flac", "wav", "wave", "aif", "aiff"];

/// Bytes hashed from each end of a file for `content_hash`.
const HASH_EDGE: u64 = 1 << 20;

/// Cooperative cancellation for a running scan. Clones share one flag.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// Progress reported while scanning (sent to the UI over a Tauri channel).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ScanEvent {
    /// An audio file was found during the walk.
    Found {
        path: String,
    },
    /// `done` of `total` found files processed.
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

    let total = walk.files.len();
    for (i, path) in walk.files.iter().enumerate() {
        if s.cancelled() {
            break;
        }
        match existing.get(path) {
            Some(t) => s.rescan(t, path)?,
            None => {
                let moved_from = |hash: &str, size: i64| {
                    let ids = by_hash.get_mut(&(hash.to_owned(), size))?;
                    let pos = ids.iter().position(|id| {
                        // A row that was already missing may live outside the
                        // scanned roots; only claim it if its old file is gone.
                        gone.get(id).is_some_and(|t| !Path::new(&t.path).exists())
                    })?;
                    Some(ids.swap_remove(pos))
                };
                if let Some(id) = s.add_or_move(path, moved_from)? {
                    gone.remove(&id);
                }
            }
        }
        s.emit(ScanEvent::Progress { done: i + 1, total });
    }

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

#[derive(Debug)]
struct ExistingTrack {
    id: TrackId,
    path: String,
    file_size: i64,
    file_mtime: String,
    content_hash: Option<String>,
    missing: bool,
}

/// What a file contributes to its track row.
struct FileInfo {
    size: i64,
    mtime: String,
    hash: String,
    probe: reks_audio::Probe,
    tags: reks_audio::Tags,
    artwork_id: Option<i64>,
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
                        if walk.files.insert(path.clone()) {
                            self.report.found += 1;
                            self.emit(ScanEvent::Found { path });
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

    /// A known path: unchanged, changed on disk, or back after being missing.
    fn rescan(&mut self, t: &ExistingTrack, path: &str) -> Result<()> {
        let Some((size, mtime)) = self.stat(path) else {
            return Ok(());
        };
        if size == t.file_size && mtime == t.file_mtime {
            if t.missing {
                self.lib.conn()?.execute(
                    "UPDATE track SET missing = 0, updated_at = ?2 WHERE id = ?1",
                    (t.id, timefmt::now_utc()),
                )?;
                self.updated(t.id, path);
            } else {
                self.report.unchanged += 1;
            }
            return Ok(());
        }
        let Some(info) = self.read_file(path, size, mtime) else {
            return Ok(());
        };
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

    /// A new path: either a known file that moved (same content hash and
    /// size as a row whose file is gone) or a new track. Returns the id of a
    /// moved row.
    fn add_or_move(
        &mut self,
        path: &str,
        moved_from: impl FnOnce(&str, i64) -> Option<TrackId>,
    ) -> Result<Option<TrackId>> {
        let Some((size, mtime)) = self.stat(path) else {
            return Ok(None);
        };
        let hash = match content_hash(Path::new(path)) {
            Ok(h) => h,
            Err(e) => {
                self.error(path, format!("cannot read file: {e}"));
                return Ok(None);
            }
        };
        if let Some(id) = moved_from(&hash, size) {
            let conn = self.lib.conn()?;
            let from: String =
                conn.query_row("SELECT path FROM track WHERE id = ?1", [id], |r| r.get(0))?;
            conn.execute(
                "UPDATE track SET path = ?2, file_mtime = ?3, missing = 0, updated_at = ?4
                 WHERE id = ?1",
                (id, path, &mtime, timefmt::now_utc()),
            )?;
            drop(conn);
            self.report.moved += 1;
            self.emit(ScanEvent::Moved {
                id,
                from,
                to: path.to_owned(),
            });
            return Ok(Some(id));
        }

        let Some(info) = self.read_file_hashed(path, size, mtime, hash) else {
            return Ok(None);
        };
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
        Ok(None)
    }

    fn updated(&mut self, id: TrackId, path: &str) {
        self.report.updated += 1;
        self.emit(ScanEvent::Updated {
            id,
            path: path.to_owned(),
        });
    }

    /// Size and mtime, or `None` after reporting an error.
    fn stat(&mut self, path: &str) -> Option<(i64, String)> {
        let meta = match std::fs::metadata(path) {
            Ok(m) => m,
            Err(e) => {
                self.error(path, format!("cannot read file: {e}"));
                return None;
            }
        };
        let size = i64::try_from(meta.len()).unwrap_or(i64::MAX);
        let mtime = meta
            .modified()
            .map(timefmt::iso8601_utc)
            .unwrap_or_default();
        Some((size, mtime))
    }

    fn read_file(&mut self, path: &str, size: i64, mtime: String) -> Option<FileInfo> {
        match content_hash(Path::new(path)) {
            Ok(hash) => self.read_file_hashed(path, size, mtime, hash),
            Err(e) => {
                self.error(path, format!("cannot read file: {e}"));
                None
            }
        }
    }

    /// Probe, tags and artwork, done outside the DB lock. `None` if the file
    /// is not playable audio (reported as an error, not added).
    fn read_file_hashed(
        &mut self,
        path: &str,
        size: i64,
        mtime: String,
        hash: String,
    ) -> Option<FileInfo> {
        let p = Path::new(path);
        let probe = match reks_audio::probe(p) {
            Ok(probe) => probe,
            Err(e) => {
                self.error(path, e.to_string());
                return None;
            }
        };
        let tags = reks_audio::tags(p).unwrap_or_else(|e| {
            tracing::warn!(path, error = %e, "no tags read");
            reks_audio::Tags::default()
        });
        let artwork_id = match reks_audio::embedded_artwork(p) {
            Ok(Some(pic)) => self.lib.store_artwork(&pic.data).unwrap_or_else(|e| {
                tracing::warn!(path, error = %e, "artwork not stored");
                None
            }),
            Ok(None) => None,
            Err(e) => {
                tracing::warn!(path, error = %e, "no artwork read");
                None
            }
        };
        Some(FileInfo {
            size,
            mtime,
            hash,
            probe,
            tags,
            artwork_id,
        })
    }
}

/// Probe fields as SQL values.
struct ProbeCols {
    kind: &'static str,
    duration_ms: i64,
    sample_rate: u32,
    bit_depth: Option<u8>,
    bit_rate_kbps: Option<u32>,
    channels: u8,
}

fn probe_cols(p: &reks_audio::Probe) -> ProbeCols {
    ProbeCols {
        kind: p.kind.as_str(),
        duration_ms: i64::try_from(p.duration_ms).unwrap_or(i64::MAX),
        sample_rate: p.sample_rate,
        bit_depth: p.bit_depth,
        bit_rate_kbps: p.bit_rate_kbps,
        channels: p.channels,
    }
}

/// Canonical key from the tag, if it parses.
fn key_cols(tags: &reks_audio::Tags) -> (Option<&'static str>, Option<&'static str>) {
    match tags.key.as_deref().map(Key::parse) {
        Some(Ok(k)) => (Some(k.canonical()), Some("tag")),
        Some(Err(e)) => {
            tracing::debug!(error = %e, "ignoring key tag");
            (None, None)
        }
        None => (None, None),
    }
}

fn bpm_cols(tags: &reks_audio::Tags) -> (Option<u32>, Option<&'static str>) {
    (tags.bpm_x100, tags.bpm_x100.map(|_| "tag"))
}

/// The library form of a path: UTF-8 and NFC, as long as the NFC spelling
/// still opens the same file (true on macOS; on other systems an NFD name is
/// kept as it is on disk).
fn library_path(path: &Path) -> Option<String> {
    let s = path.to_str()?;
    let nfc: String = s.nfc().collect();
    if nfc != s && Path::new(&nfc).exists() {
        Some(nfc)
    } else {
        Some(s.to_owned())
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

/// blake3 over the first and last 1 MiB plus the file size: cheap, and
/// stable across renames and tag-free copies. Hex-encoded.
fn content_hash(path: &Path) -> std::io::Result<String> {
    let mut f = File::open(path)?;
    let len = f.metadata()?.len();
    let mut hasher = blake3::Hasher::new();
    let mut buf = Vec::new();
    (&mut f).take(HASH_EDGE).read_to_end(&mut buf)?;
    hasher.update(&buf);
    if len > HASH_EDGE {
        buf.clear();
        f.seek(SeekFrom::Start(
            len.saturating_sub(HASH_EDGE).max(HASH_EDGE),
        ))?;
        f.take(HASH_EDGE).read_to_end(&mut buf)?;
        hasher.update(&buf);
    }
    hasher.update(&len.to_le_bytes());
    Ok(hasher.finalize().to_hex().to_string())
}
