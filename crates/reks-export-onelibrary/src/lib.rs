//! reks OneLibrary (`exportLibrary.db`) export (Phase 4, D14).
//!
//! The database is SQLCipher-encrypted with a key supplied at runtime by the
//! user (setting or `REKS_ONELIBRARY_KEY`); this crate never stores it.

pub mod cipher;
pub mod error;

pub use cipher::CipherProfile;
pub use error::{Error, Result};
