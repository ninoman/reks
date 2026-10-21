//! The playlist tree (`Library::playlists`) and its edits
//! (`Library::playlist_mutate`).
//!
//! Rules: `docs/phases/phase-1.md` (task 10 notes). Every op is validated
//! before anything is written and runs in one transaction. Entry positions
//! and sibling `sort_order`s stay dense (`0..n`).
//!
//! An entry is a track or a labelled divider (D22, `docs/phases/phase-2.5.md`).
//! Remove and reorder treat both alike; dividers are never duplicates.
//!
//! A smart playlist (D29) has no entries: its tracks are what its rule
//! matches, so the entry ops refuse it. Saving a rule, and moving a node,
//! refuse a smart list that would include itself.

use std::collections::{BTreeSet, HashSet};

use rusqlite::types::Value;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::Library;
use crate::error::{Error, Result};
use crate::query::{playlist_nodes, Filter, PlaylistId, PlaylistNode, Sort, SortColumn};
use crate::rule::{Compiler, PlaylistIndex, RuleNode};
use crate::scan::TrackId;
use crate::timefmt::now_utc;
use crate::track;

/// Every playlist and folder, depth first, siblings in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistTree {
    pub nodes: Vec<PlaylistNode>,
}

/// What "add tracks" does with tracks already in the playlist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DuplicateMode {
    /// If any track is a duplicate, write nothing and report them.
    #[default]
    Ask,
    /// Add every track, duplicates included.
    Allow,
    /// Add only the tracks that are not duplicates.
    Skip,
}

/// An edit of the playlist tree or of one playlist's entries.
///
/// Indices (`position`, `before`) refer to the list as it is now, before the
/// op: `0..=len`, where `len` means the end. Moved items keep their relative
/// order.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum PlaylistOp {
    /// `parent_id` `None` = top level; `position` `None` = last.
    CreatePlaylist {
        #[serde(default)]
        parent_id: Option<PlaylistId>,
        name: String,
        #[serde(default)]
        position: Option<u32>,
    },
    CreateFolder {
        #[serde(default)]
        parent_id: Option<PlaylistId>,
        name: String,
        #[serde(default)]
        position: Option<u32>,
    },
    /// A smart playlist: its tracks are what `rule` matches, in `sort`
    /// order (`None`: date added, newest first; never `playlistPosition`).
    CreateSmart {
        #[serde(default)]
        parent_id: Option<PlaylistId>,
        name: String,
        rule: RuleNode,
        #[serde(default)]
        sort: Option<Sort>,
        #[serde(default)]
        position: Option<u32>,
    },
    /// Replaces a smart playlist's rule; `sort` `None` keeps its sort.
    SetRule {
        id: PlaylistId,
        rule: RuleNode,
        #[serde(default)]
        sort: Option<Sort>,
    },
    Rename {
        id: PlaylistId,
        name: String,
    },
    /// A folder takes its whole subtree with it.
    Delete {
        id: PlaylistId,
    },
    /// To `position` among `parent_id`'s children.
    Move {
        id: PlaylistId,
        #[serde(default)]
        parent_id: Option<PlaylistId>,
        position: u32,
    },
    /// Inserted before the entry at `before` (`None` = appended).
    AddTracks {
        playlist_id: PlaylistId,
        track_ids: Vec<TrackId>,
        #[serde(default)]
        before: Option<u32>,
        #[serde(default)]
        duplicates: DuplicateMode,
    },
    /// Removes these entries (not every copy of their tracks).
    RemoveEntries {
        playlist_id: PlaylistId,
        positions: Vec<u32>,
    },
    /// Moves these entries, as a block, to before the entry at `before`.
    ReorderEntries {
        playlist_id: PlaylistId,
        positions: Vec<u32>,
        before: u32,
    },
    /// A divider inserted before the entry at `position` (`None` = appended).
    InsertDivider {
        playlist_id: PlaylistId,
        #[serde(default)]
        position: Option<u32>,
        label: String,
    },
    /// Relabels the divider at `position`.
    RenameDivider {
        playlist_id: PlaylistId,
        position: u32,
        label: String,
    },
}

/// Longest divider label, in characters. The players' display limit is
/// UNVERIFIED (DIV2).
pub const MAX_DIVIDER_LABEL_CHARS: usize = 64;

