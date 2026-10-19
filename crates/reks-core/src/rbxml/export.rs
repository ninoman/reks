//! Export a library selection as rekordbox XML: the inverse of [`import`].
//!
//! Used as the input for the legacy USB writer (baken, D7; Phase 2 task 6):
//! the file holds only the selected playlists, their folders and the tracks
//! they reference, and [`XmlExport::baken_playlists`] gives the matching
//! `Options.playlists`. `TrackID` is our track id.
//!
//! [`import`]: super::import()

use std::collections::{HashMap, HashSet};
use std::path::Path;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use super::colour::colour_rgb;
use super::{path_to_location, Collection, PositionMark, Product, Tempo, XmlNode, XmlTrack};
use crate::db::Library;
use crate::error::{Error, Result};
use crate::query::PlaylistId;
use crate::scan::TrackId;

/// What to export: playlists and folders by id. A folder stands for every
/// playlist under it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Selection {
    pub playlists: Vec<PlaylistId>,
}

/// An exported selection.
#[derive(Debug, Clone, PartialEq)]
pub struct XmlExport {
    pub collection: Collection,
    /// Exported playlists in tree order, each with its `Folder/Name` path
    /// under `ROOT` (baken's `Playlist::path`).
    pub playlists: Vec<(PlaylistId, String)>,
}

impl XmlExport {
    /// baken's `Options.playlists` for this file.
    ///
    /// baken finds a playlist by its first exact path match, so two exported
    /// playlists with the same path (same-named siblings, or a name with `/`
    /// that reads like a folder) would export one of them twice. Then this
    /// returns an empty list, which baken reads as "every playlist in the
    /// file": the same set, since the file holds only the selection.
    pub fn baken_playlists(&self) -> Vec<String> {
        let mut seen = HashSet::new();
        if self.playlists.iter().all(|(_, p)| seen.insert(p.as_str())) {
            self.playlists.iter().map(|(_, p)| p.clone()).collect()
        } else {
            Vec::new()
        }
    }
}

/// Builds the collection for `sel`.
///
/// Errors: an id that is not in the library ([`Error::PlaylistNotFound`]),
/// or a selection with no playlist in it ([`Error::EmptySelection`]).
pub fn export(lib: &Library, sel: &Selection) -> Result<XmlExport> {
    let conn = lib.conn()?;
    let nodes = load_nodes(&conn)?;

    // Selected playlists (folders expanded), then everything to keep in the
    // tree: those playlists and their ancestors.
    let mut selected = HashSet::new();
    for &id in &sel.playlists {
        if !nodes.by_id.contains_key(&id) {
            return Err(Error::PlaylistNotFound(id));
        }
        nodes.collect_playlists(id, &mut selected);
    }
    if selected.is_empty() {
        return Err(Error::EmptySelection);
    }
    let mut keep = HashSet::new();
    for &id in &selected {
        let mut cur = Some(id);
        while let Some(c) = cur {
            if !keep.insert(c) {
                break;
            }
            cur = nodes.by_id.get(&c).and_then(|n| n.parent);
        }
    }

    let mut b = Builder {
        conn: &conn,
        nodes: &nodes,
        keep: &keep,
        playlists: Vec::new(),
        tracks: Vec::new(),
        seen: HashSet::new(),
    };
    let children = b.children(None, "")?;
    let tracks = b
        .tracks
        .iter()
        .map(|&id| xml_track(&conn, id))
        .collect::<Result<_>>()?;
    Ok(XmlExport {
        collection: Collection {
            product: Product {
                name: Some("reks".to_owned()),
                version: Some(crate::VERSION.to_owned()),
                company: Some("reks".to_owned()),
                ..Product::default()
            },
            tracks,
            playlists: XmlNode::folder("ROOT", children),
            ..Collection::default()
        },
        playlists: b.playlists,
    })
}

