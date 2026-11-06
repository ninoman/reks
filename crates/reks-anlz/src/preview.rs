//! Track-list waveform thumbnails as rekordbox's colour preview (`PWV4`):
//! the 1200-column whole-track view rekordbox draws in its own track list
//! and the players show from the stick.
//!
//! The payload is 4 bytes per column, `[height, low, mid, high]`, each
//! 0..=127 ([`crate::waveform::BAND_BYTE_MAX`]): PWV4 bytes 0, 3, 4 and 5 (the height and the
//! energies of the bottom, middle and top thirds of the spectrum; the UI
//! colours a column from the last three). Always from reks's own columns
//! (its analysis, else the columns cache, else a decode) through the PWV4
//! encoder, never from a mapped rekordbox analysis: the list shows reks's
//! analysis, drawn the way rekordbox draws its preview.

use reks_analysis::cache;
use reks_analysis::columns::{columns_path, decode_columns, track_columns, ColumnsError};
use reks_analysis::waveform::WaveformColumns;
use reks_core::analysis::file_content_hash;
use reks_core::{Library, TrackId};

use crate::waveform::{color_preview, WaveformCalibration};

/// PWV4 bytes kept per column, in payload order.
const KEPT: [usize; 4] = [0, 3, 4, 5];

/// Track `id`'s preview payload. Without `decode`, only from analyses and
/// caches already on disk: empty when none has the track's current file.
/// With it, a miss decodes the file (seconds; call it off the UI thread).
pub fn track_preview(lib: &Library, id: TrackId, decode: bool) -> Result<Vec<u8>, ColumnsError> {
    let (path, key) = lib.track_file_key(id)?;
    let cache_dir = lib.cache_dir();
    // The analysis caches are keyed by content: a few KB read from the file's edges.
    if let Ok(hash) = file_content_hash(&path) {
        if let Some(columns) = cache::load_columns(&cache_dir, id, &hash) {
            return Ok(from_columns(&columns));
        }
    }
    if let Ok(file) = std::fs::read(columns_path(&cache_dir, id)) {
        if let Some(payload) = decode_columns(&key, &file) {
            return Ok(from_interleaved(payload));
        }
    }
    if !decode {
        return Ok(Vec::new());
    }
    Ok(from_interleaved(&track_columns(lib, id)?))
}

/// The payload from reks columns, through the PWV4 encoder.
pub fn from_columns(columns: &WaveformColumns) -> Vec<u8> {
    color_preview(columns, &WaveformCalibration::default())
        .map(|pwv4| pwv4.iter().flat_map(|col| KEPT.map(|k| col[k])).collect())
        .unwrap_or_default()
}

fn from_interleaved(payload: &[u8]) -> Vec<u8> {
    WaveformColumns::from_interleaved(payload)
        .map(|c| from_columns(&c))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::waveform::{Waveforms, COLOR_PREVIEW_COLUMNS};

    fn columns() -> WaveformColumns {
        let n = 3000;
        WaveformColumns {
            low: (0..n).map(|i| (i % 256) as u8).collect(),
            mid: (0..n).map(|i| ((i * 3) % 256) as u8).collect(),
            high: (0..n).map(|i| ((i * 7) % 256) as u8).collect(),
            mono: (0..n).map(|i| (i % 32) as u8).collect(),
        }
    }

    #[test]
    fn columns_give_the_encoders_pwv4_bytes() {
        let c = columns();
        let pwv4 = Waveforms::encode(&c, &WaveformCalibration::default())
            .unwrap()
            .pwv4;
        let got = from_columns(&c);
        assert_eq!(got.len(), COLOR_PREVIEW_COLUMNS * 4);
        for (j, col) in pwv4.iter().enumerate() {
            assert_eq!(got[4 * j..4 * j + 4], [col[0], col[3], col[4], col[5]]);
        }
    }

    #[test]
    fn ragged_columns_give_nothing() {
        let ragged = WaveformColumns {
            low: vec![1],
            ..WaveformColumns::default()
        };
        assert!(from_columns(&ragged).is_empty());
    }
}
