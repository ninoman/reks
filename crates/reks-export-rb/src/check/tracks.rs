//! One track's files: its path, audio (probe + [`audiocheck`]) and ANLZ
//! files. Reads headers and the ANLZ files only; never decodes the audio.

use std::path::{Component, Path, PathBuf};

use reks_anlz::read::{self as anlz, Section, PPTH};
use reks_anlz::write::PWAV;
use reks_core::fsname;

use super::{CheckedTrack, FindingKind, Findings, Severity};
use crate::audiocheck::{self, Severity as AudioSeverity};
use crate::validate::locate;
use crate::TargetProfile;

pub(super) fn check(root: &Path, t: &CheckedTrack, target: TargetProfile, out: &mut Findings) {
    if let Err(problem) = fsname::check_path(&t.stick_path) {
        out.item(
            FindingKind::PathUnsafe,
            Severity::Warning,
            t.item(Some(capitalise(&problem.to_string()))),
        );
    }
    match find(root, t, &t.stick_path, "audio", out) {
        Found::Here(audio) => check_audio(&audio, t, target, out),
        Found::Missing => out.item(FindingKind::AudioMissing, Severity::Problem, t.item(None)),
        Found::Reported => {}
    }
    check_analysis(root, t, out);
}

enum Found {
    Here(PathBuf),
    Missing,
    /// Not looked for, or not readable; already a finding.
    Reported,
}

/// `rel` on the stick. A path that leaves the stick is never followed: a
/// library is data from a foreign stick.
fn find(root: &Path, t: &CheckedTrack, rel: &str, what: &str, out: &mut Findings) -> Found {
    if !rel.starts_with('/')
        || Path::new(rel)
            .components()
            .any(|c| matches!(c, Component::ParentDir))
    {
        out.item(
            FindingKind::PathOutsideStick,
            Severity::Problem,
            t.item(Some(format!("The {what} path is {rel}"))),
        );
        return Found::Reported;
    }
    match locate(root, rel) {
        Ok(Some(path)) => Found::Here(path),
        Ok(None) => Found::Missing,
        Err(e) => {
            let kind = if what == "audio" {
                FindingKind::AudioUnreadable
            } else {
                FindingKind::AnlzUnreadable
            };
            out.item(
                kind,
                Severity::Problem,
                t.item(Some(format!("Cannot read {rel}: {e}"))),
            );
            Found::Reported
        }
    }
}

fn check_audio(path: &Path, t: &CheckedTrack, target: TargetProfile, out: &mut Findings) {
    let probe = match reks_audio::probe(path) {
        Ok(probe) => probe,
        Err(e) => {
            out.item(
                FindingKind::AudioUnreadable,
                Severity::Problem,
                t.item(Some(e.to_string())),
            );
            return;
        }
    };
    match audiocheck::check(path, &probe, target) {
        Ok(issues) => {
            for issue in issues {
                let severity = match issue.severity(target) {
                    AudioSeverity::Error => Severity::Problem,
                    AudioSeverity::Warning => Severity::Warning,
                };
                out.push(
                    FindingKind::AudioFormat,
                    severity,
                    capitalise(&issue.to_string()),
                    Some(t.item(None)),
                );
            }
        }
        Err(e) => out.item(
            FindingKind::AudioUnreadable,
            Severity::Problem,
            t.item(Some(e.to_string())),
        ),
    }
}

/// The `.DAT` the library names and the `.EXT` and `.2EX` beside it.
fn check_analysis(root: &Path, t: &CheckedTrack, out: &mut Findings) {
    if t.analysis_path.is_empty() {
        out.item(FindingKind::NoAnalysis, Severity::Warning, t.item(None));
        return;
    }
    let dat = match find(root, t, &t.analysis_path, "analysis", out) {
        Found::Here(path) => path,
        Found::Missing => {
            out.item(
                FindingKind::AnlzMissing,
                Severity::Problem,
                t.item(Some(format!("{} is not on the stick", t.analysis_path))),
            );
            return;
        }
        Found::Reported => return,
    };
    match read(&dat) {
        Ok(bytes) => match anlz::sections(&bytes) {
            Ok(sections) => {
                check_ppth(t, ".DAT", &sections, out);
                if !has_preview(&sections) {
                    out.item(
                        FindingKind::NoWaveformPreview,
                        Severity::Warning,
                        t.item(None),
                    );
                }
            }
            Err(e) => unreadable(t, ".DAT", e.to_string(), Severity::Problem, out),
        },
        Err(e) => unreadable(t, ".DAT", e, Severity::Problem, out),
    }

    // A missing .2EX is only a note: rekordbox before 6 never wrote one.
    for (ext, missing, damaged) in [
        (
            "EXT",
            (FindingKind::ExtMissing, Severity::Warning),
            Severity::Problem,
        ),
        (
            "2EX",
            (FindingKind::TwoExMissing, Severity::Note),
            Severity::Warning,
        ),
    ] {
        let label = format!(".{ext}");
        let Some(path) = locate(root, &sibling(&t.analysis_path, ext)).ok().flatten() else {
            out.item(missing.0, missing.1, t.item(None));
            continue;
        };
        let bytes = match read(&path) {
            Ok(bytes) => bytes,
            Err(e) => {
                unreadable(t, &label, e, damaged, out);
                continue;
            }
        };
        match anlz::sections(&bytes) {
            Ok(sections) if ext == "EXT" => check_ppth(t, &label, &sections, out),
            Ok(_) => {}
            Err(e) => unreadable(t, &label, e.to_string(), damaged, out),
        }
    }
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| e.to_string())
}

fn unreadable(t: &CheckedTrack, file: &str, why: String, severity: Severity, out: &mut Findings) {
    out.item(
        FindingKind::AnlzUnreadable,
        severity,
        t.item(Some(format!("{file}: {why}"))),
    );
}

/// `PPTH` must name the track's own file, or players don't link the analysis.
fn check_ppth(t: &CheckedTrack, file: &str, sections: &[Section<'_>], out: &mut Findings) {
    let named = sections
        .iter()
        .find(|s| s.tag == PPTH)
        .and_then(|s| anlz::path(s).ok());
    let same = named
        .as_deref()
        .is_some_and(|p| super::path_key(p) == super::path_key(&t.stick_path));
    if !same {
        out.item(
            FindingKind::PpthMismatch,
            Severity::Problem,
            t.item(Some(format!(
                "{file} names {}",
                named.as_deref().unwrap_or("no file")
            ))),
        );
    }
}

/// The `.DAT`'s `PWAV`, the preview strip players draw, holds anything.
fn has_preview(sections: &[Section<'_>]) -> bool {
    sections
        .iter()
        .any(|s| s.tag == PWAV && s.payload().iter().any(|&b| b != 0))
}

/// `…/ANLZ0000.DAT` → `…/ANLZ0000.<ext>`.
fn sibling(dat: &str, ext: &str) -> String {
    match dat.rsplit_once('.') {
        Some((stem, _)) => format!("{stem}.{ext}"),
        None => format!("{dat}.{ext}"),
    }
}

fn capitalise(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}
