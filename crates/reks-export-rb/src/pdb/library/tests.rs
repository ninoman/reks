use std::path::PathBuf;

use super::*;
use crate::testutil::*;

fn reference_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/onelibrary/ref-7.2.19")
}

#[test]
fn decodes_tracks_lookups_and_playlists() {
    let pdb = synthetic_pdb(&[
        (
            T_TRACKS,
            vec![vec![
                track_row(7, 3, 125, "One", "/Contents/A/B/one.mp3"),
                track_row(8, 0, 90, "Two", "/Contents/A/B/two.flac"),
            ]],
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
            vec![vec![playlist_entry(2, 7, 2), playlist_entry(1, 8, 2)]],
        ),
    ]);
    let lib = read_pdb(&pdb).unwrap();
    assert_eq!(lib.tracks.len(), 2);
    let one = &lib.tracks[0];
    assert_eq!(
        (one.id, one.artist_id, one.duration, one.title.as_str()),
        (7, 3, 125, "One")
    );
    assert_eq!(one.filename, "one.mp3");
    assert_eq!(one.file_path, "/Contents/A/B/one.mp3");
    assert_eq!(lib.artists.get(&3).map(String::as_str), Some("Artist"));
    assert_eq!(lib.playlists.len(), 2);
    assert!(lib.playlists[0].is_folder);
    assert_eq!(lib.playlists[1].parent_id, 1);
    // Entries in entry-index order, not row order.
    assert_eq!(lib.playlists[1].entries, [8, 7]);
}

#[test]
fn an_entry_for_a_missing_playlist_is_an_error() {
    let pdb = synthetic_pdb(&[(T_PLAYLIST_ENTRIES, vec![vec![playlist_entry(1, 1, 9)]])]);
    assert!(read_pdb(&pdb).unwrap_err().contains("missing playlist 9"));
}

#[test]
fn missing_pdb_is_reported() {
    let dir = tempfile::tempdir().unwrap();
    assert!(matches!(
        read_stick(dir.path()),
        Err(PdbReadError::Missing { .. })
    ));
}

/// rekordbox 7.2.19's export of the fixture tracks (Phase 4 task 1).
#[test]
fn reads_the_rekordbox_reference() {
    let lib = read_stick(&reference_root()).unwrap();
    assert_eq!(lib.tracks.len(), 7);
    let wav = lib
        .tracks
        .iter()
        .find(|t| t.filename == "sine-click-128.wav")
        .unwrap();
    assert_eq!(
        wav.file_path,
        "/Contents/UnknownArtist/UnknownAlbum/sine-click-128.wav"
    );
    assert_eq!(
        wav.analyze_path,
        "/PIONEER/USBANLZ/P027/0000B55F/ANLZ0000.DAT"
    );
    assert_eq!(
        (wav.tempo, wav.file_type, wav.file_size),
        (12800, 11, 2_646_044)
    );
    // The values exportLibrary.db's content row repeats.
    assert_eq!(wav.bitmask, 788_224);
    assert_eq!(wav.analysed_bits, 41);
    assert_eq!(lib.keys.get(&wav.key_id).map(String::as_str), Some("Am"));
    assert_eq!(lib.playlists.len(), 1);
    assert_eq!(lib.playlists[0].name, "reks-test");
    assert_eq!(lib.playlists[0].entries.len(), 7);
}

/// rekordcrate, the independent parser, sees the same rows per table and
/// the same playlist tree in rekordbox's own export as [`super::super::Pdb`].
#[test]
fn agrees_with_rekordcrate_on_the_rekordbox_reference() {
    use std::io::{Cursor, Seek, SeekFrom};

    use binrw::{BinRead, Endian};
    use rekordcrate::pdb::{Header, Page, Row};

    let bytes = std::fs::read(reference_root().join("PIONEER/rekordbox/export.pdb")).unwrap();
    let ours = super::super::Pdb::parse(&bytes).unwrap();
    let mut reader = Cursor::new(&bytes[..]);
    let header = Header::read(&mut reader).unwrap();
    assert_eq!(ours.tables().len(), header.tables.len());
    let mut names = Vec::new();
    for (table, theirs) in ours.tables().iter().zip(&header.tables) {
        assert_eq!(
            u64::from(table.first_page) * u64::from(header.page_size),
            theirs.first_page.offset(header.page_size)
        );
        let mut count = 0;
        let mut index = theirs.first_page.clone();
        loop {
            reader
                .seek(SeekFrom::Start(index.offset(header.page_size)))
                .unwrap();
            let page =
                Page::read_options(&mut reader, Endian::Little, (header.page_size,)).unwrap();
            for row in page.row_groups.iter().flat_map(|g| g.present_rows()) {
                count += 1;
                if let Row::PlaylistTreeNode(n) = row {
                    names.push((n.id.0, n.name.clone().into_string().unwrap()));
                }
            }
            if page.page_index == theirs.last_page {
                break;
            }
            index = page.next_page;
        }
        assert_eq!(ours.rows(table).unwrap().len(), count, "{:?}", table.table);
    }
    let lib = read_pdb(&bytes).unwrap();
    let mut mine: Vec<(u32, String)> = lib
        .playlists
        .iter()
        .map(|p| (p.id, p.name.clone()))
        .collect();
    mine.sort();
    names.sort();
    assert_eq!(mine, names);
}
