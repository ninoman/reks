//! Track table queries: paged, sorted, filtered and searched (`Library::tracks`),
//! plus the option lists the filter sidebar needs (`Library::facets`).
//!
//! Semantics: `docs/phases/phase-1.md` (task 8 notes). Every user value is a
//! bound parameter; the SQL text is assembled only from fixed fragments.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::OnceLock;

use rusqlite::types::Value;
use rusqlite::{params_from_iter, Connection, Row};
use serde::{Deserialize, Serialize};

use crate::db::Library;
use crate::error::Result;
use crate::keys::{Key, Mode};
use crate::playlist::{entries, Entry};
use crate::rbxml::TRACK_COLOURS;
use crate::rule::{Compiler, PlaylistIndex, RuleNode};
use crate::scan::TrackId;
use crate::tag::{tags_of, TagId};

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
    /// A rule tree (D29), ANDed with `filters` and `text`. Playlist filters
    /// inside it never make the rows a playlist's entries.
    pub rule: Option<RuleNode>,
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
            rule: None,
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
    /// Play count 0 and no last played (D23: baseline plus included sessions).
    NeverPlayed,
    /// Play count within `min..=max`.
    PlayCountRange { min: u32, max: u32 },
    /// Last played on or after `date` (`YYYY-MM-DD`).
    PlayedSince { date: String },
    /// Has at least one of these My Tags (none listed: matches nothing).
    TagAny { ids: Vec<TagId> },
    /// Rating within `min..=max` (0..5).
    RatingRange { min: u8, max: u8 },
    /// Has every one of these My Tags (none listed: matches nothing).
    TagAll { ids: Vec<TagId> },
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
    /// Never-played tracks sort last in both directions.
    LastPlayed,
    Kind,
    BitRate,
    /// Position in the playlist of the first `Filter::Playlist` (for a
    /// smart playlist, its own sort); id order without one.
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
    /// `YYYY-MM-DD` (or a rekordbox date), derived like `play_count` (D23).
    pub last_played: Option<String>,
    pub missing: bool,
    pub needs_analysis: bool,
    /// The picture's id for `artwork_thumb`; `None` without embedded artwork.
    pub artwork_id: Option<i64>,
    /// The entry's position in a playlist view (one row per entry, so a
    /// track in the playlist twice is two rows); `None` elsewhere.
    pub entry_position: Option<i64>,
    /// My Tags, in display order (category, then tag).
    pub tags: Vec<TagId>,
    /// Hot cues by slot, for the waveform thumbnail.
    pub hot_cues: Vec<HotCueMark>,
}

/// A hot cue as the track list marks it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HotCueMark {
    /// 0..=7 = A..H.
    pub slot: u8,
    /// Rounded: a mark, not a position to play from.
    pub position_ms: i64,
    /// 0xRRGGBB, `None` for the default colour.
    pub color_rgb: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackPage {
    pub rows: Vec<TrackRow>,
    /// Matching tracks over all pages. Dividers are not tracks.
    pub total: u64,
    /// In a playlist view in playlist order with no other filter or search
    /// (`TrackQuery::shows_dividers`): every divider of the playlist, on
    /// every page, in order. Empty elsewhere. A divider at `position` sits
    /// between the track rows whose `entry_position`s surround it.
    pub dividers: Vec<DividerRow>,
    /// Whether this is such a view (dividers shown, even if there are none):
    /// the only view where dividers can be inserted.
    pub divider_view: bool,
}

/// One page of rows from `Library::track_pages`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackPageRows {
    pub page: u32,
    pub rows: Vec<TrackRow>,
}

/// The FROM/WHERE/ORDER parts of a track query, shared by the paged and the
/// multi-page reads.
struct RowSelect {
    view: Option<PlaylistId>,
    from: &'static str,
    entry_col: &'static str,
    where_sql: String,
    where_params: Vec<Value>,
    order: String,
    order_params: Vec<Value>,
}

impl RowSelect {
    fn build(conn: &Connection, q: &TrackQuery) -> Result<Self> {
        let index = if q.mentions_playlist() {
            PlaylistIndex::load(conn)?
        } else {
            PlaylistIndex::default()
        };
        let view = first_playlist(q).filter(|&id| index.is_static_playlist(id));
        let w = where_clause(q, view, &index)?;
        let (order, order_params) = order_by(q.sort, view, first_playlist(q), &index);
        // In a playlist view every entry is a row; `pe.position` is its place.
        let (from, entry_col) = match view {
            Some(_) => (
                "track t JOIN playlist_track pe ON pe.track_id = t.id AND pe.playlist_id = ?",
                "pe.position",
            ),
            None => ("track t", "NULL"),
        };
        let mut where_params: Vec<Value> = view.map(Value::Integer).into_iter().collect();
        where_params.extend(w.params);
        Ok(Self {
            view,
            from,
            entry_col,
            where_sql: w.sql,
            where_params,
            order,
            order_params,
        })
    }

