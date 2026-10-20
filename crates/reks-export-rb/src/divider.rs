//! Playlist dividers on a legacy stick (D22, Phase 2.5 task 4).
//!
//! With [`ExportOptions::dividers`](crate::plan::ExportOptions::dividers) on,
//! [`expand`] turns each divider of the exported playlists into a planned
//! track: a silent WAV generated here ([`wav_bytes`]) in the cache dir, with
//! a synthetic track id ([`DIVIDER_ID_TOP`] downwards) and the label as
//! title. From there it goes through the planner like any track (probe,
//! `audiocheck`, stick path, reuse, stale files), its ANLZ is always
//! generated, the temp XML gets a row for it
//! ([`reks_core::rbxml::DividerTrack`]) and the manifest marks it
//! `item_kind = 'divider'`.
//!
//! A divider's file is `div-<playlist id>-<n>.wav`, `n` counting the
//! playlist's dividers from 1 in entry order. Every file has the same bytes,
//! so a file is written only when it is missing or differs: its time stays,
//! and the stick copy of an unchanged divider is reused. Removing a divider
//! renumbers the ones after it, so the playlist's last file goes stale (its
//! WAV and ANLZ are deleted) and the others keep theirs.
//!
//! Whether players load such a file, and how they show the label, is
//! **UNVERIFIED** until the Phase 2.5 hardware session (task 11); the option
//! is off by default until then.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use reks_audio::write::{wav_header, PcmSpec};
use reks_core::{PlaylistId, TrackId};
use serde::{Deserialize, Serialize};

use crate::plan::{PlanError, SourceTrack, Sources};

/// Length of a divider's silence (DIV1: 1 s against 4 s is open).
pub const DIVIDER_SECONDS: u32 = 4;
/// PCM 16-bit, 44.1 kHz, stereo: what every player loads.
pub const DIVIDER_SPEC: PcmSpec = PcmSpec {
    sample_rate: 44_100,
    channels: 2,
    bits_per_sample: 16,
};
/// Synthetic track ids count down from here (rekordbox's 32-bit `TrackID`),
/// far above any library id; [`expand`] refuses a collision.
pub const DIVIDER_ID_TOP: TrackId = u32::MAX as TrackId;
/// Folder under the cache dir that holds the generated files.
pub const DIVIDER_DIR: &str = "dividers";

/// What makes a planned track a divider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DividerInfo {
    pub playlist_id: PlaylistId,
    /// 1-based among the playlist's dividers, in entry order.
    pub ordinal: u32,
    pub label: String,
}

/// A divider of an exported playlist, as the library holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceDivider {
    pub info: DividerInfo,
    /// Track entries before it in the playlist (its index into the
    /// playlist's `track_ids` as loaded).
    pub tracks_before: usize,
}

/// The bytes of every divider file: a 44-byte header and
/// [`DIVIDER_SECONDS`] of digital silence.
pub fn wav_bytes() -> Vec<u8> {
    let frames = DIVIDER_SPEC.sample_rate * DIVIDER_SECONDS;
    let data_len = frames * u32::from(DIVIDER_SPEC.block_align());
    let mut out = wav_header(&DIVIDER_SPEC, data_len).to_vec();
    out.resize(out.len() + data_len as usize, 0);
    out
}

/// `div-<playlist id>-<ordinal>.wav`.
pub fn file_name(playlist_id: PlaylistId, ordinal: u32) -> String {
    format!("div-{playlist_id}-{ordinal}.wav")
}

/// Makes sure `dir/<file_name>` holds [`wav_bytes`], writing it (through a
/// temp file and a rename) only when it is missing or differs.
pub fn ensure_wav(dir: &Path, playlist_id: PlaylistId, ordinal: u32) -> std::io::Result<PathBuf> {
    let path = dir.join(file_name(playlist_id, ordinal));
    let bytes = wav_bytes();
    if fs::read(&path).is_ok_and(|have| have == bytes) {
        return Ok(path);
    }
    fs::create_dir_all(dir)?;
    let tmp = path.with_extension("wav.tmp");
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(&bytes)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, &path)?;
    tracing::debug!(path = %path.display(), "divider file written");
    Ok(path)
}

