//! Error type for `reks-audio`.

use std::path::PathBuf;

/// Errors returned by `reks-audio`. Every variant names the file involved.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot open {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Not an audio format/codec we handle (e.g. HE-AAC, video, unknown file).
    #[error("unsupported audio in {path}: {reason}")]
    Unsupported { path: PathBuf, reason: String },

    /// Symphonia could not read the container or stream.
    #[error("cannot read audio in {path}: {source}")]
    Decode {
        path: PathBuf,
        #[source]
        source: symphonia::core::errors::Error,
    },

    /// lofty could not read the file's tags or properties.
    #[error("cannot read tags in {path}: {source}")]
    Tags {
        path: PathBuf,
        #[source]
        source: lofty::error::FileParseError,
    },
}

/// Result alias for `reks-audio`.
pub type Result<T> = std::result::Result<T, Error>;
