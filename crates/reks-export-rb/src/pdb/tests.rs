//! The reader on synthetic and damaged files: header and page-chain damage
//! are errors, a bad row is an error or a finding by mode, and no input
//! makes any stick reader panic (review F03).

use std::fs;
use std::path::Path;

use proptest::prelude::*;

use super::*;
use crate::history::{self, HistoryReadOptions, HistorySource};
use crate::testutil::*;

/// A track, an artist, a two-node playlist tree with two entries and a
/// history session.
fn sample() -> Vec<u8> {
    synthetic_pdb(&[
        (
            T_TRACKS,
            vec![vec![track_row(7, 3, 125, "One", "/Contents/A/one.mp3")]],
        ),
        (T_ARTISTS, vec![vec![artist_row(3, "Artist")]]),
        (
            T_PLAYLIST_TREE,
            vec![vec![
                playlist_node(1, 0, 0, true, "Folder"),
                playlist_node(2, 1, 0, false, "List"),
            ]],
        ),
        (
            T_PLAYLIST_ENTRIES,
            vec![vec![playlist_entry(1, 7, 2), playlist_entry(2, 7, 2)]],
        ),
        (
            T_HISTORY_PLAYLISTS,
            vec![vec![history_playlist(1, "HISTORY 001")]],
        ),
        (T_HISTORY_ENTRIES, vec![vec![history_entry(7, 1, 1)]]),
    ])
}

/// `sample` with the first byte of the track title replaced by `header`.
fn with_title_header(header: u8) -> Vec<u8> {
    let mut row = track_row(7, 3, 125, "One", "/Contents/A/one.mp3");
    let at = usize::from(u16::from_le_bytes([row[0x5E + 34], row[0x5E + 35]]));
    row[at] = header;
    synthetic_pdb(&[(T_TRACKS, vec![vec![row]])])
}

#[test]
fn reads_tables_pages_and_rows() {
    let bytes = sample();
    let pdb = Pdb::parse(&bytes).unwrap();
    assert_eq!(pdb.page_size(), PAGE);
    let codes: Vec<u32> = pdb.tables().iter().map(|t| t.table.code()).collect();
    assert_eq!(codes, [0, 2, 7, 8, 11, 12]);
    assert_eq!(pdb.rows_of(Table::PlaylistTree).unwrap().len(), 2);
    let track = rows::track(pdb.rows_of(Table::Tracks).unwrap()[0]).unwrap();
    assert_eq!((track.id, track.title.as_str()), (7, "One"));
    assert_eq!(
        rows::history_entry(pdb.rows_of(Table::HistoryEntries).unwrap()[0]).unwrap(),
        (7, 1, 1)
    );
    for code in 0..32 {
        assert_eq!(Table::from_code(code).code(), code);
    }
}

#[test]
fn header_damage_is_an_error() {
    let good = sample();
    assert!(Pdb::parse(&[]).is_err());
    assert!(Pdb::parse(&good[..6]).is_err());
    let mut b = good.clone();
    put_u32(&mut b, 0, 1);
    assert!(Pdb::parse(&b).is_err());
    for size in [0, 511, 65_537, u32::MAX] {
        let mut b = good.clone();
        put_u32(&mut b, 4, size);
        assert!(Pdb::parse(&b).is_err(), "page size {size}");
    }
    let mut b = good.clone();
    put_u32(&mut b, 8, u32::MAX);
    assert!(Pdb::parse(&b).is_err());
}

#[test]
fn page_chain_damage_is_an_error() {
    let at = |t: usize| 28 + 16 * t;
    // First page outside the file, and the file header as a page.
    for first in [u32::MAX, 0, 1_000] {
        let mut b = sample();
        put_u32(&mut b, at(0) + 8, first);
        let pdb = Pdb::parse(&b).unwrap();
        assert!(pdb.rows_of(Table::Tracks).is_err(), "first page {first}");
    }
    // A chain that loops: the last page is never reached.
    let mut b = sample();
    put_u32(&mut b, at(0) + 12, 99);
    put_u32(&mut b, PAGE + 12, 1);
    let e = Pdb::parse(&b).unwrap().rows_of(Table::Tracks).unwrap_err();
    assert!(e.to_string().contains("loops"), "{e}");
    // A page whose header names another table.
    let mut b = sample();
    put_u32(&mut b, PAGE + 8, 5);
    let e = Pdb::parse(&b).unwrap().rows_of(Table::Tracks).unwrap_err();
    assert!(e.to_string().contains("wrong index or type"), "{e}");
    // The other tables still read.
    assert_eq!(
        Pdb::parse(&b)
            .unwrap()
            .rows_of(Table::Artists)
            .unwrap()
            .len(),
        1
    );
}

/// A page with more than 255 rows: the row count is the low 13 bits of the
/// 24-bit field at 0x18, not its first byte nor the old `num_rows_large`
/// at 0x22 (which baken leaves one short). Reading 255 lost the last
/// entries and showed a false read-back problem after an export.
#[test]
fn pages_with_more_than_255_rows_read_every_row() {
    let entries: Vec<Vec<u8>> = (1..=284).map(|i| playlist_entry(i, 7, 2)).collect();
    let mut b = synthetic_pdb(&[
        (
            T_TRACKS,
            vec![vec![track_row(7, 3, 125, "One", "/Contents/A/one.mp3")]],
        ),
        (
            T_PLAYLIST_TREE,
            vec![vec![playlist_node(2, 0, 0, false, "List")]],
        ),
        (T_PLAYLIST_ENTRIES, vec![entries]),
    ]);
    let entries_page = 3 * PAGE;
    put_u16(&mut b, entries_page + 0x22, 283);
    let pdb = Pdb::parse(&b).unwrap();
    assert_eq!(pdb.rows_of(Table::PlaylistEntries).unwrap().len(), 284);
    let lib = read_pdb(&b).unwrap();
    assert_eq!(lib.playlists[0].entries.len(), 284);
}

