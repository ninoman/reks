//! reks library model: SQLite schema and migrations, scan/import, tags, artwork, playlists, rekordbox XML, services
//!
//! See `docs/phases/` for what lands here and when.

pub mod artwork;
pub mod db;
pub mod error;
mod fileinfo;
pub mod keys;
pub mod playlist;
pub mod preview;
pub mod query;
pub mod rbxml;
pub mod scan;
pub mod timefmt;
pub mod track;

pub use artwork::ArtworkId;
pub use db::Library;
pub use error::{Error, Result};
pub use keys::{Key, Mode};
pub use playlist::{DuplicateMode, PlaylistOp, PlaylistOutcome, PlaylistTree};
pub use preview::PreviewAudio;
pub use query::{
    Facets, Filter, KeyInfo, PlaylistId, PlaylistNode, Sort, SortColumn, TrackPage, TrackQuery,
    TrackRow,
};
pub use scan::{scan, CancelToken, ScanEvent, ScanReport, TrackId};
pub use track::{ArtworkInfo, BeatgridSummary, Cue, Track, TrackDetail, TrackPatch};

/// Crate version, from Cargo metadata.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_set() {
        assert!(!super::VERSION.is_empty());
    }
}
