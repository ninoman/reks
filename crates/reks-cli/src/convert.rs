//! `reks convert ...`: the file converter, headless (Phase 2.6 task 7).
//!
//! ```text
//! reks convert plan --library <library.sqlite> (--track <id> | --playlist <id|Folder/Name>)...
//!                   [--preset older-players|save-space] [--format wav|aiff|flac]
//!                   [--depth keep|16|24] [--rate keep|max48|44100|48000]
//!                   [--dest <dir>] [--add-as-new] [--trash] [--json]
//!     print the dry run; exits non-zero when nothing would convert
//! reks convert apply <plan flags> [--yes]
//!     print the dry run; with --yes, convert and update the library;
//!     exits non-zero unless every planned file converted
//! ```
//!
//! The preset (default `older-players`) sets format, depth and rate; the
//! other flags override it. `--rate max48` is the Older players cap (88.2 →
//! 44.1, 96 → 48 kHz). Converted files go next to their originals unless
//! `--dest` names a folder. The track takes the new file (Replace) unless
//! `--add-as-new`; `--trash` moves the originals to the OS Trash after a
//! verified conversion (Replace only). Originals are never overwritten or
//! deleted. A `--playlist` folder converts every track under it. Ctrl-C
//! during `convert apply` leaves at most a `.reks-tmp` file next to the
//! destination and the current track unchanged.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context};
use reks_core::convert::{
    self, view::Outcome, CancelToken, ConvertEvent, ConvertOptions, ConvertSpec, Depth,
    Destination, LibraryMode, Originals, PlanView, Preset, Rate, ReportView, Target,
};
use reks_core::{Library, TrackId};
use reks_export_rb::plan::{self, Selection};
use serde::Serialize;

const USAGE: &str = "usage: reks convert plan --library <library.sqlite> \
(--track <id> | --playlist <id|Folder/Name>)... [--preset older-players|save-space] \
[--format wav|aiff|flac] [--depth keep|16|24] [--rate keep|max48|44100|48000] \
[--dest <dir>] [--add-as-new] [--trash] [--json]
       reks convert apply <plan flags> [--yes]";

pub fn run(args: &[String]) -> anyhow::Result<()> {
    match args {
        [cmd, flags @ ..] if cmd == "plan" => {
            let args = parse_args(flags, false)?;
            let (lib, tracks) = open(&args)?;
            let plan = convert::plan(&lib, &tracks, &args.spec, &args.opts)?;
            let view = convert::plan_view(&lib, &plan);
            if args.json {
                println!("{}", serde_json::to_string_pretty(&view)?);
            } else {
                print!("{}", render_plan(&view));
            }
            if !view.ready {
                bail!("nothing to convert");
            }
            Ok(())
        }
        [cmd, flags @ ..] if cmd == "apply" => apply(&parse_args(flags, true)?),
        _ => bail!("{USAGE}"),
    }
}

/// `convert apply --json` output.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ApplyOut<'a> {
    plan: &'a PlanView,
    /// `None` without `--yes`.
    report: Option<&'a ReportView>,
}

