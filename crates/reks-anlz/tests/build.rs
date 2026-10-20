//! Builder tests: section layout per target, validation, and round trips
//! (our reader: build → parse → build is byte-identical; rekordcrate as the
//! independent parser where 0.3.0 can read the sections).

#![allow(clippy::unwrap_used, clippy::panic)]

mod common;

use std::io::Cursor;

use binrw::{BinRead, BinWrite};
use proptest::prelude::*;
use rekordcrate::anlz::{Content, CueListType, CueType, ANLZ};
use reks_analysis::grid::Grid;
use reks_analysis::waveform::WaveformColumns;
use reks_anlz::grid::pqtz_beats;
use reks_anlz::validate::{validate_file, Expect, FileKind, Issue};
use reks_anlz::{build, AnlzInput, AnlzTarget, BuildError, Waveforms, PLACEHOLDER_CALIBRATION};
use reks_core::track::Cue;

const PATH: &str = "/Contents/Artist/Album/Track.mp3";

/// Pseudo-random columns (xorshift) so tests need no audio.
fn columns(n: usize, seed: u64) -> WaveformColumns {
    let mut x = seed | 1;
    let mut next = || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    let mut c = WaveformColumns::default();
    for _ in 0..n {
        let r = next();
        c.low.push(r as u8);
        c.mid.push((r >> 8) as u8);
        c.high.push((r >> 16) as u8);
        c.mono.push(((r >> 24) % 32) as u8);
    }
    c
}

fn waves(duration_ms: u64, seed: u64) -> Waveforms {
    let n = duration_ms.saturating_mul(150).div_ceil(1000) as usize;
    Waveforms::encode(&columns(n, seed), &PLACEHOLDER_CALIBRATION).unwrap()
}

fn cue(slot: Option<u8>, kind: i64, start_ms: f64, end_ms: Option<f64>) -> Cue {
    Cue {
        id: 0,
        slot,
        kind,
        start_ms,
        end_ms,
        name: None,
        color_rgb: None,
    }
}

fn grid_128() -> Grid {
    Grid::Constant {
        bpm: 128.0,
        first_beat: 1_116,
        first_beat_in_bar: 1,
    }
}

#[test]
fn layout_per_target() {
    let duration = 180_000;
    let beats = pqtz_beats(&grid_128(), 44_100, duration).unwrap();
    let w = waves(duration, 7);
    let cues = [
        cue(Some(0), 0, 1_000.0, None),
        cue(Some(5), 0, 2_000.0, None),
    ];
    let input = AnlzInput {
        contents_path: PATH,
        duration_ms: duration,
        beats: &beats,
        cues: &cues,
        waveforms: &w,
        mp3_audio_frames: Some(6_890),
    };
    let std = build(&input, AnlzTarget::Standard).unwrap();
    assert!(std.ex2.is_none());
    assert_eq!(
        std.files().iter().map(|f| f.0).collect::<Vec<_>>(),
        ["DAT", "EXT"]
    );
    let full = build(&input, AnlzTarget::Cdj3000).unwrap();
    assert_eq!((&full.dat, &full.ext), (&std.dat, &std.ext));
    let ex2 = full.ex2.as_deref().unwrap();
    assert_eq!(
        common::tags(&full.dat),
        ["PPTH", "PVBR", "PQTZ", "PWAV", "PWV2", "PCOB", "PCOB"]
    );
    assert_eq!(
        common::tags(&full.ext),
        ["PPTH", "PWV3", "PCOB", "PCOB", "PCO2", "PCO2", "PQT2", "PWV5", "PWV4"]
    );
    assert_eq!(common::tags(ex2), ["PPTH", "PWV7", "PWV6", "PWVC"]);
    for f in [&full.dat, &full.ext, &ex2.to_vec()] {
        assert_eq!(common::path(f), PATH);
    }
    assert_eq!(common::beats(&full.dat), beats);
    assert_eq!(common::mp3_frames(&full.dat), Some(6_890));
    // Hot cue A in the .DAT PCOB, F in the .EXT one, both in PCO2.
    let summary = |b: &[u8]| reks_anlz::read::summary(b).unwrap();
    assert_eq!(summary(&full.dat).cues, Some(1));
    assert_eq!(summary(&full.ext).cues, Some(1));
    assert_eq!(summary(&full.ext).extended_cues, Some(2));
}

