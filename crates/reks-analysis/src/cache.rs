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

/// Deletes `track`'s entry, if any.
pub fn remove(cache_dir: &Path, track: TrackId) -> Result<(), CacheError> {
    let dir = entry_dir(cache_dir, track);
    match fs::remove_dir_all(&dir) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(io_err(&dir)(e)),
        _ => Ok(()),
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
