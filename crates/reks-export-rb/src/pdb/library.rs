//! Typed read of a whole `export.pdb`: every track row field, the lookup
//! tables and the playlist tree with its entries (Phase 4 task 4), on the
//! bounds-checked reader in [`super`].
//!
//! The OneLibrary writer builds `exportLibrary.db` from this, so the two
//! libraries on a stick describe the same rows, ids and paths, as rekordbox's
//! do. [`Mode::Strict`] ([`read_pdb`]) fails on a track, playlist or entry
//! row that does not decode: a writer must not drop tracks silently.
//! [`Mode::Lenient`] ([`read_pdb_lenient`]) leaves such a row out and
//! describes it, for reports ([`crate::check`]). Lookup rows (artists,
//! albums, …) that do not decode are left out in both modes; a track then
//! shows no name for that id.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::{rows, Pdb, Table};
use crate::validate::{locate, PDB_PATH};

/// How a damaged row is handled (module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Strict,
    Lenient,
}

/// A track row, field for field (crate-digger `track_row`). Ids are 0 when
/// unset.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PdbTrack {
    pub id: u32,
    /// `bitmask` (0x04); rekordbox and baken write 0xC0700, the value
    /// OneLibrary's `content.contentLink` carries.
    pub bitmask: u32,
    pub sample_rate: u32,
    pub composer_id: u32,
    pub file_size: u32,
    /// 0x14, unknown; baken writes the row id + 20.
    pub unknown_0x14: u32,
    /// 0x18; OneLibrary's `content.masterDbId` on rekordbox sticks.
    pub master_db_id: u32,
    pub artwork_id: u32,
    pub key_id: u32,
    pub original_artist_id: u32,
    pub label_id: u32,
    pub remixer_id: u32,
    pub bitrate: u32,
    pub track_number: u32,
    /// Centi-BPM.
    pub tempo: u32,
    pub genre_id: u32,
    pub album_id: u32,
    pub artist_id: u32,
    pub disc_number: u16,
    pub play_count: u16,
    pub year: u16,
    pub sample_depth: u16,
    /// Whole seconds.
    pub duration: u16,
    /// 0x56; 41 when analysed, OneLibrary's `content.analysedBits`.
    pub analysed_bits: u16,
    pub color_id: u8,
    pub rating: u8,
    pub file_type: u16,
    pub isrc: String,
    pub kuvo_public: String,
    pub autoload_hotcues: String,
    pub date_added: String,
    pub release_date: String,
    pub mix_name: String,
    pub analyze_path: String,
    pub comment: String,
    pub title: String,
    pub filename: String,
    pub file_path: String,
}

/// An album row: name and album artist id (0 when unset).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PdbAlbum {
    pub name: String,
    pub artist_id: u32,
}

/// A playlist tree node and, for a playlist, its track ids in entry order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PdbPlaylist {
    pub id: u32,
    /// 0 at the root.
    pub parent_id: u32,
    pub sort_order: u32,
    pub name: String,
    pub is_folder: bool,
    pub entries: Vec<u32>,
}

/// Everything [`read_pdb`] decodes, keyed by row id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PdbLibrary {
    /// In table order.
    pub tracks: Vec<PdbTrack>,
    pub artists: BTreeMap<u32, String>,
    pub albums: BTreeMap<u32, PdbAlbum>,
    pub genres: BTreeMap<u32, String>,
    pub labels: BTreeMap<u32, String>,
    pub keys: BTreeMap<u32, String>,
    pub colors: BTreeMap<u32, String>,
    /// Artwork id → image path.
    pub artwork: BTreeMap<u32, String>,
    /// In table order; entries sorted by entry index.
    pub playlists: Vec<PdbPlaylist>,
}

#[derive(Debug, thiserror::Error)]
pub enum PdbReadError {
    #[error("no export.pdb under {}", root.display())]
    Missing { root: PathBuf },
    #[error("cannot read {}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("export.pdb: {0}")]
    Unreadable(String),
}

