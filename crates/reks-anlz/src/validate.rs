//! ANLZ validator: checks one `.DAT`/`.EXT`/`.2EX` file, or a set, for the
//! structure players rely on. [`crate::build`] runs it on everything it
//! produces; it also reads rekordbox's own files (the golden tests), so
//! sections we never write (`PSSI`, a full `PQT2`) are allowed.
//!
//! Checks: `PMAI` framing; `PPTH` first and equal to the expected path in
//! every file; only sections that belong in that file, and the required ones
//! present; fixed preview sizes; entry counts that match payloads; detail
//! waveforms (PWV3/PWV5/PWV7) with one column count, close to 150/s of the
//! duration; beats in order with `beat_number` 1–4 and a tempo; cues inside
//! the track, loops that end after they start, and `PCOB`/`PCO2` lists that
//! agree on how many cues there are.

use std::fmt;

use crate::read::{self, ReadError, Section};
use crate::waveform::{COLOR_PREVIEW_COLUMNS, PWAV_COLUMNS, PWV2_COLUMNS};
use crate::write::{
    LIST_HOT, LIST_MEMORY, PCO2, PCOB, PMAI_HEADER_LEN, PPTH, PQT2, PQTZ, PVBR, PVBR_ENTRIES, PWAV,
    PWV2, PWV3, PWV4, PWV5, PWV6, PWV7, PWVC,
};

/// Phrase analysis; rekordbox writes it, we don't.
const PSSI: [u8; 4] = *b"PSSI";
/// Detail columns may differ from 150/s of the duration by this many (1 s):
/// the duration the library stores and the decoded length need not agree to
/// the frame.
const DETAIL_TOLERANCE: u64 = 150;

/// Which of the three files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileKind {
    Dat,
    Ext,
    Ex2,
}

impl FileKind {
    /// File extension, as on the stick.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Dat => "DAT",
            Self::Ext => "EXT",
            Self::Ex2 => "2EX",
        }
    }

    fn allowed(self) -> &'static [[u8; 4]] {
        match self {
            Self::Dat => &[PPTH, PVBR, PQTZ, PWAV, PWV2, PCOB],
            Self::Ext => &[PPTH, PWV3, PCOB, PCO2, PQT2, PWV5, PWV4, PSSI],
            Self::Ex2 => &[PPTH, PWV7, PWV6, PWVC],
        }
    }

    fn required(self) -> &'static [[u8; 4]] {
        match self {
            Self::Dat => &[PPTH, PQTZ, PWAV, PWV2, PCOB],
            Self::Ext => &[PPTH, PWV3, PCOB, PCO2, PWV5, PWV4],
            Self::Ex2 => &[PPTH, PWV7, PWV6],
        }
    }
}

impl fmt::Display for FileKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.extension())
    }
}

/// What a file is checked against; `None` skips that check.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Expect<'a> {
    /// The `PPTH` path (`/Contents/...`).
    pub path: Option<&'a str>,
    /// Track length.
    pub duration_ms: Option<u64>,
}

/// One problem found.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Issue {
    #[error("{file}: unreadable: {error}")]
    Unreadable { file: FileKind, error: ReadError },
    #[error("{file}: PMAI header is {len} bytes, expected 28")]
    Header { file: FileKind, len: usize },
    #[error("{file}: PPTH is not the first section")]
    PathNotFirst { file: FileKind },
    #[error("{file}: PPTH is {found:?}, expected {expected:?}")]
    PathMismatch {
        file: FileKind,
        found: String,
        expected: String,
    },
    #[error("{file}: no {tag} section")]
    Missing { file: FileKind, tag: String },
    #[error("{file}: {tag} does not belong in this file")]
    Unexpected { file: FileKind, tag: String },
    #[error("{file}: {tag}: {what}")]
    Section {
        file: FileKind,
        tag: String,
        what: String,
    },
    #[error("{file}: beat {index}: {what}")]
    Beat {
        file: FileKind,
        index: usize,
        what: &'static str,
    },
    #[error("{file}: cue at {time_ms} ms: {what}")]
    Cue {
        file: FileKind,
        time_ms: u32,
        what: &'static str,
    },
    #[error("detail waveforms disagree: {what}")]
    Detail { what: String },
    #[error("cue lists disagree: {what}")]
    CueCounts { what: String },
}

fn tag(t: [u8; 4]) -> String {
    String::from_utf8_lossy(&t).into_owned()
}

/// Facts a file contributes to the cross-file checks.
#[derive(Debug, Default)]
struct Facts {
    path: Option<String>,
    detail_columns: Vec<(String, u64)>,
    /// (list type, cue count) per `PCOB`.
    pcob: Vec<(u32, usize)>,
    /// (list type, cue count) per `PCO2`.
    pco2: Vec<(u32, usize)>,
}

struct Checker<'e> {
    file: FileKind,
    expect: Expect<'e>,
    issues: Vec<Issue>,
    facts: Facts,
}

