//! reks's own `export.pdb` reader (review F03, F18): the file header, each
//! table's page chain, the present rows of a page and the row layouts reks
//! uses, decoded at crate-digger `pdb.ksy` offsets. Every reader of stick
//! bytes goes through it: [`library`] (the typed read the OneLibrary writer,
//! Devices browse and check-a-stick use), the history readers, the export
//! validator and the dev diff tool.
//!
//! The bytes are untrusted (a stick from any player or tool, possibly
//! damaged), and release builds abort on panic, so nothing here panics or
//! allocates from a length it read: every offset is bounds-checked, string
//! lengths are checked against the row, and page chains are bounded by the
//! file size with a cycle check. Damage is an error value. rekordcrate 0.3.0
//! is not used on stick bytes: its `DeviceSQLString` parser underflows on a
//! short-string header of `0x01` (`string.rs:164`), and a `Page` parse
//! decodes every row eagerly. It stays the independent parser in tests.
//!
//! Strict or lenient is the caller's choice per row: [`Pdb::rows`] fails only
//! when a table's page chain is broken, and each row decoder in [`rows`]
//! returns a [`RowError`] that a strict reader turns into a failure and a
//! lenient one into a finding ([`library::Mode`]).

pub mod library;
pub mod rows;
pub mod string;

use std::collections::HashSet;

pub use library::{
    read_pdb, read_pdb_lenient, read_stick, Mode, PdbAlbum, PdbLibrary, PdbPlaylist, PdbReadError,
    PdbTrack,
};
pub use rows::RowError;

/// A table's page type (crate-digger `page_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Table {
    Tracks,
    Genres,
    Artists,
    Albums,
    Labels,
    Keys,
    Colors,
    PlaylistTree,
    PlaylistEntries,
    HistoryPlaylists,
    HistoryEntries,
    Artwork,
    Columns,
    History,
    Unknown(u32),
}

impl Table {
    pub fn from_code(code: u32) -> Self {
        match code {
            0 => Self::Tracks,
            1 => Self::Genres,
            2 => Self::Artists,
            3 => Self::Albums,
            4 => Self::Labels,
            5 => Self::Keys,
            6 => Self::Colors,
            7 => Self::PlaylistTree,
            8 => Self::PlaylistEntries,
            11 => Self::HistoryPlaylists,
            12 => Self::HistoryEntries,
            13 => Self::Artwork,
            16 => Self::Columns,
            19 => Self::History,
            n => Self::Unknown(n),
        }
    }

    pub fn code(self) -> u32 {
        match self {
            Self::Tracks => 0,
            Self::Genres => 1,
            Self::Artists => 2,
            Self::Albums => 3,
            Self::Labels => 4,
            Self::Keys => 5,
            Self::Colors => 6,
            Self::PlaylistTree => 7,
            Self::PlaylistEntries => 8,
            Self::HistoryPlaylists => 11,
            Self::HistoryEntries => 12,
            Self::Artwork => 13,
            Self::Columns => 16,
            Self::History => 19,
            Self::Unknown(n) => n,
        }
    }
}

/// Damage that keeps a file or a table from being read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PdbError {
    #[error("PDB header: {0}")]
    Header(String),
    #[error("{table:?} page at byte {offset} {problem}")]
    Page {
        table: Table,
        offset: u64,
        problem: &'static str,
    },
}

/// A table in the file header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableRef {
    pub table: Table,
    pub first_page: u32,
    pub last_page: u32,
}

/// The file header page (`page 0`).
const HEADER_TABLES_AT: usize = 0x1C;
const TABLE_REF_LEN: usize = 16;
/// Page header size; the row heap follows.
pub(crate) const HEAP: usize = 0x28;
/// A row group at the end of a page: 16 u16 row offsets, a u16 presence
/// mask, a u16 (unknown).
pub(crate) const GROUP: usize = 0x24;

/// An `export.pdb` whose header parsed. Pages are read on demand.
#[derive(Debug, Clone)]
pub struct Pdb<'a> {
    bytes: &'a [u8],
    page_size: usize,
    tables: Vec<TableRef>,
}

impl<'a> Pdb<'a> {
    /// Reads the header: page size (512..=65 536), table list (bounded by
    /// the header page).
    pub fn parse(bytes: &'a [u8]) -> Result<Self, PdbError> {
        let bad = |m: String| PdbError::Header(m);
        if u32_at(bytes, 0) != Some(0) {
            return Err(bad("not an export.pdb (first word is not 0)".into()));
        }
        let page_size = u32_at(bytes, 4).ok_or_else(|| bad("file too short".into()))?;
        if !(512..=65_536).contains(&page_size) {
            return Err(bad(format!("implausible page size {page_size}")));
        }
        let page_size = page_size as usize;
        let num_tables = u32_at(bytes, 8).ok_or_else(|| bad("file too short".into()))? as usize;
        let room = page_size.saturating_sub(HEADER_TABLES_AT) / TABLE_REF_LEN;
        if num_tables > room {
            return Err(bad(format!(
                "{num_tables} tables do not fit in a {page_size}-byte header"
            )));
        }
        let mut tables = Vec::with_capacity(num_tables);
        for i in 0..num_tables {
            let at = HEADER_TABLES_AT + i * TABLE_REF_LEN;
            let field =
                |o: usize| u32_at(bytes, at + o).ok_or_else(|| bad("file too short".into()));
            tables.push(TableRef {
                table: Table::from_code(field(0)?),
                first_page: field(8)?,
                last_page: field(12)?,
            });
        }
        Ok(Self {
            bytes,
            page_size,
            tables,
        })
    }

    pub fn page_size(&self) -> usize {
        self.page_size
    }

