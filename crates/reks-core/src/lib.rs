//! reks library model: SQLite schema and migrations, scan/import, tags, artwork, playlists, rekordbox XML, services
//!
//! See `docs/phases/` for what lands here and when.

pub mod artwork;
pub mod db;
pub mod error;
pub mod keys;
pub mod scan;
pub mod timefmt;

pub use artwork::ArtworkId;
pub use db::Library;
pub use error::{Error, Result};
pub use keys::{Key, Mode};
pub use scan::{scan, CancelToken, ScanEvent, ScanReport, TrackId};

/// Crate version, from Cargo metadata.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_set() {
        assert!(!super::VERSION.is_empty());
    }
}
