//! `PQTZ` beats from a reks beatgrid ([`reks_analysis::grid::Grid`]).
//!
//! Expansion rule (checked against both rekordbox 6 reference tracks, see
//! `tests/golden.rs`): one entry per beat from the grid's first beat while
//! the beat lies before the end of the track; `time` = the beat's position
//! rounded to the nearest ms, `tempo` = round(BPM × 100), `beat_number`
//! cycles 1–4. A constant grid starts at its `first_beat` (the grid builder
//! already extrapolates back to the earliest beat ≥ 0; rekordbox's reference
//! grids also start within one beat of 0). Whether rekordbox ever emits beats
//! before the first detected one is UNVERIFIED (phase 3 doc, PQTZ rule).
//!
//! Exports take the library's grid instead ([`pqtz_beats_from_anchors`]):
//! the same rule over `beatgrid` anchors.

use reks_analysis::grid::Grid;
use reks_core::beatgrid::GridAnchor;

use crate::write::PqtzBeat;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum GridError {
    #[error("sample rate is zero")]
    ZeroSampleRate,
    #[error("grid tempo {0} BPM is outside 1..=655.35")]
    BadTempo(f64),
    #[error("beat {index} is not after the previous one")]
    UnorderedBeats { index: usize },
}

/// Round(BPM × 100) as a `PQTZ` tempo.
fn tempo_x100(bpm: f64) -> Result<u16, GridError> {
    let t = (bpm * 100.0).round();
    if bpm.is_finite() && (100.0..=f64::from(u16::MAX)).contains(&t) {
        Ok(t as u16)
    } else {
        Err(GridError::BadTempo(bpm))
    }
}

/// Milliseconds, rounded; negative positions clamp to 0.
fn ms(seconds: f64) -> u32 {
    (seconds * 1000.0).round().clamp(0.0, f64::from(u32::MAX)) as u32
}

fn next_number(n: u16) -> u16 {
    n % 4 + 1
}

/// The `PQTZ` beats of `grid` (positions in samples at `sample_rate`) for a
/// track `duration_ms` long.
///
/// Constant grid: beats at `first_beat + i × 60 / bpm` seconds while that
/// time (unrounded) is below the duration. Dynamic grid: the grid's beats
/// before the duration; each beat's tempo is the one implied by the gap to
/// the next beat (the last beat repeats the previous gap's tempo). A
/// dynamic grid with a single beat has no tempo and is an error.
pub fn pqtz_beats(
    grid: &Grid,
    sample_rate: u32,
    duration_ms: u64,
) -> Result<Vec<PqtzBeat>, GridError> {
    if sample_rate == 0 {
        return Err(GridError::ZeroSampleRate);
    }
    let sr = f64::from(sample_rate);
    let end_s = duration_ms as f64 / 1000.0;
    match grid {
        Grid::Constant {
            bpm,
            first_beat,
            first_beat_in_bar,
        } => {
            let tempo = tempo_x100(*bpm)?;
            let period = 60.0 / bpm;
            let start = *first_beat as f64 / sr;
            let mut number = u16::from((*first_beat_in_bar).clamp(1, 4));
            let mut out = Vec::new();
            for i in 0u32.. {
                let t = start + f64::from(i) * period;
                if t >= end_s {
                    break;
                }
                out.push(PqtzBeat {
                    beat_number: number,
                    tempo,
                    time_ms: ms(t),
                });
                number = next_number(number);
            }
            Ok(out)
        }
        Grid::Dynamic { beats } => {
            for (index, pair) in beats.windows(2).enumerate() {
                if pair[1].position <= pair[0].position {
                    return Err(GridError::UnorderedBeats { index: index + 1 });
                }
            }
            let kept: Vec<_> = beats
                .iter()
                .filter(|b| (b.position as f64 / sr) < end_s)
                .collect();
            let gap_tempo = |i: usize| -> Result<u16, GridError> {
                // Gap to the next beat in the full grid, or the previous gap
                // for the very last beat.
                let (a, b) = match (beats.get(i), beats.get(i + 1)) {
                    (Some(a), Some(b)) => (a, b),
                    _ if i >= 1 => (&beats[i - 1], &beats[i]),
                    _ => return Err(GridError::BadTempo(0.0)),
                };
                let secs = (b.position - a.position) as f64 / sr;
                tempo_x100(60.0 / secs)
            };
            kept.iter()
                .enumerate()
                .map(|(i, b)| {
                    Ok(PqtzBeat {
                        beat_number: u16::from(b.beat_in_bar.clamp(1, 4)),
                        tempo: gap_tempo(i)?,
                        time_ms: ms(b.position as f64 / sr),
                    })
                })
                .collect()
        }
    }
}