impl Checker<'_> {
    fn section_issue(&mut self, s: &Section<'_>, what: impl Into<String>) {
        self.issues.push(Issue::Section {
            file: self.file,
            tag: s.tag_str(),
            what: what.into(),
        });
    }

    /// Header fields must be `words` long and the payload `entries ×
    /// entry_len` bytes where `entries` is the header word at `count_at`.
    fn check_entries(&mut self, s: &Section<'_>, header: usize, count_at: usize, entry_len: usize) {
        if s.len_header != 12 + header {
            self.section_issue(
                s,
                format!("len_header {} (expected {})", s.len_header, 12 + header),
            );
            return;
        }
        match s.u32(count_at) {
            Ok(n) if (n as usize).checked_mul(entry_len) == Some(s.payload().len()) => {}
            Ok(n) => self.section_issue(
                s,
                format!(
                    "{n} entries of {entry_len} bytes but {} payload bytes",
                    s.payload().len()
                ),
            ),
            Err(e) => self.section_issue(s, e.to_string()),
        }
    }

    fn check_fixed(
        &mut self,
        s: &Section<'_>,
        header: usize,
        count_at: usize,
        entry_len: usize,
        n: usize,
    ) {
        self.check_entries(s, header, count_at, entry_len);
        if s.payload().len() != n * entry_len {
            self.section_issue(
                s,
                format!("{} bytes, expected {n} columns", s.payload().len()),
            );
        }
    }

    fn check_detail(&mut self, s: &Section<'_>, entry_len: usize) {
        self.check_entries(s, 12, 16, entry_len);
        let columns = (s.payload().len() / entry_len) as u64;
        self.facts.detail_columns.push((s.tag_str(), columns));
    }

    fn check_path(&mut self, s: &Section<'_>) {
        match read::path(s) {
            Ok(p) => {
                if let Some(want) = self.expect.path {
                    if p != want {
                        self.issues.push(Issue::PathMismatch {
                            file: self.file,
                            found: p.clone(),
                            expected: want.to_owned(),
                        });
                    }
                }
                let units = p.encode_utf16().count();
                if s.u32(12).ok() != Some(2 * (units as u32 + 1))
                    || s.payload().len() != 2 * (units + 1)
                {
                    self.section_issue(s, "len_path does not match the path");
                }
                self.facts.path = Some(p);
            }
            Err(e) => self.section_issue(s, e.to_string()),
        }
    }

    fn check_beats(&mut self, s: &Section<'_>) {
        if s.len_header != 24 {
            self.section_issue(s, format!("len_header {}", s.len_header));
        }
        let beats = match read::pqtz_beats(s) {
            Ok(b) => b,
            Err(e) => return self.section_issue(s, e.to_string()),
        };
        let mut prev: Option<u32> = None;
        for (index, b) in beats.iter().enumerate() {
            let what = if !(1..=4).contains(&b.beat_number) {
                Some("beat_number is not 1–4")
            } else if b.tempo == 0 {
                Some("tempo is 0")
            } else if prev.is_some_and(|p| b.time_ms <= p) {
                Some("not after the previous beat")
            } else if self
                .expect
                .duration_ms
                .is_some_and(|d| u64::from(b.time_ms) > d)
            {
                Some("after the end of the track")
            } else {
                None
            };
            if let Some(what) = what {
                self.issues.push(Issue::Beat {
                    file: self.file,
                    index,
                    what,
                });
            }
            prev = Some(b.time_ms);
        }
    }

    fn check_cues(&mut self, s: &Section<'_>) {
        let header = if s.tag == PCOB { 12 } else { 8 };
        if s.len_header != 12 + header {
            self.section_issue(s, format!("len_header {}", s.len_header));
        }
        let list = match read::cue_list(s) {
            Ok(l) => l,
            Err(e) => return self.section_issue(s, e.to_string()),
        };
        if list.list_type != LIST_HOT && list.list_type != LIST_MEMORY {
            self.section_issue(s, format!("list type {}", list.list_type));
        }
        for c in &list.cues {
            let hot_ok = if list.list_type == LIST_HOT {
                (1..=8).contains(&c.hot_cue)
            } else {
                c.hot_cue == 0
            };
            let what = if !hot_ok {
                Some("hot cue number does not fit the list")
            } else if c.loop_end_ms.is_some_and(|e| e <= c.time_ms) {
                Some("loop ends before it starts")
            } else if self
                .expect
                .duration_ms
                .is_some_and(|d| u64::from(c.time_ms) > d)
            {
                Some("after the end of the track")
            } else {
                None
            };
            if let Some(what) = what {
                self.issues.push(Issue::Cue {
                    file: self.file,
                    time_ms: c.time_ms,
                    what,
                });
            }
        }
        let entry = (list.list_type, list.cues.len());
        if s.tag == PCOB {
            self.facts.pcob.push(entry);
        } else {
            self.facts.pco2.push(entry);
        }
    }

    fn check_section(&mut self, s: &Section<'_>) {
        match s.tag {
            PPTH => self.check_path(s),
            PVBR if s.len_header != 16 || s.payload().len() != 4 * PVBR_ENTRIES + 4 => {
                self.section_issue(s, "unexpected PVBR size");
            }
            PQTZ => self.check_beats(s),
            PCOB | PCO2 => self.check_cues(s),
            PWAV => self.check_fixed(s, 8, 12, 1, PWAV_COLUMNS),
            PWV2 => self.check_fixed(s, 8, 12, 1, PWV2_COLUMNS),
            PWV4 => self.check_fixed(s, 12, 16, 6, COLOR_PREVIEW_COLUMNS),
            PWV6 => self.check_fixed(s, 8, 16, 3, COLOR_PREVIEW_COLUMNS),
            PWV3 => self.check_detail(s, 1),
            PWV5 => self.check_detail(s, 2),
            PWV7 => self.check_detail(s, 3),
            PWVC if s.len_header != 14 || s.payload().len() != 6 => {
                self.section_issue(s, "unexpected PWVC size");
            }
            _ => {}
        }
    }
}

