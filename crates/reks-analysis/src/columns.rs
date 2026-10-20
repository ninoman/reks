//! Waveform columns of a library track, cached on disk (Phase 3 task 13: the
//! grid editor computes them on demand when no analysis has run yet).
//!
//! Cache file: `<cache_dir>/analysis/<track_id>/columns.bin` =
//! `b"RKWC"`, format version (u32 LE), the track's file key (32 ASCII hex
//! digits, [`Library::track_file_key`]), column count (u32 LE), then the
//! payload: 4 bytes per column, `[low, mid, high, mono]`
//! ([`WaveformColumns::to_interleaved`]). A file whose key or version does
//! not match is recomputed, so a changed file or a new band filter never
//! shows stale columns. The payload is also what the UI receives.

use std::path::{Path, PathBuf};

use reks_audio::{decode_f32, Layout};
use reks_core::{Library, TrackId};

use crate::waveform::{WaveformAnalyzer, WaveformColumns, WaveformError};

const MAGIC: &[u8; 4] = b"RKWC";
/// Bump when the band filters or the payload layout change.
pub const COLUMNS_FORMAT: u32 = 1;
const KEY_LEN: usize = 32;
const HEADER_LEN: usize = 4 + 4 + KEY_LEN + 4;

#[derive(Debug, thiserror::Error)]
pub enum ColumnsError {
    #[error(transparent)]
    Library(#[from] reks_core::Error),
    #[error(transparent)]
    Audio(#[from] reks_audio::Error),
    #[error(transparent)]
    Waveform(#[from] WaveformError),
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

impl WaveformColumns {
    /// 4 bytes per column: `[low, mid, high, mono]`. `None` if the band
    /// vectors differ in length.
    pub fn to_interleaved(&self) -> Option<Vec<u8>> {
        if !self.is_consistent() {
            return None;
        }
        let mut out = Vec::with_capacity(self.len() * 4);
        for i in 0..self.len() {
            out.extend_from_slice(&[self.low[i], self.mid[i], self.high[i], self.mono[i]]);
        }
        Some(out)
    }

    /// Inverse of [`to_interleaved`](Self::to_interleaved); `None` unless the
    /// length is a multiple of 4.
    pub fn from_interleaved(bytes: &[u8]) -> Option<Self> {
        if !bytes.len().is_multiple_of(4) {
            return None;
        }
        let mut c = Self::default();
        for col in bytes.as_chunks::<4>().0 {
            c.low.push(col[0]);
            c.mid.push(col[1]);
            c.high.push(col[2]);
            c.mono.push(col[3]);
        }
        Some(c)
    }
}

/// The cache file for `columns` under `key`.
pub fn encode_columns(key: &str, columns: &WaveformColumns) -> Option<Vec<u8>> {
    let key = key.as_bytes();
    if key.len() != KEY_LEN {
        return None;
    }
    let payload = columns.to_interleaved()?;
    let count = u32::try_from(columns.len()).ok()?;
    let mut out = Vec::with_capacity(HEADER_LEN + payload.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&COLUMNS_FORMAT.to_le_bytes());
    out.extend_from_slice(key);
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&payload);
    Some(out)
}

/// The payload of a cache file, if it is well formed, of the current format
/// and for `key`.
pub fn decode_columns<'a>(key: &str, file: &'a [u8]) -> Option<&'a [u8]> {
    let header = file.get(..HEADER_LEN)?;
    let payload = &file[HEADER_LEN..];
    let u32_at = |at: usize| -> Option<u32> {
        Some(u32::from_le_bytes(header.get(at..at + 4)?.try_into().ok()?))
    };
    let ok = &header[..4] == MAGIC
        && u32_at(4)? == COLUMNS_FORMAT
        && &header[8..8 + KEY_LEN] == key.as_bytes()
        && usize::try_from(u32_at(8 + KEY_LEN)?).ok()? * 4 == payload.len();
    ok.then_some(payload)
}

/// Where track `id`'s columns are cached.
pub fn columns_path(cache_dir: &Path, id: TrackId) -> PathBuf {
    cache_dir
        .join("analysis")
        .join(id.to_string())
        .join("columns.bin")
}

/// Columns of track `id` as the interleaved payload, from the cache or, on a
/// miss, by decoding the file (blocking, seconds for a long track; call it
/// off the UI thread). A failed cache write is logged, not returned.
pub fn track_columns(lib: &Library, id: TrackId) -> Result<Vec<u8>, ColumnsError> {
    let (path, key) = lib.track_file_key(id)?;
    let cache = columns_path(&lib.cache_dir(), id);
    if let Ok(file) = std::fs::read(&cache) {
        if let Some(payload) = decode_columns(&key, &file) {
            return Ok(payload.to_vec());
        }
    }
    let columns = file_columns(&path)?;
    if let Some(file) = encode_columns(&key, &columns) {
        if let Err(e) = write_atomic(&cache, &file) {
            tracing::warn!(error = %e, "columns cache not written");
        }
    }
    Ok(columns.to_interleaved().unwrap_or_default())
}

/// Columns of the audio file at `path` (decoded whole, mono, at its own rate;
/// blocking). No cache.
pub fn file_columns(path: &Path) -> Result<WaveformColumns, ColumnsError> {
    let pcm = decode_f32(path, Layout::Mono)?;
    let mut analyzer = WaveformAnalyzer::new(pcm.sample_rate, 1)?;
    analyzer.push_interleaved(&pcm.samples);
    Ok(analyzer.finish())
}

fn write_atomic(path: &Path, data: &[u8]) -> Result<(), ColumnsError> {
    let io = |source| ColumnsError::Io {
        path: path.to_path_buf(),
        source,
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(io)?;
    }
    let tmp = path.with_extension("bin.tmp");
    std::fs::write(&tmp, data).map_err(io)?;
    std::fs::rename(&tmp, path).map_err(io)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "0123456789abcdef0123456789abcdef";

    fn cols() -> WaveformColumns {
        WaveformColumns {
            low: vec![1, 2, 3],
            mid: vec![4, 5, 6],
            high: vec![7, 8, 9],
            mono: vec![10, 11, 12],
        }
    }

    #[test]
    fn interleaved_round_trip() {
        let bytes = cols().to_interleaved().unwrap();
        assert_eq!(bytes, [1, 4, 7, 10, 2, 5, 8, 11, 3, 6, 9, 12]);
        assert_eq!(WaveformColumns::from_interleaved(&bytes).unwrap(), cols());
        assert!(WaveformColumns::from_interleaved(&[1, 2, 3]).is_none());
        let mut bad = cols();
        bad.mono.pop();
        assert!(bad.to_interleaved().is_none());
    }

    #[test]
    fn cache_file_round_trip_and_rejects() {
        let file = encode_columns(KEY, &cols()).unwrap();
        assert_eq!(file.len(), HEADER_LEN + 12);
        assert_eq!(&file[..4], b"RKWC");
        assert_eq!(&file[8 + KEY_LEN..HEADER_LEN], &3u32.to_le_bytes());
        assert_eq!(
            decode_columns(KEY, &file).unwrap(),
            &cols().to_interleaved().unwrap()[..]
        );
        // Another file, another format, truncated.
        assert!(decode_columns("ffffffffffffffffffffffffffffffff", &file).is_none());
        let mut old = file.clone();
        old[4] = 0;
        assert!(decode_columns(KEY, &old).is_none());
        assert!(decode_columns(KEY, &file[..file.len() - 1]).is_none());
        assert!(decode_columns(KEY, &file[..10]).is_none());
        assert!(encode_columns("short", &cols()).is_none());
    }
}
