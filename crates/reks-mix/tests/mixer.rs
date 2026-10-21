//! Phase 5 task 6 render tests: channel strips, crossfader, master and
//! limiter through the whole graph, driven by mixer controls.

use std::sync::Arc;

use reks_mix::dsp::limiter::CEILING_DB;
use reks_mix::dsp::xfader::crossfader_gains;
use reks_mix::{render, ChannelParam, Control, DeckSource, FilterParam, FilterUnit, Graph};

const RATE: u32 = 48_000;

/// `seconds` of a stereo source from a per-frame `(left, right)` function.
fn source(rate: u32, seconds: f64, f: impl Fn(usize) -> (f64, f64)) -> Arc<DeckSource> {
    let frames = (f64::from(rate) * seconds) as usize;
    let pcm = (0..frames)
        .flat_map(|i| {
            let (l, r) = f(i);
            [l, r].map(|v| (v.clamp(-1.0, 1.0) * 32_767.0).round() as i16)
        })
        .collect();
    Arc::new(DeckSource::new(pcm, rate))
}

fn tone(rate: u32, hz: f64, amp: f64) -> Arc<DeckSource> {
    source(rate, 10.0, |i| {
        let v = amp * (std::f64::consts::TAU * hz * i as f64 / f64::from(rate)).sin();
        (v, v)
    })
}

fn play(g: &mut Graph, deck: u8, src: Arc<DeckSource>) {
    g.apply(Control::Load { deck, src });
    g.apply(Control::Play { deck });
}

fn channel(g: &mut Graph, ch: u8, param: ChannelParam, value: f32) {
    g.apply(Control::Channel { ch, param, value });
}

fn filter(g: &mut Graph, unit: FilterUnit, param: FilterParam, value: f32) {
    g.apply(Control::Filter { unit, param, value });
}

fn frames(g: &mut Graph, n: usize) -> Vec<f32> {
    let mut out = vec![0.0; n * 2];
    render(g, &mut out);
    out
}

fn peak(buf: &[f32]) -> f32 {
    buf.iter().fold(0.0, |a, s| a.max(s.abs()))
}

fn db(g: f32) -> f32 {
    20.0 * g.log10()
}

/// Largest change between consecutive frames on either side.
fn max_step(buf: &[f32]) -> f32 {
    buf.as_chunks::<2>()
        .0
        .windows(2)
        .map(|w| (w[1][0] - w[0][0]).abs().max((w[1][1] - w[0][1]).abs()))
        .fold(0.0, f32::max)
}

#[test]
fn crossfader_gains_at_both_ends_and_centre_for_both_curve_extremes() {
    let mut g = Graph::new(RATE).unwrap();
    // Deck 0 (side X) on the left only, deck 1 (side Y) on the right only.
    play(&mut g, 0, source(RATE, 5.0, |_| (0.5, 0.0)));
    play(&mut g, 1, source(RATE, 5.0, |_| (0.0, 0.5)));
    channel(&mut g, 0, ChannelParam::XfadeAssign, 0.0);
    channel(&mut g, 1, ChannelParam::XfadeAssign, 1.0);
    let level = 0.5;
    for curve in [0.0, 1.0] {
        for pos in [0.0, 0.5, 1.0] {
            g.apply(Control::Crossfader { pos, curve });
            let out = frames(&mut g, 2_400);
            let [l, r] = out.as_chunks::<2>().0[2_399];
            let want = crossfader_gains(pos, curve);
            assert!(
                (l - level * want.x).abs() < 1e-6,
                "curve {curve} pos {pos}: X {l}"
            );
            assert!(
                (r - level * want.y).abs() < 1e-6,
                "curve {curve} pos {pos}: Y {r}"
            );
            let (x, y) = (db(l / level), db(r / level));
            match (curve == 0.0, pos) {
                (true, 0.5) => {
                    assert!((x + 4.0).abs() < 0.01 && (y + 4.0).abs() < 0.01, "{x} {y}");
                }
                (false, 0.5) => assert!(x.abs() < 0.01 && y.abs() < 0.01, "{x} {y}"),
                (_, 0.0) => assert!(x.abs() < 0.01 && r == 0.0, "{x} {r}"),
                _ => assert!(l == 0.0 && y.abs() < 0.01, "{l} {y}"),
            }
        }
    }
    // An unassigned channel ignores the crossfader.
    channel(&mut g, 1, ChannelParam::XfadeAssign, 0.5);
    g.apply(Control::Crossfader {
        pos: 0.0,
        curve: 0.0,
    });
    let out = frames(&mut g, 2_400);
    assert!((out[out.len() - 1] - level).abs() < 1e-6);
}

