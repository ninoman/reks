//! `reks anlz build`: Pioneer ANLZ files from reks's analysis, headless
//! (Phase 3 task 10).
//!
//! ```text
//! reks anlz build <audio> <out-dir> [--path </Contents/...>] [--target standard|3000]
//!                 [--model-dir <dir>]
//!     analyse the file and build ANLZ0000.DAT/.EXT/.2EX from the analysis
//!     grid and waveforms (no cues)
//! reks anlz build --library <library.sqlite> --track <id> <out-dir> [--cache-dir <dir>]
//!                 [--path </Contents/...>] [--target standard|3000]
//!     build what a USB export writes for the track: the library's grid and
//!     cues, waveforms from the analysis cache (the track must be analysed)
//! ```
//!
//! Every set passes `reks_anlz::validate_set` (the builder checks it). The
//! output directory is a plain folder, never a stick: exports go through
//! `reks rb export`. `--path` is the `PPTH` stick path; default
//! `/Contents/<file name>` (ASCII, D24). `--target` defaults to `3000` (with
//! `.2EX`), what exports write for every player profile. `--cache-dir`: as
//! for `reks rb` (default: the cache folder stored in the library, else
//! `cache/` beside it).

use std::path::{Path, PathBuf};

use anyhow::{bail, Context};
use reks_analysis::ANALYSIS_RATE;
use reks_anlz::grid::pqtz_beats;
use reks_anlz::{read, AnlzInput, AnlzSet, AnlzTarget, WaveformCalibration, Waveforms};
use reks_audio::Kind;
use reks_core::{fsname, Library, TrackId};
use reks_export_rb::reksanlz::{self, ReksTrack};

const USAGE: &str = "usage: reks anlz build <audio> <out-dir> [--path </Contents/...>] \
                     [--target standard|3000] [--model-dir <dir>]\n       \
                     reks anlz build --library <library.sqlite> --track <id> <out-dir> \
                     [--cache-dir <dir>] [--path </Contents/...>] [--target standard|3000]";