fn check_file(file: FileKind, bytes: &[u8], expect: Expect<'_>) -> (Vec<Issue>, Facts) {
    let mut c = Checker {
        file,
        expect,
        issues: Vec::new(),
        facts: Facts::default(),
    };
    let sections = match read::sections(bytes) {
        Ok(s) => s,
        Err(error) => {
            c.issues.push(Issue::Unreadable { file, error });
            return (c.issues, c.facts);
        }
    };
    let header_len = bytes
        .get(4..8)
        .map_or(0, |b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize);
    if header_len != PMAI_HEADER_LEN {
        c.issues.push(Issue::Header {
            file,
            len: header_len,
        });
    }
    if sections.first().map(|s| s.tag) != Some(PPTH) {
        c.issues.push(Issue::PathNotFirst { file });
    }
    for t in file.required() {
        if !sections.iter().any(|s| s.tag == *t) {
            c.issues.push(Issue::Missing { file, tag: tag(*t) });
        }
    }
    for s in &sections {
        if !file.allowed().contains(&s.tag) {
            c.issues.push(Issue::Unexpected {
                file,
                tag: s.tag_str(),
            });
            continue;
        }
        c.check_section(s);
    }
    if let Some(d) = expect.duration_ms {
        let want = d.saturating_mul(150).div_ceil(1000);
        for (t, n) in &c.facts.detail_columns {
            if n.abs_diff(want) > DETAIL_TOLERANCE {
                c.issues.push(Issue::Detail {
                    what: format!("{t} has {n} columns, {d} ms is {want}"),
                });
            }
        }
    }
    (c.issues, c.facts)
}

/// Checks one file on its own.
pub fn validate_file(file: FileKind, bytes: &[u8], expect: Expect<'_>) -> Vec<Issue> {
    check_file(file, bytes, expect).0
}

/// Checks a set (`.2EX` optional) and what the files must agree on: the
/// same `PPTH`, one detail column count, and as many hot and memory cues in
/// the `PCOB` lists (`.DAT` + `.EXT`) as in the `PCO2` lists.
pub fn validate_set(dat: &[u8], ext: &[u8], ex2: Option<&[u8]>, expect: Expect<'_>) -> Vec<Issue> {
    let mut issues = Vec::new();
    let mut facts = Vec::new();
    let files = [
        (FileKind::Dat, Some(dat)),
        (FileKind::Ext, Some(ext)),
        (FileKind::Ex2, ex2),
    ];
    for (kind, bytes) in files {
        if let Some(bytes) = bytes {
            let (i, f) = check_file(kind, bytes, expect);
            issues.extend(i);
            facts.push((kind, f));
        }
    }
    let paths: Vec<&str> = facts
        .iter()
        .filter_map(|(_, f)| f.path.as_deref())
        .collect();
    if expect.path.is_none() && paths.windows(2).any(|w| w[0] != w[1]) {
        issues.push(Issue::Section {
            file: FileKind::Dat,
            tag: tag(PPTH),
            what: format!("files name different paths: {paths:?}"),
        });
    }
    let columns: Vec<(String, u64)> = facts
        .iter()
        .flat_map(|(_, f)| f.detail_columns.iter().cloned())
        .collect();
    if columns.windows(2).any(|w| w[0].1 != w[1].1) {
        issues.push(Issue::Detail {
            what: format!("column counts {columns:?}"),
        });
    }
    let count = |list: u32, pick: fn(&Facts) -> &Vec<(u32, usize)>| -> usize {
        facts
            .iter()
            .flat_map(|(_, f)| pick(f).iter())
            .filter(|(t, _)| *t == list)
            .map(|(_, n)| n)
            .sum()
    };
    for (list, name) in [(LIST_HOT, "hot"), (LIST_MEMORY, "memory")] {
        let (pcob, pco2) = (count(list, |f| &f.pcob), count(list, |f| &f.pco2));
        if pcob != pco2 {
            issues.push(Issue::CueCounts {
                what: format!("{pcob} {name} cues in PCOB, {pco2} in PCO2"),
            });
        }
    }
    issues
}
