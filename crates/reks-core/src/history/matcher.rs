//! Stick entry → library track (phase-2.5.md, Matching).
//!
//! In order: `export_manifest` by (volume serial, stick path), which also marks
//! dividers; then the stick path's suffix against library paths; then
//! (artist, title, duration ±1 s). A fallback counts only when exactly one
//! track fits. Anything else stays unmatched.

use std::collections::HashMap;

use rusqlite::{Connection, OptionalExtension};
use unicode_normalization::UnicodeNormalization;

use super::{EntryKind, MatchKind, SourceEntry};
use crate::error::Result;
use crate::scan::TrackId;

/// Durations agree when they differ by at most this (PDB stores whole seconds).
const DURATION_TOLERANCE_MS: i64 = 1000;

/// The outcome for one entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Match {
    pub kind: EntryKind,
    pub track_id: Option<TrackId>,
    pub matched_by: Option<MatchKind>,
}

impl Match {
    const UNMATCHED: Self = Self {
        kind: EntryKind::Track,
        track_id: None,
        matched_by: None,
    };
}

struct LibTrack {
    id: TrackId,
    /// Folded path components, file name last.
    components: Vec<String>,
    artist: String,
    title: String,
    duration_ms: Option<i64>,
}

/// The library tracks, indexed once per scan.
pub(super) struct Matcher {
    tracks: Vec<LibTrack>,
    /// Folded file name → indexes into `tracks`.
    by_file_name: HashMap<String, Vec<usize>>,
    /// (folded artist, folded title) → indexes into `tracks`.
    by_artist_title: HashMap<(String, String), Vec<usize>>,
}

impl Matcher {
    pub(super) fn load(conn: &Connection) -> Result<Self> {
        let mut stmt = conn.prepare("SELECT id, path, artist, title, duration_ms FROM track")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, TrackId>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<i64>>(4)?,
            ))
        })?;
        let mut m = Self {
            tracks: Vec::new(),
            by_file_name: HashMap::new(),
            by_artist_title: HashMap::new(),
        };
        for row in rows {
            let (id, path, artist, title, duration_ms) = row?;
            let t = LibTrack {
                id,
                components: components(&path),
                artist: fold(artist.as_deref().unwrap_or("")),
                title: fold(title.as_deref().unwrap_or("")),
                duration_ms,
            };
            let i = m.tracks.len();
            if let Some(name) = t.components.last() {
                m.by_file_name.entry(name.clone()).or_default().push(i);
            }
            if !t.title.is_empty() {
                m.by_artist_title
                    .entry((t.artist.clone(), t.title.clone()))
                    .or_default()
                    .push(i);
            }
            m.tracks.push(t);
        }
        Ok(m)
    }

    pub(super) fn find(
        &self,
        conn: &Connection,
        serial: Option<&str>,
        e: &SourceEntry,
    ) -> Result<Match> {
        if let (Some(serial), Some(path)) = (serial, e.stick_path.as_deref()) {
            if let Some(m) = by_manifest(conn, serial, path)? {
                return Ok(m);
            }
        }
        let found = self
            .by_path_suffix(e)
            .map(|id| (id, MatchKind::PathSuffix))
            .or_else(|| self.by_metadata(e).map(|id| (id, MatchKind::Metadata)));
        Ok(match found {
            Some((id, how)) => Match {
                kind: EntryKind::Track,
                track_id: Some(id),
                matched_by: Some(how),
            },
            None => Match::UNMATCHED,
        })
    }

    /// The tracks sharing the stick file's name; the one with the strictly
    /// longest matching path suffix wins. A known duration must agree too.
    fn by_path_suffix(&self, e: &SourceEntry) -> Option<TrackId> {
        let stick = components(e.stick_path.as_deref()?);
        let name = stick.last()?;
        let mut best: Option<(usize, TrackId)> = None;
        let mut tie = false;
        for &i in self.by_file_name.get(name)? {
            let t = &self.tracks[i];
            if !durations_agree(t.duration_ms, e.duration_ms, false) {
                continue;
            }
            let len = common_suffix(&t.components, &stick);
            match best {
                Some((b, _)) if len < b => {}
                Some((b, _)) if len == b => tie = true,
                _ => {
                    best = Some((len, t.id));
                    tie = false;
                }
            }
        }
        if tie {
            return None;
        }
        best.map(|(_, id)| id)
    }

    /// Same artist and title, durations within ±1 s; both durations needed.
    fn by_metadata(&self, e: &SourceEntry) -> Option<TrackId> {
        let title = fold(e.title.as_deref()?);
        if title.is_empty() {
            return None;
        }
        let artist = fold(e.artist.as_deref().unwrap_or(""));
        let mut hits = self
            .by_artist_title
            .get(&(artist, title))?
            .iter()
            .map(|&i| &self.tracks[i])
            .filter(|t| durations_agree(t.duration_ms, e.duration_ms, true));
        let first = hits.next()?;
        hits.next().is_none().then_some(first.id)
    }
}

fn by_manifest(conn: &Connection, serial: &str, path: &str) -> Result<Option<Match>> {
    let row: Option<(String, Option<TrackId>)> = conn
        .query_row(
            "SELECT item_kind, track_id FROM export_manifest
             WHERE volume_serial = ?1 AND stick_path = ?2",
            [serial, path],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    Ok(match row {
        Some((kind, _)) if kind == "divider" => Some(Match {
            kind: EntryKind::Divider,
            track_id: None,
            matched_by: Some(MatchKind::Manifest),
        }),
        Some((_, Some(id))) => Some(Match {
            kind: EntryKind::Track,
            track_id: Some(id),
            matched_by: Some(MatchKind::Manifest),
        }),
        // The library track was deleted since the export: try the fallbacks.
        _ => None,
    })
}

/// `required`: both must be known. Otherwise an unknown side passes.
fn durations_agree(lib: Option<i64>, stick: Option<i64>, required: bool) -> bool {
    match (lib, stick) {
        (Some(a), Some(b)) => (a - b).abs() <= DURATION_TOLERANCE_MS,
        _ => !required,
    }
}

/// Folded, non-empty path components; `/` and `\` both separate.
fn components(path: &str) -> Vec<String> {
    path.split(['/', '\\'])
        .filter(|c| !c.is_empty())
        .map(fold)
        .collect()
}

fn common_suffix(a: &[String], b: &[String]) -> usize {
    a.iter()
        .rev()
        .zip(b.iter().rev())
        .take_while(|(x, y)| x == y)
        .count()
}

/// NFC, trimmed, lower case: FAT32 and players ignore case.
fn fold(s: &str) -> String {
    s.trim().nfc().collect::<String>().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suffix_counts_whole_components() {
        let a = components("/Users/me/Music/Artist/Album/01 Song.mp3");
        let b = components("/Contents/Artist/Album/01 song.MP3");
        assert_eq!(common_suffix(&a, &b), 3);
        let c = components("C:\\Music\\Other\\01 Song.mp3");
        assert_eq!(common_suffix(&c, &b), 1);
    }

    #[test]
    fn durations_within_one_second() {
        assert!(durations_agree(Some(245_900), Some(245_000), true));
        assert!(!durations_agree(Some(246_100), Some(245_000), true));
        assert!(!durations_agree(None, Some(245_000), true));
        assert!(durations_agree(None, Some(245_000), false));
    }
}
