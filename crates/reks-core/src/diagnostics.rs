//! Bug reports and feedback (D34): the user's draft plus, when they opt in,
//! app diagnostics with home and library paths redacted. Pure; the app
//! gathers the inputs, shows the rendered report and sends exactly it.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// Longest title kept, in characters (GitHub allows 256).
pub const MAX_TITLE_CHARS: usize = 120;
/// Longest body sent, in bytes; GitHub's limit is 65,536 characters.
pub const MAX_BODY_BYTES: usize = 60_000;
/// Most log lines included.
pub const LOG_TAIL_LINES: usize = 300;
/// Most log bytes included, before the body cap trims further.
pub const LOG_TAIL_BYTES: usize = 32 * 1024;

const TRUNCATED: &str = "\n\n…(truncated)";

/// What a report is about; the relay maps it to an issue label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ReportKind {
    Bug,
    Feature,
    Feedback,
}

/// What the user wrote. Empty strings mean "left blank".
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportDraft {
    pub kind: ReportKind,
    pub title: String,
    pub description: String,
    /// Bugs only.
    pub steps: String,
    /// Bugs only.
    pub expected: String,
    /// Optional email for follow-up questions.
    pub contact: String,
    /// The error message the report was started from, if any.
    pub context: String,
    pub include_diagnostics: bool,
}

/// App state worth knowing when fixing a bug. Contains raw paths until
/// rendered through [`render`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostics {
    pub app_version: String,
    pub os: String,
    pub arch: String,
    pub schema_version: i64,
    pub track_count: u64,
    pub playlist_count: u64,
    /// The newest crash file, if a recent one exists.
    pub crash: Option<String>,
    /// The end of the newest log file.
    pub log: String,
}

/// Paths replaced before diagnostics leave the machine.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Redactions {
    /// `(path, placeholder)`, longest path first so a root inside the home
    /// dir is replaced before the home dir is.
    pairs: Vec<(String, String)>,
}

impl Redactions {
    /// Library roots become `<library-root-N>`, the home dir `~`.
    pub fn new(home: Option<&Path>, roots: &[String]) -> Self {
        let mut pairs: Vec<(String, String)> = roots
            .iter()
            .enumerate()
            .map(|(i, root)| {
                (
                    trim_sep(root).to_owned(),
                    format!("<library-root-{}>", i + 1),
                )
            })
            .collect();
        if let Some(home) = home.and_then(Path::to_str) {
            pairs.push((trim_sep(home).to_owned(), "~".to_owned()));
        }
        // A bare "/" or "" would swallow every separator.
        pairs.retain(|(path, _)| path.len() > 1);
        pairs.sort_by_key(|(path, _)| std::cmp::Reverse(path.len()));
        Self { pairs }
    }

    /// `text` with every known path replaced by its placeholder.
    pub fn apply(&self, text: &str) -> String {
        self.pairs
            .iter()
            .fold(text.to_owned(), |acc, (path, placeholder)| {
                acc.replace(path, placeholder)
            })
    }
}

fn trim_sep(path: &str) -> &str {
    path.trim_end_matches(['/', '\\'])
}

/// A report as sent to the relay and shown in the preview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub kind: ReportKind,
    pub title: String,
    pub body: String,
}

/// Why a draft cannot be sent.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReportError {
    #[error("the report needs a title")]
    NoTitle,
    #[error("the report needs a description")]
    NoDescription,
}

/// Checks the fields the relay requires.
pub fn validate(draft: &ReportDraft) -> Result<(), ReportError> {
    if draft.title.trim().is_empty() {
        return Err(ReportError::NoTitle);
    }
    if draft.description.trim().is_empty() {
        return Err(ReportError::NoDescription);
    }
    Ok(())
}

