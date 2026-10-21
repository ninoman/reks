//! On-disk analysis cache (phase 3, "Analysis cache"):
//! `<cache_dir>/analysis/<track_id>/{analysis.json, bands.bin}`.
//!
//! `analysis.json` holds everything but the waveform columns (beats, grid,
//! key, confidences) plus the analyser version and the file's content hash;
//! an entry is valid only when both match ([`load_valid`]). `bands.bin` is
//! the [`WaveformColumns`] as raw bytes (not `columns.bin`: that name belongs
//! to the grid editor's columns cache, task 13, which has its own format;
//! the two are to be merged into one file once both have landed):
//!
//! ```text
//! b"RKWC"  u32 LE format (1)  u32 LE column count n
//! low[n]  mid[n]  high[n]  mono[n]
//! ```
//!
//! Files are written to a temporary name and renamed, columns first, so a
//! crash leaves either the old entry, a stale one, or the new one; a
//! half-written JSON never parses as valid.
//!
//! A track rekordbox analysed can instead have a **rekordbox entry** in the
//! same folder (D10, "rekordbox mapping"): copies of rekordbox's
//! `ANLZ0000.DAT/.EXT/.2EX` as `rekordbox.{DAT,EXT,2EX}` and
//! `rekordbox.json` (`source: "rekordbox"`, the file's content hash, the
//! track length). Its grid, key and BPM are the library's rows from the XML
//! import, so they are not repeated here. It is versioned by
//! [`REKORDBOX_FORMAT`], not the analyser version: a new analyser doesn't
//! make rekordbox's analysis stale. Written files first, JSON last.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use reks_core::{Key, TrackId};
use serde::{Deserialize, Serialize};

use crate::beats::{Beat, BeatList};
use crate::grid::{Grid, GridBeat};
use crate::key::KeyAnalysis;
use crate::pipeline::{TrackAnalysis, ANALYSER_VERSION};
use crate::waveform::WaveformColumns;

const META_FILE: &str = "analysis.json";
const COLUMNS_FILE: &str = "bands.bin";
const COLUMNS_MAGIC: &[u8; 4] = b"RKWC";
const COLUMNS_FORMAT: u32 = 1;
const RB_META_FILE: &str = "rekordbox.json";
/// Extensions of rekordbox's three files, each saved as `rekordbox.<ext>`.
const RB_EXTS: [&str; 3] = ["DAT", "EXT", "2EX"];
/// Bump when the rekordbox entry's layout changes.
pub const REKORDBOX_FORMAT: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("analysis cache I/O at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("analysis cache entry {path} is unreadable: {reason}")]
    Corrupt { path: PathBuf, reason: String },
}

fn io_err(path: &Path) -> impl FnOnce(io::Error) -> CacheError + '_ {
    move |source| CacheError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// Directory of one track's entry.
