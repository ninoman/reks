//! The rekordbox XML the import wizard last parsed, kept between the
//! preview (re-run on every remap edit) and the import so a large file is
//! parsed once.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use reks_core::rbxml::{self, Collection};

/// Identifies the file contents the cached parse came from.
#[derive(Debug, PartialEq, Eq)]
struct Stamp {
    path: PathBuf,
    modified: Option<SystemTime>,
    len: u64,
}

impl Stamp {
    fn of(path: &Path) -> std::io::Result<Self> {
        let meta = std::fs::metadata(path)?;
        Ok(Self {
            path: path.to_path_buf(),
            modified: meta.modified().ok(),
            len: meta.len(),
        })
    }
}

#[derive(Debug, Default)]
pub struct XmlCache {
    entry: Option<(Stamp, Arc<Collection>)>,
}

impl XmlCache {
    /// The parsed `path`: the cached one while the file is unchanged, else a
    /// fresh parse that replaces it.
    pub fn get(&mut self, path: &Path) -> reks_core::Result<Arc<Collection>> {
        // A stat failure falls through to `parse_file`, which reports it.
        let stamp = Stamp::of(path).ok();
        if let (Some(stamp), Some((cached, collection))) = (&stamp, &self.entry) {
            if stamp == cached {
                return Ok(Arc::clone(collection));
            }
        }
        self.entry = None;
        let collection = Arc::new(rbxml::parse_file(path)?);
        if let Some(stamp) = stamp {
            self.entry = Some((stamp, Arc::clone(&collection)));
        }
        Ok(collection)
    }

    /// Frees the parse (after an import, or when the wizard closes).
    pub fn clear(&mut self) {
        self.entry = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<DJ_PLAYLISTS Version="1.0.0"><PRODUCT Name="rekordbox" Version="6" Company="AlphaTheta"/>
<COLLECTION Entries="0"></COLLECTION><PLAYLISTS><NODE Type="0" Name="ROOT" Count="0"/></PLAYLISTS></DJ_PLAYLISTS>"#;

    #[test]
    fn reuses_the_parse_until_the_file_changes_or_is_cleared() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("collection.xml");
        std::fs::write(&path, XML).unwrap();
        let mut cache = XmlCache::default();

        let first = cache.get(&path).unwrap();
        assert!(Arc::ptr_eq(&first, &cache.get(&path).unwrap()));

        std::fs::write(&path, format!("{XML}\n<!-- edited -->")).unwrap();
        let second = cache.get(&path).unwrap();
        assert!(!Arc::ptr_eq(&first, &second));

        cache.clear();
        assert!(!Arc::ptr_eq(&second, &cache.get(&path).unwrap()));
    }

    #[test]
    fn a_missing_file_is_an_error_not_a_stale_hit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("collection.xml");
        std::fs::write(&path, XML).unwrap();
        let mut cache = XmlCache::default();
        cache.get(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert!(cache.get(&path).is_err());
    }
}
