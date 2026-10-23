//! Master tempo (key lock) render tests: with it on, the tempo fader changes
//! speed and keeps the key; with it off, pitch follows the speed.

use std::f64::consts::TAU;
use std::sync::Arc;

use reks_mix::{render, Control, DeckSource, Graph};

const RATE: u32 = 48_000;

fn tone(hz: f64, seconds: f64) -> Arc<DeckSource> {
    let frames = (f64::from(RATE) * seconds) as usize;
    let pcm = (0..frames)
        .flat_map(|i| {
            let v = (0.3 * (TAU * hz * i as f64 / f64::from(RATE)).sin() * 32_767.0) as i16;
            [v, v]
        })
        .collect();
    Arc::new(DeckSource::new(pcm, RATE))
}

/// `g` playing `src` on deck 0 at `tempo`, master tempo `on`.
fn playing(mut g: Graph, src: Arc<DeckSource>, tempo: f64, on: bool) -> Graph {
    g.apply(Control::Load { deck: 0, src });
    g.apply(Control::MasterTempo { deck: 0, on });
    g.apply(Control::Tempo {
        deck: 0,
        ratio: tempo,
    });
    g.apply(Control::Play { deck: 0 });
    g
}

fn left(g: &mut Graph, frames: usize) -> Vec<f64> {
    let mut out = vec![0.0f32; frames * 2];
    render(g, &mut out);
    out.as_chunks::<2>()
        .0
        .iter()
        .map(|f| f64::from(f[0]))
        .collect()
}

/// Largest change between consecutive samples.
fn max_step(x: &[f64]) -> f64 {
    x.windows(2)
        .map(|w| (w[1] - w[0]).abs())
        .fold(0.0, f64::max)
}

/// Magnitude of `x` at `hz` (Hann-windowed DFT bin at any frequency).
fn magnitude(x: &[f64], hz: f64) -> f64 {
    let n = x.len() as f64;
    let (mut re, mut im) = (0.0, 0.0);
    for (i, v) in x.iter().enumerate() {
        let w = 0.5 - 0.5 * (TAU * i as f64 / n).cos();
        let ph = TAU * hz * i as f64 / f64::from(RATE);
        re += v * w * ph.cos();
        im += v * w * ph.sin();
    }
    re.hypot(im)
}

/// The strongest frequency within ±20% of `near`, to 0.1 Hz.
fn dominant(x: &[f64], near: f64) -> f64 {
    let scan = |lo: f64, hi: f64, by: f64| {
        let steps = ((hi - lo) / by) as usize;
        (0..=steps)
            .map(|k| lo + k as f64 * by)
            .max_by(|a, b| magnitude(x, *a).total_cmp(&magnitude(x, *b)))
            .unwrap_or(near)
    };
    let coarse = scan(near * 0.8, near * 1.2, 2.0);
    scan(coarse - 2.0, coarse + 2.0, 0.1)
}

/// Dominant frequency of a `hz` tone played at `tempo`, after a second to
/// settle, over the next second.
fn played_frequency(g: Graph, hz: f64, tempo: f64, on: bool) -> f64 {
    let mut g = playing(g, tone(hz, 10.0), tempo, on);
    left(&mut g, RATE as usize);
    dominant(&left(&mut g, RATE as usize), hz)
}

#[test]
fn master_tempo_keeps_the_key_and_without_it_pitch_follows_the_tempo() {
    for hz in [440.0, 1_000.0, 90.0] {
        for tempo in [1.06, 0.94] {
            let kept = played_frequency(Graph::new(RATE).unwrap(), hz, tempo, true);
            assert!(
                (kept / hz - 1.0).abs() < 0.005,
                "{hz} Hz at {tempo} with master tempo: {kept} Hz"
            );
            let shifted = played_frequency(Graph::new(RATE).unwrap(), hz, tempo, false);
            assert!(
                (shifted / (hz * tempo) - 1.0).abs() < 0.005,
                "{hz} Hz at {tempo} without master tempo: {shifted} Hz"
            );
        }
    }
}

#[test]
fn with_master_tempo_the_head_still_moves_at_the_tempo() {
    let mut g = playing(Graph::new(RATE).unwrap(), tone(440.0, 10.0), 1.06, true);
    left(&mut g, 4_800);
    let from = g.state().decks[0].position;
    left(&mut g, 96_000);
    let moved = (g.state().decks[0].position - from) as f64;
    assert!((moved / 96_000.0 - 1.06).abs() < 1e-3, "{moved}");
}

#[test]
fn toggling_master_tempo_while_playing_does_not_click() {
    let mut g = playing(Graph::new(RATE).unwrap(), tone(440.0, 10.0), 1.06, false);
    left(&mut g, 9_600);
    // A 440 Hz sine at 0.3 changes by at most 0.3 × 2π × 466 / 48k ≈ 0.018
    // per frame; a click is a jump well above that.
    for on in [true, false, true] {
        g.apply(Control::MasterTempo { deck: 0, on });
        let worst = max_step(&left(&mut g, 9_600));
        assert!(worst < 0.04, "master tempo {on}: step {worst}");
    }
}

#[test]
fn master_tempo_follows_the_tempo_fader_and_loops() {
    let mut g = playing(Graph::new(RATE).unwrap(), tone(440.0, 10.0), 1.0, true);
    left(&mut g, 4_800);
    g.apply(Control::Tempo {
        deck: 0,
        ratio: 1.08,
    });
    left(&mut g, RATE as usize / 2);
    let kept = dominant(&left(&mut g, RATE as usize), 440.0);
    assert!((kept / 440.0 - 1.0).abs() < 0.005, "{kept}");
    // A seek back (as a loop wrap does) keeps it locked, and crossfades.
    g.apply(Control::Seek {
        deck: 0,
        frame: 24_000,
    });
    let worst = max_step(&left(&mut g, 4_800));
    assert!(worst < 0.04, "seek: step {worst}");
    let kept = dominant(&left(&mut g, RATE as usize), 440.0);
    assert!((kept / 440.0 - 1.0).abs() < 0.005, "{kept}");
}