/// Renders the issue title and Markdown body. `diagnostics` is included
/// only when the draft opts in; the log is trimmed to keep the body under
/// [`MAX_BODY_BYTES`].
pub fn render(draft: &ReportDraft, diagnostics: Option<&Diagnostics>, r: &Redactions) -> Report {
    let mut body = String::new();
    section(&mut body, "Description", draft.description.trim());
    if draft.kind == ReportKind::Bug {
        section(&mut body, "Steps to reproduce", draft.steps.trim());
        section(&mut body, "Expected", draft.expected.trim());
    }
    let context = draft.context.trim();
    if !context.is_empty() {
        section(&mut body, "Error shown", &fenced(&r.apply(context)));
    }
    let contact = draft.contact.trim();
    if !contact.is_empty() {
        section(&mut body, "Contact", contact);
    }

    if let Some(d) = diagnostics.filter(|_| draft.include_diagnostics) {
        body.push_str(&details("Diagnostics", &summary_table(d)));
        if let Some(crash) = &d.crash {
            body.push_str(&details("Last crash", &fenced(&r.apply(crash))));
        }
        let log = r.apply(&d.log);
        // What remains for the log once its own wrapper is accounted for.
        let wrapper = details("Log", &fenced("")).len();
        let budget = MAX_BODY_BYTES
            .saturating_sub(body.len() + wrapper)
            .min(LOG_TAIL_BYTES);
        let log = tail(&log, LOG_TAIL_LINES, budget);
        if !log.is_empty() {
            body.push_str(&details("Log", &fenced(log)));
        }
    }

    Report {
        kind: draft.kind,
        title: cap_chars(draft.title.trim(), MAX_TITLE_CHARS),
        body: cap_bytes(&body, MAX_BODY_BYTES),
    }
}

fn section(body: &mut String, heading: &str, text: &str) {
    if text.is_empty() {
        return;
    }
    body.push_str(&format!("### {heading}\n\n{text}\n\n"));
}

fn details(summary: &str, inner: &str) -> String {
    format!("<details><summary>{summary}</summary>\n\n{inner}\n\n</details>\n\n")
}

fn summary_table(d: &Diagnostics) -> String {
    format!(
        "| | |\n|---|---|\n| reks | {} |\n| OS | {} ({}) |\n| Schema | {} |\n| Tracks | {} |\n| Playlists | {} |",
        d.app_version, d.os, d.arch, d.schema_version, d.track_count, d.playlist_count
    )
}

/// A code block whose fence is longer than any backtick run in `text`, so
/// pasted log lines cannot close it early.
fn fenced(text: &str) -> String {
    let longest = text.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat(longest.max(2) + 1);
    format!("{fence}text\n{text}\n{fence}")
}

/// The last `max_lines` lines of `text`, at most `max_bytes` long, starting
/// on a line boundary where possible.
pub fn tail(text: &str, max_lines: usize, max_bytes: usize) -> &str {
    let text = text.trim_end();
    if max_lines == 0 {
        return "";
    }
    let mut start = text
        .rmatch_indices('\n')
        .nth(max_lines - 1)
        .map_or(0, |(i, _)| i + 1);
    if text.len() - start > max_bytes {
        let mut cut = text.len() - max_bytes;
        while !text.is_char_boundary(cut) {
            cut += 1;
        }
        start = if text[..cut].ends_with('\n') {
            cut
        } else {
            // Skip the partial line, unless it is the only one.
            match text[cut..].find('\n') {
                Some(j) => cut + j + 1,
                None => cut,
            }
        };
    }
    &text[start..]
}

fn cap_chars(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((i, _)) => format!("{}…", &text[..i]),
        None => text.to_owned(),
    }
}

