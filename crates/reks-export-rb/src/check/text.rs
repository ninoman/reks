//! The report's wording: group titles with counts, what each finding means
//! for the gig, section titles and the summary line.

use std::collections::HashMap;

use reks_core::history::{HistoryLibrary, SourceSession};

use super::{
    Finding, FindingGroup, FindingItem, FindingKind, ReportSection, Section, Severity, Status,
    Summary, STRICTEST,
};
use crate::device::FsKind;
use crate::dryrun::target_label;
use crate::TargetProfile;

pub(super) const ABSENT: &str = "Not on this drive";

pub(super) fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// "7 tracks in 1 playlist".
pub(super) fn contents(tracks: usize, playlists: usize) -> String {
    format!(
        "{} in {}",
        count(tracks, "track", "tracks"),
        count(playlists, "playlist", "playlists")
    )
}

pub(super) fn fs_label(fs: &FsKind, fs_name: &str) -> String {
    match fs {
        FsKind::Fat32 => "FAT32".into(),
        FsKind::ExFat => "exFAT".into(),
        FsKind::Hfs => "Mac OS Extended (HFS+)".into(),
        FsKind::Other(name) if !name.is_empty() => name.clone(),
        FsKind::Other(_) => fs_name.to_owned(),
    }
}

pub(super) fn target_text(chosen: Option<TargetProfile>) -> String {
    match chosen {
        Some(t) => format!(
            "Audio checked against {}, the players this stick is for.",
            target_label(t)
        ),
        None => format!(
            "Audio checked against {}, the strictest players. Pick the oldest player \
             the stick must work on in Export to USB to check against it.",
            target_label(STRICTEST)
        ),
    }
}

/// `HISTORY 001 · Device Library · 12 tracks`.
pub(super) fn session_item(s: &SourceSession) -> FindingItem {
    let library = match s.library {
        HistoryLibrary::DeviceLibrary => "Device Library",
        HistoryLibrary::OneLibrary => "OneLibrary",
    };
    FindingItem {
        subject: s.source_name.clone(),
        detail: Some(format!(
            "{library} · {}",
            count(s.entries.len(), "track", "tracks")
        )),
        path: None,
    }
}

fn section_title(s: Section) -> &'static str {
    match s {
        Section::Library => "Libraries",
        Section::Filesystem => "File system",
        Section::Playlists => "Playlists",
        Section::Files => "Track files",
        Section::Audio => "Audio formats",
        Section::History => "Play history",
    }
}

fn title(kind: FindingKind, key: &str, n: usize) -> String {
    use FindingKind::*;
    let tracks = || count(n, "track", "tracks");
    match kind {
        NoLibrary => "No DJ library on this drive".into(),
        NoDeviceLibrary => "No Device Library (export.pdb)".into(),
        NoOneLibrary => "No OneLibrary (exportLibrary.db)".into(),
        DeviceLibraryUnreadable => "export.pdb does not open".into(),
        DeviceLibraryDamaged => "export.pdb is incomplete".into(),
        RowsDamaged if n == 1 => "1 row of export.pdb does not decode".into(),
        RowsDamaged => format!("{n} rows of export.pdb do not decode"),
        OneLibraryNotRead => "exportLibrary.db was not read".into(),
        OneLibraryDamaged => "exportLibrary.db fails its integrity check".into(),
        TrackInOneLibrary => format!("{} in only one library", tracks()),
        FilesystemExFat => "The drive is exFAT".into(),
        FilesystemHfs => "The drive is Mac OS Extended (HFS+)".into(),
        FilesystemUnsupported => format!("The drive is {key}"),
        PlaylistInOneLibrary => {
            format!("{} in only one library", count(n, "playlist", "playlists"))
        }
        PlaylistDiffers => format!(
            "{} differ between the libraries",
            count(n, "playlist", "playlists")
        ),
        PathOutsideStick => format!("{} point outside the stick", tracks()),
        PathUnsafe => format!("{} with a path players may not handle", tracks()),
        AudioMissing => format!("{}: audio file missing", tracks()),
        AudioUnreadable => format!("{}: audio file unreadable", tracks()),
        NoAnalysis => format!("{} without analysis", tracks()),
        AnlzMissing => format!("{}: analysis file missing", tracks()),
        AnlzUnreadable => format!("{}: analysis file damaged", tracks()),
        PpthMismatch => format!("{}: analysis names another file", tracks()),
        NoWaveformPreview => format!("{} without a waveform preview", tracks()),
        ExtMissing => format!("{} without an extended analysis file (.EXT)", tracks()),
        TwoExMissing => format!("{} without a 3-band waveform file (.2EX)", tracks()),
        AudioFormat => format!("{}: {key}", tracks()),
        HistoryNotImported => format!(
            "{} not imported into your library",
            count(n, "play history session", "play history sessions")
        ),
        HistoryOnStick => format!(
            "{} on the stick",
            count(n, "play history session", "play history sessions")
        ),
        HistoryUnreadable => "Play history not read".into(),
    }
}

