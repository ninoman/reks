//! Comparing two `exportLibrary.db` dumps (task 10, `reks onelibrary diff`):
//! ours against a rekordbox reference export, or any two.
//!
//! - Schema: each `sqlite_master` entry's SQL text, by name.
//! - Seed tables (`menuItem`, `category`, `sort`, `myTag`, `color`): row
//!   for row.
//! - `content`: rows matched on `path`, every column compared, with lookup
//!   ids resolved to the names (or image path) they point at, so ids that
//!   differ between writers do not show up.
//! - Lookups (`artist`, `album`, `genre`, `label`, `key`, `image`): their
//!   sets of names.
//! - Playlists: the tree as `Folder/Name` paths with folder flag and
//!   sibling order, and each playlist's entries in order, as content paths.
//! - `property`, and every other table's rows.
//!
//! By default ids, `masterDbId`/`masterContentId` (the writing library's
//! ids) and dates are ignored, as the phase 4 acceptance test asks;
//! [`DiffOptions::strict`] compares them too.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde_json::Value;

use crate::dump::{Dump, Table};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DiffOptions {
    /// Also compare ids, master ids and dates.
    pub strict: bool,
}

/// One difference, "ours" against "theirs".
#[derive(Debug, Clone, PartialEq)]
pub enum Difference {
    /// A schema entry's SQL differs or is on one side only.
    Schema {
        name: String,
        ours: Option<String>,
        theirs: Option<String>,
    },
    /// A table's row `index` (rowid order) differs; `None` when one side
    /// has fewer rows.
    Row {
        table: String,
        index: usize,
        ours: Option<Vec<Value>>,
        theirs: Option<Vec<Value>>,
    },
    /// A track is in one database only.
    ContentOnlyIn { side: Side, path: String },
    ContentField {
        path: String,
        column: String,
        ours: Value,
        theirs: Value,
    },
    /// A lookup name (or image path) is in one database only.
    LookupOnlyIn {
        table: String,
        side: Side,
        name: String,
    },
    /// A playlist tree node is in one database only, or differs in
    /// folder flag or sibling order.
    PlaylistNode {
        path: String,
        ours: Option<String>,
        theirs: Option<String>,
    },
    /// A playlist's entries (content paths) differ.
    PlaylistEntries {
        path: String,
        ours: Vec<String>,
        theirs: Vec<String>,
    },
    Property {
        column: String,
        ours: Value,
        theirs: Value,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Ours,
    Theirs,
}

const LOOKUPS: [(&str, &str); 6] = [
    ("artist", "name"),
    ("album", "name"),
    ("genre", "name"),
    ("label", "name"),
    ("key", "name"),
    ("image", "path"),
];
/// `content` columns that point into a lookup: (column, table).
const CONTENT_REFS: [(&str, &str); 11] = [
    ("artist_id_artist", "artist"),
    ("artist_id_remixer", "artist"),
    ("artist_id_originalArtist", "artist"),
    ("artist_id_composer", "artist"),
    ("artist_id_lyricist", "artist"),
    ("album_id", "album"),
    ("genre_id", "genre"),
    ("label_id", "label"),
    ("key_id", "key"),
    ("color_id", "color"),
    ("image_id", "image"),
];
const CONTENT_IGNORED: [&str; 5] = [
    "content_id",
    "masterDbId",
    "masterContentId",
    "dateCreated",
    "dateAdded",
];
const HANDLED: [&str; 4] = ["content", "playlist", "playlist_content", "property"];

/// Everything that differs between `ours` and `theirs`.
pub fn diff(ours: &Dump, theirs: &Dump, opts: DiffOptions) -> Vec<Difference> {
    let mut out = Vec::new();
    diff_schema(ours, theirs, &mut out);
    let empty = Table::default();
    let names: BTreeSet<&str> = ours
        .tables
        .keys()
        .chain(theirs.tables.keys())
        .map(String::as_str)
        .collect();
    for name in names {
        if HANDLED.contains(&name) {
            continue;
        }
        let a = ours.table(name).unwrap_or(&empty);
        let b = theirs.table(name).unwrap_or(&empty);
        // Seed tables and the rest go row for row.
        match LOOKUPS.iter().find(|(t, _)| *t == name) {
            Some((_, col)) if !opts.strict => diff_lookup(name, col, a, b, &mut out),
            _ => diff_rows(name, a, b, &mut out),
        }
    }
    diff_content(ours, theirs, opts, &mut out);
    if opts.strict {
        for t in ["playlist", "playlist_content"] {
            diff_rows(
                t,
                ours.table(t).unwrap_or(&empty),
                theirs.table(t).unwrap_or(&empty),
                &mut out,
            );
        }
    } else {
        diff_playlists(ours, theirs, &mut out);
    }
    diff_property(ours, theirs, opts, &mut out);
    out
}

fn diff_schema(a: &Dump, b: &Dump, out: &mut Vec<Difference>) {
    let map = |d: &Dump| -> BTreeMap<String, Option<String>> {
        d.schema
            .iter()
            .map(|e| (e.name.clone(), e.sql.clone()))
            .collect()
    };
    let (ma, mb) = (map(a), map(b));
    let names: BTreeSet<&String> = ma.keys().chain(mb.keys()).collect();
    for name in names {
        let (x, y) = (
            ma.get(name).cloned().flatten(),
            mb.get(name).cloned().flatten(),
        );
        if x != y || ma.contains_key(name) != mb.contains_key(name) {
            out.push(Difference::Schema {
                name: name.clone(),
                ours: x,
                theirs: y,
            });
        }
    }
}

fn diff_rows(table: &str, a: &Table, b: &Table, out: &mut Vec<Difference>) {
    for i in 0..a.rows.len().max(b.rows.len()) {
        let (x, y) = (a.rows.get(i), b.rows.get(i));
        if x != y {
            out.push(Difference::Row {
                table: table.to_owned(),
                index: i,
                ours: x.cloned(),
                theirs: y.cloned(),
            });
        }
    }
}

fn column_values(t: &Table, col: &str) -> BTreeSet<String> {
    t.col(col)
        .map(|i| t.rows.iter().filter_map(|r| r.get(i)).map(text).collect())
        .unwrap_or_default()
}

fn diff_lookup(table: &str, col: &str, a: &Table, b: &Table, out: &mut Vec<Difference>) {
    let (x, y) = (column_values(a, col), column_values(b, col));
    for (side, only) in [
        (Side::Ours, x.difference(&y)),
        (Side::Theirs, y.difference(&x)),
    ] {
        for name in only {
            out.push(Difference::LookupOnlyIn {
                table: table.to_owned(),
                side,
                name: name.clone(),
            });
        }
    }
}

/// id → name (or path) of a lookup table.
fn lookup(d: &Dump, table: &str) -> BTreeMap<i64, String> {
    let col = LOOKUPS
        .iter()
        .find(|(t, _)| *t == table)
        .map_or("name", |(_, c)| c);
    let Some(t) = d.table(table) else {
        return BTreeMap::new();
    };
    let id_col = 0;
    let Some(name_col) = t.col(col) else {
        return BTreeMap::new();
    };
    t.rows
        .iter()
        .filter_map(|r| Some((r.get(id_col)?.as_i64()?, text(r.get(name_col)?))))
        .collect()
}

/// `content` rows by path, with references resolved (unless strict) and
/// ignored columns left out.
fn contents(d: &Dump, opts: DiffOptions) -> BTreeMap<String, BTreeMap<String, Value>> {
    let lookups: BTreeMap<&str, BTreeMap<i64, String>> = CONTENT_REFS
        .iter()
        .map(|(_, t)| (*t, lookup(d, t)))
        .collect();
    let Some(t) = d.table("content") else {
        return BTreeMap::new();
    };
    t.records()
        .map(|rec| {
            let path = rec.get("path").map(|v| text(v)).unwrap_or_default();
            let fields = rec
                .into_iter()
                .filter(|(c, _)| opts.strict || !CONTENT_IGNORED.contains(c))
                .map(|(c, v)| {
                    let resolved = match CONTENT_REFS.iter().find(|(col, _)| *col == c) {
                        Some((_, table)) if !opts.strict => match v.as_i64() {
                            Some(id) => lookups[table].get(&id).map_or_else(
                                || format!("#{id} (no row)").into(),
                                |n| Value::from(n.clone()),
                            ),
                            None => v.clone(),
                        },
                        _ => v.clone(),
                    };
                    (c.to_owned(), resolved)
                })
                .collect();
            (path, fields)
        })
        .collect()
}

fn diff_content(a: &Dump, b: &Dump, opts: DiffOptions, out: &mut Vec<Difference>) {
    let (x, y) = (contents(a, opts), contents(b, opts));
    for (path, fa) in &x {
        let Some(fb) = y.get(path) else {
            out.push(Difference::ContentOnlyIn {
                side: Side::Ours,
                path: path.clone(),
            });
            continue;
        };
        let cols: BTreeSet<&String> = fa.keys().chain(fb.keys()).collect();
        for c in cols {
            let (va, vb) = (
                fa.get(c).cloned().unwrap_or(Value::Null),
                fb.get(c).cloned().unwrap_or(Value::Null),
            );
            if va != vb {
                out.push(Difference::ContentField {
                    path: path.clone(),
                    column: c.clone(),
                    ours: va,
                    theirs: vb,
                });
            }
        }
    }
    for path in y.keys().filter(|p| !x.contains_key(*p)) {
        out.push(Difference::ContentOnlyIn {
            side: Side::Theirs,
            path: path.clone(),
        });
    }
}

/// Playlist tree as path → ("folder"/"playlist" @ sequenceNo, entry paths).
fn tree(d: &Dump) -> BTreeMap<String, (String, Vec<String>)> {
    let Some(t) = d.table("playlist") else {
        return BTreeMap::new();
    };
    let nodes: BTreeMap<i64, (String, i64, i64, i64)> = t
        .records()
        .filter_map(|r| {
            Some((
                r.get("playlist_id")?.as_i64()?,
                (
                    text(r.get("name")?),
                    r.get("playlist_id_parent")?.as_i64().unwrap_or(0),
                    r.get("attribute")?.as_i64().unwrap_or(0),
                    r.get("sequenceNo")?.as_i64().unwrap_or(0),
                ),
            ))
        })
        .collect();
    let path_of = |mut id: i64| {
        let mut parts = Vec::new();
        // Bounded by the node count, in case of a cycle.
        for _ in 0..=nodes.len() {
            let Some((name, parent, _, _)) = nodes.get(&id) else {
                break;
            };
            parts.push(name.clone());
            id = *parent;
        }
        parts.reverse();
        parts.join("/")
    };
    let paths: BTreeMap<i64, String> = d
        .table("content")
        .map(|c| {
            c.records()
                .filter_map(|r| Some((r.get("content_id")?.as_i64()?, text(r.get("path")?))))
                .collect()
        })
        .unwrap_or_default();
    let mut entries: BTreeMap<i64, Vec<(i64, String)>> = BTreeMap::new();
    if let Some(pc) = d.table("playlist_content") {
        for r in pc.records() {
            let (Some(p), Some(c)) = (
                r.get("playlist_id").and_then(|v| v.as_i64()),
                r.get("content_id").and_then(|v| v.as_i64()),
            ) else {
                continue;
            };
            let seq = r.get("sequenceNo").and_then(|v| v.as_i64()).unwrap_or(0);
            let path = paths
                .get(&c)
                .cloned()
                .unwrap_or_else(|| format!("#{c} (no content)"));
            entries.entry(p).or_default().push((seq, path));
        }
    }
    nodes
        .iter()
        .map(|(&id, (_, _, attribute, seq))| {
            let kind = if *attribute == 1 {
                "folder"
            } else {
                "playlist"
            };
            let mut list = entries.remove(&id).unwrap_or_default();
            list.sort();
            (
                path_of(id),
                (
                    format!("{kind} @ {seq}"),
                    list.into_iter().map(|(_, p)| p).collect(),
                ),
            )
        })
        .collect()
}

fn diff_playlists(a: &Dump, b: &Dump, out: &mut Vec<Difference>) {
    let (x, y) = (tree(a), tree(b));
    let paths: BTreeSet<&String> = x.keys().chain(y.keys()).collect();
    for path in paths {
        let (na, nb) = (x.get(path), y.get(path));
        if na.map(|n| &n.0) != nb.map(|n| &n.0) {
            out.push(Difference::PlaylistNode {
                path: path.clone(),
                ours: na.map(|n| n.0.clone()),
                theirs: nb.map(|n| n.0.clone()),
            });
        }
        if let (Some(na), Some(nb)) = (na, nb) {
            if na.1 != nb.1 {
                out.push(Difference::PlaylistEntries {
                    path: path.clone(),
                    ours: na.1.clone(),
                    theirs: nb.1.clone(),
                });
            }
        }
    }
}

fn diff_property(a: &Dump, b: &Dump, opts: DiffOptions, out: &mut Vec<Difference>) {
    let first = |d: &Dump| -> BTreeMap<String, Value> {
        d.table("property")
            .and_then(|t| {
                t.records().next().map(|r| {
                    r.into_iter()
                        .map(|(c, v)| (c.to_owned(), v.clone()))
                        .collect()
                })
            })
            .unwrap_or_default()
    };
    let (x, y) = (first(a), first(b));
    let cols: BTreeSet<&String> = x.keys().chain(y.keys()).collect();
    for c in cols {
        if !opts.strict && c == "createdDate" {
            continue;
        }
        let (va, vb) = (
            x.get(c).cloned().unwrap_or(Value::Null),
            y.get(c).cloned().unwrap_or(Value::Null),
        );
        if va != vb {
            out.push(Difference::Property {
                column: c.clone(),
                ours: va,
                theirs: vb,
            });
        }
    }
}

/// A value as plain text: strings unquoted, everything else as JSON.
fn text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

impl fmt::Display for Side {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Ours => "ours",
            Self::Theirs => "theirs",
        })
    }
}