pub fn entry_dir(cache_dir: &Path, track: TrackId) -> PathBuf {
    cache_dir.join("analysis").join(track.to_string())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Meta {
    analyser_version: u32,
    content_hash: String,
    duration_ms: u64,
    /// Rate of every sample position below.
    sample_rate: u32,
    beat_confidence: f32,
    /// `[sample, downbeat]` pairs.
    beats: Vec<(i64, bool)>,
    grid: Option<MetaGrid>,
    /// Canonical key name; `None` = silence.
    key: Option<String>,
    key_confidence: f32,
    key_similarity: f32,
    chroma: [f32; 12],
    columns: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum MetaGrid {
    #[serde(rename_all = "camelCase")]
    Constant {
        bpm: f64,
        first_beat: i64,
        first_beat_in_bar: u8,
    },
    /// `[sample, beat_in_bar]` pairs.
    Dynamic { beats: Vec<(i64, u8)> },
}

impl From<&Grid> for MetaGrid {
    fn from(g: &Grid) -> Self {
        match g {
            Grid::Constant {
                bpm,
                first_beat,
                first_beat_in_bar,
            } => Self::Constant {
                bpm: *bpm,
                first_beat: *first_beat,
                first_beat_in_bar: *first_beat_in_bar,
            },
            Grid::Dynamic { beats } => Self::Dynamic {
                beats: beats.iter().map(|b| (b.position, b.beat_in_bar)).collect(),
            },
        }
    }
}

impl From<MetaGrid> for Grid {
    fn from(g: MetaGrid) -> Self {
        match g {
            MetaGrid::Constant {
                bpm,
                first_beat,
                first_beat_in_bar,
            } => Self::Constant {
                bpm,
                first_beat,
                first_beat_in_bar,
            },
            MetaGrid::Dynamic { beats } => Self::Dynamic {
                beats: beats
                    .into_iter()
                    .map(|(position, beat_in_bar)| GridBeat {
                        position,
                        beat_in_bar,
                    })
                    .collect(),
            },
        }
    }
}

/// Writes `analysis` for `track`, analysed from a file with `content_hash`.
pub fn write(
    cache_dir: &Path,
    track: TrackId,
    analysis: &TrackAnalysis,
    content_hash: &str,
) -> Result<(), CacheError> {
    let dir = entry_dir(cache_dir, track);
    fs::create_dir_all(&dir).map_err(io_err(&dir))?;
    let meta = Meta {
        analyser_version: ANALYSER_VERSION,
        content_hash: content_hash.to_owned(),
        duration_ms: analysis.duration_ms,
        sample_rate: analysis.beats.sample_rate,
        beat_confidence: analysis.beats.confidence,
        beats: analysis
            .beats
            .beats
            .iter()
            .map(|b| (b.sample, b.downbeat))
            .collect(),
        grid: analysis.grid.as_ref().map(MetaGrid::from),
        key: analysis.key.key.map(|k| k.canonical().to_owned()),
        key_confidence: analysis.key.confidence,
        key_similarity: analysis.key.similarity,
        chroma: analysis.key.chroma,
        columns: analysis.columns.len(),
    };
    let json = serde_json::to_vec_pretty(&meta).map_err(|e| CacheError::Corrupt {
        path: dir.join(META_FILE),
        reason: e.to_string(),
    })?;
    replace(&dir.join(COLUMNS_FILE), &encode_columns(&analysis.columns))?;
    replace(&dir.join(META_FILE), &json)
}

/// Reads `track`'s entry if it was written by this analyser version for a
/// file with `content_hash`. Missing, stale or unreadable entries give
/// `None` (unreadable ones are logged).
pub fn load_valid(cache_dir: &Path, track: TrackId, content_hash: &str) -> Option<TrackAnalysis> {
    match read(cache_dir, track) {
        Ok(Some((meta, analysis)))
            if meta.analyser_version == ANALYSER_VERSION && meta.content_hash == content_hash =>
        {
            Some(analysis)
        }
        Ok(_) => None,
        Err(err) => {
            tracing::warn!(track, %err, "ignoring analysis cache entry");
            None
        }
    }
}

/// Whether `track` has an entry [`load_valid`] would return, judged from
/// `analysis.json` alone (the export planner's cheap check; the columns are
/// read when the ANLZ files are built).
pub fn has_valid(cache_dir: &Path, track: TrackId, content_hash: &str) -> bool {
    let dir = entry_dir(cache_dir, track);
    let Ok(json) = fs::read(dir.join(META_FILE)) else {
        return false;
    };
    match serde_json::from_slice::<Meta>(&json) {
        Ok(meta) => {
            meta.analyser_version == ANALYSER_VERSION
                && meta.content_hash == content_hash
                && dir.join(COLUMNS_FILE).is_file()
        }
        Err(err) => {
            tracing::warn!(track, %err, "ignoring analysis cache entry");
            false
        }
    }
}

/// Deletes `track`'s entry, if any.
pub fn remove(cache_dir: &Path, track: TrackId) -> Result<(), CacheError> {
    let dir = entry_dir(cache_dir, track);
    match fs::remove_dir_all(&dir) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(io_err(&dir)(e)),
        _ => Ok(()),
    }
}

/// rekordbox's analysis of one track, as copied into the cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RekordboxEntry {
    /// The library's track length when the entry was written.
    pub duration_ms: u64,
    /// `ANLZ0000.DAT`, `.EXT` and `.2EX` bytes.
    pub dat: Vec<u8>,
    pub ext: Vec<u8>,
    pub ex2: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RbMeta {
    format: u32,
    /// Always `rekordbox`: where every value of this entry came from.
    source: String,
    content_hash: String,
    duration_ms: u64,
    /// The `.DAT` it was copied from (for people reading the cache).
    copied_from: PathBuf,
}

/// Writes `entry` as `track`'s rekordbox entry for a file with
/// `content_hash`; `from` is rekordbox's `.DAT` it was read from.
pub fn write_rekordbox(
    cache_dir: &Path,
    track: TrackId,
    entry: &RekordboxEntry,
    content_hash: &str,
    from: &Path,
) -> Result<(), CacheError> {
    let dir = entry_dir(cache_dir, track);
    fs::create_dir_all(&dir).map_err(io_err(&dir))?;
    let meta = RbMeta {
        format: REKORDBOX_FORMAT,
        source: "rekordbox".to_owned(),
        content_hash: content_hash.to_owned(),
        duration_ms: entry.duration_ms,
        copied_from: from.to_path_buf(),
    };
    let json = serde_json::to_vec_pretty(&meta).map_err(|e| CacheError::Corrupt {
        path: dir.join(RB_META_FILE),
        reason: e.to_string(),
    })?;
    for (ext, bytes) in RB_EXTS.iter().zip([&entry.dat, &entry.ext, &entry.ex2]) {
        replace(&rb_file(&dir, ext), bytes)?;
    }
    replace(&dir.join(RB_META_FILE), &json)
}

/// `track`'s rekordbox entry if it was written in this format for a file
/// with `content_hash`. Missing, stale or unreadable entries give `None`
/// (unreadable ones are logged).
pub fn load_rekordbox(
    cache_dir: &Path,
    track: TrackId,
    content_hash: &str,
) -> Option<RekordboxEntry> {
    let dir = entry_dir(cache_dir, track);
    let meta = rb_meta(&dir, track, content_hash)?;
    let read = |ext: &str| match fs::read(rb_file(&dir, ext)) {
        Ok(b) => Some(b),
        Err(err) => {
            tracing::warn!(track, %err, "ignoring rekordbox cache entry");
            None
        }
    };
    Some(RekordboxEntry {
        duration_ms: meta.duration_ms,
        dat: read("DAT")?,
        ext: read("EXT")?,
        ex2: read("2EX")?,
    })
}

/// Whether [`load_rekordbox`] would find an entry, from the JSON and the
/// files' presence (the export planner's cheap check).
pub fn has_valid_rekordbox(cache_dir: &Path, track: TrackId, content_hash: &str) -> bool {
    let dir = entry_dir(cache_dir, track);
    rb_meta(&dir, track, content_hash).is_some()
        && RB_EXTS.iter().all(|e| rb_file(&dir, e).is_file())
}

fn rb_file(dir: &Path, ext: &str) -> PathBuf {
    dir.join(format!("rekordbox.{ext}"))
}

fn rb_meta(dir: &Path, track: TrackId, content_hash: &str) -> Option<RbMeta> {
    let json = fs::read(dir.join(RB_META_FILE)).ok()?;
    match serde_json::from_slice::<RbMeta>(&json) {
        Ok(m) if m.format == REKORDBOX_FORMAT && m.content_hash == content_hash => Some(m),
        Ok(_) => None,
        Err(err) => {
            tracing::warn!(track, %err, "ignoring rekordbox cache entry");
            None
        }
    }
}

fn read(cache_dir: &Path, track: TrackId) -> Result<Option<(Meta, TrackAnalysis)>, CacheError> {
    let dir = entry_dir(cache_dir, track);
    let meta_path = dir.join(META_FILE);
    let json = match fs::read(&meta_path) {
        Ok(b) => b,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io_err(&meta_path)(e)),
    };
    let corrupt = |reason: String| CacheError::Corrupt {
        path: meta_path.clone(),
        reason,
    };
    let meta: Meta = serde_json::from_slice(&json).map_err(|e| corrupt(e.to_string()))?;
    let cols_path = dir.join(COLUMNS_FILE);
    let bytes = match fs::read(&cols_path) {
        Ok(b) => b,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io_err(&cols_path)(e)),
    };
    let columns = decode_columns(&bytes).ok_or_else(|| CacheError::Corrupt {
        path: cols_path.clone(),
        reason: "bad bands.bin".into(),
    })?;
    if columns.len() != meta.columns {
        return Err(corrupt(format!(
            "{} columns on disk, {} expected",
            columns.len(),
            meta.columns
        )));
    }
    let key = match &meta.key {
        Some(name) => Some(Key::parse(name).map_err(|e| corrupt(e.to_string()))?),
        None => None,
    };
    let analysis = TrackAnalysis {
        duration_ms: meta.duration_ms,
        beats: BeatList {
            sample_rate: meta.sample_rate,
            beats: meta
                .beats
                .iter()
                .map(|&(sample, downbeat)| Beat { sample, downbeat })
                .collect(),
            confidence: meta.beat_confidence,
        },
        grid: meta.grid.clone().map(Grid::from),
        key: KeyAnalysis {
            key,
            confidence: meta.key_confidence,
            similarity: meta.key_similarity,
            chroma: meta.chroma,
        },
        columns,
    };
    Ok(Some((meta, analysis)))
}

