//! Click-track fixtures and onset detection for the render tests.
//!
//! A click is a 1 kHz cosine burst that starts at its peak and decays over a
//! few milliseconds, so its onset is a sharp step that survives resampling.
//! Downbeats are twice as loud. Each fixture puts its clicks on one channel,
//! so two decks can be told apart in the summed output.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use reks_mix::{render, DeckSource, Graph, GridBeat};

/// Downbeat click peak (full scale = 1).
pub const LOUD: f64 = 0.8;
/// Other beats.
pub const SOFT: f64 = 0.4;
/// Onset threshold: under half the soft peak, far above pre-ringing.
const THRESHOLD: f32 = 0.15;
/// Peak level that marks a downbeat.
const LOUD_ABOVE: f32 = 0.6;
/// Click length in ms.
const CLICK_MS: f64 = 15.0;

/// Which channel carries the clicks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

/// A tempo segment of a fixture: `beats` beats at `bpm`.
#[derive(Debug, Clone, Copy)]
pub struct Section {
    pub bpm: f64,
    pub beats: usize,
}

/// A generated click track with its grid.
pub struct Fixture {
    pub src: Arc<DeckSource>,
    /// Exact beat times in source frames.
    pub beats: Vec<f64>,
}

/// Clicks on every beat of `sections` (4/4, first beat a downbeat), starting
/// `lead_ms` into the track; the grid is every beat, as the library's beat
/// rule gives it.
pub fn click_track(rate: u32, side: Side, lead_ms: f64, sections: &[Section]) -> Fixture {
    let fr = f64::from(rate);
    let mut beats = Vec::new();
    let mut grid = Vec::new();
    let mut t = lead_ms * fr / 1000.0;
    for s in sections {
        let period = 60.0 * fr / s.bpm;
        for _ in 0..s.beats {
            let n = beats.len();
            grid.push(GridBeat {
                frame: t.round(),
                bpm: s.bpm,
                beat_in_bar: (n % 4) as u8 + 1,
                beats_per_bar: 4,
            });
            beats.push(t);
            t += period;
        }
    }
    let frames = (t + fr) as usize;
    let mut pcm = vec![0i16; frames * 2];
    let ch = match side {
        Side::Left => 0,
        Side::Right => 1,
    };
    let len = (CLICK_MS * fr / 1000.0) as usize;
    for (n, &at) in beats.iter().enumerate() {
        // Clicks start on whole frames; the grid gets the same rounding.
        let start = at.round() as usize;
        let amp = if n % 4 == 0 { LOUD } else { SOFT };
        for k in 0..len {
            let x = k as f64 / fr;
            let v = amp * (2.0 * std::f64::consts::PI * 1_000.0 * x).cos() * (-x / 0.004).exp();
            if let Some(s) = pcm.get_mut((start + k) * 2 + ch) {
                *s = (v * 32_767.0).round() as i16;
            }
        }
    }
    let beats = beats.iter().map(|b| b.round()).collect();
    Fixture {
        src: Arc::new(
            DeckSource::new(pcm, rate)
                .with_grid(&grid)
                .with_hot_cues(vec![]),
        ),
        beats,
    }
}

/// One detected click: output time in frames (sub-frame) and whether it was
/// a downbeat.
#[derive(Debug, Clone, Copy)]
pub struct Onset {
    pub at: f64,
    pub loud: bool,
}

/// Streams rendered output and collects onsets per channel, so long renders
/// need no full-length buffer.
#[derive(Debug, Default)]
pub struct Onsets {
    pub left: Vec<Onset>,
    pub right: Vec<Onset>,
    frame: u64,
    prev: [f32; 2],
    /// Per channel: frames left in the current click, and its onset index.
    hold: [(u32, usize); 2],
    hold_frames: u32,
}

impl Onsets {
    pub fn new(rate: u32) -> Self {
        Self {
            hold_frames: (f64::from(rate) * CLICK_MS * 2.0 / 1000.0) as u32,
            ..Self::default()
        }
    }

    pub fn feed(&mut self, out: &[f32]) {
        for pair in out.as_chunks::<2>().0 {
            for (ch, &v) in pair.iter().enumerate() {
                let list = if ch == 0 {
                    &mut self.left
                } else {
                    &mut self.right
                };
                let (left, idx) = &mut self.hold[ch];
                if *left > 0 {
                    *left -= 1;
                    if v.abs() > LOUD_ABOVE {
                        list[*idx].loud = true;
                    }
                } else if v.abs() > THRESHOLD {
                    // Linear interpolation between the last two frames.
                    let p = self.prev[ch].abs();
                    let frac = f64::from((THRESHOLD - p) / (v.abs() - p));
                    list.push(Onset {
                        at: self.frame as f64 - 1.0 + frac.clamp(0.0, 1.0),
                        loud: v.abs() > LOUD_ABOVE,
                    });
                    *left = self.hold_frames;
                    *idx = list.len() - 1;
                }
                self.prev[ch] = v;
            }
            self.frame += 1;
        }
    }
}

/// Renders `frames` frames in device-sized blocks, feeding `onsets`.
pub fn run(g: &mut Graph, onsets: &mut Onsets, frames: usize) {
    let mut buf = vec![0.0f32; 512 * 2];
    let mut left = frames;
    while left > 0 {
        let n = left.min(512);
        render(g, &mut buf[..n * 2]);
        onsets.feed(&buf[..n * 2]);
        left -= n;
    }
}

/// Largest gap in ms between each left onset after `from_frame` and the
/// nearest right onset; panics when a left onset has no partner.
pub fn worst_gap_ms(o: &Onsets, rate: u32, from_frame: f64) -> f64 {
    let mut worst: f64 = 0.0;
    let mut checked = 0;
    for l in o.left.iter().filter(|l| l.at >= from_frame) {
        let i = o.right.partition_point(|r| r.at < l.at);
        let near = [i.checked_sub(1), Some(i)]
            .into_iter()
            .flatten()
            .filter_map(|j| o.right.get(j))
            .map(|r| (r.at - l.at).abs())
            .fold(f64::INFINITY, f64::min);
        worst = worst.max(near * 1000.0 / f64::from(rate));
        checked += 1;
    }
    assert!(checked > 0, "no onsets checked");
    worst
}
