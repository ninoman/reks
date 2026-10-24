//! My Tags (D29): the tag tree (`Library::tags`) and its edits
//! (`Library::tag_mutate`).
//!
//! Two levels, as in rekordbox: categories, and tags below a category. Only
//! tags are assigned to tracks. Every op is validated before anything is
//! written and runs in one transaction; sibling `sort_order`s stay dense.

use std::collections::BTreeSet;

use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::Library;
use crate::error::{Error, Result};
use crate::playlist::{index, to_i64, to_u32, valid_name};
use crate::scan::TrackId;
use crate::timefmt::now_utc;

/// Row id in the `tag` table (a category or a tag).
pub type TagId = i64;

/// Every category with its tags, both in display order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagTree {
    pub categories: Vec<TagCategory>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagCategory {
    pub id: TagId,
    pub name: String,
    pub tags: Vec<TagInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagInfo {
    pub id: TagId,
    pub name: String,
    /// Tracks that have this tag.
    pub track_count: u32,
}

/// An edit of the tag tree or of track tags. `position` is an index among
/// the siblings as they are now: `0..=len` (`None` = last).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum TagOp {
    CreateCategory {
        name: String,
        #[serde(default)]
        position: Option<u32>,
    },
    CreateTag {
        category_id: TagId,
        name: String,
        #[serde(default)]
        position: Option<u32>,
    },
    /// A category or a tag.
    Rename { id: TagId, name: String },
    /// A category takes its tags with it; links to tracks go too.
    Delete { id: TagId },
    /// A category: `category_id` `None`. A tag: the category to move it to
    /// (its own, or another).
    Move {
        id: TagId,
        #[serde(default)]
        category_id: Option<TagId>,
        position: u32,
    },
    /// Links every track to every tag; existing links are kept.
    Assign {
        track_ids: Vec<TrackId>,
        tag_ids: Vec<TagId>,
    },
    Unassign {
        track_ids: Vec<TrackId>,
        tag_ids: Vec<TagId>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagOutcome {
    /// The new category or tag, for the create ops.
    pub created_id: Option<TagId>,
    /// Links added (assign) or removed (unassign).
    pub changed: u32,
}

impl Library {
    /// The tag tree, with a track count per tag.
    pub fn tags(&self) -> Result<TagTree> {
        tag_tree(&*self.conn()?)
    }

    /// Applies `op` in one transaction. A rejected op changes nothing.
    pub fn tag_mutate(&self, op: TagOp) -> Result<TagOutcome> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        let now = now_utc();
        let mut out = TagOutcome {
            created_id: None,
            changed: 0,
        };
        match op {
            TagOp::CreateCategory { name, position } => {
                out.created_id = Some(create(&tx, None, &name, position, &now)?);
            }
            TagOp::CreateTag {
                category_id,
                name,
                position,
            } => {
                require_category(&tx, category_id)?;
                out.created_id = Some(create(&tx, Some(category_id), &name, position, &now)?);
            }
            TagOp::Rename { id, name } => rename(&tx, id, &name, &now)?,
            TagOp::Delete { id } => {
                let parent = parent_of(&tx, id)?;
                tx.prepare_cached("DELETE FROM tag WHERE id = ?1")?
                    .execute([id])?;
                write_order(&tx, &siblings(&tx, parent)?, &now)?;
            }
            TagOp::Move {
                id,
                category_id,
                position,
            } => move_node(&tx, id, category_id, position, &now)?,
            TagOp::Assign { track_ids, tag_ids } => {
                out.changed = assign(&tx, &track_ids, &tag_ids)?;
            }
            TagOp::Unassign { track_ids, tag_ids } => {
                out.changed = unassign(&tx, &track_ids, &tag_ids)?;
            }
        }
        tx.commit()?;
        Ok(out)
    }
}

fn tag_tree(c: &Connection) -> Result<TagTree> {
    let mut categories: Vec<TagCategory> = c
        .prepare_cached("SELECT id, name FROM tag WHERE parent_id IS NULL ORDER BY sort_order, id")?
        .query_map([], |r| {
            Ok(TagCategory {
                id: r.get(0)?,
                name: r.get(1)?,
                tags: Vec::new(),
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    let mut stmt = c.prepare_cached(
        "SELECT g.parent_id, g.id, g.name,
                (SELECT count(*) FROM track_tag tt WHERE tt.tag_id = g.id)
         FROM tag g WHERE g.parent_id IS NOT NULL ORDER BY g.sort_order, g.id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, TagId>(0)?,
            TagInfo {
                id: r.get(1)?,
                name: r.get(2)?,
                track_count: r.get(3)?,
            },
        ))
    })?;
    for row in rows {
        let (parent, tag) = row?;
        if let Some(cat) = categories.iter_mut().find(|c| c.id == parent) {
            cat.tags.push(tag);
        }
    }
    Ok(TagTree { categories })
}

/// Inserts a category (`parent` `None`) or a tag; the caller checked `parent`.
fn create(
    c: &Connection,
    parent: Option<TagId>,
    name: &str,
    position: Option<u32>,
    now: &str,
) -> Result<TagId> {
    let name = valid_name(name)?;
    require_unique(c, parent, &name, None)?;
    let mut siblings = siblings(c, parent)?;
    let at = match position {
        Some(p) => index(p, siblings.len())?,
        None => siblings.len(),
    };
    c.prepare_cached(
        "INSERT INTO tag (parent_id, name, sort_order, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?4)",
    )?
    .execute(params![parent, name, to_i64(at), now])?;
    let id = c.last_insert_rowid();
    siblings.insert(at, id);
    write_order(c, &siblings, now)?;
    Ok(id)
}

fn rename(c: &Connection, id: TagId, name: &str, now: &str) -> Result<()> {
    let name = valid_name(name)?;
    let parent = parent_of(c, id)?;
    require_unique(c, parent, &name, Some(id))?;
    c.prepare_cached("UPDATE tag SET name = ?2, updated_at = ?3 WHERE id = ?1 AND name <> ?2")?
        .execute(params![id, name, now])?;
    Ok(())
}

fn move_node(
    c: &Connection,
    id: TagId,
    target: Option<TagId>,
    position: u32,
    now: &str,
) -> Result<()> {
    let parent = parent_of(c, id)?;
    match (parent, target) {
        (None, Some(_)) => {
            return Err(Error::InvalidField {
                field: "categoryId",
                reason: "a category cannot move into another category".into(),
            })
        }
        (Some(_), None) => {
            return Err(Error::InvalidField {
                field: "categoryId",
                reason: "a tag must stay in a category".into(),
            })
        }
        (Some(_), Some(cat)) => require_category(c, cat)?,
        (None, None) => {}
    }
    if target != parent {
        let name: String = c
            .prepare_cached("SELECT name FROM tag WHERE id = ?1")?
            .query_row([id], |r| r.get(0))?;
        require_unique(c, target, &name, Some(id))?;
    }
    let mut dest = siblings(c, target)?;
    let mut at = index(position, dest.len())?;
    if let Some(i) = dest.iter().position(|&s| s == id) {
        dest.remove(i);
        if i < at {
            at -= 1;
        }
    }
    dest.insert(at, id);
    if target != parent {
        c.prepare_cached("UPDATE tag SET parent_id = ?2, updated_at = ?3 WHERE id = ?1")?
            .execute(params![id, target, now])?;
        write_order(c, &siblings(c, parent)?, now)?;
    }
    write_order(c, &dest, now)
}

/// Links every track to every tag; returns the links added.
pub(crate) fn assign(c: &Connection, track_ids: &[TrackId], tag_ids: &[TagId]) -> Result<u32> {
    let (tracks, tags) = valid_links(c, track_ids, tag_ids)?;
    let mut stmt =
        c.prepare_cached("INSERT OR IGNORE INTO track_tag (track_id, tag_id) VALUES (?1, ?2)")?;
    let mut added = 0;
    for &t in &tracks {
        for &g in &tags {
            added += stmt.execute([t, g])?;
        }
    }
    Ok(to_u32(added))
}

/// Unlinks every track from every tag; returns the links removed.
pub(crate) fn unassign(c: &Connection, track_ids: &[TrackId], tag_ids: &[TagId]) -> Result<u32> {
    let (tracks, tags) = valid_links(c, track_ids, tag_ids)?;
    let mut stmt = c.prepare_cached("DELETE FROM track_tag WHERE track_id = ?1 AND tag_id = ?2")?;
    let mut removed = 0;
    for &t in &tracks {
        for &g in &tags {
            removed += stmt.execute([t, g])?;
        }
    }
    Ok(to_u32(removed))
}

/// Checks every id; returns them de-duplicated.
fn valid_links(
    c: &Connection,
    track_ids: &[TrackId],
    tag_ids: &[TagId],
) -> Result<(BTreeSet<TrackId>, BTreeSet<TagId>)> {
    let mut exists = c.prepare_cached("SELECT 1 FROM track WHERE id = ?1")?;
    for &t in track_ids {
        if exists.query_row([t], |_| Ok(())).optional()?.is_none() {
            return Err(Error::TrackNotFound(t));
        }
    }
    for &g in tag_ids {
        if parent_of(c, g)?.is_none() {
            return Err(Error::NotATag(g));
        }
    }
    Ok((
        track_ids.iter().copied().collect(),
        tag_ids.iter().copied().collect(),
    ))
}

/// The tag with this name (case-insensitive) in any category; the first in
/// display order if several categories have one.
pub(crate) fn find_by_name(c: &Connection, name: &str) -> Result<Option<TagId>> {
    Ok(c.prepare_cached(
        "SELECT g.id FROM tag g JOIN tag cat ON cat.id = g.parent_id
             WHERE g.name = ?1 COLLATE NOCASE ORDER BY cat.sort_order, g.sort_order LIMIT 1",
    )?
    .query_row([name.trim()], |r| r.get(0))
    .optional()?)
}

/// The category with this name, created at the end if there is none.
pub(crate) fn find_or_create_category(c: &Connection, name: &str, now: &str) -> Result<TagId> {
    let found: Option<TagId> = c
        .prepare_cached("SELECT id FROM tag WHERE parent_id IS NULL AND name = ?1 COLLATE NOCASE")?
        .query_row([name], |r| r.get(0))
        .optional()?;
    match found {
        Some(id) => Ok(id),
        None => create(c, None, name, None, now),
    }
}

/// A new tag at the end of `category`.
pub(crate) fn create_tag(c: &Connection, category: TagId, name: &str, now: &str) -> Result<TagId> {
    require_category(c, category)?;
    create(c, Some(category), name, None, now)
}

/// The parent of `id` (`None` for a category).
fn parent_of(c: &Connection, id: TagId) -> Result<Option<TagId>> {
    c.prepare_cached("SELECT parent_id FROM tag WHERE id = ?1")?
        .query_row([id], |r| r.get(0))
        .optional()?
        .ok_or(Error::TagNotFound(id))
}

fn require_category(c: &Connection, id: TagId) -> Result<()> {
    match parent_of(c, id)? {
        None => Ok(()),
        Some(_) => Err(Error::NotATagCategory(id)),
    }
}

/// No sibling under `parent` other than `except` is called `name` (any case).
fn require_unique(
    c: &Connection,
    parent: Option<TagId>,
    name: &str,
    except: Option<TagId>,
) -> Result<()> {
    let taken = c
        .prepare_cached(
            "SELECT 1 FROM tag WHERE parent_id IS ?1 AND name = ?2 COLLATE NOCASE
               AND id IS NOT ?3",
        )?
        .query_row(params![parent, name, except], |_| Ok(()))
        .optional()?
        .is_some();
    if taken {
        return Err(Error::InvalidField {
            field: "name",
            reason: format!("'{name}' already exists here"),
        });
    }
    Ok(())
}

fn siblings(c: &Connection, parent: Option<TagId>) -> Result<Vec<TagId>> {
    Ok(
        c.prepare_cached("SELECT id FROM tag WHERE parent_id IS ?1 ORDER BY sort_order, id")?
            .query_map([parent], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?,
    )
}

/// Sets `sort_order` to the index in `ids`, touching only rows that change.
fn write_order(c: &Connection, ids: &[TagId], now: &str) -> Result<()> {
    let mut stmt = c.prepare_cached(
        "UPDATE tag SET sort_order = ?2, updated_at = ?3 WHERE id = ?1 AND sort_order <> ?2",
    )?;
    for (i, id) in ids.iter().enumerate() {
        stmt.execute(params![id, to_i64(i), now])?;
    }
    Ok(())
}

/// The tags of these tracks, each track's in display order (category, then tag).
pub(crate) fn tags_of(c: &Connection, track_ids: &[TrackId]) -> Result<Vec<(TrackId, TagId)>> {
    if track_ids.is_empty() {
        return Ok(Vec::new());
    }
    let sql = format!(
        "SELECT tt.track_id, tt.tag_id FROM track_tag tt
         JOIN tag g ON g.id = tt.tag_id JOIN tag cat ON cat.id = g.parent_id
         WHERE tt.track_id IN ({})
         ORDER BY tt.track_id, cat.sort_order, cat.id, g.sort_order, g.id",
        vec!["?"; track_ids.len()].join(", ")
    );
    Ok(c.prepare(&sql)?
        .query_map(params_from_iter(track_ids), |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?)
}

/// rekordbox's "Add My Tag to Comments" ends Comments with the track's tags
/// as `/* Tag A / Tag B */`, after any text already there (checked against a
/// rekordbox 6.8.6 `collection.xml`, 2026-10-21, D29). Names are split on
/// ` / ` only, so `Deep/minimal House` is one tag. Returns the names, in
/// order, without duplicates.
pub fn parse_comment_tags(comment: &str) -> Vec<String> {
    let Some(rest) = comment.trim_end().strip_suffix("*/") else {
        return Vec::new();
    };
    let Some(start) = rest.rfind("/*") else {
        return Vec::new();
    };
    let mut out: Vec<String> = Vec::new();
    for name in rest[start + 2..].split(" / ") {
        let name = name.trim();
        if !name.is_empty() && !out.iter().any(|n| n.eq_ignore_ascii_case(name)) {
            out.push(name.to_owned());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comment_tags_are_parsed_from_the_closing_block() {
        // As rekordbox 6.8.6 writes them.
        assert_eq!(
            parse_comment_tags(" /* Psychodelic trance / Prog Trance / G 3 */"),
            ["Psychodelic trance", "Prog Trance", "G 3"]
        );
        assert_eq!(
            parse_comment_tags(
                "Visit https://x.bandcamp.com /* Tech House / Deep/minimal House */"
            ),
            ["Tech House", "Deep/minimal House"]
        );
        assert_eq!(parse_comment_tags("/*Deep*/  "), ["Deep"]);
        assert_eq!(
            parse_comment_tags(" /* Deep / deep /  / Peak */"),
            ["Deep", "Peak"]
        );
    }

    #[test]
    fn comments_without_a_closing_block_have_no_tags() {
        assert!(parse_comment_tags("").is_empty());
        assert!(parse_comment_tags("great track").is_empty());
        assert!(parse_comment_tags("/* Deep */ great track").is_empty());
        assert!(parse_comment_tags("never opened */").is_empty());
        assert!(parse_comment_tags(" /* */").is_empty());
    }
}