#[test]
fn limiter_ceiling_holds_with_a_maximum_resonance_sweep() {
    let ceiling = 10f32.powf(CEILING_DB / 20.0);
    for rate in [44_100, 48_000, 96_000] {
        for mode in [FilterParam::LowPass, FilterParam::HighPass] {
            let mut g = Graph::new(rate).unwrap();
            // Full-scale saw plus noise, trim fully up (+10 dB).
            let mut seed = 0x1234_5678u32;
            let noise: Vec<f64> = (0..rate as usize * 3)
                .map(|_| {
                    seed ^= seed << 13;
                    seed ^= seed >> 17;
                    seed ^= seed << 5;
                    f64::from(seed) / f64::from(u32::MAX) * 2.0 - 1.0
                })
                .collect();
            play(
                &mut g,
                0,
                source(rate, 3.0, |i| {
                    let saw = 2.0 * (i as f64 * 110.0 / f64::from(rate)).fract() - 1.0;
                    let s = 0.7 * saw + 0.3 * noise[i];
                    (s, -s)
                }),
            );
            channel(&mut g, 0, ChannelParam::Trim, 1.0);
            channel(&mut g, 0, ChannelParam::FilterAssign, 0.0);
            filter(&mut g, FilterUnit::X, FilterParam::LowPass, 0.0);
            filter(&mut g, FilterUnit::X, mode, 1.0);
            filter(&mut g, FilterUnit::X, FilterParam::Resonance, 1.0);
            filter(&mut g, FilterUnit::X, FilterParam::On, 1.0);
            // FREQ 30 Hz → 20 kHz → 30 Hz over two seconds in 1 ms steps.
            let steps = 2_000;
            let block = (rate / 1000) as usize;
            let mut worst = 0.0f32;
            let mut hottest = 0.0f32;
            for i in 0..steps {
                let v = 1.0 - (2.0 * i as f32 / steps as f32 - 1.0).abs();
                filter(&mut g, FilterUnit::X, FilterParam::Freq, v);
                worst = worst.max(peak(&frames(&mut g, block)));
                hottest = hottest.max(g.state().master.level);
            }
            assert!(hottest > 4.0, "{rate} {mode:?}: sweep too tame ({hottest})");
            assert!(worst <= ceiling, "{rate} {mode:?}: {worst}");
        }
    }
}

#[test]
fn assign_and_filter_switches_crossfade_without_clicks() {
    let mut g = Graph::new(RATE).unwrap();
    // 50 Hz at half scale moves at most 0.0033 a frame.
    play(&mut g, 0, tone(RATE, 50.0, 0.5));
    filter(&mut g, FilterUnit::X, FilterParam::On, 1.0);
    filter(&mut g, FilterUnit::X, FilterParam::LowPass, 0.0);
    filter(&mut g, FilterUnit::X, FilterParam::HighPass, 1.0);
    filter(&mut g, FilterUnit::X, FilterParam::Freq, 0.5);
    filter(&mut g, FilterUnit::Y, FilterParam::On, 1.0);
    g.apply(Control::Crossfader {
        pos: 1.0,
        curve: 0.0,
    });
    let mut out = frames(&mut g, 4_800);
    let switches = [
        // FILTER assign: dry → X (high-pass, removes the tone) → Y → off.
        (ChannelParam::FilterAssign, 0.0),
        (ChannelParam::FilterAssign, 1.0),
        (ChannelParam::FilterAssign, 0.0),
        // XFADE assign: X (closed at this crossfader position) → off → Y.
        (ChannelParam::XfadeAssign, 0.0),
        (ChannelParam::XfadeAssign, 0.5),
        (ChannelParam::XfadeAssign, 1.0),
    ];
    for (param, value) in switches {
        channel(&mut g, 0, param, value);
        out.extend(frames(&mut g, 2_400));
    }
    // Filter type and ON while the channel goes through X.
    for (param, value) in [
        (FilterParam::LowPass, 1.0),
        (FilterParam::HighPass, 0.0),
        (FilterParam::BandPass, 1.0),
        (FilterParam::On, 0.0),
        (FilterParam::On, 1.0),
    ] {
        filter(&mut g, FilterUnit::X, param, value);
        out.extend(frames(&mut g, 2_400));
    }
    let step = max_step(&out);
    assert!(step < 0.01, "largest step {step}");
    // The high-pass took the tone out while the channel was on X.
    let on_x = &out[2 * (4_800 + 1_800)..2 * (4_800 + 2_400)];
    assert!(peak(on_x) < 0.05, "{}", peak(on_x));
}

