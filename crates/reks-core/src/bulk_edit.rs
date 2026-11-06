//! Bulk metadata edit (`Library::bulk_update_tracks`): one patch applied to
//! many tracks in a single transaction (phase 1, task 9 follow-up).
//!
//! Only fields that are safe to set on many tracks at once are offered: not
//! title, BPM, grid, cues or path. Field rules are `track::validate`'s, so a
//! bulk edit accepts exactly what the single-track form accepts. My Tags are
//! added or removed, never replaced.

use std::collections::BTreeSet;

use rusqlite::types::Value;
use rusqlite::{params_from_iter, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::backup::BackupReason;
use crate::db::Library;
use crate::error::{Error, Result};
use crate::query::{fill_lists, track_row, TrackRow, ROW_COLUMNS};
use crate::scan::TrackId;
use crate::tag::{self, TagId};
use crate::timefmt::now_utc;
use crate::track::{self, TrackPatch};

/// How a comment edit treats each track's existing comment.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(
    tag = "mode",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum CommentEdit {
    /// Every track gets this comment; `None` or empty text clears it.
    Replace { text: Option<String> },
    /// `text` goes after each track's comment, one space between them.
    Append { text: String },
}

/// A bulk edit. An absent field is left as it is; `null` clears it, as in
/// `TrackPatch`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BulkPatch {
    #[serde(default, deserialize_with = "track::nullable")]
    pub artist: Option<Option<String>>,
    #[serde(default, deserialize_with = "track::nullable")]
    pub album: Option<Option<String>>,
    #[serde(default, deserialize_with = "track::nullable")]
    pub genre: Option<Option<String>>,
    #[serde(default, deserialize_with = "track::nullable")]
    pub label: Option<Option<String>>,
    #[serde(default, deserialize_with = "track::nullable")]
    pub year: Option<Option<i64>>,
    #[serde(default)]
    pub rating: Option<i64>,
    #[serde(default, deserialize_with = "track::nullable")]
    pub color: Option<Option<String>>,
    #[serde(default, deserialize_with = "track::nullable")]
    pub key: Option<Option<String>>,
    #[serde(default)]
    pub comment: Option<CommentEdit>,
    /// Tags to link to every track; links that exist are kept.
    #[serde(default)]
    pub add_tags: Vec<TagId>,
    /// Tags to unlink from every track.
    #[serde(default)]
    pub remove_tags: Vec<TagId>,
}

/// What a bulk edit did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkOutcome {
    /// The edited tracks as the table shows them, in id order.
    pub rows: Vec<TrackRow>,
    pub tags_added: u32,
    pub tags_removed: u32,
}

/// The comment edit after validation.
enum Planned {
    Replace(Value),
    Append(String),
}

impl BulkPatch {
    /// The plain fields as a `TrackPatch`, which owns their validation.
    fn scalars(&self) -> TrackPatch {
        TrackPatch {
            artist: self.artist.clone(),
            album: self.album.clone(),
            genre: self.genre.clone(),
            label: self.label.clone(),
            year: self.year,
            rating: self.rating,
            color: self.color.clone(),
            key: self.key.clone(),
            ..TrackPatch::default()
        }
    }
}

impl Library {
    /// Applies `patch` to every track in one transaction and sets their
    /// `updated_at`. Everything is checked before and while writing; any
    /// error (`InvalidField`, `TrackNotFound`, `NotATag`) changes nothing.
    /// Duplicate ids count once; an empty patch writes nothing.
    pub fn bulk_update_tracks(
        &self,
        track_ids: &[TrackId],
        patch: &BulkPatch,
    ) -> Result<BulkOutcome> {
        let scalars = track::validate(&patch.scalars())?;
        if let Some(t) = patch
            .add_tags
            .iter()
            .find(|t| patch.remove_tags.contains(t))
        {
            return Err(Error::InvalidField {
                field: "tags",
                reason: format!("tag {t} is both added and removed"),
            });
        }
        let comment = match &patch.comment {
            None => None,
            Some(CommentEdit::Replace { text }) => Some(Planned::Replace(track::text(
                "comment",
                text.as_deref(),
                true,
            )?)),
            // Appending nothing leaves comments alone.
            Some(CommentEdit::Append { text }) => match track::text("comment", Some(text), true)? {
                Value::Text(add) => Some(Planned::Append(add)),
                _ => None,
            },
        };
        let ids: BTreeSet<TrackId> = track_ids.iter().copied().collect();
        let ids: Vec<TrackId> = ids.into_iter().collect();

        self.snapshot_before(BackupReason::BulkEdit);
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        let now = now_utc();
        for &id in &ids {
            let old: Option<Option<String>> = tx
                .query_row("SELECT comment FROM track WHERE id = ?1", [id], |r| {
                    r.get(0)
                })
                .optional()?;
            let Some(old) = old else {
                return Err(Error::TrackNotFound(id));
            };
            let mut sets = scalars.clone();
            match &comment {
                None => {}
                Some(Planned::Replace(value)) => sets.push(("comment", value.clone())),
                Some(Planned::Append(add)) => {
                    let joined = match old.as_deref().map(str::trim) {
                        Some(old) if !old.is_empty() => format!("{old} {add}"),
                        _ => add.clone(),
                    };
                    // Re-checked: the joined text can exceed the length limit.
                    sets.push(("comment", track::text("comment", Some(&joined), true)?));
                }
            }
            if sets.is_empty() {
                continue;
            }
            // Column names come from `validate`'s fixed list; values are bound.
            let cols: Vec<String> = sets.iter().map(|(c, _)| format!("{c} = ?")).collect();
            let sql = format!(
                "UPDATE track SET {}, updated_at = ? WHERE id = ?",
                cols.join(", ")
            );
            let mut params: Vec<Value> = sets.into_iter().map(|(_, v)| v).collect();
            params.push(Value::Text(now.clone()));
            params.push(Value::Integer(id));
            tx.prepare_cached(&sql)?
                .execute(params_from_iter(&params))?;
        }
        let tags_added = tag::assign(&tx, &ids, &patch.add_tags)?;
        let tags_removed = tag::unassign(&tx, &ids, &patch.remove_tags)?;

        let mut rows = Vec::with_capacity(ids.len());
        {
            let mut stmt = tx.prepare_cached(&format!(
                "SELECT {ROW_COLUMNS} FROM track t WHERE t.id = ?1"
            ))?;
            for &id in &ids {
                rows.push(stmt.query_row([id], track_row)?);
            }
        }
        fill_lists(&tx, &mut rows)?;
        tx.commit()?;
        Ok(BulkOutcome {
            rows,
            tags_added,
            tags_removed,
        })
    }
}
