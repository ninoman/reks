//! Key detection on generated chord progressions (phase 3 task 5).
//!
//! Fixtures are synthesised here: harmonic tones, a root in the bass and a
//! close triad above, I–IV–V–I in major and i–iv–v–i (natural minor) in
//! minor, so all seven scale degrees sound and the tonic chord dominates.

use reks_analysis::key::{detect_key, from_keyfinder_index, KeyAnalysis};
use reks_core::{Key, Mode};

const RATE: u32 = 44_100;
const CHORD_S: f64 = 2.0;

fn midi_hz(note: i32) -> f64 {
    440.0 * 2f64.powf(f64::from(note - 69) / 12.0)
}

/// Chord roots and qualities (semitones above the tonic, is-major).
fn progression(mode: Mode) -> [(i32, bool); 4] {
    match mode {
        Mode::Major => [(0, true), (5, true), (7, true), (0, true)],
        Mode::Minor => [(0, false), (5, false), (7, false), (0, false)],
    }
}

/// Mono PCM of the key's progression, peak about 0.5.
fn chord_fixture(key: Key) -> Vec<f32> {
    let chord_len = (CHORD_S * f64::from(RATE)) as usize;
    let mut out = Vec::with_capacity(chord_len * 4);
    for (root_step, major) in progression(key.mode()) {
        let root = i32::from(key.tonic()) + root_step;
        let third = if major { 4 } else { 3 };
        // Bass root in octave 2, triad from octave 4 (MIDI 60 = C4).
        let notes = [
            36 + root % 12,
            60 + root % 12,
            60 + root % 12 + third,
            60 + root % 12 + 7,
        ];
        // Four harmonics per note (amplitude 1/h), as phasors rotated per
        // sample: much cheaper than `sin` in debug builds.
        let mut partials: Vec<(f64, f64, f64, f64, f64)> = Vec::new(); // re, im, step re, step im, amp
        for f in notes.into_iter().map(midi_hz) {
            for h in 1..=4 {
                let w = 2.0 * std::f64::consts::PI * f * f64::from(h) / f64::from(RATE);
                partials.push((1.0, 0.0, w.cos(), w.sin(), 1.0 / f64::from(h)));
            }
        }
        for n in 0..chord_len {
            let t = n as f64 / f64::from(RATE);
            // 10 ms attack, gentle exponential decay: no clicks between chords.
            let env = (t / 0.01).min(1.0) * (-t * 0.7).exp();
            let mut v = 0.0;
            for (re, im, cr, ci, amp) in &mut partials {
                v += *im * *amp;
                (*re, *im) = (*re * *cr - *im * *ci, *re * *ci + *im * *cr);
            }
            out.push((v * env * 0.08) as f32);
        }
    }
    out
}

/// Deterministic white noise in −0.5..0.5 (xorshift).
fn noise(len: usize) -> Vec<f32> {
    let mut state: u32 = 0x9E37_79B9;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (f64::from(state) / f64::from(u32::MAX) - 0.5) as f32
        })
        .collect()
}

#[test]
fn detects_all_24_keys_from_chord_progressions() {
    let mut wrong = Vec::new();
    for key in Key::all() {
        let a = detect_key(&chord_fixture(key), RATE).unwrap();
        if a.key != Some(key) {
            wrong.push(format!("{key} → {:?}", a.key.map(|k| k.canonical())));
        }
        assert!(a.confidence > 0.0 && a.confidence <= 1.0, "{key}: {a:?}");
        assert!(a.similarity > 0.5 && a.similarity <= 1.0, "{key}: {a:?}");
        // Max-normalised chroma; the tonic is prominent (harmonics also lift the fifth).
        assert_eq!(a.chroma.iter().copied().fold(0.0, f32::max), 1.0);
        assert!(
            a.chroma[usize::from(key.tonic())] > 0.8,
            "{key}: {:?}",
            a.chroma
        );
    }
    assert!(wrong.is_empty(), "misdetected: {wrong:?}");
}

