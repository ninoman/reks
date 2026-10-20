//! The rows reks writes to `exportLibrary.db`, one struct per table with the
//! reference schema's columns (`data/<version>/schema.sql`). Seed tables
//! (`menuItem`, `category`, `sort`, `myTag`) come from `seed.sql`, not here.

/// A `content` row. `None` is SQL NULL, as rekordbox writes it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Content {
    pub content_id: i64,
    pub title: String,
    pub title_for_search: Option<String>,
    pub subtitle: String,
    pub bpmx100: i64,
    /// Whole seconds.
    pub length: i64,
    pub track_no: i64,
    pub disc_no: i64,
    pub artist_id_artist: Option<i64>,
    pub artist_id_remixer: Option<i64>,
    pub artist_id_original_artist: Option<i64>,
    pub artist_id_composer: Option<i64>,
    /// rekordbox writes 0 here where the other artist ids are NULL.
    pub artist_id_lyricist: i64,
    pub album_id: Option<i64>,
    pub genre_id: Option<i64>,
    pub label_id: Option<i64>,
    pub key_id: Option<i64>,
    /// 0 = no colour (not NULL).
    pub color_id: i64,
    pub image_id: Option<i64>,
    pub dj_comment: String,
    pub rating: i64,
    pub release_year: i64,
    pub release_date: String,
    /// `YYYY-MM-DD`.
    pub date_created: String,
    pub date_added: String,
    /// `/Contents/...`, as in `export.pdb`.
    pub path: String,
    pub file_name: String,
    pub file_size: i64,
    /// `export.pdb` file type codes.
    pub file_type: i64,
    pub bitrate: i64,
    pub bit_depth: i64,
    pub sampling_rate: i64,
    pub isrc: String,
    pub dj_play_count: i64,
    pub is_hot_cue_auto_load_on: i64,
    pub is_kuvo_deliver_status_on: i64,
    pub kuvo_delivery_comment: String,
    pub master_db_id: i64,
    pub master_content_id: i64,
    /// `/PIONEER/USBANLZ/.../ANLZ0000.DAT`.
    pub analysis_data_file_path: String,
    pub analysed_bits: i64,
    pub content_link: i64,
    pub has_modified: i64,
    pub cue_update_count: Option<i64>,
    pub analysis_data_update_count: Option<i64>,
    pub information_update_count: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Artist {
    pub artist_id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Album {
    pub album_id: i64,
    pub name: String,
    pub artist_id: Option<i64>,
}

/// A row of a two-column lookup (`genre`, `label`, `key`, `color`): id, name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Named {
    pub id: i64,
    pub name: String,
}

/// An `image` row: id and the artwork path on the stick.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Image {
    pub image_id: i64,
    pub path: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Playlist {
    pub playlist_id: i64,
    pub sequence_no: i64,
    pub name: String,
    /// [`ATTRIBUTE_PLAYLIST`] or [`ATTRIBUTE_FOLDER`].
    pub attribute: i64,
    /// 0 at the root.
    pub parent_id: i64,
}

/// `playlist.attribute` of a playlist (reference export).
pub const ATTRIBUTE_PLAYLIST: i64 = 0;
/// `playlist.attribute` of a folder. UNVERIFIED: the reference has no
/// folder; 1 is rekordbox's `master.db` value and baken's.
pub const ATTRIBUTE_FOLDER: i64 = 1;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlaylistEntry {
    pub playlist_id: i64,
    pub content_id: i64,
    /// 1-based.
    pub sequence_no: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Property {
    pub device_name: String,
    pub db_version: String,
    pub number_of_contents: i64,
    /// `YYYY-MM-DD`.
    pub created_date: String,
    pub back_ground_color_type: i64,
    pub my_tag_master_dbid: i64,
}

/// Every non-seed row of one `exportLibrary.db`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OneLibrary {
    pub contents: Vec<Content>,
    pub artists: Vec<Artist>,
    pub albums: Vec<Album>,
    pub genres: Vec<Named>,
    pub labels: Vec<Named>,
    pub keys: Vec<Named>,
    pub colors: Vec<Named>,
    pub images: Vec<Image>,
    pub playlists: Vec<Playlist>,
    pub playlist_entries: Vec<PlaylistEntry>,
    pub property: Property,
}