    fn select_sql(&self) -> String {
        format!(
            "SELECT {ROW_COLUMNS}, {} FROM {}{}",
            self.entry_col, self.from, self.where_sql
        )
    }
}

/// A divider entry in a playlist (D22).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DividerRow {
    /// Entry position, the same numbering as `TrackRow::entry_position`.
    pub position: i64,
    pub label: String,
    /// Plain ASCII. Other characters are allowed but flagged until the
    /// hardware session shows how players render them (UNVERIFIED, DIV2).
    pub ascii: bool,
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
    pub open_key: String,
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
    /// Number of track entries, dividers left out (0 for folders). For a
    /// smart playlist, the tracks its rule matches now.
    pub entries: u32,
    /// A smart playlist (D29): its tracks follow a saved rule.
    pub smart: bool,
}

pub(crate) const ROW_COLUMNS: &str =
    "t.id, t.title, t.artist, t.album, t.genre, t.label, t.remixer, \
     t.comment, t.key, t.bpm_x100, t.rating, t.color, t.duration_ms, t.year, t.kind, \
     t.bit_rate_kbps, t.date_added, t.play_count, t.missing, \
     NOT EXISTS (SELECT 1 FROM beatgrid b WHERE b.track_id = t.id), t.last_played, t.artwork_id";

