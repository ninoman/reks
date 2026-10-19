//! `convert()` (Phase 2.6 task 4) per source × target on the fixtures:
//! same rate and depth is sample-exact, depth and rate changes keep length
//! and stay close to the signal, tags and artwork are copied, lossy and
//! multichannel sources are refused, and cancel or failure leaves no temp file.

// Helpers outside #[test] fns unwrap too.
#![allow(clippy::unwrap_used, clippy::panic)]

use std::cell::RefCell;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use lofty::config::WriteOptions;
use lofty::file::TaggedFileExt;
use lofty::picture::{MimeType, Picture, PictureType};
use lofty::tag::TagExt;
use reks_audio::{
    convert, embedded_artwork, pcm_hash, probe, tags, CancelToken, ConvertSpec, Depth, Error,
    NativeStream, Output, Rate, Target,
};
use reks_fixtures_gen as gen;

const TARGETS: [Target; 3] = [Target::Wav, Target::Aiff, Target::Flac];

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/audio")
        .join(name)
}

fn ext(target: Target) -> &'static str {
    match target {
        Target::Wav => "wav",
        Target::Aiff => "aiff",
        Target::Flac => "flac",
    }
}

fn spec(target: Target, depth: Depth, rate: Rate) -> ConvertSpec {
    ConvertSpec {
        target,
        depth,
        rate,
    }
}

/// Converts `src` into `dir`, checking progress runs from 0 to 1 and only goes up.
fn run(src: &Path, dir: &Path, spec: ConvertSpec) -> (PathBuf, Output) {
    let name = src.file_name().unwrap().to_string_lossy();
    let out = dir.join(format!(
        "{name}-{:?}-{:?}.{}.reks-tmp",
        spec.depth,
        spec.rate,
        ext(spec.target)
    ));
    let seen = RefCell::new(Vec::new());
    let output = convert(
        src,
        &out,
        &spec,
        &|p| seen.borrow_mut().push(p),
        &CancelToken::new(),
    )
    .unwrap_or_else(|e| panic!("{} -> {spec:?}: {e}", src.display()));
    let seen = seen.into_inner();
    assert_eq!(seen.first(), Some(&0.0));
    assert_eq!(seen.last(), Some(&1.0));
    assert!(seen.windows(2).all(|w| w[0] < w[1]), "{seen:?}");
    assert!(seen.len() <= 102, "{} progress calls", seen.len());
    (out, output)
}

/// Every sample of `path` and its stream format.
fn samples(path: &Path) -> (u32, u16, u8, Vec<i32>) {
    let mut stream = NativeStream::open(path).unwrap();
    let mut all = Vec::new();
    let mut chunk = Vec::new();
    while stream.read(&mut chunk).unwrap() {
        all.extend_from_slice(&chunk);
    }
    let f = *stream.format();
    (f.sample_rate, f.channels, f.bits_per_sample, all)
}

/// The header forms older players load: WAV `fmt` tag 1 with a 16-byte
/// `fmt`, AIFF not AIFF-C, FLAC magic.
fn assert_plain_header(path: &Path, target: Target) {
    let bytes = std::fs::read(path).unwrap();
    match target {
        Target::Wav => {
            assert_eq!(&bytes[0..4], b"RIFF");
            assert_eq!(&bytes[8..16], b"WAVEfmt ");
            assert_eq!(&bytes[16..20], &16u32.to_le_bytes());
            assert_eq!(&bytes[20..22], &1u16.to_le_bytes());
        }
        Target::Aiff => {
            assert_eq!(&bytes[0..4], b"FORM");
            assert_eq!(&bytes[8..12], b"AIFF");
        }
        Target::Flac => assert_eq!(&bytes[0..4], b"fLaC"),
    }
}

fn hires_wav(dir: &Path) -> PathBuf {
    let path = dir.join("hires.wav");
    gen::write_wav_hires(BufWriter::new(File::create(&path).unwrap())).unwrap();
    path
}

fn float_wav(dir: &Path) -> PathBuf {
    let path = dir.join("float.wav");
    gen::write_wav_float(BufWriter::new(File::create(&path).unwrap())).unwrap();
    path
}

