//! One track end to end (phase 3 task 11): decode → beats → grid → key →
//! waveform columns, and the conversion of the result into library terms
//! ([`reks_core::AnalysisRecord`]).
//!
//! Pioneer waveform encodings are not made here: they are a pure function of
//! [`WaveformColumns`] (`reks-anlz`), computed at export time from the
//! cached columns.

use std::path::Path;

use reks_core::analysis::file_content_hash;
use reks_core::{AnalysisAnchor, AnalysisRecord, GridAnchor, Library, TrackId};

use crate::beats::{BeatError, BeatList, BeatTracker};
use crate::grid::{self, build_grid_with_pulse, Grid, GridError, GridOptions, TrackedBeat};
use crate::input::{AnalysisInput, InputError, ANALYSIS_RATE};
use crate::key::{detect_key, KeyAnalysis, KeyError};
use crate::onset::{KickOnsets, KickPulse};
use crate::waveform::{columns_from_interleaved, WaveformColumns, WaveformError};

/// Version of the whole analyser. Bump it whenever a change alters results
/// (model, constants, grid rules, waveform filters): every track is then
/// re-analysed and every cache entry is stale.
///
/// 2: MP3 positions on rekordbox's timeline (`reks_audio::timing`, task 4).
/// 3: whole-track tempo fit, dynamic grids only on a tempo change, grid
///    phase from the kicks (task 12a).
/// 4: bar placement by the tracker's downbeat vote and the first kick
///    (`crate::bar`, task 12a).
pub const ANALYSER_VERSION: u32 = 4;

/// A dynamic grid's beats are merged into one tempo anchor while each stays
/// within this distance of where the anchor's rounded tempo puts it.
const ANCHOR_TOLERANCE_MS: f64 = 2.0;

