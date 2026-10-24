//! `reks rb check <stick> [--target <target>] [--library <library.sqlite>] [--json]`:
//! the read-only stick report ([`reks_export_rb::check`]) a DJ runs before
//! a gig. `exportLibrary.db` is read with `REKS_ONELIBRARY_KEY` or the
//! built-in key. `--target` names the players the stick is for (default:
//! the strictest); `--library` compares the stick's play history with that
//! library. Exits non-zero when the report has problems.

use std::fmt::Write as _;
use std::path::PathBuf;

use anyhow::{bail, Context};
use reks_core::Library;
use reks_export_onelibrary::check::CheckReader;
use reks_export_rb::apply::StickCheck;
use reks_export_rb::check::{check_stick, CheckOptions, Severity, Status, StickReport};
use reks_export_rb::onelibrary::resolve_key;

use crate::rb::{parse_target, Stick};

const USAGE: &str =
    "usage: reks rb check <stick> [--target nxs|nxs2|3000|xdj-xz|generic] [--library <library.sqlite>] [--json]";

pub fn run(st: &Stick, flags: &[String]) -> anyhow::Result<()> {
    let mut opts = CheckOptions {
        target: None,
        volume_serial: st.serial.clone(),
        directory: st.check == StickCheck::Directory,
    };
    let mut library: Option<PathBuf> = None;
    let mut json = false;
    let mut it = flags.iter();
    while let Some(flag) = it.next() {
        let mut value = || it.next().with_context(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--target" => opts.target = Some(parse_target(value()?)?),
            "--library" => library = Some(PathBuf::from(value()?)),
            "--json" => json = true,
            _ => bail!("{USAGE}"),
        }
    }
    let lib = match &library {
        Some(db) if !db.is_file() => bail!("no library at {}", db.display()),
        Some(db) => Some(Library::open(db, &db.parent().unwrap_or(db).join("cache"))?),
        None => None,
    };
    let reader = CheckReader { key: resolve_key() };
    let mut dev = st.dev.clone();
    if opts.directory {
        // The label is the computer's volume, not the stick's.
        dev.label.clear();
    }
    let report = check_stick(&dev, &opts, &reader, lib.as_ref());
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!("{}", render(&report));
    }
    if report.summary.status == Status::Problems {
        bail!("{}", report.summary.headline);
    }
    Ok(())
}

fn render(r: &StickReport) -> String {
    let mut s = String::new();
    let name = if r.label.is_empty() {
        r.mount_point.display().to_string()
    } else {
        format!("{} ({})", r.label, r.mount_point.display())
    };
    let _ = writeln!(s, "{name}: {}", r.summary.headline);
    let _ = writeln!(s, "{}", r.target_text);
    let _ = writeln!(
        s,
        "{} · {} tracks checked",
        r.filesystem.text, r.tracks_checked
    );
    let _ = writeln!(s, "\nLibrary files");
    for f in &r.formats {
        let _ = writeln!(s, "  {:<26} {:<36} {}", f.name, f.file, f.status);
        if !f.players.is_empty() {
            let _ = writeln!(s, "    read by: {}", f.players.join(", "));
        }
    }
    let _ = writeln!(s, "  {}", r.players_source);
    for section in &r.sections {
        let _ = writeln!(s, "\n{}", section.title);
        for g in &section.groups {
            let tag = match g.severity {
                Severity::Problem => "problem",
                Severity::Warning => "warning",
                Severity::Note => "note",
            };
            let _ = writeln!(s, "  [{tag}] {}", g.title);
            let _ = writeln!(s, "      {}", g.detail);
            for item in &g.items {
                let mut line = item.subject.clone();
                if let Some(d) = &item.detail {
                    let _ = write!(line, ": {d}");
                }
                if let Some(p) = item.path.as_ref().filter(|p| **p != item.subject) {
                    let _ = write!(line, " ({p})");
                }
                let _ = writeln!(s, "      - {line}");
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use reks_export_rb::device::{DeviceInfo, FsKind};
    use reks_export_rb::testutil::*;

    use super::*;

    const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/audio");

    fn stick_at(root: &Path) -> Stick {
        Stick {
            dev: DeviceInfo {
                mount_point: root.to_owned(),
                fs: FsKind::Fat32,
                fs_name: "msdos".into(),
                removable: true,
                total_bytes: 8_000_000_000,
                free_bytes: 1_000_000_000,
                label: "GIG".into(),
                os_name: "GIG".into(),
            },
            serial: None,
            check: StickCheck::Mounted,
        }
    }

    /// A stick with one track whose audio is there and one whose is not;
    /// neither has analysis.
    fn fake_stick(root: &Path) {
        let pdb = root.join("PIONEER/rekordbox/export.pdb");
        fs::create_dir_all(pdb.parent().unwrap()).unwrap();
        let tables: Vec<(u32, Vec<Vec<Vec<u8>>>)> = (0..20)
            .map(|t| {
                let rows = match t {
                    T_TRACKS => vec![
                        track_row(1, 1, 30, "Here", "/Contents/a/b/here.wav"),
                        track_row(2, 1, 30, "Gone", "/Contents/a/b/gone.wav"),
                    ],
                    T_ARTISTS => vec![artist_row(1, "reks")],
                    _ => Vec::new(),
                };
                (t, vec![rows])
            })
            .collect();
        fs::write(&pdb, synthetic_pdb(&tables)).unwrap();
        let wav = root.join("Contents/a/b/here.wav");
        fs::create_dir_all(wav.parent().unwrap()).unwrap();
        fs::copy(Path::new(FIXTURES).join("sine-click-128.wav"), wav).unwrap();
    }

    #[test]
    fn problems_fail_the_command_and_render_as_text() {
        let dir = tempfile::tempdir().unwrap();
        fake_stick(dir.path());
        let st = stick_at(dir.path());
        let err = run(&st, &["--target".into(), "3000".into()]).unwrap_err();
        assert_eq!(err.to_string(), "1 problem, 3 warnings");

        let report = check_stick(
            &st.dev,
            &CheckOptions::default(),
            &CheckReader { key: None },
            None,
        );
        let text = render(&report);
        assert!(text.starts_with("GIG ("), "{text}");
        assert!(
            text.contains("[problem] 1 track: audio file missing"),
            "{text}"
        );
        assert!(
            text.contains("      - reks – Gone (/Contents/a/b/gone.wav)"),
            "{text}"
        );
        assert!(text.contains("UNVERIFIED"), "{text}");
    }

    #[test]
    fn flags() {
        let dir = tempfile::tempdir().unwrap();
        fake_stick(dir.path());
        let st = stick_at(dir.path());
        assert!(run(&st, &["--bogus".into()]).is_err());
        let err = run(&st, &["--library".into(), "/nope.sqlite".into()]).unwrap_err();
        assert!(err.to_string().starts_with("no library at"));
        assert!(run(&st, &["--target".into(), "cdj9000".into()]).is_err());
    }
}