/// The result of an op. Duplicates are an outcome, not an error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PlaylistOutcome {
    Done {
        /// The new node, for the create ops.
        created_id: Option<PlaylistId>,
        /// Entries added (add tracks).
        added: u32,
        /// Duplicates left out (add tracks with `skip`).
        skipped: u32,
    },
    /// Add tracks with `ask` found duplicates; nothing was written.
    Duplicates {
        playlist_id: PlaylistId,
        /// The duplicated tracks, each once, in request order.
        track_ids: Vec<TrackId>,
        /// Requested tracks that are duplicates: already in the playlist, or
        /// earlier in the same request.
        duplicate_count: u32,
        requested: u32,
    },
}

impl PlaylistOutcome {
    fn done() -> Self {
        Self::Done {
            created_id: None,
            added: 0,
            skipped: 0,
        }
    }
}

impl Library {
    /// The saved rule of a smart playlist, for editing it.
    pub fn playlist_rule(&self, id: PlaylistId) -> Result<RuleNode> {
        let conn = self.conn()?;
        if !node(&conn, id)?.smart {
            return Err(Error::NotASmartPlaylist(id));
        }
        let json: String = conn
            .prepare_cached("SELECT smart_rule FROM playlist WHERE id = ?1")?
            .query_row([id], |r| r.get(0))?;
        serde_json::from_str(&json).map_err(|e| Error::InvalidField {
            field: "rule",
            reason: format!("the stored rule does not parse: {e}"),
        })
    }

    /// The playlist tree.
    pub fn playlists(&self) -> Result<PlaylistTree> {
        let conn = self.conn()?;
        Ok(PlaylistTree {
            nodes: playlist_nodes(&conn)?,
        })
    }

    /// Applies `op` in one transaction and sets `updated_at` on every node it
    /// changes. A rejected op changes nothing.
    pub fn playlist_mutate(&self, op: PlaylistOp) -> Result<PlaylistOutcome> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        let now = now_utc();
        let outcome = match op {
            PlaylistOp::CreatePlaylist {
                parent_id,
                name,
                position,
            } => create(&tx, parent_id, &name, position, false, &now)?,
            PlaylistOp::CreateFolder {
                parent_id,
                name,
                position,
            } => create(&tx, parent_id, &name, position, true, &now)?,
            PlaylistOp::CreateSmart {
                parent_id,
                name,
                rule,
                sort,
                position,
            } => create_smart(&tx, parent_id, &name, &rule, sort, position, &now)?,
            PlaylistOp::SetRule { id, rule, sort } => set_rule(&tx, id, &rule, sort, &now)?,
            PlaylistOp::Rename { id, name } => rename(&tx, id, &name, &now)?,
            PlaylistOp::Delete { id } => delete(&tx, id, &now)?,
            PlaylistOp::Move {
                id,
                parent_id,
                position,
            } => move_node(&tx, id, parent_id, position, &now)?,
            PlaylistOp::AddTracks {
                playlist_id,
                track_ids,
                before,
                duplicates,
            } => add_tracks(&tx, playlist_id, &track_ids, before, duplicates, &now)?,
            PlaylistOp::RemoveEntries {
                playlist_id,
                positions,
            } => remove_entries(&tx, playlist_id, &positions, &now)?,
            PlaylistOp::ReorderEntries {
                playlist_id,
                positions,
                before,
            } => reorder_entries(&tx, playlist_id, &positions, before, &now)?,
            PlaylistOp::InsertDivider {
                playlist_id,
                position,
                label,
            } => insert_divider(&tx, playlist_id, position, &label, &now)?,
            PlaylistOp::RenameDivider {
                playlist_id,
                position,
                label,
            } => rename_divider(&tx, playlist_id, position, &label, &now)?,
        };
        tx.commit()?;
        Ok(outcome)
    }
}

