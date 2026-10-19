//! The export manifest (D23, Phase 2 task 13): what reks last wrote to each
//! stick, one row per file, keyed by (volume serial, stick path).
//!
//! The export fills it from the validator's read-back after a successful
//! export, replacing that stick's previous rows. Phase 2.5 matches player
//! history entries through it, so PDB track ids need not stay stable across
//! exports. Deleting a library track keeps its row with `track_id` NULL.

use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::db::Library;
use crate::error::{Error, Result};
use crate::scan::TrackId;
use crate::timefmt::now_utc;

/// What a stick file is to reks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ManifestItemKind {
    /// A library track's audio file.
    #[default]
    Track,
    /// A generated playlist divider (Phase 2.5, D22).
    Divider,
}

impl ManifestItemKind {
    fn as_sql(self) -> &'static str {
        match self {
            Self::Track => "track",
            Self::Divider => "divider",
        }
    }

    fn from_sql(s: &str) -> Option<Self> {
        match s {
            "track" => Some(Self::Track),
            "divider" => Some(Self::Divider),
            _ => None,
        }
    }
}

/// One file of an export, as the caller hands it in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestRow {
    /// `/Contents/...`, exactly as in the PDB track row.
    pub stick_path: String,
    pub item_kind: ManifestItemKind,
    /// The library track; `None` for dividers.
    pub track_id: Option<TrackId>,
}

/// A stored manifest row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestEntry {
    pub volume_serial: String,
    pub stick_path: String,
    pub item_kind: ManifestItemKind,
    /// `None` once the library track was deleted (or for dividers).
    pub track_id: Option<TrackId>,
    /// UTC ISO-8601.
    pub exported_at: String,
}

const COLUMNS: &str = "volume_serial, stick_path, item_kind, track_id, exported_at";

fn entry(r: &Row<'_>) -> rusqlite::Result<ManifestEntry> {
    let kind: String = r.get(2)?;
    let item_kind = ManifestItemKind::from_sql(&kind).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            2,
            rusqlite::types::Type::Text,
            format!("unknown export_manifest item_kind {kind:?}").into(),
        )
    })?;
    Ok(ManifestEntry {
        volume_serial: r.get(0)?,
        stick_path: r.get(1)?,
        item_kind,
        track_id: r.get(3)?,
        exported_at: r.get(4)?,
    })
}

