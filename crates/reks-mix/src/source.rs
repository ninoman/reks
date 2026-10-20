//! A track ready for a deck: decoded PCM, its grid and hot cues.
//!
//! Built off the audio thread by the app layer; the graph only reads it.

use std::sync::Arc;

use crate::beatmap::{BeatMap, GridAnchor};

/// Interleaved stereo `i16` at the file's rate (or the engine's, when the
/// caller resampled at load), with the beat grid and hot cues in its frames.
pub struct DeckSource {
    pcm: Arc<[i16]>,
    sample_rate: u32,
    beats: BeatMap,
    /// `(slot, frame)`, sorted by slot.
    hot_cues: Box<[(u8, i64)]>,
}

impl DeckSource {
    /// `samples` is `L R L R …`; a trailing odd sample is dropped.
    pub fn new(mut samples: Vec<i16>, sample_rate: u32) -> Self {
        samples.truncate(samples.len() & !1);
        Self {
            pcm: samples.into(),
            sample_rate,
            beats: BeatMap::default(),
            hot_cues: Box::default(),
        }
    }

    /// Adds the grid (`beatgrid` rows in this source's frames).
    pub fn with_grid(mut self, anchors: &[GridAnchor]) -> Self {
        self.beats = BeatMap::new(anchors, self.sample_rate);
        self
    }

    /// Adds hot cues as `(slot, frame)`; a repeated slot keeps the first.
    pub fn with_hot_cues(mut self, mut cues: Vec<(u8, i64)>) -> Self {
        cues.sort_by_key(|c| c.0);
        cues.dedup_by_key(|c| c.0);
        self.hot_cues = cues.into();
        self
    }

    /// Number of stereo frames.
    pub fn frames(&self) -> i64 {
        (self.pcm.len() / 2) as i64
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn beats(&self) -> &BeatMap {
        &self.beats
    }

    /// The frame of hot cue `slot`, if set.
    pub fn hot_cue(&self, slot: u8) -> Option<i64> {
        self.hot_cues
            .binary_search_by_key(&slot, |c| c.0)
            .ok()
            .map(|i| self.hot_cues[i].1)
    }

    /// One stereo frame as `f32`, silence past either end.
    pub(crate) fn frame(&self, frame: i64) -> (f32, f32) {
        if !(0..self.frames()).contains(&frame) {
            return (0.0, 0.0);
        }
        let i = (frame * 2) as usize;
        (to_f32(self.pcm[i]), to_f32(self.pcm[i + 1]))
    }

    /// The frame at fractional time `t`, linearly interpolated.
    pub(crate) fn frame_lerp(&self, t: f64) -> (f32, f32) {
        let i = t.floor();
        let x = (t - i) as f32;
        let (l0, r0) = self.frame(i as i64);
        if x == 0.0 {
            return (l0, r0);
        }
        let (l1, r1) = self.frame(i as i64 + 1);
        (l0 + (l1 - l0) * x, r0 + (r1 - r0) * x)
    }

    /// Fills interleaved `out` from frame `start` on, silence past either end.
    pub(crate) fn read_f32(&self, start: i64, out: &mut [f32]) {
        for (k, pair) in out.as_chunks_mut::<2>().0.iter_mut().enumerate() {
            let (l, r) = self.frame(start + k as i64);
            *pair = [l, r];
        }
    }
}

fn to_f32(s: i16) -> f32 {
    f32::from(s) / 32_768.0
}

impl std::fmt::Debug for DeckSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeckSource")
            .field("frames", &self.frames())
            .field("sample_rate", &self.sample_rate)
            .field("hot_cues", &self.hot_cues.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_frames_and_silence_outside() {
        let s = DeckSource::new(vec![100, -100, 200, -200, 7], 44_100);
        assert_eq!(s.frames(), 2);
        assert_eq!(s.frame(1), (200.0 / 32_768.0, -200.0 / 32_768.0));
        assert_eq!(s.frame(-1), (0.0, 0.0));
        assert_eq!(s.frame(2), (0.0, 0.0));
        let (l, _) = s.frame_lerp(0.25);
        assert!((l - 125.0 / 32_768.0).abs() < 1e-9);
        let mut out = [1.0; 6];
        s.read_f32(1, &mut out);
        assert_eq!(out[0] * 32_768.0, 200.0);
        assert_eq!(&out[2..], [0.0; 4]);
    }

    #[test]
    fn hot_cues_by_slot() {
        let s = DeckSource::new(vec![], 48_000).with_hot_cues(vec![(3, 30), (1, 10), (3, 99)]);
        assert_eq!(s.hot_cue(1), Some(10));
        assert_eq!(s.hot_cue(3), Some(30));
        assert_eq!(s.hot_cue(2), None);
    }
}
