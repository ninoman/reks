//! Dry-run wording: one line per [`Warning`] and [`Problem`], for the CLI
//! and the export UI (task 10), so neither formats plan data itself.

use std::fmt;

use super::{Problem, Warning};
use crate::history::HistoryPolicy;

/// `1.5 MB`-style size (decimal units, as file managers show sticks).
pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if bytes < 1000 {
        return format!("{bytes} B");
    }
    let mut v = bytes as f64 / 1000.0;
    let mut unit = 0;
    while v >= 1000.0 && unit + 1 < UNITS.len() {
        v /= 1000.0;
        unit += 1;
    }
    format!("{v:.1} {}", UNITS[unit])
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExFat { target } => write!(
                f,
                "the stick is exFAT, which older players do not read (target {target:?}); \
                 FAT32 is the safe choice"
            ),
            Self::Hfs => f.write_str(
                "the stick is HFS+, which only some players read; FAT32 is the safe choice",
            ),
            Self::NotRemovable => f.write_str("the OS does not report this volume as removable"),
            Self::RekordboxStick { found } => write!(
                f,
                "rekordbox wrote this stick ({}): reks rewrites its Device Library completely \
                 and rekordbox will show its \"out of matching\" caution afterwards",
                found.join(", ")
            ),
            Self::Audio { track_id, message } => write!(f, "track {track_id}: {message}"),
            Self::Skipped { track_id, reason } => write!(f, "track {track_id} left out: {reason}"),
            Self::StaleKept { files, bytes } => write!(
                f,
                "{files} stale files ({}) stay on the stick",
                human_bytes(*bytes)
            ),
            Self::EmptyPlaylist { id } => {
                write!(f, "playlist {id} is empty after leaving tracks out")
            }
            Self::SmartPlaylistFixed { id } => write!(
                f,
                "smart playlist {id} is written as a fixed list of the tracks it matches now"
            ),
            Self::StickHistory {
                sessions,
                entries,
                unreadable,
                onelibrary_db,
                policy,
            } => {
                let mut found = Vec::new();
                if *sessions > 0 || *entries > 0 {
                    found.push(format!(
                        "export.pdb holds play history ({sessions} sessions, {entries} entries)"
                    ));
                }
                if *unreadable {
                    found.push("export.pdb cannot be read and may hold play history".to_owned());
                }
                if *onelibrary_db {
                    found.push(
                        "exportLibrary.db is present and players may have logged history there"
                            .to_owned(),
                    );
                }
                write!(f, "{}; the export erases it, ", found.join("; "))?;
                f.write_str(match policy {
                    HistoryPolicy::Import => {
                        "so its sessions are imported and the stick's database files backed up first"
                    }
                    HistoryPolicy::BackUp => "so the stick's database files are backed up first",
                    HistoryPolicy::Discard => "and it is discarded without a backup (confirmed)",
                })
            }
            Self::NoVolumeSerial => f.write_str(
                "the OS gave no volume serial: backups go under \"unknown-serial\" and no \
                 export manifest is kept for this stick",
            ),
            Self::OneLibraryExperimental => f.write_str(
                "OneLibrary (exportLibrary.db) is experimental: no player has been tested with \
                 reks's file, and AlphaTheta does not support third-party OneLibrary exports",
            ),
        }
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NothingSelected => f.write_str("no playlist selected"),
            Self::PlaylistNotFound { id } => write!(f, "playlist {id} is not in the library"),
            Self::UnsupportedFilesystem { fs_name } => write!(
                f,
                "players cannot read the {fs_name} file system; format the stick as FAT32"
            ),
            Self::SourceMissing { track_id, path } => {
                write!(f, "track {track_id}: file not found: {}", path.display())
            }
            Self::SourceUnreadable {
                track_id,
                path,
                message,
            } => write!(
                f,
                "track {track_id}: cannot read {}: {message}",
                path.display()
            ),
            Self::Unsupported { track_id, message } => write!(f, "track {track_id}: {message}"),
            Self::BadPath {
                track_id,
                path,
                message,
            } => write!(
                f,
                "track {track_id}: stick path {path} is not allowed: {message}"
            ),
            Self::NotEnoughSpace { needed, free } => write!(
                f,
                "not enough space: the export writes {}, the stick has {} free",
                human_bytes(*needed),
                human_bytes(*free)
            ),
            Self::DiscardNotConfirmed => {
                f.write_str("discarding the stick's play history needs an explicit confirmation")
            }
            Self::HistoryNotImported { sessions } => write!(
                f,
                "the stick holds {sessions} play history session{} not imported into the \
                 library; choose import, back up or discard",
                if *sessions == 1 { "" } else { "s" }
            ),
            Self::HistoryImportUnavailable { message } => write!(
                f,
                "cannot import the stick's play history ({message}); back it up or discard it"
            ),
            Self::OneLibraryKeyMissing => f.write_str(
                "the OneLibrary export needs its key: paste it in Settings → Experimental \
                 or set REKS_ONELIBRARY_KEY",
            ),
            Self::OneLibraryUnavailable => f.write_str(
                "this build of reks cannot write exportLibrary.db yet; turn the OneLibrary \
                 export off",
            ),
            Self::OneLibraryHistoryNotImported { sessions } => write!(
                f,
                "exportLibrary.db on the stick holds {sessions} play history session{} not \
                 in your library; import, back up or discard {}",
                if *sessions == 1 { "" } else { "s" },
                if *sessions == 1 { "it" } else { "them" },
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes() {
        assert_eq!(human_bytes(999), "999 B");
        assert_eq!(human_bytes(1_500_000), "1.5 MB");
        assert_eq!(human_bytes(32_000_000_000), "32.0 GB");
    }

    #[test]
    fn history_wording_names_what_was_found_and_the_policy() {
        let w = Warning::StickHistory {
            sessions: 2,
            entries: 3,
            unreadable: false,
            onelibrary_db: true,
            policy: HistoryPolicy::BackUp,
        };
        assert_eq!(
            w.to_string(),
            "export.pdb holds play history (2 sessions, 3 entries); exportLibrary.db is \
             present and players may have logged history there; the export erases it, so the \
             stick's database files are backed up first"
        );
    }
}