/// Reads the stick's `export.pdb` under `device_root`.
pub fn read_stick(device_root: &Path) -> Result<PdbLibrary, PdbReadError> {
    let io = |path: &Path| {
        let path = path.to_owned();
        move |source| PdbReadError::Io { path, source }
    };
    let path = locate(device_root, PDB_PATH)
        .map_err(io(device_root))?
        .ok_or_else(|| PdbReadError::Missing {
            root: device_root.to_owned(),
        })?;
    let bytes = std::fs::read(&path).map_err(io(&path))?;
    read_pdb(&bytes).map_err(PdbReadError::Unreadable)
}

/// Decodes an `export.pdb`'s tracks, lookups and playlists. Any track,
/// playlist or entry row that does not decode is an error.
pub fn read_pdb(bytes: &[u8]) -> Result<PdbLibrary, String> {
    read(bytes, Mode::Strict).map(|(lib, _)| lib)
}

/// [`read_pdb`] for reports ([`crate::check`]): a row that does not decode
/// is left out and described in the second value, in row order. `Err` only
/// when the header or a page chain can't be walked.
pub fn read_pdb_lenient(bytes: &[u8]) -> Result<(PdbLibrary, Vec<String>), String> {
    read(bytes, Mode::Lenient)
}

/// [`read_pdb`] or [`read_pdb_lenient`] by `mode`; in strict mode the
/// damage list is always empty.
pub fn read(bytes: &[u8], mode: Mode) -> Result<(PdbLibrary, Vec<String>), String> {
    let pdb = Pdb::parse(bytes).map_err(|e| e.to_string())?;
    let rows = |t| pdb.rows_of(t).map_err(|e| e.to_string());
    let mut lib = PdbLibrary::default();
    let mut damage = Vec::new();
    let mut bad = |table: Table, e: rows::RowError| -> Result<(), String> {
        let message = format!("{table:?} row does not decode: {e}");
        match mode {
            Mode::Strict => Err(message),
            Mode::Lenient => {
                damage.push(message);
                Ok(())
            }
        }
    };
    for r in rows(Table::Tracks)? {
        match rows::track(r) {
            Ok(t) => lib.tracks.push(t),
            Err(e) => bad(Table::Tracks, e)?,
        }
    }
    type Lookup = fn(&[u8]) -> Result<(u32, String), rows::RowError>;
    let lookups: [(Table, &mut BTreeMap<u32, String>, Lookup); 6] = [
        (Table::Genres, &mut lib.genres, |r| rows::named(r, 4)),
        (Table::Labels, &mut lib.labels, |r| rows::named(r, 4)),
        (Table::Artwork, &mut lib.artwork, |r| rows::named(r, 4)),
        (Table::Keys, &mut lib.keys, |r| rows::named(r, 8)),
        (Table::Colors, &mut lib.colors, rows::color),
        (Table::Artists, &mut lib.artists, rows::artist),
    ];
    for (table, map, decode) in lookups {
        map.extend(rows(table)?.into_iter().filter_map(|r| decode(r).ok()));
    }
    lib.albums.extend(
        rows(Table::Albums)?
            .into_iter()
            .filter_map(|r| rows::album(r).ok()),
    );
    for r in rows(Table::PlaylistTree)? {
        match rows::playlist_node(r) {
            Ok(p) => lib.playlists.push(p),
            Err(e) => bad(Table::PlaylistTree, e)?,
        }
    }
    let mut entries: Vec<(u32, u32, u32)> = Vec::new(); // (playlist, index, track)
    for r in rows(Table::PlaylistEntries)? {
        match rows::playlist_entry(r) {
            Ok(e) => entries.push(e),
            Err(e) => bad(Table::PlaylistEntries, e)?,
        }
    }
    entries.sort_unstable();
    for (playlist, _, track) in entries {
        match lib.playlists.iter_mut().find(|p| p.id == playlist) {
            Some(p) => p.entries.push(track),
            None => {
                let message = format!("entry for missing playlist {playlist}");
                match mode {
                    Mode::Strict => return Err(message),
                    Mode::Lenient => damage.push(message),
                }
            }
        }
    }
    Ok((lib, damage))
}

#[cfg(test)]
mod tests;
