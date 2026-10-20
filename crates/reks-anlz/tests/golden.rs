//! Golden comparisons with rekordbox's own ANLZ files.
//!
//! `rekordbox6_demo_tracks` (always runs) uses `fixtures/rekordbox/
//! ref-rb6-demo` (no audio): we build from the constant grid fitted to the
//! reference beats (what rekordbox's XML would carry), its cues and
//! duration, then require the same section list and lengths, identical
//! PPTH, PVBR, PCOB and PCO2 bytes, and PQTZ entries with the same beat
//! numbers and tempo and times within 1 ms. rekordbox's beat times are not
//! exactly linear in its tempo: on 8 of 368 and 9 of 257 beats they differ
//! from the best-fitting line by a rounding step, alternating near the
//! crossover, so no constant grid reproduces them all exactly. Waveforms there
//! come from columns rebuilt from rekordbox's own PWV5, so their error is
//! reported, not judged.
//!
//! `rekordbox_reference_tracks` (ignored) runs the same comparison from real
//! audio through our waveform analysis, for reference sets that can't be
//! committed (commercial audio). Point `REKS_ANLZ_REF` at a folder with one
//! subfolder per track holding the audio file and rekordbox's
//! `ANLZ0000.DAT/.EXT/.2EX` for it, then run
//! `cargo test -p reks-anlz --test golden -- --ignored --nocapture`.
//! Waveform error thresholds come with calibration (phase 3 task 8); until
//! then the test prints the errors.

#![allow(clippy::print_stderr, clippy::unwrap_used, clippy::panic)]

mod common;

use std::path::{Path, PathBuf};

use reks_analysis::grid::Grid;
use reks_analysis::waveform::{columns_from_interleaved, WaveformColumns};
use reks_anlz::compare::{layout, waveform_errors, SectionLayout};
use reks_anlz::grid::pqtz_beats;
use reks_anlz::read;
use reks_anlz::validate::{validate_set, Expect};
use reks_anlz::waveform::unpack_color;
use reks_anlz::write::{self, PqtzBeat};
use reks_anlz::{build, AnlzInput, AnlzSet, AnlzTarget, Waveforms, PLACEHOLDER_CALIBRATION};

const RATE: u32 = 44_100;

struct Reference {
    name: String,
    dat: Vec<u8>,
    ext: Vec<u8>,
    ex2: Vec<u8>,
}

impl Reference {
    fn load(dir: &Path) -> Self {
        let read = |ext: &str| std::fs::read(dir.join(format!("ANLZ0000.{ext}"))).unwrap();
        Self {
            name: dir
                .strip_prefix(dir.parent().and_then(Path::parent).unwrap_or(dir))
                .unwrap_or(dir)
                .display()
                .to_string(),
            dat: read("DAT"),
            ext: read("EXT"),
            ex2: read("2EX"),
        }
    }
}

fn demo_dirs() -> Vec<PathBuf> {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/rekordbox/ref-rb6-demo/USBANLZ");
    vec![root.join("P016/0000875E"), root.join("P053/0001D21F")]
}

/// Least-squares line through the beat times: (first beat ms, period ms).
fn fit(beats: &[PqtzBeat]) -> (f64, f64) {
    let n = beats.len() as f64;
    let (mut sx, mut sy, mut sxx, mut sxy) = (0.0, 0.0, 0.0, 0.0);
    for (i, b) in beats.iter().enumerate() {
        let (x, y) = (i as f64, f64::from(b.time_ms));
        sx += x;
        sy += y;
        sxx += x * x;
        sxy += x * y;
    }
    let period = (n * sxy - sx * sy) / (n * sxx - sx * sx);
    ((sy - period * sx) / n, period)
}

/// The constant grid rekordbox's XML would carry for these beats.
fn constant_grid(beats: &[PqtzBeat]) -> Grid {
    let (first_ms, period_ms) = fit(beats);
    Grid::Constant {
        bpm: 60_000.0 / period_ms,
        first_beat: (first_ms * f64::from(RATE) / 1000.0).round() as i64,
        first_beat_in_bar: beats[0].beat_number as u8,
    }
}

