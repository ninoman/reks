//! A library track ready for a mixer deck (Phase 5 task 8): decoded audio at
//! the engine rate plus its grid anchors and hot cues in frames of that audio.
//! The app layer turns this into a `reks_mix::DeckSource` field by field, so
//! this crate stays free of the audio engine.

use reks_audio::StereoPcm;

use crate::beatgrid::GridAnchor;
use crate::db::Library;
use crate::error::Result;
use crate::scan::TrackId;
use crate::track::Cue;

/// rekordbox cue `Type` of a plain cue and of a loop ([`Cue::kind`]).
const KIND_CUE: i64 = 0;
const KIND_LOOP: i64 = 4;

/// A track decoded for a deck.
#[derive(Debug)]
pub struct DeckAudio {
    /// Resampled to the rate asked for.
    pub pcm: StereoPcm,
    /// Grid anchors in frames of `pcm`, sorted; empty without a grid.
    pub anchors: Vec<DeckAnchor>,
    /// `(slot, frame)` of each hot cue (slot 0..7 = A..H), sorted by slot.
    pub hot_cues: Vec<(u8, i64)>,
}

/// One `beatgrid` row in frames of the deck's audio.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeckAnchor {
    pub frame: i64,
    pub bpm: f64,
    /// 1..=`beats_per_bar`.
    pub beat_in_bar: u8,
    pub beats_per_bar: u8,
}

impl Library {
    /// Decodes track `id` whole at `sample_rate` with its grid and hot cues.
    /// Runs for seconds on a long track: call it off the UI and audio threads.
    pub fn deck_audio(&self, id: TrackId, sample_rate: u32) -> Result<DeckAudio> {
        let anchors = deck_anchors(&self.beatgrid(id)?.anchors, sample_rate);
        let hot_cues = hot_cue_frames(&self.track(id)?.cues, sample_rate);
        let pcm = self.preview_audio(id, sample_rate)?.pcm;
        Ok(DeckAudio {
            pcm,
            anchors,
            hot_cues,
        })
    }
}

/// Milliseconds to the nearest frame at `rate`.
pub fn ms_to_frame(ms: f64, rate: u32) -> i64 {
    (ms * f64::from(rate) / 1000.0).round() as i64
}

/// Grid rows in frames at `rate`.
pub fn deck_anchors(anchors: &[GridAnchor], rate: u32) -> Vec<DeckAnchor> {
    anchors
        .iter()
        .map(|a| DeckAnchor {
            frame: ms_to_frame(a.position_ms as f64, rate),
            bpm: a.bpm_x100 as f64 / 100.0,
            beat_in_bar: a.beat_in_bar,
            beats_per_bar: a.beats_per_bar,
        })
        .collect()
}

/// Hot cues (and hot loops, by their start) in frames at `rate`; memory cues
/// and fade or load markers are left out.
pub fn hot_cue_frames(cues: &[Cue], rate: u32) -> Vec<(u8, i64)> {
    let mut out: Vec<(u8, i64)> = cues
        .iter()
        .filter(|c| matches!(c.kind, KIND_CUE | KIND_LOOP))
        .filter_map(|c| Some((c.slot?, ms_to_frame(c.start_ms, rate))))
        .collect();
    out.sort_by_key(|c| c.0);
    out.dedup_by_key(|c| c.0);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cue(slot: Option<u8>, kind: i64, start_ms: f64) -> Cue {
        Cue {
            id: 0,
            slot,
            kind,
            start_ms,
            end_ms: None,
            name: None,
            color_rgb: None,
        }
    }

    #[test]
    fn anchors_convert_to_frames_at_the_rate() {
        let rows = [
            GridAnchor {
                position_ms: 12,
                bpm_x100: 12_800,
                beat_in_bar: 1,
                beats_per_bar: 4,
            },
            GridAnchor {
                position_ms: 60_000,
                bpm_x100: 12_550,
                beat_in_bar: 3,
                beats_per_bar: 4,
            },
        ];
        assert_eq!(
            deck_anchors(&rows, 48_000),
            [
                DeckAnchor {
                    frame: 576,
                    bpm: 128.0,
                    beat_in_bar: 1,
                    beats_per_bar: 4
                },
                DeckAnchor {
                    frame: 2_880_000,
                    bpm: 125.5,
                    beat_in_bar: 3,
                    beats_per_bar: 4
                },
            ]
        );
    }

    #[test]
    fn hot_cues_keep_slotted_cues_and_loops_sorted_by_slot() {
        let cues = [
            cue(Some(2), KIND_LOOP, 2_000.0),
            cue(None, KIND_CUE, 500.0),
            cue(Some(0), KIND_CUE, 1_000.5),
            cue(Some(1), 3, 1_500.0),
            cue(Some(0), KIND_CUE, 9_000.0),
        ];
        assert_eq!(
            hot_cue_frames(&cues, 44_100),
            [(0, 44_122), (2, 88_200)],
            "memory and load cues dropped, a repeated slot keeps the first"
        );
    }
}
