//! Snapshot file names: `reks-<UTC time>-<reason>.sqlite`, e.g.
//! `reks-20261024T153012123Z-startup.sqlite`. The time is `iso8601_utc` with
//! the separators removed, so names sort by age and are legal on every OS.

use super::BackupReason;
use crate::timefmt;

const PREFIX: &str = "reks-";
const EXT: &str = ".sqlite";

/// What a snapshot's file name says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ParsedName {
    /// Unix milliseconds.
    pub taken_ms: i64,
    pub reason: BackupReason,
}

pub(super) fn file_name(taken_ms: i64, reason: BackupReason) -> String {
    let at = std::time::UNIX_EPOCH + std::time::Duration::from_millis(taken_ms.unsigned_abs());
    let iso = timefmt::iso8601_utc(at);
    let compact: String = iso
        .chars()
        .filter(|c| !matches!(c, '-' | ':' | '.'))
        .collect();
    format!("{PREFIX}{compact}-{}{EXT}", reason.slug())
}

/// `None` for files that are not snapshots (leave them alone).
pub(super) fn parse(file_name: &str) -> Option<ParsedName> {
    let rest = file_name.strip_prefix(PREFIX)?.strip_suffix(EXT)?;
    let (time, slug) = rest.split_once('-')?;
    let reason = BackupReason::from_slug(slug)?;
    let b = time.as_bytes();
    if b.len() != 19 || b[8] != b'T' || b[18] != b'Z' || !time.is_ascii() {
        return None;
    }
    let iso = format!(
        "{}-{}-{}T{}:{}:{}.{}Z",
        &time[0..4],
        &time[4..6],
        &time[6..8],
        &time[9..11],
        &time[11..13],
        &time[13..15],
        &time[15..18]
    );
    Some(ParsedName {
        taken_ms: timefmt::parse_iso8601_utc(&iso)?,
        reason,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_round_trips() {
        let ms = 1_792_411_384_567;
        let name = file_name(ms, BackupReason::BeforeRestore);
        assert_eq!(name, "reks-20261019T120304567Z-before-restore.sqlite");
        assert_eq!(
            parse(&name),
            Some(ParsedName {
                taken_ms: ms,
                reason: BackupReason::BeforeRestore
            })
        );
    }

    #[test]
    fn other_files_are_not_snapshots() {
        for name in [
            "reks.sqlite",
            "notes.txt",
            "reks-20261019T120304567Z-startup.sqlite-wal",
            "reks-20261019T120304567Z-nonsense.sqlite",
            "reks-2026-startup.sqlite",
            ".reks-20261019T120304567Z-startup.sqlite.partial",
        ] {
            assert_eq!(parse(name), None, "{name}");
        }
    }
}
