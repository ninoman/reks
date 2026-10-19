//! Converter building blocks (Phase 2.6 tasks 1–3) on the fixtures: the WAV,
//! AIFF and FLAC writers read back through Symphonia sample-exact, the native
//! decode stream passes samples through unchanged, and the resample and
//! dither paths keep length and stay within 1 LSB.

// Helpers outside #[test] fns unwrap too.
#![allow(clippy::unwrap_used)]

use std::fs::File;
use std::io::{BufWriter, Cursor};
use std::path::{Path, PathBuf};

use reks_audio::dither::{to_float, Requantiser};
use reks_audio::{
    probe, AiffWriter, Error, FlacWriter, Kind, NativeFormat, NativeStream, PcmSpec, PcmWriter,
    StreamResampler, WavWriter,
};
use reks_fixtures_gen as gen;

fn fixture(ext: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/audio")
        .join(format!("sine-click-128.{ext}"))
}

/// The 16-bit mono fixture's samples, from hound.
fn mono_16() -> Vec<i32> {
    hound::WavReader::open(fixture("wav"))
        .unwrap()
        .samples::<i16>()
        .map(|s| i32::from(s.unwrap()))
        .collect()
}

/// Writes the 24/96 stereo fixture (EXTENSIBLE, from hound) into `dir`.
fn hires_wav(dir: &Path) -> PathBuf {
    let path = dir.join("hires.wav");
    gen::write_wav_hires(BufWriter::new(File::create(&path).unwrap())).unwrap();
    path
}

/// Every sample of `path` through [`NativeStream`], and its format.
fn read_all(path: &Path) -> (NativeFormat, Vec<i32>) {
    let mut stream = NativeStream::open(path).unwrap();
    let mut all = Vec::new();
    let mut chunk = Vec::new();
    while stream.read(&mut chunk).unwrap() {
        all.extend_from_slice(&chunk);
    }
    assert_eq!(
        stream.frames_read() * u64::from(stream.format().channels),
        all.len() as u64
    );
    (*stream.format(), all)
}

#[derive(Clone, Copy, Debug)]
enum Target {
    Wav,
    Aiff,
    Flac,
}

/// Writes `samples` as `target` in pieces of `piece` frames.
fn write(dir: &Path, target: Target, spec: PcmSpec, samples: &[i32], piece: usize) -> PathBuf {
    fn run<W: PcmWriter<BufWriter<File>>>(mut w: W, samples: &[i32], piece: usize) {
        let ch = usize::from(w.spec().channels);
        for chunk in samples.chunks(piece * ch) {
            w.write(chunk).unwrap();
        }
        assert_eq!(w.frames(), (samples.len() / ch) as u64);
        w.finish().unwrap();
    }
    let ext = match target {
        Target::Wav => "wav",
        Target::Aiff => "aiff",
        Target::Flac => "flac",
    };
    let path = dir.join(format!(
        "out-{}-{}.{ext}",
        spec.bits_per_sample, spec.sample_rate
    ));
    let file = BufWriter::new(File::create(&path).unwrap());
    match target {
        Target::Wav => run(WavWriter::new(file, spec).unwrap(), samples, piece),
        Target::Aiff => run(AiffWriter::new(file, spec).unwrap(), samples, piece),
        Target::Flac => run(FlacWriter::new(file, spec).unwrap(), samples, piece),
    }
    path
}

#[test]
fn writers_round_trip_through_symphonia_sample_exact() {
    let dir = tempfile::tempdir().unwrap();
    let mono = mono_16();
    let hires = gen::render_hires();
    let cases = [
        (
            PcmSpec {
                sample_rate: 44_100,
                channels: 1,
                bits_per_sample: 16,
            },
            &mono,
        ),
        (
            PcmSpec {
                sample_rate: 96_000,
                channels: 2,
                bits_per_sample: 24,
            },
            &hires,
        ),
    ];
    for (spec, samples) in cases {
        for target in [Target::Wav, Target::Aiff, Target::Flac] {
            // 1,000-frame pieces: not a multiple of the FLAC block size.
            let path = write(dir.path(), target, spec, samples, 1_000);
            let (format, back) = read_all(&path);
            let what = format!("{target:?} {spec:?}");
            assert_eq!(format.sample_rate, spec.sample_rate, "{what}");
            assert_eq!(format.channels, spec.channels, "{what}");
            assert_eq!(format.bits_per_sample, spec.bits_per_sample, "{what}");
            assert!(!format.float, "{what}");
            let frames = (samples.len() / usize::from(spec.channels)) as u64;
            assert_eq!(format.frames, Some(frames), "{what}");
            assert!(back == *samples, "{what}: samples differ");

            let p = probe(&path).unwrap();
            let kind = match target {
                Target::Wav => Kind::Wav,
                Target::Aiff => Kind::Aiff,
                Target::Flac => Kind::Flac,
            };
            assert_eq!(p.kind, kind, "{what}");
            assert_eq!(p.bit_depth, Some(spec.bits_per_sample), "{what}");
        }
    }
}

