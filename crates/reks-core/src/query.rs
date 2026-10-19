//! Track table queries: paged, sorted, filtered and searched (`Library::tracks`),
//! plus the option lists the filter sidebar needs (`Library::facets`).
//!
//! Semantics: `docs/phases/phase-1.md` (task 8 notes). Every user value is a
//! bound parameter; the SQL text is assembled only from fixed fragments.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::OnceLock;

use rusqlite::types::Value;
use rusqlite::{params_from_iter, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::db::Library;
use crate::error::Result;
use crate::keys::{Key, Mode};
use crate::rbxml::TRACK_COLOURS;
use crate::scan::TrackId;

/// Row id in the `playlist` table.
pub type PlaylistId = i64;

/// Largest page `Library::tracks` returns, whatever `limit` asks for.
pub const MAX_PAGE: u32 = 1_000;

/// One page of the track table.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TrackQuery {
    /// Free text, matched against the full-text index (title, artist, album,
    /// genre, comment, label, remixer). Every word must match as a prefix.
    pub text: Option<String>,
    /// All filters must hold.
    pub filters: Vec<Filter>,
    pub sort: Sort,
    pub offset: u32,
    /// Clamped to `1..=MAX_PAGE`.
    pub limit: u32,
}

impl Default for TrackQuery {
    fn default() -> Self {
        Self {
            text: None,
            filters: Vec::new(),
            sort: Sort::default(),
            offset: 0,
            limit: 100,
        }
    }
}

