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

    /// No cue with this id on the track.
    #[error("cue {0} not found")]
    CueNotFound(i64),

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

    /// The op edits a playlist's entries, and this playlist is smart: its
    /// tracks follow its rules (D29).
    #[error("{0} is a smart playlist; its tracks follow its rules")]
    SmartPlaylist(crate::query::PlaylistId),

    /// The op needs a smart playlist, and this id is a static playlist or a folder.
    #[error("{0} is not a smart playlist")]
    NotASmartPlaylist(crate::query::PlaylistId),

    /// Saving would make smart playlist `{0}` include itself through its
    /// rules (directly, or through other smart lists or a folder holding it).
    #[error("smart playlist {0} would include itself through its rules")]
    SmartPlaylistCycle(crate::query::PlaylistId),

    /// No tag or tag category with this id.
    #[error("tag {0} not found")]
    TagNotFound(crate::tag::TagId),

    /// The op needs a tag, and this id is a category.
    #[error("{0} is a tag category, not a tag")]
    NotATag(crate::tag::TagId),

    /// The op needs a category, and this id is a tag.
    #[error("{0} is a tag, not a tag category")]
    NotATagCategory(crate::tag::TagId),

    /// No saved filter with this id.
    #[error("saved filter {0} not found")]
    FilterPresetNotFound(crate::filter_preset::FilterPresetId),

    /// Another saved filter has this name (any case).
    #[error("a saved filter named '{0}' already exists")]
    FilterPresetNameTaken(String),

    /// An entry position or sibling index outside the list.
    #[error("index {index} out of range (length {len})")]
    IndexOutOfRange { index: u32, len: u32 },

    /// The op needs a divider, and the entry at `position` is a track.
    #[error("entry {position} of playlist {playlist} is not a divider")]
    NotADivider {
        playlist: crate::query::PlaylistId,
        position: u32,
    },

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

    /// No play session with this id.
    #[error("play session {0} not found")]
    SessionNotFound(crate::history::SessionId),

    /// No play session entry at this position.
    #[error("session {session} has no entry {position}")]
    SessionEntryNotFound {
        session: crate::history::SessionId,
        position: u32,
    },

    /// The op needs a track entry, and this session entry is a divider.
    #[error("entry {position} of session {session} is not a track entry")]
    NotATrackEntry {
        session: crate::history::SessionId,
        position: u32,
    },

    /// A history import plan no longer fits the library; nothing was written.
    #[error("the history import plan is out of date: {0}")]
    StaleHistoryPlan(String),

    /// An adopt plan no longer fits the library; nothing was written.
    #[error("the adopt plan is out of date: {0}")]
    StaleAdoptPlan(String),

    /// The grid edit needs a beatgrid and the track has none.
    #[error("track {0} has no beatgrid")]
    NoGrid(crate::scan::TrackId),

    /// The edit would leave a degenerate grid (D10, "Grid editing").
    #[error("beatgrid of track {track}: {problem}")]
    InvalidGrid {
        track: crate::scan::TrackId,
        problem: crate::beatgrid::GridProblem,
    },

    /// Reset found no grid from before the first edit, and no analysis.
    #[error("track {0} has no analysed grid to reset to")]
    NoGridBase(crate::scan::TrackId),

    /// The track's grid is locked; unlock it to edit.
    #[error("the beatgrid of track {0} is locked")]
    GridLocked(crate::scan::TrackId),

    /// A thread panicked while holding the connection lock.
    #[error("library connection lock poisoned")]
    LockPoisoned,
}

/// Result alias for `reks-core`.
pub type Result<T> = std::result::Result<T, Error>;
