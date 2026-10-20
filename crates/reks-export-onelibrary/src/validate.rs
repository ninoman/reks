//! Validating a written `exportLibrary.db` against the stick (task 7): the
//! same tracks, paths and ANLZ files as `export.pdb`, a `property` count
//! that matches, and no playlist rows pointing nowhere.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;

use reks_export_rb::onelibrary::DB_PATH;
use reks_export_rb::pdbread::{self, PdbLibrary};
use reks_export_rb::validate::locate;

use crate::error::{Error, Result};
use crate::reader::OneLibraryReader;
use crate::rows::OneLibrary;

/// One problem with the stick's `exportLibrary.db`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Finding {
    DbMissing,
    /// The key does not open it.
    NotDecrypted,
    IntegrityCheck(String),
    ContentCount {
        pdb: usize,
        db: usize,
    },
    /// `property.numberOfContents` is not the `content` row count.
    PropertyCount {
        property: i64,
        contents: usize,
    },
    /// A `content` path that `export.pdb` does not list.
    NotInPdb {
        path: String,
    },
    /// An `export.pdb` track with no `content` row.
    NotInDb {
        path: String,
    },
    /// `analysisDataFilePath` differs from the PDB row's `analyze_path`.
    AnlzPath {
        path: String,
        pdb: String,
        db: String,
    },
    /// `analysisDataFilePath` names a file that is not on the stick.
    AnlzMissing {
        path: String,
        anlz: String,
    },
    PlaylistCount {
        pdb: usize,
        db: usize,
    },
    /// A playlist whose parent is neither 0 nor a playlist row.
    OrphanPlaylist {
        playlist_id: i64,
        parent_id: i64,
    },
    /// A `playlist_content` row whose playlist or content row is missing.
    OrphanEntry {
        playlist_id: i64,
        content_id: i64,
    },
}

/// Opens the stick's `exportLibrary.db` with `key` and checks it against
/// its `export.pdb` and ANLZ files. Read-only.
pub fn validate(device_root: &Path, key: &str) -> Result<Vec<Finding>> {
    let io = |source| Error::Io {
        path: device_root.to_owned(),
        source,
    };
    let Some(db) = locate(device_root, DB_PATH).map_err(io)? else {
        return Ok(vec![Finding::DbMissing]);
    };
    let reader = match OneLibraryReader::open(&db, key) {
        Ok(r) => r,
        Err(Error::NotDecrypted { .. }) => return Ok(vec![Finding::NotDecrypted]),
        Err(e) => return Err(e),
    };
    let mut findings = Vec::new();
    let integrity = reader.integrity_check()?;
    if integrity != "ok" {
        findings.push(Finding::IntegrityCheck(integrity));
    }
    let pdb = pdbread::read_stick(device_root).map_err(|e| Error::Pdb(e.to_string()))?;
    let lib = reader.library()?;
    let anlz_exists = |rel: &str| locate(device_root, rel).ok().flatten().is_some();
    findings.extend(check(&lib, &pdb, anlz_exists));
    Ok(findings)
}

/// The checks on rows already read. Pure but for `anlz_exists`.
pub fn check(
    lib: &OneLibrary,
    pdb: &PdbLibrary,
    anlz_exists: impl Fn(&str) -> bool,
) -> Vec<Finding> {
    let mut out = Vec::new();
    if lib.contents.len() != pdb.tracks.len() {
        out.push(Finding::ContentCount {
            pdb: pdb.tracks.len(),
            db: lib.contents.len(),
        });
    }
    if usize::try_from(lib.property.number_of_contents).ok() != Some(lib.contents.len()) {
        out.push(Finding::PropertyCount {
            property: lib.property.number_of_contents,
            contents: lib.contents.len(),
        });
    }
    let pdb_paths: BTreeMap<&str, &str> = pdb
        .tracks
        .iter()
        .map(|t| (t.file_path.as_str(), t.analyze_path.as_str()))
        .collect();
    let db_paths: BTreeSet<&str> = lib.contents.iter().map(|c| c.path.as_str()).collect();
    for c in &lib.contents {
        match pdb_paths.get(c.path.as_str()) {
            None => out.push(Finding::NotInPdb {
                path: c.path.clone(),
            }),
            Some(&anlz) if anlz != c.analysis_data_file_path => out.push(Finding::AnlzPath {
                path: c.path.clone(),
                pdb: anlz.to_owned(),
                db: c.analysis_data_file_path.clone(),
            }),
            Some(_) => {}
        }
        if !c.analysis_data_file_path.is_empty() && !anlz_exists(&c.analysis_data_file_path) {
            out.push(Finding::AnlzMissing {
                path: c.path.clone(),
                anlz: c.analysis_data_file_path.clone(),
            });
        }
    }
    for path in pdb_paths.keys().filter(|p| !db_paths.contains(*p)) {
        out.push(Finding::NotInDb {
            path: (*path).to_owned(),
        });
    }
    if lib.playlists.len() != pdb.playlists.len() {
        out.push(Finding::PlaylistCount {
            pdb: pdb.playlists.len(),
            db: lib.playlists.len(),
        });
    }
    let playlists: BTreeSet<i64> = lib.playlists.iter().map(|p| p.playlist_id).collect();
    let contents: BTreeSet<i64> = lib.contents.iter().map(|c| c.content_id).collect();
    for p in &lib.playlists {
        if p.parent_id != 0 && !playlists.contains(&p.parent_id) {
            out.push(Finding::OrphanPlaylist {
                playlist_id: p.playlist_id,
                parent_id: p.parent_id,
            });
        }
    }
    for e in &lib.playlist_entries {
        if !playlists.contains(&e.playlist_id) || !contents.contains(&e.content_id) {
            out.push(Finding::OrphanEntry {
                playlist_id: e.playlist_id,
                content_id: e.content_id,
            });
        }
    }
    out
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DbMissing => write!(f, "no exportLibrary.db"),
            Self::NotDecrypted => write!(f, "the key does not open exportLibrary.db"),
            Self::IntegrityCheck(m) => write!(f, "integrity check: {m}"),
            Self::ContentCount { pdb, db } => {
                write!(f, "{db} content rows, export.pdb has {pdb} tracks")
            }
            Self::PropertyCount { property, contents } => write!(
                f,
                "property.numberOfContents is {property}, content has {contents} rows"
            ),
            Self::NotInPdb { path } => write!(f, "{path}: in exportLibrary.db, not in export.pdb"),
            Self::NotInDb { path } => write!(f, "{path}: in export.pdb, not in exportLibrary.db"),
            Self::AnlzPath { path, pdb, db } => {
                write!(f, "{path}: analysis path {db}, export.pdb has {pdb}")
            }
            Self::AnlzMissing { path, anlz } => write!(f, "{path}: {anlz} is not on the stick"),
            Self::PlaylistCount { pdb, db } => {
                write!(f, "{db} playlist rows, export.pdb has {pdb}")
            }
            Self::OrphanPlaylist {
                playlist_id,
                parent_id,
            } => write!(
                f,
                "playlist {playlist_id}: parent {parent_id} does not exist"
            ),
            Self::OrphanEntry {
                playlist_id,
                content_id,
            } => write!(
                f,
                "playlist_content ({playlist_id}, {content_id}) points at a missing row"
            ),
        }
    }
}

#[cfg(test)]
mod tests;
