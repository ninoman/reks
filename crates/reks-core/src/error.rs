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

    /// SQLite refused WAL mode (e.g. a filesystem without shared-memory support).
    #[error("could not enable WAL journal mode (got {0:?})")]
    JournalMode(String),

    /// A thread panicked while holding the connection lock.
    #[error("library connection lock poisoned")]
    LockPoisoned,
}

/// Result alias for `reks-core`.
pub type Result<T> = std::result::Result<T, Error>;
