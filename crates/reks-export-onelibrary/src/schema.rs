//! The `exportLibrary.db` schema and seed rows per observed rekordbox version
//! (task 3), copied from the reference exports under `fixtures/onelibrary/`.
//! The writer refuses versions it has no reference for.

use rusqlite::Connection;

use crate::cipher::CipherProfile;
use crate::error::Result;

/// The cipher every observed version uses: stock SQLCipher 4, the key as a
/// passphrase (task 2, rekordbox 7.2.19).
pub const PROFILE: CipherProfile = CipherProfile::Sqlcipher4;

/// A rekordbox version whose `exportLibrary.db` we have a reference for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RbVersion {
    /// rekordbox 7.2.19 (reference captured 2026-10-20).
    V7_2_19,
}

impl RbVersion {
    /// The version new exports target.
    pub const LATEST: Self = Self::V7_2_19;

    pub fn as_str(self) -> &'static str {
        match self {
            Self::V7_2_19 => "7.2.19",
        }
    }

    /// `CREATE` statements, as rekordbox's `sqlite_master` holds them, in
    /// its creation order.
    pub fn ddl(self) -> &'static str {
        match self {
            Self::V7_2_19 => include_str!("../data/7.2.19/schema.sql"),
        }
    }

    /// Rows of the tables rekordbox fills the same way on every export
    /// (`menuItem`, `category`, `sort`, `myTag`).
    pub fn seed(self) -> &'static str {
        match self {
            Self::V7_2_19 => include_str!("../data/7.2.19/seed.sql"),
        }
    }
}

/// Creates the schema and seed rows of `version` in an empty database.
pub fn create_empty(conn: &Connection, version: RbVersion) -> Result<()> {
    conn.execute_batch(version.ddl())?;
    conn.execute_batch(version.seed())?;
    Ok(())
}
