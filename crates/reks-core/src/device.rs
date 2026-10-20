//! Stick profiles (D27, Phase 2 task 15): the storage side. A [`Device`] is a
//! USB stick keyed by the OS's own volume ID (D28), with a saved, live
//! selection of playlists and folders and a record of what the last
//! successful sync wrote ([`SyncedRecord`]).
//!
//! This module only stores rows. Identifying a mounted stick, turning a
//! profile into an export selection, per-playlist status and recording a sync
//! from an export report live in `reks_export_rb::devices`, next to the
//! export types they need. Spec: `docs/stick-profiles.md`.

use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::db::Library;
use crate::error::{Error, Result};
use crate::query::PlaylistId;
use crate::scan::TrackId;
use crate::timefmt::now_utc;

/// Row id of a [`Device`].
pub type DeviceId = i64;

/// A known stick.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub id: DeviceId,
    /// The OS's volume ID (D28): `VolumeUUID` on macOS, the FAT/exFAT serial
    /// on Windows and Linux. `None` when the OS gave none.
    pub volume_serial: Option<String>,
    /// Volume label when last seen (may be empty).
    pub label: String,
    /// Volume size when last seen.
    pub size_bytes: u64,
    /// User-given name.
    pub name: String,
    /// The export target, as `TargetProfile` serialises (`"Cdj3000"`).
    pub target: String,
    /// UTC ISO-8601.
    pub created_at: String,
    /// UTC ISO-8601; `None` before the first recorded sync.
    pub last_synced_at: Option<String>,
}

/// What [`Library::create_device`] needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewDevice {
    pub volume_serial: Option<String>,
    pub label: String,
    pub size_bytes: u64,
    pub name: String,
    pub target: String,
}

/// One playlist as a sync left it on the stick.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncedPlaylist {
    /// The library playlist it was written from; `None` once that playlist is
    /// deleted, or for a stick playlist reks can't place.
    pub playlist_id: Option<PlaylistId>,
    /// Folder names, then the playlist name, root first, as on the stick.
    pub path: Vec<String>,
    /// Library track ids in entry order; `None` for an entry that is no
    /// library track.
    pub entries: Vec<Option<TrackId>>,
}

/// A stored [`SyncedPlaylist`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncedRecord {
    #[serde(flatten)]
    pub playlist: SyncedPlaylist,
    /// [`entries_hash`] of the entries.
    pub content_hash: String,
    /// UTC ISO-8601.
    pub synced_at: String,
}

/// BLAKE3 (hex) of ordered playlist entries: the `content_hash` of
/// `device_synced_playlist`. Equal hashes mean the same tracks in the same
/// order.
pub fn entries_hash(entries: &[Option<TrackId>]) -> String {
    let mut h = blake3::Hasher::new();
    for e in entries {
        match e {
            Some(id) => {
                h.update(b"t");
                h.update(&id.to_le_bytes());
            }
            None => {
                h.update(b"?");
            }
        }
    }
    h.finalize().to_hex().to_string()
}

const COLUMNS: &str =
    "id, volume_serial, label, size_bytes, name, target, created_at, last_synced_at";

fn device_row(r: &Row<'_>) -> rusqlite::Result<Device> {
    let size: i64 = r.get(3)?;
    Ok(Device {
        id: r.get(0)?,
        volume_serial: r.get(1)?,
        label: r.get(2)?,
        size_bytes: u64::try_from(size).unwrap_or(0),
        name: r.get(4)?,
        target: r.get(5)?,
        created_at: r.get(6)?,
        last_synced_at: r.get(7)?,
    })
}

pub(crate) fn get_device(conn: &Connection, id: DeviceId) -> Result<Device> {
    conn.query_row(
        &format!("SELECT {COLUMNS} FROM device WHERE id = ?1"),
        [id],
        device_row,
    )
    .optional()?
    .ok_or(Error::DeviceNotFound(id))
}

fn by_serial(conn: &Connection, serial: &str) -> Result<Option<Device>> {
    Ok(conn
        .query_row(
            &format!("SELECT {COLUMNS} FROM device WHERE volume_serial = ?1"),
            [serial],
            device_row,
        )
        .optional()?)
}

/// `None` for a missing or blank serial.
fn clean_serial(serial: Option<&str>) -> Option<&str> {
    serial.map(str::trim).filter(|s| !s.is_empty())
}

fn clean_name(name: &str) -> Result<&str> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::InvalidField {
            field: "name",
            reason: "a stick needs a name".into(),
        });
    }
    Ok(name)
}

fn size_sql(size: u64) -> Result<i64> {
    i64::try_from(size).map_err(|_| Error::InvalidField {
        field: "size_bytes",
        reason: format!("{size} is too large"),
    })
}

