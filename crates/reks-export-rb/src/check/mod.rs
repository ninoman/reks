//! Check a stick before a gig (maintainer request 2026-10-24, phase 2.5
//! extra task): a read-only report on any mounted drive, whoever wrote it.
//!
//! [`check_stick`] looks at:
//!
//! - the library files on the stick (`export.pdb`, `exportExt.pdb`,
//!   `exportLibrary.db`) and which players read each ([`players`]);
//! - damage in `export.pdb`: missing tables (the validator's check) and rows
//!   that do not decode, each a finding, the rest still checked
//!   ([`crate::pdbread::read_pdb_lenient`]);
//! - playlists in one library but not the other, or with other contents
//!   ([`playlists`]);
//! - every track's audio file, path, ANLZ files (`.DAT`, `.EXT`, `.2EX`),
//!   waveform preview and [`crate::audiocheck`] issues ([`tracks`]);
//! - the file system (D16) and the play history not yet imported (D23).
//!
//! The report is worded here, so the UI and the CLI only show it. Nothing
//! is written to the stick: files are only opened for reading, and
//! `exportLibrary.db` is read through [`OneLibraryRead`], whose
//! implementation (`reks-export-onelibrary`) opens a temp copy. That crate
//! depends on this one, so the app and the CLI pass the reader in, as for
//! [`crate::onelibrary::OneLibraryExport`].

pub mod players;
mod playlists;
mod text;
mod tracks;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use reks_core::history::{SessionAction, SourceHistory, SourceSession};
use reks_core::Library;
use serde::Serialize;
use unicode_normalization::UnicodeNormalization;

use crate::device::{DeviceInfo, FsKind};
use crate::history::{read_play_history, HistoryReadOptions, HistorySource};
use crate::pdbread::{read_pdb_lenient, PdbLibrary};
use crate::plan::human_bytes;
use crate::validate::{self, locate, Finding as ValidatorFinding};
use crate::TargetProfile;
pub use players::LibraryFormat;

/// The players audio is checked against when none are chosen: CDJ-2000NXS
/// (no FLAC or ALAC, 44.1/48 kHz), the strictest [`TargetProfile`].
pub const STRICTEST: TargetProfile = TargetProfile::Cdj2000Nxs;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CheckOptions {
    /// The players the stick is for (its profile's target); `None` checks
    /// against [`STRICTEST`].
    pub target: Option<TargetProfile>,
    /// From [`crate::device::volume_serial`]; matches play history sessions
    /// to the ones the library imported.
    pub volume_serial: Option<String>,
    /// The root is a folder standing in for a stick (CLI, tests): its file
    /// system is the computer's, so it is not checked.
    pub directory: bool,
}

/// Reads an `exportLibrary.db` for the check without writing it.
pub trait OneLibraryRead {
    /// The tracks, playlists and history in the database at `db`. `Err` is
    /// the reason it could not be read (no key, wrong key, damaged), shown
    /// to the user.
    fn read(&self, db: &Path) -> Result<OneLibraryContents, String>;
}

/// What the check needs from an `exportLibrary.db`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OneLibraryContents {
    pub tracks: Vec<OneLibraryTrack>,
    pub playlists: Vec<OneLibraryPlaylist>,
    /// The play history players logged in it.
    pub sessions: Vec<SourceSession>,
    /// `PRAGMA integrity_check`'s answer, `ok` when sound.
    pub integrity: String,
}

/// A `content` row.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OneLibraryTrack {
    pub id: i64,
    pub title: String,
    pub artist: Option<String>,
    /// `/Contents/...`.
    pub path: String,
    /// `analysisDataFilePath`, empty without analysis.
    pub analysis_path: String,
}

/// A `playlist` row with its entries.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OneLibraryPlaylist {
    pub id: i64,
    /// 0 at the root.
    pub parent_id: i64,
    pub sequence_no: i64,
    pub name: String,
    pub is_folder: bool,
    /// `content_id`s in entry order.
    pub track_ids: Vec<i64>,
}

