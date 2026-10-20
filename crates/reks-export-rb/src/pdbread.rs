//! Typed read of a whole `export.pdb`: every track row field, the lookup
//! tables and the playlist tree with its entries (Phase 4 task 4).
//!
//! The OneLibrary writer builds `exportLibrary.db` from this, so the two
//! libraries on a stick describe the same rows, ids and paths, as rekordbox's
//! do. Rows are decoded from their raw bytes at crate-digger `pdb.ksy`
//! offsets (rekordcrate 0.3.0 keeps its row fields private), walking pages as
//! [`crate::history`] does. Unlike [`crate::validate::read`], which reports
//! damage, a row that does not decode is an error here: a writer must not
//! drop tracks silently.

use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use binrw::BinRead;
use rekordcrate::pdb::string::DeviceSQLString;
use rekordcrate::pdb::PageType;

use crate::history::read::raw_rows;
use crate::history::walk_pages;
use crate::validate::{locate, PDB_PATH};

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

const TABLES: [PageType; 10] = [
    PageType::Tracks,
    PageType::Genres,
    PageType::Artists,
    PageType::Albums,
    PageType::Labels,
    PageType::Keys,
    PageType::Colors,
    PageType::PlaylistTree,
    PageType::PlaylistEntries,
    PageType::Artwork,
];

/// Decodes an `export.pdb`'s tracks, lookups and playlists.
pub fn read_pdb(bytes: &[u8]) -> Result<PdbLibrary, String> {
    let mut rows: Vec<(PageType, Vec<u8>)> = Vec::new();
    walk_pages(bytes, &TABLES, |page, raw| {
        if page.has_data() {
            rows.extend(raw_rows(page, raw).into_iter().map(|r| (page.page_type, r)));
        }
    })?;
    let mut lib = PdbLibrary::default();
    let mut entries: Vec<(u32, u32, u32)> = Vec::new(); // (playlist, index, track)
    for (table, r) in &rows {
        let r = r.as_slice();
        let bad = || format!("{table:?} row does not decode");
        match table {
            PageType::Tracks => lib.tracks.push(track(r).ok_or_else(bad)?),
            PageType::Genres => insert(&mut lib.genres, u32_at(r, 0), string(r, 4)),
            PageType::Labels => insert(&mut lib.labels, u32_at(r, 0), string(r, 4)),
            PageType::Artwork => insert(&mut lib.artwork, u32_at(r, 0), string(r, 4)),
            PageType::Keys => insert(&mut lib.keys, u32_at(r, 0), string(r, 8)),
            PageType::Colors => insert(&mut lib.colors, u16_at(r, 5).map(u32::from), string(r, 8)),
            PageType::Artists => {
                // Subtype 0x64 has a u16 name offset at 0x0A, else u8 at 9.
                let name = match u16_at(r, 0) {
                    Some(0x64) => u16_at(r, 0x0A).and_then(|o| string(r, o.into())),
                    _ => r.get(9).and_then(|&o| string(r, o.into())),
                };
                insert(&mut lib.artists, u32_at(r, 4), name);
            }
            PageType::Albums => {
                // Subtype 0x84 has a u16 name offset at 0x16, else u8 at 0x15.
                let name = match u16_at(r, 0) {
                    Some(0x84) => u16_at(r, 0x16).and_then(|o| string(r, o.into())),
                    _ => r.get(0x15).and_then(|&o| string(r, o.into())),
                };
                let album = u32_at(r, 8)
                    .zip(name)
                    .map(|(artist_id, name)| PdbAlbum { name, artist_id });
                insert(&mut lib.albums, u32_at(r, 12), album);
            }
            PageType::PlaylistTree => {
                let node = (|| {
                    Some(PdbPlaylist {
                        parent_id: u32_at(r, 0)?,
                        sort_order: u32_at(r, 8)?,
                        id: u32_at(r, 12)?,
                        is_folder: u32_at(r, 16)? != 0,
                        name: string(r, 20)?,
                        entries: Vec::new(),
                    })
                })();
                lib.playlists.push(node.ok_or_else(bad)?);
            }
            PageType::PlaylistEntries => {
                let e = (|| Some((u32_at(r, 8)?, u32_at(r, 0)?, u32_at(r, 4)?)))();
                entries.push(e.ok_or_else(bad)?);
            }
            _ => {}
        }
    }
    entries.sort_unstable();
    for (playlist, _, track) in entries {
        let p = lib
            .playlists
            .iter_mut()
            .find(|p| p.id == playlist)
            .ok_or_else(|| format!("entry for missing playlist {playlist}"))?;
        p.entries.push(track);
    }
    Ok(lib)
}

fn insert<T>(map: &mut BTreeMap<u32, T>, id: Option<u32>, value: Option<T>) {
    if let (Some(id), Some(value)) = (id, value) {
        map.insert(id, value);
    }
}

/// String `i` of the 21 in a track row's offset table at 0x5E.
fn track_string(r: &[u8], i: usize) -> Option<String> {
    string(r, u16_at(r, 0x5E + 2 * i)?.into())
}

fn track(r: &[u8]) -> Option<PdbTrack> {
    Some(PdbTrack {
        bitmask: u32_at(r, 0x04)?,
        sample_rate: u32_at(r, 0x08)?,
        composer_id: u32_at(r, 0x0C)?,
        file_size: u32_at(r, 0x10)?,
        unknown_0x14: u32_at(r, 0x14)?,
        master_db_id: u32_at(r, 0x18)?,
        artwork_id: u32_at(r, 0x1C)?,
        key_id: u32_at(r, 0x20)?,
        original_artist_id: u32_at(r, 0x24)?,
        label_id: u32_at(r, 0x28)?,
        remixer_id: u32_at(r, 0x2C)?,
        bitrate: u32_at(r, 0x30)?,
        track_number: u32_at(r, 0x34)?,
        tempo: u32_at(r, 0x38)?,
        genre_id: u32_at(r, 0x3C)?,
        album_id: u32_at(r, 0x40)?,
        artist_id: u32_at(r, 0x44)?,
        id: u32_at(r, 0x48)?,
        disc_number: u16_at(r, 0x4C)?,
        play_count: u16_at(r, 0x4E)?,
        year: u16_at(r, 0x50)?,
        sample_depth: u16_at(r, 0x52)?,
        duration: u16_at(r, 0x54)?,
        analysed_bits: u16_at(r, 0x56)?,
        color_id: *r.get(0x58)?,
        rating: *r.get(0x59)?,
        file_type: u16_at(r, 0x5A)?,
        isrc: track_string(r, 0)?,
        kuvo_public: track_string(r, 6)?,
        autoload_hotcues: track_string(r, 7)?,
        date_added: track_string(r, 10)?,
        release_date: track_string(r, 11)?,
        mix_name: track_string(r, 12)?,
        analyze_path: track_string(r, 14)?,
        comment: track_string(r, 16)?,
        title: track_string(r, 17)?,
        filename: track_string(r, 19)?,
        file_path: track_string(r, 20)?,
    })
}

fn string(raw: &[u8], at: usize) -> Option<String> {
    let tail = raw.get(at..)?;
    DeviceSQLString::read(&mut Cursor::new(tail))
        .ok()?
        .into_string()
        .ok()
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    b.get(at..at + 2)?.try_into().ok().map(u16::from_le_bytes)
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    b.get(at..at + 4)?.try_into().ok().map(u32::from_le_bytes)
}

#[cfg(test)]
mod tests;
