//! One track in full (`Library::track`) and metadata edits
//! (`Library::update_track`) for the detail panel.
//!
//! Edit rules: `docs/phases/phase-1.md` (task 9 notes). Edits change the
//! library DB only; tags in the audio files are never written.

use rusqlite::types::Value;
use rusqlite::{params_from_iter, Connection, OptionalExtension};
use serde::{Deserialize, Deserializer, Serialize};

use crate::artwork::ArtworkId;
use crate::db::Library;
use crate::error::{Error, Result};
use crate::keys::Key;
use crate::query::{fill_tags, track_row, TrackRow, ROW_COLUMNS};
use crate::rbxml::TRACK_COLOURS;
use crate::scan::TrackId;
use crate::timefmt::now_utc;

/// Longest text value an edit accepts, in characters. Our own limit, not a
/// format one; it only guards against pasting a whole document into a field.
pub const MAX_TEXT_CHARS: usize = 1_024;

/// Every `track` column the detail panel shows. Serialises as the table
/// row's fields plus the rest, so it can replace a `TrackRow` in the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    #[serde(flatten)]
    pub row: TrackRow,
    pub path: String,
    pub file_size: i64,
    pub file_mtime: String,
    pub album_artist: Option<String>,
    pub composer: Option<String>,
    pub mix: Option<String>,
    pub grouping: Option<String>,
    pub track_no: Option<i64>,
    pub disc_no: Option<i64>,
    pub sample_rate: Option<i64>,
    pub bit_depth: Option<i64>,
    pub channels: Option<i64>,
    pub key_source: Option<String>,
    pub bpm_source: Option<String>,
    pub rb_track_id: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
    pub artwork: Option<ArtworkInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtworkInfo {
    pub id: ArtworkId,
    pub mime: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
}

/// A hot cue (`slot` 0..7 = A..H), memory cue (`slot` `None`) or loop (`end_ms` set).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Cue {
    pub id: i64,
    pub slot: Option<u8>,
    /// rekordbox `Type`: 0 cue, 1 fade-in, 2 fade-out, 3 load, 4 loop.
    pub kind: i64,
    pub start_ms: f64,
    pub end_ms: Option<f64>,
    pub name: Option<String>,
    /// 0xRRGGBB.
    pub color_rgb: Option<u32>,
    /// A memory loop playback enters by itself at its start (task 17;
    /// rekordbox's "active loop"). False for every other cue.
    pub active_loop: bool,
}

/// The beatgrid at a glance.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BeatgridSummary {
    /// Tempo anchors (`beatgrid` rows); 1 for a constant grid.
    pub markers: u32,
    pub first_beat_ms: f64,
    /// Equal to `bpm_max_x100` for a constant grid.
    pub bpm_min_x100: i64,
    pub bpm_max_x100: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackDetail {
    pub track: Track,
    /// Hot cues by slot, then memory cues and loops by position.
    pub cues: Vec<Cue>,
    /// `None` when the track has no grid (needs analysis).
    pub beatgrid: Option<BeatgridSummary>,
}

