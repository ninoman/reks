//! Beat positions from a track's grid: source frame ↔ beat number.
//!
//! A grid is its list of beats, as the library's beat rule gives them
//! (reks-core `beatgrid::grid_beats`, converted to source frames by the app
//! layer), so a deck follows exactly the beats the grid editor draws and the
//! stick's `PQTZ` holds, tempo sections included (D10, "Grid editing").
//! Between two beats the beat number runs linearly, so it never jumps; before
//! the first beat the first interval carries on, after the last its tempo
//! does. Sync and quantise work on these beat numbers.

/// One beat of a grid: beat `beat_in_bar` of a bar of `beats_per_bar` at
/// `frame`, in a section of `bpm`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridBeat {
    /// Source frame (fractional: beats need not fall on a frame).
    pub frame: f64,
    pub bpm: f64,
    /// 1..=`beats_per_bar`.
    pub beat_in_bar: u8,
    pub beats_per_bar: u8,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Segment {
    frame: f64,
    /// Source frames to the next beat (past the last: one beat of its tempo).
    period: f64,
    /// Place of this beat in its bar, 0 = downbeat.
    in_bar: f64,
    beats_per_bar: f64,
}

/// A grid ready for lookups. Empty when the track has no usable grid.
/// Segment `i` starts at beat number `i`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BeatMap {
    segs: Box<[Segment]>,
}

impl BeatMap {
    /// Builds the map. Beats with a non-finite frame or a non-positive tempo
    /// are dropped, and so is a beat not after the one before it.
    pub fn new(beats: &[GridBeat], sample_rate: u32) -> Self {
        let mut kept: Vec<&GridBeat> = Vec::with_capacity(beats.len());
        for b in beats {
            let usable = b.frame.is_finite() && b.bpm.is_finite() && b.bpm > 0.0;
            if usable && kept.last().is_none_or(|p| b.frame > p.frame) && sample_rate > 0 {
                kept.push(b);
            }
        }
        let segs = kept
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let bpb = b.beats_per_bar.max(1);
                Segment {
                    frame: b.frame,
                    period: kept.get(i + 1).map_or_else(
                        || 60.0 * f64::from(sample_rate) / b.bpm,
                        |next| next.frame - b.frame,
                    ),
                    in_bar: f64::from(b.beat_in_bar.clamp(1, bpb) - 1),
                    beats_per_bar: f64::from(bpb),
                }
            })
            .collect();
        Self { segs }
    }

    pub fn is_empty(&self) -> bool {
        self.segs.is_empty()
    }

    /// The segment holding `frame` and its beat number.
    fn seg_at_frame(&self, frame: f64) -> Option<(f64, &Segment)> {
        let i = self
            .segs
            .partition_point(|s| s.frame <= frame)
            .saturating_sub(1);
        self.segs.get(i).map(|s| (i as f64, s))
    }

    /// Beat number at a source frame (fractional; 0 at the first beat).
    pub fn beat_at(&self, frame: f64) -> Option<f64> {
        let (beat, s) = self.seg_at_frame(frame)?;
        Some(beat + (frame - s.frame) / s.period)
    }

    /// Source frame of a beat number; the inverse of [`Self::beat_at`].
    pub fn frame_at(&self, beat: f64) -> Option<f64> {
        let last = self.segs.len().checked_sub(1)?;
        let i = if beat.is_nan() || beat <= 0.0 {
            0
        } else {
            (beat.floor() as usize).min(last)
        };
        let s = &self.segs[i];
        Some(s.frame + (beat - i as f64) * s.period)
    }

    /// Source frames per beat at a frame.
    pub fn period_at(&self, frame: f64) -> Option<f64> {
        self.seg_at_frame(frame).map(|(_, s)| s.period)
    }

    /// Place in the bar at a frame, in beats (`0..beats_per_bar`, 0 on the
    /// downbeat), and the beats per bar.
    pub fn bar_at(&self, frame: f64) -> Option<(f64, u8)> {
        let (_, s) = self.seg_at_frame(frame)?;
        let in_bar = s.in_bar + (frame - s.frame) / s.period;
        Some((in_bar.rem_euclid(s.beats_per_bar), s.beats_per_bar as u8))
    }

    /// The beat nearest `frame`; `frame` itself without a grid.
    pub fn snap(&self, frame: f64) -> f64 {
        self.beat_at(frame)
            .and_then(|b| self.frame_at(b.round()))
            .unwrap_or(frame)
    }

    /// Where a jump from `from` to `to` should land to keep the beat phase:
    /// `to` moved by under half a beat. `to` itself without a grid.
    pub fn keep_phase(&self, from: f64, to: f64) -> f64 {
        match (self.beat_at(from), self.beat_at(to)) {
            (Some(bf), Some(bt)) => {
                let shift = wrap(bf.rem_euclid(1.0) - bt.rem_euclid(1.0), 1.0);
                self.frame_at(bt + shift).unwrap_or(to)
            }
            _ => to,
        }
    }
}

