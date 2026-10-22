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
    /// What the deck's waveforms mark: the hot cues with their colours, then
    /// the memory cues by position.
    pub marks: Vec<CueMark>,
}

/// A hot cue (`slot` set) or memory cue (`slot` `None`) to draw on a deck.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CueMark {
    pub slot: Option<u8>,
    pub frame: i64,
    /// 0xRRGGBB from the library; `None` uses the deck's default colour.
    pub color_rgb: Option<u32>,
    /// The cue's name from the library (shown on its hot cue pad).
    pub name: Option<String>,
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
        let cues = self.track(id)?.cues;
        let hot_cues = hot_cue_frames(&cues, sample_rate);
        let marks = cue_marks(&cues, sample_rate);
        let pcm = self.preview_audio(id, sample_rate)?.pcm;
        Ok(DeckAudio {
            pcm,
            anchors,
            hot_cues,
            marks,
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
    hot_cues(cues)
        .iter()
        .filter_map(|c| Some((c.slot?, ms_to_frame(c.start_ms, rate))))
        .collect()
}

/// Hot cues sorted by slot, the first of a repeated slot only.
fn hot_cues(cues: &[Cue]) -> Vec<&Cue> {
    let mut out: Vec<&Cue> = cues
        .iter()
        .filter(|c| c.slot.is_some() && matches!(c.kind, KIND_CUE | KIND_LOOP))
        .collect();
    out.sort_by_key(|c| c.slot);
    out.dedup_by_key(|c| c.slot);
    out
}

/// The hot cues (as [`hot_cue_frames`], with colour) and then the memory
/// cues and loops (no slot) by position, in frames at `rate`.
pub fn cue_marks(cues: &[Cue], rate: u32) -> Vec<CueMark> {
    let mark = |c: &Cue| CueMark {
        slot: c.slot,
        frame: ms_to_frame(c.start_ms, rate),
        color_rgb: c.color_rgb,
        name: c.name.clone(),
    };
    let mut memory: Vec<CueMark> = cues
        .iter()
        .filter(|c| c.slot.is_none() && matches!(c.kind, KIND_CUE | KIND_LOOP))
        .map(mark)
        .collect();
    memory.sort_by_key(|m| m.frame);
    let mut out: Vec<CueMark> = hot_cues(cues).into_iter().map(mark).collect();
    out.extend(memory);
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

    #[test]
    fn cue_marks_list_hot_cues_with_colour_then_memory_cues_by_position() {
        let mut red = cue(Some(1), KIND_CUE, 1_000.0);
        red.color_rgb = Some(0xff_00_00);
        red.name = Some("Drop".into());
        let cues = [
            cue(None, KIND_CUE, 3_000.0),
            red,
            cue(None, KIND_LOOP, 2_000.0),
            cue(None, 3, 500.0),
            cue(Some(0), KIND_LOOP, 4_000.0),
        ];
        let mark = |slot, frame, color_rgb, name: Option<&str>| CueMark {
            slot,
            frame,
            color_rgb,
            name: name.map(String::from),
        };
        assert_eq!(
            cue_marks(&cues, 1_000),
            [
                mark(Some(0), 4_000, None, None),
                mark(Some(1), 1_000, Some(0xff_00_00), Some("Drop")),
                mark(None, 2_000, None, None),
                mark(None, 3_000, None, None),
            ],
            "load cue dropped"
        );
    }
}
