//! Phase 5 task 7 render tests: the headphone cue through the whole graph.
//! Output routing per mode is tested on the engine's callback
//! (`engine::tests::each_cue_routing_puts_master_and_cue_on_the_expected_outputs`).

use std::sync::Arc;

use reks_mix::dsp::limiter::CEILING_DB;
use reks_mix::{ChannelParam, Control, DeckSource, FilterParam, FilterUnit, Graph, MasterParam};

const RATE: u32 = 48_000;

fn tone(hz: f64, amp: f64) -> Arc<DeckSource> {
    let pcm = (0..RATE as usize * 10)
        .flat_map(|i| {
            let v = amp * (std::f64::consts::TAU * hz * i as f64 / f64::from(RATE)).sin();
            let s = (v * 32_767.0).round() as i16;
            [s, s]
        })
        .collect();
    Arc::new(DeckSource::new(pcm, RATE))
}

fn play(g: &mut Graph, deck: u8, src: Arc<DeckSource>) {
    g.apply(Control::Load { deck, src });
    g.apply(Control::Play { deck });
}

fn channel(g: &mut Graph, ch: u8, param: ChannelParam, value: f32) {
    g.apply(Control::Channel { ch, param, value });
}

fn master(g: &mut Graph, param: MasterParam, value: f32) {
    g.apply(Control::Master { param, value });
}

/// Renders `n` frames; returns (master, phones).
fn both(g: &mut Graph, n: usize) -> (Vec<f32>, Vec<f32>) {
    let mut m = vec![0.0; n * 2];
    let mut p = vec![0.0; n * 2];
    g.render_outputs(&mut m, Some(&mut p), drop);
    (m, p)
}

/// Phones peak after letting ramps settle.
fn phones_peak(g: &mut Graph) -> f32 {
    both(g, 9_600);
    peak(&both(g, 4_800).1)
}

fn peak(buf: &[f32]) -> f32 {
    buf.iter().fold(0.0, |a, s| a.max(s.abs()))
}

#[test]
fn cue_is_pre_fader_and_pre_eq_unless_switched_post() {
    let mut g = Graph::new(RATE).unwrap();
    play(&mut g, 0, tone(50.0, 0.5));
    assert_eq!(phones_peak(&mut g), 0.0, "nothing cued");
    channel(&mut g, 0, ChannelParam::Cue, 1.0);
    channel(&mut g, 0, ChannelParam::Fader, 0.0);
    channel(&mut g, 0, ChannelParam::EqLo, 0.0);
    // Fader shut and LO killed: the master is silent, the pre-EQ cue is not.
    let p = phones_peak(&mut g);
    assert!((p - 0.5).abs() < 1e-3, "pre-EQ, pre-fader: {p}");
    assert_eq!(peak(&both(&mut g, 4_800).0), 0.0);
    master(&mut g, MasterParam::CuePostEq, 1.0);
    let p = phones_peak(&mut g);
    assert!(p < 0.5 / 4.0, "post-EQ hears the LO kill: {p}");
    // Trim reaches both taps.
    master(&mut g, MasterParam::CuePostEq, 0.0);
    channel(&mut g, 0, ChannelParam::Trim, 0.0);
    let p = phones_peak(&mut g);
    assert!((20.0 * (p / 0.5).log10() + 10.0).abs() < 0.05, "{p}");
}

#[test]
fn cue_mix_blends_in_the_master_and_split_cue_splits_it() {
    let mut g = Graph::new(RATE).unwrap();
    // Deck 0 to the master only, deck 1 cued with its fader shut.
    play(&mut g, 0, tone(100.0, 0.4));
    play(&mut g, 1, tone(1_000.0, 0.2));
    channel(&mut g, 1, ChannelParam::Fader, 0.0);
    channel(&mut g, 1, ChannelParam::Cue, 1.0);
    let cue_only = phones_peak(&mut g);
    assert!((cue_only - 0.2).abs() < 1e-3, "{cue_only}");
    master(&mut g, MasterParam::CueMix, 1.0);
    let master_only = phones_peak(&mut g);
    assert!((master_only - 0.4).abs() < 1e-3, "{master_only}");
    master(&mut g, MasterParam::SplitCue, 1.0);
    both(&mut g, 9_600);
    let (_, p) = both(&mut g, 4_800);
    let left: Vec<f32> = p.iter().step_by(2).copied().collect();
    let right: Vec<f32> = p.iter().skip(1).step_by(2).copied().collect();
    assert!((peak(&left) - 0.2).abs() < 1e-3, "cue left");
    assert!((peak(&right) - 0.4).abs() < 1e-3, "master right");
    master(&mut g, MasterParam::CueLevel, 0.5);
    let p = phones_peak(&mut g);
    assert!((p - 0.4 * 0.125).abs() < 1e-3, "cue level: {p}");
}

#[test]
fn cue_limiter_holds_with_a_maximum_resonance_sweep() {
    let ceiling = 10f32.powf(CEILING_DB / 20.0);
    let mut g = Graph::new(RATE).unwrap();
    play(&mut g, 0, tone(110.0, 0.9));
    channel(&mut g, 0, ChannelParam::Trim, 1.0);
    channel(&mut g, 0, ChannelParam::FilterAssign, 0.0);
    channel(&mut g, 0, ChannelParam::Cue, 1.0);
    master(&mut g, MasterParam::CuePostEq, 1.0);
    for (param, value) in [
        (FilterParam::Resonance, 1.0),
        (FilterParam::On, 1.0),
        (FilterParam::Freq, 0.0),
    ] {
        g.apply(Control::Filter {
            unit: FilterUnit::X,
            param,
            value,
        });
    }
    let mut worst = [0.0f32; 2];
    for i in 0..2_000 {
        let v = 1.0 - (2.0 * i as f32 / 2_000.0 - 1.0).abs();
        g.apply(Control::Filter {
            unit: FilterUnit::X,
            param: FilterParam::Freq,
            value: v,
        });
        let (m, p) = both(&mut g, 48);
        worst = [worst[0].max(peak(&m)), worst[1].max(peak(&p))];
    }
    assert!(worst[0] <= ceiling && worst[1] <= ceiling, "{worst:?}");
    // The resonance peak at 110 Hz pushed both buses into the limiters.
    assert!(worst[1] > 0.85, "{worst:?}");
}

#[test]
fn cue_switches_crossfade_without_clicks() {
    let mut g = Graph::new(RATE).unwrap();
    play(&mut g, 0, tone(50.0, 0.5));
    channel(&mut g, 0, ChannelParam::EqLo, 0.0);
    let mut phones = Vec::new();
    for (ch, param, value) in [
        (Some(ChannelParam::Cue), None, 1.0),
        (None, Some(MasterParam::CuePostEq), 1.0),
        (None, Some(MasterParam::CuePostEq), 0.0),
        (None, Some(MasterParam::SplitCue), 1.0),
        (None, Some(MasterParam::CueMix), 1.0),
        (Some(ChannelParam::Cue), None, 0.0),
    ] {
        if let Some(p) = ch {
            channel(&mut g, 0, p, value);
        }
        if let Some(p) = param {
            master(&mut g, p, value);
        }
        phones.extend(both(&mut g, 2_400).1);
    }
    let step = phones
        .as_chunks::<2>()
        .0
        .windows(2)
        .map(|w| (w[1][0] - w[0][0]).abs().max((w[1][1] - w[0][1]).abs()))
        .fold(0.0, f32::max);
    assert!(step < 0.01, "largest step {step}");
}
