//! Close-zoom deck waveform: signed peaks at [`POINTS_PER_SECOND`], fine
//! enough to show the real oscillating signal at the deck's 2 s, 1 s and
//! 0.5 s zooms, where the 150/s [`crate::waveform`] columns look like blobs.
//!
//! Each point is [`POINT_BYTES`] bytes: `[min, max]` of the mono mix as
//! `i8` (−127..=127 for −1..=1), then `[low, mid, high]` band levels 0..=255
//! from the same Bessel crossovers as the columns. A band peak over one
//! point (1/3000 s) follows the band's own cycle, so each level is the band
//! peak over the ±[`BAND_SPREAD`] points around it: the colour changes with
//! the music, not with every wiggle.
//!
//! Built from a loaded deck's PCM, any range at a time ([`stereo_detail`]),
//! so the app can build the part near the playhead first and a long mix
//! never needs its whole payload at once. Not cached.

use crate::waveform::{Bessel4, WaveformError, LOW_MID_CROSSOVER_HZ, MID_HIGH_CROSSOVER_HZ};

/// Points per second of audio.
pub const POINTS_PER_SECOND: u32 = 3000;
/// Bytes per point: `[min, max, low, mid, high]`.
pub const POINT_BYTES: usize = 5;
/// Points either side whose band peak a point's band level takes (±5 ms).
pub const BAND_SPREAD: u64 = 15;
/// Points built before a range and dropped, so the band filters have
/// settled by its first point (10 ms; the 600 Hz Bessel settles in ~3 ms).
const PRE_ROLL: u64 = 30;

/// First frame of point `k` at `sample_rate`: ⌊k × sample_rate / 3000⌋.
fn boundary(k: u64, sample_rate: u32) -> u64 {
    let v = u128::from(k) * u128::from(sample_rate) / u128::from(POINTS_PER_SECOND);
    u64::try_from(v).unwrap_or(u64::MAX)
}

/// Points for `frames` frames at `sample_rate`: one per started 1/3000 s.
pub fn point_count(frames: u64, sample_rate: u32) -> u64 {
    if sample_rate == 0 {
        return 0;
    }
    let num = u128::from(frames) * u128::from(POINTS_PER_SECOND);
    u64::try_from(num.div_ceil(u128::from(sample_rate))).unwrap_or(u64::MAX)
}

/// Streaming builder over mono samples, from point `first` of a track.
#[derive(Debug, Clone)]
struct DetailBuilder {
    sample_rate: u32,
    low_pass: Bessel4,
    mid_high_pass: Bessel4,
    mid_low_pass: Bessel4,
    high_pass: Bessel4,
    /// The track point the next closed point is.
    point: u64,
    /// Frames into the track so far.
    frame: u64,
    next_boundary: u64,
    /// Open point: signal min and max, band peaks.
    min: f32,
    max: f32,
    band: [f32; 3],
    open: bool,
    peaks: Vec<[i8; 2]>,
    /// Raw band peaks per point, spread in [`finish`](Self::finish).
    bands: Vec<[u8; 3]>,
}

impl DetailBuilder {
    fn new(sample_rate: u32, first: u64) -> Result<Self, WaveformError> {
        if sample_rate == 0 {
            return Err(WaveformError::ZeroSampleRate);
        }
        Ok(Self {
            sample_rate,
            low_pass: Bessel4::new(LOW_MID_CROSSOVER_HZ, sample_rate, false)?,
            mid_high_pass: Bessel4::new(LOW_MID_CROSSOVER_HZ, sample_rate, true)?,
            mid_low_pass: Bessel4::new(MID_HIGH_CROSSOVER_HZ, sample_rate, false)?,
            high_pass: Bessel4::new(MID_HIGH_CROSSOVER_HZ, sample_rate, true)?,
            point: first,
            frame: boundary(first, sample_rate),
            next_boundary: boundary(first + 1, sample_rate),
            min: 0.0,
            max: 0.0,
            band: [0.0; 3],
            open: false,
            peaks: Vec::new(),
            bands: Vec::new(),
        })
    }

    /// The track frame the next sample is.
    fn frame(&self) -> u64 {
        self.frame
    }

    fn push(&mut self, s: f32) {
        let x = f64::from(s);
        let bands = [
            self.low_pass.process(x),
            self.mid_low_pass.process(self.mid_high_pass.process(x)),
            self.high_pass.process(x),
        ];
        for (p, v) in self.band.iter_mut().zip(bands) {
            *p = p.max(v.abs() as f32);
        }
        if self.open {
            self.min = self.min.min(s);
            self.max = self.max.max(s);
        } else {
            (self.min, self.max) = (s, s);
            self.open = true;
        }
        self.frame += 1;
        if self.frame >= self.next_boundary {
            self.close_point();
        }
    }

