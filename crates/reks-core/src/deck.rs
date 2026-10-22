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

impl DeckAudio {
    /// Where auto cue parks a loaded track (task 17): the first memory cue,
    /// else the first beat of the grid, else the start.
    pub fn auto_cue_frame(&self) -> i64 {
        auto_cue_frame(&self.marks, &self.anchors, self.pcm.sample_rate)
    }
}

/// A hot cue (`slot` set) or memory cue (`slot` `None`) to draw on a deck.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CueMark {
    /// The library row ([`Cue::id`]), to rename or delete it.
    pub id: i64,
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
        id: c.id,
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

/// See [`DeckAudio::auto_cue_frame`]. `marks` as [`cue_marks`] gives them
/// (memory cues by position after the hot cues). Before the first anchor
/// the grid runs on at its tempo, as in the engine, so the first beat is
/// the first anchor moved back by whole beats to just after the start.
pub fn auto_cue_frame(marks: &[CueMark], anchors: &[DeckAnchor], rate: u32) -> i64 {
    if let Some(m) = marks.iter().find(|m| m.slot.is_none()) {
        return m.frame.max(0);
    }
    let Some(a) = anchors.iter().min_by_key(|a| a.frame) else {
        return 0;
    };
    if !(a.bpm.is_finite() && a.bpm > 0.0) {
        return 0;
    }
    let period = 60.0 * f64::from(rate) / a.bpm;
    (a.frame as f64).rem_euclid(period).round() as i64
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
            id: 0,
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

    #[test]
    fn auto_cue_parks_on_the_first_memory_cue_else_the_first_beat() {
        let anchor = |frame| DeckAnchor {
            frame,
            bpm: 120.0,
            beat_in_bar: 1,
            beats_per_bar: 4,
        };
        let marks = cue_marks(
            &[
                cue(Some(0), KIND_CUE, 100.0),
                cue(None, KIND_CUE, 3_000.0),
                cue(None, KIND_CUE, 2_000.0),
            ],
            1_000,
        );
        assert_eq!(auto_cue_frame(&marks, &[anchor(1_250)], 1_000), 2_000);
        let hot_only = cue_marks(&[cue(Some(0), KIND_CUE, 100.0)], 1_000);
        // 120 BPM at 1 kHz: a beat is 500 frames, so 1 250 → 250.
        assert_eq!(auto_cue_frame(&hot_only, &[anchor(1_250)], 1_000), 250);
        assert_eq!(auto_cue_frame(&hot_only, &[anchor(40)], 1_000), 40);
        assert_eq!(auto_cue_frame(&[], &[], 1_000), 0, "no grid: the start");
    }
}
