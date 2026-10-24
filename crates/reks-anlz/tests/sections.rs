//! Edited grids with tempo sections through `PQTZ` (D10, "Grid editing"):
//! a full built set holds the library beat rule's beats, our reader and
//! rekordcrate read them back, and the anchors come back from the beats.

#![allow(clippy::unwrap_used, clippy::panic)]

mod common;

use std::io::Cursor;

use binrw::BinRead;
use proptest::prelude::*;
use rekordcrate::anlz::{Content, ANLZ};
use reks_analysis::waveform::WaveformColumns;
use reks_anlz::grid::{anchors_from_pqtz, pqtz_beats_from_anchors};
use reks_anlz::{build, AnlzInput, AnlzTarget, PqtzBeat, Waveforms, CALIBRATION};
use reks_core::beatgrid::{grid_beats, validate_anchors};
use reks_core::GridAnchor;

const PATH: &str = "/Contents/Artist/Album/Track.wav";

fn waves(duration_ms: u64) -> Waveforms {
    let n = duration_ms.saturating_mul(150).div_ceil(1000) as usize;
    let c = WaveformColumns {
        low: vec![80; n],
        mid: vec![60; n],
        high: vec![40; n],
        mono: vec![20; n],
    };
    Waveforms::encode(&c, &CALIBRATION).unwrap()
}

fn anchor(position_ms: i64, bpm_x100: i64, beat_in_bar: u8) -> GridAnchor {
    GridAnchor {
        position_ms,
        bpm_x100,
        beat_in_bar,
        beats_per_bar: 4,
    }
}

/// Builds a full set for `anchors` and returns the `.DAT`'s beats as our
/// reader and rekordcrate see them (they must agree).
fn built_beats(anchors: &[GridAnchor], duration_ms: u64) -> Vec<PqtzBeat> {
    let beats = pqtz_beats_from_anchors(anchors, duration_ms).unwrap();
    let w = waves(duration_ms);
    let input = AnlzInput {
        contents_path: PATH,
        duration_ms,
        beats: &beats,
        cues: &[],
        waveforms: &w,
        mp3_audio_frames: None,
    };
    // `build` runs the validator: beats strictly increasing, numbers 1–4.
    let set = build(&input, AnlzTarget::Cdj3000).unwrap();
    let ours = common::beats(&set.dat);
    assert_eq!(ours, beats);
    let anlz = ANLZ::read(&mut Cursor::new(&set.dat)).unwrap();
    let theirs: Vec<PqtzBeat> = anlz
        .sections
        .iter()
        .find_map(|s| match &s.content {
            Content::BeatGrid(g) => Some(
                g.beats
                    .iter()
                    .map(|b| PqtzBeat {
                        beat_number: b.beat_number,
                        tempo: b.tempo,
                        time_ms: b.time,
                    })
                    .collect(),
            ),
            _ => None,
        })
        .unwrap();
    assert_eq!(theirs, ours);
    ours
}

#[test]
fn two_tempo_grid_on_the_stick() {
    // 140 BPM, then 174 BPM from beat 141 (60 s), re-phased to beat 3.
    let g = [anchor(0, 14_000, 1), anchor(60_000, 17_400, 3)];
    let beats = built_beats(&g, 120_000);
    let at = beats.iter().position(|b| b.time_ms == 60_000).unwrap();
    assert_eq!(at, 140);
    assert_eq!(
        (
            beats[at - 1].tempo,
            beats[at - 1].beat_number,
            beats[at - 1].time_ms
        ),
        (14_000, 4, 59_571)
    );
    assert_eq!((beats[at].tempo, beats[at].beat_number), (17_400, 3));
    assert_eq!(beats[at + 1].time_ms, 60_345);
    assert_eq!(anchors_from_pqtz(&beats), g);
}

#[test]
fn rekordbox_style_markers_do_not_double_a_beat() {
    // A marker 1 ms after the beat the section predicts (5,279 of the
    // maintainer's 8,157 rekordbox marker pairs, D10).
    let g = [anchor(10, 12_800, 1), anchor(30_011, 12_800, 1)];
    let beats = built_beats(&g, 60_000);
    for pair in beats.windows(2) {
        assert!(pair[1].time_ms - pair[0].time_ms >= 468, "{pair:?}");
    }
}

/// Valid grids: 1–6 sections, 60–200 BPM, gaps of ½–64 beats, the first
/// marker within its first beat (the canonical form `PQTZ` gives back).
fn valid_grid() -> impl Strategy<Value = Vec<GridAnchor>> {
    (
        0.0f64..1.0,
        prop::collection::vec((6_000i64..20_000, 1u8..=4, 0.5f64..64.0), 1..6),
    )
        .prop_map(|(start, sections)| {
            let mut out: Vec<GridAnchor> = Vec::new();
            let mut pos = 0;
            for (bpm_x100, beat_in_bar, beats) in sections {
                let period = 6_000_000.0 / bpm_x100 as f64;
                pos = match out.last() {
                    None => (start * period) as i64,
                    Some(prev) => pos + (beats * 6_000_000.0 / prev.bpm_x100 as f64).ceil() as i64,
                };
                out.push(anchor(pos, bpm_x100, beat_in_bar));
            }
            out
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn sections_round_trip_through_pqtz(g in valid_grid()) {
        prop_assert_eq!(validate_anchors(&g), Ok(()));
        let duration = g.last().map_or(0, |a| a.position_ms) as u64 + 20_000;
        let beats = built_beats(&g, duration);
        prop_assert_eq!(beats.len(), grid_beats(&g, duration as f64).len());
        // The beats come back within the reader's 1 ms, with tempo and bar.
        let back = anchors_from_pqtz(&beats);
        let again = pqtz_beats_from_anchors(&back, duration).unwrap();
        prop_assert_eq!(again.len(), beats.len());
        for (a, b) in again.iter().zip(&beats) {
            prop_assert_eq!((a.tempo, a.beat_number), (b.tempo, b.beat_number));
            prop_assert!(a.time_ms.abs_diff(b.time_ms) <= 1);
        }
        // Markers that change tempo, phase or time by over 1 ms come back.
        let distinct = g.windows(2).all(|p| {
            let n = reks_core::beatgrid::section_beats(&p[0], &p[1]);
            p[1].bpm_x100 != p[0].bpm_x100
                || p[1].beat_in_bar != p[0].beat_in_bar_after(n)
                || (p[0].beat_ms(n).round() - p[1].position_ms as f64).abs() > 1.0
        });
        if distinct {
            prop_assert_eq!(back, g);
        }
    }
}