#[test]
fn eq_and_fader_controls_reach_the_strip() {
    let mut g = Graph::new(RATE).unwrap();
    play(&mut g, 2, tone(RATE, 50.0, 0.5));
    frames(&mut g, 4_800);
    let flat = peak(&frames(&mut g, 4_800));
    assert!((flat - 0.5).abs() < 1e-3, "{flat}");
    channel(&mut g, 2, ChannelParam::EqLo, 0.0);
    frames(&mut g, 9_600);
    let killed = peak(&frames(&mut g, 4_800));
    assert!(
        db(killed / flat) < -12.0,
        "LO kill at 50 Hz: {}",
        db(killed / flat)
    );
    channel(&mut g, 2, ChannelParam::EqLo, 0.5);
    channel(&mut g, 2, ChannelParam::Fader, 0.5);
    frames(&mut g, 9_600);
    let half = peak(&frames(&mut g, 4_800));
    assert!((db(half / flat) + 18.06).abs() < 0.1, "{}", db(half / flat));
    g.apply(Control::Master {
        param: reks_mix::MasterParam::Level,
        value: 0.0,
    });
    frames(&mut g, 960);
    assert_eq!(peak(&frames(&mut g, 960)), 0.0);
}

#[test]
fn vcf_to_xfader_and_link_move_the_cutoff() {
    let mut g = Graph::new(RATE).unwrap();
    play(&mut g, 0, tone(RATE, 2_000.0, 0.5));
    channel(&mut g, 0, ChannelParam::FilterAssign, 1.0);
    for unit in [FilterUnit::X, FilterUnit::Y] {
        filter(&mut g, unit, FilterParam::On, 1.0);
        filter(&mut g, unit, FilterParam::Freq, 1.0);
    }
    let level = |g: &mut Graph| {
        frames(g, 9_600);
        peak(&frames(g, 4_800))
    };
    let open = level(&mut g);
    assert!(open > 0.45, "{open}");
    // LINK: Y follows X's FREQ.
    filter(&mut g, FilterUnit::X, FilterParam::Freq, 0.0);
    assert!((level(&mut g) - open).abs() < 1e-3, "not linked yet");
    filter(&mut g, FilterUnit::Y, FilterParam::Link, 1.0);
    assert!(level(&mut g) < 0.01, "linked to 30 Hz");
    // VCF TO XFADER on X: the crossfader is X's FREQ, and Y follows.
    filter(&mut g, FilterUnit::X, FilterParam::VcfToXfader, 1.0);
    g.apply(Control::Crossfader {
        pos: 1.0,
        curve: 0.0,
    });
    assert!(level(&mut g) > 0.45, "crossfader opened the filter");
}

#[test]
fn meters_report_channels_and_master() {
    let mut g = Graph::new(RATE).unwrap();
    play(&mut g, 1, source(RATE, 4.0, |_| (0.25, -0.5)));
    channel(&mut g, 1, ChannelParam::Fader, 0.5);
    // The master meter caught the fader's ramp down; let its display fall.
    frames(&mut g, RATE as usize * 3 / 2);
    let st = g.state();
    assert!((st.channels[1].level - 0.5).abs() < 1e-3, "pre-fader");
    assert_eq!(st.channels[0].level, 0.0);
    assert!((st.master.level - 0.5 * 0.125).abs() < 1e-3, "post-fader");
    assert!(!st.channels[1].clip && !st.master.clip);
    // Trim fully up drives the channel over 0 dBFS; the output stays limited.
    channel(&mut g, 1, ChannelParam::Trim, 1.0);
    channel(&mut g, 1, ChannelParam::Fader, 1.0);
    let out = frames(&mut g, 4_800);
    let st = g.state();
    assert!(st.channels[1].clip && st.master.clip);
    assert!(peak(&out) <= 10f32.powf(CEILING_DB / 20.0));
    frames(&mut g, 32);
    assert!(g.state().master.clip, "still over");
}

#[test]
fn channels_1_2_default_to_filter_x_and_3_4_to_filter_y() {
    // One 60 Hz tone per channel, each alone; a unit set to HP at 20 kHz
    // removes it from the channels assigned to that unit only.
    let level = |unit_on: Option<FilterUnit>, deck: u8| {
        let mut g = Graph::new(RATE).unwrap();
        play(&mut g, deck, tone(RATE, 60.0, 0.5));
        if let Some(unit) = unit_on {
            filter(&mut g, unit, FilterParam::LowPass, 0.0);
            filter(&mut g, unit, FilterParam::HighPass, 1.0);
            filter(&mut g, unit, FilterParam::Freq, 1.0);
            filter(&mut g, unit, FilterParam::On, 1.0);
        }
        frames(&mut g, 9_600);
        peak(&frames(&mut g, 4_800))
    };
    for deck in 0..4u8 {
        let (own, other) = if deck < 2 {
            (FilterUnit::X, FilterUnit::Y)
        } else {
            (FilterUnit::Y, FilterUnit::X)
        };
        let dry = level(None, deck);
        assert!((dry - 0.5).abs() < 0.01, "deck {deck} dry: {dry}");
        let cut = level(Some(own), deck);
        assert!(cut < 0.001, "deck {deck} through {own:?}: {cut}");
        let kept = level(Some(other), deck);
        assert!(
            (kept - 0.5).abs() < 0.01,
            "deck {deck} not on {other:?}: {kept}"
        );
    }
}