impl Library {
    /// One page of tracks matching `q`, and the total match count.
    pub fn tracks(&self, q: &TrackQuery) -> Result<TrackPage> {
        let conn = self.conn()?;
        let sel = RowSelect::build(&conn, q)?;
        let total: i64 = conn.query_row(
            &format!("SELECT count(*) FROM {}{}", sel.from, sel.where_sql),
            params_from_iter(&sel.where_params),
            |r| r.get(0),
        )?;

        let sql = format!(
            "{} ORDER BY {} LIMIT ? OFFSET ?",
            sel.select_sql(),
            sel.order
        );
        let mut params = sel.where_params.clone();
        params.extend(sel.order_params.iter().cloned());
        params.push(Value::Integer(i64::from(q.limit.clamp(1, MAX_PAGE))));
        params.push(Value::Integer(i64::from(q.offset)));
        let mut stmt = conn.prepare_cached(&sql)?;
        let mut rows = stmt
            .query_map(params_from_iter(&params), |r| {
                let mut row = track_row(r)?;
                row.entry_position = r.get(22)?;
                Ok(row)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        fill_lists(&conn, &mut rows)?;

        let view = sel.view;
        let divider_view = view.is_some_and(|id| q.shows_dividers(id));
        let dividers = match view {
            Some(id) if divider_view => conn
                .prepare_cached(
                    "SELECT position, label FROM playlist_track
                     WHERE playlist_id = ?1 AND item_kind = 'divider' ORDER BY position",
                )?
                .query_map([id], |r| {
                    let label: String = r.get(1)?;
                    Ok(DividerRow {
                        position: r.get(0)?,
                        ascii: label.is_ascii(),
                        label,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?,
            _ => Vec::new(),
        };

        Ok(TrackPage {
            rows,
            total: u64::try_from(total).unwrap_or(0),
            dividers,
            divider_view,
        })
    }

    /// The pages `pages` (of `page_size` tracks each, in `q`'s order, `q`'s
    /// offset and limit ignored) in one sorted pass instead of one `OFFSET`
    /// query per page. Pages past the end are left out.
    pub fn track_pages(
        &self,
        q: &TrackQuery,
        page_size: u32,
        pages: &[u32],
    ) -> Result<Vec<TrackPageRows>> {
        let Some(&last) = pages.iter().max() else {
            return Ok(Vec::new());
        };
        let size = page_size.clamp(1, MAX_PAGE) as usize;
        let wanted: std::collections::BTreeSet<usize> = pages.iter().map(|&p| p as usize).collect();
        let conn = self.conn()?;
        let sel = RowSelect::build(&conn, q)?;
        let sql = format!("{} ORDER BY {}", sel.select_sql(), sel.order);
        let mut params = sel.where_params.clone();
        params.extend(sel.order_params.iter().cloned());
        let mut stmt = conn.prepare_cached(&sql)?;
        let mut rows = stmt.query(params_from_iter(&params))?;
        let mut out: Vec<TrackPageRows> = Vec::new();
        let mut index = 0usize;
        while let Some(r) = rows.next()? {
            let page = index / size;
            if page > last as usize {
                break;
            }
            index += 1;
            if !wanted.contains(&page) {
                continue;
            }
            let mut row = track_row(r)?;
            row.entry_position = r.get(22)?;
            match out.last_mut() {
                Some(p) if p.page as usize == page => p.rows.push(row),
                _ => out.push(TrackPageRows {
                    page: page as u32,
                    rows: vec![row],
                }),
            }
        }
        for p in &mut out {
            fill_lists(&conn, &mut p.rows)?;
        }
        Ok(out)
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
                    open_key: key.open_key(),
                })
                .collect(),
            colours: TRACK_COLOURS.iter().map(|(n, _)| (*n).to_owned()).collect(),
            playlists: playlist_nodes(&conn)?,
        })
    }
}

/// Sets `tags` and `hot_cues` on every row (one query each for the page).
pub(crate) fn fill_lists(conn: &Connection, rows: &mut [TrackRow]) -> Result<()> {
    let ids: Vec<TrackId> = rows.iter().map(|r| r.id).collect();
    let mut tags: HashMap<TrackId, Vec<TagId>> = HashMap::new();
    for (track, tag) in tags_of(conn, &ids)? {
        tags.entry(track).or_default().push(tag);
    }
    let mut cues: HashMap<TrackId, Vec<HotCueMark>> = HashMap::new();
    for (track, cue) in hot_cues_of(conn, &ids)? {
        cues.entry(track).or_default().push(cue);
    }
    for row in rows {
        row.tags = tags.remove(&row.id).unwrap_or_default();
        row.hot_cues = cues.remove(&row.id).unwrap_or_default();
    }
    Ok(())
}

fn hot_cues_of(c: &Connection, track_ids: &[TrackId]) -> Result<Vec<(TrackId, HotCueMark)>> {
    if track_ids.is_empty() {
        return Ok(Vec::new());
    }
    let sql = format!(
        "SELECT track_id, slot, start_ms, color_rgb FROM cue
         WHERE slot IS NOT NULL AND track_id IN ({})
         ORDER BY track_id, slot",
        vec!["?"; track_ids.len()].join(", ")
    );
    let mut stmt = c.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(track_ids), |r| {
        let start: f64 = r.get(2)?;
        Ok((
            r.get(0)?,
            HotCueMark {
                slot: r.get(1)?,
                position_ms: start.round() as i64,
                color_rgb: r.get(3)?,
            },
        ))
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
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
        last_played: r.get(20)?,
        artwork_id: r.get(21)?,
        entry_position: None,
        tags: Vec::new(),
        hot_cues: Vec::new(),
    })
}

impl TrackQuery {
    /// Whether the entry view of `playlist` shows its dividers: playlist
    /// order (ascending), no search text, no filter but the playlist and no
    /// rule that could drop a track.
    fn shows_dividers(&self, playlist: PlaylistId) -> bool {
        self.sort.column == SortColumn::PlaylistPosition
            && !self.sort.descending
            && self.text.as_deref().and_then(fts_query).is_none()
            && self.rule.as_ref().is_none_or(RuleNode::is_everything)
            && self
                .filters
                .iter()
                .all(|f| matches!(f, Filter::Playlist { id } if *id == playlist))
    }

    fn mentions_playlist(&self) -> bool {
        first_playlist(self).is_some()
            || self.rule.as_ref().is_some_and(RuleNode::mentions_playlist)
    }
}

/// The first top-level `Filter::Playlist`. When it names a static playlist
/// (not a folder, not smart), its entries are the rows: the entry view.
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
fn where_clause(q: &TrackQuery, view: Option<PlaylistId>, index: &PlaylistIndex) -> Result<Where> {
    let mut conds: Vec<String> = Vec::new();
    let mut c = Compiler::new(index);
    let mut view_filter = view;

    if let Some(m) = q.text.as_deref().and_then(fts_query) {
        conds.push("t.id IN (SELECT rowid FROM track_fts WHERE track_fts MATCH ?)".into());
        c.params.push(Value::Text(m));
    }
    for f in &q.filters {
        if let Filter::Playlist { id } = f {
            if view_filter == Some(*id) {
                view_filter = None;
                continue;
            }
        }
        conds.push(c.filter(f)?);
    }
    if let Some(rule) = &q.rule {
        rule.validate()?;
        conds.push(c.rule(rule)?);
    }

    let sql = if conds.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", conds.join(" AND "))
    };
    Ok(Where {
        sql,
        params: c.params,
    })
}