/// The `PQTZ` beats of a library grid (`beatgrid` rows, whole ms, D10) for a
/// track `duration_ms` long: what an export writes, so grid edits (task 13)
/// and rekordbox's imported grids reach the stick as the library holds them.
///
/// Each anchor's beats run at its tempo from its position up to the next
/// anchor (exclusive) or the end of the track (exclusive, as in
/// [`pqtz_beats`]); `time` = the unrounded position rounded to the nearest ms,
/// `tempo` = the anchor's `bpm_x100`, `beat_number` counts from the anchor's
/// `beat_in_bar` in bars of `beats_per_bar` (capped at 4, PQTZ's range).
/// Beats before 0 ms are left out, and a beat that would round onto the
/// previous one is dropped, so times strictly increase.
pub fn pqtz_beats_from_anchors(
    anchors: &[GridAnchor],
    duration_ms: u64,
) -> Result<Vec<PqtzBeat>, GridError> {
    let end = duration_ms as f64;
    let mut out: Vec<PqtzBeat> = Vec::new();
    for (i, a) in anchors.iter().enumerate() {
        if let Some(prev) = i.checked_sub(1).map(|p| &anchors[p]) {
            if a.position_ms <= prev.position_ms {
                return Err(GridError::UnorderedBeats { index: i });
            }
        }
        let tempo = u16::try_from(a.bpm_x100)
            .ok()
            .filter(|&t| t >= 100)
            .ok_or(GridError::BadTempo(a.bpm_x100 as f64 / 100.0))?;
        let period = 6_000_000.0 / f64::from(tempo);
        let stop = anchors
            .get(i + 1)
            .map_or(end, |next| (next.position_ms as f64).min(end));
        let bpb = i64::from(a.beats_per_bar.max(1));
        let start = a.position_ms as f64;
        // First beat at or after 0 ms.
        let first = if start < 0.0 {
            (-start / period).ceil() as i64
        } else {
            0
        };
        for n in first.. {
            let t = start + n as f64 * period;
            if t >= stop {
                break;
            }
            let time_ms = ms(t / 1000.0);
            if out.last().is_some_and(|b| b.time_ms >= time_ms) {
                continue;
            }
            let in_bar = (i64::from(a.beat_in_bar.max(1)) - 1 + n).rem_euclid(bpb) + 1;
            out.push(PqtzBeat {
                beat_number: in_bar.clamp(1, 4) as u16,
                tempo,
                time_ms,
            });
        }
    }
    Ok(out)
}

