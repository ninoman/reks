//! reks's own analysis on the stick (Phase 3 task 10): ANLZ files built with
//! [`reks_anlz::build`] for tracks planned with [`AnlzSource::Reks`].
//!
//! - **Planning** asks a [`ReksLocator`] whether a track has reks analysis:
//!   [`ReksCache`] looks for a current entry in the analysis cache
//!   (`reks_analysis::cache`, same analyser version, same file content
//!   hash). [`ExportOptions::anlz_preference`](crate::plan::ExportOptions)
//!   decides between that and rekordbox's cache.
//! - **Writing**: the writer builds each track's set into its scratch folder
//!   before anything goes to the stick ([`write_all`]) and hands baken the
//!   `.DAT` as if it were rekordbox's (D7). baken then rewrites `PPTH` and
//!   splices the cue lists from the temp XML, as it does for rekordbox's
//!   files; the read-back validator checks the result with
//!   [`reks_anlz::validate_set`].
//!
//! What a built set holds: waveforms encoded from the cached 150/s columns
//! ([`Waveforms::encode`], [`WaveformCalibration::default`] until task 8
//! calibrates it); the library's beatgrid
//! ([`pqtz_beats_from_anchors`], so grid edits and rekordbox grids are what
//! the player shows) and cues; `PVBR` from the MP3's frame count. No `PSSI`,
//! no FLAC seek table (`PVB2`), as with baken's generated files. All of it is
//! UNVERIFIED on players until the Phase 3 hardware check (task 14).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use reks_analysis::cache;
use reks_anlz::grid::{pqtz_beats_from_anchors, GridError};
use reks_anlz::waveform::EncodeError;
use reks_anlz::{AnlzInput, AnlzSet, AnlzTarget, BuildError, WaveformCalibration, Waveforms};
use reks_audio::Kind;
use reks_core::analysis::file_content_hash;
use reks_core::{Library, TrackId};

use crate::plan::{AnlzSource, ExportPlan, PlannedTrack, ReksLocator, SourceTrack};
use crate::TargetProfile;

/// File name of every built set's `.DAT` (`.EXT`/`.2EX` beside it), as
/// rekordbox names its own.
const DAT_NAME: &str = "ANLZ0000.DAT";

/// [`ReksLocator`] over the library's analysis cache.
#[derive(Debug, Clone)]
pub struct ReksCache {
    cache_dir: PathBuf,
}

impl ReksCache {
    /// The cache under `cache_dir` (the library's `cache_dir()`).
    pub fn new(cache_dir: impl Into<PathBuf>) -> Self {
        Self {
            cache_dir: cache_dir.into(),
        }
    }
}

impl ReksLocator for ReksCache {
    fn has_analysis(&self, track: &SourceTrack) -> bool {
        if track.divider.is_some() {
            return false;
        }
        match file_content_hash(&track.src) {
            Ok(hash) => cache::has_valid(&self.cache_dir, track.track_id, &hash),
            Err(err) => {
                tracing::debug!(track = track.track_id, %err, "no content hash");
                false
            }
        }
    }
}

/// The ANLZ files for `target`: the `.2EX` (3-band waveforms) for every
/// profile. [`TargetProfile`] names the *oldest* player, so any stick may
/// also meet a CDJ-3000; rekordbox and baken write `.2EX` on every export
/// and older players ignore it ("harmless elsewhere", phase 3 doc).
/// [`AnlzTarget::Standard`] stays for `reks anlz build --target standard`.
pub fn anlz_target(target: TargetProfile) -> AnlzTarget {
    match target {
        TargetProfile::Cdj2000Nxs
        | TargetProfile::Cdj2000Nxs2
        | TargetProfile::Cdj3000
        | TargetProfile::XdjXz
        | TargetProfile::Generic => AnlzTarget::Cdj3000,
    }
}

/// Bytes a built set takes, from above (for the plan's space check): fixed
/// sections plus 150 columns/s in `PWV3` (1 byte), `PWV5` (2) and `PWV7`
/// (3), and 8-byte beats up to 300 BPM; paths and cues are in the margin.
pub fn estimated_bytes(duration_ms: u64, target: AnlzTarget) -> u64 {
    let secs = duration_ms.div_ceil(1000);
    let dat = (4 << 10) + 40 * secs;
    let ext = (12 << 10) + 450 * secs;
    let ex2 = match target {
        AnlzTarget::Standard => 0,
        AnlzTarget::Cdj3000 => (5 << 10) + 450 * secs,
    };
    dat + ext + ex2
}

