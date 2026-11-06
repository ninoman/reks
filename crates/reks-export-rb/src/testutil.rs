//! Synthetic rekordbox files for tests: a minimal `export.pdb` with history
//! tables and a minimal `ANLZ0000.DAT`. Shared by this crate's tests and,
//! through the `test-util` feature, by reks-cli's.

use baken_export::anlz::section::{section, AnlzFile};

pub const PAGE: usize = 4096;
pub const T_HISTORY_PLAYLISTS: u32 = 11;
pub const T_HISTORY_ENTRIES: u32 = 12;
pub const T_TRACKS: u32 = 0;
pub const T_ARTISTS: u32 = 2;
pub const T_PLAYLIST_TREE: u32 = 7;
pub const T_PLAYLIST_ENTRIES: u32 = 8;
/// Table 19 (`history`): layout undecoded (HIST1).
pub const T_HISTORY: u32 = 19;

/// A DeviceSQL short ASCII string: ((len + 1) << 1) | 1, then the bytes.
pub fn short_string(s: &str) -> Vec<u8> {
    let mut out = vec![(((s.len() + 1) << 1) | 1) as u8];
    out.extend_from_slice(s.as_bytes());
    out
}

/// A track row (crate-digger `track_row`): `id`, `artist_id`, `duration`
/// in seconds; strings empty except title (17), filename (19), file path (20).
pub fn track_row(
    id: u32,
    artist_id: u32,
    duration_s: u16,
    title: &str,
    file_path: &str,
) -> Vec<u8> {
    let mut row = vec![0u8; 0x5E + 2 * 21];
    put_u16(&mut row, 0x00, 0x24);
    put_u32(&mut row, 0x44, artist_id);
    put_u32(&mut row, 0x48, id);
    put_u16(&mut row, 0x54, duration_s);
    let filename = file_path.rsplit('/').next().unwrap_or_default();
    for i in 0..21 {
        let s = match i {
            17 => title,
            19 => filename,
            20 => file_path,
            _ => "",
        };
        let at = row.len() as u16;
        put_u16(&mut row, 0x5E + 2 * i, at);
        row.extend_from_slice(&short_string(s));
    }
    row
}

/// An artist row with a near name offset (subtype `0x60`).
pub fn artist_row(id: u32, name: &str) -> Vec<u8> {
    let mut row = vec![0u8; 10];
    put_u16(&mut row, 0, 0x60);
    put_u32(&mut row, 4, id);
    row[8] = 0x03;
    row[9] = 10;
    row.extend_from_slice(&short_string(name));
    row
}

/// A guessed table-19 row: a u32 id and an ASCII `YYYY-MM-DD` string. The
/// real layout is undecoded (HIST1); the reader only relies on the date text.
pub fn history_date_row(id: u32, date: &str) -> Vec<u8> {
    let mut row = id.to_le_bytes().to_vec();
    row.extend_from_slice(&[0, 0, 0, 0]);
    row.extend_from_slice(&short_string(date));
    row
}

pub fn history_playlist(id: u32, name: &str) -> Vec<u8> {
    let mut row = id.to_le_bytes().to_vec();
    row.extend_from_slice(&short_string(name));
    row
}

pub fn history_entry(track_id: u32, playlist_id: u32, index: u32) -> Vec<u8> {
    [track_id, playlist_id, index]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect()
}

/// A playlist tree row: parent (0 = root), unknown, sort order, id,
/// folder flag, name.
pub fn playlist_node(
    id: u32,
    parent_id: u32,
    sort_order: u32,
    is_folder: bool,
    name: &str,
) -> Vec<u8> {
    let mut row: Vec<u8> = [parent_id, 0, sort_order, id, u32::from(is_folder)]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    row.extend_from_slice(&short_string(name));
    row
}

/// A playlist entry row: entry index, track id, playlist id.
pub fn playlist_entry(index: u32, track_id: u32, playlist_id: u32) -> Vec<u8> {
    [index, track_id, playlist_id]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect()
}

pub fn put_u16(buf: &mut [u8], at: usize, v: u16) {
    buf[at..at + 2].copy_from_slice(&v.to_le_bytes());
}

