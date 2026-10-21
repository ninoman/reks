//! rekordbox's analysis mapped into reks's cache (D10, "rekordbox mapping",
//! maintainer decision 2026-10-21) instead of running reks's analyser on
//! tracks rekordbox already analysed.
//!
//! A pending track qualifies when the library has a rekordbox grid and a key
//! (`Library::mapping_candidates`, from the XML import) and rekordbox's
//! local cache ([`RbCache`], the match the export uses) has its `.DAT`,
//! `.EXT` and `.2EX` with every waveform section
//! ([`reks_anlz::rbwave::read_waveforms`]) for the track's length. Those
//! three files are copied into the track's cache entry
//! (`reks_analysis::cache::write_rekordbox`) and the track is marked
//! analysed (`Library::store_rekordbox_mapping`).
//!
//! What is mapped and what is not:
//! - Grid, BPM, key and cues: already the library's rows from the import
//!   (`source = 'rekordbox'`); nothing to copy.
//! - Waveforms: rekordbox's PWAV/PWV2–PWV7/PWVC bytes, written back
//!   unchanged when reks builds the ANLZ files ([`crate::reksanlz`]).
//! - Confidences: none (rekordbox has none).
//! - The deck and grid-editor waveform: still reks's own columns, computed
//!   on demand from the audio (`reks_analysis::columns`), as before.
//!
//! Everything else stays pending and goes through reks's analyser.

use std::path::Path;

use reks_analysis::cache::{self, RekordboxEntry};
use reks_anlz::rbwave::{read_waveforms, RbWaveError};
use reks_core::analysis::file_content_hash;
use reks_core::{Library, MappingCandidate, TrackId};

use crate::rbcache::{AnlzQuery, Lookup, Missing, RbCache};

/// PWV3 may differ from 150 columns/s of the library's length by this many
/// columns (1 s, the export validator's tolerance).
const LENGTH_TOLERANCE_COLUMNS: u64 = 150;

/// Why a candidate was left to reks's analyser.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Skip {
    #[error("{0}")]
    NotFound(Missing),
    #[error("rekordbox's analysis has no .EXT or .2EX (made by rekordbox 5 or older)")]
    NoExtendedFiles,
    #[error("rekordbox's waveforms: {0}")]
    Waveforms(RbWaveError),
    #[error("the track has no length")]
    NoLength,
    #[error("rekordbox's waveform has {columns} columns, the track is {duration_ms} ms")]
    Length { columns: u64, duration_ms: u64 },
    #[error("{0}")]
    Io(String),
}

/// What [`map_pending`] did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MapReport {
    pub candidates: usize,
    /// Tracks now analysed from rekordbox's data.
    pub mapped: Vec<TrackId>,
    /// Candidates left to reks's analyser, and why.
    pub skipped: Vec<(TrackId, Skip)>,
}

#[derive(Debug, thiserror::Error)]
pub enum MapError {
    #[error(transparent)]
    Library(#[from] reks_core::Error),
    #[error(transparent)]
    Cache(#[from] cache::CacheError),
}

/// Maps every pending track with rekordbox analysis in `rb` into the cache
/// under `lib.cache_dir()`. `version` is the analyser version the pending
/// list is computed for (`reks_analysis::ANALYSER_VERSION`). Per-track
/// problems skip the track; library and cache write errors stop.
pub fn map_pending(lib: &Library, rb: &RbCache, version: u32) -> Result<MapReport, MapError> {
    let candidates = lib.mapping_candidates(version)?;
    let mut report = MapReport {
        candidates: candidates.len(),
        ..MapReport::default()
    };
    let cache_dir = lib.cache_dir();
    for c in &candidates {
        match prepare(rb, c) {
            Ok((entry, hash, from)) => {
                cache::write_rekordbox(&cache_dir, c.track_id, &entry, &hash, &from)?;
                lib.store_rekordbox_mapping(c.track_id, &hash)?;
                report.mapped.push(c.track_id);
            }
            Err(skip) => {
                tracing::debug!(track = c.track_id, %skip, "rekordbox analysis not mapped");
                report.skipped.push((c.track_id, skip));
            }
        }
    }
    tracing::info!(
        candidates = report.candidates,
        mapped = report.mapped.len(),
        "rekordbox analysis mapped"
    );
    Ok(report)
}

/// The cache entry, the file's content hash and rekordbox's `.DAT` path.
fn prepare(
    rb: &RbCache,
    c: &MappingCandidate,
) -> Result<(RekordboxEntry, String, std::path::PathBuf), Skip> {
    let duration_ms = c
        .duration_ms
        .and_then(|d| u64::try_from(d).ok())
        .filter(|&d| d > 0)
        .ok_or(Skip::NoLength)?;
    let query = AnlzQuery {
        path: c.path.clone(),
        total_time_s: u32::try_from(duration_ms / 1000).unwrap_or(u32::MAX),
        first_tempo: Some((c.first_beat_ms as f64 / 1000.0, c.bpm_x100 as f64 / 100.0)),
    };
    let set = match rb.locate(&query) {
        Lookup::Found(set) => set,
        Lookup::Missing(m) => return Err(Skip::NotFound(m)),
    };
    let (Some(ext), Some(two_ex)) = (&set.ext, &set.two_ex) else {
        return Err(Skip::NoExtendedFiles);
    };
    let read = |p: &Path| std::fs::read(p).map_err(|e| Skip::Io(format!("{}: {e}", p.display())));
    let entry = RekordboxEntry {
        duration_ms,
        dat: read(&set.dat)?,
        ext: read(ext)?,
        ex2: read(two_ex)?,
    };
    let waves = read_waveforms(&entry.dat, &entry.ext, &entry.ex2).map_err(Skip::Waveforms)?;
    let columns = waves.pwv3.len() as u64;
    if columns.abs_diff(duration_ms * 150 / 1000) > LENGTH_TOLERANCE_COLUMNS {
        return Err(Skip::Length {
            columns,
            duration_ms,
        });
    }
    let hash = file_content_hash(Path::new(&c.path)).map_err(|e| Skip::Io(e.to_string()))?;
    Ok((entry, hash, set.dat.clone()))
}