/// [`export`], written to `path` with [`super::write`].
pub fn export_file(lib: &Library, sel: &Selection, path: &Path) -> Result<XmlExport> {
    let out = export(lib, sel)?;
    let file = std::fs::File::create(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    super::write(&out.collection, file)?;
    Ok(out)
}

struct Node {
    parent: Option<PlaylistId>,
    is_folder: bool,
    name: String,
}

struct Nodes {
    by_id: HashMap<PlaylistId, Node>,
    /// Children in display order; key `None` = top level.
    children: HashMap<Option<PlaylistId>, Vec<PlaylistId>>,
}

impl Nodes {
    fn collect_playlists(&self, id: PlaylistId, out: &mut HashSet<PlaylistId>) {
        match self.by_id.get(&id) {
            Some(n) if !n.is_folder => {
                out.insert(id);
            }
            Some(_) => {
                for &c in self.children.get(&Some(id)).into_iter().flatten() {
                    self.collect_playlists(c, out);
                }
            }
            None => {}
        }
    }
}

fn load_nodes(conn: &Connection) -> Result<Nodes> {
    let mut stmt = conn
        .prepare("SELECT id, parent_id, is_folder, name FROM playlist ORDER BY sort_order, id")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, PlaylistId>(0)?,
            Node {
                parent: r.get(1)?,
                is_folder: r.get(2)?,
                name: r.get(3)?,
            },
        ))
    })?;
    let mut nodes = Nodes {
        by_id: HashMap::new(),
        children: HashMap::new(),
    };
    for row in rows {
        let (id, node) = row?;
        nodes.children.entry(node.parent).or_default().push(id);
        nodes.by_id.insert(id, node);
    }
    Ok(nodes)
}

struct Builder<'a> {
    conn: &'a Connection,
    nodes: &'a Nodes,
    keep: &'a HashSet<PlaylistId>,
    playlists: Vec<(PlaylistId, String)>,
    /// Referenced tracks, in order of first appearance.
    tracks: Vec<TrackId>,
    seen: HashSet<TrackId>,
}

impl Builder<'_> {
    fn children(&mut self, parent: Option<PlaylistId>, prefix: &str) -> Result<Vec<XmlNode>> {
        let mut out = Vec::new();
        let ids = self
            .nodes
            .children
            .get(&parent)
            .cloned()
            .unwrap_or_default();
        for id in ids.into_iter().filter(|id| self.keep.contains(id)) {
            let Some(node) = self.nodes.by_id.get(&id) else {
                continue;
            };
            let path = if prefix.is_empty() {
                node.name.clone()
            } else {
                format!("{prefix}/{}", node.name)
            };
            if node.is_folder {
                let children = self.children(Some(id), &path)?;
                out.push(XmlNode::folder(node.name.clone(), children));
            } else {
                let entries = self.entries(id)?;
                for &t in &entries {
                    if self.seen.insert(t) {
                        self.tracks.push(t);
                    }
                }
                let keys = entries.iter().map(|t| t.to_string()).collect();
                out.push(XmlNode::playlist(node.name.clone(), 0, keys));
                self.playlists.push((id, path));
            }
        }
        Ok(out)
    }

    fn entries(&self, playlist: PlaylistId) -> Result<Vec<TrackId>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT track_id FROM playlist_track
             WHERE playlist_id = ?1 AND item_kind = 'track' ORDER BY position",
        )?;
        let ids = stmt
            .query_map([playlist], |r| r.get(0))?
            .collect::<rusqlite::Result<Vec<TrackId>>>()?;
        Ok(ids)
    }
}