pub(crate) fn create(
    c: &Connection,
    parent: Option<PlaylistId>,
    name: &str,
    position: Option<u32>,
    is_folder: bool,
    now: &str,
) -> Result<PlaylistOutcome> {
    let name = valid_name(name)?;
    require_folder(c, parent)?;
    let mut siblings = siblings(c, parent)?;
    let at = match position {
        Some(p) => index(p, siblings.len())?,
        None => siblings.len(),
    };
    c.prepare_cached(
        "INSERT INTO playlist (parent_id, is_folder, name, sort_order, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
    )?
    .execute(params![parent, is_folder, name, to_i64(at), now])?;
    let id = c.last_insert_rowid();
    siblings.insert(at, id);
    write_order(c, &siblings, now)?;
    Ok(PlaylistOutcome::Done {
        created_id: Some(id),
        added: 0,
        skipped: 0,
    })
}

fn create_smart(
    c: &Connection,
    parent: Option<PlaylistId>,
    name: &str,
    rule: &RuleNode,
    sort: Option<Sort>,
    position: Option<u32>,
    now: &str,
) -> Result<PlaylistOutcome> {
    valid_rule(rule, sort)?;
    let out = create(c, parent, name, position, false, now)?;
    if let PlaylistOutcome::Done {
        created_id: Some(id),
        ..
    } = out
    {
        c.prepare_cached("UPDATE playlist SET smart_rule = ?2, smart_sort = ?3 WHERE id = ?1")?
            .execute(params![
                id,
                to_json(rule)?,
                sort.map(|s| to_json(&s)).transpose()?
            ])?;
        check_cycles(c, Some(id))?;
    }
    Ok(out)
}

fn set_rule(
    c: &Connection,
    id: PlaylistId,
    rule: &RuleNode,
    sort: Option<Sort>,
    now: &str,
) -> Result<PlaylistOutcome> {
    if !node(c, id)?.smart {
        return Err(Error::NotASmartPlaylist(id));
    }
    valid_rule(rule, sort)?;
    c.prepare_cached("UPDATE playlist SET smart_rule = ?2, updated_at = ?3 WHERE id = ?1")?
        .execute(params![id, to_json(rule)?, now])?;
    if let Some(sort) = sort {
        c.prepare_cached("UPDATE playlist SET smart_sort = ?2 WHERE id = ?1")?
            .execute(params![id, to_json(&sort)?])?;
    }
    check_cycles(c, Some(id))?;
    Ok(PlaylistOutcome::done())
}

/// A rule within the size limits, and a sort a smart list can have (it has
/// no positions of its own).
fn valid_rule(rule: &RuleNode, sort: Option<Sort>) -> Result<()> {
    rule.validate()?;
    if sort.is_some_and(|s| s.column == SortColumn::PlaylistPosition) {
        return Err(Error::InvalidField {
            field: "sort",
            reason: "a smart playlist has no positions of its own".into(),
        });
    }
    Ok(())
}

/// Fails with [`Error::SmartPlaylistCycle`] if a smart list reaches itself
/// through its rules, as the tree is now. `first` is checked first, so its
/// cycle is the one reported.
fn check_cycles(c: &Connection, first: Option<PlaylistId>) -> Result<()> {
    let index = PlaylistIndex::load(c)?;
    let mut ids: Vec<PlaylistId> = index.smart_ids().collect();
    ids.sort_unstable_by_key(|&id| (Some(id) != first, id));
    for id in ids {
        Compiler::strict(&index).filter(&Filter::Playlist { id })?;
    }
    Ok(())
}

fn to_json<T: Serialize>(v: &T) -> Result<String> {
    serde_json::to_string(v).map_err(|e| Error::InvalidField {
        field: "rule",
        reason: e.to_string(),
    })
}

fn rename(c: &Connection, id: PlaylistId, name: &str, now: &str) -> Result<PlaylistOutcome> {
    let name = valid_name(name)?;
    node(c, id)?;
    c.prepare_cached(
        "UPDATE playlist SET name = ?2, updated_at = ?3 WHERE id = ?1 AND name <> ?2",
    )?
    .execute(params![id, name, now])?;
    Ok(PlaylistOutcome::done())
}

fn delete(c: &Connection, id: PlaylistId, now: &str) -> Result<PlaylistOutcome> {
    let n = node(c, id)?;
    // `ON DELETE CASCADE` removes the subtree and its entries.
    c.prepare_cached("DELETE FROM playlist WHERE id = ?1")?
        .execute([id])?;
    write_order(c, &siblings(c, n.parent_id)?, now)?;
    Ok(PlaylistOutcome::done())
}

