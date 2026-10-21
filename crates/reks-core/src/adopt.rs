//! The library side of adopting a foreign stick (D27, Phase 2.5 task 12;
//! `docs/stick-profiles.md` §4.3).
//!
//! `reks_export_rb::adopt` reads the stick and decides per playlist; this
//! module writes the outcome in one transaction: created playlists go under
//! `From stick/<stick name>/`, keeping the stick's own folders below that;
//! linked playlists must still hold exactly the tracks the plan saw; every
//! created and linked playlist is ticked in the stick's profile. Nothing here
//! touches a stick.

use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::db::Library;
use crate::device::{get_device, DeviceId};
use crate::error::{Error, Result};
use crate::playlist::{create, write_entries, Entry, PlaylistOutcome};
use crate::query::{playlist_track_ids, PlaylistId};
use crate::rule::PlaylistIndex;
use crate::scan::TrackId;
use crate::timefmt::now_utc;

/// Top-level folder that holds adopted playlists.
pub const FROM_STICK_FOLDER: &str = "From stick";

/// What [`Library::adopt_playlists`] writes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AdoptWrite {
    pub device_id: DeviceId,
    /// The folder under [`FROM_STICK_FOLDER`]; ` (2)`, ... is added when a
    /// folder of that name is already there.
    pub stick_name: String,
    /// In stick tree order.
    pub creates: Vec<AdoptCreate>,
    pub links: Vec<AdoptLink>,
}

/// A new playlist from a stick playlist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdoptCreate {
    /// The stick folders above it, root first.
    pub folders: Vec<String>,
    pub name: String,
    /// In stick order, duplicates kept.
    pub tracks: Vec<TrackId>,
}

/// An existing playlist that already equals a stick playlist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdoptLink {
    pub playlist_id: PlaylistId,
    /// Its track entries (dividers left out) as the plan saw them.
    pub tracks: Vec<TrackId>,
}

/// What [`Library::adopt_playlists`] did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdoptWritten {
    /// `From stick/<name>`; `None` when nothing was created.
    pub folder_id: Option<PlaylistId>,
    /// In the order of [`AdoptWrite::creates`].
    pub created: Vec<PlaylistId>,
    pub linked: Vec<PlaylistId>,
}

impl Library {
    /// Creates and links the adopted playlists and adds them to the stick's
    /// profile, in one transaction. A link whose playlist changed since the
    /// plan fails with [`Error::StaleAdoptPlan`] and writes nothing.
    pub fn adopt_playlists(&self, w: &AdoptWrite) -> Result<AdoptWritten> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        let now = now_utc();
        get_device(&tx, w.device_id)?;
        let mut out = AdoptWritten::default();

        let index = PlaylistIndex::load(&tx)?;
        for l in &w.links {
            // A smart list's contents change by themselves: never linked.
            if index.smart(l.playlist_id).is_some() {
                return Err(Error::StaleAdoptPlan(format!(
                    "playlist {} is a smart playlist",
                    l.playlist_id
                )));
            }
            let found = playlist_track_ids(&tx, &index, l.playlist_id)?;
            if found != l.tracks {
                return Err(Error::StaleAdoptPlan(format!(
                    "playlist {} changed since the scan",
                    l.playlist_id
                )));
            }
            out.linked.push(l.playlist_id);
        }

        if !w.creates.is_empty() {
            let top = match top_folder(&tx)? {
                Some(id) => id,
                None => new_node(&tx, None, FROM_STICK_FOLDER, true, &now)?,
            };
            let name = free_name(&tx, top, &w.stick_name)?;
            let root = new_node(&tx, Some(top), &name, true, &now)?;
            out.folder_id = Some(root);
            let mut folders: HashMap<Vec<String>, PlaylistId> = HashMap::new();
            for c in &w.creates {
                let mut parent = root;
                for depth in 1..=c.folders.len() {
                    let key = c.folders[..depth].to_vec();
                    parent = match folders.get(&key) {
                        Some(&id) => id,
                        None => {
                            let id =
                                new_node(&tx, Some(parent), &c.folders[depth - 1], true, &now)?;
                            folders.insert(key, id);
                            id
                        }
                    };
                }
                let id = new_node(&tx, Some(parent), &c.name, false, &now)?;
                let mut exists = tx.prepare_cached("SELECT 1 FROM track WHERE id = ?1")?;
                for &t in &c.tracks {
                    if !exists.exists([t])? {
                        return Err(Error::TrackNotFound(t));
                    }
                }
                let new: Vec<Entry> = c.tracks.iter().map(|&t| Entry::Track(t)).collect();
                write_entries(&tx, id, &[], &new, &now)?;
                out.created.push(id);
            }
        }

