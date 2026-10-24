//! My Tags ↔ Comments (D29, D36): the `/* Tag A / Tag B */` block rekordbox's
//! "Add My Tag to Comments" ends Comments with. Reading it into tags (the XML
//! import's option and [`Library::comment_tags_import`] over the library) and,
//! with the [`crate::settings::TAGS_IN_COMMENTS`] setting on, writing each
//! track's tags back into it in the same format.

use std::collections::HashSet;

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use unicode_normalization::UnicodeNormalization;

use crate::db::Library;
use crate::error::Result;
use crate::scan::TrackId;
use crate::settings;
use crate::tag::{self, TagId};
use crate::timefmt::now_utc;

/// The category that tags read from Comments are created in.
pub const IMPORTED_TAG_CATEGORY: &str = "Imported";

/// What reading tags from Comments would do, before anything is written.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentTagsPreview {
    /// Tracks whose comment ends with a tag block.
    pub tracks: usize,
    /// Distinct tag names (case-insensitive).
    pub names: usize,
    /// Names no tag in any category has yet.
    pub new_names: usize,
}

/// What reading tags from Comments did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentTagsReport {
    /// Tracks whose comment ends with a tag block.
    pub tracks: usize,
    /// Tags created under [`IMPORTED_TAG_CATEGORY`].
    pub created: usize,
    /// Track–tag links added.
    pub assigned: usize,
    /// Every tag named in a block (created or already there), ascending: what
    /// the organizer shows after the run.
    pub tag_ids: Vec<TagId>,
}

impl Library {
    /// Counts the tag blocks in the library's comments.
    pub fn comment_tags_preview(&self) -> Result<CommentTagsPreview> {
        let conn = self.conn()?;
        let comments = comments(&conn, None)?;
        preview(&conn, comments.iter().map(|(_, c)| c.as_str()))
    }

    /// Reads the tag block of every track's comment into tags, in one
    /// transaction. With [`settings::TAGS_IN_COMMENTS`] on, the blocks of
    /// tagged tracks are then rewritten from their tags.
    pub fn comment_tags_import(&self) -> Result<CommentTagsReport> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        let report = import_tags(&tx, None)?;
        sync_tagged(&tx, None)?;
        tx.commit()?;
        Ok(report)
    }

    /// Rewrites the tag block of every track that has tags (`apply` false:
    /// only counts). Tracks without tags keep their comment, so a block that
    /// was never imported is not lost. Returns the comments changed.
    pub fn comment_tags_sync_all(&self, apply: bool) -> Result<u32> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        let ids = tagged(&tx, None)?;
        let n = rewrite(&tx, &ids, apply)?;
        tx.commit()?;
        Ok(n)
    }
}

/// rekordbox's block ends Comments, after any text already there (checked
/// against a rekordbox 6.8.6 `collection.xml`, 2026-10-21, D29). Names are
/// split on ` / ` only, so `Deep/minimal House` is one tag, and NFC-normalised
/// so a decomposed name from a file tag matches the composed one. Returns the
/// names, in order, without duplicates (case-insensitive).
pub fn parse_comment_tags(comment: &str) -> Vec<String> {
    let Some((_, inner)) = split_block(comment) else {
        return Vec::new();
    };
    let mut out: Vec<String> = Vec::new();
    for name in inner.split(" / ") {
        let name: String = name.trim().nfc().collect();
        if !name.is_empty() && !out.iter().any(|n| eq_nocase(n, &name)) {
            out.push(name);
        }
    }
    out
}

/// `comment` with its tag block replaced by `names` in rekordbox's format:
/// `"{text} /* A / B */"`, with the leading space even when there is no text,
/// as rekordbox 6.8.6 writes it. No names removes the block; `None` when
/// nothing is left.
pub fn with_tag_block(comment: Option<&str>, names: &[&str]) -> Option<String> {
    let comment = comment.unwrap_or("");
    let text = split_block(comment)
        .map_or(comment, |(before, _)| before)
        .trim_end();
    if names.is_empty() {
        return (!text.is_empty()).then(|| text.to_owned());
    }
    Some(format!("{text} /* {} */", names.join(" / ")))
}

