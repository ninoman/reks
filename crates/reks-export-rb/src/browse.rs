//! Browsing a plugged-in stick (the sidebar's Devices section): its tracks,
//! playlists and play history as players see them, and its tracks ready for
//! a deck. Read-only: nothing here writes to the stick.
//!
//! The library comes from `export.pdb` ([`crate::pdbread`] for tracks and
//! playlists, [`crate::history::read_play_history`] for the sessions players
//! logged). A track plays straight from the stick with the grid and cues of
//! its ANLZ files ([`anlz`]), as a player would. Its deck waveform comes from
//! the stick's `PWV7`/`PWV3` turned back into deck columns
//! ([`reks_anlz::deckwave`]), or from the audio when the stick has no `.2EX`.

pub mod anlz;

use std::path::{Component, Path, PathBuf};

use reks_analysis::waveform::WaveformColumns;
use reks_anlz::WaveformCalibration;
use reks_audio::StereoPcm;
use reks_core::beatgrid::beat_marks;
use reks_core::deck::{cue_marks, deck_beats, hot_cue_frames};
use reks_core::DataSource;
use reks_core::{Beatgrid, DeckAudio, Key};
use serde::Serialize;

use crate::history::{read_play_history, HistoryError, HistoryReadOptions, HistorySource};
use crate::pdbread::{read_stick, PdbLibrary, PdbReadError, PdbTrack};
use crate::validate::locate;
use anlz::StickAnalysis;