pub fn run(args: &[String]) -> anyhow::Result<()> {
    match args {
        [cmd, rest @ ..] if cmd == "build" => build(&parse(rest)?),
        _ => bail!("{USAGE}"),
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Args {
    /// Positional arguments: `<audio> <out-dir>` or `<out-dir>`.
    positional: Vec<PathBuf>,
    library: Option<PathBuf>,
    track: Option<TrackId>,
    cache_dir: Option<PathBuf>,
    path: Option<String>,
    target: Option<AnlzTarget>,
    model_dir: Option<PathBuf>,
}

fn parse(args: &[String]) -> anyhow::Result<Args> {
    let mut out = Args::default();
    let mut args = args.iter();
    while let Some(a) = args.next() {
        let mut value = || args.next().with_context(|| format!("{a} needs a value"));
        match a.as_str() {
            "--library" => out.library = Some(PathBuf::from(value()?)),
            "--track" => out.track = Some(value()?.parse().context("--track needs an id")?),
            "--cache-dir" => out.cache_dir = Some(PathBuf::from(value()?)),
            "--path" => out.path = Some(value()?.clone()),
            "--target" => {
                out.target = Some(match value()?.as_str() {
                    "standard" => AnlzTarget::Standard,
                    "3000" => AnlzTarget::Cdj3000,
                    other => bail!("unknown --target {other}"),
                })
            }
            "--model-dir" => out.model_dir = Some(PathBuf::from(value()?)),
            flag if flag.starts_with("--") => bail!("unknown flag {flag}\n{USAGE}"),
            p => out.positional.push(PathBuf::from(p)),
        }
    }
    Ok(out)
}

fn build(args: &Args) -> anyhow::Result<()> {
    let target = args.target.unwrap_or(AnlzTarget::Cdj3000);
    let (set, out, contents) = match (&args.library, args.track, args.positional.as_slice()) {
        (Some(db), Some(id), [out]) => {
            let (set, contents) = from_library(db, args.cache_dir.as_deref(), id, args, target)?;
            (set, out, contents)
        }
        (None, None, [audio, out]) => {
            let contents = contents_path(args, audio);
            let set = from_file(audio, &contents, args.model_dir.as_deref(), target)?;
            (set, out, contents)
        }
        _ => bail!("{USAGE}"),
    };
    let dat = reksanlz::write_set(out, &set)?;
    println!("PPTH {contents}");
    for (ext, bytes) in set.files() {
        let tags: Vec<String> = read::sections(bytes)?
            .iter()
            .map(read::Section::tag_str)
            .collect();
        println!(
            "{} {:>8} bytes  {}",
            dat.with_extension(ext).display(),
            bytes.len(),
            tags.join(" ")
        );
    }
    Ok(())
}

/// `--path`, else `/Contents/<file name of audio>`.
fn contents_path(args: &Args, audio: &Path) -> String {
    args.path.clone().unwrap_or_else(|| {
        let file = audio
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        format!("/Contents/{}", fsname::sanitize_component(&file, 64))
    })
}

/// Analyses `audio` now and builds from the result.
fn from_file(
    audio: &Path,
    contents: &str,
    model_dir: Option<&Path>,
    target: AnlzTarget,
) -> anyhow::Result<AnlzSet> {
    let a = crate::analyse::analyse(audio, model_dir)?;
    let beats = match &a.grid {
        Some(g) => pqtz_beats(g, ANALYSIS_RATE, a.duration_ms)?,
        None => Vec::new(),
    };
    let waveforms = Waveforms::encode(&a.columns, &WaveformCalibration::default())?;
    let kind = reks_audio::probe(audio)?.kind;
    let mp3_audio_frames = match kind {
        Kind::Mp3 => reks_audio::mp3_audio_frames(audio)?,
        _ => None,
    };
    let input = AnlzInput {
        contents_path: contents,
        duration_ms: a.duration_ms,
        beats: &beats,
        cues: &[],
        waveforms: &waveforms,
        mp3_audio_frames,
    };
    Ok(reks_anlz::build(&input, target)?)
}

/// What an export builds for library track `id`.
fn from_library(
    db: &Path,
    cache_dir: Option<&Path>,
    id: TrackId,
    args: &Args,
    target: AnlzTarget,
) -> anyhow::Result<(AnlzSet, String)> {
    if !db.is_file() {
        bail!("no library at {}", db.display());
    }
    let beside = db.parent().unwrap_or(Path::new(".")).join("cache");
    let lib = Library::open(db, cache_dir.unwrap_or(&beside))?;
    if cache_dir.is_none() {
        lib.use_stored_cache_dir()?;
    }
    let t = lib.track(id)?.track;
    let src = PathBuf::from(&t.path);
    let contents = contents_path(args, &src);
    let kind = reks_audio::probe(&src)?.kind;
    let track = ReksTrack {
        track_id: id,
        src: &src,
        contents_path: &contents,
        kind,
        rekordbox_dat: None,
    };
    let set = reksanlz::build(&lib, &track, target).with_context(|| {
        format!(
            "track {id} (cache {}); is it analysed?",
            lib.cache_dir().display()
        )
    })?;
    Ok((set, contents))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn parses_both_forms() {
        let a = parse(&strings(&["x.mp3", "out", "--target", "standard"])).unwrap();
        assert_eq!(a.positional, [PathBuf::from("x.mp3"), PathBuf::from("out")]);
        assert_eq!(a.target, Some(AnlzTarget::Standard));
        let a = parse(&strings(&[
            "--library",
            "l.sqlite",
            "--track",
            "7",
            "out",
            "--path",
            "/Contents/a.mp3",
        ]))
        .unwrap();
        assert_eq!(a.track, Some(7));
        assert_eq!(a.path.as_deref(), Some("/Contents/a.mp3"));
        assert!(parse(&strings(&["--target", "nxs"])).is_err());
        assert!(parse(&strings(&["--bogus"])).is_err());
    }

    #[test]
    fn default_path_is_ascii() {
        let a = Args::default();
        assert_eq!(
            contents_path(&a, Path::new("/music/Песня.mp3")),
            "/Contents/Pesnia.mp3"
        );
    }

    /// The whole file path: analysis with the bundled model, then the build.
    #[test]
    fn builds_from_a_fixture() {
        let audio =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/audio/sine-click-128.mp3");
        let out = tempfile::tempdir().unwrap();
        let args = Args {
            positional: vec![audio.clone(), out.path().to_path_buf()],
            ..Args::default()
        };
        build(&args).unwrap();
        let dat = std::fs::read(out.path().join("ANLZ0000.DAT")).unwrap();
        let ext = std::fs::read(out.path().join("ANLZ0000.EXT")).unwrap();
        let ex2 = std::fs::read(out.path().join("ANLZ0000.2EX")).unwrap();
        let issues = reks_anlz::validate_set(
            &dat,
            &ext,
            Some(&ex2),
            reks_anlz::Expect {
                path: Some("/Contents/sine-click-128.mp3"),
                duration_ms: None,
            },
        );
        assert!(issues.is_empty(), "{issues:?}");
        let pqtz = read::sections(&dat)
            .unwrap()
            .into_iter()
            .find(|s| &s.tag == b"PQTZ")
            .unwrap();
        let beats = read::pqtz_beats(&pqtz).unwrap();
        assert!(!beats.is_empty());
        assert!(beats.iter().all(|b| b.tempo == 12_800), "{:?}", &beats[..4]);
    }
}