fn move_node(
    c: &Connection,
    id: PlaylistId,
    parent: Option<PlaylistId>,
    position: u32,
    now: &str,
) -> Result<PlaylistOutcome> {
    let n = node(c, id)?;
    require_folder(c, parent)?;
    if let Some(p) = parent {
        if is_in_subtree(c, id, p)? {
            return Err(Error::MoveCycle { id, parent: p });
        }
    }
    let mut target = siblings(c, parent)?;
    let mut at = index(position, target.len())?;
    if let Some(i) = target.iter().position(|&s| s == id) {
        target.remove(i);
        if i < at {
            at -= 1;
        }
    }
    target.insert(at, id);
    if n.parent_id != parent {
        c.prepare_cached("UPDATE playlist SET parent_id = ?2, updated_at = ?3 WHERE id = ?1")?
            .execute(params![id, parent, now])?;
        write_order(c, &siblings(c, n.parent_id)?, now)?;
    }
    write_order(c, &target, now)?;
    // A smart list moved into a folder its rules name would include itself.
    check_cycles(c, Some(id))?;
    Ok(PlaylistOutcome::done())
}

fn add_tracks(
    c: &Connection,
    playlist: PlaylistId,
    track_ids: &[TrackId],
    before: Option<u32>,
    mode: DuplicateMode,
    now: &str,
) -> Result<PlaylistOutcome> {
    require_static(c, playlist)?;
    let current = entries(c, playlist)?;
    let at = match before {
        Some(b) => index(b, current.len())?,
        None => current.len(),
    };
    let mut exists = c.prepare_cached("SELECT 1 FROM track WHERE id = ?1")?;
    for &t in track_ids {
        if exists.query_row([t], |_| Ok(())).optional()?.is_none() {
            return Err(Error::TrackNotFound(t));
        }
    }

    // A track is a duplicate if the playlist has it or the request named it earlier.
    let mut seen: HashSet<TrackId> = current.iter().filter_map(Entry::track).collect();
    let flagged: Vec<(TrackId, bool)> = track_ids.iter().map(|&t| (t, !seen.insert(t))).collect();
    let dup_count = flagged.iter().filter(|(_, dup)| *dup).count();
    if mode == DuplicateMode::Ask && dup_count > 0 {
        let mut listed = HashSet::new();
        let ids = flagged
            .iter()
            .filter(|&&(t, dup)| dup && listed.insert(t))
            .map(|&(t, _)| t)
            .collect();
        return Ok(PlaylistOutcome::Duplicates {
            playlist_id: playlist,
            track_ids: ids,
            duplicate_count: to_u32(dup_count),
            requested: to_u32(track_ids.len()),
        });
    }
    let add: Vec<TrackId> = flagged
        .iter()
        .filter(|(_, dup)| mode == DuplicateMode::Allow || !dup)
        .map(|&(t, _)| t)
        .collect();
    let mut new = current.clone();
    new.splice(at..at, add.iter().map(|&t| Entry::Track(t)));
    write_entries(c, playlist, &current, &new, now)?;
    Ok(PlaylistOutcome::Done {
        created_id: None,
        added: to_u32(add.len()),
        skipped: to_u32(track_ids.len() - add.len()),
    })
}

fn remove_entries(
    c: &Connection,
    playlist: PlaylistId,
    positions: &[u32],
    now: &str,
) -> Result<PlaylistOutcome> {
    require_static(c, playlist)?;
    let current = entries(c, playlist)?;
    let drop = entry_set(positions, current.len())?;
    let new: Vec<Entry> = current
        .iter()
        .enumerate()
        .filter(|(i, _)| !drop.contains(i))
        .map(|(_, e)| e.clone())
        .collect();
    write_entries(c, playlist, &current, &new, now)?;
    Ok(PlaylistOutcome::done())
}

fn reorder_entries(
    c: &Connection,
    playlist: PlaylistId,
    positions: &[u32],
    before: u32,
    now: &str,
) -> Result<PlaylistOutcome> {
    require_static(c, playlist)?;
    let current = entries(c, playlist)?;
    let moved = entry_set(positions, current.len())?;
    let before = index(before, current.len())?;
    let block: Vec<Entry> = moved.iter().map(|&i| current[i].clone()).collect();
    let mut new: Vec<Entry> = current
        .iter()
        .enumerate()
        .filter(|(i, _)| !moved.contains(i))
        .map(|(_, e)| e.clone())
        .collect();
    let at = before - moved.range(..before).count();
    new.splice(at..at, block);
    write_entries(c, playlist, &current, &new, now)?;
    Ok(PlaylistOutcome::done())
}

