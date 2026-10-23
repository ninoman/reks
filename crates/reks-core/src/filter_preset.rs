//! Saved filters (D35): named rule trees the Filters drawer loads
//! (`Library::filter_presets`) and their edits
//! (`Library::filter_preset_mutate`).
//!
//! Every op is validated before anything is written and runs in one
//! transaction; positions stay dense (`0..n`). Names are unique
//! case-insensitively (ASCII, as tag names).
//!
//! References work as in smart playlists (D29): tag and playlist ids are kept
//! as saved, and deleting a tag or playlist does not touch the presets. Such
//! an id still loads; in a query it matches no track. The UI marks a row
//! whose ids are all gone as unfinished ("Tag deleted") and leaves it out.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::Library;
use crate::error::{Error, Result};
use crate::playlist::{index, to_i64, valid_name};
use crate::rule::RuleNode;
use crate::timefmt::now_utc;

/// Row id in the `filter_preset` table.
pub type FilterPresetId = i64;

/// A saved filter, as listed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterPreset {
    pub id: FilterPresetId,
    pub name: String,
    /// `None`: the stored rule does not parse (written by a newer reks). It
    /// can still be renamed, moved, duplicated, deleted or given a new rule.
    pub rule: Option<RuleNode>,
    pub created_at: String,
    pub updated_at: String,
}

/// An edit of the saved filters. `position` is an index in the list as it
/// is now: `0..=len` (`None` = last).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum FilterPresetOp {
    Create {
        name: String,
        rule: RuleNode,
        #[serde(default)]
        position: Option<u32>,
    },
    Rename {
        id: FilterPresetId,
        name: String,
    },
    /// Replaces the rule (the drawer's Update).
    SetRule {
        id: FilterPresetId,
        rule: RuleNode,
    },
    /// A copy right after the original, named "<name> copy" (or "copy 2", …).
    Duplicate {
        id: FilterPresetId,
    },
    Delete {
        id: FilterPresetId,
    },
    Move {
        id: FilterPresetId,
        position: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterPresetOutcome {
    /// The new preset, for create and duplicate.
    pub created_id: Option<FilterPresetId>,
}

impl Library {
    /// Every saved filter, in display order.
    pub fn filter_presets(&self) -> Result<Vec<FilterPreset>> {
        list(&*self.conn()?)
    }

    /// Applies `op` in one transaction. A rejected op changes nothing.
    pub fn filter_preset_mutate(&self, op: FilterPresetOp) -> Result<FilterPresetOutcome> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        let now = now_utc();
        let created_id = match op {
            FilterPresetOp::Create {
                name,
                rule,
                position,
            } => Some(create(&tx, &name, &rule_json(&rule)?, position, &now)?),
            FilterPresetOp::Rename { id, name } => {
                rename(&tx, id, &name, &now)?;
                None
            }
            FilterPresetOp::SetRule { id, rule } => {
                let rule = rule_json(&rule)?;
                require_exists(&tx, id)?;
                tx.prepare_cached(
                    "UPDATE filter_preset SET rule = ?2, updated_at = ?3
                     WHERE id = ?1 AND rule <> ?2",
                )?
                .execute(params![id, rule, now])?;
                None
            }
            FilterPresetOp::Duplicate { id } => Some(duplicate(&tx, id, &now)?),
            FilterPresetOp::Delete { id } => {
                require_exists(&tx, id)?;
                tx.prepare_cached("DELETE FROM filter_preset WHERE id = ?1")?
                    .execute([id])?;
                write_order(&tx, &order(&tx)?, &now)?;
                None
            }
            FilterPresetOp::Move { id, position } => {
                move_to(&tx, id, position, &now)?;
                None
            }
        };
        tx.commit()?;
        Ok(FilterPresetOutcome { created_id })
    }
}

