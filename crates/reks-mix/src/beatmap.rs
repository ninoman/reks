//! Beat positions from a track's grid: source frame ↔ beat number.
//!
//! A grid is a list of tempo anchors (`beatgrid` rows, converted to source
//! frames by the app layer). Each anchor is a beat; between two anchors the
//! beat number runs linearly, so a dynamic grid's beats land exactly on its
//! anchors and the beat position never jumps. Past the last anchor (and
//! before the first) the anchor's tempo carries on. Sync and quantise work
//! on these beat numbers.

/// One grid anchor: the beat at `frame` is beat `beat_in_bar` of a bar of
/// `beats_per_bar`, at `bpm` until the next anchor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridAnchor {
    /// Source frame of the beat.
    pub frame: i64,
    pub bpm: f64,
    /// 1..=`beats_per_bar`.
    pub beat_in_bar: u8,
    pub beats_per_bar: u8,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Segment {
    frame: f64,
    /// Beat number at `frame`; whole, counted from the first anchor.
    beat: f64,
    /// Source frames per beat up to the next segment.
    period: f64,
    /// Added to a beat number before `rem_euclid(beats_per_bar)` to give the
    /// beat's place in its bar (0 = downbeat).
    bar_offset: f64,
    beats_per_bar: f64,
}

/// A grid ready for lookups. Empty when the track has no usable grid.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BeatMap {
    segs: Box<[Segment]>,
}

impl BeatMap {
    /// Builds the map. Anchors with a non-positive or non-finite tempo are
    /// dropped, and so is an anchor less than half a beat after the one
    /// before it.
    pub fn new(anchors: &[GridAnchor], sample_rate: u32) -> Self {
        let mut sorted: Vec<GridAnchor> = anchors
            .iter()
            .copied()
            .filter(|a| a.bpm.is_finite() && a.bpm > 0.0 && sample_rate > 0)
            .collect();
        sorted.sort_by_key(|a| a.frame);
        let period = |a: &GridAnchor| 60.0 * f64::from(sample_rate) / a.bpm;
        let mut segs: Vec<Segment> = Vec::with_capacity(sorted.len());
        for a in &sorted {
            let frame = a.frame as f64;
            let beat = match segs.last_mut() {
                None => 0.0,
                Some(prev) => {
                    let beats = ((frame - prev.frame) / prev.period).round();
                    if beats < 1.0 {
                        continue;
                    }
                    // Stretch the previous segment to end exactly here.
                    prev.period = (frame - prev.frame) / beats;
                    prev.beat + beats
                }
            };
            let bpb = a.beats_per_bar.max(1);
            segs.push(Segment {
                frame,
                beat,
                period: period(a),
                bar_offset: f64::from(a.beat_in_bar.clamp(1, bpb) - 1) - beat,
                beats_per_bar: f64::from(bpb),
            });
        }
        Self { segs: segs.into() }
    }

    pub fn is_empty(&self) -> bool {
        self.segs.is_empty()
    }

    fn seg_at_frame(&self, frame: f64) -> Option<&Segment> {
        let i = self.segs.partition_point(|s| s.frame <= frame);
        self.segs.get(i.saturating_sub(1))
    }

    fn seg_at_beat(&self, beat: f64) -> Option<&Segment> {
        let i = self.segs.partition_point(|s| s.beat <= beat);
        self.segs.get(i.saturating_sub(1))
    }

    /// Beat number at a source frame (fractional; 0 at the first anchor).
    pub fn beat_at(&self, frame: f64) -> Option<f64> {
        let s = self.seg_at_frame(frame)?;
        Some(s.beat + (frame - s.frame) / s.period)
    }

    /// Source frame of a beat number; the inverse of [`Self::beat_at`].
    pub fn frame_at(&self, beat: f64) -> Option<f64> {
        let s = self.seg_at_beat(beat)?;
        Some(s.frame + (beat - s.beat) * s.period)
    }

    /// Source frames per beat at a frame.
    pub fn period_at(&self, frame: f64) -> Option<f64> {
        self.seg_at_frame(frame).map(|s| s.period)
    }

    /// Place in the bar at a frame, in beats (`0..beats_per_bar`, 0 on the
    /// downbeat), and the beats per bar.
    pub fn bar_at(&self, frame: f64) -> Option<(f64, u8)> {
        let s = self.seg_at_frame(frame)?;
        let beat = s.beat + (frame - s.frame) / s.period;
        Some((
            (beat + s.bar_offset).rem_euclid(s.beats_per_bar),
            s.beats_per_bar as u8,
        ))
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

    fn anchor(frame: i64, bpm: f64, beat_in_bar: u8) -> GridAnchor {
        GridAnchor {
            frame,
            bpm,
            beat_in_bar,
            beats_per_bar: 4,
        }
    }

    #[test]
    fn constant_grid() {
        // 120 BPM: 24,000 frames per beat; the anchor is beat 2 of its bar.
        let m = BeatMap::new(&[anchor(1_000, 120.0, 2)], RATE);
        assert_eq!(m.beat_at(1_000.0), Some(0.0));
        assert_eq!(m.beat_at(13_000.0), Some(0.5));
        assert_eq!(m.beat_at(-23_000.0), Some(-1.0));
        assert_eq!(m.frame_at(3.0), Some(73_000.0));
        assert_eq!(m.period_at(0.0), Some(24_000.0));
        // Beat 2 of the bar at the anchor; the downbeat a beat before it.
        assert_eq!(m.bar_at(1_000.0), Some((1.0, 4)));
        assert_eq!(m.bar_at(-23_000.0), Some((0.0, 4)));
        assert_eq!(m.bar_at(85_000.0), Some((0.5, 4)));
    }

    #[test]
    fn dynamic_grid_runs_through_its_anchors() {
        // 4 beats at 120, then 100 BPM; the second anchor is 1 ms late.
        let m = BeatMap::new(&[anchor(0, 120.0, 1), anchor(96_048, 100.0, 1)], RATE);
        assert_eq!(m.beat_at(96_048.0), Some(4.0));
        assert_eq!(m.frame_at(4.0), Some(96_048.0));
        assert_eq!(m.period_at(10.0), Some(24_012.0));
        assert_eq!(m.period_at(96_048.0), Some(28_800.0));
        assert_eq!(m.frame_at(5.0), Some(96_048.0 + 28_800.0));
        // No jump in the beat number across the anchor.
        let before = m.beat_at(96_047.999).unwrap();
        assert!((before - 4.0).abs() < 1e-6);
        assert_eq!(m.bar_at(96_048.0), Some((0.0, 4)));
    }

    #[test]
    fn bad_anchors_are_dropped() {
        let m = BeatMap::new(
            &[
                anchor(50_000, 120.0, 1),
                anchor(0, f64::NAN, 1),
                anchor(0, 120.0, 1),
                anchor(30_000, 120.0, 1), // 1.25 beats in: kept, 1 beat
                anchor(35_000, 120.0, 1), // 0.2 beats on: dropped
            ],
            RATE,
        );
        assert_eq!(m.beat_at(30_000.0), Some(1.0));
        assert_eq!(m.period_at(1.0), Some(30_000.0));
        assert_eq!(m.beat_at(50_000.0), Some(2.0));
        assert!(BeatMap::new(&[], RATE).is_empty());
        assert_eq!(BeatMap::new(&[], RATE).beat_at(5.0), None);
    }

    #[test]
    fn snap_and_keep_phase() {
        let m = BeatMap::new(&[anchor(0, 120.0, 1)], RATE);
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
