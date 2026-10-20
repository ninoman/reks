//! `export.pdb` → OneLibrary rows (tasks 4 and 5). Pure.
//!
//! rekordbox fills both libraries from one model: on its 7.2.19 reference
//! every `content` value is the `export.pdb` track row's (ids included,
//! `masterDbId` at 0x18, `masterContentId` at 0x14, `contentLink` the row
//! bitmask, `analysedBits` at 0x56), lookups keep the PDB's ids, and the
//! playlist tree is the PDB's. So the writer mirrors the `export.pdb` baken
//! just wrote, and the two libraries describe the same tracks by
//! construction.

use reks_export_rb::pdbread::{PdbLibrary, PdbTrack};

use crate::rows::{
    Album, Artist, Content, Image, Named, OneLibrary, Playlist, PlaylistEntry, Property,
    ATTRIBUTE_FOLDER, ATTRIBUTE_PLAYLIST,
};

/// `property.dbVersion` on the reference (a string; "10000" is reported
/// elsewhere and is not what 7.2.19 writes).
pub const DB_VERSION: &str = "1000";

/// The rows for `pdb`. `created_date` (`YYYY-MM-DD`) is the export date.
pub fn from_pdb(pdb: &PdbLibrary, created_date: &str) -> OneLibrary {
    OneLibrary {
        contents: pdb.tracks.iter().map(content).collect(),
        artists: pdb
            .artists
            .iter()
            .map(|(&id, name)| Artist {
                artist_id: id.into(),
                name: name.clone(),
            })
            .collect(),
        albums: pdb
            .albums
            .iter()
            .map(|(&id, a)| Album {
                album_id: id.into(),
                name: a.name.clone(),
                artist_id: some(a.artist_id),
            })
            .collect(),
        genres: named(&pdb.genres),
        labels: named(&pdb.labels),
        keys: named(&pdb.keys),
        colors: named(&pdb.colors),
        images: pdb
            .artwork
            .iter()
            .map(|(&id, path)| Image {
                image_id: id.into(),
                path: path.clone(),
            })
            .collect(),
        playlists: pdb
            .playlists
            .iter()
            .map(|p| Playlist {
                playlist_id: p.id.into(),
                sequence_no: p.sort_order.into(),
                name: p.name.clone(),
                attribute: if p.is_folder {
                    ATTRIBUTE_FOLDER
                } else {
                    ATTRIBUTE_PLAYLIST
                },
                parent_id: p.parent_id.into(),
            })
            .collect(),
        playlist_entries: pdb
            .playlists
            .iter()
            .flat_map(|p| {
                p.entries.iter().zip(1..).map(|(&track, n)| PlaylistEntry {
                    playlist_id: p.id.into(),
                    content_id: track.into(),
                    sequence_no: n,
                })
            })
            .collect(),
        property: Property {
            // rekordbox leaves it empty on the reference.
            device_name: String::new(),
            db_version: DB_VERSION.to_owned(),
            number_of_contents: i64::try_from(pdb.tracks.len()).unwrap_or(i64::MAX),
            created_date: created_date.to_owned(),
            back_ground_color_type: 0,
            // rekordbox's My Tag master id; reks exports no My Tags. UNVERIFIED.
            my_tag_master_dbid: 0,
        },
    }
}

fn content(t: &PdbTrack) -> Content {
    Content {
        content_id: t.id.into(),
        title: t.title.clone(),
        title_for_search: None,
        subtitle: t.mix_name.clone(),
        bpmx100: t.tempo.into(),
        length: t.duration.into(),
        track_no: t.track_number.into(),
        disc_no: t.disc_number.into(),
        artist_id_artist: some(t.artist_id),
        artist_id_remixer: some(t.remixer_id),
        artist_id_original_artist: some(t.original_artist_id),
        artist_id_composer: some(t.composer_id),
        artist_id_lyricist: 0,
        album_id: some(t.album_id),
        genre_id: some(t.genre_id),
        label_id: some(t.label_id),
        key_id: some(t.key_id),
        color_id: t.color_id.into(),
        image_id: some(t.artwork_id),
        dj_comment: t.comment.clone(),
        rating: t.rating.into(),
        release_year: t.year.into(),
        release_date: t.release_date.clone(),
        // The PDB has no creation date; equal to dateAdded on the reference.
        date_created: t.date_added.clone(),
        date_added: t.date_added.clone(),
        path: t.file_path.clone(),
        file_name: t.filename.clone(),
        file_size: t.file_size.into(),
        file_type: t.file_type.into(),
        bitrate: t.bitrate.into(),
        bit_depth: t.sample_depth.into(),
        sampling_rate: t.sample_rate.into(),
        isrc: t.isrc.clone(),
        dj_play_count: t.play_count.into(),
        is_hot_cue_auto_load_on: on(&t.autoload_hotcues),
        is_kuvo_deliver_status_on: on(&t.kuvo_public),
        kuvo_delivery_comment: String::new(),
        master_db_id: t.master_db_id.into(),
        master_content_id: t.unknown_0x14.into(),
        analysis_data_file_path: t.analyze_path.clone(),
        analysed_bits: t.analysed_bits.into(),
        content_link: t.bitmask.into(),
        has_modified: 0,
        cue_update_count: None,
        analysis_data_update_count: None,
        information_update_count: None,
    }
}

