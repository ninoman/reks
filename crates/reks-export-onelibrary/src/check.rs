//! The OneLibrary side of checking a stick
//! ([`reks_export_rb::check`]): [`CheckReader`] reads an `exportLibrary.db`
//! into what the check compares with `export.pdb`. Like every read here, it
//! opens a temp copy, so the stick is never written.

use std::collections::HashMap;
use std::path::Path;

use reks_core::OneLibraryKey;
use reks_export_rb::check::{
    OneLibraryContents, OneLibraryPlaylist, OneLibraryRead, OneLibraryTrack,
};

use crate::reader::OneLibraryReader;
use crate::rows::ATTRIBUTE_FOLDER;

/// Reads with `key`; without one, every database is "not read".
#[derive(Debug)]
pub struct CheckReader {
    pub key: Option<OneLibraryKey>,
}

impl OneLibraryRead for CheckReader {
    fn read(&self, db: &Path) -> Result<OneLibraryContents, String> {
        let key = self
            .key
            .as_ref()
            .ok_or("this build has no OneLibrary key")?;
        read(db, key.expose()).map_err(|e| e.to_string())
    }
}

fn read(db: &Path, key: &str) -> crate::Result<OneLibraryContents> {
    let reader = OneLibraryReader::open(db, key)?;
    let integrity = reader.integrity_check()?;
    let lib = reader.library()?;
    let artists: HashMap<i64, &str> = lib
        .artists
        .iter()
        .map(|a| (a.artist_id, a.name.as_str()))
        .collect();
    let mut entries: HashMap<i64, Vec<(i64, i64)>> = HashMap::new();
    for e in &lib.playlist_entries {
        entries
            .entry(e.playlist_id)
            .or_default()
            .push((e.sequence_no, e.content_id));
    }
    let tracks = lib
        .contents
        .iter()
        .map(|c| OneLibraryTrack {
            id: c.content_id,
            title: c.title.clone(),
            artist: c
                .artist_id_artist
                .and_then(|id| artists.get(&id))
                .map(|&name| name.to_owned()),
            path: c.path.clone(),
            analysis_path: c.analysis_data_file_path.clone(),
        })
        .collect();
    let playlists = lib
        .playlists
        .iter()
        .map(|p| {
            let mut list = entries.remove(&p.playlist_id).unwrap_or_default();
            list.sort_unstable();
            OneLibraryPlaylist {
                id: p.playlist_id,
                parent_id: p.parent_id,
                sequence_no: p.sequence_no,
                name: p.name.clone(),
                is_folder: p.attribute == ATTRIBUTE_FOLDER,
                track_ids: list.into_iter().map(|(_, id)| id).collect(),
            }
        })
        .collect();
    Ok(OneLibraryContents {
        tracks,
        playlists,
        sessions: reader.play_history()?,
        integrity,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use reks_export_rb::check::{check_stick, CheckOptions, FindingKind, Section};
    use reks_export_rb::device::{DeviceInfo, FsKind};
    use reks_export_rb::onelibrary::DB_PATH;
    use reks_export_rb::TargetProfile;

    use super::*;
    use crate::testutil::{reference_root, reference_stick, TEST_KEY};
    use crate::writer::{build_file, OneLibraryWriter};
    use crate::RbVersion;

    /// The reference stick with an `exportLibrary.db` reks built from its
    /// `export.pdb` after `edit`, and the fixture audio it lists.
    fn stick(edit: impl FnOnce(&mut crate::rows::OneLibrary)) -> tempfile::TempDir {
        let stick = reference_stick();
        let mut rows = OneLibraryWriter::default().rows(stick.path()).unwrap();
        edit(&mut rows);
        let db = stick.path().join(DB_PATH.trim_start_matches('/'));
        build_file(&db, TEST_KEY, RbVersion::LATEST, &rows).unwrap();
        let audio = reference_root().join("../../audio");
        for c in &rows.contents {
            let name = c.path.rsplit('/').next().unwrap();
            let to = stick.path().join(c.path.trim_start_matches('/'));
            std::fs::create_dir_all(to.parent().unwrap()).unwrap();
            std::fs::copy(audio.join(name), to).unwrap();
        }
        stick
    }

    fn check(root: &Path, key: Option<&str>) -> reks_export_rb::check::StickReport {
        let dev = DeviceInfo {
            mount_point: PathBuf::from(root),
            fs: FsKind::Fat32,
            fs_name: "msdos".into(),
            removable: true,
            total_bytes: 32 << 30,
            free_bytes: 16 << 30,
            label: "REKS".into(),
            os_name: "disk9s1".into(),
        };
        let reader = CheckReader {
            key: key.and_then(OneLibraryKey::new),
        };
        let opts = CheckOptions {
            target: Some(TargetProfile::Cdj3000),
            volume_serial: None,
            directory: false,
        };
        check_stick(&dev, &opts, &reader, None)
    }

    fn kinds(r: &reks_export_rb::check::StickReport) -> Vec<FindingKind> {
        r.sections
            .iter()
            .flat_map(|s| &s.groups)
            .map(|g| g.kind)
            .collect()
    }

    #[test]
    fn both_libraries_from_one_export_agree() {
        let stick = stick(|_| {});
        let r = check(stick.path(), Some(TEST_KEY));
        assert_eq!(r.formats[2].status, "7 tracks in 1 playlist");
        assert_eq!(r.tracks_checked, 7);
        let k = kinds(&r);
        for kind in [
            FindingKind::OneLibraryNotRead,
            FindingKind::OneLibraryDamaged,
            FindingKind::TrackInOneLibrary,
            FindingKind::PlaylistInOneLibrary,
            FindingKind::PlaylistDiffers,
        ] {
            assert!(!k.contains(&kind), "{kind:?} in {k:?}");
        }
        assert!(!r.sections.iter().any(|s| s.section == Section::Files));
    }

    #[test]
    fn a_reordered_playlist_and_a_missing_key() {
        let stick = stick(|rows| {
            let first = rows
                .playlist_entries
                .iter()
                .map(|e| e.sequence_no)
                .min()
                .unwrap();
            let last = rows
                .playlist_entries
                .iter()
                .map(|e| e.sequence_no)
                .max()
                .unwrap();
            for e in &mut rows.playlist_entries {
                e.sequence_no = first + last - e.sequence_no;
            }
        });
        let r = check(stick.path(), Some(TEST_KEY));
        let differs = r
            .sections
            .iter()
            .flat_map(|s| &s.groups)
            .find(|g| g.kind == FindingKind::PlaylistDiffers)
            .unwrap();
        assert_eq!(
            differs.items[0].detail.as_deref(),
            Some("Same tracks in another order")
        );

        let r = check(stick.path(), None);
        assert!(kinds(&r).contains(&FindingKind::OneLibraryNotRead));
        assert_eq!(
            r.formats[2].status,
            "Not read: this build has no OneLibrary key"
        );
        let r = check(stick.path(), Some("wrong key"));
        assert!(kinds(&r).contains(&FindingKind::OneLibraryNotRead));
    }
}