/// Refuses `serial` when it already belongs to a device other than `own`.
fn check_serial_free(conn: &Connection, serial: &str, own: Option<DeviceId>) -> Result<()> {
    match by_serial(conn, serial)? {
        Some(other) if Some(other.id) != own => Err(Error::DeviceSerialInUse { name: other.name }),
        _ => Ok(()),
    }
}

impl Library {
    /// Every known stick, by name.
    pub fn devices(&self) -> Result<Vec<Device>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLUMNS} FROM device ORDER BY name COLLATE NOCASE, id"
        ))?;
        let rows = stmt
            .query_map([], device_row)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// One stick.
    pub fn device(&self, id: DeviceId) -> Result<Device> {
        get_device(&*self.conn()?, id)
    }

    /// The stick with this volume ID, if any.
    pub fn device_by_serial(&self, serial: &str) -> Result<Option<Device>> {
        by_serial(&*self.conn()?, serial)
    }

    /// Sticks last seen with this label and size (the relink candidates).
    pub fn devices_by_label_size(&self, label: &str, size_bytes: u64) -> Result<Vec<Device>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLUMNS} FROM device WHERE label = ?1 AND size_bytes = ?2 ORDER BY id"
        ))?;
        let rows = stmt
            .query_map(params![label, size_sql(size_bytes)?], device_row)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// Stores a new stick with an empty profile. A blank serial is stored as
    /// none; a serial another stick already has is refused.
    pub fn create_device(&self, new: &NewDevice) -> Result<Device> {
        let name = clean_name(&new.name)?;
        let serial = clean_serial(new.volume_serial.as_deref());
        let conn = self.conn()?;
        if let Some(s) = serial {
            check_serial_free(&conn, s, None)?;
        }
        conn.execute(
            "INSERT INTO device (volume_serial, label, size_bytes, name, target, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                serial,
                new.label,
                size_sql(new.size_bytes)?,
                name,
                new.target,
                now_utc()
            ],
        )?;
        get_device(&conn, conn.last_insert_rowid())
    }

    /// Links stick `id` to the volume now mounted: replaces the stored volume
    /// ID with `serial` (D28: a reformat, or a library moved to another OS)
    /// and refreshes label and size. With `serial` `None` (the OS gives none)
    /// the stored ID is kept.
    pub fn relink_device(
        &self,
        id: DeviceId,
        serial: Option<&str>,
        label: &str,
        size_bytes: u64,
    ) -> Result<Device> {
        let serial = clean_serial(serial);
        let conn = self.conn()?;
        get_device(&conn, id)?;
        if let Some(s) = serial {
            check_serial_free(&conn, s, Some(id))?;
        }
        conn.execute(
            "UPDATE device SET volume_serial = COALESCE(?2, volume_serial), label = ?3,
                               size_bytes = ?4
             WHERE id = ?1",
            params![id, serial, label, size_sql(size_bytes)?],
        )?;
        get_device(&conn, id)
    }

    /// Renames stick `id`.
    pub fn rename_device(&self, id: DeviceId, name: &str) -> Result<Device> {
        let name = clean_name(name)?;
        let conn = self.conn()?;
        get_device(&conn, id)?;
        conn.execute(
            "UPDATE device SET name = ?2 WHERE id = ?1",
            params![id, name],
        )?;
        get_device(&conn, id)
    }

    /// The profile of stick `id`: selected playlists and folders, by id.
    pub fn device_selection(&self, id: DeviceId) -> Result<Vec<PlaylistId>> {
        let conn = self.conn()?;
        get_device(&conn, id)?;
        let mut stmt = conn.prepare(
            "SELECT playlist_id FROM device_selection WHERE device_id = ?1 ORDER BY playlist_id",
        )?;
        let rows = stmt
            .query_map([id], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// Replaces the profile of stick `id` (duplicates ignored). An unknown
    /// playlist id is refused and leaves the old profile in place.
    pub fn set_device_selection(&self, id: DeviceId, playlists: &[PlaylistId]) -> Result<()> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        get_device(&tx, id)?;
        tx.execute("DELETE FROM device_selection WHERE device_id = ?1", [id])?;
        {
            let mut exists = tx.prepare("SELECT 1 FROM playlist WHERE id = ?1")?;
            let mut insert = tx.prepare(
                "INSERT OR IGNORE INTO device_selection (device_id, playlist_id) VALUES (?1, ?2)",
            )?;
            for &p in playlists {
                if !exists.exists([p])? {
                    return Err(Error::PlaylistNotFound(p));
                }
                insert.execute([id, p])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// What the last recorded sync wrote to stick `id`, in stick tree order.
    pub fn device_synced(&self, id: DeviceId) -> Result<Vec<SyncedRecord>> {
        let conn = self.conn()?;
        get_device(&conn, id)?;
        let mut stmt = conn.prepare(
            "SELECT playlist_id, path, entries, content_hash, synced_at
             FROM device_synced_playlist WHERE device_id = ?1 ORDER BY position",
        )?;
        let rows = stmt
            .query_map([id], |r| {
                Ok((
                    r.get::<_, Option<PlaylistId>>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(|(playlist_id, path, entries, content_hash, synced_at)| {
                Ok(SyncedRecord {
                    playlist: SyncedPlaylist {
                        playlist_id,
                        path: from_json("path", &path)?,
                        entries: from_json("entries", &entries)?,
                    },
                    content_hash,
                    synced_at,
                })
            })
            .collect()
    }

    /// Records a successful sync of stick `id`: replaces its synced
    /// playlists with `playlists` (in stick tree order) and sets
    /// `last_synced_at`, in one transaction. A `playlist_id` the library no
    /// longer has is stored as none. Returns the rows stored.
    pub fn record_device_sync(&self, id: DeviceId, playlists: &[SyncedPlaylist]) -> Result<usize> {
        let now = now_utc();
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        get_device(&tx, id)?;
        tx.execute(
            "DELETE FROM device_synced_playlist WHERE device_id = ?1",
            [id],
        )?;
        {
            let mut insert = tx.prepare(
                "INSERT INTO device_synced_playlist
                   (device_id, position, playlist_id, path, track_count, entries, content_hash,
                    synced_at)
                 VALUES (?1, ?2, (SELECT id FROM playlist WHERE id = ?3), ?4, ?5, ?6, ?7, ?8)",
            )?;
            for (position, p) in playlists.iter().enumerate() {
                insert.execute(params![
                    id,
                    position as i64,
                    p.playlist_id,
                    to_json(&p.path)?,
                    p.entries.len() as i64,
                    to_json(&p.entries)?,
                    entries_hash(&p.entries),
                    now
                ])?;
            }
        }
        tx.execute(
            "UPDATE device SET last_synced_at = ?2 WHERE id = ?1",
            params![id, now],
        )?;
        tx.commit()?;
        Ok(playlists.len())
    }
}

impl Library {
    /// Sets the export target of stick `id` (a `TargetProfile` name).
    pub fn set_device_target(&self, id: DeviceId, target: &str) -> Result<Device> {
        let conn = self.conn()?;
        get_device(&conn, id)?;
        conn.execute(
            "UPDATE device SET target = ?2 WHERE id = ?1",
            params![id, target],
        )?;
        get_device(&conn, id)
    }

    /// The entries of each of `playlists` in order, each with its track's
    /// file size: what profile status needs, without loading track details.
    /// Unknown ids and folders map to no entries.
    pub fn playlist_entry_sizes(
        &self,
        playlists: &[PlaylistId],
    ) -> Result<HashMap<PlaylistId, Vec<(TrackId, u64)>>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT pt.track_id, t.file_size FROM playlist_track pt
             JOIN track t ON t.id = pt.track_id
             WHERE pt.playlist_id = ?1 ORDER BY pt.position",
        )?;
        let mut out = HashMap::new();
        for &p in playlists {
            let rows = stmt
                .query_map([p], |r| {
                    let size: i64 = r.get(1)?;
                    Ok((r.get(0)?, u64::try_from(size).unwrap_or(0)))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            out.insert(p, rows);
        }
        Ok(out)
    }
}

fn to_json<T: Serialize>(v: &T) -> Result<String> {
    serde_json::to_string(v).map_err(|e| Error::InvalidField {
        field: "device_synced_playlist",
        reason: e.to_string(),
    })
}

fn from_json<T: for<'de> Deserialize<'de>>(field: &'static str, s: &str) -> Result<T> {
    serde_json::from_str(s).map_err(|e| Error::InvalidField {
        field,
        reason: format!("stored value {s:?} does not parse: {e}"),
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::{PlaylistOp, PlaylistOutcome};

    fn lib() -> Library {
        Library::open_in_memory(Path::new("cache-unused")).unwrap()
    }

    fn new_device(serial: Option<&str>, name: &str) -> NewDevice {
        NewDevice {
            volume_serial: serial.map(str::to_owned),
            label: "REKS".into(),
            size_bytes: 32 << 30,
            name: name.into(),
            target: "Cdj3000".into(),
        }
    }

    fn playlist(lib: &Library, name: &str) -> PlaylistId {
        match lib
            .playlist_mutate(PlaylistOp::CreatePlaylist {
                parent_id: None,
                name: name.into(),
                position: None,
            })
            .unwrap()
        {
            PlaylistOutcome::Done {
                created_id: Some(id),
                ..
            } => id,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn create_find_rename_and_relink() {
        let lib = lib();
        let a = lib
            .create_device(&new_device(Some("AAAA-1111"), " Main "))
            .unwrap();
        assert_eq!(a.name, "Main");
        assert_eq!(a.last_synced_at, None);
        assert_eq!(lib.device_by_serial("AAAA-1111").unwrap(), Some(a.clone()));
        assert_eq!(
            lib.devices_by_label_size("REKS", 32 << 30).unwrap(),
            vec![a.clone()]
        );
        assert!(lib
            .devices_by_label_size("REKS", 16 << 30)
            .unwrap()
            .is_empty());

        // A second stick can't take the same serial; a blank one is none.
        assert!(matches!(
            lib.create_device(&new_device(Some("AAAA-1111"), "Backup")),
            Err(Error::DeviceSerialInUse { name }) if name == "Main"
        ));
        let b = lib
            .create_device(&new_device(Some("  "), "Backup"))
            .unwrap();
        assert_eq!(b.volume_serial, None);
        assert!(lib.create_device(&new_device(None, " ")).is_err());

        assert_eq!(lib.rename_device(a.id, "Gig").unwrap().name, "Gig");
        let names: Vec<_> = lib.devices().unwrap().into_iter().map(|d| d.name).collect();
        assert_eq!(names, ["Backup", "Gig"]);

        // Relink replaces the ID; without one the stored ID stays.
        let a = lib
            .relink_device(a.id, Some("BBBB-2222"), "NEW", 16 << 30)
            .unwrap();
        assert_eq!(a.volume_serial.as_deref(), Some("BBBB-2222"));
        assert_eq!((a.label.as_str(), a.size_bytes), ("NEW", 16 << 30));
        assert_eq!(lib.device_by_serial("AAAA-1111").unwrap(), None);
        let a = lib.relink_device(a.id, None, "NEW", 16 << 30).unwrap();
        assert_eq!(a.volume_serial.as_deref(), Some("BBBB-2222"));
        assert!(matches!(
            lib.relink_device(b.id, Some("BBBB-2222"), "x", 1),
            Err(Error::DeviceSerialInUse { .. })
        ));
        assert!(matches!(lib.device(99), Err(Error::DeviceNotFound(99))));
    }

    #[test]
    fn selection_cascades_with_the_playlist() {
        let lib = lib();
        let d = lib.create_device(&new_device(Some("S"), "Main")).unwrap();
        let p1 = playlist(&lib, "One");
        let p2 = playlist(&lib, "Two");
        lib.set_device_selection(d.id, &[p2, p1, p2]).unwrap();
        assert_eq!(lib.device_selection(d.id).unwrap(), [p1, p2]);

        // An unknown id is refused and keeps the old profile.
        assert!(matches!(
            lib.set_device_selection(d.id, &[p1, 999]),
            Err(Error::PlaylistNotFound(999))
        ));
        assert_eq!(lib.device_selection(d.id).unwrap(), [p1, p2]);

        lib.playlist_mutate(PlaylistOp::Delete { id: p1 }).unwrap();
        assert_eq!(lib.device_selection(d.id).unwrap(), [p2]);
    }

    #[test]
    fn synced_rows_round_trip_and_survive_playlist_delete() {
        let lib = lib();
        let d = lib.create_device(&new_device(Some("S"), "Main")).unwrap();
        let p1 = playlist(&lib, "One");
        let rows = vec![
            SyncedPlaylist {
                playlist_id: Some(p1),
                path: vec!["Set".into(), "One/Two".into()],
                entries: vec![Some(3), None, Some(3)],
            },
            SyncedPlaylist {
                playlist_id: Some(12345), // gone already: stored as none
                path: vec!["Old".into()],
                entries: vec![],
            },
        ];
        assert_eq!(lib.record_device_sync(d.id, &rows).unwrap(), 2);
        assert!(lib.device(d.id).unwrap().last_synced_at.is_some());

        let got = lib.device_synced(d.id).unwrap();
        assert_eq!(got[0].playlist, rows[0]);
        assert_eq!(got[0].content_hash, entries_hash(&rows[0].entries));
        assert_eq!(got[1].playlist.playlist_id, None);

        lib.playlist_mutate(PlaylistOp::Delete { id: p1 }).unwrap();
        let got = lib.device_synced(d.id).unwrap();
        assert_eq!(got[0].playlist.playlist_id, None);
        assert_eq!(got[0].playlist.path, rows[0].path);

        // A second record replaces the first.
        lib.record_device_sync(d.id, &rows[1..]).unwrap();
        assert_eq!(lib.device_synced(d.id).unwrap().len(), 1);
    }

    #[test]
    fn entries_hash_depends_on_order_and_unknowns() {
        let a = entries_hash(&[Some(1), Some(2)]);
        assert_eq!(a, entries_hash(&[Some(1), Some(2)]));
        assert_ne!(a, entries_hash(&[Some(2), Some(1)]));
        assert_ne!(a, entries_hash(&[Some(1), Some(2), None]));
        assert_ne!(entries_hash(&[]), entries_hash(&[None]));
    }
}