#[derive(Debug, thiserror::Error)]
pub enum ReksAnlzError {
    /// The cache entry is gone or no longer matches the file (it changed,
    /// or the analyser did, since planning).
    #[error("reks's analysis is missing or out of date; plan again")]
    Stale,
    #[error(transparent)]
    Library(#[from] reks_core::Error),
    #[error("cannot read the MP3 frames: {0}")]
    Audio(#[from] reks_audio::Error),
    #[error("library beatgrid: {0}")]
    Grid(#[from] GridError),
    #[error("waveforms: {0}")]
    Encode(#[from] EncodeError),
    #[error(transparent)]
    Build(#[from] BuildError),
    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// [`ReksAnlzError`] for one track.
#[derive(Debug, thiserror::Error)]
#[error("reks analysis of track {track_id}: {error}")]
pub struct TrackBuildError {
    pub track_id: TrackId,
    #[source]
    pub error: ReksAnlzError,
}

/// What [`build`] needs from the library and the cache for one track.
#[derive(Debug, Clone)]
pub struct ReksTrack<'a> {
    pub track_id: TrackId,
    pub src: &'a Path,
    /// `/Contents/...` on the stick.
    pub contents_path: &'a str,
    pub kind: Kind,
}

impl<'a> From<&'a PlannedTrack> for ReksTrack<'a> {
    fn from(t: &'a PlannedTrack) -> Self {
        Self {
            track_id: t.track_id,
            src: &t.src,
            contents_path: &t.dst_rel,
            kind: t.kind,
        }
    }
}

/// Builds `track`'s ANLZ set from the analysis cache under `lib.cache_dir()`
/// and the library's grid and cues. The set passed
/// [`reks_anlz::validate_set`] ([`reks_anlz::build`] checks every set).
pub fn build(
    lib: &Library,
    track: &ReksTrack<'_>,
    target: AnlzTarget,
) -> Result<AnlzSet, ReksAnlzError> {
    let hash = file_content_hash(track.src)?;
    let analysis =
        cache::load_valid(&lib.cache_dir(), track.track_id, &hash).ok_or(ReksAnlzError::Stale)?;
    let cues = lib.track(track.track_id)?.cues;
    let anchors = lib.beatgrid(track.track_id)?.anchors;
    // The decoded length: the one the columns, grid and cue checks agree on.
    let duration_ms = analysis.duration_ms;
    let beats = pqtz_beats_from_anchors(&anchors, duration_ms)?;
    let waveforms = Waveforms::encode(&analysis.columns, &WaveformCalibration::default())?;
    let mp3_audio_frames = match track.kind {
        Kind::Mp3 => reks_audio::mp3_audio_frames(track.src)?,
        _ => None,
    };
    let input = AnlzInput {
        contents_path: track.contents_path,
        duration_ms,
        beats: &beats,
        cues: &cues,
        waveforms: &waveforms,
        mp3_audio_frames,
    };
    Ok(reks_anlz::build(&input, target)?)
}

/// Writes `set` into `dir` as `ANLZ0000.DAT/.EXT/.2EX`; returns the `.DAT`.
pub fn write_set(dir: &Path, set: &AnlzSet) -> Result<PathBuf, ReksAnlzError> {
    let io = |path: &Path| {
        let path = path.to_path_buf();
        move |source| ReksAnlzError::Io { path, source }
    };
    std::fs::create_dir_all(dir).map_err(io(dir))?;
    let dat = dir.join(DAT_NAME);
    for (ext, bytes) in set.files() {
        let path = dat.with_extension(ext);
        std::fs::write(&path, bytes).map_err(io(&path))?;
    }
    Ok(dat)
}

/// Builds the set of every [`AnlzSource::Reks`] track of `plan` into
/// `dir/<track_id>/` (a scratch folder, never the stick). Returns each
/// track's `.DAT`; the first failure stops it.
pub fn write_all(
    lib: &Library,
    plan: &ExportPlan,
    dir: &Path,
) -> Result<HashMap<TrackId, PathBuf>, TrackBuildError> {
    let target = anlz_target(plan.options.target);
    let mut out = HashMap::new();
    for t in plan
        .tracks
        .iter()
        .filter(|t| t.anlz_source == AnlzSource::Reks)
    {
        let built = build(lib, &ReksTrack::from(t), target)
            .and_then(|set| write_set(&dir.join(t.track_id.to_string()), &set));
        match built {
            Ok(dat) => {
                out.insert(t.track_id, dat);
            }
            Err(error) => {
                return Err(TrackBuildError {
                    track_id: t.track_id,
                    error,
                })
            }
        }
    }
    tracing::debug!(tracks = out.len(), "built reks ANLZ");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_profile_gets_the_2ex() {
        for t in [
            TargetProfile::Cdj2000Nxs,
            TargetProfile::Cdj2000Nxs2,
            TargetProfile::Cdj3000,
            TargetProfile::XdjXz,
            TargetProfile::Generic,
        ] {
            assert_eq!(anlz_target(t), AnlzTarget::Cdj3000);
        }
    }

    #[test]
    fn estimate_grows_with_length_and_target() {
        let a = estimated_bytes(60_000, AnlzTarget::Standard);
        let b = estimated_bytes(60_000, AnlzTarget::Cdj3000);
        assert!(b > a);
        assert!(estimated_bytes(120_000, AnlzTarget::Cdj3000) > b);
        assert_eq!(estimated_bytes(0, AnlzTarget::Standard), 16 << 10);
    }
}
