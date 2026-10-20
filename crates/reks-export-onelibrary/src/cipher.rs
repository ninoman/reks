//! Opening and creating SQLCipher databases with a runtime key (D14).
//!
//! The key goes to SQLCipher through `PRAGMA key` and nowhere else: no logs,
//! no error messages.

use std::path::Path;

use rusqlite::{Connection, ErrorCode, OpenFlags};

use crate::error::{Error, Result};

/// SQLCipher parameters a database is encrypted with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CipherProfile {
    /// Stock SQLCipher 4: AES-256-CBC, PBKDF2-HMAC-SHA512 256 000 rounds,
    /// HMAC-SHA512, 4096-byte pages, the key used as a passphrase.
    Sqlcipher4,
}

impl CipherProfile {
    fn apply(self, conn: &Connection, key: &str) -> rusqlite::Result<()> {
        // `PRAGMA key` must be the first statement on the connection.
        conn.pragma_update(None, "key", key)?;
        match self {
            Self::Sqlcipher4 => conn.pragma_update(None, "cipher_compatibility", 4),
        }
    }
}

/// Opens an existing encrypted database read-only and checks the key works.
pub fn open_read_only(path: &Path, key: &str, profile: CipherProfile) -> Result<Connection> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    unlock(conn, path, key, profile)
}

/// Creates (or opens) an encrypted database for writing.
pub fn open_read_write(path: &Path, key: &str, profile: CipherProfile) -> Result<Connection> {
    let conn = Connection::open(path)?;
    unlock(conn, path, key, profile)
}

fn unlock(conn: Connection, path: &Path, key: &str, profile: CipherProfile) -> Result<Connection> {
    profile.apply(&conn, key)?;
    // SQLCipher only decrypts on first read; a wrong key shows up here.
    match conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| {
        r.get::<_, i64>(0)
    }) {
        Ok(_) => Ok(conn),
        Err(e) if e.sqlite_error_code() == Some(ErrorCode::NotADatabase) => {
            Err(Error::NotDecrypted {
                path: path.to_owned(),
            })
        }
        Err(e) => Err(e.into()),
    }
}

/// The SQLCipher library version, or `None` when the build links plain SQLite.
pub fn cipher_version(conn: &Connection) -> Result<Option<String>> {
    let mut stmt = conn.prepare("PRAGMA cipher_version")?;
    let mut rows = stmt.query([])?;
    Ok(match rows.next()? {
        Some(row) => Some(row.get(0)?),
        None => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "throwaway-test-key-not-the-real-one";

    fn tiny_db(path: &Path) {
        let conn = open_read_write(path, KEY, CipherProfile::Sqlcipher4).unwrap();
        conn.execute_batch("CREATE TABLE t (v TEXT); INSERT INTO t VALUES ('hello');")
            .unwrap();
    }

    #[test]
    fn build_links_sqlcipher_4() {
        let conn = Connection::open_in_memory().unwrap();
        let v = cipher_version(&conn).unwrap().unwrap();
        assert!(v.starts_with("4."), "{v}");
    }

    #[test]
    fn round_trips_with_the_same_key() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.db");
        tiny_db(&path);
        let conn = open_read_only(&path, KEY, CipherProfile::Sqlcipher4).unwrap();
        let v: String = conn.query_row("SELECT v FROM t", [], |r| r.get(0)).unwrap();
        assert_eq!(v, "hello");
    }

    #[test]
    fn file_is_encrypted_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.db");
        tiny_db(&path);
        let bytes = std::fs::read(&path).unwrap();
        assert!(!bytes.starts_with(b"SQLite format 3\0"));
        assert!(!bytes.windows(5).any(|w| w == b"hello"));
        // Without a key the file is not a database.
        let plain = Connection::open(&path).unwrap();
        let err = plain
            .query_row("SELECT count(*) FROM sqlite_master", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap_err();
        assert_eq!(err.sqlite_error_code(), Some(ErrorCode::NotADatabase));
    }

    #[test]
    fn wrong_key_is_not_decrypted_and_not_leaked() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.db");
        tiny_db(&path);
        let err = open_read_only(&path, "wrong-key", CipherProfile::Sqlcipher4).unwrap_err();
        assert!(matches!(err, Error::NotDecrypted { .. }), "{err:?}");
        let shown = format!("{err} {err:?}");
        assert!(!shown.contains("wrong-key"), "{shown}");
    }
}