#[test]
fn invalid_input_is_refused() {
    let duration = 10_000;
    let beats = pqtz_beats(&grid_128(), 44_100, duration).unwrap();
    let w = waves(duration, 1);
    let late = [cue(None, 0, 20_000.0, None)];
    let input = AnlzInput {
        contents_path: PATH,
        duration_ms: duration,
        beats: &beats,
        cues: &late,
        waveforms: &w,
        mp3_audio_frames: None,
    };
    let Err(BuildError::Invalid(issues)) = build(&input, AnlzTarget::Standard) else {
        panic!("a cue after the end must fail validation");
    };
    assert!(issues.iter().all(|i| matches!(
        i,
        Issue::Cue {
            time_ms: 20_000,
            ..
        }
    )));
    // Waveforms for a much longer track.
    let long = waves(60_000, 1);
    let input = AnlzInput {
        cues: &[],
        waveforms: &long,
        ..input
    };
    assert!(matches!(
        build(&input, AnlzTarget::Standard),
        Err(BuildError::Invalid(_))
    ));
}

#[test]
fn validator_catches_damage() {
    let duration = 30_000;
    let beats = pqtz_beats(&grid_128(), 44_100, duration).unwrap();
    let w = waves(duration, 3);
    let input = AnlzInput {
        contents_path: PATH,
        duration_ms: duration,
        beats: &beats,
        cues: &[],
        waveforms: &w,
        mp3_audio_frames: None,
    };
    let set = build(&input, AnlzTarget::Cdj3000).unwrap();
    let expect = Expect {
        path: Some(PATH),
        duration_ms: Some(duration),
    };
    assert!(validate_file(FileKind::Dat, &set.dat, expect).is_empty());
    // The .DAT read as a .2EX: wrong sections.
    let issues = validate_file(FileKind::Ex2, &set.dat, expect);
    assert!(issues.iter().any(|i| matches!(i, Issue::Unexpected { .. })));
    assert!(issues.iter().any(|i| matches!(i, Issue::Missing { .. })));
    // Another path.
    let other = Expect {
        path: Some("/Contents/x.mp3"),
        ..expect
    };
    assert!(matches!(
        validate_file(FileKind::Dat, &set.dat, other)[..],
        [Issue::PathMismatch { .. }]
    ));
    // Beat 2's beat_number set to 9.
    let mut dat = set.dat.clone();
    let pqtz = reks_anlz::read::sections(&dat).unwrap()[2].at;
    dat[pqtz + 24 + 8] = 0;
    dat[pqtz + 24 + 9] = 9;
    assert!(matches!(
        validate_file(FileKind::Dat, &dat, expect)[..],
        [Issue::Beat { index: 1, .. }]
    ));
    // Truncated file.
    assert!(matches!(
        validate_file(FileKind::Dat, &set.dat[..100], expect)[..],
        [Issue::Unreadable { .. }]
    ));
}

