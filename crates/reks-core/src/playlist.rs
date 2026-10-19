//! The playlist tree (`Library::playlists`) and its edits
//! (`Library::playlist_mutate`).
//!
//! Rules: `docs/phases/phase-1.md` (task 10 notes). Every op is validated
//! before anything is written and runs in one transaction. Entry positions
//! and sibling `sort_order`s stay dense (`0..n`).

use std::collections::{BTreeSet, HashSet};

use rusqlite::types::Value;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::Library;
use crate::error::{Error, Result};
use crate::query::{playlist_nodes, PlaylistId, PlaylistNode};
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
}

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
        };
        tx.commit()?;
        Ok(outcome)
    }
}

fn create(
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
    require_playlist(c, playlist)?;
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
    let mut seen: HashSet<TrackId> = current.iter().copied().collect();
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
    new.splice(at..at, add.iter().copied());
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
    require_playlist(c, playlist)?;
    let current = entries(c, playlist)?;
    let drop = entry_set(positions, current.len())?;
    let new: Vec<TrackId> = current
        .iter()
        .enumerate()
        .filter(|(i, _)| !drop.contains(i))
        .map(|(_, &t)| t)
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
    require_playlist(c, playlist)?;
    let current = entries(c, playlist)?;
    let moved = entry_set(positions, current.len())?;
    let before = index(before, current.len())?;
    let block: Vec<TrackId> = moved.iter().map(|&i| current[i]).collect();
    let mut new: Vec<TrackId> = current
        .iter()
        .enumerate()
        .filter(|(i, _)| !moved.contains(i))
        .map(|(_, &t)| t)
        .collect();
    let at = before - moved.range(..before).count();
    new.splice(at..at, block);
    write_entries(c, playlist, &current, &new, now)?;
    Ok(PlaylistOutcome::done())
}

struct Node {
    parent_id: Option<PlaylistId>,
    is_folder: bool,
}

fn node(c: &Connection, id: PlaylistId) -> Result<Node> {
    c.prepare_cached("SELECT parent_id, is_folder FROM playlist WHERE id = ?1")?
        .query_row([id], |r| {
            Ok(Node {
                parent_id: r.get(0)?,
                is_folder: r.get(1)?,
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

fn require_playlist(c: &Connection, id: PlaylistId) -> Result<()> {
    if node(c, id)?.is_folder {
        return Err(Error::NotAPlaylist(id));
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

fn entries(c: &Connection, playlist: PlaylistId) -> Result<Vec<TrackId>> {
    Ok(c.prepare_cached(
        "SELECT track_id FROM playlist_track WHERE playlist_id = ?1 ORDER BY position",
    )?
    .query_map([playlist], |r| r.get(0))?
    .collect::<rusqlite::Result<_>>()?)
}

/// Replaces `old` with `new`. Rows from the first difference on are deleted
/// and re-inserted, so positions stay `0..n` and the `(playlist_id, position)`
/// key never sees two rows at one position.
fn write_entries(
    c: &Connection,
    playlist: PlaylistId,
    old: &[TrackId],
    new: &[TrackId],
    now: &str,
) -> Result<()> {
    if old == new {
        return Ok(());
    }
    let k = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    c.prepare_cached("DELETE FROM playlist_track WHERE playlist_id = ?1 AND position >= ?2")?
        .execute(params![playlist, to_i64(k)])?;
    let mut insert = c.prepare_cached(
        "INSERT INTO playlist_track (playlist_id, track_id, position) VALUES (?1, ?2, ?3)",
    )?;
    for (i, t) in new.iter().enumerate().skip(k) {
        insert.execute(params![playlist, t, to_i64(i)])?;
    }
    c.prepare_cached("UPDATE playlist SET updated_at = ?2 WHERE id = ?1")?
        .execute(params![playlist, now])?;
    Ok(())
}

/// Trimmed, non-empty, no control characters (the track-edit text rules).
fn valid_name(name: &str) -> Result<String> {
    match track::text("name", Some(name), false)? {
        Value::Text(s) => Ok(s),
        _ => Err(Error::InvalidField {
            field: "name",
            reason: "must not be empty".into(),
        }),
    }
}

/// An insertion index into a list of `len`: `0..=len`.
fn index(i: u32, len: usize) -> Result<usize> {
    let at = usize::try_from(i).unwrap_or(usize::MAX);
    if at > len {
        return Err(Error::IndexOutOfRange {
            index: i,
            len: to_u32(len),
        });
    }
    Ok(at)
}

/// Entry positions, each `< len`; repeats collapse.
fn entry_set(positions: &[u32], len: usize) -> Result<BTreeSet<usize>> {
    positions
        .iter()
        .map(|&p| match index(p, len)? {
            at if at < len => Ok(at),
            _ => Err(Error::IndexOutOfRange {
                index: p,
                len: to_u32(len),
            }),
        })
        .collect()
}

fn to_i64(n: usize) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

fn to_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}
