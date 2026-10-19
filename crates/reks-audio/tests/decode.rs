//! Whole-track decode on the generated fixtures (`fixtures/audio/`, 30 s,
//! 44.1 kHz mono 16-bit, clicks at 128 BPM).

// Helpers outside #[test] fns unwrap too.
#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use reks_audio::overview::FRAMES_PER_COLUMN;
use reks_audio::{decode_stereo, Error};

const FRAMES: usize = 1_323_000;

fn fixture(ext: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/audio")
        .join(format!("sine-click-128.{ext}"))
}

fn wav_samples() -> Vec<i16> {
    hound::WavReader::open(fixture("wav"))
        .unwrap()
        .samples::<i16>()
        .map(Result::unwrap)
        .collect()
}

/// First frame at or after `from` whose left sample exceeds the sine's peak
/// (amplitude 0.25): the click onset.
fn onset(samples: &[i16], from: usize) -> usize {
    (from..samples.len() / 2)
        .find(|&f| i32::from(samples[f * 2]).abs() > 12_000)
        .unwrap()
}

#[test]
fn lossless_fixtures_decode_to_the_wav_samples() {
    let mono = wav_samples();
    assert_eq!(mono.len(), FRAMES);
    for ext in ["wav", "flac", "aiff"] {
        let d = decode_stereo(&fixture(ext), None).unwrap();
        assert_eq!(d.source_rate, 44_100, "{ext}");
        assert_eq!(d.pcm.sample_rate, 44_100, "{ext}");
        assert_eq!(d.pcm.frames(), FRAMES, "{ext}");
        assert_eq!(d.pcm.duration_ms(), 30_000, "{ext}");
        for (i, frame) in d.pcm.samples.as_chunks::<2>().0.iter().enumerate() {
            assert_eq!(*frame, [mono[i], mono[i]], "{ext} frame {i}");
        }
    }
}

#[test]
fn lossy_fixtures_decode_to_about_thirty_seconds() {
    // The MP3's LAME header trims delay/padding; AAC priming stays (~23 ms).
    for (ext, ms) in [("mp3", 30_000..=30_001), ("m4a", 30_000..=30_030)] {
        let d = decode_stereo(&fixture(ext), None).unwrap();
        assert!(ms.contains(&d.pcm.duration_ms()), "{ext}: {:?}", d.pcm);
    }
}

#[test]
fn resampling_keeps_length_and_beat_positions() {
    let src = decode_stereo(&fixture("wav"), None).unwrap();
    let up = decode_stereo(&fixture("wav"), Some(48_000)).unwrap();
    assert_eq!(up.source_rate, 44_100);
    assert_eq!(up.pcm.sample_rate, 48_000);
    assert_eq!(up.pcm.frames(), 1_440_000);
    // The overview stays at the source rate.
    assert_eq!(up.overview, src.overview);

    // Beat n starts at (n * 2_646_000 + 64) / 128 frames at 44.1 kHz.
    for beat in [1usize, 32, 63] {
        let at_44 = (beat * 2_646_000 + 64) / 128;
        let found_44 = onset(&src.pcm.samples, at_44 - 50);
        let found_48 = onset(&up.pcm.samples, (at_44 - 50) * 48_000 / 44_100);
        let expected_48 = found_44 as f64 * 48_000.0 / 44_100.0;
        assert!(
            (found_48 as f64 - expected_48).abs() <= 2.0,
            "beat {beat}: {found_48} vs {expected_48:.1}"
        );
    }
}

#[test]
fn overview_has_one_column_per_thousand_frames() {
    let d = decode_stereo(&fixture("wav"), None).unwrap();
    assert_eq!(d.overview.len(), FRAMES.div_ceil(FRAMES_PER_COLUMN) * 2);
    let peak = d.overview.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    // Sine 0.25 plus click 0.6.
    assert!((0.8..=0.86).contains(&peak), "peak {peak}");
}

#[test]
fn decode_rejects_non_audio() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("notes.wav");
    std::fs::write(&path, b"definitely not audio").unwrap();
    let err = decode_stereo(&path, None).unwrap_err();
    assert!(
        matches!(err, Error::Unsupported { .. } | Error::Decode { .. }),
        "{err:?}"
    );
}