fn cap_bytes(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_owned();
    }
    let mut cut = max.saturating_sub(TRUNCATED.len());
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}{TRUNCATED}", &text[..cut])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(kind: ReportKind) -> ReportDraft {
        ReportDraft {
            kind,
            title: "Export fails".into(),
            description: "It stops at 40%.".into(),
            steps: "1. Export\n2. Wait".into(),
            expected: "It finishes.".into(),
            contact: String::new(),
            context: String::new(),
            include_diagnostics: true,
        }
    }

    fn diagnostics(log: &str) -> Diagnostics {
        Diagnostics {
            app_version: "0.1.0".into(),
            os: "macos".into(),
            arch: "aarch64".into(),
            schema_version: 12,
            track_count: 3000,
            playlist_count: 40,
            crash: None,
            log: log.into(),
        }
    }

    #[test]
    fn redacts_roots_before_home() {
        let r = Redactions::new(
            Some(Path::new("/Users/dj")),
            &["/Users/dj/Music/".into(), "/Volumes/Crates".into()],
        );
        assert_eq!(
            r.apply("open /Users/dj/Music/a.mp3 and /Users/dj/x and /Volumes/Crates/b.flac"),
            "open <library-root-1>/a.mp3 and ~/x and <library-root-2>/b.flac"
        );
    }

    #[test]
    fn ignores_root_paths_that_would_match_everything() {
        let r = Redactions::new(Some(Path::new("/")), &["".into()]);
        assert_eq!(r.apply("/a/b"), "/a/b");
    }

    #[test]
    fn validate_requires_title_and_description() {
        let mut d = draft(ReportKind::Feedback);
        assert_eq!(validate(&d), Ok(()));
        d.description = "  ".into();
        assert_eq!(validate(&d), Err(ReportError::NoDescription));
        d.title = String::new();
        assert_eq!(validate(&d), Err(ReportError::NoTitle));
    }

    #[test]
    fn bug_body_has_steps_and_redacted_diagnostics() {
        let r = Redactions::new(Some(Path::new("/home/dj")), &[]);
        let report = render(
            &draft(ReportKind::Bug),
            Some(&diagnostics("INFO opened /home/dj/.local/reks.sqlite")),
            &r,
        );
        assert!(report.body.contains("### Steps to reproduce"));
        assert!(report.body.contains("| Tracks | 3000 |"));
        assert!(report.body.contains("opened ~/.local/reks.sqlite"));
        assert!(!report.body.contains("/home/dj"));
    }

    #[test]
    fn feature_body_skips_bug_fields_and_unrequested_diagnostics() {
        let mut d = draft(ReportKind::Feature);
        d.include_diagnostics = false;
        let report = render(&d, Some(&diagnostics("log")), &Redactions::default());
        assert!(!report.body.contains("Steps"));
        assert!(!report.body.contains("Diagnostics"));
    }

    #[test]
    fn fence_outlasts_backticks_in_text() {
        let f = fenced("a ```` b");
        assert!(f.starts_with("`````text\n"));
    }

    #[test]
    fn tail_keeps_last_lines_within_budget() {
        let text = "one\ntwo\nthree\nfour\n";
        assert_eq!(tail(text, 2, 1000), "three\nfour");
        assert_eq!(tail(text, 10, 1000), "one\ntwo\nthree\nfour");
        assert_eq!(tail(text, 10, 10), "three\nfour");
        assert_eq!(tail("abcdefgh", 10, 3), "fgh");
        assert_eq!(tail("ééé", 10, 3), "é");
    }

    #[test]
    fn body_stays_under_cap_with_a_huge_log() {
        let log = "x".repeat(200).repeat(1000).replace("xx", "x\n");
        let report = render(
            &draft(ReportKind::Bug),
            Some(&diagnostics(&log)),
            &Redactions::default(),
        );
        assert!(report.body.len() <= MAX_BODY_BYTES);
        assert!(report.body.contains("<summary>Log</summary>"));
    }

    #[test]
    fn huge_description_is_truncated() {
        let mut d = draft(ReportKind::Feedback);
        d.description = "é".repeat(MAX_BODY_BYTES);
        let report = render(&d, None, &Redactions::default());
        assert!(report.body.len() <= MAX_BODY_BYTES);
        assert!(report.body.ends_with("(truncated)"));
    }

    #[test]
    fn long_title_is_capped() {
        let mut d = draft(ReportKind::Feedback);
        d.title = "a".repeat(500);
        assert_eq!(
            render(&d, None, &Redactions::default())
                .title
                .chars()
                .count(),
            MAX_TITLE_CHARS + 1
        );
    }
}