fn detail(kind: FindingKind, target: TargetProfile) -> String {
    use FindingKind::*;
    match kind {
        NoLibrary => "There is neither export.pdb nor exportLibrary.db, so players can only \
            browse its folders, without waveforms, beatgrids or cues."
            .into(),
        NoDeviceLibrary => "Players that read only export.pdb (CDJ-3000, CDJ-2000NXS2, \
            XDJ-XZ and older) will show no playlists from this stick."
            .into(),
        NoOneLibrary => "Players that read only exportLibrary.db (CDJ-3000X, OPUS-QUAD, \
            XDJ-AZ, OMNIS-DUO and others) will show no playlists from this stick."
            .into(),
        DeviceLibraryUnreadable => "Players that read export.pdb may refuse the stick. \
            Export it again before you play from it."
            .into(),
        DeviceLibraryDamaged => "export.pdb lacks tables every Device Library must have. \
            Players may refuse the stick; export it again."
            .into(),
        RowsDamaged => "These rows are left out of this check. A player may skip them or \
            refuse the stick; export it again."
            .into(),
        OneLibraryNotRead => "Its playlists, tracks and play history are not checked.".into(),
        OneLibraryDamaged => "Players that read exportLibrary.db may refuse the stick. \
            Export it again."
            .into(),
        TrackInOneLibrary => "Players that read the other library won't find these \
            tracks."
            .into(),
        FilesystemExFat => "CDJ-2000NXS2 and older players can't read exFAT. FAT32 works \
            on every player."
            .into(),
        FilesystemHfs => "Only some players read HFS+. FAT32 works on every player.".into(),
        FilesystemUnsupported => "Players can't read this file system. Use a stick \
            formatted as FAT32."
            .into(),
        PlaylistInOneLibrary => "Players that read the other library won't show these \
            playlists."
            .into(),
        PlaylistDiffers => "Players show different track lists depending on which \
            library they read."
            .into(),
        PathOutsideStick => "The library names a file outside the stick. Players can't \
            load these tracks."
            .into(),
        PathUnsafe => "Too long, too deep, not NFC-normalised or with characters FAT32 \
            does not allow. Some players may not find these files."
            .into(),
        AudioMissing => "Players list these tracks but can't load them.".into(),
        AudioUnreadable => "The file's header does not read as audio. Players will \
            probably refuse it."
            .into(),
        NoAnalysis => "Players show no waveform, beatgrid or cues for these tracks.".into(),
        AnlzMissing => "The library names an analysis file that is not on the stick, so \
            players show no waveform, beatgrid or cues."
            .into(),
        AnlzUnreadable => "The analysis file does not parse. Players may show no waveform \
            or refuse the track."
            .into(),
        PpthMismatch => "The analysis file names another audio file, so players won't \
            link it to the track."
            .into(),
        NoWaveformPreview => "After rekordbox 7.2.12 wrote sticks players could not load, \
            AlphaTheta told users to look for tracks like these."
            .into(),
        ExtMissing => "Players that read it show no colour waveform, cue colours or \
            comments for these tracks."
            .into(),
        TwoExMissing => "rekordbox 6 and newer write it for 3-band waveforms. Older \
            exports don't have it."
            .into(),
        AudioFormat => format!("Read from the file headers, for {}.", target_label(target)),
        HistoryNotImported => "Import them (Histories → +) so their plays count. reks \
            asks before an export would replace them."
            .into(),
        HistoryOnStick => "Not compared with a library.".into(),
        HistoryUnreadable => "The check could not tell whether this stick holds plays \
            your library lacks."
            .into(),
    }
}

/// Groups findings by kind, key and severity (in the order first found),
/// then by section in a fixed order, most severe first in each section.
pub(super) fn sections(findings: Vec<Finding>, target: TargetProfile) -> Vec<ReportSection> {
    let mut groups: Vec<(Finding, Vec<FindingItem>)> = Vec::new();
    let mut index: HashMap<(FindingKind, String, Severity), usize> = HashMap::new();
    for f in findings {
        let at = *index
            .entry((f.kind, f.key.clone(), f.severity))
            .or_insert_with(|| {
                groups.push((f.clone(), Vec::new()));
                groups.len() - 1
            });
        if let Some(item) = f.item {
            groups[at].1.push(item);
        }
    }
    let mut out: Vec<ReportSection> = Vec::new();
    for (f, items) in groups {
        let section = f.kind.section();
        let group = FindingGroup {
            kind: f.kind,
            severity: f.severity,
            title: title(f.kind, &f.key, items.len().max(1)),
            detail: detail(f.kind, target),
            items,
        };
        match out.iter_mut().find(|s| s.section == section) {
            Some(s) => s.groups.push(group),
            None => out.push(ReportSection {
                section,
                title: section_title(section).to_owned(),
                groups: vec![group],
            }),
        }
    }
    out.sort_by_key(|s| s.section);
    for s in &mut out {
        s.groups.sort_by_key(|g| g.severity);
    }
    out
}

pub(super) fn summary(sections: &[ReportSection]) -> Summary {
    let mut n = [0usize; 3];
    for g in sections.iter().flat_map(|s| &s.groups) {
        n[g.severity as usize] += g.items.len().max(1);
    }
    let [problems, warnings, notes] = n;
    let status = if problems > 0 {
        Status::Problems
    } else if warnings > 0 {
        Status::Warnings
    } else {
        Status::Ok
    };
    let headline = match (problems, warnings) {
        (0, 0) => "Nothing wrong found".to_owned(),
        (0, w) => count(w, "warning", "warnings"),
        (p, 0) => count(p, "problem", "problems"),
        (p, w) => format!(
            "{}, {}",
            count(p, "problem", "problems"),
            count(w, "warning", "warnings")
        ),
    };
    Summary {
        status,
        problems,
        warnings,
        notes,
        headline,
    }
}