/// `x` wrapped into `-span/2..span/2`.
pub(crate) fn wrap(x: f64, span: f64) -> f64 {
    (x + span / 2.0).rem_euclid(span) - span / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn beat(frame: f64, bpm: f64, beat_in_bar: u8) -> GridBeat {
        GridBeat {
            frame,
            bpm,
            beat_in_bar,
            beats_per_bar: 4,
        }
    }

    /// `n` beats at `bpm` from `frame`, the first beat `beat_in_bar`.
    fn run(frame: f64, bpm: f64, beat_in_bar: u8, n: usize) -> Vec<GridBeat> {
        let period = 60.0 * f64::from(RATE) / bpm;
        (0..n)
            .map(|k| {
                let in_bar = (usize::from(beat_in_bar) - 1 + k) % 4 + 1;
                beat(frame + k as f64 * period, bpm, in_bar as u8)
            })
            .collect()
    }

    #[test]
    fn constant_grid() {
        // 120 BPM: 24,000 frames per beat; the first beat is beat 2 of its bar.
        let m = BeatMap::new(&run(1_000.0, 120.0, 2, 8), RATE);
        assert_eq!(m.beat_at(1_000.0), Some(0.0));
        assert_eq!(m.beat_at(13_000.0), Some(0.5));
        assert_eq!(m.beat_at(-23_000.0), Some(-1.0));
        assert_eq!(m.frame_at(3.0), Some(73_000.0));
        assert_eq!(m.period_at(0.0), Some(24_000.0));
        // Beat 2 of the bar at the first beat; the downbeat a beat before it.
        assert_eq!(m.bar_at(1_000.0), Some((1.0, 4)));
        assert_eq!(m.bar_at(-23_000.0), Some((0.0, 4)));
        assert_eq!(m.bar_at(85_000.0), Some((0.5, 4)));
        // Past the last beat (frame 169,000) the tempo carries on.
        assert_eq!(m.frame_at(10.0), Some(241_000.0));
        assert_eq!(m.beat_at(241_000.0), Some(10.0));
        // One beat is a constant grid too.
        let one = BeatMap::new(&[beat(1_000.0, 120.0, 2)], RATE);
        assert_eq!(one.frame_at(3.0), Some(73_000.0));
        assert_eq!(one.bar_at(-23_000.0), Some((0.0, 4)));
    }

    #[test]
    fn sections_follow_their_beats() {
        // 120 BPM, then 100 BPM from a marker 1.6 beats after the 4th beat:
        // the rule's last interval (1.6 beats) is one beat here.
        let mut beats = run(0.0, 120.0, 1, 4);
        beats.extend(run(110_400.0, 100.0, 1, 4));
        let m = BeatMap::new(&beats, RATE);
        assert_eq!(m.beat_at(110_400.0), Some(4.0));
        assert_eq!(m.frame_at(4.0), Some(110_400.0));
        assert_eq!(m.period_at(10.0), Some(24_000.0));
        assert_eq!(m.period_at(80_000.0), Some(38_400.0));
        assert_eq!(m.period_at(110_400.0), Some(28_800.0));
        assert_eq!(m.frame_at(5.0), Some(110_400.0 + 28_800.0));
        // No jump in the beat number across the marker.
        let before = m.beat_at(110_399.999).unwrap();
        assert!((before - 4.0).abs() < 1e-6);
        assert_eq!(m.bar_at(110_400.0), Some((0.0, 4)));
        assert_eq!(m.bar_at(72_000.0 + 19_200.0), Some((3.5, 4)));
    }

    #[test]
    fn bad_beats_are_dropped() {
        let m = BeatMap::new(
            &[
                beat(0.0, 120.0, 1),
                beat(f64::NAN, 120.0, 2),
                beat(24_000.0, 0.0, 2),
                beat(24_000.0, 120.0, 2),
                beat(24_000.0, 120.0, 3), // not after the one before
                beat(48_000.0, 120.0, 3),
            ],
            RATE,
        );
        assert_eq!(m.beat_at(48_000.0), Some(2.0));
        assert_eq!(m.bar_at(48_000.0), Some((2.0, 4)));
        assert!(BeatMap::new(&[], RATE).is_empty());
        assert_eq!(BeatMap::new(&[], RATE).beat_at(5.0), None);
        assert_eq!(BeatMap::new(&[], RATE).frame_at(5.0), None);
    }

    #[test]
    fn snap_and_keep_phase() {
        let m = BeatMap::new(&run(0.0, 120.0, 1, 16), RATE);
        assert_eq!(m.snap(13_000.0), 24_000.0);
        assert_eq!(m.snap(11_000.0), 0.0);
        // A quarter beat in, jumping near beat 10: lands a quarter beat past it.
        assert_eq!(m.keep_phase(6_000.0, 240_500.0), 246_000.0);
        assert_eq!(m.keep_phase(6_000.0, 263_000.0), 270_000.0);
        assert_eq!(BeatMap::default().snap(7.0), 7.0);
        assert_eq!(BeatMap::default().keep_phase(1.0, 7.0), 7.0);
    }

    #[test]
    fn wrap_is_centred() {
        assert_eq!(wrap(0.75, 1.0), -0.25);
        assert_eq!(wrap(-0.75, 1.0), 0.25);
        assert_eq!(wrap(3.0, 4.0), -1.0);
        assert_eq!(wrap(0.0, 4.0), 0.0);
    }
}