#[test]
fn wav_24_bit_is_format_tag_1_not_extensible() {
    let dir = tempfile::tempdir().unwrap();
    let source = hires_wav(dir.path());
    // The source is the EXTENSIBLE form older players reject…
    assert_eq!(&std::fs::read(&source).unwrap()[20..22], &[0xFE, 0xFF]);
    let (format, samples) = read_all(&source);
    let spec = PcmSpec {
        sample_rate: format.sample_rate,
        channels: format.channels,
        bits_per_sample: format.bits_per_sample,
    };
    // …ours is a 16-byte `fmt ` chunk with tag 1, and the same samples.
    let out = write(dir.path(), Target::Wav, spec, &samples, 4_096);
    let bytes = std::fs::read(&out).unwrap();
    assert_eq!(&bytes[12..16], b"fmt ");
    assert_eq!(&bytes[16..20], &[16, 0, 0, 0]);
    assert_eq!(&bytes[20..22], &[1, 0]);
    assert_eq!(&bytes[34..36], &[24, 0]);
    assert_eq!(&bytes[36..40], b"data");
    assert_eq!(bytes.len(), 44 + 360_000 * 6);
    // hound reads it as plain PCM too.
    let hound_back: Vec<i32> = hound::WavReader::new(Cursor::new(bytes))
        .unwrap()
        .samples::<i32>()
        .map(Result::unwrap)
        .collect();
    assert!(hound_back == samples);
}

#[test]
fn wav_16_bit_matches_the_checked_in_fixture_byte_for_byte() {
    // hound writes 16-bit mono as plain tag-1 PCM, the same form as ours.
    let dir = tempfile::tempdir().unwrap();
    let spec = PcmSpec {
        sample_rate: 44_100,
        channels: 1,
        bits_per_sample: 16,
    };
    let path = write(dir.path(), Target::Wav, spec, &mono_16(), 4_096);
    let ours = std::fs::read(&path).unwrap();
    assert!(ours == std::fs::read(fixture("wav")).unwrap());
}

#[test]
fn flac_is_smaller_and_its_md5_is_checked() {
    let dir = tempfile::tempdir().unwrap();
    let spec = PcmSpec {
        sample_rate: 44_100,
        channels: 1,
        bits_per_sample: 16,
    };
    let path = write(dir.path(), Target::Flac, spec, &mono_16(), 4_096);
    let mut bytes = std::fs::read(&path).unwrap();
    let wav_len = std::fs::metadata(fixture("wav")).unwrap().len();
    assert!((bytes.len() as u64) < wav_len / 2, "{} bytes", bytes.len());

    // Flip one MD5 byte in STREAMINFO: the stream must fail at its end.
    bytes[8 + 18] ^= 0xFF;
    let bad = dir.path().join("bad-md5.flac");
    std::fs::write(&bad, &bytes).unwrap();
    let mut stream = NativeStream::open(&bad).unwrap();
    let mut chunk = Vec::new();
    let err = loop {
        match stream.read(&mut chunk) {
            Ok(true) => continue,
            Ok(false) => panic!("MD5 mismatch not reported"),
            Err(e) => break e,
        }
    };
    assert!(matches!(err, Error::Corrupt { .. }), "{err:?}");
    assert_eq!(stream.frames_read(), 1_323_000);
}