/// Writes `bytes` to `path` through a temporary file in the same directory.
fn replace(path: &Path, bytes: &[u8]) -> Result<(), CacheError> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes).map_err(io_err(&tmp))?;
    fs::rename(&tmp, path).map_err(io_err(path))
}

fn encode_columns(c: &WaveformColumns) -> Vec<u8> {
    let n = c.len();
    let mut out = Vec::with_capacity(12 + 4 * n);
    out.extend_from_slice(COLUMNS_MAGIC);
    out.extend_from_slice(&COLUMNS_FORMAT.to_le_bytes());
    // More than u32::MAX columns would be an 11-month track.
    out.extend_from_slice(&u32::try_from(n).unwrap_or(u32::MAX).to_le_bytes());
    for band in [&c.low, &c.mid, &c.high, &c.mono] {
        out.extend_from_slice(&band[..n.min(band.len())]);
    }
    out
}

fn decode_columns(b: &[u8]) -> Option<WaveformColumns> {
    let u32_at = |at: usize| -> Option<u32> {
        Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
    };
    if b.get(..4)? != COLUMNS_MAGIC || u32_at(4)? != COLUMNS_FORMAT {
        return None;
    }
    let n = usize::try_from(u32_at(8)?).ok()?;
    if b.len() != 12 + 4 * n {
        return None;
    }
    let band = |i: usize| b[12 + i * n..12 + (i + 1) * n].to_vec();
    Some(WaveformColumns {
        low: band(0),
        mid: band(1),
        high: band(2),
        mono: band(3),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> TrackAnalysis {
        TrackAnalysis {
            duration_ms: 1234,
            beats: BeatList {
                sample_rate: 44_100,
                beats: vec![
                    Beat {
                        sample: 10,
                        downbeat: true,
                    },
                    Beat {
                        sample: 20_000,
                        downbeat: false,
                    },
                ],
                confidence: 0.75,
            },
            grid: Some(Grid::Dynamic {
                beats: vec![
                    GridBeat {
                        position: 10,
                        beat_in_bar: 1,
                    },
                    GridBeat {
                        position: 20_000,
                        beat_in_bar: 2,
                    },
                ],
            }),
            key: KeyAnalysis {
                key: Some(Key::parse("F#m").unwrap()),
                confidence: 0.25,
                similarity: 0.5,
                chroma: [0.125; 12],
            },
            columns: WaveformColumns {
                low: vec![1, 2, 3],
                mid: vec![4, 5, 6],
                high: vec![7, 8, 9],
                mono: vec![10, 11, 12],
            },
        }
    }

    #[test]
    fn round_trip_and_invalidation() {
        let dir = tempfile::tempdir().unwrap();
        let a = sample();
        assert!(load_valid(dir.path(), 5, "h").is_none());
        write(dir.path(), 5, &a, "h").unwrap();
        assert!(entry_dir(dir.path(), 5).join("analysis.json").is_file());
        assert!(entry_dir(dir.path(), 5).join("bands.bin").is_file());
        assert_eq!(load_valid(dir.path(), 5, "h"), Some(a.clone()));
        // Other contents: stale.
        assert!(load_valid(dir.path(), 5, "other").is_none());
        assert!(has_valid(dir.path(), 5, "h"));
        assert!(!has_valid(dir.path(), 5, "other"));
        assert!(!has_valid(dir.path(), 6, "h"));

        // Constant grid and silence survive too.
        let mut b = a;
        b.grid = Some(Grid::Constant {
            bpm: 127.5,
            first_beat: 99,
            first_beat_in_bar: 4,
        });
        b.key.key = None;
        write(dir.path(), 5, &b, "h2").unwrap();
        assert_eq!(load_valid(dir.path(), 5, "h2"), Some(b));

        remove(dir.path(), 5).unwrap();
        assert!(load_valid(dir.path(), 5, "h2").is_none());
        remove(dir.path(), 5).unwrap();
    }

    #[test]
    fn other_analyser_version_is_stale() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), 1, &sample(), "h").unwrap();
        let path = entry_dir(dir.path(), 1).join("analysis.json");
        let mut meta: Meta = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        meta.analyser_version = ANALYSER_VERSION + 1;
        fs::write(&path, serde_json::to_vec(&meta).unwrap()).unwrap();
        assert!(load_valid(dir.path(), 1, "h").is_none());
        assert!(!has_valid(dir.path(), 1, "h"));
    }

    #[test]
    fn corrupt_entries_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), 1, &sample(), "h").unwrap();
        let cols = entry_dir(dir.path(), 1).join("bands.bin");
        let mut bytes = fs::read(&cols).unwrap();
        bytes.pop();
        fs::write(&cols, &bytes).unwrap();
        assert!(load_valid(dir.path(), 1, "h").is_none());
        fs::write(entry_dir(dir.path(), 1).join("analysis.json"), b"{").unwrap();
        assert!(load_valid(dir.path(), 1, "h").is_none());
        assert!(!has_valid(dir.path(), 1, "h"));
    }

    #[test]
    fn rekordbox_entry_round_trip_and_invalidation() {
        let dir = tempfile::tempdir().unwrap();
        let e = RekordboxEntry {
            duration_ms: 300_000,
            dat: b"dat".to_vec(),
            ext: b"ext".to_vec(),
            ex2: b"2ex".to_vec(),
        };
        assert!(load_rekordbox(dir.path(), 3, "h").is_none());
        write_rekordbox(dir.path(), 3, &e, "h", Path::new("/rb/ANLZ0000.DAT")).unwrap();
        assert_eq!(load_rekordbox(dir.path(), 3, "h"), Some(e.clone()));
        assert!(has_valid_rekordbox(dir.path(), 3, "h"));
        assert!(!has_valid_rekordbox(dir.path(), 3, "other"));
        // Lives beside a reks entry without touching it.
        write(dir.path(), 3, &sample(), "h").unwrap();
        assert_eq!(load_valid(dir.path(), 3, "h"), Some(sample()));
        assert_eq!(load_rekordbox(dir.path(), 3, "h"), Some(e));
        let json = fs::read_to_string(entry_dir(dir.path(), 3).join("rekordbox.json")).unwrap();
        assert!(json.contains(r#""source": "rekordbox""#), "{json}");
        // A missing file makes it invalid.
        fs::remove_file(entry_dir(dir.path(), 3).join("rekordbox.2EX")).unwrap();
        assert!(!has_valid_rekordbox(dir.path(), 3, "h"));
        assert!(load_rekordbox(dir.path(), 3, "h").is_none());
    }

    #[test]
    fn columns_encoding() {
        let c = sample().columns;
        let b = encode_columns(&c);
        assert_eq!(&b[..4], b"RKWC");
        assert_eq!(&b[4..12], &[1, 0, 0, 0, 3, 0, 0, 0]);
        assert_eq!(&b[12..], &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
        assert_eq!(decode_columns(&b), Some(c));
        assert_eq!(decode_columns(b"RKWC"), None);
    }
}
