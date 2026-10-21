//! Phase 5 task 2 render tests: the deck against the decoder and a click track.

mod common;

use std::sync::Arc;

use common::{click_track, run, Onsets, Section, Side};
use reks_mix::{render, Control, DeckSource, Graph, RAMP_MS};

#[test]
fn at_unit_speed_the_render_equals_the_decode_sample_for_sample() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("clicks.wav");
    let file = std::fs::File::create(&path).unwrap();
    reks_fixtures_gen::write_wav(std::io::BufWriter::new(file)).unwrap();
    let decoded = reks_audio::decode_stereo(&path, None).unwrap();
    let rate = decoded.pcm.sample_rate;
    let pcm = decoded.pcm.samples;
    assert_eq!(pcm.len(), reks_fixtures_gen::DURATION_SAMPLES * 2);

    let mut g = Graph::new(rate).unwrap();
    g.apply(Control::Load {
        deck: 0,
        src: Arc::new(DeckSource::new(pcm.clone(), rate)),
    });
    g.apply(Control::Tempo {
        deck: 0,
        ratio: 1.0,
    });
    g.apply(Control::Play { deck: 0 });
    // The mixer is flat and the fixture stays under the limiter's ceiling,
    // so only the limiter's look-ahead delay is left.
    let lat = g.latency() * 2;
    let mut out = vec![0.0f32; pcm.len() + 2_000 + lat];
    render(&mut g, &mut out);
    assert!(out[..lat].iter().all(|&s| s == 0.0));
    let out = &out[lat..];

    // The 5 ms fade-in only scales the first frames down.
    let ramp = (rate * RAMP_MS / 1000) as usize * 2;
    for (o, &s) in out[..ramp].iter().zip(&pcm) {
        assert!(o.abs() <= (f32::from(s) / 32_768.0).abs());
    }
    for (i, (o, &s)) in out[ramp..pcm.len()].iter().zip(&pcm[ramp..]).enumerate() {
        assert_eq!(o * 32_768.0, f32::from(s), "sample {}", i + ramp);
    }
    assert!(out[pcm.len()..].iter().all(|&s| s == 0.0));
    assert!(!g.state().decks[0].playing);
}

#[test]
fn at_1_05_click_spacing_is_1_over_1_05_within_a_sample() {
    const RATE: u32 = 48_000;
    // 120 BPM: 24,000 frames between clicks.
    let fx = click_track(
        RATE,
        Side::Left,
        100.0,
        &[Section {
            bpm: 120.0,
            beats: 40,
        }],
        false,
    );
    let mut g = Graph::new(RATE).unwrap();
    g.apply(Control::Load {
        deck: 0,
        src: Arc::clone(&fx.src),
    });
    g.apply(Control::Tempo {
        deck: 0,
        ratio: 1.05,
    });
    g.apply(Control::Play { deck: 0 });
    let mut onsets = Onsets::new(RATE);
    run(
        &mut g,
        &mut onsets,
        (fx.src.frames() as f64 / 1.05) as usize,
    );

    let lat = g.latency() as f64;
    let got: Vec<f64> = onsets.left.iter().map(|o| o.at - lat).collect();
    assert_eq!(got.len(), fx.beats.len());
    for (w, b) in got.windows(2).zip(fx.beats.windows(2)) {
        let want = (b[1] - b[0]) / 1.05;
        let spacing = w[1] - w[0];
        assert!(
            (spacing - want).abs() <= 1.0,
            "spacing {spacing:.3} vs {want:.3}"
        );
    }
    // And the first click lands where 1.05× puts it. The detector fires up
    // to 2 frames early on a resampled click (its threshold is low on the
    // band-limited step); the head itself is pinned by the varispeed tests.
    assert!(
        (got[0] - fx.beats[0] / 1.05).abs() <= 2.0,
        "{} vs {}",
        got[0],
        fx.beats[0] / 1.05
    );
    assert!(onsets.right.is_empty());
}