#[test]
fn native_stream_passes_fixture_samples_through() {
    let mono = mono_16();
    for ext in ["wav", "aiff", "flac"] {
        let (format, samples) = read_all(&fixture(ext));
        assert_eq!(
            format,
            NativeFormat {
                sample_rate: 44_100,
                channels: 1,
                bits_per_sample: 16,
                float: false,
                frames: Some(1_323_000),
            },
            "{ext}"
        );
        assert!(samples == mono, "{ext}: samples differ");
    }
    let dir = tempfile::tempdir().unwrap();
    let (format, samples) = read_all(&hires_wav(dir.path()));
    assert_eq!(
        (format.sample_rate, format.channels, format.bits_per_sample),
        (96_000, 2, 24)
    );
    assert!(samples == gen::render_hires());
}

#[test]
fn native_stream_reads_float_as_32_bit() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("float.wav");
    gen::write_wav_float(BufWriter::new(File::create(&path).unwrap())).unwrap();
    let (format, samples) = read_all(&path);
    assert!(format.float);
    assert_eq!(format.bits_per_sample, 32);
    assert_eq!((format.sample_rate, format.channels), (44_100, 2));
    let expected = gen::render_float();
    assert_eq!(samples.len(), expected.len());
    for (i, (&s, &f)) in samples.iter().zip(&expected).enumerate() {
        let back = to_float(s, 32);
        assert!(
            (back - f64::from(f)).abs() < 1e-9,
            "sample {i}: {back} vs {f}"
        );
    }
}

#[test]
fn resampling_96_to_48_keeps_length_and_beats() {
    let dir = tempfile::tempdir().unwrap();
    let mut stream = NativeStream::open(&hires_wav(dir.path())).unwrap();
    let bits = stream.format().bits_per_sample;
    let mut rs = StreamResampler::<f64>::new(96_000, 48_000, 2).unwrap();
    let mut q = Requantiser::new(24);
    let (mut chunk, mut float_in, mut float_out) = (Vec::new(), Vec::new(), Vec::new());
    let mut out = Vec::new();
    while stream.read(&mut chunk).unwrap() {
        float_in.clear();
        float_in.extend(chunk.iter().map(|&s| to_float(s, bits)));
        rs.push(&float_in, &mut float_out).unwrap();
        out.extend(float_out.drain(..).map(|x| q.float(x)));
    }
    rs.finish(&mut float_out).unwrap();
    out.extend(float_out.drain(..).map(|x| q.float(x)));
    assert_eq!(out.len(), 180_000 * 2);

    // Click onsets move to half the frame index (±1 frame).
    let onset = |samples: &[i32], from: usize| {
        (from..samples.len() / 2)
            .find(|&f| samples[f * 2].abs() > 3_500_000)
            .unwrap()
    };
    let source = gen::render_hires();
    for beat in [1usize, 4, 7] {
        let at_96 = gen::beat_onset_at(96_000, beat);
        let found_96 = onset(&source, at_96 - 100);
        let found_48 = onset(&out, (at_96 - 100) / 2);
        let expected = found_96 as f64 / 2.0;
        assert!(
            (found_48 as f64 - expected).abs() <= 1.0,
            "beat {beat}: {found_48} vs {expected}"
        );
    }

    // Written as 24/48 AIFF and read back unchanged.
    let spec = PcmSpec {
        sample_rate: 48_000,
        channels: 2,
        bits_per_sample: 24,
    };
    let path = write(dir.path(), Target::Aiff, spec, &out, 777);
    assert!(read_all(&path).1 == out);
}

#[test]
fn dither_to_16_bits_stays_within_one_lsb_of_the_source() {
    let dir = tempfile::tempdir().unwrap();
    let (format, source) = read_all(&hires_wav(dir.path()));
    let mut q = Requantiser::new(16);
    let dithered: Vec<i32> = source
        .iter()
        .map(|&s| q.int(s, format.bits_per_sample))
        .collect();
    let mut moved = 0usize;
    for (&s, &d) in source.iter().zip(&dithered) {
        let rounded = (f64::from(s) / 256.0).round() as i32;
        assert!((d - rounded).abs() <= 1, "{s}: {d} vs {rounded}");
        moved += usize::from(d != rounded);
    }
    assert!(moved > source.len() / 5, "dither inactive: {moved}");

    let spec = PcmSpec {
        sample_rate: 96_000,
        channels: 2,
        bits_per_sample: 16,
    };
    let path = write(dir.path(), Target::Wav, spec, &dithered, 4_096);
    let (format, back) = read_all(&path);
    assert_eq!(format.bits_per_sample, 16);
    assert!(back == dithered);
}