/// Every section the builder writes for a track without point cues parses
/// with rekordcrate and writes back byte-identically; field values agree.
/// rekordcrate 0.3.0 can't read point cues (cue type 1, see D8) or PCO2
/// entries (it expects no padding after the colour and a non-empty comment
/// length), so cue-bearing `.EXT` files are covered by our own reader below.
#[test]
fn round_trip_through_rekordcrate() {
    let duration = 95_000;
    let beats = pqtz_beats(&grid_128(), 44_100, duration).unwrap();
    let w = waves(duration, 11);
    let loops = [
        cue(Some(1), 4, 1_000.0, Some(2_875.0)),
        cue(None, 4, 30_000.0, Some(31_875.0)),
    ];
    let input = AnlzInput {
        contents_path: PATH,
        duration_ms: duration,
        beats: &beats,
        cues: &[],
        waveforms: &w,
        mp3_audio_frames: Some(3_637),
    };
    let plain = build(&input, AnlzTarget::Cdj3000).unwrap();
    let with_loops = build(
        &AnlzInput {
            cues: &loops,
            ..input
        },
        AnlzTarget::Cdj3000,
    )
    .unwrap();
    let parse = |bytes: &[u8]| {
        let anlz = ANLZ::read(&mut Cursor::new(bytes)).unwrap();
        let mut back = Cursor::new(Vec::new());
        anlz.write(&mut back).unwrap();
        assert_eq!(back.into_inner(), bytes);
        anlz
    };
    for f in plain.files() {
        parse(f.1);
    }
    let dat = parse(&with_loops.dat);
    let mut lists = 0;
    for s in &dat.sections {
        match &s.content {
            Content::Path(p) => assert_eq!(p.path.to_string(), PATH),
            Content::BeatGrid(g) => {
                assert_eq!(g.beats.len(), beats.len());
                for (a, b) in g.beats.iter().zip(&beats) {
                    assert_eq!(
                        (a.beat_number, a.tempo, a.time),
                        (b.beat_number, b.tempo, b.time_ms)
                    );
                }
            }
            Content::CueList(l) => {
                lists += 1;
                assert_eq!(l.cues.len(), 1);
                let c = &l.cues[0];
                assert_eq!(c.cue_type, CueType::Loop);
                match l.list_type {
                    CueListType::HotCues => {
                        assert_eq!((c.hot_cue, c.time, c.loop_time), (2, 1_000, 2_875))
                    }
                    CueListType::MemoryCues => {
                        assert_eq!((c.hot_cue, c.time, c.loop_time), (0, 30_000, 31_875))
                    }
                }
            }
            _ => {}
        }
    }
    assert_eq!(lists, 2);
}

fn palette_colour() -> impl Strategy<Value = Option<u32>> {
    prop_oneof![
        Just(None),
        any::<u32>().prop_map(|c| Some(c & 0xFF_FFFF)),
        Just(Some(0x0028_E214)),
    ]
}

fn arb_cue(duration_ms: u64) -> impl Strategy<Value = Cue> {
    (
        prop::option::of(0u8..10),
        prop_oneof![Just(0i64), Just(4), Just(3)],
        0.0..(duration_ms as f64 - 1.0),
        prop::option::of(0.0..20_000.0f64),
        prop::option::of("[a-zA-Z0-9 äß🎵]{0,10}"),
        palette_colour(),
    )
        .prop_map(|(slot, kind, start_ms, len, name, color_rgb)| Cue {
            id: 0,
            slot,
            kind,
            start_ms,
            end_ms: len.map(|l| start_ms + l),
            name,
            color_rgb,
        })
}

fn arb_input() -> impl Strategy<Value = (u64, Grid, Vec<Cue>, u64, Option<u32>, String)> {
    (2_000u64..90_000).prop_flat_map(|duration| {
        (
            Just(duration),
            (60.0..200.0f64, 0i64..44_100, 1u8..=4).prop_map(|(bpm, first_beat, b)| {
                Grid::Constant {
                    bpm,
                    first_beat,
                    first_beat_in_bar: b,
                }
            }),
            prop::collection::vec(arb_cue(duration), 0..12),
            any::<u64>(),
            prop::option::of(1u32..1_000_000),
            "/Contents/[A-Za-z0-9 _-]{1,30}\\.(mp3|flac|wav)",
        )
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// build → parse (our reader) → build is byte-identical, for both targets.
    #[test]
    fn build_parse_build_is_identical(
        (duration, grid, cues, seed, frames, path) in arb_input()
    ) {
        let beats = pqtz_beats(&grid, 44_100, duration).unwrap();
        let w = waves(duration, seed);
        let input = AnlzInput {
            contents_path: &path,
            duration_ms: duration,
            beats: &beats,
            cues: &cues,
            waveforms: &w,
            mp3_audio_frames: frames,
        };
        for target in [AnlzTarget::Standard, AnlzTarget::Cdj3000] {
            let set = build(&input, target).unwrap();
            let path = common::path(&set.dat);
            let beats = common::beats(&set.dat);
            let cues = common::cues(&set.ext);
            let w = common::waveforms(&set.dat, &set.ext, set.ex2.as_deref(), w.pwvc);
            let again = build(
                &AnlzInput {
                    contents_path: &path,
                    duration_ms: duration,
                    beats: &beats,
                    cues: &cues,
                    waveforms: &w,
                    mp3_audio_frames: common::mp3_frames(&set.dat),
                },
                target,
            )
            .unwrap();
            prop_assert_eq!(&again, &set);
        }
    }
}