/// The text before a closing `/* … */` and the block's inside.
fn split_block(comment: &str) -> Option<(&str, &str)> {
    let rest = comment.trim_end().strip_suffix("*/")?;
    let start = rest.rfind("/*")?;
    Some((&rest[..start], &rest[start + 2..]))
}

fn eq_nocase(a: &str, b: &str) -> bool {
    a.chars()
        .flat_map(char::to_lowercase)
        .eq(b.chars().flat_map(char::to_lowercase))
}

/// Counts what [`import_tags`] would do with these comments.
pub(crate) fn preview<'a>(
    c: &Connection,
    comments: impl Iterator<Item = &'a str>,
) -> Result<CommentTagsPreview> {
    let mut p = CommentTagsPreview::default();
    let mut names: Vec<String> = Vec::new();
    for comment in comments {
        let found = parse_comment_tags(comment);
        p.tracks += usize::from(!found.is_empty());
        for name in found {
            if !names.iter().any(|n| eq_nocase(n, &name)) {
                names.push(name);
            }
        }
    }
    p.names = names.len();
    for name in &names {
        p.new_names += usize::from(tag::find_by_name(c, name)?.is_none());
    }
    Ok(p)
}

/// Tags named in the comments of `ids` (`None`: every track): an existing tag
/// of that name (any category) or a new one under [`IMPORTED_TAG_CATEGORY`].
/// Comments are not changed.
pub(crate) fn import_tags(c: &Connection, ids: Option<&[TrackId]>) -> Result<CommentTagsReport> {
    let now = now_utc();
    let mut report = CommentTagsReport::default();
    let mut category: Option<TagId> = None;
    for (track, comment) in comments(c, ids)? {
        let names = parse_comment_tags(&comment);
        if names.is_empty() {
            continue;
        }
        report.tracks += 1;
        let mut tag_ids = Vec::with_capacity(names.len());
        for name in &names {
            let tag_id = match tag::find_by_name(c, name)? {
                Some(g) => g,
                None => {
                    let cat = match category {
                        Some(cat) => cat,
                        None => *category.insert(tag::find_or_create_category(
                            c,
                            IMPORTED_TAG_CATEGORY,
                            &now,
                        )?),
                    };
                    report.created += 1;
                    tag::create_tag(c, cat, name, &now)?
                }
            };
            tag_ids.push(tag_id);
        }
        report.assigned += usize::try_from(tag::assign(c, &[track], &tag_ids)?).unwrap_or(0);
        report.tag_ids.extend(tag_ids);
    }
    report.tag_ids.sort_unstable();
    report.tag_ids.dedup();
    Ok(report)
}

/// With the setting on, rewrites the blocks of `ids` from their tags (a track
/// with no tags left loses its block). For tracks whose tags an edit changed.
pub(crate) fn sync_comments(c: &Connection, ids: &[TrackId]) -> Result<u32> {
    if ids.is_empty() || !settings::tags_in_comments(c)? {
        return Ok(0);
    }
    rewrite(c, ids, true)
}

/// With the setting on, rewrites the blocks of the tracks among `ids`
/// (`None`: every track) that have tags; the others keep their comment.
pub(crate) fn sync_tagged(c: &Connection, ids: Option<&[TrackId]>) -> Result<u32> {
    if !settings::tags_in_comments(c)? {
        return Ok(0);
    }
    rewrite(c, &tagged(c, ids)?, true)
}