/// The whole report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StickReport {
    pub mount_point: PathBuf,
    pub label: String,
    pub target: TargetProfile,
    /// Which players the audio was checked against, and why.
    pub target_text: String,
    pub summary: Summary,
    /// One row per [`LibraryFormat`].
    pub formats: Vec<FormatRow>,
    /// Where the formats' player lists come from (UNVERIFIED).
    pub players_source: String,
    pub filesystem: FilesystemRow,
    /// Distinct audio files the libraries name.
    pub tracks_checked: usize,
    /// Sections with findings, in a fixed order.
    pub sections: Vec<ReportSection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub status: Status,
    /// Findings by severity (each item of a group counts).
    pub problems: usize,
    pub warnings: usize,
    pub notes: usize,
    pub headline: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    /// Neither problems nor warnings (notes don't count).
    Ok,
    Warnings,
    Problems,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatRow {
    pub format: LibraryFormat,
    pub name: String,
    /// From the stick root.
    pub file: String,
    pub present: bool,
    /// "7 tracks in 1 playlist", "Not on this drive", "Not read: …".
    pub status: String,
    pub players: Vec<String>,
    /// False for every row until a hardware test is logged.
    pub players_verified: bool,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilesystemRow {
    pub fs: FsKind,
    pub fs_name: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
    /// "FAT32 · 3.2 GB free of 15.9 GB".
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportSection {
    pub section: Section,
    pub title: String,
    /// Most severe first.
    pub groups: Vec<FindingGroup>,
}

/// Findings of one kind (and, for audio, one issue) with the same severity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FindingGroup {
    pub kind: FindingKind,
    pub severity: Severity,
    /// With the count: "3 tracks: audio file missing".
    pub title: String,
    /// What it means for the gig and what to do.
    pub detail: String,
    /// One per track, playlist or file; empty for a finding about the stick.
    pub items: Vec<FindingItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FindingItem {
    /// A track ("Artist – Title"), playlist path or file.
    pub subject: String,
    pub detail: Option<String>,
    /// The stick path concerned.
    pub path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    /// Likely to fail on a player.
    Problem,
    /// May fail or look wrong on some players.
    Warning,
    /// For information; does not affect the status.
    Note,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Section {
    Library,
    Filesystem,
    Playlists,
    Files,
    Audio,
    History,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FindingKind {
    NoLibrary,
    NoDeviceLibrary,
    NoOneLibrary,
    DeviceLibraryUnreadable,
    DeviceLibraryDamaged,
    RowsDamaged,
    OneLibraryNotRead,
    OneLibraryDamaged,
    TrackInOneLibrary,
    FilesystemExFat,
    FilesystemHfs,
    FilesystemUnsupported,
    PlaylistInOneLibrary,
    PlaylistDiffers,
    PathOutsideStick,
    PathUnsafe,
    AudioMissing,
    AudioUnreadable,
    NoAnalysis,
    AnlzMissing,
    AnlzUnreadable,
    PpthMismatch,
    NoWaveformPreview,
    ExtMissing,
    TwoExMissing,
    AudioFormat,
    HistoryNotImported,
    HistoryOnStick,
    HistoryUnreadable,
}

impl FindingKind {
    pub fn section(self) -> Section {
        use FindingKind::*;
        match self {
            NoLibrary
            | NoDeviceLibrary
            | NoOneLibrary
            | DeviceLibraryUnreadable
            | DeviceLibraryDamaged
            | RowsDamaged
            | OneLibraryNotRead
            | OneLibraryDamaged
            | TrackInOneLibrary => Section::Library,
            FilesystemExFat | FilesystemHfs | FilesystemUnsupported => Section::Filesystem,
            PlaylistInOneLibrary | PlaylistDiffers => Section::Playlists,
            PathOutsideStick | PathUnsafe | AudioMissing | AudioUnreadable | NoAnalysis
            | AnlzMissing | AnlzUnreadable | PpthMismatch | NoWaveformPreview | ExtMissing
            | TwoExMissing => Section::Files,
            AudioFormat => Section::Audio,
            HistoryNotImported | HistoryOnStick | HistoryUnreadable => Section::History,
        }
    }
}

/// One finding before grouping.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Finding {
    kind: FindingKind,
    severity: Severity,
    /// Splits a kind into groups: the issue for [`FindingKind::AudioFormat`],
    /// the file system name for [`FindingKind::FilesystemUnsupported`].
    key: String,
    item: Option<FindingItem>,
}

/// Findings as they are found.
#[derive(Debug, Default)]
struct Findings(Vec<Finding>);

impl Findings {
    fn stick(&mut self, kind: FindingKind, severity: Severity) {
        self.push(kind, severity, String::new(), None);
    }

    fn item(&mut self, kind: FindingKind, severity: Severity, item: FindingItem) {
        self.push(kind, severity, String::new(), Some(item));
    }

    fn push(
        &mut self,
        kind: FindingKind,
        severity: Severity,
        key: String,
        item: Option<FindingItem>,
    ) {
        self.0.push(Finding {
            kind,
            severity,
            key,
            item,
        });
    }
}

/// The Device Library as read.
enum DeviceLibrary {
    Absent,
    Unreadable(String),
    Read(PdbLibrary),
}

/// The OneLibrary as read.
enum OneLibrary {
    Absent,
    NotRead(String),
    Read(OneLibraryContents),
}

/// A distinct audio file named by either library.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CheckedTrack {
    title: String,
    artist: Option<String>,
    stick_path: String,
    /// From `export.pdb` when it has the track, else `exportLibrary.db`.
    analysis_path: String,
    in_device_library: bool,
    in_onelibrary: bool,
}

impl CheckedTrack {
    /// "Artist – Title", else the title, else the file name.
    fn subject(&self) -> String {
        let title = if self.title.is_empty() {
            self.stick_path.rsplit('/').next().unwrap_or_default()
        } else {
            &self.title
        };
        match self.artist.as_deref().filter(|a| !a.is_empty()) {
            Some(artist) => format!("{artist} – {title}"),
            None => title.to_owned(),
        }
    }

    fn item(&self, detail: Option<String>) -> FindingItem {
        FindingItem {
            subject: self.subject(),
            detail,
            path: Some(self.stick_path.clone()),
        }
    }
}

/// Stick paths compare as FAT32 does: case-insensitively, after NFC.
fn path_key(path: &str) -> String {
    path.nfc().collect::<String>().to_lowercase()
}

/// Checks the stick on `dev`. Never fails and never writes: whatever can't
/// be read becomes a finding.
pub fn check_stick(
    dev: &DeviceInfo,
    opts: &CheckOptions,
    onelibrary: &dyn OneLibraryRead,
    library: Option<&Library>,
) -> StickReport {
    let root = dev.mount_point.as_path();
    let target = opts.target.unwrap_or(STRICTEST);
    let mut out = Findings::default();

    if !opts.directory {
        filesystem(&dev.fs, &dev.fs_name, target, &mut out);
    }
    let pdb = read_device_library(root, &mut out);
    let ext = locate(root, LibraryFormat::DeviceLibraryExt.path())
        .ok()
        .flatten()
        .is_some();
    let one = read_onelibrary(root, onelibrary, &mut out);
    format_findings(&pdb, &one, opts.target.is_some(), &mut out);

    let tracks = merge_tracks(&pdb, &one);
    if matches!(pdb, DeviceLibrary::Read(_)) && matches!(one, OneLibrary::Read(_)) {
        for t in &tracks {
            if t.in_device_library != t.in_onelibrary {
                let only = if t.in_device_library {
                    "Only in the Device Library"
                } else {
                    "Only in OneLibrary"
                };
                out.item(
                    FindingKind::TrackInOneLibrary,
                    Severity::Warning,
                    t.item(Some(only.into())),
                );
            }
        }
    }
    for t in &tracks {
        tracks::check(root, t, target, &mut out);
    }
    if let (DeviceLibrary::Read(pdb), OneLibrary::Read(one)) = (&pdb, &one) {
        playlists::compare(pdb, one, &mut out);
    }
    history(root, opts, &pdb, &one, library, &mut out);

    let formats = format_rows(&pdb, ext, &one);
    let sections = text::sections(out.0, target);
    StickReport {
        mount_point: dev.mount_point.clone(),
        label: dev.label.clone(),
        target,
        target_text: text::target_text(opts.target),
        summary: text::summary(&sections),
        formats,
        players_source: players::SOURCE.to_owned(),
        filesystem: FilesystemRow {
            fs: dev.fs.clone(),
            fs_name: dev.fs_name.clone(),
            total_bytes: dev.total_bytes,
            free_bytes: dev.free_bytes,
            text: if opts.directory {
                "A folder on this computer; file system not checked".to_owned()
            } else {
                format!(
                    "{} · {} free of {}",
                    text::fs_label(&dev.fs, &dev.fs_name),
                    human_bytes(dev.free_bytes),
                    human_bytes(dev.total_bytes)
                )
            },
        },
        tracks_checked: tracks.len(),
        sections,
    }
}

/// D16: FAT32 is read by every player; exFAT only by CDJ-3000, XDJ-XZ,
/// XDJ-RX3 and newer (R§4).
fn filesystem(fs: &FsKind, fs_name: &str, target: TargetProfile, out: &mut Findings) {
    match fs {
        FsKind::Fat32 => {}
        FsKind::ExFat => {
            if !matches!(target, TargetProfile::Cdj3000 | TargetProfile::XdjXz) {
                out.stick(FindingKind::FilesystemExFat, Severity::Warning);
            }
        }
        FsKind::Hfs => out.stick(FindingKind::FilesystemHfs, Severity::Warning),
        FsKind::Other(name) => {
            let name = if name.is_empty() { fs_name } else { name };
            out.push(
                FindingKind::FilesystemUnsupported,
                Severity::Problem,
                name.to_owned(),
                None,
            );
        }
    }
}

fn read_device_library(root: &Path, out: &mut Findings) -> DeviceLibrary {
    let path = match locate(root, validate::PDB_PATH) {
        Ok(Some(path)) => path,
        Ok(None) => return DeviceLibrary::Absent,
        Err(e) => return DeviceLibrary::Unreadable(e.to_string()),
    };
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) => return DeviceLibrary::Unreadable(e.to_string()),
    };
    let (lib, damage) = match read_pdb_lenient(&bytes) {
        Ok(read) => read,
        Err(message) => return DeviceLibrary::Unreadable(message),
    };
    // The validator's table check; its row findings are covered below.
    if let Ok(report) = validate::read(root) {
        for f in report.findings {
            if matches!(
                f,
                ValidatorFinding::TableMissing { .. } | ValidatorFinding::TableUnexpected { .. }
            ) {
                out.item(
                    FindingKind::DeviceLibraryDamaged,
                    Severity::Problem,
                    file_item(validate::PDB_PATH, f.to_string()),
                );
            }
        }
    }
    for message in damage {
        out.item(
            FindingKind::RowsDamaged,
            Severity::Problem,
            file_item(validate::PDB_PATH, message),
        );
    }
    DeviceLibrary::Read(lib)
}