/// What the stick's `export.pdb` holds, for display.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StickLibrary {
    /// In table order.
    pub tracks: Vec<StickTrack>,
    /// Folders and playlists, siblings in the order players show them.
    pub playlists: Vec<StickPlaylist>,
    /// The sessions players logged, oldest first.
    pub histories: Vec<StickSession>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StickTrack {
    /// The `export.pdb` row id.
    pub id: u32,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    pub bpm_x100: Option<i64>,
    /// Canonical name ([`Key::canonical`]); `None` when unset or unreadable.
    pub key: Option<String>,
    /// From the row's whole seconds; `None` when 0.
    pub duration_ms: Option<i64>,
    /// As in the PDB: `/`-separated from the stick root.
    pub stick_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StickPlaylist {
    pub id: u32,
    /// `None` at the root.
    pub parent_id: Option<u32>,
    pub name: String,
    pub is_folder: bool,
    /// In entry order; ids of [`StickLibrary::tracks`].
    pub track_ids: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StickSession {
    /// The history playlist's row id.
    pub id: u32,
    /// `HISTORY 001`, ...; `HISTORY #<id>` when its playlist row is missing.
    pub name: String,
    /// In player order. Ids may name tracks the PDB no longer has.
    pub track_ids: Vec<u32>,
}

#[derive(Debug, thiserror::Error)]
pub enum BrowseError {
    #[error(transparent)]
    Pdb(#[from] PdbReadError),
    #[error(transparent)]
    History(#[from] HistoryError),
    #[error("the stick has no track {0}")]
    NoTrack(u32),
    #[error("the stick's library names a path outside the stick: {0}")]
    OutsideStick(String),
    #[error("{} is not on the stick", root.join(rel.trim_start_matches('/')).display())]
    FileMissing { root: PathBuf, rel: String },
    #[error("cannot read {}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    Audio(#[from] reks_audio::Error),
    #[error(transparent)]
    Columns(#[from] reks_analysis::columns::ColumnsError),
}

/// The library on the stick at `root`; `None` when it has no `export.pdb`.
pub fn read_library(root: &Path) -> Result<Option<StickLibrary>, BrowseError> {
    let pdb = match read_stick(root) {
        Ok(pdb) => pdb,
        Err(PdbReadError::Missing { .. }) => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let history = read_play_history(
        &HistorySource::Stick(root.to_owned()),
        &HistoryReadOptions::default(),
    )?;
    let histories = history
        .sessions
        .iter()
        .map(|s| StickSession {
            id: s.id,
            name: s
                .name
                .clone()
                .unwrap_or_else(|| format!("HISTORY #{}", s.id)),
            track_ids: s.entries.iter().map(|e| e.track_id).collect(),
        })
        .collect();
    Ok(Some(StickLibrary {
        tracks: pdb.tracks.iter().map(|t| display_track(&pdb, t)).collect(),
        playlists: playlists(&pdb),
        histories,
    }))
}

fn display_track(pdb: &PdbLibrary, t: &PdbTrack) -> StickTrack {
    let name = |id: u32, map: &std::collections::BTreeMap<u32, String>| {
        map.get(&id).filter(|s| !s.is_empty()).cloned()
    };
    StickTrack {
        id: t.id,
        title: t.title.clone(),
        artist: name(t.artist_id, &pdb.artists),
        album: pdb
            .albums
            .get(&t.album_id)
            .map(|a| a.name.clone())
            .filter(|s| !s.is_empty()),
        genre: name(t.genre_id, &pdb.genres),
        bpm_x100: (t.tempo > 0).then_some(i64::from(t.tempo)),
        key: pdb
            .keys
            .get(&t.key_id)
            .and_then(|k| Key::parse(k).ok())
            .map(|k| k.canonical().to_owned()),
        duration_ms: (t.duration > 0).then_some(i64::from(t.duration) * 1000),
        stick_path: t.file_path.clone(),
    }
}

/// Playlists with siblings by `sort_order` (then id), parents before children.
fn playlists(pdb: &PdbLibrary) -> Vec<StickPlaylist> {
    let mut nodes: Vec<_> = pdb.playlists.iter().collect();
    nodes.sort_by_key(|p| (p.parent_id, p.sort_order, p.id));
    nodes
        .into_iter()
        .map(|p| StickPlaylist {
            id: p.id,
            parent_id: (p.parent_id != 0).then_some(p.parent_id),
            name: p.name.clone(),
            is_folder: p.is_folder,
            track_ids: p.entries.clone(),
        })
        .collect()
}

/// Track `id` of the stick at `root` decoded at `sample_rate`, with the grid
/// and cues of its ANLZ files (none when it has none). Blocking: decodes the
/// whole file.
pub fn deck_audio(root: &Path, id: u32, sample_rate: u32) -> Result<DeckAudio, BrowseError> {
    let track = stick_track(root, id)?;
    let audio = stick_file(root, &track.file_path)?;
    let pcm: StereoPcm = reks_audio::decode_stereo(&audio, Some(sample_rate))?.pcm;
    let analysis = read_analysis(root, &track)?;
    Ok(DeckAudio {
        beats: deck_beats(&analysis.anchors, sample_rate, pcm.frames()),
        pcm,
        hot_cues: hot_cue_frames(&analysis.cues, sample_rate),
        marks: cue_marks(&analysis.cues, sample_rate),
    })
}

/// Track `id`'s grid from its ANLZ files, as the grid views take it. The
/// stick is not the library: `source` is `stick` and it counts as locked.
pub fn beatgrid(root: &Path, id: u32) -> Result<Beatgrid, BrowseError> {
    let track = stick_track(root, id)?;
    let analysis = read_analysis(root, &track)?;
    let end_ms = (analysis.last_beat_ms + 1).max(i64::from(track.duration) * 1000);
    Ok(Beatgrid {
        beats: beat_marks(&analysis.anchors, end_ms),
        anchors: analysis.anchors,
        source: Some(DataSource::Stick),
        locked: true,
    })
}

/// Track `id`'s waveform columns (the payload of `waveform_columns`): from
/// the stick's `PWV7`/`PWV3` when it has them ([`reks_anlz::deckwave`],
/// fast), else computed from the audio (blocking: decodes the whole file).
pub fn waveform_columns(root: &Path, id: u32) -> Result<Vec<u8>, BrowseError> {
    let track = stick_track(root, id)?;
    if let Some(columns) = stick_columns(root, &track)? {
        return Ok(columns.to_interleaved().unwrap_or_default());
    }
    let audio = stick_file(root, &track.file_path)?;
    let columns = reks_analysis::columns::file_columns(&audio)?;
    Ok(columns.to_interleaved().unwrap_or_default())
}

/// Columns decoded from the track's `.2EX` and `.EXT`; `None` without a
/// `.2EX` or when it does not decode (logged).
fn stick_columns(root: &Path, track: &PdbTrack) -> Result<Option<WaveformColumns>, BrowseError> {
    if track.analyze_path.is_empty() {
        return Ok(None);
    }
    let Some(ex2) = read_optional(root, &sibling(&track.analyze_path, "2EX"))? else {
        return Ok(None);
    };
    let ext = read_optional(root, &sibling(&track.analyze_path, "EXT"))?.unwrap_or_default();
    match reks_anlz::deckwave::columns(&ext, &ex2, &WaveformCalibration::default()) {
        Ok(columns) if !columns.is_empty() => Ok(Some(columns)),
        Ok(_) => Ok(None),
        Err(e) => {
            tracing::warn!(track = track.id, error = %e, "stick waveform unreadable; computing it");
            Ok(None)
        }
    }
}

fn stick_track(root: &Path, id: u32) -> Result<PdbTrack, BrowseError> {
    read_stick(root)?
        .tracks
        .into_iter()
        .find(|t| t.id == id)
        .ok_or(BrowseError::NoTrack(id))
}

/// The file at the stick path `rel`, matched case-insensitively as FAT32
/// does. `..` is refused: a PDB is data from a foreign stick.
fn stick_file(root: &Path, rel: &str) -> Result<PathBuf, BrowseError> {
    if Path::new(rel)
        .components()
        .any(|c| matches!(c, Component::ParentDir))
    {
        return Err(BrowseError::OutsideStick(rel.to_owned()));
    }
    locate(root, rel)
        .map_err(|source| BrowseError::Io {
            path: root.join(rel.trim_start_matches('/')),
            source,
        })?
        .ok_or_else(|| BrowseError::FileMissing {
            root: root.to_owned(),
            rel: rel.to_owned(),
        })
}

/// The `.DAT` named by the track row and the `.EXT` beside it. A track with
/// no ANLZ path or files plays without a grid or cues; damaged files are
/// logged and treated the same way, so the track still plays.
fn read_analysis(root: &Path, track: &PdbTrack) -> Result<StickAnalysis, BrowseError> {
    if track.analyze_path.is_empty() {
        return Ok(StickAnalysis::default());
    }
    let Some(dat) = read_optional(root, &track.analyze_path)? else {
        return Ok(StickAnalysis::default());
    };
    let ext = read_optional(root, &sibling(&track.analyze_path, "EXT"))?;
    Ok(anlz::analysis(&dat, ext.as_deref()).unwrap_or_else(|e| {
        tracing::warn!(track = track.id, error = %e, "stick ANLZ unreadable; no grid or cues");
        StickAnalysis::default()
    }))
}

/// The bytes of the stick file `rel`; `None` when it is not there.
fn read_optional(root: &Path, rel: &str) -> Result<Option<Vec<u8>>, BrowseError> {
    match stick_file(root, rel) {
        Ok(path) => std::fs::read(&path)
            .map(Some)
            .map_err(|source| BrowseError::Io { path, source }),
        Err(BrowseError::FileMissing { .. }) => Ok(None),
        Err(e) => Err(e),
    }
}

/// `…/ANLZ0000.DAT` → `…/ANLZ0000.<ext>`.
fn sibling(dat: &str, ext: &str) -> String {
    match dat.rsplit_once('.') {
        Some((stem, _)) => format!("{stem}.{ext}"),
        None => format!("{dat}.{ext}"),
    }
}

#[cfg(test)]
mod tests;
