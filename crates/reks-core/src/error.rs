//! Error type for `reks-core`.

use std::path::PathBuf;

/// Errors returned by `reks-core`.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("rekordbox XML: {0}")]
    RbXml(#[from] crate::rbxml::RbXmlError),

    #[error("database migration failed: {0}")]
    Migration(#[from] rusqlite_migration::Error),

    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Decoding a track for playback or its waveform failed.
    #[error(transparent)]
    Audio(#[from] reks_audio::Error),

    /// SQLite refused WAL mode (e.g. a filesystem without shared-memory support).
    #[error("could not enable WAL journal mode (got {0:?})")]
    JournalMode(String),

    /// No track with this id.
    #[error("track {0} not found")]
    TrackNotFound(crate::scan::TrackId),

    /// No artwork with this id.
    #[error("artwork {0} not found")]
    ArtworkNotFound(crate::artwork::ArtworkId),

    /// A track edit was rejected; nothing was written.
    #[error("invalid {field}: {reason}")]
    InvalidField { field: &'static str, reason: String },

    /// No playlist or folder with this id.
    #[error("playlist {0} not found")]
    PlaylistNotFound(crate::query::PlaylistId),

    /// The op needs a playlist, and this id is a folder.
    #[error("{0} is a folder, not a playlist")]
    NotAPlaylist(crate::query::PlaylistId),

    /// The op needs a folder (a parent), and this id is a playlist.
    #[error("{0} is a playlist, not a folder")]
    NotAFolder(crate::query::PlaylistId),

    /// Moving `id` under `parent` would put it inside itself.
    #[error("cannot move {id} into itself or one of its descendants ({parent})")]
    MoveCycle {
        id: crate::query::PlaylistId,
        parent: crate::query::PlaylistId,
    },

    /// An entry position or sibling index outside the list.
    #[error("index {index} out of range (length {len})")]
    IndexOutOfRange { index: u32, len: u32 },

    /// The cache dir cannot move to `path`; nothing was changed.
    #[error("cannot move the cache to {path}: {reason}")]
    CacheDirRefused { path: PathBuf, reason: &'static str },

    /// No library root with this id.
    #[error("library folder {0} not found")]
    LibraryRootNotFound(i64),

    /// An export selection that holds no playlist (nothing, or only empty folders).
    #[error("the selection contains no playlist")]
    EmptySelection,

    /// No stick with this id.
    #[error("stick {0} not found")]
    DeviceNotFound(crate::device::DeviceId),

    /// The volume ID already belongs to another known stick.
    #[error("this stick's volume ID already belongs to '{name}'")]
    DeviceSerialInUse { name: String },

    /// One file of a conversion failed a check; its track is unchanged.
    #[error("cannot convert {path}: {reason}")]
    Convert { path: String, reason: String },

    /// A thread panicked while holding the connection lock.
    #[error("library connection lock poisoned")]
    LockPoisoned,
}

/// Result alias for `reks-core`.
pub type Result<T> = std::result::Result<T, Error>;