fn read_onelibrary(root: &Path, reader: &dyn OneLibraryRead, out: &mut Findings) -> OneLibrary {
    let path = match locate(root, LibraryFormat::OneLibrary.path()) {
        Ok(Some(path)) => path,
        Ok(None) => return OneLibrary::Absent,
        Err(e) => return OneLibrary::NotRead(e.to_string()),
    };
    match reader.read(&path) {
        Ok(contents) => {
            if contents.integrity != "ok" {
                out.item(
                    FindingKind::OneLibraryDamaged,
                    Severity::Problem,
                    file_item(LibraryFormat::OneLibrary.path(), contents.integrity.clone()),
                );
            }
            OneLibrary::Read(contents)
        }
        Err(reason) => OneLibrary::NotRead(reason),
    }
}

fn file_item(path: &str, detail: String) -> FindingItem {
    FindingItem {
        subject: path.rsplit('/').next().unwrap_or(path).to_owned(),
        detail: Some(detail),
        path: Some(path.to_owned()),
    }
}

/// Which libraries are missing or unreadable. A chosen target is a Device
/// Library player, so then a missing `export.pdb` is a problem.
fn format_findings(pdb: &DeviceLibrary, one: &OneLibrary, targeted: bool, out: &mut Findings) {
    let pdb_absent = matches!(pdb, DeviceLibrary::Absent);
    let one_absent = matches!(one, OneLibrary::Absent);
    if pdb_absent && one_absent {
        out.stick(FindingKind::NoLibrary, Severity::Problem);
    } else if pdb_absent {
        let severity = if targeted {
            Severity::Problem
        } else {
            Severity::Warning
        };
        out.stick(FindingKind::NoDeviceLibrary, severity);
    } else if one_absent {
        out.stick(FindingKind::NoOneLibrary, Severity::Warning);
    }
    if let DeviceLibrary::Unreadable(message) = pdb {
        out.item(
            FindingKind::DeviceLibraryUnreadable,
            Severity::Problem,
            file_item(validate::PDB_PATH, message.clone()),
        );
    }
    if let OneLibrary::NotRead(reason) = one {
        out.item(
            FindingKind::OneLibraryNotRead,
            Severity::Warning,
            file_item(LibraryFormat::OneLibrary.path(), reason.clone()),
        );
    }
}