/// Columns rebuilt from rekordbox's PWV5 (colour → bands, height → mono).
fn columns_from_pwv5(ext: &[u8]) -> WaveformColumns {
    let sections = read::sections(ext).unwrap();
    let mut c = WaveformColumns::default();
    for e in common::find(&sections, write::PWV5)
        .payload()
        .as_chunks::<2>()
        .0
    {
        let col = unpack_color(u16::from_be_bytes(*e));
        let band = |v: u8| (u32::from(v) * 255 / 7) as u8;
        c.low.push(band(col.red));
        c.mid.push(band(col.green));
        c.high.push(band(col.blue));
        c.mono.push(col.height);
    }
    c
}

/// Builds our set for `r` from `beats`, its cues and `waves`.
fn build_like(r: &Reference, beats: &[PqtzBeat], duration_ms: u64, waves: &Waveforms) -> AnlzSet {
    let path = common::path(&r.dat);
    let cues = common::cues(&r.ext);
    build(
        &AnlzInput {
            contents_path: &path,
            duration_ms,
            beats,
            cues: &cues,
            waveforms: waves,
            mp3_audio_frames: common::mp3_frames(&r.dat),
        },
        AnlzTarget::Cdj3000,
    )
    .unwrap()
}

/// Section lists equal (minus rekordbox's PSSI), lengths equal except the
/// full PQT2 rekordbox writes, and `exact` sections byte-identical.
fn compare_structure(name: &str, ours: &[u8], theirs: &[u8], exact: &[[u8; 4]]) {
    let strip = |l: Vec<SectionLayout>| -> Vec<SectionLayout> {
        l.into_iter().filter(|s| s.tag != "PSSI").collect()
    };
    let (a, b) = (strip(layout(ours).unwrap()), strip(layout(theirs).unwrap()));
    let tags = |l: &[SectionLayout]| l.iter().map(|s| s.tag.clone()).collect::<Vec<_>>();
    assert_eq!(tags(&a), tags(&b), "{name}: section list");
    for (x, y) in a.iter().zip(&b) {
        if x.tag != "PQT2" {
            assert_eq!(x, y, "{name}: {} lengths", x.tag);
        }
    }
    let (sa, sb) = (
        read::sections(ours).unwrap(),
        read::sections(theirs).unwrap(),
    );
    for (x, y) in sa.iter().zip(&sb) {
        if exact.contains(&x.tag) {
            assert_eq!(x.bytes, y.bytes, "{name}: {} bytes", x.tag_str());
        }
    }
}

fn report(name: &str, ours: &AnlzSet, r: &Reference) {
    let pairs = [
        (&ours.dat, &r.dat),
        (&ours.ext, &r.ext),
        (ours.ex2.as_ref().unwrap(), &r.ex2),
    ];
    for (a, b) in pairs {
        for e in waveform_errors(a, b).unwrap() {
            assert!(e.error.iter().all(|v| (0.0..=1.0).contains(v)));
            assert_eq!(e.columns.0, e.columns.1, "{name}: {} columns", e.tag);
            let per: Vec<String> = e
                .channels
                .iter()
                .zip(&e.error)
                .map(|(c, v)| format!("{c} {v:.3}"))
                .collect();
            eprintln!(
                "{name}: {} error {:.3} ({})",
                e.tag,
                e.mean(),
                per.join(", ")
            );
        }
    }
}

const EXACT: [[u8; 4]; 4] = [write::PPTH, write::PVBR, write::PCOB, write::PCO2];