fn leftovers(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.to_string_lossy().ends_with(".reks-tmp"))
        .collect()
}

#[test]
fn same_rate_and_depth_is_sample_exact() {
    let dir = tempfile::tempdir().unwrap();
    let hires = hires_wav(dir.path());
    // A 24/96 FLAC made by the converter itself, as a further source.
    let made = dir.path().join("made");
    std::fs::create_dir(&made).unwrap();
    let (hires_flac, _) = run(&hires, &made, spec(Target::Flac, Depth::Keep, Rate::Keep));
    let sources = [
        (fixture("sine-click-128.wav"), 16, 1_323_000),
        (fixture("sine-click-128.aiff"), 16, 1_323_000),
        (fixture("sine-click-128.flac"), 16, 1_323_000),
        (fixture("sine-click-128-hires.m4a"), 24, 96_000),
        (hires.clone(), 24, 360_000),
        (hires_flac, 24, 360_000),
    ];
    for (src, bits, frames) in sources {
        let before = std::fs::read(&src).unwrap();
        let src_hash = pcm_hash(&src).unwrap();
        let src_rate = probe(&src).unwrap().sample_rate;
        for target in TARGETS {
            let (out, output) = run(&src, dir.path(), spec(target, Depth::Keep, Rate::Keep));
            let what = format!("{} -> {target:?}", src.display());
            assert_eq!(output.frames, frames, "{what}");
            assert_eq!(output.bit_depth, bits, "{what}");
            assert_eq!(output.sample_rate, src_rate, "{what}");
            assert_eq!(output.pcm_hash, src_hash, "{what}");
            assert_eq!(pcm_hash(&out).unwrap(), src_hash, "{what}");
            assert_plain_header(&out, target);
        }
        assert_eq!(std::fs::read(&src).unwrap(), before, "source changed");
    }
}

#[test]
fn alac_fixture_is_the_hires_signal() {
    // The ALAC fixture is the first second of the generated 24/96 signal.
    let (rate, channels, bits, alac) = samples(&fixture("sine-click-128-hires.m4a"));
    assert_eq!((rate, channels, bits), (96_000, 2, 24));
    assert_eq!(alac, gen::render_hires()[..alac.len()]);
    assert_eq!(alac.len(), 2 * 96_000);
}

#[test]
fn widening_to_24_bits_is_exact() {
    let dir = tempfile::tempdir().unwrap();
    let src = fixture("sine-click-128.flac");
    let (_, _, _, original) = samples(&src);
    for target in TARGETS {
        let (out, output) = run(&src, dir.path(), spec(target, Depth::Bits24, Rate::Keep));
        assert_eq!(output.bit_depth, 24);
        let (_, channels, bits, wide) = samples(&out);
        assert_eq!((channels, bits), (1, 24));
        assert!(original.iter().zip(&wide).all(|(&a, &b)| a << 8 == b));
        assert_eq!(wide.len(), original.len());
    }
}

#[test]
fn float_wav_becomes_24_bit() {
    let dir = tempfile::tempdir().unwrap();
    let src = float_wav(dir.path());
    let expected: Vec<f64> = gen::render_float()
        .iter()
        .map(|&x| f64::from(x) * f64::from(1 << 23))
        .collect();
    for target in TARGETS {
        let (out, output) = run(&src, dir.path(), spec(target, Depth::Keep, Rate::Keep));
        assert_eq!(
            (output.frames, output.sample_rate, output.bit_depth),
            (165_375, 44_100, 24)
        );
        assert_plain_header(&out, target);
        let (_, channels, _, got) = samples(&out);
        assert_eq!(channels, 2);
        assert_eq!(got.len(), expected.len());
        for (&g, &e) in got.iter().zip(&expected) {
            assert!((f64::from(g) - e).abs() <= 0.5, "{g} vs {e}");
        }
    }
}

