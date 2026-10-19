//! Audio for the preview player, and the overview waveform cache
//! (Phase 1 task 11).
//!
//! The overview is computed whenever a track is decoded and cached as
//! little-endian `f32` `[min, max]` pairs ([`reks_audio::overview`]) in
//! `<cache_dir>/waveform/overview/<key>.f32`. The key hashes the path, size
//! and mtime, so a changed or moved file gets a new entry; stale entries are
//! never read again and are safe to delete.

use std::path::{Path, PathBuf};

use reks_audio::overview::{from_le_bytes, to_le_bytes};
use reks_audio::StereoPcm;
use rusqlite::OptionalExtension;

use crate::artwork::write_atomic;
use crate::db::Library;
use crate::error::{Error, Result};
use crate::scan::TrackId;

/// Decoded audio for one track, ready for the engine.
#[derive(Debug, Clone, PartialEq)]
pub struct PreviewAudio {
    /// Resampled to the rate asked for.
    pub pcm: StereoPcm,
    /// Overview columns at the file's rate.
    pub overview: Vec<f32>,
}

struct TrackFile {
    path: String,
    size: i64,
    mtime: String,
}

impl Library {
    /// Decodes track `id` whole at `sample_rate` and refreshes its cached
    /// overview. A cache write failure is logged, not returned: playback
    /// does not depend on it.
    pub fn preview_audio(&self, id: TrackId, sample_rate: u32) -> Result<PreviewAudio> {
        let file = self.track_file(id)?;
        let decoded = reks_audio::decode_stereo(Path::new(&file.path), Some(sample_rate))?;
        let cache = self.overview_path(&file);
        if let Err(e) = write_atomic(&cache, &to_le_bytes(&decoded.overview)) {
            tracing::warn!(error = %e, "overview cache not written");
        }
        Ok(PreviewAudio {
            pcm: decoded.pcm,
            overview: decoded.overview,
        })
    }

    /// The overview waveform of track `id` as little-endian `f32` bytes,
    /// from the cache or, on a miss, by decoding the file once.
    pub fn overview_waveform(&self, id: TrackId) -> Result<Vec<u8>> {
        let file = self.track_file(id)?;
        let cache = self.overview_path(&file);
        if let Ok(bytes) = std::fs::read(&cache) {
            if from_le_bytes(&bytes).is_some() {
                return Ok(bytes);
            }
        }
        let decoded = reks_audio::decode_stereo(Path::new(&file.path), None)?;
        let bytes = to_le_bytes(&decoded.overview);
        if let Err(e) = write_atomic(&cache, &bytes) {
            tracing::warn!(error = %e, "overview cache not written");
        }
        Ok(bytes)
    }

    fn track_file(&self, id: TrackId) -> Result<TrackFile> {
        let conn = self.conn()?;
        conn.query_row(
            "SELECT path, file_size, file_mtime FROM track WHERE id = ?1",
            [id],
            |r| {
                Ok(TrackFile {
                    path: r.get(0)?,
                    size: r.get(1)?,
                    mtime: r.get(2)?,
                })
            },
        )
        .optional()?
        .ok_or(Error::TrackNotFound(id))
    }

    fn overview_path(&self, file: &TrackFile) -> PathBuf {
        let mut h = blake3::Hasher::new();
        h.update(file.path.as_bytes());
        h.update(&[0]);
        h.update(&file.size.to_le_bytes());
        h.update(file.mtime.as_bytes());
        let key = h.finalize().to_hex();
        self.cache_dir()
            .join("waveform/overview")
            .join(format!("{}.f32", &key[..32]))
    }
}
