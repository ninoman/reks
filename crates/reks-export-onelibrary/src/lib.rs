//! reks OneLibrary (`exportLibrary.db`) export (Phase 4, D14).
//!
//! The database is SQLCipher-encrypted with a key supplied at runtime by the
//! user (setting or `REKS_ONELIBRARY_KEY`); this crate never stores it.
//! [`build::from_pdb`] turns the `export.pdb` just written into rows,
//! [`schema`] and [`store`] put them into a database, [`reader`] reads one
//! back, [`writer`] puts it on a stick, [`validate`] and [`diff`] check it.
//! [`history`] reads the play history players logged in one (task 11).

pub mod build;
pub mod cipher;
pub mod diff;
pub mod dump;
pub mod error;
pub mod history;
pub mod reader;
pub mod rows;
pub mod schema;
pub mod store;
#[cfg(test)]
mod testutil;
pub mod validate;
pub mod writer;

pub use cipher::CipherProfile;
pub use error::{Error, Result};
pub use reader::OneLibraryReader;
pub use schema::RbVersion;
pub use writer::OneLibraryWriter;