fn insert_divider(
    c: &Connection,
    playlist: PlaylistId,
    position: Option<u32>,
    label: &str,
    now: &str,
) -> Result<PlaylistOutcome> {
    let label = valid_label(label)?;
    require_static(c, playlist)?;
    let current = entries(c, playlist)?;
    let at = match position {
        Some(p) => index(p, current.len())?,
        None => current.len(),
    };
    let mut new = current.clone();
    new.insert(at, Entry::Divider(label));
    write_entries(c, playlist, &current, &new, now)?;
    Ok(PlaylistOutcome::done())
}

fn rename_divider(
    c: &Connection,
    playlist: PlaylistId,
    position: u32,
    label: &str,
    now: &str,
) -> Result<PlaylistOutcome> {
    let label = valid_label(label)?;
    require_static(c, playlist)?;
    let current = entries(c, playlist)?;
    let at = entry_index(position, current.len())?;
    if !matches!(current[at], Entry::Divider(_)) {
        return Err(Error::NotADivider { playlist, position });
    }
    let mut new = current.clone();
    new[at] = Entry::Divider(label);
    write_entries(c, playlist, &current, &new, now)?;
    Ok(PlaylistOutcome::done())
}

/// One `playlist_track` row: a track or a divider with its label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Entry {
    Track(TrackId),
    Divider(String),
}

impl Entry {
    pub(crate) fn track(&self) -> Option<TrackId> {
        match self {
            Self::Track(t) => Some(*t),
            Self::Divider(_) => None,
        }
    }

    pub(crate) fn from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        // The table's CHECK guarantees exactly one of the two is set.
        match (
            r.get::<_, Option<TrackId>>(0)?,
            r.get::<_, Option<String>>(1)?,
        ) {
            (Some(t), _) => Ok(Self::Track(t)),
            (None, label) => Ok(Self::Divider(label.unwrap_or_default())),
        }
    }
}

struct Node {
    parent_id: Option<PlaylistId>,
    is_folder: bool,
    smart: bool,
}

fn node(c: &Connection, id: PlaylistId) -> Result<Node> {
    c.prepare_cached(
        "SELECT parent_id, is_folder, smart_rule IS NOT NULL FROM playlist WHERE id = ?1",
    )?
    .query_row([id], |r| {
        Ok(Node {
            parent_id: r.get(0)?,
            is_folder: r.get(1)?,
            smart: r.get(2)?,
        })
    })
    .optional()?
    .ok_or(Error::PlaylistNotFound(id))
}

/// `parent` is the top level or an existing folder.
fn require_folder(c: &Connection, parent: Option<PlaylistId>) -> Result<()> {
    match parent {
        Some(p) if !node(c, p)?.is_folder => Err(Error::NotAFolder(p)),
        _ => Ok(()),
    }
}

/// `id` is a playlist with entries: not a folder, not smart.
fn require_static(c: &Connection, id: PlaylistId) -> Result<()> {
    let n = node(c, id)?;
    if n.is_folder {
        return Err(Error::NotAPlaylist(id));
    }
    if n.smart {
        return Err(Error::SmartPlaylist(id));
    }
    Ok(())
}

/// `candidate` is `root` or below it.
fn is_in_subtree(c: &Connection, root: PlaylistId, candidate: PlaylistId) -> Result<bool> {
    Ok(c.prepare_cached(
        "WITH RECURSIVE sub(id) AS (
           SELECT ?1 UNION SELECT p.id FROM playlist p JOIN sub ON p.parent_id = sub.id)
         SELECT 1 FROM sub WHERE id = ?2",
    )?
    .query_row([root, candidate], |_| Ok(()))
    .optional()?
    .is_some())
}