fn xml_track(conn: &Connection, id: TrackId) -> Result<XmlTrack> {
    let mut t = conn
        .prepare_cached(
            "SELECT path, kind, file_size, duration_ms, sample_rate, bit_rate_kbps,
                    title, artist, album, genre, composer, remixer, label, mix, grouping,
                    track_no, disc_no, year, comment, rating, color, key, bpm_x100,
                    play_count, date_added, last_played
             FROM track WHERE id = ?1",
        )?
        .query_row([id], |r| {
            let path: String = r.get(0)?;
            let kind: String = r.get(1)?;
            Ok((
                path,
                XmlTrack {
                    track_id: 0,
                    kind: xml_kind(&kind).map(str::to_owned),
                    size: r
                        .get::<_, Option<i64>>(2)?
                        .and_then(|s| u64::try_from(s).ok()),
                    total_time: r
                        .get::<_, Option<i64>>(3)?
                        .and_then(|ms| u32::try_from(ms / 1000).ok()),
                    sample_rate: r.get(4)?,
                    bit_rate: r.get(5)?,
                    name: r.get(6)?,
                    artist: r.get(7)?,
                    album: r.get(8)?,
                    genre: r.get(9)?,
                    composer: r.get(10)?,
                    remixer: r.get(11)?,
                    label: r.get(12)?,
                    mix: r.get(13)?,
                    grouping: r.get(14)?,
                    track_number: r.get(15)?,
                    disc_number: r.get(16)?,
                    year: r.get(17)?,
                    comments: r.get(18)?,
                    rating: Some(r.get::<_, i64>(19)?.clamp(0, 5) as u8 * 51),
                    colour: r
                        .get::<_, Option<String>>(20)?
                        .as_deref()
                        .and_then(colour_rgb),
                    tonality: r.get(21)?,
                    average_bpm: r.get::<_, Option<i64>>(22)?.map(|b| b as f64 / 100.0),
                    play_count: r.get(23)?,
                    date_added: date(r.get(24)?),
                    last_played: r.get::<_, Option<String>>(25)?.and_then(date),
                    ..XmlTrack::default()
                },
            ))
        })
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Error::TrackNotFound(id),
            e => e.into(),
        })
        .and_then(|(path, mut t)| {
            t.location = Some(path_to_location(Path::new(&path))?);
            Ok(t)
        })?;
    t.track_id = u32::try_from(id).map_err(|_| Error::InvalidField {
        field: "track id",
        reason: format!("{id} does not fit rekordbox's 32-bit TrackID"),
    })?;
    t.tempos = conn
        .prepare_cached(
            "SELECT position_ms, bpm_x100, meter_num, meter_den, beat_in_bar
             FROM beatgrid WHERE track_id = ?1 ORDER BY position_ms, id",
        )?
        .query_map([id], |r| {
            Ok(Tempo {
                inizio: seconds(r.get(0)?),
                bpm: r.get::<_, i64>(1)? as f64 / 100.0,
                metro: Some(format!("{}/{}", r.get::<_, i64>(2)?, r.get::<_, i64>(3)?)),
                battito: u8::try_from(r.get::<_, i64>(4)?).ok(),
                extra: Default::default(),
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    // Memory cues first, then hot cues by slot, as rekordbox writes them.
    t.position_marks = conn
        .prepare_cached(
            "SELECT slot, kind, start_ms, end_ms, name, color_rgb FROM cue WHERE track_id = ?1
             ORDER BY slot IS NOT NULL, slot, start_ms, id",
        )?
        .query_map([id], |r| {
            Ok(PositionMark {
                num: r.get::<_, Option<i32>>(0)?.unwrap_or(-1),
                kind: u8::try_from(r.get::<_, i64>(1)?).unwrap_or(0),
                start: seconds(r.get(2)?),
                end: r.get::<_, Option<f64>>(3)?.map(seconds),
                name: r.get(4)?,
                rgb: r.get::<_, Option<i64>>(5)?.map(|c| {
                    let c = c as u32;
                    ((c >> 16) as u8, (c >> 8) as u8, c as u8)
                }),
                extra: Default::default(),
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(t)
}

/// rekordbox's `Kind` text for `track.kind`. AAC is `M4A File`; baken reads
/// `ALAC File` to tell ALAC from AAC in an `.m4a`.
fn xml_kind(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "mp3" => "MP3 File",
        "aac" => "M4A File",
        "alac" => "ALAC File",
        "flac" => "FLAC File",
        "wav" => "WAV File",
        "aiff" => "AIFF File",
        _ => return None,
    })
}

/// ms → seconds, snapped to rekordbox's 3 decimals when it is one already
/// (import stores `Inizio × 1000`, so `0.025 s` comes back as
/// `25.000000000000004 ms`; this writes `0.025` again).
fn seconds(ms: f64) -> f64 {
    if (ms - ms.round()).abs() < 1e-6 {
        ms.round() / 1000.0
    } else {
        ms / 1000.0
    }
}

/// The `yyyy-mm-dd` part of a stored date (dates are `yyyy-mm-dd` or
/// ISO-8601 timestamps).
fn date(s: String) -> Option<String> {
    let d = s.get(..10)?;
    let b = d.as_bytes();
    (b[4] == b'-' && b[7] == b'-').then(|| d.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seconds_snap_to_milliseconds() {
        assert_eq!(seconds(0.025 * 1000.0), 0.025);
        assert_eq!(seconds(25.0), 0.025);
        assert_eq!(seconds(15_000.0), 15.0);
        assert_eq!(seconds(0.0), 0.0);
        assert_eq!(seconds(12.5), 0.0125);
    }

    #[test]
    fn dates() {
        assert_eq!(date("2026-10-19".into()).as_deref(), Some("2026-10-19"));
        assert_eq!(
            date("2026-10-19T12:00:00Z".into()).as_deref(),
            Some("2026-10-19")
        );
        assert_eq!(date("19/10/2026".into()), None);
        assert_eq!(date("".into()), None);
    }

    #[test]
    fn kinds_match_import() {
        for k in ["mp3", "aac", "alac", "flac", "wav", "aiff"] {
            assert!(xml_kind(k).is_some(), "{k}");
        }
        assert_eq!(xml_kind("ogg"), None);
    }
}
