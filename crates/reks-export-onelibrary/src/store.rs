//! [`OneLibrary`] rows in and out of an `exportLibrary.db` connection. The
//! only place that knows the tables' column order.

use rusqlite::{params, Connection, Row};

use crate::error::Result;
use crate::rows::{
    Album, Artist, Content, Image, Named, OneLibrary, Playlist, PlaylistEntry, Property,
};

const CONTENT_COLUMNS: &str = "content_id, title, titleForSearch, subtitle, bpmx100, length, \
    trackNo, discNo, artist_id_artist, artist_id_remixer, artist_id_originalArtist, \
    artist_id_composer, artist_id_lyricist, album_id, genre_id, label_id, key_id, color_id, \
    image_id, djComment, rating, releaseYear, releaseDate, dateCreated, dateAdded, path, \
    fileName, fileSize, fileType, bitrate, bitDepth, samplingRate, isrc, djPlayCount, \
    isHotCueAutoLoadOn, isKuvoDeliverStatusOn, kuvoDeliveryComment, masterDbId, \
    masterContentId, analysisDataFilePath, analysedBits, contentLink, hasModified, \
    cueUpdateCount, analysisDataUpdateCount, informationUpdateCount";

/// Inserts `lib` into an empty database with the reference schema, in one
/// transaction.
pub fn insert(conn: &mut Connection, lib: &OneLibrary) -> Result<()> {
    let tx = conn.transaction()?;
    {
        let marks = vec!["?"; 46].join(", ");
        let mut s = tx.prepare(&format!(
            "INSERT INTO content ({CONTENT_COLUMNS}) VALUES ({marks})"
        ))?;
        for c in &lib.contents {
            s.execute(params![
                c.content_id,
                c.title,
                c.title_for_search,
                c.subtitle,
                c.bpmx100,
                c.length,
                c.track_no,
                c.disc_no,
                c.artist_id_artist,
                c.artist_id_remixer,
                c.artist_id_original_artist,
                c.artist_id_composer,
                c.artist_id_lyricist,
                c.album_id,
                c.genre_id,
                c.label_id,
                c.key_id,
                c.color_id,
                c.image_id,
                c.dj_comment,
                c.rating,
                c.release_year,
                c.release_date,
                c.date_created,
                c.date_added,
                c.path,
                c.file_name,
                c.file_size,
                c.file_type,
                c.bitrate,
                c.bit_depth,
                c.sampling_rate,
                c.isrc,
                c.dj_play_count,
                c.is_hot_cue_auto_load_on,
                c.is_kuvo_deliver_status_on,
                c.kuvo_delivery_comment,
                c.master_db_id,
                c.master_content_id,
                c.analysis_data_file_path,
                c.analysed_bits,
                c.content_link,
                c.has_modified,
                c.cue_update_count,
                c.analysis_data_update_count,
                c.information_update_count,
            ])?;
        }
        let mut s =
            tx.prepare("INSERT INTO artist (artist_id, name, nameForSearch) VALUES (?, ?, NULL)")?;
        for a in &lib.artists {
            s.execute(params![a.artist_id, a.name])?;
        }
        let mut s = tx.prepare(
            "INSERT INTO album (album_id, name, artist_id, image_id, isComplation, nameForSearch) \
             VALUES (?, ?, ?, NULL, 0, NULL)",
        )?;
        for a in &lib.albums {
            s.execute(params![a.album_id, a.name, a.artist_id])?;
        }
        for (table, rows) in [
            ("genre", &lib.genres),
            ("label", &lib.labels),
            ("key", &lib.keys),
            ("color", &lib.colors),
        ] {
            let mut s = tx.prepare(&format!("INSERT INTO {table} VALUES (?, ?)"))?;
            for r in rows {
                s.execute(params![r.id, r.name])?;
            }
        }
        let mut s = tx.prepare("INSERT INTO image (image_id, path) VALUES (?, ?)")?;
        for i in &lib.images {
            s.execute(params![i.image_id, i.path])?;
        }
        let mut s = tx.prepare(
            "INSERT INTO playlist (playlist_id, sequenceNo, name, image_id, attribute, \
             playlist_id_parent) VALUES (?, ?, ?, NULL, ?, ?)",
        )?;
        for p in &lib.playlists {
            s.execute(params![
                p.playlist_id,
                p.sequence_no,
                p.name,
                p.attribute,
                p.parent_id
            ])?;
        }
        let mut s = tx.prepare(
            "INSERT INTO playlist_content (playlist_id, content_id, sequenceNo) VALUES (?, ?, ?)",
        )?;
        for e in &lib.playlist_entries {
            s.execute(params![e.playlist_id, e.content_id, e.sequence_no])?;
        }
        let p = &lib.property;
        tx.execute(
            "INSERT INTO property (deviceName, dbVersion, numberOfContents, createdDate, \
             backGroundColorType, myTagMasterDBID) VALUES (?, ?, ?, ?, ?, ?)",
            params![
                p.device_name,
                p.db_version,
                p.number_of_contents,
                p.created_date,
                p.back_ground_color_type,
                p.my_tag_master_dbid
            ],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// Reads every non-seed row back, in rowid order. `property` is the first
/// row (default when there is none).
pub fn read(conn: &Connection) -> Result<OneLibrary> {
    Ok(OneLibrary {
        contents: rows(
            conn,
            &format!("SELECT {CONTENT_COLUMNS} FROM content ORDER BY rowid"),
            content,
        )?,
        artists: rows(
            conn,
            "SELECT artist_id, name FROM artist ORDER BY rowid",
            |r| {
                Ok(Artist {
                    artist_id: r.get(0)?,
                    name: r.get(1)?,
                })
            },
        )?,
        albums: rows(
            conn,
            "SELECT album_id, name, artist_id FROM album ORDER BY rowid",
            |r| {
                Ok(Album {
                    album_id: r.get(0)?,
                    name: r.get(1)?,
                    artist_id: r.get(2)?,
                })
            },
        )?,
        genres: named(conn, "genre")?,
        labels: named(conn, "label")?,
        keys: named(conn, "key")?,
        colors: named(conn, "color")?,
        images: rows(
            conn,
            "SELECT image_id, path FROM image ORDER BY rowid",
            |r| {
                Ok(Image {
                    image_id: r.get(0)?,
                    path: r.get(1)?,
                })
            },
        )?,
        playlists: rows(
            conn,
            "SELECT playlist_id, sequenceNo, name, attribute, playlist_id_parent FROM playlist \
             ORDER BY rowid",
            |r| {
                Ok(Playlist {
                    playlist_id: r.get(0)?,
                    sequence_no: r.get(1)?,
                    name: r.get(2)?,
                    attribute: r.get(3)?,
                    parent_id: r.get(4)?,
                })
            },
        )?,
        playlist_entries: rows(
            conn,
            "SELECT playlist_id, content_id, sequenceNo FROM playlist_content ORDER BY rowid",
            |r| {
                Ok(PlaylistEntry {
                    playlist_id: r.get(0)?,
                    content_id: r.get(1)?,
                    sequence_no: r.get(2)?,
                })
            },
        )?,
        property: rows(
            conn,
            "SELECT deviceName, dbVersion, numberOfContents, createdDate, backGroundColorType, \
             myTagMasterDBID FROM property ORDER BY rowid LIMIT 1",
            |r| {
                Ok(Property {
                    device_name: r.get(0)?,
                    db_version: r.get(1)?,
                    number_of_contents: r.get(2)?,
                    created_date: r.get(3)?,
                    back_ground_color_type: r.get(4)?,
                    my_tag_master_dbid: r.get(5)?,
                })
            },
        )?
        .pop()
        .unwrap_or_default(),
    })
}

fn rows<T>(
    conn: &Connection,
    sql: &str,
    f: impl FnMut(&Row<'_>) -> rusqlite::Result<T>,
) -> Result<Vec<T>> {
    let mut stmt = conn.prepare(sql)?;
    let out = stmt.query_map([], f)?.collect::<rusqlite::Result<_>>()?;
    Ok(out)
}

fn named(conn: &Connection, table: &str) -> Result<Vec<Named>> {
    rows(
        conn,
        &format!("SELECT * FROM {table} ORDER BY rowid"),
        |r| {
            Ok(Named {
                id: r.get(0)?,
                name: r.get(1)?,
            })
        },
    )
}

fn content(r: &Row<'_>) -> rusqlite::Result<Content> {
    Ok(Content {
        content_id: r.get(0)?,
        title: r.get(1)?,
        title_for_search: r.get(2)?,
        subtitle: r.get(3)?,
        bpmx100: r.get(4)?,
        length: r.get(5)?,
        track_no: r.get(6)?,
        disc_no: r.get(7)?,
        artist_id_artist: r.get(8)?,
        artist_id_remixer: r.get(9)?,
        artist_id_original_artist: r.get(10)?,
        artist_id_composer: r.get(11)?,
        artist_id_lyricist: r.get(12)?,
        album_id: r.get(13)?,
        genre_id: r.get(14)?,
        label_id: r.get(15)?,
        key_id: r.get(16)?,
        color_id: r.get(17)?,
        image_id: r.get(18)?,
        dj_comment: r.get(19)?,
        rating: r.get(20)?,
        release_year: r.get(21)?,
        release_date: r.get(22)?,
        date_created: r.get(23)?,
        date_added: r.get(24)?,
        path: r.get(25)?,
        file_name: r.get(26)?,
        file_size: r.get(27)?,
        file_type: r.get(28)?,
        bitrate: r.get(29)?,
        bit_depth: r.get(30)?,
        sampling_rate: r.get(31)?,
        isrc: r.get(32)?,
        dj_play_count: r.get(33)?,
        is_hot_cue_auto_load_on: r.get(34)?,
        is_kuvo_deliver_status_on: r.get(35)?,
        kuvo_delivery_comment: r.get(36)?,
        master_db_id: r.get(37)?,
        master_content_id: r.get(38)?,
        analysis_data_file_path: r.get(39)?,
        analysed_bits: r.get(40)?,
        content_link: r.get(41)?,
        has_modified: r.get(42)?,
        cue_update_count: r.get(43)?,
        analysis_data_update_count: r.get(44)?,
        information_update_count: r.get(45)?,
    })
}
