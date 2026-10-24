//! Which player families read which library file, per AlphaTheta's notice
//! of 2026-03-18 and the rekordbox FAQ (R§2). No player here has a logged
//! hardware test (`docs/hw-test-log.md`), so every row is UNVERIFIED.

use serde::Serialize;

/// A library file a stick can hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LibraryFormat {
    /// `export.pdb`.
    DeviceLibrary,
    /// `exportExt.pdb`, which rekordbox writes beside `export.pdb`.
    DeviceLibraryExt,
    /// `exportLibrary.db`.
    OneLibrary,
}

impl LibraryFormat {
    pub const ALL: [LibraryFormat; 3] = [
        LibraryFormat::DeviceLibrary,
        LibraryFormat::DeviceLibraryExt,
        LibraryFormat::OneLibrary,
    ];

    /// The file, from the stick root.
    pub fn path(self) -> &'static str {
        match self {
            LibraryFormat::DeviceLibrary => crate::validate::PDB_PATH,
            LibraryFormat::DeviceLibraryExt => "/PIONEER/rekordbox/exportExt.pdb",
            LibraryFormat::OneLibrary => crate::onelibrary::DB_PATH,
        }
    }

    /// What the format is called in rekordbox and in the report.
    pub fn name(self) -> &'static str {
        match self {
            LibraryFormat::DeviceLibrary => "Device Library",
            LibraryFormat::DeviceLibraryExt => "Device Library extension",
            LibraryFormat::OneLibrary => "OneLibrary",
        }
    }

    /// The players that read this file (R§2). Empty for `exportExt.pdb`:
    /// the notice names none.
    pub fn players(self) -> &'static [&'static str] {
        match self {
            LibraryFormat::DeviceLibrary => &[
                "CDJ-3000",
                "CDJ-2000NXS2 and older CDJ-2000 models",
                "CDJ-900NXS2",
                "XDJ-XZ",
                "XDJ-RX3",
                "XDJ-RR",
                "XDJ-1000MK2",
                "XDJ-700",
            ],
            LibraryFormat::DeviceLibraryExt => &[],
            LibraryFormat::OneLibrary => &[
                "CDJ-3000X",
                "CDJ-1500X",
                "OPUS-QUAD",
                "OMNIS-DUO",
                "XDJ-AZ",
                "XDJ-AN",
            ],
        }
    }

    /// One line on what the file is for.
    pub fn note(self) -> &'static str {
        match self {
            LibraryFormat::DeviceLibrary => {
                "The classic rekordbox stick library. Players that read it ignore exportLibrary.db."
            }
            LibraryFormat::DeviceLibraryExt => {
                "Extra data rekordbox writes beside export.pdb. reks does not write it; \
                 the notice names no player that needs it."
            }
            LibraryFormat::OneLibrary => {
                "The newer encrypted library. Players that read it ignore export.pdb."
            }
        }
    }
}

/// Where the player list comes from, shown under the formats table.
pub const SOURCE: &str = "AlphaTheta notice of 2026-03-18 and the rekordbox FAQ. \
UNVERIFIED: reks has not tested any of these players.";
