//! reks library model: SQLite schema and migrations, scan/import, tags, artwork, playlists, rekordbox XML, services
//!
//! See `docs/phases/` for what lands here and when.

pub mod adopt;
pub mod analysis;
pub mod artwork;
pub mod backup;
pub mod beatgrid;
pub mod bulk_edit;
pub mod convert;
pub mod cue;
pub mod db;
pub mod deck;
pub mod device;
pub mod diagnostics;
pub mod error;
mod fileinfo;
pub mod filter_preset;
pub mod fsname;
pub mod history;
pub mod keys;
pub mod manifest;
pub mod playlist;
mod pool;
pub mod preview;
pub mod query;
pub mod rbxml;
pub mod relocate;
pub mod rule;
pub mod scan;
pub mod secret;
pub mod settings;
pub mod tag;
pub mod tag_comments;
pub mod timefmt;
pub mod track;

pub use analysis::{AnalysisAnchor, AnalysisJob, AnalysisRecord, MappingCandidate, StoreOutcome};
pub use artwork::ArtworkId;
pub use backup::{BackupContents, BackupFailure, BackupInfo, BackupReason, BackupStatus};
pub use beatgrid::{BeatMark, Beatgrid, GridAnchor, GridEdit};
pub use bulk_edit::{BulkOutcome, BulkPatch, CommentEdit};
pub use cue::{HOT_CUE_PALETTE, HOT_CUE_SLOTS, MAX_CUE_NAME_CHARS};
pub use db::Library;
pub use deck::{deck_beats, CueMark, DeckAudio, DeckBeat};
pub use device::{Device, DeviceId, NewDevice, SyncedPlaylist, SyncedRecord};
pub use error::{Error, Result};
pub use filter_preset::{FilterPreset, FilterPresetId, FilterPresetOp, FilterPresetOutcome};
pub use keys::{Key, KeyNotation, Mode};
pub use manifest::{ManifestEntry, ManifestItemKind, ManifestRow};
pub use playlist::{
    DuplicateMode, PlaylistEntries, PlaylistOp, PlaylistOutcome, PlaylistTree,
    MAX_DIVIDER_LABEL_CHARS,
};
pub use preview::PreviewAudio;
pub use query::{
    DividerRow, Facets, Filter, KeyInfo, PlaylistId, PlaylistNode, Sort, SortColumn, TrackPage,
    TrackPageRows, TrackQuery, TrackRow,
};
pub use rule::{Match, RuleNode, MAX_RULE_DEPTH, MAX_RULE_NODES};
pub use scan::{scan, CancelToken, ScanEvent, ScanReport, TrackId};
pub use secret::OneLibraryKey;
pub use settings::{
    AnlzPreference, DeckWaveform, FilterSaveTarget, LibraryRoot, MixerEq, MixerFilters, MixerView,
    RowDensity, Settings, SettingsPatch, Theme, UiState,
};
pub use tag::{TagCategory, TagId, TagInfo, TagOp, TagOutcome, TagTree};
pub use tag_comments::{CommentTagsPreview, CommentTagsReport};
pub use track::{
    ArtworkInfo, BeatgridSummary, Cue, ExportTrackFields, Track, TrackDetail, TrackPatch,
};

/// Crate version, from Cargo metadata.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_set() {
        assert!(!super::VERSION.is_empty());
    }
}