/// 0 is "none" in `export.pdb`; rekordbox writes NULL for it here.
fn some(id: u32) -> Option<i64> {
    (id != 0).then_some(id.into())
}

fn on(flag: &str) -> i64 {
    i64::from(flag == "ON")
}

fn named(map: &std::collections::BTreeMap<u32, String>) -> Vec<Named> {
    map.iter()
        .map(|(&id, name)| Named {
            id: id.into(),
            name: name.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use reks_export_rb::pdbread::{PdbAlbum, PdbPlaylist};

    use super::*;

    fn track(id: u32) -> PdbTrack {
        PdbTrack {
            id,
            title: format!("T{id}"),
            file_path: format!("/Contents/A/B/t{id}.mp3"),
            filename: format!("t{id}.mp3"),
            analyze_path: "/PIONEER/USBANLZ/P000/00000001/ANLZ0000.DAT".into(),
            date_added: "2026-10-20".into(),
            autoload_hotcues: "ON".into(),
            kuvo_public: String::new(),
            tempo: 12_800,
            artist_id: 3,
            unknown_0x14: 99,
            master_db_id: 7,
            bitmask: 788_224,
            analysed_bits: 41,
            ..PdbTrack::default()
        }
    }

    #[test]
    fn content_fields_mirror_the_track_row() {
        let pdb = PdbLibrary {
            tracks: vec![track(5)],
            ..PdbLibrary::default()
        };
        let c = &from_pdb(&pdb, "2026-10-21").contents[0];
        assert_eq!(c.content_id, 5);
        assert_eq!(c.title, "T5");
        assert_eq!(c.bpmx100, 12_800);
        assert_eq!(c.artist_id_artist, Some(3));
        // Unset ids are NULL, except the lyricist (0) and the colour (0).
        assert_eq!(
            (c.artist_id_remixer, c.album_id, c.key_id),
            (None, None, None)
        );
        assert_eq!((c.artist_id_lyricist, c.color_id), (0, 0));
        assert_eq!((c.master_db_id, c.master_content_id), (7, 99));
        assert_eq!((c.content_link, c.analysed_bits), (788_224, 41));
        assert_eq!(
            (c.is_hot_cue_auto_load_on, c.is_kuvo_deliver_status_on),
            (1, 0)
        );
        assert_eq!(c.date_created, "2026-10-20");
        assert_eq!(c.title_for_search, None);
    }

    #[test]
    fn lookups_keep_pdb_ids() {
        let mut pdb = PdbLibrary::default();
        pdb.keys.insert(4, "Am".into());
        pdb.albums.insert(
            2,
            PdbAlbum {
                name: "Al".into(),
                artist_id: 0,
            },
        );
        let ol = from_pdb(&pdb, "2026-10-21");
        assert_eq!(
            ol.keys,
            [Named {
                id: 4,
                name: "Am".into()
            }]
        );
        assert_eq!(ol.albums[0].artist_id, None);
    }

    #[test]
    fn playlists_keep_tree_and_order() {
        let pdb = PdbLibrary {
            tracks: vec![track(1), track(2)],
            playlists: vec![
                PdbPlaylist {
                    id: 1,
                    parent_id: 0,
                    sort_order: 0,
                    name: "Folder".into(),
                    is_folder: true,
                    entries: vec![],
                },
                PdbPlaylist {
                    id: 2,
                    parent_id: 1,
                    sort_order: 3,
                    name: "List".into(),
                    is_folder: false,
                    entries: vec![2, 1],
                },
            ],
            ..PdbLibrary::default()
        };
        let ol = from_pdb(&pdb, "2026-10-21");
        assert_eq!(ol.playlists[0].attribute, ATTRIBUTE_FOLDER);
        assert_eq!(
            (ol.playlists[1].parent_id, ol.playlists[1].sequence_no),
            (1, 3)
        );
        let entries: Vec<(i64, i64)> = ol
            .playlist_entries
            .iter()
            .map(|e| (e.content_id, e.sequence_no))
            .collect();
        assert_eq!(entries, [(2, 1), (1, 2)]);
    }

    #[test]
    fn property_counts_tracks() {
        let pdb = PdbLibrary {
            tracks: vec![track(1), track(2)],
            ..PdbLibrary::default()
        };
        let p = from_pdb(&pdb, "2026-10-21").property;
        assert_eq!(p.number_of_contents, 2);
        assert_eq!(p.db_version, "1000");
        assert_eq!(p.created_date, "2026-10-21");
    }
}