impl Library {
    /// Replaces the manifest of the stick `volume_serial` with `rows`, in one
    /// transaction, and returns the number of rows stored.
    ///
    /// Without a serial (the OS did not give one) nothing is stored and this
    /// returns `Ok(0)`: the export still succeeds, only its history can't be
    /// matched later. Duplicate stick paths or unknown track ids are rejected
    /// and leave the previous rows in place.
    pub fn replace_manifest(
        &self,
        volume_serial: Option<&str>,
        rows: &[ManifestRow],
    ) -> Result<usize> {
        let Some(serial) = volume_serial.filter(|s| !s.is_empty()) else {
            tracing::warn!(
                rows = rows.len(),
                "no volume serial; export manifest not stored"
            );
            return Ok(0);
        };
        if let Some(bad) = rows.iter().find(|r| !r.stick_path.starts_with('/')) {
            return Err(Error::InvalidField {
                field: "stick_path",
                reason: format!("{:?} is not an absolute stick path", bad.stick_path),
            });
        }
        let exported_at = now_utc();
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM export_manifest WHERE volume_serial = ?1",
            [serial],
        )?;
        {
            let mut insert = tx.prepare(
                "INSERT INTO export_manifest (volume_serial, stick_path, item_kind, track_id, exported_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for row in rows {
                insert.execute(params![
                    serial,
                    row.stick_path,
                    row.item_kind.as_sql(),
                    row.track_id,
                    exported_at
                ])?;
            }
        }
        tx.commit()?;
        Ok(rows.len())
    }

    /// The manifest of one stick, ordered by stick path.
    pub fn manifest(&self, volume_serial: &str) -> Result<Vec<ManifestEntry>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLUMNS} FROM export_manifest WHERE volume_serial = ?1 ORDER BY stick_path"
        ))?;
        let rows = stmt
            .query_map([volume_serial], entry)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// The manifest row for one file on one stick (Phase 2.5 history matching).
    pub fn manifest_entry(
        &self,
        volume_serial: &str,
        stick_path: &str,
    ) -> Result<Option<ManifestEntry>> {
        let conn = self.conn()?;
        Ok(conn
            .query_row(
                &format!(
                    "SELECT {COLUMNS} FROM export_manifest
                     WHERE volume_serial = ?1 AND stick_path = ?2"
                ),
                [volume_serial, stick_path],
                entry,
            )
            .optional()?)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn lib() -> Library {
        Library::open_in_memory(Path::new("cache-unused")).unwrap()
    }

    fn insert_track(lib: &Library, path: &str) -> TrackId {
        let c = lib.conn().unwrap();
        c.execute(
            "INSERT INTO track (path, file_size, file_mtime, kind, title, artist,
                                date_added, created_at, updated_at)
             VALUES (?1, 1, '2026-01-01T00:00:00Z', 'wav', 'T', 'A',
                     '2026-01-01', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [path],
        )
        .unwrap();
        c.last_insert_rowid()
    }

    fn row(path: &str, track_id: Option<TrackId>) -> ManifestRow {
        ManifestRow {
            stick_path: path.into(),
            item_kind: ManifestItemKind::Track,
            track_id,
        }
    }

    fn paths(entries: &[ManifestEntry]) -> Vec<&str> {
        entries.iter().map(|e| e.stick_path.as_str()).collect()
    }

    #[test]
    fn re_export_replaces_only_that_serials_rows() {
        let lib = lib();
        let a = insert_track(&lib, "/m/a.wav");
        let b = insert_track(&lib, "/m/b.wav");
        let c = insert_track(&lib, "/m/c.wav");

        let first = [
            row("/Contents/A/a.wav", Some(a)),
            row("/Contents/B/b.wav", Some(b)),
        ];
        assert_eq!(lib.replace_manifest(Some("4993-1CFB"), &first).unwrap(), 2);
        lib.replace_manifest(Some("OTHER-0001"), &[row("/Contents/A/a.wav", Some(a))])
            .unwrap();

        let second = [
            row("/Contents/B/b.wav", Some(b)),
            row("/Contents/C/c.wav", Some(c)),
        ];
        assert_eq!(lib.replace_manifest(Some("4993-1CFB"), &second).unwrap(), 2);

        let stick = lib.manifest("4993-1CFB").unwrap();
        assert_eq!(paths(&stick), ["/Contents/B/b.wav", "/Contents/C/c.wav"]);
        assert!(stick.iter().all(|e| e.exported_at.ends_with('Z')));
        assert_eq!(
            paths(&lib.manifest("OTHER-0001").unwrap()),
            ["/Contents/A/a.wav"]
        );
    }

    #[test]
    fn deleting_a_track_keeps_its_row_with_null_track_id() {
        let lib = lib();
        let a = insert_track(&lib, "/m/a.wav");
        lib.replace_manifest(Some("S"), &[row("/Contents/A/a.wav", Some(a))])
            .unwrap();
        lib.conn()
            .unwrap()
            .execute("DELETE FROM track WHERE id = ?1", [a])
            .unwrap();

        let e = lib
            .manifest_entry("S", "/Contents/A/a.wav")
            .unwrap()
            .unwrap();
        assert_eq!(e.track_id, None);
        assert_eq!(e.item_kind, ManifestItemKind::Track);
    }

    #[test]
    fn no_serial_stores_nothing_and_succeeds() {
        let lib = lib();
        let a = insert_track(&lib, "/m/a.wav");
        let rows = [row("/Contents/A/a.wav", Some(a))];
        assert_eq!(lib.replace_manifest(None, &rows).unwrap(), 0);
        assert_eq!(lib.replace_manifest(Some(""), &rows).unwrap(), 0);
        let n: i64 = lib
            .conn()
            .unwrap()
            .query_row("SELECT count(*) FROM export_manifest", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn rejected_rows_keep_the_previous_manifest() {
        let lib = lib();
        let a = insert_track(&lib, "/m/a.wav");
        lib.replace_manifest(Some("S"), &[row("/Contents/A/a.wav", Some(a))])
            .unwrap();

        let dup = [
            row("/Contents/X.wav", Some(a)),
            row("/Contents/X.wav", Some(a)),
        ];
        assert!(lib.replace_manifest(Some("S"), &dup).is_err());
        let unknown_track = [row("/Contents/Y.wav", Some(a + 100))];
        assert!(lib.replace_manifest(Some("S"), &unknown_track).is_err());
        assert!(matches!(
            lib.replace_manifest(Some("S"), &[row("Contents/Z.wav", None)]),
            Err(Error::InvalidField {
                field: "stick_path",
                ..
            })
        ));

        assert_eq!(paths(&lib.manifest("S").unwrap()), ["/Contents/A/a.wav"]);
    }

    #[test]
    fn dividers_round_trip_without_a_track() {
        let lib = lib();
        let divider = ManifestRow {
            stick_path: "/Contents/reks/divider-1.wav".into(),
            item_kind: ManifestItemKind::Divider,
            track_id: None,
        };
        lib.replace_manifest(Some("S"), std::slice::from_ref(&divider))
            .unwrap();
        let e = lib
            .manifest_entry("S", &divider.stick_path)
            .unwrap()
            .unwrap();
        assert_eq!(e.item_kind, ManifestItemKind::Divider);
        assert_eq!(e.track_id, None);
        assert!(lib
            .manifest_entry("S", "/Contents/none.wav")
            .unwrap()
            .is_none());
    }
}
