//! Export a library selection as rekordbox XML: the inverse of [`import`].
//!
//! Used as the input for the legacy USB writer (baken, D7; Phase 2 task 6):
//! the file holds only the selected playlists, their folders and the tracks
//! they reference, and [`XmlExport::baken_playlists`] gives the matching
//! `Options.playlists`. `TrackID` is our track id.
//!
//! Playlist dividers (D22) are left out, except those passed to
//! [`export_with_dividers`]: the legacy writer turns each into a track row
//! for a generated silent file, placed at the divider's entry position.
//!
//! A smart playlist (D29) is written as a plain playlist of the tracks its
//! rule matches now, in its own sort.
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
use crate::playlist::Entry;
use crate::query::{playlist_track_ids, PlaylistId};
use crate::rule::PlaylistIndex;
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

/// A playlist divider written as a track row (legacy USB export, D22): Name
/// is the label; no artist, album, genre, key, BPM, `TEMPO` or
/// `POSITION_MARK`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DividerTrack {
    pub playlist_id: PlaylistId,
    /// 1-based among the playlist's dividers, in entry order.
    pub ordinal: u32,
    /// `TrackID`; must not be a library track id in the file.
    pub track_id: u32,
    pub label: String,
    /// The generated WAV.
    pub path: std::path::PathBuf,
    pub size: u64,
    pub duration_ms: u64,
    pub sample_rate: u32,
    pub bit_rate_kbps: u32,
}

impl DividerTrack {
    fn xml(&self) -> Result<XmlTrack> {
        Ok(XmlTrack {
            track_id: self.track_id,
            name: Some(self.label.clone()),
            kind: xml_kind("wav").map(str::to_owned),
            size: Some(self.size),
            total_time: u32::try_from(self.duration_ms / 1000).ok(),
            sample_rate: Some(self.sample_rate),
            bit_rate: Some(self.bit_rate_kbps),
            location: Some(path_to_location(&self.path)?),
            ..XmlTrack::default()
        })
    }
}

/// How memory loops marked active (Phase 5 task 17, [`crate::Cue::active_loop`])
/// go into the XML, which has no field for them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActiveLoops {
    /// Plain memory loops (rekordbox reads the XML; the flag stays in reks).
    #[default]
    Leave,
    /// The earliest active memory loop's `Name` starts with
    /// [`BAKEN_ACTIVE_LOOP_MARKER`]: baken-export writes it as the `.DAT`
    /// memory list's active loop (`PCPT` status 4). For the temp XML handed
    /// to baken only, behind the `export.active_loops` flag.
    BakenMarker,
}

/// baken-export's `collection::ACTIVE_LOOP_MARKER` (issue #210); a test in
/// `reks-export-rb` keeps the two equal.
pub const BAKEN_ACTIVE_LOOP_MARKER: &str = "[active]";

/// Builds the collection for `sel`.
///
/// Errors: an id that is not in the library ([`Error::PlaylistNotFound`]),
/// or a selection with no playlist in it ([`Error::EmptySelection`]).
pub fn export(lib: &Library, sel: &Selection) -> Result<XmlExport> {
    export_with_dividers(lib, sel, &[], ActiveLoops::Leave)
}

/// [`export`], plus `dividers` as track rows at their playlist positions.
/// Dividers not listed stay out; a listed divider whose playlist is not
/// exported or has fewer dividers is ignored.
pub fn export_with_dividers(
    lib: &Library,
    sel: &Selection,
    dividers: &[DividerTrack],
    active_loops: ActiveLoops,
) -> Result<XmlExport> {
    let conn = lib.conn()?;
    let nodes = load_nodes(&conn)?;
    let index = PlaylistIndex::load(&conn)?;

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

    let by_slot: HashMap<(PlaylistId, u32), &DividerTrack> = dividers
        .iter()
        .map(|d| ((d.playlist_id, d.ordinal), d))
        .collect();
    let mut b = Builder {
        conn: &conn,
        index: &index,
        nodes: &nodes,
        keep: &keep,
        dividers: &by_slot,
        playlists: Vec::new(),
        tracks: Vec::new(),
        seen: HashSet::new(),
        divider_rows: Vec::new(),
    };
    let children = b.children(None, "")?;
    let mut tracks = b
        .tracks
        .iter()
        .map(|&id| xml_track(&conn, id, active_loops))
        .collect::<Result<Vec<_>>>()?;
    for d in &b.divider_rows {
        tracks.push(d.xml()?);
    }
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
    export_file_with_dividers(lib, sel, &[], ActiveLoops::Leave, path)
}

/// [`export_with_dividers`], written to `path` with [`super::write`].
pub fn export_file_with_dividers(
    lib: &Library,
    sel: &Selection,
    dividers: &[DividerTrack],
    active_loops: ActiveLoops,
    path: &Path,
) -> Result<XmlExport> {
    let out = export_with_dividers(lib, sel, dividers, active_loops)?;
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
    index: &'a PlaylistIndex,
    nodes: &'a Nodes,
    keep: &'a HashSet<PlaylistId>,
    dividers: &'a HashMap<(PlaylistId, u32), &'a DividerTrack>,
    playlists: Vec<(PlaylistId, String)>,
    /// Referenced tracks, in order of first appearance.
    tracks: Vec<TrackId>,
    seen: HashSet<TrackId>,
    /// Dividers written, in order of appearance.
    divider_rows: Vec<&'a DividerTrack>,
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
            } else if self.index.smart(id).is_some() {
                let mut keys = Vec::new();
                for t in playlist_track_ids(self.conn, self.index, id)? {
                    if self.seen.insert(t) {
                        self.tracks.push(t);
                    }
                    keys.push(t.to_string());
                }
                out.push(XmlNode::playlist(node.name.clone(), 0, keys));
                self.playlists.push((id, path));
            } else {
                let mut keys = Vec::new();
                let mut ordinal = 0u32;
                for entry in crate::playlist::entries(self.conn, id)? {
                    match entry {
                        Entry::Track(t) => {
                            if self.seen.insert(t) {
                                self.tracks.push(t);
                            }
                            keys.push(t.to_string());
                        }
                        Entry::Divider(_) => {
                            ordinal += 1;
                            if let Some(&d) = self.dividers.get(&(id, ordinal)) {
                                self.divider_rows.push(d);
                                keys.push(d.track_id.to_string());
                            }
                        }
                    }
                }
                out.push(XmlNode::playlist(node.name.clone(), 0, keys));
                self.playlists.push((id, path));
            }
        }
        Ok(out)
    }
}

fn xml_track(conn: &Connection, id: TrackId, active_loops: ActiveLoops) -> Result<XmlTrack> {
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
            "SELECT slot, kind, start_ms, end_ms, name, color_rgb, active_loop FROM cue
             WHERE track_id = ?1 ORDER BY slot IS NOT NULL, slot, start_ms, id",
        )?
        .query_map([id], |r| {
            Ok((
                PositionMark {
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
                },
                r.get::<_, bool>(6)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .scan(false, |marked, (mut m, active)| {
            // Memory loops come first, by start: the first active one is the earliest.
            let mark = active_loops == ActiveLoops::BakenMarker
                && active
                && !*marked
                && m.num == -1
                && m.kind == 4
                && m.end.is_some();
            if mark {
                *marked = true;
                m.name = Some(match m.name.take() {
                    Some(n) => format!("{BAKEN_ACTIVE_LOOP_MARKER} {n}"),
                    None => BAKEN_ACTIVE_LOOP_MARKER.to_owned(),
                });
            }
            Some(m)
        })
        .collect();
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
