//! rekordbox XML (`collection.xml`): model, streaming parser and writer.
//!
//! Format reference: `docs/research.md` §7. Model and round-trip rules:
//! `docs/phases/phase-1.md` (task 6 notes). Import into the library:
//! [`import()`] (task 7 notes). Export of a playlist selection (the input
//! for the USB writer): [`export()`] (`docs/phases/phase-2.md` task 6).
//!
//! Every attribute we don't model is kept in an `extra` map, and a known
//! numeric attribute whose value doesn't parse is kept there too (raw), so
//! `parse(write(c)) == c` holds for any parsed collection.

mod colour;
mod export;
mod import;
mod location;
mod parse;
mod write;

use std::collections::BTreeMap;

pub use colour::{colour_name, colour_rgb, TRACK_COLOURS};
pub use export::{
    export, export_file, export_file_with_dividers, export_with_dividers, DividerTrack, Selection,
    XmlExport,
};
pub use import::{
    import, preview, ImportEvent, ImportOptions, ImportPreview, ImportReport, ImportStage,
    ImportWarning, PrefixRemap, Unmatched, UnmatchedReason, IMPORTED_TAG_CATEGORY, PREVIEW_SAMPLE,
};
pub use location::{location_to_path, path_to_location};
pub use parse::{parse, parse_file};
pub use write::write;

/// Attributes we don't model, by name, with their (unescaped) values.
pub type Extra = BTreeMap<String, String>;

/// A whole rekordbox XML document (`DJ_PLAYLISTS`).
#[derive(Debug, Clone, PartialEq)]
pub struct Collection {
    /// `DJ_PLAYLISTS@Version`; `"1.0.0"` in every known file.
    pub version: String,
    pub product: Product,
    /// `COLLECTION` entries, in document order.
    pub tracks: Vec<XmlTrack>,
    /// The `ROOT` folder under `PLAYLISTS` (an empty folder if absent).
    pub playlists: XmlNode,
    /// Other `DJ_PLAYLISTS` attributes.
    pub extra: Extra,
}

impl Default for Collection {
    fn default() -> Self {
        Self {
            version: "1.0.0".to_owned(),
            product: Product::default(),
            tracks: Vec::new(),
            playlists: XmlNode::folder("ROOT", Vec::new()),
            extra: Extra::new(),
        }
    }
}

/// `PRODUCT`: the application that wrote the file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Product {
    pub name: Option<String>,
    pub version: Option<String>,
    pub company: Option<String>,
    pub extra: Extra,
}

/// One `COLLECTION/TRACK`. Field names follow the XML attributes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct XmlTrack {
    /// `TrackID`: what playlist `TRACK Key` refers to when `KeyType="0"`.
    pub track_id: u32,
    pub name: Option<String>,
    pub artist: Option<String>,
    pub composer: Option<String>,
    pub album: Option<String>,
    pub grouping: Option<String>,
    pub genre: Option<String>,
    /// e.g. `"MP3 File"`.
    pub kind: Option<String>,
    /// Bytes.
    pub size: Option<u64>,
    /// Whole seconds.
    pub total_time: Option<u32>,
    pub disc_number: Option<u32>,
    pub track_number: Option<u32>,
    pub year: Option<u32>,
    pub average_bpm: Option<f64>,
    /// Raw, as written (`yyyy-mm-dd`).
    pub date_modified: Option<String>,
    /// Raw, as written (`yyyy-mm-dd`).
    pub date_added: Option<String>,
    /// kbit/s.
    pub bit_rate: Option<u32>,
    /// Hz.
    pub sample_rate: Option<u32>,
    pub comments: Option<String>,
    pub play_count: Option<u32>,
    /// Raw, as written.
    pub last_played: Option<String>,
    /// 0, 51, 102, 153, 204, 255 for 0–5 stars.
    pub rating: Option<u8>,
    /// Raw `file://localhost/...` URL; see [`location_to_path`].
    pub location: Option<String>,
    pub remixer: Option<String>,
    /// Key as rekordbox wrote it; normalise with `keys.rs`.
    pub tonality: Option<String>,
    pub label: Option<String>,
    pub mix: Option<String>,
    /// `0xRRGGBB`.
    pub colour: Option<u32>,
    pub tempos: Vec<Tempo>,
    pub position_marks: Vec<PositionMark>,
    pub extra: Extra,
}

/// `TEMPO`: one beatgrid anchor.
#[derive(Debug, Clone, PartialEq)]
pub struct Tempo {
    /// `Inizio`: seconds from track start.
    pub inizio: f64,
    pub bpm: f64,
    /// `Metro`, e.g. `"4/4"`.
    pub metro: Option<String>,
    /// `Battito`: beat number in the bar (1–4).
    pub battito: Option<u8>,
    pub extra: Extra,
}

/// `POSITION_MARK`: a cue or loop.
#[derive(Debug, Clone, PartialEq)]
pub struct PositionMark {
    pub name: Option<String>,
    /// `Type`: 0 cue, 1 fade-in, 2 fade-out, 3 load, 4 loop.
    pub kind: u8,
    /// Seconds.
    pub start: f64,
    /// Seconds; loops only.
    pub end: Option<f64>,
    /// −1 memory cue, 0..7 hot cue A..H.
    pub num: i32,
    /// `Red`/`Green`/`Blue`, present only when all three are.
    pub rgb: Option<(u8, u8, u8)>,
    pub extra: Extra,
}

/// A `PLAYLISTS/NODE`.
#[derive(Debug, Clone, PartialEq)]
pub struct XmlNode {
    pub name: String,
    pub kind: NodeKind,
    pub extra: Extra,
}

/// Folder (`Type="0"`) or playlist (`Type="1"`).
#[derive(Debug, Clone, PartialEq)]
pub enum NodeKind {
    Folder(Vec<XmlNode>),
    Playlist {
        /// 0: keys are `TrackID`s; 1: keys are `Location`s.
        key_type: u8,
        /// `TRACK Key` values, in playlist order.
        keys: Vec<String>,
    },
}

impl XmlNode {
    pub fn folder(name: impl Into<String>, children: Vec<XmlNode>) -> Self {
        Self {
            name: name.into(),
            kind: NodeKind::Folder(children),
            extra: Extra::new(),
        }
    }

    pub fn playlist(name: impl Into<String>, key_type: u8, keys: Vec<String>) -> Self {
        Self {
            name: name.into(),
            kind: NodeKind::Playlist { key_type, keys },
            extra: Extra::new(),
        }
    }
}

/// Errors from parsing or writing rekordbox XML.
#[derive(Debug, thiserror::Error)]
pub enum RbXmlError {
    #[error("malformed XML near byte {position}: {source}")]
    Xml {
        position: u64,
        #[source]
        source: quick_xml::Error,
    },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("<{element}> near byte {position} lacks required attribute {attr}")]
    Missing {
        element: &'static str,
        attr: &'static str,
        position: u64,
    },

    #[error("<{element}> near byte {position} has invalid {attr}={value:?}")]
    Invalid {
        element: &'static str,
        attr: &'static str,
        value: String,
        position: u64,
    },

    /// Well-formed XML that isn't a rekordbox collection.
    #[error("not a rekordbox XML collection: {0}")]
    Structure(String),

    #[error("invalid Location {url:?}: {reason}")]
    InvalidLocation { url: String, reason: &'static str },
}