/// A metadata edit. An absent field is left as it is; `null` clears it.
/// Text is trimmed and empty text clears too.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrackPatch {
    #[serde(default, deserialize_with = "nullable")]
    pub title: Option<Option<String>>,
    #[serde(default, deserialize_with = "nullable")]
    pub artist: Option<Option<String>>,
    #[serde(default, deserialize_with = "nullable")]
    pub album: Option<Option<String>>,
    #[serde(default, deserialize_with = "nullable")]
    pub album_artist: Option<Option<String>>,
    #[serde(default, deserialize_with = "nullable")]
    pub genre: Option<Option<String>>,
    #[serde(default, deserialize_with = "nullable")]
    pub composer: Option<Option<String>>,
    #[serde(default, deserialize_with = "nullable")]
    pub remixer: Option<Option<String>>,
    #[serde(default, deserialize_with = "nullable")]
    pub label: Option<Option<String>>,
    #[serde(default, deserialize_with = "nullable")]
    pub mix: Option<Option<String>>,
    #[serde(default, deserialize_with = "nullable")]
    pub grouping: Option<Option<String>>,
    /// May contain line breaks and tabs.
    #[serde(default, deserialize_with = "nullable")]
    pub comment: Option<Option<String>>,
    /// 1..=999.
    #[serde(default, deserialize_with = "nullable")]
    pub track_no: Option<Option<i64>>,
    /// 1..=999.
    #[serde(default, deserialize_with = "nullable")]
    pub disc_no: Option<Option<i64>>,
    /// 1..=9999.
    #[serde(default, deserialize_with = "nullable")]
    pub year: Option<Option<i64>>,
    /// 0..=5; not nullable.
    #[serde(default)]
    pub rating: Option<i64>,
    /// A name from `rbxml::TRACK_COLOURS` (any case).
    #[serde(default, deserialize_with = "nullable")]
    pub color: Option<Option<String>>,
    /// Any notation `Key::parse` accepts; stored canonical, `key_source = 'manual'`.
    #[serde(default, deserialize_with = "nullable")]
    pub key: Option<Option<String>>,
}

/// Present-but-null → `Some(None)`; absent → `None` (via `#[serde(default)]`).
fn nullable<'de, D, T>(d: D) -> std::result::Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

impl Library {
    /// The full track, its cues and its beatgrid summary.
    pub fn track(&self, id: TrackId) -> Result<TrackDetail> {
        let conn = self.conn()?;
        detail(&conn, id)
    }

    /// Applies `patch` and sets `updated_at`, then returns the updated track.
    /// Every field is validated before anything is written; a rejected edit
    /// (`Error::InvalidField`) changes nothing. An empty patch writes nothing.
    pub fn update_track(&self, id: TrackId, patch: &TrackPatch) -> Result<TrackDetail> {
        let sets = validate(patch)?;
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        let exists = tx
            .query_row("SELECT 1 FROM track WHERE id = ?1", [id], |_| Ok(()))
            .optional()?
            .is_some();
        if !exists {
            return Err(Error::TrackNotFound(id));
        }
        if !sets.is_empty() {
            // Column names come from `validate`'s fixed list; values are bound.
            let cols: Vec<String> = sets.iter().map(|(c, _)| format!("{c} = ?")).collect();
            let sql = format!(
                "UPDATE track SET {}, updated_at = ? WHERE id = ?",
                cols.join(", ")
            );
            let mut params: Vec<Value> = sets.into_iter().map(|(_, v)| v).collect();
            params.push(Value::Text(now_utc()));
            params.push(Value::Integer(id));
            tx.execute(&sql, params_from_iter(&params))?;
        }
        if patch.comment.is_some() {
            // The tag block is the tags', not the user's text (D36).
            crate::tag_comments::sync_tagged(&tx, Some(&[id]))?;
        }
        let out = detail(&tx, id)?;
        tx.commit()?;
        Ok(out)
    }
}