/// Sets each track's block to its tags in display order; only rows that
/// change are written (`apply` false: only counted).
fn rewrite(c: &Connection, ids: &[TrackId], apply: bool) -> Result<u32> {
    let mut names = c.prepare_cached(
        "SELECT g.name FROM track_tag tt
         JOIN tag g ON g.id = tt.tag_id JOIN tag cat ON cat.id = g.parent_id
         WHERE tt.track_id = ?1 ORDER BY cat.sort_order, cat.id, g.sort_order, g.id",
    )?;
    let mut current = c.prepare_cached("SELECT comment FROM track WHERE id = ?1")?;
    let mut write =
        c.prepare_cached("UPDATE track SET comment = ?2, updated_at = ?3 WHERE id = ?1")?;
    let now = now_utc();
    let mut changed = 0;
    for &id in ids {
        let Some(comment) = current
            .query_row([id], |r| r.get::<_, Option<String>>(0))
            .optional()?
        else {
            continue;
        };
        let tags: Vec<String> = names
            .query_map([id], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        let tags: Vec<&str> = tags.iter().map(String::as_str).collect();
        let new = with_tag_block(comment.as_deref(), &tags);
        if new != comment {
            changed += 1;
            if apply {
                write.execute(params![id, new, now])?;
            }
        }
    }
    Ok(changed)
}

/// Tracks among `ids` (`None`: every track) that have at least one tag.
fn tagged(c: &Connection, ids: Option<&[TrackId]>) -> Result<Vec<TrackId>> {
    let mut all: Vec<TrackId> = c
        .prepare_cached("SELECT DISTINCT track_id FROM track_tag ORDER BY track_id")?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    if let Some(ids) = ids {
        let wanted: HashSet<TrackId> = ids.iter().copied().collect();
        all.retain(|t| wanted.contains(t));
    }
    Ok(all)
}

/// `(track, comment)` for `ids` (`None`: every track) that have a comment.
fn comments(c: &Connection, ids: Option<&[TrackId]>) -> Result<Vec<(TrackId, String)>> {
    match ids {
        None => Ok(c
            .prepare_cached("SELECT id, comment FROM track WHERE comment IS NOT NULL ORDER BY id")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?),
        Some(ids) => {
            let mut stmt = c.prepare_cached("SELECT comment FROM track WHERE id = ?1")?;
            let mut out = Vec::with_capacity(ids.len());
            for &id in ids {
                if let Some(Some(comment)) = stmt
                    .query_row([id], |r| r.get::<_, Option<String>>(0))
                    .optional()?
                {
                    out.push((id, comment));
                }
            }
            Ok(out)
        }
    }
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

    #[test]
    fn names_are_nfc_and_compared_without_case() {
        // "Café" with a combining acute, then precomposed and upper case.
        assert_eq!(
            parse_comment_tags(" /* Cafe\u{301} / CAFÉ / Ägäis */"),
            ["Café", "Ägäis"]
        );
    }

    #[test]
    fn blocks_are_written_in_rekordbox_format() {
        assert_eq!(
            with_tag_block(None, &["Deep", "Peak"]).as_deref(),
            Some(" /* Deep / Peak */")
        );
        assert_eq!(
            with_tag_block(Some("great  "), &["Deep"]).as_deref(),
            Some("great /* Deep */")
        );
        // An old block is replaced, the text before it kept.
        assert_eq!(
            with_tag_block(Some("Visit x /* Old / Gone */"), &["New"]).as_deref(),
            Some("Visit x /* New */")
        );
        // No tags: the block goes, and with it an empty comment.
        assert_eq!(
            with_tag_block(Some("Visit x /* Old */"), &[]).as_deref(),
            Some("Visit x")
        );
        assert_eq!(with_tag_block(Some(" /* Old */"), &[]), None);
        assert_eq!(
            with_tag_block(Some("no block"), &[]).as_deref(),
            Some("no block")
        );
    }

    #[test]
    fn written_blocks_parse_back() {
        let names = ["Tech House", "Deep/minimal House", "G 3"];
        for text in [None, Some("text"), Some("text /* stale */")] {
            let written = with_tag_block(text, &names).unwrap();
            assert_eq!(parse_comment_tags(&written), names);
            assert_eq!(with_tag_block(Some(&written), &names).unwrap(), written);
        }
    }
}
