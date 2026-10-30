//! Row layouts (crate-digger `pdb.ksy`), decoded from a row's raw bytes
//! ([`super::Page::rows`]). Offsets are relative to the row start; a field
//! past the row or a string that does not decode is a [`RowError`].

use super::library::{PdbAlbum, PdbPlaylist, PdbTrack};
use super::string::{self, StringError};
use super::{u16_at, u32_at};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RowError {
    #[error("{0} row too short")]
    Short(&'static str),
    #[error("{field}: {source}")]
    String {
        field: &'static str,
        #[source]
        source: StringError,
    },
}

fn u16f(r: &[u8], at: usize, row: &'static str) -> Result<u16, RowError> {
    u16_at(r, at).ok_or(RowError::Short(row))
}

fn u32f(r: &[u8], at: usize, row: &'static str) -> Result<u32, RowError> {
    u32_at(r, at).ok_or(RowError::Short(row))
}

fn u8f(r: &[u8], at: usize, row: &'static str) -> Result<u8, RowError> {
    r.get(at).copied().ok_or(RowError::Short(row))
}

fn text(r: &[u8], at: usize, field: &'static str) -> Result<String, RowError> {
    string::decode(r, at).map_err(|source| RowError::String { field, source })
}

/// Names of the 21 strings of a track row, in offset-table order.
pub const TRACK_STRINGS: [&str; 21] = [
    "isrc",
    "texter",
    "unknown_string_2",
    "unknown_string_3",
    "unknown_string_4",
    "message",
    "kuvo_public",
    "autoload_hotcues",
    "unknown_string_5",
    "unknown_string_6",
    "date_added",
    "release_date",
    "mix_name",
    "unknown_string_7",
    "analyze_path",
    "analyze_date",
    "comment",
    "title",
    "unknown_string_8",
    "filename",
    "file_path",
];
pub const STR_ANALYZE_PATH: usize = 14;
pub const STR_TITLE: usize = 17;
pub const STR_FILE_PATH: usize = 20;
/// The track row's id field.
pub const TRACK_ID_AT: usize = 0x48;

/// String `i` of a track row: its offset is the u16 at `0x5E + 2i`.
pub fn track_string(r: &[u8], i: usize) -> Result<String, RowError> {
    let field = TRACK_STRINGS.get(i).copied().unwrap_or("track string");
    let at = u16f(r, 0x5E + 2 * i, "track")?;
    text(r, at.into(), field)
}

/// A track row, field for field.
pub fn track(r: &[u8]) -> Result<PdbTrack, RowError> {
    const T: &str = "track";
    Ok(PdbTrack {
        bitmask: u32f(r, 0x04, T)?,
        sample_rate: u32f(r, 0x08, T)?,
        composer_id: u32f(r, 0x0C, T)?,
        file_size: u32f(r, 0x10, T)?,
        unknown_0x14: u32f(r, 0x14, T)?,
        master_db_id: u32f(r, 0x18, T)?,
        artwork_id: u32f(r, 0x1C, T)?,
        key_id: u32f(r, 0x20, T)?,
        original_artist_id: u32f(r, 0x24, T)?,
        label_id: u32f(r, 0x28, T)?,
        remixer_id: u32f(r, 0x2C, T)?,
        bitrate: u32f(r, 0x30, T)?,
        track_number: u32f(r, 0x34, T)?,
        tempo: u32f(r, 0x38, T)?,
        genre_id: u32f(r, 0x3C, T)?,
        album_id: u32f(r, 0x40, T)?,
        artist_id: u32f(r, 0x44, T)?,
        id: u32f(r, TRACK_ID_AT, T)?,
        disc_number: u16f(r, 0x4C, T)?,
        play_count: u16f(r, 0x4E, T)?,
        year: u16f(r, 0x50, T)?,
        sample_depth: u16f(r, 0x52, T)?,
        duration: u16f(r, 0x54, T)?,
        analysed_bits: u16f(r, 0x56, T)?,
        color_id: u8f(r, 0x58, T)?,
        rating: u8f(r, 0x59, T)?,
        file_type: u16f(r, 0x5A, T)?,
        isrc: track_string(r, 0)?,
        kuvo_public: track_string(r, 6)?,
        autoload_hotcues: track_string(r, 7)?,
        date_added: track_string(r, 10)?,
        release_date: track_string(r, 11)?,
        mix_name: track_string(r, 12)?,
        analyze_path: track_string(r, STR_ANALYZE_PATH)?,
        comment: track_string(r, 16)?,
        title: track_string(r, STR_TITLE)?,
        filename: track_string(r, 19)?,
        file_path: track_string(r, STR_FILE_PATH)?,
    })
}

/// An artist row: `(id, name)`. Subtype `0x64` has a u16 name offset at
/// 0x0A, else a u8 at 9.
pub fn artist(r: &[u8]) -> Result<(u32, String), RowError> {
    const A: &str = "artist";
    let at = match u16f(r, 0, A)? {
        0x64 => usize::from(u16f(r, 0x0A, A)?),
        _ => usize::from(u8f(r, 9, A)?),
    };
    Ok((u32f(r, 4, A)?, text(r, at, "artist name")?))
}

/// An album row: `(id, album)`. Subtype `0x84` has a u16 name offset at
/// 0x16, else a u8 at 0x15.
pub fn album(r: &[u8]) -> Result<(u32, PdbAlbum), RowError> {
    const A: &str = "album";
    let at = match u16f(r, 0, A)? {
        0x84 => usize::from(u16f(r, 0x16, A)?),
        _ => usize::from(u8f(r, 0x15, A)?),
    };
    Ok((
        u32f(r, 12, A)?,
        PdbAlbum {
            artist_id: u32f(r, 8, A)?,
            name: text(r, at, "album name")?,
        },
    ))
}

/// A row that is a u32 id and a name at `name_at`: genres, labels and
/// artwork (4), keys (8).
pub fn named(r: &[u8], name_at: usize) -> Result<(u32, String), RowError> {
    Ok((u32f(r, 0, "name")?, text(r, name_at, "name")?))
}

/// A colour row: 5 bytes, a u16 id, a byte, the name.
pub fn color(r: &[u8]) -> Result<(u32, String), RowError> {
    Ok((u32::from(u16f(r, 5, "colour")?), text(r, 8, "colour name")?))
}

/// A playlist tree row (no entries).
pub fn playlist_node(r: &[u8]) -> Result<PdbPlaylist, RowError> {
    const P: &str = "playlist tree";
    Ok(PdbPlaylist {
        parent_id: u32f(r, 0, P)?,
        sort_order: u32f(r, 8, P)?,
        id: u32f(r, 12, P)?,
        is_folder: u32f(r, 16, P)? != 0,
        name: text(r, 20, "playlist name")?,
        entries: Vec::new(),
    })
}

/// A playlist entry row: `(playlist id, entry index, track id)`.
pub fn playlist_entry(r: &[u8]) -> Result<(u32, u32, u32), RowError> {
    const E: &str = "playlist entry";
    Ok((u32f(r, 8, E)?, u32f(r, 0, E)?, u32f(r, 4, E)?))
}

/// A history playlist row: `(id, name)`.
pub fn history_playlist(r: &[u8]) -> Result<(u32, String), RowError> {
    Ok((
        u32f(r, 0, "history playlist")?,
        text(r, 4, "history playlist name")?,
    ))
}

/// A history entry row: `(track id, playlist id, entry index)`.
pub fn history_entry(r: &[u8]) -> Result<(u32, u32, u32), RowError> {
    const H: &str = "history entry";
    Ok((u32f(r, 0, H)?, u32f(r, 4, H)?, u32f(r, 8, H)?))
}