/// `ORDER BY` terms and their parameters. Empty values sort last in both
/// directions; ties break on id so pages are stable. `first`: the first
/// top-level playlist filter, whose order `PlaylistPosition` means (a smart
/// list's own sort).
fn order_by(
    sort: Sort,
    view: Option<PlaylistId>,
    first: Option<PlaylistId>,
    index: &PlaylistIndex,
) -> (String, Vec<Value>) {
    let dir = if sort.descending { "DESC" } else { "ASC" };
    let text = |col: &str| format!("({col} IS NULL OR {col} = ''), {col} COLLATE NOCASE {dir}");
    let num = |expr: &str| format!("({expr}) IS NULL, ({expr}) {dir}");
    let mut params = Vec::new();
    let order = match sort.column {
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
        SortColumn::LastPlayed => text("t.last_played"),
        SortColumn::BitRate => num("t.bit_rate_kbps"),
        SortColumn::PlaylistPosition if view.is_some() => num("pe.position"),
        SortColumn::PlaylistPosition => match first.map(|id| (id, index.smart(id))) {
            // A smart list has no positions: its own sort, reversed by `descending`.
            Some((_, Some(def))) if def.sort.column != SortColumn::PlaylistPosition => {
                let own = Sort {
                    column: def.sort.column,
                    descending: def.sort.descending != sort.descending,
                };
                return order_by(own, None, None, index);
            }
            Some((id, None)) => {
                // The expression appears twice in `num`.
                params.extend([Value::Integer(id), Value::Integer(id)]);
                num("SELECT min(pt.position) FROM playlist_track pt \
                     WHERE pt.track_id = t.id AND pt.playlist_id = ?")
            }
            _ => format!("t.id {dir}"),
        },
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
        "SELECT p.id, p.parent_id, p.name, p.is_folder, coalesce(e.n, 0),
                p.smart_rule IS NOT NULL FROM playlist p
         LEFT JOIN (SELECT playlist_id, count(*) AS n FROM playlist_track
                    WHERE item_kind = 'track' GROUP BY playlist_id) e
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
                smart: r.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if all.iter().any(|n| n.smart) {
        let index = PlaylistIndex::load(conn)?;
        for n in all.iter_mut().filter(|n| n.smart) {
            n.entries = smart_count(conn, &index, n.id);
        }
    }

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

/// How many tracks smart list `id` holds now; 0 (with a warning) if its
/// rule can't be evaluated.
fn smart_count(conn: &Connection, index: &PlaylistIndex, id: PlaylistId) -> u32 {
    let count = || -> Result<i64> {
        let mut c = Compiler::new(index);
        let cond = c.filter(&Filter::Playlist { id })?;
        Ok(conn.query_row(
            &format!("SELECT count(*) FROM track t WHERE {cond}"),
            params_from_iter(&c.params),
            |r| r.get(0),
        )?)
    };
    match count() {
        Ok(n) => u32::try_from(n).unwrap_or(u32::MAX),
        Err(e) => {
            tracing::warn!(playlist = id, error = %e, "cannot count smart playlist");
            0
        }
    }
}

/// A playlist's tracks in order, dividers left out: a static list's track
/// entries (duplicates kept), or the tracks a smart list's rule matches now
/// in its own sort. A folder or an unknown id has none. Every reader of a
/// playlist's contents goes through this, so smart lists read like static
/// ones everywhere (XML export, stick size, adopt).
pub(crate) fn playlist_track_ids(
    conn: &Connection,
    index: &PlaylistIndex,
    id: PlaylistId,
) -> Result<Vec<TrackId>> {
    if index.smart(id).is_none() {
        return Ok(entries(conn, id)?.iter().filter_map(Entry::track).collect());
    }
    let mut c = Compiler::new(index);
    let cond = c.filter(&Filter::Playlist { id })?;
    let position = Sort {
        column: SortColumn::PlaylistPosition,
        descending: false,
    };
    let (order, order_params) = order_by(position, None, Some(id), index);
    let mut params = c.params;
    params.extend(order_params);
    let ids = conn
        .prepare_cached(&format!(
            "SELECT t.id FROM track t WHERE {cond} ORDER BY {order}"
        ))?
        .query_map(params_from_iter(&params), |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<TrackId>>>()?;
    Ok(ids)
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
