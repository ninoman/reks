//! Error type for `reks-export-onelibrary`. Messages never contain the key.

use std::path::PathBuf;

/// Errors returned by this crate.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// The key did not open the file, or it is not an SQLCipher database.
    #[error("{path} could not be decrypted with this key and cipher profile")]
    NotDecrypted { path: PathBuf },

    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Result alias for this crate.
pub type Result<T, E = Error> = std::result::Result<T, E>;