fn detail(conn: &Connection, id: TrackId) -> Result<TrackDetail> {
    let mut track = conn
        .query_row(
            &format!(
                "SELECT {ROW_COLUMNS}, t.path, t.file_size, t.file_mtime, t.album_artist,
                        t.composer, t.mix, t.grouping, t.track_no, t.disc_no, t.sample_rate,
                        t.bit_depth, t.channels, t.key_source, t.bpm_source,
                        t.rb_track_id, t.created_at, t.updated_at,
                        a.id, a.mime, a.width, a.height
                 FROM track t LEFT JOIN artwork a ON a.id = t.artwork_id
                 WHERE t.id = ?1"
            ),
            [id],
            |r| {
                // Columns 0..=21 are `ROW_COLUMNS`.
                let artwork_id: Option<ArtworkId> = r.get(39)?;
                let artwork = match artwork_id {
                    Some(id) => Some(ArtworkInfo {
                        id,
                        mime: r.get(40)?,
                        width: r.get(41)?,
                        height: r.get(42)?,
                    }),
                    None => None,
                };
                Ok(Track {
                    row: track_row(r)?,
                    path: r.get(22)?,
                    file_size: r.get(23)?,
                    file_mtime: r.get(24)?,
                    album_artist: r.get(25)?,
                    composer: r.get(26)?,
                    mix: r.get(27)?,
                    grouping: r.get(28)?,
                    track_no: r.get(29)?,
                    disc_no: r.get(30)?,
                    sample_rate: r.get(31)?,
                    bit_depth: r.get(32)?,
                    channels: r.get(33)?,
                    key_source: r.get(34)?,
                    bpm_source: r.get(35)?,
                    rb_track_id: r.get(36)?,
                    created_at: r.get(37)?,
                    updated_at: r.get(38)?,
                    artwork,
                })
            },
        )
        .optional()?
        .ok_or(Error::TrackNotFound(id))?;
    fill_tags(conn, std::slice::from_mut(&mut track.row))?;

    let cues = conn
        .prepare_cached(
            "SELECT id, slot, kind, start_ms, end_ms, name, color_rgb, active_loop FROM cue
             WHERE track_id = ?1 ORDER BY slot IS NULL, slot, start_ms, id",
        )?
        .query_map([id], |r| {
            Ok(Cue {
                id: r.get(0)?,
                slot: r.get(1)?,
                kind: r.get(2)?,
                start_ms: r.get(3)?,
                end_ms: r.get(4)?,
                name: r.get(5)?,
                color_rgb: r.get(6)?,
                active_loop: r.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let beatgrid = conn.query_row(
        "SELECT count(*), min(position_ms), min(bpm_x100), max(bpm_x100)
         FROM beatgrid WHERE track_id = ?1",
        [id],
        |r| {
            let markers: u32 = r.get(0)?;
            Ok(if markers == 0 {
                None
            } else {
                Some(BeatgridSummary {
                    markers,
                    first_beat_ms: r.get(1)?,
                    bpm_min_x100: r.get(2)?,
                    bpm_max_x100: r.get(3)?,
                })
            })
        },
    )?;

    Ok(TrackDetail {
        track,
        cues,
        beatgrid,
    })
}

/// Checks every field of `patch` and returns the `(column, value)` pairs to set.
fn validate(p: &TrackPatch) -> Result<Vec<(&'static str, Value)>> {
    let mut sets = Vec::new();
    let texts = [
        ("title", &p.title),
        ("artist", &p.artist),
        ("album", &p.album),
        ("album_artist", &p.album_artist),
        ("genre", &p.genre),
        ("composer", &p.composer),
        ("remixer", &p.remixer),
        ("label", &p.label),
        ("mix", &p.mix),
        ("grouping", &p.grouping),
    ];
    for (col, v) in texts {
        if let Some(v) = v {
            sets.push((col, text(col, v.as_deref(), false)?));
        }
    }
    if let Some(v) = &p.comment {
        sets.push(("comment", text("comment", v.as_deref(), true)?));
    }
    let ints = [
        ("track_no", &p.track_no, 1..=999),
        ("disc_no", &p.disc_no, 1..=999),
        ("year", &p.year, 1..=9999),
    ];
    for (col, v, range) in ints {
        if let Some(v) = v {
            sets.push((col, int(col, *v, range)?));
        }
    }
    if let Some(r) = p.rating {
        sets.push(("rating", int("rating", Some(r), 0..=5)?));
    }
    if let Some(v) = &p.color {
        sets.push(("color", colour(v.as_deref())?));
    }
    if let Some(v) = &p.key {
        let key = key(v.as_deref())?;
        let source = match key {
            Value::Null => Value::Null,
            _ => Value::Text("manual".into()),
        };
        sets.push(("key", key));
        sets.push(("key_source", source));
    }
    Ok(sets)
}

fn invalid(field: &'static str, reason: impl Into<String>) -> Error {
    Error::InvalidField {
        field,
        reason: reason.into(),
    }
}

/// Trimmed text, `NULL` when empty. Control characters are refused (they
/// can't round-trip through rekordbox XML); `multiline` allows LF and tab.
pub(crate) fn text(field: &'static str, v: Option<&str>, multiline: bool) -> Result<Value> {
    let s = v.map(str::trim).unwrap_or_default();
    if s.is_empty() {
        return Ok(Value::Null);
    }
    if s.chars().count() > MAX_TEXT_CHARS {
        return Err(invalid(
            field,
            format!("longer than {MAX_TEXT_CHARS} characters"),
        ));
    }
    let allowed = |c: char| multiline && (c == '\n' || c == '\t');
    if let Some(c) = s.chars().find(|&c| c.is_control() && !allowed(c)) {
        let what = if c == '\n' || c == '\r' {
            "a line break".to_owned()
        } else {
            format!("control character U+{:04X}", u32::from(c))
        };
        return Err(invalid(field, format!("contains {what}")));
    }
    Ok(Value::Text(s.to_owned()))
}

fn int(field: &'static str, v: Option<i64>, range: std::ops::RangeInclusive<i64>) -> Result<Value> {
    match v {
        None => Ok(Value::Null),
        Some(n) if range.contains(&n) => Ok(Value::Integer(n)),
        Some(n) => Err(invalid(
            field,
            format!("{n} is outside {}..={}", range.start(), range.end()),
        )),
    }
}

fn colour(v: Option<&str>) -> Result<Value> {
    let s = v.map(str::trim).unwrap_or_default();
    if s.is_empty() {
        return Ok(Value::Null);
    }
    TRACK_COLOURS
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(s))
        .map(|(name, _)| Value::Text((*name).to_owned()))
        .ok_or_else(|| invalid("color", format!("{s:?} is not a rekordbox track colour")))
}

fn key(v: Option<&str>) -> Result<Value> {
    let s = v.map(str::trim).unwrap_or_default();
    if s.is_empty() {
        return Ok(Value::Null);
    }
    Key::parse(s)
        .map(|k| Value::Text(k.canonical().to_owned()))
        .map_err(|e| invalid("key", e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_distinguishes_absent_from_null() {
        let p: TrackPatch =
            serde_json::from_str(r#"{"title": "X", "albumArtist": null, "rating": 3}"#).unwrap();
        assert_eq!(p.title, Some(Some("X".into())));
        assert_eq!(p.album_artist, Some(None));
        assert_eq!(p.artist, None);
        assert_eq!(p.rating, Some(3));
        assert_eq!(
            serde_json::from_str::<TrackPatch>("{}").unwrap(),
            TrackPatch::default()
        );
        // Unknown or read-only fields are refused, not silently dropped.
        assert!(serde_json::from_str::<TrackPatch>(r#"{"bpmX100": 12800}"#).is_err());
    }

    #[test]
    fn text_rules() {
        assert_eq!(
            text("title", Some("  a  "), false).unwrap(),
            Value::Text("a".into())
        );
        assert_eq!(text("title", Some("   "), false).unwrap(), Value::Null);
        assert_eq!(text("title", None, false).unwrap(), Value::Null);
        assert!(text("title", Some("a\nb"), false).is_err());
        assert!(text("title", Some("a\u{7}b"), false).is_err());
        assert!(text("comment", Some("a\nb\tc"), true).is_ok());
        assert!(text("comment", Some("a\rb"), true).is_err());
        let long = "é".repeat(MAX_TEXT_CHARS);
        assert!(text("title", Some(&long), false).is_ok());
        assert!(text("title", Some(&format!("{long}x")), false).is_err());
    }

    #[test]
    fn colour_and_key_normalise() {
        assert_eq!(colour(Some("Red")).unwrap(), Value::Text("red".into()));
        assert!(colour(Some("magenta")).is_err());
        assert_eq!(key(Some("8A")).unwrap(), Value::Text("Am".into()));
        assert!(key(Some("H#")).is_err());
    }
}