fn apply(args: &Args) -> anyhow::Result<()> {
    let (lib, tracks) = open(args)?;
    let plan = convert::plan(&lib, &tracks, &args.spec, &args.opts)?;
    let view = convert::plan_view(&lib, &plan);
    if !args.json {
        print!("{}", render_plan(&view));
    }
    if !view.ready || !args.yes {
        if args.json {
            let out = ApplyOut {
                plan: &view,
                report: None,
            };
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        if !view.ready {
            bail!("nothing to convert");
        }
        eprintln!("Nothing written. Pass --yes to convert.");
        return Ok(());
    }
    let report = convert::apply(&lib, &plan, &print_event, &CancelToken::new())?;
    let report = convert::report_view(&lib, &plan, &report);
    if args.json {
        let out = ApplyOut {
            plan: &view,
            report: Some(&report),
        };
        println!("{}", serde_json::to_string_pretty(&out)?);
    } else {
        print!("{}", render_report(&report));
    }
    match report.outcome {
        Outcome::Success => Ok(()),
        Outcome::Problems => bail!("{} file(s) failed", report.failed.len()),
        Outcome::Cancelled => bail!("the conversion was cancelled"),
    }
}

/// Progress on stderr.
fn print_event(e: ConvertEvent) {
    match e {
        ConvertEvent::Started { index, total, .. } => eprint!("\r  file {}/{total}", index + 1),
        ConvertEvent::Converted { .. } | ConvertEvent::Progress { .. } => {}
        ConvertEvent::Failed { track_id, message } => {
            eprintln!("\n  track {track_id} failed: {message}");
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Args {
    library: PathBuf,
    tracks: Vec<TrackId>,
    playlists: Vec<String>,
    spec: ConvertSpec,
    opts: ConvertOptions,
    json: bool,
    /// `convert apply`: convert.
    yes: bool,
}

fn parse_args(flags: &[String], apply: bool) -> anyhow::Result<Args> {
    let mut library = None;
    let mut tracks = Vec::new();
    let mut playlists = Vec::new();
    let mut preset = Preset::OlderPlayers;
    let (mut target, mut depth, mut rate) = (None, None, None);
    let mut opts = ConvertOptions::default();
    let mut json = false;
    let mut yes = false;
    let mut flags = flags.iter();
    while let Some(flag) = flags.next() {
        let mut value = || {
            flags
                .next()
                .with_context(|| format!("{flag} needs a value"))
        };
        match flag.as_str() {
            "--library" => library = Some(PathBuf::from(value()?)),
            "--track" => {
                let v = value()?;
                tracks.push(v.parse().with_context(|| format!("--track {v}"))?);
            }
            "--playlist" => playlists.push(value()?.clone()),
            "--preset" => {
                preset = match value()?.as_str() {
                    "older-players" => Preset::OlderPlayers,
                    "save-space" => Preset::SaveSpace,
                    other => bail!("unknown preset {other}"),
                }
            }
            "--format" => {
                target = Some(match value()?.as_str() {
                    "wav" => Target::Wav,
                    "aiff" => Target::Aiff,
                    "flac" => Target::Flac,
                    other => bail!("unknown format {other}"),
                })
            }
            "--depth" => {
                depth = Some(match value()?.as_str() {
                    "keep" => Depth::Keep,
                    "16" => Depth::Bits16,
                    "24" => Depth::Bits24,
                    other => bail!("unknown depth {other}"),
                })
            }
            "--rate" => {
                rate = Some(match value()?.as_str() {
                    "keep" => (Rate::Keep, false),
                    "max48" => (Rate::Keep, true),
                    "44100" => (Rate::Hz44100, false),
                    "48000" => (Rate::Hz48000, false),
                    other => bail!("unknown rate {other}"),
                })
            }
            "--dest" => opts.destination = Destination::Folder(PathBuf::from(value()?)),
            "--add-as-new" => opts.library = LibraryMode::AddAsNew,
            "--trash" => opts.originals = Originals::Trash,
            "--json" => json = true,
            "--yes" if apply => yes = true,
            other => bail!("unknown flag {other}\n{USAGE}"),
        }
    }
    let Some(library) = library else {
        bail!("--library is required\n{USAGE}");
    };
    if tracks.is_empty() && playlists.is_empty() {
        bail!("at least one --track or --playlist is required\n{USAGE}");
    }
    let mut spec = preset.spec();
    opts.cap_rate_48k = preset.options().cap_rate_48k;
    spec.target = target.unwrap_or(spec.target);
    spec.depth = depth.unwrap_or(spec.depth);
    if let Some((r, cap)) = rate {
        spec.rate = r;
        opts.cap_rate_48k = cap;
    }
    Ok(Args {
        library,
        tracks,
        playlists,
        spec,
        opts,
        json,
        yes,
    })
}

/// The library and the selected track ids: `--track`s first, then each
/// playlist's tracks (a folder: everything under it). `plan` skips repeats.
fn open(args: &Args) -> anyhow::Result<(Library, Vec<TrackId>)> {
    if !args.library.is_file() {
        bail!("no library at {}", args.library.display());
    }
    let cache_dir = args
        .library
        .parent()
        .unwrap_or(Path::new("."))
        .join("cache");
    let lib = Library::open(&args.library, &cache_dir)?;
    let mut tracks = args.tracks.clone();
    if !args.playlists.is_empty() {
        let sel = Selection {
            playlists: crate::rb::resolve_playlists(&lib.playlists()?.nodes, &args.playlists)?,
        };
        let sources = plan::load(&lib, &sel)?;
        for t in sources.tracks {
            if !tracks.contains(&t.track_id) {
                tracks.push(t.track_id);
            }
        }
    }
    Ok((lib, tracks))
}

fn render_plan(v: &PlanView) -> String {
    let mut out = String::new();
    let o = &mut out;
    let _ = writeln!(o, "Dry run: {}", v.status);
    for f in &v.facts {
        let _ = writeln!(o, "{:<12} {}", f.label, f.value);
    }
    if !v.items.is_empty() {
        let _ = writeln!(o, "Convert");
        for i in &v.items {
            let _ = writeln!(o, "  {} (track {})", i.label, i.track_id);
            let _ = writeln!(o, "    {} -> {}  ({})", i.from, i.to, i.size_text);
            let _ = writeln!(o, "    {}", i.source);
            let _ = writeln!(o, "    -> {}", i.dest);
        }
    }
    if !v.skipped.is_empty() {
        let _ = writeln!(o, "Skipped");
        for s in &v.skipped {
            let _ = writeln!(o, "  {} (track {}): {}", s.label, s.track_id, s.reason);
        }
    }
    if !v.warnings.is_empty() {
        let _ = writeln!(o, "Warnings");
        for w in &v.warnings {
            let _ = writeln!(o, "  {w}");
        }
    }
    out
}

fn render_report(r: &ReportView) -> String {
    let mut out = String::new();
    let o = &mut out;
    let _ = writeln!(o, "{}. {}", r.title, r.status);
    for f in &r.facts {
        let _ = writeln!(o, "{:<12} {}", f.label, f.value);
    }
    for c in &r.converted {
        let _ = writeln!(o, "  {} -> {}", c.label, c.dest);
        let _ = writeln!(o, "    {}", c.original);
    }
    for f in &r.failed {
        let _ = writeln!(o, "  FAILED {}: {}", f.label, f.message);
    }
    out
}

#[cfg(test)]
mod tests {
    use reks_core::scan;

    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn flags_override_the_preset() {
        let a = parse_args(&args("--library l.sqlite --track 3"), false).unwrap();
        assert_eq!(a.spec, Preset::OlderPlayers.spec());
        assert_eq!(a.opts, Preset::OlderPlayers.options());
        assert!(!a.yes);

        let a = parse_args(
            &args(
                "--library l.sqlite --playlist Gigs/Friday --preset save-space --format wav \
                 --depth 16 --rate 48000 --dest out --add-as-new --json --yes",
            ),
            true,
        )
        .unwrap();
        assert_eq!(
            a.spec,
            ConvertSpec {
                target: Target::Wav,
                depth: Depth::Bits16,
                rate: Rate::Hz48000
            }
        );
        assert_eq!(a.opts.destination, Destination::Folder("out".into()));
        assert_eq!(a.opts.library, LibraryMode::AddAsNew);
        assert!(!a.opts.cap_rate_48k);
        assert!(a.json && a.yes);

        let a = parse_args(
            &args("--library l --track 1 --preset save-space --rate max48 --trash"),
            false,
        )
        .unwrap();
        assert!(a.opts.cap_rate_48k);
        assert_eq!(a.opts.originals, Originals::Trash);

        assert!(parse_args(&args("--library l"), false).is_err());
        assert!(parse_args(&args("--track 1"), false).is_err());
        assert!(parse_args(&args("--library l --track 1 --yes"), false).is_err());
        assert!(parse_args(&args("--library l --track x"), false).is_err());
        assert!(parse_args(&args("--library l --track 1 --rate 96000"), false).is_err());
    }

    /// A library with `music/sine.flac` and `music/sine.mp3`, scanned in.
    fn library(dir: &Path) -> (PathBuf, TrackId, TrackId) {
        let music = dir.join("music");
        std::fs::create_dir_all(&music).unwrap();
        let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/audio");
        for ext in ["flac", "mp3"] {
            std::fs::copy(
                fixtures.join(format!("sine-click-128.{ext}")),
                music.join(format!("sine.{ext}")),
            )
            .unwrap();
        }
        let db = dir.join("reks.sqlite");
        let lib = Library::open(&db, &dir.join("cache")).unwrap();
        scan(
            std::slice::from_ref(&music),
            &lib,
            &CancelToken::new(),
            |_| {},
        )
        .unwrap();
        let id = |name: &str| {
            let page = lib.tracks(&reks_core::TrackQuery::default()).unwrap();
            page.rows
                .iter()
                .find(|r| r.kind == name)
                .map(|r| r.id)
                .unwrap()
        };
        (db, id("flac"), id("mp3"))
    }

    #[test]
    fn plan_then_apply_with_yes() {
        let dir = tempfile::tempdir().unwrap();
        let (db, flac, mp3) = library(dir.path());
        let flags = format!("--library {} --track {flac} --track {mp3}", db.display());
        let a = parse_args(&args(&flags), true).unwrap();
        let (lib, tracks) = open(&a).unwrap();
        let p = convert::plan(&lib, &tracks, &a.spec, &a.opts).unwrap();
        let text = render_plan(&convert::plan_view(&lib, &p));
        assert!(
            text.starts_with("Dry run: Ready to convert 1 file."),
            "{text}"
        );
        assert!(
            text.contains("FLAC · 16-bit · 44.1 kHz · mono -> AIFF"),
            "{text}"
        );
        assert!(
            text.contains("MP3 or AAC: only lossless files convert"),
            "{text}"
        );
        drop(lib);

        // Without --yes: nothing written.
        apply(&a).unwrap();
        let aiff = dir.path().join("music/sine.aiff");
        assert!(!aiff.exists());

        let a = Args { yes: true, ..a };
        apply(&a).unwrap();
        assert!(aiff.is_file());
        assert!(dir.path().join("music/sine.flac").is_file());

        // Converted already: nothing left to do.
        let a = parse_args(
            &args(&format!("--library {} --track {flac}", db.display())),
            true,
        )
        .unwrap();
        let err = apply(&a).unwrap_err();
        assert!(err.to_string().contains("nothing to convert"), "{err}");
    }
}