        {
            let mut tick = tx.prepare(
                "INSERT OR IGNORE INTO device_selection (device_id, playlist_id) VALUES (?1, ?2)",
            )?;
            for &p in out.created.iter().chain(&out.linked) {
                tick.execute(params![w.device_id, p])?;
            }
        }
        tx.commit()?;
        Ok(out)
    }
}

fn top_folder(c: &Connection) -> Result<Option<PlaylistId>> {
    Ok(c.query_row(
        "SELECT id FROM playlist WHERE parent_id IS NULL AND is_folder = 1 AND name = ?1
             ORDER BY sort_order, id LIMIT 1",
        [FROM_STICK_FOLDER],
        |r| r.get(0),
    )
    .optional()?)
}

/// `name`, or `name (2)`, `name (3)`, ... if a child of `parent` has it.
fn free_name(c: &Connection, parent: PlaylistId, name: &str) -> Result<String> {
    let mut stmt = c.prepare("SELECT 1 FROM playlist WHERE parent_id = ?1 AND name = ?2")?;
    let base = name.trim();
    let mut candidate = base.to_owned();
    let mut n = 2;
    while stmt.exists(params![parent, candidate])? {
        candidate = format!("{base} ({n})");
        n += 1;
    }
    Ok(candidate)
}

fn new_node(
    c: &Connection,
    parent: Option<PlaylistId>,
    name: &str,
    is_folder: bool,
    now: &str,
) -> Result<PlaylistId> {
    match create(c, parent, name, None, is_folder, now)? {
        PlaylistOutcome::Done {
            created_id: Some(id),
            ..
        } => Ok(id),
        _ => Err(Error::InvalidField {
            field: "name",
            reason: format!("could not create {name:?}"),
        }),
    }
}