#[test]
fn detection_reports_camelot_and_open_key() {
    let a = detect_key(&chord_fixture(Key::parse("Am").unwrap()), RATE).unwrap();
    let key = a.key.unwrap();
    assert_eq!(
        (key.camelot(), key.open_key()),
        ("8A".to_owned(), "1m".to_owned())
    );
    // F♯ major is spelled F#, Camelot 2B.
    let a = detect_key(&chord_fixture(Key::parse("Gb").unwrap()), RATE).unwrap();
    assert_eq!(
        a.key.map(|k| (k.canonical(), k.camelot())),
        Some(("F#", "2B".into()))
    );
}

#[test]
fn other_sample_rates_work() {
    // Same progression rendered at 48 kHz by resampling the time axis.
    let key = Key::parse("Eb").unwrap();
    let at_44 = chord_fixture(key);
    let len = at_44.len() * 48_000 / 44_100;
    let at_48: Vec<f32> = (0..len).map(|n| at_44[n * 44_100 / 48_000]).collect();
    let a = detect_key(&at_48, 48_000).unwrap();
    assert_eq!(a.key, Some(key));
}

#[test]
fn noise_does_not_panic_and_has_low_confidence() {
    let a: KeyAnalysis = detect_key(&noise(RATE as usize * 10), RATE).unwrap();
    assert!(a.key.is_some());
    assert!(a.confidence < 0.05, "{a:?}");
    assert!(a.confidence.is_finite() && a.similarity.is_finite());
}

#[test]
fn silence_and_tiny_inputs_do_not_panic() {
    assert_eq!(detect_key(&[], RATE).unwrap().key, None);
    assert_eq!(
        detect_key(&vec![0.0; RATE as usize * 5], RATE).unwrap().key,
        None
    );
    // A click shorter than one hop still yields a result.
    let mut click = vec![0.0f32; 300];
    click[0] = 1.0;
    let a = detect_key(&click, RATE).unwrap();
    assert!(a.confidence.is_finite());
}

/// Compares against the C++ libKeyFinder (target ≥ 95% agreement).
///
/// Needs the `fixtures/keyfinder/driver.cpp` binary (see its header for the
/// build line) at `REKS_KEYFINDER_DRIVER`. Run with
/// `cargo test -p reks-analysis --test key -- --ignored`. UNVERIFIED: not
/// yet run anywhere (see phase-3.md task 5).
#[test]
#[ignore = "needs the libkeyfinder C++ driver (REKS_KEYFINDER_DRIVER)"]
fn matches_cpp_libkeyfinder() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let driver = std::env::var("REKS_KEYFINDER_DRIVER")
        .expect("set REKS_KEYFINDER_DRIVER to the built fixtures/keyfinder driver");
    let mut fixtures: Vec<(String, Vec<f32>)> = Key::all()
        .map(|k| (k.canonical().to_owned(), chord_fixture(k)))
        .collect();
    fixtures.push(("noise".into(), noise(RATE as usize * 10)));
    fixtures.push(("silence".into(), vec![0.0; RATE as usize * 3]));

    let mut agree = 0;
    let mut report = Vec::new();
    for (name, pcm) in &fixtures {
        let mut child = Command::new(&driver)
            .arg(RATE.to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn driver");
        let bytes: Vec<u8> = pcm.iter().flat_map(|s| s.to_le_bytes()).collect();
        child.stdin.take().unwrap().write_all(&bytes).unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(out.status.success(), "driver failed on {name}");
        let index: u8 = String::from_utf8_lossy(&out.stdout).trim().parse().unwrap();
        let theirs = from_keyfinder_index(index);
        let ours = detect_key(pcm, RATE).unwrap().key;
        if ours == theirs {
            agree += 1;
        }
        report.push(format!("{name}: reks {ours:?} / libkeyfinder {theirs:?}"));
    }
    let ratio = f64::from(agree) / fixtures.len() as f64;
    assert!(
        ratio >= 0.95,
        "agreement {ratio:.3}:\n{}",
        report.join("\n")
    );
}