#[test]
fn rekordbox6_demo_tracks() {
    for dir in demo_dirs() {
        let r = Reference::load(&dir);
        let theirs = common::beats(&r.dat);
        // Duration from the MP3 frame count rekordbox stored in PVBR.
        let frames = u64::from(common::mp3_frames(&r.dat).unwrap());
        let duration_ms = frames * 1152 * 1000 / u64::from(RATE);
        let beats = pqtz_beats(&constant_grid(&theirs), RATE, duration_ms).unwrap();
        assert_eq!(beats.len(), theirs.len(), "{}: beat count", r.name);
        let mut exact = 0;
        for (i, (a, b)) in beats.iter().zip(&theirs).enumerate() {
            assert_eq!(
                (a.beat_number, a.tempo),
                (b.beat_number, b.tempo),
                "{}: beat {i}",
                r.name
            );
            assert!(
                a.time_ms.abs_diff(b.time_ms) <= 1,
                "{}: beat {i} time",
                r.name
            );
            exact += usize::from(a.time_ms == b.time_ms);
        }
        assert!(
            exact * 100 >= theirs.len() * 95,
            "{}: {exact} exact times",
            r.name
        );
        eprintln!(
            "{}: PQTZ {exact}/{} times exact, rest ±1 ms",
            r.name,
            theirs.len()
        );

        // rekordbox's own files pass our validator.
        let expect = Expect {
            path: Some(&common::path(&r.dat)),
            duration_ms: Some(duration_ms),
        };
        let issues = validate_set(&r.dat, &r.ext, Some(&r.ex2), expect);
        assert!(issues.is_empty(), "{}: {issues:?}", r.name);

        let waves =
            Waveforms::encode(&columns_from_pwv5(&r.ext), &PLACEHOLDER_CALIBRATION).unwrap();
        let ours = build_like(&r, &beats, duration_ms, &waves);
        compare_structure(&r.name, &ours.dat, &r.dat, &EXACT);
        compare_structure(&r.name, &ours.ext, &r.ext, &EXACT);
        compare_structure(
            &r.name,
            ours.ex2.as_deref().unwrap(),
            &r.ex2,
            &[write::PPTH],
        );
        report(&r.name, &ours, &r);
    }
}

#[test]
#[ignore = "needs rekordbox reference exports with audio in REKS_ANLZ_REF (not committable)"]
fn rekordbox_reference_tracks() {
    let Some(root) = std::env::var_os("REKS_ANLZ_REF") else {
        panic!("set REKS_ANLZ_REF to a folder of track folders (audio + ANLZ0000.*)");
    };
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.join("ANLZ0000.DAT").exists())
        .collect();
    dirs.sort();
    assert!(!dirs.is_empty(), "no track folders with ANLZ0000.DAT");
    for dir in dirs {
        let r = Reference::load(&dir);
        let audio = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| !p.file_name().unwrap().to_string_lossy().starts_with("ANLZ"))
            .unwrap_or_else(|| panic!("{}: no audio file", r.name));
        let pcm = reks_audio::decode_f32(&audio, reks_audio::float::Layout::Interleaved).unwrap();
        let duration_ms = pcm.frames() as u64 * 1000 / u64::from(pcm.sample_rate);
        let columns =
            columns_from_interleaved(&pcm.samples, pcm.sample_rate, pcm.channels).unwrap();
        let waves = Waveforms::encode(&columns, &PLACEHOLDER_CALIBRATION).unwrap();
        // The grid is rekordbox's (our grid analysis is measured in task 12).
        let beats = common::beats(&r.dat);
        let ours = build_like(&r, &beats, duration_ms, &waves);
        compare_structure(
            &r.name,
            &ours.dat,
            &r.dat,
            &[write::PPTH, write::PQTZ, write::PCOB],
        );
        compare_structure(&r.name, &ours.ext, &r.ext, &[write::PPTH, write::PCOB]);
        // PCO2: same cues; rekordbox may fill the entry tail (FLAC seek data).
        assert_eq!(
            common::cues(&ours.ext),
            common::cues(&r.ext),
            "{}: PCO2 cues",
            r.name
        );
        compare_structure(
            &r.name,
            ours.ex2.as_deref().unwrap(),
            &r.ex2,
            &[write::PPTH],
        );
        report(&r.name, &ours, &r);
    }
}
