//! Phase 3 task 17: decks on a grid with tempo sections. The 140 → 174 BPM
//! click track from `fixtures/gen`, gridded by the library's beat rule
//! (reks-core `deck_beats`) as the app loads it: the grid's beats sit on the
//! clicks, quantised beat jumps and loops cross the tempo change on the
//! beat, and a synced deck follows it.

// Test helpers outside `#[test]` fns are not covered by clippy's allow-unwrap-in-tests.
#![allow(clippy::unwrap_used)]

mod common;

use std::sync::Arc;

use common::{click_track, run, worst_gap_ms, Onsets, Section, Side};
use reks_core::deck::deck_beats;
use reks_core::GridAnchor;
use reks_fixtures_gen::tempo::{beat_onsets, render_tempo, LEAD_MS, TWO_TEMPO};
use reks_mix::{Control, DeckSource, Graph, GridBeat};

const RATE: u32 = 48_000;
/// Beats in the first (140 BPM) section.
const FIRST: usize = 140;

/// The two-tempo click track on the left channel with its library grid: a
/// marker on the first click and one on the first 174 BPM click (60 s on).
fn two_tempo() -> (Arc<DeckSource>, Vec<GridBeat>, Vec<usize>) {
    let mono = render_tempo(RATE, &TWO_TEMPO);
    let pcm: Vec<i16> = mono
        .iter()
        .flat_map(|x| [(x * 32_767.0).round() as i16, 0])
        .collect();
    let lead = i64::from(LEAD_MS);
    let anchors = [
        GridAnchor {
            position_ms: lead,
            bpm_x100: 14_000,
            beat_in_bar: 1,
            beats_per_bar: 4,
        },
        GridAnchor {
            position_ms: lead + 60_000,
            bpm_x100: 17_400,
            beat_in_bar: 1,
            beats_per_bar: 4,
        },
    ];
    let beats: Vec<GridBeat> = deck_beats(&anchors, RATE, mono.len())
        .iter()
        .map(|b| GridBeat {
            frame: b.frame,
            bpm: b.bpm,
            beat_in_bar: b.beat_in_bar,
            beats_per_bar: b.beats_per_bar,
        })
        .collect();
    let src = Arc::new(DeckSource::new(pcm, RATE).with_grid(&beats));
    (src, beats, beat_onsets(RATE, &TWO_TEMPO))
}

#[test]
fn the_grid_sits_on_every_click() {
    let (_, beats, onsets) = two_tempo();
    assert!(beats.len() >= onsets.len());
    for (n, (b, &at)) in beats.iter().zip(&onsets).enumerate() {
        assert!(
            (b.frame - at as f64).abs() <= 1.0,
            "beat {n}: {} vs {at}",
            b.frame
        );
    }
    assert_eq!((beats[FIRST - 1].bpm, beats[FIRST].bpm), (140.0, 174.0));
    assert_eq!(beats[FIRST].beat_in_bar, 1);
}

/// Loads the two-tempo track on deck 0 with quantise on and the head on
/// beat `n`.
fn at_beat(n: usize) -> (Graph, Vec<usize>) {
    let (src, _, onsets) = two_tempo();
    let mut g = Graph::new(RATE).unwrap();
    let mut o = Onsets::new(RATE);
    g.apply(Control::Load { deck: 0, src });
    run(&mut g, &mut o, 512);
    g.apply(Control::Quantise { deck: 0, on: true });
    g.apply(Control::Seek {
        deck: 0,
        frame: onsets[n] as i64,
    });
    (g, onsets)
}

#[test]
fn quantised_beat_jumps_cross_the_tempo_change_on_the_beat() {
    let (mut g, on) = at_beat(FIRST - 2);
    let near = |g: &mut Graph, at: usize| (g.state().decks[0].position - at as i64).abs() <= 1;
    g.apply(Control::BeatJump {
        deck: 0,
        beats: 4.0,
    });
    assert!(
        near(&mut g, on[FIRST + 2]),
        "{}",
        g.state().decks[0].position
    );
    g.apply(Control::BeatJump {
        deck: 0,
        beats: 16.0,
    });
    assert!(near(&mut g, on[FIRST + 18]));
    g.apply(Control::BeatJump {
        deck: 0,
        beats: -32.0,
    });
    assert!(near(&mut g, on[FIRST - 14]));
}

#[test]
fn a_beat_loop_over_the_tempo_change_ends_on_a_beat() {
    let (mut g, on) = at_beat(FIRST - 2);
    g.apply(Control::BeatLoop {
        deck: 0,
        beats: 4.0,
    });
    let l = g.state().decks[0].loop_region.unwrap();
    assert!((l.start - on[FIRST - 2] as i64).abs() <= 1);
    assert!((l.end - on[FIRST + 2] as i64).abs() <= 1, "{l:?}");
    assert_eq!(l.beats, Some(4.0));
}

#[test]
fn a_synced_deck_follows_the_tempo_change_within_2_ms() {
    let (lead, _, on) = two_tempo();
    let follow = click_track(
        RATE,
        Side::Right,
        300.0,
        &[Section {
            bpm: 128.0,
            beats: 260,
        }],
    );
    let mut g = Graph::new(RATE).unwrap();
    let mut o = Onsets::new(RATE);
    g.apply(Control::Load { deck: 0, src: lead });
    g.apply(Control::Play { deck: 0 });
    run(&mut g, &mut o, RATE as usize * 50);
    g.apply(Control::Load {
        deck: 1,
        src: Arc::clone(&follow.src),
    });
    g.apply(Control::Sync {
        deck: 1,
        on: true,
        bar: true,
    });
    g.apply(Control::Play { deck: 1 });
    // Through the change at 60.1 s and 16 bars of 174 BPM.
    let until = on[FIRST + 64];
    run(&mut g, &mut o, until - RATE as usize * 50);

    let settle = RATE as f64 * 50.05;
    let gap = worst_gap_ms(&o, RATE, settle);
    eprintln!("two-tempo leader: worst gap {gap:.4} ms");
    assert!(gap <= 2.0, "worst gap {gap:.3} ms");
    let checked = o.left.iter().filter(|l| l.at >= on[FIRST] as f64).count();
    assert!(checked >= 60, "{checked} beats after the change");
    let st = g.state().decks[1];
    assert!((st.speed - 174.0 / 128.0).abs() < 1e-3, "{}", st.speed);
}