#[derive(Debug, thiserror::Error)]
pub enum AnalyseError {
    #[error(transparent)]
    Input(#[from] InputError),
    #[error(transparent)]
    Beats(#[from] BeatError),
    #[error(transparent)]
    Grid(#[from] GridError),
    #[error(transparent)]
    Key(#[from] KeyError),
    #[error(transparent)]
    Waveform(#[from] WaveformError),
}

/// Everything the analyser found for one track. Positions are samples at
/// [`ANALYSIS_RATE`] from the first decoded sample.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackAnalysis {
    pub duration_ms: u64,
    /// The tracker's raw beats (kept for parity measurement and the editor).
    pub beats: BeatList,
    /// `None` when there are fewer than 2 beats (silence, beatless audio).
    pub grid: Option<Grid>,
    pub key: KeyAnalysis,
    pub columns: WaveformColumns,
}

/// Decodes and analyses `path`. Blocking and CPU-heavy.
pub fn analyse_file(tracker: &mut BeatTracker, path: &Path) -> Result<TrackAnalysis, AnalyseError> {
    let input = AnalysisInput::from_file(path)?;
    analyse_input(tracker, &input)
}

/// Analyses an already conditioned input.
pub fn analyse_input(
    tracker: &mut BeatTracker,
    input: &AnalysisInput,
) -> Result<TrackAnalysis, AnalyseError> {
    let kicks = KickOnsets::from_mono(&input.mono);
    let beats = tracker.track_with(input, &kicks)?;
    analyse_tracked(input, beats, Some(&kicks.pulse()))
}

/// [`analyse_input`] after beat tracking: grid, key and columns for
/// `beats` (placed on the audio, as [`BeatTracker::track`] returns them);
/// `pulse` lines the constant grid up with the kicks
/// ([`grid::kick_phase`]).
pub fn analyse_tracked(
    input: &AnalysisInput,
    beats: BeatList,
    pulse: Option<&KickPulse>,
) -> Result<TrackAnalysis, AnalyseError> {
    let tracked = grid::BeatList {
        sample_rate: beats.sample_rate,
        beats: beats
            .beats
            .iter()
            .map(|b| TrackedBeat {
                position: b.sample,
                downbeat: b.downbeat,
            })
            .collect(),
    };
    let grid = match build_grid_with_pulse(&tracked, pulse, &GridOptions::default()) {
        Ok(g) => Some(g),
        Err(GridError::TooFewBeats(_)) => None,
        Err(e) => return Err(e.into()),
    };
    let key = detect_key(&input.mono, ANALYSIS_RATE)?;
    let columns = columns_from_interleaved(&input.mono, ANALYSIS_RATE, 1)?;
    Ok(TrackAnalysis {
        duration_ms: input.duration_ms(),
        beats,
        grid,
        key,
        columns,
    })
}

impl TrackAnalysis {
    /// The library's view of this result, for `Library::store_analysis`.
    pub fn record(&self, content_hash: String) -> AnalysisRecord {
        let grid = self.grid.as_ref().map(grid_anchors).unwrap_or_default();
        AnalysisRecord {
            version: ANALYSER_VERSION,
            content_hash,
            bpm_x100: self.grid.as_ref().and_then(display_bpm_x100),
            grid,
            key: self.key.key,
            beat_confidence: self.beats.confidence,
            key_confidence: self.key.confidence,
        }
    }
}

fn ms(samples: f64) -> f64 {
    samples * 1000.0 / f64::from(ANALYSIS_RATE)
}

/// Beatgrid positions are stored as whole ms (D10).
fn whole_ms(samples: f64) -> i64 {
    ms(samples).round() as i64
}

fn bpm_x100(beat_len_samples: f64) -> i64 {
    (60.0 * f64::from(ANALYSIS_RATE) / beat_len_samples * 100.0).round() as i64
}

/// The grid of track `id`'s current analysis cache entry as library
/// anchors (4/4); `None` without a current entry or a grid. The grid
/// editor's Reset falls back to it for grids edited before migration 0013.
pub fn cached_grid(lib: &Library, id: TrackId) -> Option<Vec<GridAnchor>> {
    let (path, _) = lib.track_file_key(id).ok()?;
    let hash = file_content_hash(&path).ok()?;
    let analysis = crate::cache::load_valid(&lib.cache_dir(), id, &hash)?;
    let anchors = grid_anchors(analysis.grid.as_ref()?)
        .into_iter()
        .map(|a| GridAnchor {
            position_ms: a.position_ms,
            bpm_x100: a.bpm_x100,
            beat_in_bar: a.beat_in_bar,
            beats_per_bar: 4,
        })
        .collect();
    Some(anchors)
}

/// `beatgrid` rows for `grid`: one for a constant grid; for a dynamic grid a
/// new anchor wherever the beats leave the current anchor's tempo by more
/// than [`ANCHOR_TOLERANCE_MS`].
pub fn grid_anchors(grid: &Grid) -> Vec<AnalysisAnchor> {
    match grid {
        Grid::Constant {
            bpm,
            first_beat,
            first_beat_in_bar,
        } => vec![AnalysisAnchor {
            position_ms: whole_ms(*first_beat as f64),
            bpm_x100: (bpm * 100.0).round() as i64,
            beat_in_bar: *first_beat_in_bar,
        }],
        Grid::Dynamic { beats } => {
            let mut out: Vec<AnalysisAnchor> = Vec::new();
            // (index of the anchor beat, its position, beat length from its rounded tempo)
            let mut anchor: Option<(usize, i64, f64)> = None;
            for (i, b) in beats.iter().enumerate() {
                if let Some((a, pos, len)) = anchor {
                    let predicted = pos as f64 + (i - a) as f64 * len;
                    if ms((predicted - b.position as f64).abs()) <= ANCHOR_TOLERANCE_MS {
                        continue;
                    }
                }
                // Tempo from this beat to the next (the previous gap at the end).
                let gap = match (
                    beats.get(i + 1),
                    i.checked_sub(1).and_then(|p| beats.get(p)),
                ) {
                    (Some(next), _) => next.position - b.position,
                    (None, Some(prev)) => b.position - prev.position,
                    (None, None) => break,
                };
                if gap <= 0 {
                    continue;
                }
                let x100 = bpm_x100(gap as f64);
                out.push(AnalysisAnchor {
                    position_ms: whole_ms(b.position as f64),
                    bpm_x100: x100,
                    beat_in_bar: b.beat_in_bar,
                });
                let len = 60.0 * f64::from(ANALYSIS_RATE) * 100.0 / x100 as f64;
                anchor = Some((i, b.position, len));
            }
            out
        }
    }
}

/// The tempo shown in the library: the constant tempo, or the median beat
/// gap of a dynamic grid.
fn display_bpm_x100(grid: &Grid) -> Option<i64> {
    match grid {
        Grid::Constant { bpm, .. } => Some((bpm * 100.0).round() as i64),
        Grid::Dynamic { beats } => {
            let mut gaps: Vec<i64> = beats
                .windows(2)
                .map(|w| w[1].position - w[0].position)
                .filter(|&g| g > 0)
                .collect();
            if gaps.is_empty() {
                return None;
            }
            gaps.sort_unstable();
            Some(bpm_x100(gaps[gaps.len() / 2] as f64))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::GridBeat;

    #[test]
    fn constant_grid_is_one_anchor() {
        let g = Grid::Constant {
            bpm: 128.0,
            first_beat: 441,
            first_beat_in_bar: 3,
        };
        assert_eq!(
            grid_anchors(&g),
            [AnalysisAnchor {
                position_ms: 10,
                bpm_x100: 12800,
                beat_in_bar: 3
            }]
        );
        assert_eq!(display_bpm_x100(&g), Some(12800));
    }

    #[test]
    fn dynamic_grid_gets_an_anchor_per_tempo() {
        // 16 beats at 120 BPM, then 16 at 125 BPM.
        let mut beats = Vec::new();
        let mut pos = 1000.0f64;
        for i in 0..32u32 {
            beats.push(GridBeat {
                position: pos.round() as i64,
                beat_in_bar: (i % 4 + 1) as u8,
            });
            let bpm = if i < 15 { 120.0 } else { 125.0 };
            pos += 60.0 * 44_100.0 / bpm;
        }
        let anchors = grid_anchors(&Grid::Dynamic {
            beats: beats.clone(),
        });
        assert_eq!(anchors.len(), 2, "{anchors:?}");
        assert_eq!(anchors[0].bpm_x100, 12000);
        assert_eq!(anchors[1].bpm_x100, 12500);
        // Beat 16 is the first one off the 120 BPM line.
        assert_eq!(
            anchors[1].position_ms,
            (beats[16].position as f64 / 44.1).round() as i64
        );
        assert_eq!(anchors[1].beat_in_bar, 1);
        assert_eq!(display_bpm_x100(&Grid::Dynamic { beats }), Some(12500));
        assert!(grid_anchors(&Grid::Dynamic { beats: vec![] }).is_empty());
    }
}