impl fmt::Display for Difference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let opt = |v: &Option<String>| v.clone().unwrap_or_else(|| "(none)".into());
        let row = |v: &Option<Vec<Value>>| {
            v.as_ref()
                .map_or_else(|| "(none)".into(), |r| Value::from(r.clone()).to_string())
        };
        match self {
            Self::Schema { name, ours, theirs } => {
                write!(
                    f,
                    "schema {name}: ours {} | theirs {}",
                    opt(ours),
                    opt(theirs)
                )
            }
            Self::Row {
                table,
                index,
                ours,
                theirs,
            } => write!(
                f,
                "{table} row {index}: ours {} | theirs {}",
                row(ours),
                row(theirs)
            ),
            Self::ContentOnlyIn { side, path } => write!(f, "content {path}: only in {side}"),
            Self::ContentField {
                path,
                column,
                ours,
                theirs,
            } => write!(f, "content {path} {column}: ours {ours} | theirs {theirs}"),
            Self::LookupOnlyIn { table, side, name } => {
                write!(f, "{table} {name:?}: only in {side}")
            }
            Self::PlaylistNode { path, ours, theirs } => write!(
                f,
                "playlist {path}: ours {} | theirs {}",
                opt(ours),
                opt(theirs)
            ),
            Self::PlaylistEntries { path, ours, theirs } => write!(
                f,
                "playlist {path} entries: ours {ours:?} | theirs {theirs:?}"
            ),
            Self::Property {
                column,
                ours,
                theirs,
            } => write!(f, "property {column}: ours {ours} | theirs {theirs}"),
        }
    }
}

#[cfg(test)]
mod tests;