    /// The tables in header order.
    pub fn tables(&self) -> &[TableRef] {
        &self.tables
    }

    /// Every page of `table` in chain order, from the first page to the last.
    /// Stops at a page outside the file, a loop, or a page whose header names
    /// another index or table.
    pub fn pages(&self, table: &TableRef) -> Result<Vec<Page<'a>>, PdbError> {
        let page_size = self.page_size as u64;
        let len = self.bytes.len() as u64;
        let mut seen = HashSet::new();
        let mut index = table.first_page;
        let mut pages = Vec::new();
        loop {
            let offset = u64::from(index) * page_size;
            let damage = |problem| PdbError::Page {
                table: table.table,
                offset,
                problem,
            };
            // Page 0 is the file header; a table page past the end is damage.
            if offset < page_size || offset.saturating_add(page_size) > len {
                return Err(damage("is outside the file"));
            }
            if !seen.insert(offset) {
                return Err(damage("loops back in the page chain"));
            }
            // In bounds: checked against `len` above.
            let start = offset as usize;
            let raw = self
                .bytes
                .get(start..start + self.page_size)
                .ok_or_else(|| damage("is outside the file"))?;
            let page = Page::parse(raw).ok_or_else(|| damage("is too short"))?;
            if page.index != index || Table::from_code(page.page_type) != table.table {
                return Err(damage("has the wrong index or type"));
            }
            let next = page.next_page;
            pages.push(page);
            if index == table.last_page {
                return Ok(pages);
            }
            index = next;
        }
    }

    /// The present rows of every data page of `table`, in chain and row
    /// order, as raw bytes ([`Page::rows`]).
    pub fn rows(&self, table: &TableRef) -> Result<Vec<&'a [u8]>, PdbError> {
        Ok(self
            .pages(table)?
            .iter()
            .filter(|p| p.has_data())
            .flat_map(Page::rows)
            .collect())
    }

    /// [`Self::rows`] of every table of type `table`, in header order.
    pub fn rows_of(&self, table: Table) -> Result<Vec<&'a [u8]>, PdbError> {
        let mut out = Vec::new();
        for t in self.tables.iter().filter(|t| t.table == table) {
            out.extend(self.rows(t)?);
        }
        Ok(out)
    }
}

/// One table page.
#[derive(Debug, Clone, Copy)]
pub struct Page<'a> {
    raw: &'a [u8],
    pub index: u32,
    pub page_type: u32,
    pub next_page: u32,
    flags: u8,
    num_rows: u16,
}

impl<'a> Page<'a> {
    fn parse(raw: &'a [u8]) -> Option<Self> {
        if raw.len() < HEAP {
            return None;
        }
        // 0x18..0x1B is one 24-bit field: low 13 bits `num_row_offsets`
        // (row slots ever allocated, each with a present bit), high 11 bits
        // `num_rows` (research raw-4 §3.2). Rows are read by slot, so the
        // slot count is what matters; a page with more than 255 rows lost
        // its last ones when only the low byte was read.
        let packed = u32::from(*raw.get(0x18)?)
            | u32::from(*raw.get(0x19)?) << 8
            | u32::from(*raw.get(0x1A)?) << 16;
        Some(Self {
            raw,
            index: u32_at(raw, 0x04)?,
            page_type: u32_at(raw, 0x08)?,
            next_page: u32_at(raw, 0x0C)?,
            flags: *raw.get(0x1B)?,
            num_rows: (packed & 0x1FFF) as u16,
        })
    }

    /// Bit 0x40 marks a page without row data.
    pub fn has_data(&self) -> bool {
        self.flags & 0x40 == 0
    }

    pub fn num_rows(&self) -> u16 {
        self.num_rows
    }

    /// The page's raw bytes.
    pub fn raw(&self) -> &'a [u8] {
        self.raw
    }

    /// The present rows, as raw bytes in row order. A row's extent is up to
    /// the next row's start, or to the end of the used heap (rows carry no
    /// length). Row slots past the page are skipped.
    pub fn rows(&self) -> Vec<&'a [u8]> {
        let raw = self.raw;
        let num_rows = usize::from(self.num_rows);
        let groups = num_rows.div_ceil(16);
        let used = u16_at(raw, 0x1E).map_or(0, usize::from);
        let heap_end = (HEAP + used).min(raw.len().saturating_sub(groups * GROUP));
        let mut offsets = Vec::new();
        for i in 0..num_rows {
            let Some(group_end) = raw.len().checked_sub((i / 16) * GROUP) else {
                break;
            };
            let present = group_end
                .checked_sub(4)
                .and_then(|at| u16_at(raw, at))
                .unwrap_or(0);
            if present & (1 << (i % 16)) == 0 {
                continue;
            }
            let ofs = group_end
                .checked_sub(4 + 2 * (i % 16 + 1))
                .and_then(|at| u16_at(raw, at));
            if let Some(ofs) = ofs {
                offsets.push(HEAP + usize::from(ofs));
            }
        }
        let mut starts = offsets.clone();
        starts.sort_unstable();
        starts.dedup();
        offsets
            .iter()
            .map(|&start| {
                let end = starts
                    .iter()
                    .copied()
                    .find(|&s| s > start)
                    .unwrap_or(heap_end)
                    .min(heap_end);
                raw.get(start..end).unwrap_or_default()
            })
            .collect()
    }
}

pub(crate) fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    b.get(at..at.checked_add(2)?)?
        .try_into()
        .ok()
        .map(u16::from_le_bytes)
}

pub(crate) fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    b.get(at..at.checked_add(4)?)?
        .try_into()
        .ok()
        .map(u32::from_le_bytes)
}

#[cfg(test)]
mod tests;