pub fn put_u32(buf: &mut [u8], at: usize, v: u32) {
    buf[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

/// A minimal `export.pdb`: page 0 is the header, then each table's pages
/// in order. `tables` = (page type, pages, each page's rows (≤ 16)).
pub fn synthetic_pdb(tables: &[(u32, Vec<Vec<Vec<u8>>>)]) -> Vec<u8> {
    let total_pages = 1 + tables.iter().map(|t| t.1.len()).sum::<usize>();
    let mut buf = vec![0u8; total_pages * PAGE];
    put_u32(&mut buf, 4, PAGE as u32);
    put_u32(&mut buf, 8, tables.len() as u32);
    put_u32(&mut buf, 12, total_pages as u32); // next_unused_page
    put_u32(&mut buf, 20, 1); // sequence
    let mut next = 1usize;
    for (t, (page_type, pages)) in tables.iter().enumerate() {
        let at = 28 + 16 * t;
        put_u32(&mut buf, at, *page_type);
        put_u32(&mut buf, at + 8, next as u32);
        put_u32(&mut buf, at + 12, (next + pages.len() - 1) as u32);
        for (p, rows) in pages.iter().enumerate() {
            let index = next + p;
            let base = index * PAGE;
            let next_page = if p + 1 < pages.len() {
                index + 1
            } else {
                total_pages
            };
            put_u32(&mut buf, base + 4, index as u32);
            put_u32(&mut buf, base + 8, *page_type);
            put_u32(&mut buf, base + 12, next_page as u32);
            // 24-bit packed field: low 13 bits num_row_offsets, high 11 bits
            // num_rows (research raw-4 §3.2); every slot holds a row here.
            let n = rows.len() as u32;
            let packed = n | (n << 13);
            buf[base + 24..base + 27].copy_from_slice(&packed.to_le_bytes()[..3]);
            buf[base + 27] = 0x34; // page flags: has data
            put_u16(&mut buf, base + 38, 1); // unknown7: 1 on history pages
            let heap = base + 0x28;
            let mut used = 0usize;
            // Row groups of 16 from the page end: offsets, then the present flags.
            let mut present = vec![0u16; rows.len().div_ceil(16)];
            for (i, row) in rows.iter().enumerate() {
                buf[heap + used..heap + used + row.len()].copy_from_slice(row);
                let group_end = base + PAGE - (i / 16) * 36;
                put_u16(&mut buf, group_end - 4 - 2 * (i % 16 + 1), used as u16);
                present[i / 16] |= 1 << (i % 16);
                used += row.len().next_multiple_of(4);
            }
            put_u16(&mut buf, base + 30, used as u16); // used_size
            for (g, flags) in present.iter().enumerate() {
                put_u16(&mut buf, base + PAGE - g * 36 - 4, *flags);
            }
        }
        next += pages.len();
    }
    buf
}

/// Two sessions: `HISTORY 001` (2 entries, over two pages) and `HISTORY 002` (1).
pub fn played_pdb() -> Vec<u8> {
    synthetic_pdb(&[
        (T_TRACKS, vec![vec![]]),
        (
            T_HISTORY_PLAYLISTS,
            vec![vec![
                history_playlist(1, "HISTORY 001"),
                history_playlist(2, "HISTORY 002"),
            ]],
        ),
        (
            T_HISTORY_ENTRIES,
            vec![
                vec![history_entry(5, 1, 1), history_entry(7, 1, 2)],
                vec![history_entry(5, 2, 1)],
            ],
        ),
    ])
}

/// Tracks and history tables present, all empty.
pub fn unplayed_pdb() -> Vec<u8> {
    synthetic_pdb(&[
        (T_TRACKS, vec![vec![]]),
        (T_HISTORY_PLAYLISTS, vec![vec![]]),
        (T_HISTORY_ENTRIES, vec![vec![]]),
    ])
}

/// A minimal `.DAT`: `PPTH` = `ppth`, plus a constant `PQTZ` grid of
/// `beats` beats at `bpm_x100` from `first_ms` (none if `beats == 0`).
pub fn anlz_dat(ppth: &str, beats: u32, bpm_x100: u16, first_ms: u32) -> Vec<u8> {
    let mut file = AnlzFile {
        header_tail: [0, 0, 0, 1, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0],
        sections: Vec::new(),
    };
    if beats > 0 {
        let mut payload = Vec::new();
        payload.extend_from_slice(&0u32.to_be_bytes());
        payload.extend_from_slice(&0x0008_0000u32.to_be_bytes());
        payload.extend_from_slice(&beats.to_be_bytes());
        let step = 60_000.0 / (f64::from(bpm_x100) / 100.0);
        for i in 0..beats {
            payload.extend_from_slice(&((i % 4 + 1) as u16).to_be_bytes());
            payload.extend_from_slice(&bpm_x100.to_be_bytes());
            let t = f64::from(first_ms) + f64::from(i) * step;
            payload.extend_from_slice(&(t.round() as u32).to_be_bytes());
        }
        file.sections.push(section(b"PQTZ", 0x18, &payload));
    }
    file.set_path(ppth);
    file.to_bytes()
}