/// Every audio file either library names, `export.pdb`'s first.
fn merge_tracks(pdb: &DeviceLibrary, one: &OneLibrary) -> Vec<CheckedTrack> {
    let mut tracks: Vec<CheckedTrack> = Vec::new();
    let mut by_path: HashMap<String, usize> = HashMap::new();
    if let DeviceLibrary::Read(lib) = pdb {
        for t in &lib.tracks {
            let key = path_key(&t.file_path);
            if by_path.contains_key(&key) {
                continue;
            }
            by_path.insert(key, tracks.len());
            tracks.push(CheckedTrack {
                title: t.title.clone(),
                artist: lib.artists.get(&t.artist_id).cloned(),
                stick_path: t.file_path.clone(),
                analysis_path: t.analyze_path.clone(),
                in_device_library: true,
                in_onelibrary: false,
            });
        }
    }
    if let OneLibrary::Read(contents) = one {
        for t in &contents.tracks {
            let key = path_key(&t.path);
            match by_path.get(&key) {
                Some(&i) => tracks[i].in_onelibrary = true,
                None => {
                    by_path.insert(key, tracks.len());
                    tracks.push(CheckedTrack {
                        title: t.title.clone(),
                        artist: t.artist.clone(),
                        stick_path: t.path.clone(),
                        analysis_path: t.analysis_path.clone(),
                        in_device_library: false,
                        in_onelibrary: true,
                    });
                }
            }
        }
    }
    tracks
}