fn list(c: &Connection) -> Result<Vec<FilterPreset>> {
    let mut stmt = c.prepare_cached(
        "SELECT id, name, rule, created_at, updated_at FROM filter_preset
         ORDER BY position, id",
    )?;
    let rows = stmt.query_map([], |r| {
        let id: FilterPresetId = r.get(0)?;
        let rule: String = r.get(2)?;
        Ok(FilterPreset {
            id,
            name: r.get(1)?,
            rule: serde_json::from_str(&rule)
                .inspect_err(|e| {
                    tracing::warn!(preset = id, error = %e, "saved filter rule does not parse");
                })
                .ok(),
            created_at: r.get(3)?,
            updated_at: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// A rule within the size limits, as stored JSON.
fn rule_json(rule: &RuleNode) -> Result<String> {
    rule.validate()?;
    serde_json::to_string(rule).map_err(|e| Error::InvalidField {
        field: "rule",
        reason: e.to_string(),
    })
}

/// Inserts a preset with an already checked `rule` JSON.
fn create(
    c: &Connection,
    name: &str,
    rule: &str,
    position: Option<u32>,
    now: &str,
) -> Result<FilterPresetId> {
    let name = valid_name(name)?;
    require_unique(c, &name, None)?;
    let mut ids = order(c)?;
    let at = match position {
        Some(p) => index(p, ids.len())?,
        None => ids.len(),
    };
    c.prepare_cached(
        "INSERT INTO filter_preset (name, position, rule, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?4)",
    )?
    .execute(params![name, to_i64(at), rule, now])?;
    let id = c.last_insert_rowid();
    ids.insert(at, id);
    write_order(c, &ids, now)?;
    Ok(id)
}

fn rename(c: &Connection, id: FilterPresetId, name: &str, now: &str) -> Result<()> {
    let name = valid_name(name)?;
    require_exists(c, id)?;
    require_unique(c, &name, Some(id))?;
    c.prepare_cached(
        "UPDATE filter_preset SET name = ?2, updated_at = ?3 WHERE id = ?1 AND name <> ?2",
    )?
    .execute(params![id, name, now])?;
    Ok(())
}

/// The stored rule is copied as it is, so one that does not parse copies too.
fn duplicate(c: &Connection, id: FilterPresetId, now: &str) -> Result<FilterPresetId> {
    let (name, rule): (String, String) = c
        .prepare_cached("SELECT name, rule FROM filter_preset WHERE id = ?1")?
        .query_row([id], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?
        .ok_or(Error::FilterPresetNotFound(id))?;
    let mut n = 1;
    let copy = loop {
        let candidate = copy_name(&name, n);
        if !is_taken(c, &candidate, None)? {
            break candidate;
        }
        n += 1;
    };
    let after = order(c)?.iter().position(|&p| p == id).unwrap_or(0) + 1;
    create(
        c,
        &copy,
        &rule,
        Some(u32::try_from(after).unwrap_or(u32::MAX)),
        now,
    )
}

/// "<name> copy", then "<name> copy 2", "<name> copy 3", …
fn copy_name(name: &str, n: u32) -> String {
    if n == 1 {
        format!("{name} copy")
    } else {
        format!("{name} copy {n}")
    }
}

fn move_to(c: &Connection, id: FilterPresetId, position: u32, now: &str) -> Result<()> {
    let mut ids = order(c)?;
    let mut at = index(position, ids.len())?;
    let i = ids
        .iter()
        .position(|&p| p == id)
        .ok_or(Error::FilterPresetNotFound(id))?;
    ids.remove(i);
    if i < at {
        at -= 1;
    }
    ids.insert(at, id);
    write_order(c, &ids, now)
}

fn require_exists(c: &Connection, id: FilterPresetId) -> Result<()> {
    c.prepare_cached("SELECT 1 FROM filter_preset WHERE id = ?1")?
        .query_row([id], |_| Ok(()))
        .optional()?
        .ok_or(Error::FilterPresetNotFound(id))
}

/// No preset other than `except` is called `name` (any case).
fn require_unique(c: &Connection, name: &str, except: Option<FilterPresetId>) -> Result<()> {
    if is_taken(c, name, except)? {
        return Err(Error::FilterPresetNameTaken(name.to_owned()));
    }
    Ok(())
}

fn is_taken(c: &Connection, name: &str, except: Option<FilterPresetId>) -> Result<bool> {
    Ok(c.prepare_cached(
        "SELECT 1 FROM filter_preset WHERE name = ?1 COLLATE NOCASE AND id IS NOT ?2",
    )?
    .query_row(params![name, except], |_| Ok(()))
    .optional()?
    .is_some())
}

/// Every id in display order.
fn order(c: &Connection) -> Result<Vec<FilterPresetId>> {
    Ok(
        c.prepare_cached("SELECT id FROM filter_preset ORDER BY position, id")?
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?,
    )
}

/// Sets `position` to the index in `ids`, touching only rows that change.
fn write_order(c: &Connection, ids: &[FilterPresetId], now: &str) -> Result<()> {
    let mut stmt = c.prepare_cached(
        "UPDATE filter_preset SET position = ?2, updated_at = ?3
         WHERE id = ?1 AND position <> ?2",
    )?;
    for (i, id) in ids.iter().enumerate() {
        stmt.execute(params![id, to_i64(i), now])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_names_count_up() {
        assert_eq!(copy_name("Peak", 1), "Peak copy");
        assert_eq!(copy_name("Peak", 2), "Peak copy 2");
    }
}
