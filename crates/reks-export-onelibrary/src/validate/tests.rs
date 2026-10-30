use reks_export_rb::pdb::{PdbPlaylist, PdbTrack};

use super::*;
use crate::rows::{Content, Playlist, PlaylistEntry, Property};

fn pdb() -> PdbLibrary {
    PdbLibrary {
        tracks: vec![PdbTrack {
            id: 1,
            file_path: "/Contents/a.mp3".into(),
            analyze_path: "/PIONEER/USBANLZ/P001/00000001/ANLZ0000.DAT".into(),
            ..PdbTrack::default()
        }],
        playlists: vec![PdbPlaylist {
            id: 1,
            name: "L".into(),
            entries: vec![1],
            ..PdbPlaylist::default()
        }],
        ..PdbLibrary::default()
    }
}

fn lib() -> OneLibrary {
    crate::build::from_pdb(&pdb(), "2026-10-20")
}

#[test]
fn a_matching_database_has_no_findings() {
    assert_eq!(check(&lib(), &pdb(), |_| true), []);
}

#[test]
fn catches_anlz_path_mismatch_and_missing_file() {
    let mut l = lib();
    l.contents[0].analysis_data_file_path = "/PIONEER/USBANLZ/P002/00000002/ANLZ0000.DAT".into();
    let f = check(&l, &pdb(), |_| false);
    assert!(
        f.iter().any(|f| matches!(f, Finding::AnlzPath { .. })),
        "{f:?}"
    );
    assert!(
        f.iter().any(|f| matches!(f, Finding::AnlzMissing { .. })),
        "{f:?}"
    );
}

#[test]
fn catches_count_mismatches() {
    let mut l = lib();
    l.contents.push(Content {
        content_id: 2,
        path: "/Contents/b.mp3".into(),
        ..Content::default()
    });
    let f = check(&l, &pdb(), |_| true);
    assert!(
        f.contains(&Finding::ContentCount { pdb: 1, db: 2 }),
        "{f:?}"
    );
    assert!(f.contains(&Finding::PropertyCount {
        property: 1,
        contents: 2
    }));
    assert!(f.contains(&Finding::NotInPdb {
        path: "/Contents/b.mp3".into()
    }));

    let mut l = lib();
    l.property = Property {
        number_of_contents: 5,
        ..l.property
    };
    l.contents.clear();
    let f = check(&l, &pdb(), |_| true);
    assert!(f.contains(&Finding::NotInDb {
        path: "/Contents/a.mp3".into()
    }));
}

#[test]
fn catches_playlist_orphans() {
    let mut l = lib();
    l.playlists.push(Playlist {
        playlist_id: 9,
        parent_id: 42,
        name: "Lost".into(),
        ..Playlist::default()
    });
    l.playlist_entries.push(PlaylistEntry {
        playlist_id: 1,
        content_id: 77,
        sequence_no: 2,
    });
    let f = check(&l, &pdb(), |_| true);
    assert!(f.contains(&Finding::OrphanPlaylist {
        playlist_id: 9,
        parent_id: 42
    }));
    assert!(f.contains(&Finding::OrphanEntry {
        playlist_id: 1,
        content_id: 77
    }));
    assert!(f.contains(&Finding::PlaylistCount { pdb: 1, db: 2 }));
}

#[test]
fn a_missing_database_and_a_wrong_key_are_findings() {
    let stick = crate::testutil::reference_stick();
    assert_eq!(validate(stick.path(), "k").unwrap(), [Finding::DbMissing]);
    crate::writer::OneLibraryWriter::default()
        .write_db(crate::testutil::TEST_KEY, stick.path())
        .unwrap();
    assert_eq!(
        validate(stick.path(), "wrong").unwrap(),
        [Finding::NotDecrypted]
    );
}