/// Children of `parent` in display order.
fn siblings(c: &Connection, parent: Option<PlaylistId>) -> Result<Vec<PlaylistId>> {
    Ok(
        c.prepare_cached("SELECT id FROM playlist WHERE parent_id IS ?1 ORDER BY sort_order, id")?
            .query_map([parent], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?,
    )
}

/// Sets `sort_order` to the index in `ids`, touching only rows that change.
fn write_order(c: &Connection, ids: &[PlaylistId], now: &str) -> Result<()> {
    let mut stmt = c.prepare_cached(
        "UPDATE playlist SET sort_order = ?2, updated_at = ?3 WHERE id = ?1 AND sort_order <> ?2",
    )?;
    for (i, id) in ids.iter().enumerate() {
        stmt.execute(params![id, to_i64(i), now])?;
    }
    Ok(())
}

/// The playlist's entries in order.
pub(crate) fn entries(c: &Connection, playlist: PlaylistId) -> Result<Vec<Entry>> {
    Ok(c.prepare_cached(
        "SELECT track_id, label FROM playlist_track WHERE playlist_id = ?1 ORDER BY position",
    )?
    .query_map([playlist], Entry::from_row)?
    .collect::<rusqlite::Result<_>>()?)
}

/// Replaces `old` with `new`. Rows from the first difference on are deleted
/// and re-inserted, so positions stay `0..n` and the `(playlist_id, position)`
/// key never sees two rows at one position.
pub(crate) fn write_entries(
    c: &Connection,
    playlist: PlaylistId,
    old: &[Entry],
    new: &[Entry],
    now: &str,
) -> Result<()> {
    if old == new {
        return Ok(());
    }
    let k = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    c.prepare_cached("DELETE FROM playlist_track WHERE playlist_id = ?1 AND position >= ?2")?
        .execute(params![playlist, to_i64(k)])?;
    let mut insert = c.prepare_cached(
        "INSERT INTO playlist_track (playlist_id, track_id, position, item_kind, label)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;
    for (i, e) in new.iter().enumerate().skip(k) {
        let (track, kind, label) = match e {
            Entry::Track(t) => (Some(*t), "track", None),
            Entry::Divider(l) => (None, "divider", Some(l.as_str())),
        };
        insert.execute(params![playlist, track, to_i64(i), kind, label])?;
    }
    c.prepare_cached("UPDATE playlist SET updated_at = ?2 WHERE id = ?1")?
        .execute(params![playlist, now])?;
    Ok(())
}

/// Trimmed, non-empty, no control characters (the track-edit text rules).
pub(crate) fn valid_name(name: &str) -> Result<String> {
    match track::text("name", Some(name), false)? {
        Value::Text(s) => Ok(s),
        _ => Err(Error::InvalidField {
            field: "name",
            reason: "must not be empty".into(),
        }),
    }
}

/// A divider label: the playlist-name rules, at most
/// `MAX_DIVIDER_LABEL_CHARS` characters.
fn valid_label(label: &str) -> Result<String> {
    let label = match track::text("label", Some(label), false)? {
        Value::Text(s) => s,
        _ => {
            return Err(Error::InvalidField {
                field: "label",
                reason: "must not be empty".into(),
            })
        }
    };
    if label.chars().count() > MAX_DIVIDER_LABEL_CHARS {
        return Err(Error::InvalidField {
            field: "label",
            reason: format!("longer than {MAX_DIVIDER_LABEL_CHARS} characters"),
        });
    }
    Ok(label)
}

/// An insertion index into a list of `len`: `0..=len`.
pub(crate) fn index(i: u32, len: usize) -> Result<usize> {
    let at = usize::try_from(i).unwrap_or(usize::MAX);
    if at > len {
        return Err(Error::IndexOutOfRange {
            index: i,
            len: to_u32(len),
        });
    }
    Ok(at)
}

/// An entry position: `0..len`.
fn entry_index(p: u32, len: usize) -> Result<usize> {
    match index(p, len)? {
        at if at < len => Ok(at),
        _ => Err(Error::IndexOutOfRange {
            index: p,
            len: to_u32(len),
        }),
    }
}

/// Entry positions, each `< len`; repeats collapse.
fn entry_set(positions: &[u32], len: usize) -> Result<BTreeSet<usize>> {
    positions.iter().map(|&p| entry_index(p, len)).collect()
}

pub(crate) fn to_i64(n: usize) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

pub(crate) fn to_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}