    fn close_point(&mut self) {
        let signed = |v: f32| (v.clamp(-1.0, 1.0) * 127.0).round() as i8;
        let level = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        self.peaks.push([signed(self.min), signed(self.max)]);
        self.bands.push(self.band.map(level));
        self.band = [0.0; 3];
        self.open = false;
        self.point += 1;
        self.next_boundary = boundary(self.point + 1, self.sample_rate);
    }

    /// Closes a partial last point and returns the payload of every point
    /// built, the band levels spread over the points built.
    fn finish(mut self) -> Vec<u8> {
        if self.open {
            self.close_point();
        }
        let n = self.peaks.len();
        let spread = BAND_SPREAD as usize;
        let mut out = Vec::with_capacity(n * POINT_BYTES);
        for (i, [min, max]) in self.peaks.iter().enumerate() {
            let mut level = [0u8; 3];
            for b in &self.bands[i.saturating_sub(spread)..(i + spread + 1).min(n)] {
                for k in 0..3 {
                    level[k] = level[k].max(b[k]);
                }
            }
            out.extend_from_slice(&[min.to_le_bytes()[0], max.to_le_bytes()[0]]);
            out.extend_from_slice(&level);
        }
        out
    }
}

/// Points `first..first + count` (clamped to the track) of interleaved
/// stereo `pcm` at `sample_rate`, mixed to mono. Built with a lead-in and
/// tail so a range matches the same points of a whole-track build.
pub fn stereo_detail(
    pcm: &[i16],
    sample_rate: u32,
    first: u64,
    count: u64,
) -> Result<Vec<u8>, WaveformError> {
    let frames = (pcm.len() / 2) as u64;
    let total = point_count(frames, sample_rate);
    let end = first.saturating_add(count).min(total);
    if first >= end {
        // Checks the rate even for an empty range.
        DetailBuilder::new(sample_rate, 0)?;
        return Ok(Vec::new());
    }
    let start = first.saturating_sub(BAND_SPREAD + PRE_ROLL);
    let stop = boundary(end.saturating_add(BAND_SPREAD), sample_rate).min(frames);
    let mut b = DetailBuilder::new(sample_rate, start)?;
    while b.frame() < stop {
        // Both bounds are within `pcm`: `stop <= frames`.
        let f = b.frame() as usize;
        let mono = (f32::from(pcm[2 * f]) + f32::from(pcm[2 * f + 1])) / (2.0 * 32768.0);
        b.push(mono);
    }
    let all = b.finish();
    let from = (first - start) as usize * POINT_BYTES;
    let to = (end - start) as usize * POINT_BYTES;
    Ok(all.get(from..to).map(<[u8]>::to_vec).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::TAU;

    const SR: u32 = 48_000;

    /// Stereo `i16` of `offset + amp·sin` on both channels.
    fn sine(freq: f64, amp: f64, offset: f64, seconds: f64, sr: u32) -> Vec<i16> {
        let n = (seconds * f64::from(sr)) as usize;
        (0..n)
            .flat_map(|i| {
                let v = offset + amp * (TAU * freq * i as f64 / f64::from(sr)).sin();
                let s = (v * 32767.0).round() as i16;
                [s, s]
            })
            .collect()
    }

    fn whole(pcm: &[i16], sr: u32) -> Vec<u8> {
        stereo_detail(pcm, sr, 0, u64::MAX).unwrap()
    }

    fn points(payload: &[u8]) -> Vec<(i8, i8, [u8; 3])> {
        payload
            .as_chunks::<POINT_BYTES>()
            .0
            .iter()
            .map(|p| (p[0] as i8, p[1] as i8, [p[2], p[3], p[4]]))
            .collect()
    }

    fn byte(v: f64) -> i8 {
        (v * 127.0).round() as i8
    }

    #[test]
    fn point_count_and_layout() {
        assert_eq!(
            whole(&sine(100.0, 0.5, 0.0, 1.0, SR), SR).len(),
            3000 * POINT_BYTES
        );
        // 44.1 kHz does not divide by 3000: a partial last point counts.
        let pcm = vec![100i16; 2 * 44_101];
        assert_eq!(point_count(44_101, 44_100), 3001);
        assert_eq!(whole(&pcm, 44_100).len() / POINT_BYTES, 3001);
        assert!(whole(&[], SR).is_empty());
        assert_eq!(point_count(10, 0), 0);
    }

    #[test]
    fn slow_sine_traces_its_shape() {
        // 10 Hz at 0.8: each point (16 frames) is a thin slice of the curve.
        let pcm = sine(10.0, 0.8, 0.0, 1.0, SR);
        let pts = points(&whole(&pcm, SR));
        for (i, &(min, max, _)) in pts.iter().enumerate() {
            let frames: Vec<f64> = pcm[i * 32..(i + 1) * 32]
                .iter()
                .step_by(2)
                .map(|&s| f64::from(s) / 32768.0)
                .collect();
            let lo = frames.iter().copied().fold(f64::MAX, f64::min);
            let hi = frames.iter().copied().fold(f64::MIN, f64::max);
            assert!(
                (i32::from(min) - i32::from(byte(lo))).abs() <= 1,
                "point {i}"
            );
            assert!(
                (i32::from(max) - i32::from(byte(hi))).abs() <= 1,
                "point {i}"
            );
            assert!(min <= max);
        }
        // Crest at 1/40 s (point 75), trough at 3/40 s (point 225).
        assert_eq!(pts[75].1, byte(0.8));
        assert_eq!(pts[225].0, -byte(0.8));
        assert!(pts[75].0 > 0, "the crest point stays above the centre");
    }

    #[test]
    fn asymmetric_signal_keeps_its_sign() {
        // A 1 kHz sine riding on +0.3: peaks at +0.8 and −0.2.
        let pts = points(&whole(&sine(1000.0, 0.5, 0.3, 0.5, SR), SR));
        assert_eq!(pts.iter().map(|p| p.1).max(), Some(byte(0.8)));
        assert_eq!(pts.iter().map(|p| p.0).min(), Some(byte(-0.2)));
    }

    #[test]
    fn mixes_channels_to_mono() {
        // Left +0.5, right −0.5 cancel; left 0.6 with silent right halves.
        let cancel: Vec<i16> = (0..4800).flat_map(|_| [16_384i16, -16_384]).collect();
        assert!(points(&whole(&cancel, SR))
            .iter()
            .all(|p| p.0 == 0 && p.1 == 0));
        let half: Vec<i16> = (0..4800).flat_map(|_| [19_661i16, 0]).collect();
        assert!(points(&whole(&half, SR)).iter().all(|p| p.1 == byte(0.3)));
    }

    #[test]
    fn bands_follow_frequency_and_stay_steady() {
        for (freq, band) in [(80.0, 0), (2000.0, 1), (10_000.0, 2)] {
            let pts = points(&whole(&sine(freq, 0.8, 0.0, 1.0, SR), SR));
            // Past filter settling, the spread holds each band level within a
            // few percent from point to point despite the cycle.
            let levels: Vec<[u8; 3]> = pts[600..2900].iter().map(|p| p.2).collect();
            for l in &levels {
                for other in 0..3 {
                    if other != band {
                        assert!(l[band] > 2 * l[other], "{freq} Hz: {l:?}");
                    }
                }
            }
            let lo = levels.iter().map(|l| l[band]).min().unwrap();
            let hi = levels.iter().map(|l| l[band]).max().unwrap();
            assert!(hi - lo < 16, "{freq} Hz band {band} ranges {lo}..{hi}");
        }
    }

    #[test]
    fn a_range_matches_the_whole_build() {
        // A tone burst with a level change, at a rate 3000 does not divide.
        let sr = 44_100;
        let mut pcm = sine(220.0, 0.4, 0.0, 1.0, sr);
        pcm.extend(sine(5000.0, 0.9, 0.0, 1.0, sr));
        let all = points(&whole(&pcm, sr));
        for (first, count) in [(0, 100), (2950, 100), (2999, 1), (5900, 500), (1234, 2345)] {
            let part = points(&stereo_detail(&pcm, sr, first, count).unwrap());
            let end = (first + count).min(all.len() as u64) as usize;
            assert_eq!(part.len(), end - first as usize, "range {first}+{count}");
            for (k, (p, w)) in part.iter().zip(&all[first as usize..end]).enumerate() {
                assert_eq!((p.0, p.1), (w.0, w.1), "point {}", first as usize + k);
                for b in 0..3 {
                    // Filter state differs only by the settled pre-roll.
                    assert!(
                        p.2[b].abs_diff(w.2[b]) <= 2,
                        "point {} band {b}",
                        first as usize + k
                    );
                }
            }
        }
        assert!(stereo_detail(&pcm, sr, 6000, 10).unwrap().is_empty());
        assert!(stereo_detail(&pcm, 0, 0, 10).is_err());
    }
}