#[test]
fn older_players_preset_from_hires() {
    // 24/96 → 24/48 AIFF (the Older players preset), and 16/44.1 WAV with dither.
    let dir = tempfile::tempdir().unwrap();
    let wav = hires_wav(dir.path());
    let sources = [
        (wav, 360_000u64, 96_000u64),
        (fixture("sine-click-128-hires.m4a"), 96_000, 96_000),
    ];
    for (src, frames, from) in sources {
        for (target, depth, rate, to, bits) in [
            (Target::Aiff, Depth::Keep, Rate::Hz48000, 48_000u32, 24u8),
            (Target::Wav, Depth::Bits16, Rate::Hz44100, 44_100, 16),
            (Target::Flac, Depth::Keep, Rate::Hz44100, 44_100, 24),
        ] {
            let (out, output) = run(&src, dir.path(), spec(target, depth, rate));
            let len = (frames * u64::from(to)).div_ceil(from);
            assert_eq!(
                (output.frames, output.sample_rate, output.bit_depth),
                (len, to, bits)
            );
            assert_plain_header(&out, target);
            assert_eq!(pcm_hash(&out).unwrap(), output.pcm_hash);
            // Away from the edges, the output is the signal rendered at the new rate.
            let (_, _, _, got) = samples(&out);
            let reference = gen::render_stereo(to);
            let scale = f64::from(1u32 << (bits - 1));
            let n = got.len().min(reference.len());
            // Clicks start abruptly, so the band-limited output rings around
            // them; compare the steady sines between clicks.
            let guard = to as usize / 50; // 20 ms
            let near_click = |frame: usize| {
                (0..9).any(|b| {
                    let onset = gen::beat_onset_at(to, b);
                    frame + guard >= onset && frame < onset + to as usize / 100 + guard
                })
            };
            let worst = (2..n / 2 - guard)
                .filter(|&f| !near_click(f))
                .flat_map(|f| [2 * f, 2 * f + 1])
                .map(|i| (f64::from(got[i]) / scale - reference[i]).abs())
                .fold(0.0, f64::max);
            assert!(worst < 1e-3, "{target:?} {to}: worst error {worst}");
        }
    }
}

#[test]
fn dithered_output_is_deterministic() {
    let dir = tempfile::tempdir().unwrap();
    let src = hires_wav(dir.path());
    let a = dir.path().join("a.wav");
    let b = dir.path().join("b.wav");
    let s = spec(Target::Wav, Depth::Bits16, Rate::Keep);
    let none = CancelToken::new();
    let first = convert(&src, &a, &s, &|_| {}, &none).unwrap();
    let second = convert(&src, &b, &s, &|_| {}, &none).unwrap();
    assert_eq!(first, second);
    assert_eq!(std::fs::read(a).unwrap(), std::fs::read(b).unwrap());
}

#[test]
fn copies_tags_and_artwork() {
    let dir = tempfile::tempdir().unwrap();
    // The FLAC fixture's tags plus a front cover.
    let src = dir.path().join("tagged.flac");
    std::fs::copy(fixture("sine-click-128.flac"), &src).unwrap();
    let cover = b"\x89PNG\r\n\x1a\n not really a png".to_vec();
    let mut file = lofty::read_from_path(&src).unwrap();
    let tag = file.primary_tag_mut().unwrap();
    tag.push_picture(
        Picture::unchecked(cover.clone())
            .pic_type(PictureType::CoverFront)
            .mime_type(MimeType::Png)
            .build(),
    );
    tag.save_to_path(&src, WriteOptions::default()).unwrap();

    for target in TARGETS {
        let (out, _) = run(&src, dir.path(), spec(target, Depth::Keep, Rate::Keep));
        let t = tags(&out).unwrap();
        assert_eq!(t.title.as_deref(), Some("Sine Click 128"), "{target:?}");
        assert_eq!(t.artist.as_deref(), Some("reks fixtures"), "{target:?}");
        let art = embedded_artwork(&out).unwrap().unwrap();
        assert_eq!(art.data, cover, "{target:?}");
        assert_plain_header(&out, target);
    }
    // AIFF carries a title only; WAV has no tags, and nothing is added.
    for target in TARGETS {
        let (out, _) = run(
            &fixture("sine-click-128.aiff"),
            dir.path(),
            spec(target, Depth::Keep, Rate::Keep),
        );
        let t = tags(&out).unwrap();
        assert_eq!(t.title.as_deref(), Some("Sine Click 128"), "{target:?}");
        let (out, _) = run(
            &fixture("sine-click-128.wav"),
            dir.path(),
            spec(target, Depth::Keep, Rate::Keep),
        );
        assert_eq!(tags(&out).unwrap(), reks_audio::Tags::default());
    }
}

