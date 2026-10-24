//! Phase 5 task 3 render tests: two click tracks, one synced to the other,
//! clicks of deck 0 on the left and deck 1 on the right.

mod common;

use std::sync::Arc;

use common::{click_track, run, worst_gap_ms, Fixture, Onsets, Section, Side};
use reks_mix::{Control, Graph};

const RATE: u32 = 48_000;

fn constant(bpm: f64, beats: usize) -> [Section; 1] {
    [Section { bpm, beats }]
}

/// Loads `lead` on deck 0 and starts it; after `delay_s`, loads `follow` on
/// deck 1 with sync on and starts it. Returns the frame deck 1 started at.
fn start(g: &mut Graph, o: &mut Onsets, lead: &Fixture, follow: &Fixture, bar: bool) -> usize {
    g.apply(Control::Load {
        deck: 0,
        src: Arc::clone(&lead.src),
    });
    g.apply(Control::Play { deck: 0 });
    let delay = RATE as usize * 3 / 2;
    run(g, o, delay);
    g.apply(Control::Load {
        deck: 1,
        src: Arc::clone(&follow.src),
    });
    g.apply(Control::Sync {
        deck: 1,
        on: true,
        bar,
    });
    g.apply(Control::Play { deck: 1 });
    delay
}

#[test]
fn synced_128_and_125_stay_within_1_ms_for_64_bars() {
    // The leader at 128, the follower at 125 with its first beat elsewhere.
    let lead = click_track(RATE, Side::Left, 100.0, &constant(128.0, 300));
    let follow = click_track(RATE, Side::Right, 437.0, &constant(125.0, 330));
    let mut g = Graph::new(RATE).unwrap();
    let mut o = Onsets::new(RATE);
    let from = start(&mut g, &mut o, &lead, &follow, true);
    // 64 bars at 128 BPM after the follower starts, and a beat to spare.
    let bars = (65.0 * 4.0 * 60.0 / 128.0 * f64::from(RATE)) as usize;
    run(&mut g, &mut o, bars);

    let st = g.state();
    assert_eq!(st.master_deck, Some(0));
    assert!(st.decks[1].locked);
    assert!((st.decks[1].speed - 128.0 / 125.0).abs() < 1e-3);
    // From the first beat after the sync jump's crossfade.
    let settle = from as f64 + f64::from(RATE) * 0.05;
    let gap = worst_gap_ms(&o, RATE, settle);
    eprintln!("128/125, 64 bars: worst gap {gap:.4} ms");
    assert!(gap <= 1.0, "worst gap {gap:.3} ms");
    let checked = o.left.iter().filter(|l| l.at >= settle).count();
    assert!(checked >= 256, "{checked} beats");
    // Bar sync: downbeats on downbeats.
    for l in o.left.iter().filter(|l| l.at >= settle) {
        let r = o
            .right
            .iter()
            .min_by(|a, b| (a.at - l.at).abs().total_cmp(&(b.at - l.at).abs()))
            .unwrap();
        assert_eq!(l.loud, r.loud, "at frame {}", l.at);
    }
}

#[test]
fn a_tempo_change_in_the_follower_stays_within_2_ms() {
    // 122 BPM, a 16-beat ramp to 130 with an anchor per beat (as rekordbox
    // writes dynamic grids), then 130 BPM.
    let mut sections = vec![Section {
        bpm: 122.0,
        beats: 48,
    }];
    sections.extend((1..=16).map(|i| Section {
        bpm: 122.0 + f64::from(i) * 0.5,
        beats: 1,
    }));
    sections.push(Section {
        bpm: 130.0,
        beats: 120,
    });
    let lead = click_track(RATE, Side::Left, 250.0, &constant(126.0, 200));
    let follow = click_track(RATE, Side::Right, 80.0, &sections);
    let mut g = Graph::new(RATE).unwrap();
    let mut o = Onsets::new(RATE);
    let from = start(&mut g, &mut o, &lead, &follow, false);
    run(&mut g, &mut o, RATE as usize * 80);

    let settle = from as f64 + f64::from(RATE) * 0.05;
    let gap = worst_gap_ms(&o, RATE, settle);
    eprintln!("follower tempo change: worst gap {gap:.4} ms");
    assert!(gap <= 2.0, "worst gap {gap:.3} ms");
    // The follower crossed the ramp: it now plays a 130 BPM grid at 126.
    let st = g.state().decks[1];
    assert!((st.speed - 126.0 / 130.0).abs() < 1e-3, "{}", st.speed);
}

#[test]
fn a_tempo_change_in_the_leader_is_followed_within_2_ms() {
    let mut sections = vec![Section {
        bpm: 124.0,
        beats: 40,
    }];
    sections.extend((1..=8).map(|i| Section {
        bpm: 124.0 + f64::from(i),
        beats: 1,
    }));
    sections.push(Section {
        bpm: 132.0,
        beats: 100,
    });
    let lead = click_track(RATE, Side::Left, 0.0, &sections);
    let follow = click_track(RATE, Side::Right, 300.0, &constant(128.0, 200));
    let mut g = Graph::new(RATE).unwrap();
    let mut o = Onsets::new(RATE);
    let from = start(&mut g, &mut o, &lead, &follow, false);
    run(&mut g, &mut o, RATE as usize * 60);

    let settle = from as f64 + f64::from(RATE) * 0.05;
    let gap = worst_gap_ms(&o, RATE, settle);
    eprintln!("leader tempo change: worst gap {gap:.4} ms");
    assert!(gap <= 2.0, "worst gap {gap:.3} ms");
    assert!((g.state().decks[1].speed - 132.0 / 128.0).abs() < 1e-3);
}

/// Acceptance criterion: two synced constant-grid tracks stay within 2 ms of
/// each other for 10 minutes. Sources at 22.05 kHz (both decks resample, and
/// the two 10-minute tracks fit in about 110 MB).
#[test]
fn two_synced_constant_grids_stay_within_2_ms_for_10_minutes() {
    const SRC_RATE: u32 = 22_050;
    let lead = click_track(SRC_RATE, Side::Left, 60.0, &constant(128.0, 1_300));
    let follow = click_track(SRC_RATE, Side::Right, 510.0, &constant(126.0, 1_300));
    let mut g = Graph::new(RATE).unwrap();
    let mut o = Onsets::new(RATE);
    let from = start(&mut g, &mut o, &lead, &follow, true);
    run(&mut g, &mut o, RATE as usize * 600);

    let settle = from as f64 + f64::from(RATE) * 0.05;
    let gap = worst_gap_ms(&o, RATE, settle);
    eprintln!("128/126, 10 minutes: worst gap {gap:.4} ms");
    assert!(gap <= 2.0, "worst gap {gap:.3} ms");
    let checked = o.left.iter().filter(|l| l.at >= settle).count();
    assert!(checked >= 1_270, "{checked} beats over 10 minutes");
    assert!(g.state().decks[1].playing && g.state().decks[1].locked);
}