/// A condition on tracks.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Filter {
    /// Genre equals (case-insensitive).
    Genre { genre: String },
    /// Key is one of these.
    KeyIn { keys: Vec<Key> },
    /// `bpm_x100` within `min..=max`.
    BpmRange { min: u32, max: u32 },
    /// Colour name equals (`rbxml::TRACK_COLOURS`).
    Color { color: String },
    /// Rating at least this (0..5).
    RatingMin { rating: u8 },
    /// In this playlist, or in any playlist below this folder.
    Playlist { id: PlaylistId },
    /// File missing at the last check (or not).
    Missing { missing: bool },
    /// No beatgrid (or has one).
    NeedsAnalysis { needs: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Sort {
    pub column: SortColumn,
    pub descending: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SortColumn {
    #[default]
    Title,
    Artist,
    Album,
    Genre,
    Label,
    Bpm,
    /// Camelot order: 1A, 1B, 2A, …
    Key,
    Rating,
    Color,
    Duration,
    Year,
    DateAdded,
    PlayCount,
    Kind,
    BitRate,
    /// Position in the playlist of the first `Filter::Playlist`; id order
    /// without one.
    PlaylistPosition,
}

/// The track table's columns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackRow {
    pub id: TrackId,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    pub label: Option<String>,
    pub remixer: Option<String>,
    pub comment: Option<String>,
    /// Canonical key (`keys.rs`), or whatever is stored if it doesn't parse.
    pub key: Option<String>,
    pub bpm_x100: Option<i64>,
    pub rating: i64,
    pub color: Option<String>,
    pub duration_ms: Option<i64>,
    pub year: Option<i64>,
    pub kind: String,
    pub bit_rate_kbps: Option<i64>,
    pub date_added: String,
    pub play_count: i64,
    pub missing: bool,
    pub needs_analysis: bool,
    /// The entry's position in a playlist view (one row per entry, so a
    /// track in the playlist twice is two rows); `None` elsewhere.
    pub entry_position: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackPage {
    pub rows: Vec<TrackRow>,
    /// Matching tracks over all pages.
    pub total: u64,
}

/// Option lists for the filter sidebar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Facets {
    /// Distinct genres in the library, case-insensitively de-duplicated and sorted.
    pub genres: Vec<String>,
    /// All 24 keys in Camelot order.
    pub keys: Vec<KeyInfo>,
    /// Track colour names, in rekordbox order.
    pub colours: Vec<String>,
    /// Playlists and folders, depth first in `sort_order`.
    pub playlists: Vec<PlaylistNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyInfo {
    pub key: Key,
    pub camelot: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistNode {
    pub id: PlaylistId,
    pub parent_id: Option<PlaylistId>,
    pub name: String,
    pub is_folder: bool,
    /// 0 for top-level nodes.
    pub depth: u32,
    /// Index among its siblings (`sort_order`, kept dense by `playlist_mutate`).
    pub position: u32,
    /// Number of entries (0 for folders).
    pub entries: u32,
}

pub(crate) const ROW_COLUMNS: &str =
    "t.id, t.title, t.artist, t.album, t.genre, t.label, t.remixer, \
     t.comment, t.key, t.bpm_x100, t.rating, t.color, t.duration_ms, t.year, t.kind, \
     t.bit_rate_kbps, t.date_added, t.play_count, t.missing, \
     NOT EXISTS (SELECT 1 FROM beatgrid b WHERE b.track_id = t.id)";

impl Library {
    /// One page of tracks matching `q`, and the total match count.
    pub fn tracks(&self, q: &TrackQuery) -> Result<TrackPage> {
        let conn = self.conn()?;
        let view = entry_view(&conn, q)?;
        let w = where_clause(q, view);
        let (order, order_params) = order_by(q, view);
        // In a playlist view every entry is a row; `pe.position` is its place.
        let (from, entry_col) = match view {
            Some(_) => (
                "track t JOIN playlist_track pe ON pe.track_id = t.id AND pe.playlist_id = ?",
                "pe.position",
            ),
            None => ("track t", "NULL"),
        };
        let mut params: Vec<Value> = view.map(Value::Integer).into_iter().collect();
        params.extend(w.params);

        let total: i64 = conn.query_row(
            &format!("SELECT count(*) FROM {from}{}", w.sql),
            params_from_iter(&params),
            |r| r.get(0),
        )?;

        let sql = format!(
            "SELECT {ROW_COLUMNS}, {entry_col} FROM {from}{} ORDER BY {order} LIMIT ? OFFSET ?",
            w.sql
        );
        params.extend(order_params);
        params.push(Value::Integer(i64::from(q.limit.clamp(1, MAX_PAGE))));
        params.push(Value::Integer(i64::from(q.offset)));
        let mut stmt = conn.prepare_cached(&sql)?;
        let rows = stmt
            .query_map(params_from_iter(&params), |r| {
                let mut row = track_row(r)?;
                row.entry_position = r.get(20)?;
                Ok(row)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(TrackPage {
            rows,
            total: u64::try_from(total).unwrap_or(0),
        })
    }

    /// Genres, keys, colours and playlists for the filter sidebar.
    pub fn facets(&self) -> Result<Facets> {
        let conn = self.conn()?;
        let genres = conn
            .prepare(
                "SELECT min(genre) FROM track WHERE genre IS NOT NULL AND genre <> ''
                 GROUP BY genre COLLATE NOCASE ORDER BY genre COLLATE NOCASE",
            )?
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?;
        let mut keys: Vec<Key> = Key::all().collect();
        keys.sort_by_key(|k| camelot_rank(*k));
        Ok(Facets {
            genres,
            keys: keys
                .into_iter()
                .map(|key| KeyInfo {
                    key,
                    camelot: key.camelot(),
                })
                .collect(),
            colours: TRACK_COLOURS.iter().map(|(n, _)| (*n).to_owned()).collect(),
            playlists: playlist_nodes(&conn)?,
        })
    }
}

pub(crate) fn track_row(r: &Row<'_>) -> rusqlite::Result<TrackRow> {
    Ok(TrackRow {
        id: r.get(0)?,
        title: r.get(1)?,
        artist: r.get(2)?,
        album: r.get(3)?,
        genre: r.get(4)?,
        label: r.get(5)?,
        remixer: r.get(6)?,
        comment: r.get(7)?,
        key: r.get(8)?,
        bpm_x100: r.get(9)?,
        rating: r.get(10)?,
        color: r.get(11)?,
        duration_ms: r.get(12)?,
        year: r.get(13)?,
        kind: r.get(14)?,
        bit_rate_kbps: r.get(15)?,
        date_added: r.get(16)?,
        play_count: r.get(17)?,
        missing: r.get(18)?,
        needs_analysis: r.get(19)?,
        entry_position: None,
    })
}

/// The playlist whose entries are the rows: the first `Filter::Playlist`,
/// when it names a playlist rather than a folder.
fn entry_view(conn: &Connection, q: &TrackQuery) -> Result<Option<PlaylistId>> {
    let Some(id) = first_playlist(q) else {
        return Ok(None);
    };
    let is_folder: Option<bool> = conn
        .prepare_cached("SELECT is_folder FROM playlist WHERE id = ?1")?
        .query_row([id], |r| r.get(0))
        .optional()?;
    Ok((is_folder == Some(false)).then_some(id))
}

fn first_playlist(q: &TrackQuery) -> Option<PlaylistId> {
    q.filters.iter().find_map(|f| match f {
        Filter::Playlist { id } => Some(*id),
        _ => None,
    })
}

/// A `WHERE …` clause (empty, or starting with a space) and its parameters.
struct Where {
    sql: String,
    params: Vec<Value>,
}

/// `view`: the entry-view playlist, whose filter the join already applies.
fn where_clause(q: &TrackQuery, view: Option<PlaylistId>) -> Where {
    let mut conds: Vec<String> = Vec::new();
    let mut params: Vec<Value> = Vec::new();
    let mut view_filter = view;

    if let Some(m) = q.text.as_deref().and_then(fts_query) {
        conds.push("t.id IN (SELECT rowid FROM track_fts WHERE track_fts MATCH ?)".into());
        params.push(Value::Text(m));
    }
    for f in &q.filters {
        match f {
            Filter::Genre { genre } => {
                conds.push("t.genre = ? COLLATE NOCASE".into());
                params.push(Value::Text(genre.clone()));
            }
            Filter::KeyIn { keys } => {
                if keys.is_empty() {
                    conds.push("0".into());
                } else {
                    conds.push(format!("t.key IN ({})", placeholders(keys.len())));
                    params.extend(keys.iter().map(|k| Value::Text(k.canonical().to_owned())));
                }
            }
            Filter::BpmRange { min, max } => {
                conds.push("t.bpm_x100 BETWEEN ? AND ?".into());
                params.push(Value::Integer(i64::from(*min)));
                params.push(Value::Integer(i64::from(*max)));
            }
            Filter::Color { color } => {
                conds.push("t.color = ? COLLATE NOCASE".into());
                params.push(Value::Text(color.clone()));
            }
            Filter::RatingMin { rating } => {
                conds.push("t.rating >= ?".into());
                params.push(Value::Integer(i64::from(*rating)));
            }
            Filter::Playlist { id } => {
                if view_filter == Some(*id) {
                    view_filter = None;
                    continue;
                }
                conds.push(
                    "EXISTS (SELECT 1 FROM playlist_track pt WHERE pt.track_id = t.id
                       AND pt.playlist_id IN (
                         WITH RECURSIVE sub(id) AS (
                           SELECT ? UNION SELECT p.id FROM playlist p JOIN sub ON p.parent_id = sub.id)
                         SELECT id FROM sub))"
                        .into(),
                );
                params.push(Value::Integer(*id));
            }
            Filter::Missing { missing } => {
                conds.push("t.missing = ?".into());
                params.push(Value::Integer(i64::from(*missing)));
            }
            Filter::NeedsAnalysis { needs } => {
                let not = if *needs { "NOT " } else { "" };
                conds.push(format!(
                    "{not}EXISTS (SELECT 1 FROM beatgrid b WHERE b.track_id = t.id)"
                ));
            }
        }
    }

    let sql = if conds.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", conds.join(" AND "))
    };
    Where { sql, params }
}

/// `ORDER BY` terms and their parameters. Empty values sort last in both
/// directions; ties break on id so pages are stable.
fn order_by(q: &TrackQuery, view: Option<PlaylistId>) -> (String, Vec<Value>) {
    let dir = if q.sort.descending { "DESC" } else { "ASC" };
    let text = |col: &str| format!("({col} IS NULL OR {col} = ''), {col} COLLATE NOCASE {dir}");
    let num = |expr: &str| format!("({expr}) IS NULL, ({expr}) {dir}");
    let mut params = Vec::new();
    let order = match q.sort.column {
        SortColumn::Title => text("t.title"),
        SortColumn::Artist => text("t.artist"),
        SortColumn::Album => text("t.album"),
        SortColumn::Genre => text("t.genre"),
        SortColumn::Label => text("t.label"),
        SortColumn::Kind => text("t.kind"),
        SortColumn::Color => num(color_rank_sql()),
        SortColumn::Key => num(key_rank_sql()),
        SortColumn::Bpm => num("t.bpm_x100"),
        SortColumn::Rating => num("t.rating"),
        SortColumn::Duration => num("t.duration_ms"),
        SortColumn::Year => num("t.year"),
        SortColumn::DateAdded => text("t.date_added"),
        SortColumn::PlayCount => num("t.play_count"),
        SortColumn::BitRate => num("t.bit_rate_kbps"),
        SortColumn::PlaylistPosition if view.is_some() => num("pe.position"),
        SortColumn::PlaylistPosition => {
            match first_playlist(q) {
                Some(id) => {
                    // The expression appears twice in `num`.
                    params.extend([Value::Integer(id), Value::Integer(id)]);
                    num("SELECT min(pt.position) FROM playlist_track pt \
                         WHERE pt.track_id = t.id AND pt.playlist_id = ?")
                }
                None => format!("t.id {dir}"),
            }
        }
    };
    // Copies of one track in a playlist view tie on id; position breaks it.
    let tie = if view.is_some() {
        "t.id ASC, pe.position ASC"
    } else {
        "t.id ASC"
    };
    (format!("{order}, {tie}"), params)
}

/// Turns user text into an FTS5 query: each whitespace-separated word with
/// at least one letter or digit becomes a quoted prefix term (`"word"*`), so
/// FTS syntax characters in the input are plain text. `None` if no words.
pub(crate) fn fts_query(text: &str) -> Option<String> {
    let terms: Vec<String> = text
        .split_whitespace()
        .filter(|w| w.chars().any(char::is_alphanumeric))
        .map(|w| format!("\"{}\"*", w.replace('"', "\"\"")))
        .collect();
    (!terms.is_empty()).then(|| terms.join(" "))
}

fn placeholders(n: usize) -> String {
    vec!["?"; n].join(", ")
}

/// 1A → 1, 1B → 2, 2A → 3, …
fn camelot_rank(k: Key) -> u8 {
    let minor = u8::from(k.mode() == Mode::Minor);
    k.camelot_number() * 2 - minor
}

/// `CASE` mapping canonical key names to their Camelot rank (unknown → NULL).
/// Built from `Key::all()`: fixed strings, no user input.
fn key_rank_sql() -> &'static str {
    static SQL: OnceLock<String> = OnceLock::new();
    SQL.get_or_init(|| {
        let mut s = String::from("CASE t.key");
        for k in Key::all() {
            let _ = write!(s, " WHEN '{}' THEN {}", k.canonical(), camelot_rank(k));
        }
        s.push_str(" END");
        s
    })
}

/// `CASE` mapping colour names to their rekordbox order (unknown → NULL).
fn color_rank_sql() -> &'static str {
    static SQL: OnceLock<String> = OnceLock::new();
    SQL.get_or_init(|| {
        let mut s = String::from("CASE lower(t.color)");
        for (i, (name, _)) in TRACK_COLOURS.iter().enumerate() {
            let _ = write!(s, " WHEN '{name}' THEN {}", i + 1);
        }
        s.push_str(" END");
        s
    })
}

/// Every playlist and folder, depth first, siblings in `sort_order`.
pub(crate) fn playlist_nodes(conn: &Connection) -> Result<Vec<PlaylistNode>> {
    let mut stmt = conn.prepare_cached(
        "SELECT p.id, p.parent_id, p.name, p.is_folder, coalesce(e.n, 0) FROM playlist p
         LEFT JOIN (SELECT playlist_id, count(*) AS n FROM playlist_track GROUP BY playlist_id) e
           ON e.playlist_id = p.id
         ORDER BY p.sort_order, p.id",
    )?;
    let mut all = stmt
        .query_map([], |r| {
            Ok(PlaylistNode {
                id: r.get(0)?,
                parent_id: r.get(1)?,
                name: r.get(2)?,
                is_folder: r.get(3)?,
                depth: 0,
                position: 0,
                entries: r.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut children: HashMap<Option<PlaylistId>, Vec<usize>> = HashMap::new();
    for (i, n) in all.iter_mut().enumerate() {
        let siblings = children.entry(n.parent_id).or_default();
        n.position = u32::try_from(siblings.len()).unwrap_or(u32::MAX);
        siblings.push(i);
    }
    let mut out = Vec::with_capacity(all.len());
    // Depth-first; the stack holds (index, depth) in reverse order.
    let mut stack: Vec<(usize, u32)> = children
        .get(&None)
        .map(|c| c.iter().rev().map(|&i| (i, 0)).collect())
        .unwrap_or_default();
    while let Some((i, depth)) = stack.pop() {
        let Some(n) = all.get(i) else { continue };
        if let Some(c) = children.get(&Some(n.id)) {
            stack.extend(c.iter().rev().map(|&j| (j, depth + 1)));
        }
        out.push(PlaylistNode { depth, ..n.clone() });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fts_query_quotes_words_as_prefixes() {
        assert_eq!(fts_query("  "), None);
        assert_eq!(fts_query("- * ()"), None);
        assert_eq!(
            fts_query("midn groove").as_deref(),
            Some("\"midn\"* \"groove\"*")
        );
        assert_eq!(fts_query("a\"b").as_deref(), Some("\"a\"\"b\"*"));
        assert_eq!(fts_query("NEAR(x").as_deref(), Some("\"NEAR(x\"*"));
    }

    #[test]
    fn camelot_rank_order() {
        let mut keys: Vec<Key> = Key::all().collect();
        keys.sort_by_key(|k| camelot_rank(*k));
        let labels: Vec<String> = keys.iter().take(4).map(|k| k.camelot()).collect();
        assert_eq!(labels, ["1A", "1B", "2A", "2B"]);
        assert_eq!(keys.last().map(|k| k.camelot()).as_deref(), Some("12B"));
    }
}