#[test]
fn refuses_lossy_and_multichannel_sources() {
    let dir = tempfile::tempdir().unwrap();
    let s = spec(Target::Aiff, Depth::Keep, Rate::Keep);
    let none = CancelToken::new();
    for name in ["sine-click-128.mp3", "sine-click-128.m4a"] {
        let out = dir.path().join(format!("{name}.reks-tmp"));
        let err = convert(&fixture(name), &out, &s, &|_| {}, &none).unwrap_err();
        assert!(matches!(err, Error::Unsupported { .. }), "{name}: {err}");
        assert!(!out.exists());
    }
    let surround = dir.path().join("3ch.wav");
    let mut w = hound::WavWriter::create(
        &surround,
        hound::WavSpec {
            channels: 3,
            sample_rate: 44_100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )
    .unwrap();
    for i in 0..3_000 {
        w.write_sample(i as i16).unwrap();
    }
    w.finalize().unwrap();
    let out = dir.path().join("3ch.aiff.reks-tmp");
    let err = convert(&surround, &out, &s, &|_| {}, &none).unwrap_err();
    assert!(matches!(err, Error::Unsupported { .. }), "{err}");
    assert!(!out.exists());
}

#[test]
fn cancel_leaves_no_temp_file() {
    let dir = tempfile::tempdir().unwrap();
    let src = fixture("sine-click-128.flac");
    for target in TARGETS {
        // Before the start, and halfway through writing.
        for at in [0.0f32, 0.5] {
            let cancel = CancelToken::new();
            if at == 0.0 {
                cancel.cancel();
            }
            let out = dir.path().join(format!("{at}.{}.reks-tmp", ext(target)));
            let s = spec(target, Depth::Bits24, Rate::Hz48000);
            let progress = |p: f32| {
                if p >= at {
                    cancel.cancel();
                }
            };
            let err = convert(&src, &out, &s, &progress, &cancel).unwrap_err();
            assert!(matches!(err, Error::Cancelled { .. }), "{err}");
            assert!(!out.exists(), "{target:?} at {at}");
        }
    }
    assert!(leftovers(dir.path()).is_empty());
}

#[test]
fn never_overwrites_an_existing_file() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("taken.wav.reks-tmp");
    std::fs::write(&out, b"keep me").unwrap();
    let err = convert(
        &fixture("sine-click-128.flac"),
        &out,
        &spec(Target::Wav, Depth::Keep, Rate::Keep),
        &|_| {},
        &CancelToken::new(),
    )
    .unwrap_err();
    assert!(matches!(err, Error::Io { .. }), "{err}");
    assert_eq!(std::fs::read(&out).unwrap(), b"keep me");
}

#[test]
fn truncated_source_fails_and_cleans_up() {
    let dir = tempfile::tempdir().unwrap();
    // A FLAC cut short: its STREAMINFO promises more frames than it holds.
    let bytes = std::fs::read(fixture("sine-click-128.flac")).unwrap();
    let src = dir.path().join("cut.flac");
    std::fs::write(&src, &bytes[..bytes.len() / 2]).unwrap();
    let out = dir.path().join("cut.aiff.reks-tmp");
    let err = convert(
        &src,
        &out,
        &spec(Target::Aiff, Depth::Keep, Rate::Keep),
        &|_| {},
        &CancelToken::new(),
    )
    .unwrap_err();
    assert!(
        matches!(err, Error::Corrupt { .. } | Error::Decode { .. }),
        "{err}"
    );
    assert!(!out.exists());
}
