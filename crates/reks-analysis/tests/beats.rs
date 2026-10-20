//! Beat This! on generated fixtures (phase 3 task 2): beats within 10 ms of
//! the generated onsets, downbeats on every fourth beat of a 4/4 pattern.

// Helpers outside #[test] fns unwrap too.
#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use reks_analysis::{AnalysisInput, BeatList, BeatTracker, ModelPaths};
use reks_fixtures_gen::{bars, beat_onset_at};

fn tracker() -> BeatTracker {
    BeatTracker::load(&ModelPaths::small(&ModelPaths::dev_dir())).unwrap()
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/audio")
        .join(name)
}

/// Index of the generated beat each tracked beat is nearest to, and the
/// worst distance in ms.
fn match_beats(list: &BeatList, beats: usize) -> (Vec<usize>, f64) {
    assert_eq!(list.sample_rate, 44_100);
    let mut worst = 0.0f64;
    let indices = list
        .beats
        .iter()
        .map(|b| {
            let n = (0..=beats)
                .min_by_key(|&n| (b.sample - beat_onset_at(44_100, n) as i64).abs())
                .unwrap();
            let err = (b.sample - beat_onset_at(44_100, n) as i64).abs() as f64 / 44.1;
            worst = worst.max(err);
            n
        })
        .collect();
    (indices, worst)
}

/// Every generated beat found once, each within 10 ms.
fn assert_beats_on_time(list: &BeatList, beats: usize) -> Vec<usize> {
    let (indices, worst) = match_beats(list, beats);
    // Beat `beats` falls on the last sample; the tracker may or may not mark it.
    let found: Vec<usize> = indices.iter().copied().filter(|&n| n < beats).collect();
    assert_eq!(found, (0..beats).collect::<Vec<_>>(), "beats found");
    assert!(worst <= 10.0, "worst beat {worst:.2} ms off");
    found
}

#[test]
fn click_track_beats_within_10_ms() {
    let mut tracker = tracker();
    for name in ["sine-click-128.wav", "sine-click-128.mp3"] {
        let input = AnalysisInput::from_file(&fixture(name)).unwrap();
        let list = tracker.track(&input).unwrap();
        assert_beats_on_time(&list, reks_fixtures_gen::BEATS);
        assert!(
            list.confidence > 0.9,
            "{name}: confidence {}",
            list.confidence
        );
    }
}

#[test]
fn four_four_pattern_has_downbeats_on_bar_starts() {
    let mix: Vec<f32> = bars::render_bars(44_100, 16)
        .into_iter()
        .map(|s| s as f32)
        .collect();
    let input = AnalysisInput::from_mono(mix, 44_100).unwrap();
    let list = tracker().track(&input).unwrap();
    let found = assert_beats_on_time(&list, 64);
    for (beat, n) in list.beats.iter().zip(found) {
        assert_eq!(beat.downbeat, n % 4 == 0, "beat {n}");
    }
    let downbeats = list.downbeats_s();
    assert!(
        (downbeats[1] - 4.0 * 60.0 / 128.0).abs() < 0.01,
        "{downbeats:?}"
    );
}

#[test]
fn tracker_runs_on_a_worker_thread() {
    // How the app calls it: inside spawn_blocking.
    let input = AnalysisInput::from_file(&fixture("sine-click-128.flac")).unwrap();
    let mut tracker = tracker();
    let list = std::thread::spawn(move || tracker.track(&input))
        .join()
        .unwrap()
        .unwrap();
    assert_beats_on_time(&list, reks_fixtures_gen::BEATS);
}

#[test]
fn silence_and_noise_do_not_crash_and_score_low() {
    let mut tracker = tracker();
    let silence = AnalysisInput::from_mono(vec![0.0; 44_100 * 10], 44_100).unwrap();
    let list = tracker.track(&silence).unwrap();
    assert!(
        list.beats.is_empty(),
        "{} beats in silence",
        list.beats.len()
    );
    assert_eq!(list.confidence, 0.0);

    let mut state = 1u32;
    let noise: Vec<f32> = (0..44_100 * 10)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (f64::from(state) / f64::from(u32::MAX) - 0.5) as f32 * 0.5
        })
        .collect();
    let list = tracker
        .track(&AnalysisInput::from_mono(noise, 44_100).unwrap())
        .unwrap();
    assert!(
        list.confidence < 0.8,
        "noise: confidence {}",
        list.confidence
    );

    let empty = AnalysisInput::from_mono(Vec::new(), 44_100).unwrap();
    assert!(tracker.track(&empty).unwrap().beats.is_empty());
}