/// The tempo (BPM) in effect at `time_ms`: that of the last beat at or before
/// it, else the first beat's; `None` without beats.
pub fn bpm_at(beats: &[PqtzBeat], time_ms: u32) -> Option<f64> {
    let i = beats.partition_point(|b| b.time_ms <= time_ms);
    beats
        .get(i.saturating_sub(1))
        .map(|b| f64::from(b.tempo) / 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use reks_analysis::grid::GridBeat;

    #[test]
    fn constant_grid_runs_to_the_duration() {
        // 120 BPM at 44.1 kHz from 0.25 s, beat 3; 2.25 s long.
        let g = Grid::Constant {
            bpm: 120.0,
            first_beat: 11_025,
            first_beat_in_bar: 3,
        };
        let b = pqtz_beats(&g, 44_100, 2_250).unwrap();
        let got: Vec<_> = b
            .iter()
            .map(|b| (b.beat_number, b.tempo, b.time_ms))
            .collect();
        assert_eq!(
            got,
            [
                (3, 12_000, 250),
                (4, 12_000, 750),
                (1, 12_000, 1_250),
                (2, 12_000, 1_750)
            ]
        );
        // A beat exactly at the duration is left out.
        let g0 = Grid::Constant {
            bpm: 120.0,
            first_beat: 0,
            first_beat_in_bar: 1,
        };
        assert_eq!(pqtz_beats(&g0, 44_100, 1_000).unwrap().len(), 2);
        assert!(pqtz_beats(&g0, 44_100, 0).unwrap().is_empty());
    }

    #[test]
    fn fractional_tempo_rounds_and_does_not_drift() {
        // 128 BPM = 468.75 ms; 12 minutes: no accumulated rounding error.
        let g = Grid::Constant {
            bpm: 128.0,
            first_beat: 0,
            first_beat_in_bar: 1,
        };
        let b = pqtz_beats(&g, 44_100, 720_000).unwrap();
        assert_eq!(b.len(), 1536);
        assert_eq!(b[1].time_ms, 469); // 468.75
        assert_eq!(b[2].time_ms, 938); // 937.5 rounds half away from zero
        assert_eq!(b[1535].time_ms, 719_531); // 1535 × 468.75 = 719531.25
        let g = Grid::Constant {
            bpm: 174.555,
            first_beat: 0,
            first_beat_in_bar: 1,
        };
        assert_eq!(pqtz_beats(&g, 44_100, 1_000).unwrap()[0].tempo, 17_456);
    }

    #[test]
    fn dynamic_grid_takes_tempo_from_gaps() {
        let beats = [0, 22_050, 44_100, 66_150, 86_100]
            .iter()
            .enumerate()
            .map(|(i, &p)| GridBeat {
                position: p,
                beat_in_bar: (i % 4) as u8 + 1,
            })
            .collect();
        let g = Grid::Dynamic { beats };
        let b = pqtz_beats(&g, 44_100, 10_000).unwrap();
        let t: Vec<_> = b
            .iter()
            .map(|b| (b.beat_number, b.tempo, b.time_ms))
            .collect();
        // 0.5 s gaps → 120 BPM; the last gap 19950 samples → 132.63 BPM,
        // repeated on the last beat.
        assert_eq!(
            t,
            [
                (1, 12_000, 0),
                (2, 12_000, 500),
                (3, 12_000, 1_000),
                (4, 13_263, 1_500),
                (1, 13_263, 1_952)
            ]
        );
        // Truncated at the duration, tempo still from the full grid.
        let b = pqtz_beats(&g, 44_100, 1_600).unwrap();
        assert_eq!(b.len(), 4);
        assert_eq!(b[3].tempo, 13_263);
    }

    #[test]
    fn bad_grids_are_errors() {
        let g = Grid::Constant {
            bpm: 0.0,
            first_beat: 0,
            first_beat_in_bar: 1,
        };
        assert!(matches!(
            pqtz_beats(&g, 44_100, 1_000),
            Err(GridError::BadTempo(_))
        ));
        assert_eq!(
            pqtz_beats(&g, 0, 1_000).unwrap_err(),
            GridError::ZeroSampleRate
        );
        let g = Grid::Dynamic {
            beats: vec![
                GridBeat {
                    position: 10,
                    beat_in_bar: 1,
                },
                GridBeat {
                    position: 10,
                    beat_in_bar: 2,
                },
            ],
        };
        assert_eq!(
            pqtz_beats(&g, 44_100, 1_000).unwrap_err(),
            GridError::UnorderedBeats { index: 1 }
        );
        let lone = Grid::Dynamic {
            beats: vec![GridBeat {
                position: 0,
                beat_in_bar: 1,
            }],
        };
        assert!(pqtz_beats(&lone, 44_100, 1_000).is_err());
        let empty = Grid::Dynamic { beats: vec![] };
        assert!(pqtz_beats(&empty, 44_100, 1_000).unwrap().is_empty());
    }

    fn anchor(position_ms: i64, bpm_x100: i64, beat_in_bar: u8) -> GridAnchor {
        GridAnchor {
            position_ms,
            bpm_x100,
            beat_in_bar,
            beats_per_bar: 4,
        }
    }

    #[test]
    fn library_grid_constant_and_anchored() {
        let triple = |b: Vec<PqtzBeat>| -> Vec<_> {
            b.iter()
                .map(|b| (b.beat_number, b.tempo, b.time_ms))
                .collect()
        };
        // 128 BPM from 100 ms, beat 4: 468.75 ms apart, rounded, no drift.
        let b = pqtz_beats_from_anchors(&[anchor(100, 12_800, 4)], 1_600).unwrap();
        assert_eq!(
            triple(b),
            [
                (4, 12_800, 100),
                (1, 12_800, 569),
                (2, 12_800, 1_038),
                (3, 12_800, 1_506)
            ]
        );
        let long = pqtz_beats_from_anchors(&[anchor(0, 12_800, 1)], 720_000).unwrap();
        assert_eq!(long.len(), 1536);
        assert_eq!(long[1535].time_ms, 719_531);

        // A tempo change: beats of the first anchor stop before the second.
        let b = pqtz_beats_from_anchors(&[anchor(0, 12_000, 1), anchor(1_200, 6_000, 3)], 3_300)
            .unwrap();
        assert_eq!(
            triple(b),
            [
                (1, 12_000, 0),
                (2, 12_000, 500),
                (3, 12_000, 1_000),
                (3, 6_000, 1_200),
                (4, 6_000, 2_200),
                (1, 6_000, 3_200)
            ]
        );

        // An anchor before 0 (nudged): beats from the first at or after 0,
        // numbered as if counted from the anchor.
        let b = pqtz_beats_from_anchors(&[anchor(-300, 12_000, 1)], 1_000).unwrap();
        assert_eq!(triple(b), [(2, 12_000, 200), (3, 12_000, 700)]);

        // 3/4 bars count 1–3.
        let waltz = GridAnchor {
            beats_per_bar: 3,
            ..anchor(0, 12_000, 3)
        };
        let n: Vec<_> = pqtz_beats_from_anchors(&[waltz], 2_000)
            .unwrap()
            .iter()
            .map(|b| b.beat_number)
            .collect();
        assert_eq!(n, [3, 1, 2, 3]);

        assert!(pqtz_beats_from_anchors(&[], 1_000).unwrap().is_empty());
        assert_eq!(
            pqtz_beats_from_anchors(&[anchor(0, 0, 1)], 1_000).unwrap_err(),
            GridError::BadTempo(0.0)
        );
        assert_eq!(
            pqtz_beats_from_anchors(&[anchor(10, 12_000, 1), anchor(10, 12_000, 1)], 1_000)
                .unwrap_err(),
            GridError::UnorderedBeats { index: 1 }
        );
    }

    #[test]
    fn tempo_lookup() {
        let b = |t, ms| PqtzBeat {
            beat_number: 1,
            tempo: t,
            time_ms: ms,
        };
        let beats = [b(12_000, 100), b(13_000, 600)];
        assert_eq!(bpm_at(&beats, 0), Some(120.0));
        assert_eq!(bpm_at(&beats, 599), Some(120.0));
        assert_eq!(bpm_at(&beats, 600), Some(130.0));
        assert_eq!(bpm_at(&[], 600), None);
    }
}