impl Library {
    /// The tracks of each of `ids` in order (dividers left out; a smart
    /// list's current tracks), for adopt's link check. Folders and unknown
    /// ids map to an empty list.
    pub fn playlist_track_ids(
        &self,
        ids: &[PlaylistId],
    ) -> Result<HashMap<PlaylistId, Vec<TrackId>>> {
        let conn = self.conn()?;
        let index = PlaylistIndex::load(&conn)?;
        ids.iter()
            .map(|&id| Ok((id, playlist_track_ids(&conn, &index, id)?)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::device::NewDevice;
    use crate::PlaylistOp;

    fn lib_with_tracks(n: usize) -> (Library, Vec<TrackId>) {
        let lib = Library::open_in_memory(Path::new("cache-unused")).unwrap();
        let ids = {
            let conn = lib.conn().unwrap();
            (0..n)
                .map(|i| {
                    conn.execute(
                        "INSERT INTO track (path, file_size, file_mtime, kind, title,
                                            date_added, created_at, updated_at)
                         VALUES (?1, 1, '2026-01-01T00:00:00Z', 'mp3', ?2,
                                 '2026-01-01', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                        params![format!("/m/{i}.mp3"), format!("T{i}")],
                    )
                    .unwrap();
                    conn.last_insert_rowid()
                })
                .collect()
        };
        (lib, ids)
    }

    fn device(lib: &Library) -> DeviceId {
        lib.create_device(&NewDevice {
            volume_serial: Some("S".into()),
            label: "STICK".into(),
            size_bytes: 1,
            name: "Gig stick".into(),
            target: "Generic".into(),
        })
        .unwrap()
        .id
    }

    fn path_of(lib: &Library, id: PlaylistId) -> Vec<String> {
        let nodes = lib.playlists().unwrap().nodes;
        let mut out = Vec::new();
        let mut cur = Some(id);
        while let Some(c) = cur {
            let n = nodes.iter().find(|n| n.id == c).unwrap();
            out.push(n.name.clone());
            cur = n.parent_id;
        }
        out.reverse();
        out
    }

    #[test]
    fn creates_under_from_stick_and_ticks_the_profile() {
        let (lib, t) = lib_with_tracks(3);
        let dev = device(&lib);
        let w = AdoptWrite {
            device_id: dev,
            stick_name: "Gig stick".into(),
            creates: vec![
                AdoptCreate {
                    folders: vec!["Sets".into()],
                    name: "Peak".into(),
                    tracks: vec![t[2], t[0], t[2]],
                },
                AdoptCreate {
                    folders: vec!["Sets".into()],
                    name: "Warmup".into(),
                    tracks: vec![],
                },
            ],
            links: vec![],
        };
        let out = lib.adopt_playlists(&w).unwrap();
        assert_eq!(out.created.len(), 2);
        assert_eq!(
            path_of(&lib, out.created[0]),
            ["From stick", "Gig stick", "Sets", "Peak"]
        );
        assert_eq!(
            path_of(&lib, out.created[1]),
            ["From stick", "Gig stick", "Sets", "Warmup"]
        );
        let tracks = lib.playlist_track_ids(&out.created).unwrap();
        assert_eq!(tracks[&out.created[0]], vec![t[2], t[0], t[2]]);
        let mut sel = lib.device_selection(dev).unwrap();
        sel.sort_unstable();
        assert_eq!(sel, out.created);

        // A second adopt reuses `From stick` and numbers the stick folder.
        let again = lib.adopt_playlists(&w).unwrap();
        assert_eq!(
            path_of(&lib, again.created[0]),
            ["From stick", "Gig stick (2)", "Sets", "Peak"]
        );
        let tops = lib
            .playlists()
            .unwrap()
            .nodes
            .iter()
            .filter(|n| n.parent_id.is_none())
            .count();
        assert_eq!(tops, 1);
    }

    #[test]
    fn stale_link_writes_nothing() {
        let (lib, t) = lib_with_tracks(2);
        let dev = device(&lib);
        let id = match lib
            .playlist_mutate(PlaylistOp::CreatePlaylist {
                parent_id: None,
                name: "Peak".into(),
                position: None,
            })
            .unwrap()
        {
            PlaylistOutcome::Done { created_id, .. } => created_id.unwrap(),
            other => panic!("{other:?}"),
        };
        lib.playlist_mutate(PlaylistOp::AddTracks {
            playlist_id: id,
            track_ids: vec![t[0], t[1]],
            before: None,
            duplicates: Default::default(),
        })
        .unwrap();
        let w = AdoptWrite {
            device_id: dev,
            stick_name: "Gig stick".into(),
            creates: vec![AdoptCreate {
                folders: vec![],
                name: "Other".into(),
                tracks: vec![t[0]],
            }],
            links: vec![AdoptLink {
                playlist_id: id,
                tracks: vec![t[1], t[0]],
            }],
        };
        assert!(matches!(
            lib.adopt_playlists(&w),
            Err(Error::StaleAdoptPlan(_))
        ));
        assert_eq!(lib.playlists().unwrap().nodes.len(), 1);
        assert!(lib.device_selection(dev).unwrap().is_empty());

        let ok = AdoptWrite {
            links: vec![AdoptLink {
                playlist_id: id,
                tracks: vec![t[0], t[1]],
            }],
            ..w
        };
        let out = lib.adopt_playlists(&ok).unwrap();
        assert_eq!(out.linked, vec![id]);
        assert_eq!(lib.device_selection(dev).unwrap().len(), 2);
    }

    /// A smart list is never linked, even when it holds exactly the tracks.
    #[test]
    fn smart_list_is_never_linked() {
        let (lib, t) = lib_with_tracks(2);
        let dev = device(&lib);
        let id = match lib
            .playlist_mutate(PlaylistOp::CreateSmart {
                parent_id: None,
                name: "Everything".into(),
                rule: crate::RuleNode::Group {
                    r#match: crate::Match::All,
                    not: false,
                    rules: vec![],
                },
                sort: None,
                position: None,
            })
            .unwrap()
        {
            PlaylistOutcome::Done { created_id, .. } => created_id.unwrap(),
            other => panic!("{other:?}"),
        };
        let tracks = lib.playlist_track_ids(&[id]).unwrap()[&id].clone();
        assert_eq!(tracks.len(), t.len());
        let w = AdoptWrite {
            device_id: dev,
            stick_name: "Gig stick".into(),
            creates: vec![],
            links: vec![AdoptLink {
                playlist_id: id,
                tracks,
            }],
        };
        assert!(matches!(
            lib.adopt_playlists(&w),
            Err(Error::StaleAdoptPlan(_))
        ));
        assert!(lib.device_selection(dev).unwrap().is_empty());
    }
}