#[test]
fn row_slots_and_used_size_past_the_page_are_skipped() {
    let mut b = sample();
    // 0x1FFF row slots on one page, used size past the heap.
    let packed: u32 = 0x1FFF | (0x7FF << 13);
    b[PAGE + 0x18..PAGE + 0x1B].copy_from_slice(&packed.to_le_bytes()[..3]);
    put_u16(&mut b, PAGE + 0x1E, 0xFFFF);
    let pdb = Pdb::parse(&b).unwrap();
    let rows = pdb.rows_of(Table::Tracks).unwrap();
    assert!(rows.iter().all(|r| r.len() <= PAGE));
}

/// rekordcrate 0.3.0 underflows on a short-string header of `0x01`
/// (`string.rs:164`); every reader must turn it into an error or a finding.
#[test]
fn string_header_0x01_is_damage_in_every_reader() {
    let bytes = with_title_header(0x01);
    assert!(library::read_pdb(&bytes).unwrap_err().contains("title"));
    let (lib, damage) = library::read_pdb_lenient(&bytes).unwrap();
    assert!(lib.tracks.is_empty());
    assert_eq!(damage.len(), 1, "{damage:?}");

    let mut hist = history_playlist(1, "HISTORY 001");
    hist[4] = 0x01;
    let played = synthetic_pdb(&[
        (T_TRACKS, vec![vec![]]),
        (T_HISTORY_PLAYLISTS, vec![vec![hist]]),
        (T_HISTORY_ENTRIES, vec![vec![history_entry(5, 1, 1)]]),
    ]);
    let stick = stick_with(&played);
    assert!(history::read_sessions(stick.path()).is_err());
    let h = history::read_history(stick.path(), None);
    assert_eq!((h.pdb_sessions, h.pdb_entries), (1, 1));
    assert!(read_play(stick.path()).is_err());

    let stick = stick_with(&bytes);
    assert!(history::read_stick_tracks(stick.path()).unwrap().is_empty());
    let v = crate::validate::read(stick.path()).unwrap();
    assert!(v
        .findings
        .iter()
        .any(|f| matches!(f, crate::validate::Finding::TrackRowUnreadable { .. })));
}

fn stick_with(pdb: &[u8]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let rb = dir.path().join("PIONEER/rekordbox");
    fs::create_dir_all(&rb).unwrap();
    fs::write(rb.join("export.pdb"), pdb).unwrap();
    dir
}

fn read_play(root: &Path) -> Result<history::PlayHistory, history::HistoryError> {
    history::read_play_history(
        &HistorySource::Stick(root.into()),
        &HistoryReadOptions::default(),
    )
}

/// Every reader of stick bytes, on `bytes`: none may panic.
fn read_everything(bytes: &[u8]) {
    let _ = library::read_pdb(bytes);
    let _ = library::read_pdb_lenient(bytes);
    if let Ok(pdb) = Pdb::parse(bytes) {
        for t in pdb.tables() {
            for r in pdb.rows(t).unwrap_or_default() {
                let _ = rows::track(r);
                let _ = rows::artist(r);
                let _ = rows::album(r);
                let _ = rows::named(r, 4);
                let _ = rows::color(r);
                let _ = rows::playlist_node(r);
                let _ = rows::playlist_entry(r);
                let _ = rows::history_playlist(r);
                let _ = rows::history_entry(r);
            }
        }
    }
}

/// The file-based readers too (slower: one temp stick per case).
fn read_stick(bytes: &[u8]) {
    let stick = stick_with(bytes);
    let root = stick.path();
    let _ = history::read_history(root, None);
    let _ = history::read_sessions(root);
    let _ = read_play(root);
    let _ = history::read_stick_tracks(root);
    let _ = crate::validate::read(root);
    let _ = crate::diff::diff(root, root);
    let _ = crate::browse::read_library(root);
}

proptest! {
    #[test]
    fn mutated_files_never_panic(
        edits in proptest::collection::vec((0usize..PAGE * 7, any::<u8>()), 1..40)
    ) {
        let mut bytes = sample();
        let len = bytes.len();
        for (at, v) in edits {
            bytes[at % len] = v;
        }
        read_everything(&bytes);
    }

    /// Mutations aimed at the parts readers trust: page headers, row
    /// indexes at the page ends and string headers in the heap.
    #[test]
    fn mutated_structure_never_panics(
        page in 1usize..7,
        edits in proptest::collection::vec((0usize..0x60, any::<u8>()), 1..12),
        tail in proptest::collection::vec((1usize..0x48, any::<u8>()), 0..12),
    ) {
        let mut bytes = sample();
        let base = page * PAGE;
        for (at, v) in edits {
            bytes[base + at] = v;
        }
        for (at, v) in tail {
            bytes[base + PAGE - at] = v;
        }
        read_everything(&bytes);
    }

    #[test]
    fn arbitrary_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..2048)) {
        read_everything(&bytes);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn mutated_sticks_never_panic(
        edits in proptest::collection::vec((0usize..PAGE * 7, any::<u8>()), 1..24),
        header in any::<u8>(),
    ) {
        let mut bytes = with_title_header(header);
        let len = bytes.len();
        for (at, v) in edits {
            bytes[at % len] = v;
        }
        read_stick(&bytes);
    }
}