/// D23: sessions on the stick that the library has not imported. Without a
/// library, only how many there are.
fn history(
    root: &Path,
    opts: &CheckOptions,
    pdb: &DeviceLibrary,
    one: &OneLibrary,
    library: Option<&Library>,
    out: &mut Findings,
) {
    let mut sessions = Vec::new();
    if matches!(pdb, DeviceLibrary::Read(_)) {
        let read = read_play_history(
            &HistorySource::Stick(root.to_owned()),
            &HistoryReadOptions {
                stick_serial: opts.volume_serial.clone(),
                hist1_dates: false,
            },
        );
        match read {
            Ok(h) => sessions.extend(h.to_source_history().sessions),
            Err(e) => out.item(
                FindingKind::HistoryUnreadable,
                Severity::Warning,
                file_item(validate::PDB_PATH, e.to_string()),
            ),
        }
    }
    if let OneLibrary::Read(contents) = one {
        sessions.extend(contents.sessions.iter().cloned());
    }
    if sessions.is_empty() {
        return;
    }
    let source = SourceHistory {
        volume_serial: opts.volume_serial.clone(),
        sessions,
    };
    let Some(lib) = library else {
        for s in &source.sessions {
            out.item(
                FindingKind::HistoryOnStick,
                Severity::Note,
                text::session_item(s),
            );
        }
        return;
    };
    match reks_core::history::scan(&source, lib) {
        Ok(plan) => {
            for (planned, s) in plan.sessions.iter().zip(&source.sessions) {
                if matches!(
                    planned.action,
                    SessionAction::New | SessionAction::Extend { .. }
                ) {
                    out.item(
                        FindingKind::HistoryNotImported,
                        Severity::Note,
                        text::session_item(s),
                    );
                }
            }
        }
        Err(e) => out.item(
            FindingKind::HistoryUnreadable,
            Severity::Warning,
            FindingItem {
                subject: "Your library".into(),
                detail: Some(e.to_string()),
                path: None,
            },
        ),
    }
}

fn format_rows(pdb: &DeviceLibrary, ext: bool, one: &OneLibrary) -> Vec<FormatRow> {
    LibraryFormat::ALL
        .iter()
        .map(|&format| {
            let (present, status) = match format {
                LibraryFormat::DeviceLibrary => match pdb {
                    DeviceLibrary::Absent => (false, text::ABSENT.to_owned()),
                    DeviceLibrary::Unreadable(m) => (true, format!("Does not open: {m}")),
                    DeviceLibrary::Read(lib) => (
                        true,
                        text::contents(
                            lib.tracks.len(),
                            lib.playlists.iter().filter(|p| !p.is_folder).count(),
                        ),
                    ),
                },
                LibraryFormat::DeviceLibraryExt if ext => (true, "On this drive".to_owned()),
                LibraryFormat::DeviceLibraryExt => (false, text::ABSENT.to_owned()),
                LibraryFormat::OneLibrary => match one {
                    OneLibrary::Absent => (false, text::ABSENT.to_owned()),
                    OneLibrary::NotRead(r) => (true, format!("Not read: {r}")),
                    OneLibrary::Read(c) => (
                        true,
                        text::contents(
                            c.tracks.len(),
                            c.playlists.iter().filter(|p| !p.is_folder).count(),
                        ),
                    ),
                },
            };
            FormatRow {
                format,
                name: format.name().to_owned(),
                file: format.path().to_owned(),
                present,
                status,
                players: format.players().iter().map(|&p| p.to_owned()).collect(),
                players_verified: false,
                note: format.note().to_owned(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests;