/// Adds `sources.dividers` to the plan input: a WAV in `dir` and a
/// [`SourceTrack`] each, spliced into their playlists' entries. Tracks are
/// put back in order of first appearance.
pub fn expand(sources: &mut Sources, dir: &Path) -> Result<(), PlanError> {
    let dividers = std::mem::take(&mut sources.dividers);
    if dividers.is_empty() {
        return Ok(());
    }
    let library_ids: HashSet<TrackId> = sources.tracks.iter().map(|t| t.track_id).collect();
    let mut tracks: HashMap<TrackId, SourceTrack> = std::mem::take(&mut sources.tracks)
        .into_iter()
        .map(|t| (t.track_id, t))
        .collect();
    let mut splices: HashMap<PlaylistId, Vec<(usize, TrackId)>> = HashMap::new();
    for (i, d) in dividers.into_iter().enumerate() {
        let id = TrackId::try_from(i)
            .ok()
            .and_then(|i| DIVIDER_ID_TOP.checked_sub(i))
            .filter(|id| *id > 0 && !library_ids.contains(id))
            .ok_or(PlanError::DividerIds)?;
        let src = ensure_wav(dir, d.info.playlist_id, d.info.ordinal).map_err(|source| {
            PlanError::DividerFile {
                path: dir.join(file_name(d.info.playlist_id, d.info.ordinal)),
                source,
            }
        })?;
        splices
            .entry(d.info.playlist_id)
            .or_default()
            .push((d.tracks_before, id));
        tracks.insert(
            id,
            SourceTrack {
                track_id: id,
                src,
                artist: None,
                album: None,
                probe: None,
                has_grid: false,
                grid_user_owned: false,
                cue_count: 0,
                divider: Some(d.info),
            },
        );
    }
    for p in &mut sources.playlists {
        let Some(mut at) = splices.remove(&p.id) else {
            continue;
        };
        // Back to front, so earlier indices stay valid; stable for ties.
        at.sort_by_key(|&(before, _)| before);
        for &(before, id) in at.iter().rev() {
            p.track_ids.insert(before.min(p.track_ids.len()), id);
        }
    }
    let mut seen = HashSet::new();
    for p in &sources.playlists {
        for id in &p.track_ids {
            if seen.insert(*id) {
                if let Some(t) = tracks.remove(id) {
                    sources.tracks.push(t);
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audiocheck;
    use crate::TargetProfile;
    use reks_audio::Kind;

    #[test]
    fn wav_is_pcm16_stereo_44k_silence() {
        let b = wav_bytes();
        let frames = 44_100 * DIVIDER_SECONDS as usize;
        assert_eq!(b.len(), 44 + frames * 4);
        assert_eq!(&b[0..4], b"RIFF");
        assert_eq!(&b[8..16], b"WAVEfmt ");
        assert_eq!(u16::from_le_bytes([b[20], b[21]]), 1, "WAVE_FORMAT_PCM");
        assert_eq!(u16::from_le_bytes([b[22], b[23]]), 2);
        assert_eq!(u32::from_le_bytes(b[24..28].try_into().unwrap()), 44_100);
        assert_eq!(u16::from_le_bytes([b[34], b[35]]), 16);
        assert!(b[44..].iter().all(|&x| x == 0));
    }

    #[test]
    fn wav_probes_decodes_and_passes_audiocheck() {
        let dir = tempfile::tempdir().unwrap();
        let path = ensure_wav(dir.path(), 7, 1).unwrap();
        assert_eq!(path.file_name().unwrap(), "div-7-1.wav");
        let probe = reks_audio::probe(&path).unwrap();
        assert_eq!(probe.kind, Kind::Wav);
        assert_eq!(probe.sample_rate, 44_100);
        assert_eq!(probe.channels, 2);
        assert_eq!(probe.bit_depth, Some(16));
        assert_eq!(probe.duration_ms, u64::from(DIVIDER_SECONDS) * 1000);
        for target in [
            TargetProfile::Cdj2000Nxs,
            TargetProfile::Cdj2000Nxs2,
            TargetProfile::Cdj3000,
            TargetProfile::XdjXz,
            TargetProfile::Generic,
        ] {
            let issues = audiocheck::check(&path, &probe, target).unwrap();
            assert!(issues.is_empty(), "{target:?}: {issues:?}");
        }
        let decoded = reks_audio::decode_stereo(&path, None).unwrap();
        assert_eq!(decoded.pcm.frames(), 44_100 * DIVIDER_SECONDS as usize);
        assert!(decoded.pcm.samples.iter().all(|&s| s == 0));
    }

    #[test]
    fn ensure_wav_keeps_an_identical_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = ensure_wav(dir.path(), 1, 2).unwrap();
        let old = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
        fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(old)
            .unwrap();
        ensure_wav(dir.path(), 1, 2).unwrap();
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), old);
        // A damaged file is rewritten.
        fs::write(&path, b"RIFF").unwrap();
        ensure_wav(dir.path(), 1, 2).unwrap();
        assert_eq!(fs::read(&path).unwrap(), wav_bytes());
    }
}
