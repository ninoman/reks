//! Playlists of the two libraries compared by path (`Folder/Name`) and, for
//! playlists in both, by their tracks' stick paths in order. rekordbox and
//! reks write the same tree to both, so any difference means players of one
//! family see other lists than the other.

use std::collections::{HashMap, HashSet};

use super::{path_key, FindingItem, FindingKind, Findings, OneLibraryContents, Severity};
use crate::pdb::PdbLibrary;

/// A folder or playlist, flattened.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Node {
    /// `Folder/Name`; ` (2)`, … for siblings of the same name.
    path: String,
    parent: Option<String>,
    is_folder: bool,
    /// Track stick paths ([`path_key`]) in entry order.
    tracks: Vec<String>,
}

/// One tree node of either library before flattening.
struct Raw {
    id: i64,
    parent: i64,
    order: i64,
    name: String,
    is_folder: bool,
    tracks: Vec<String>,
}

pub(super) fn compare(pdb: &PdbLibrary, one: &OneLibraryContents, out: &mut Findings) {
    let pdb_paths: HashMap<u32, String> = pdb
        .tracks
        .iter()
        .map(|t| (t.id, path_key(&t.file_path)))
        .collect();
    let left = flatten(
        pdb.playlists
            .iter()
            .map(|p| Raw {
                id: p.id.into(),
                parent: p.parent_id.into(),
                order: p.sort_order.into(),
                name: p.name.clone(),
                is_folder: p.is_folder,
                tracks: p
                    .entries
                    .iter()
                    .map(|id| track_path(pdb_paths.get(id), (*id).into()))
                    .collect(),
            })
            .collect(),
    );
    let one_paths: HashMap<i64, String> = one
        .tracks
        .iter()
        .map(|t| (t.id, path_key(&t.path)))
        .collect();
    let right = flatten(
        one.playlists
            .iter()
            .map(|p| Raw {
                id: p.id,
                parent: p.parent_id,
                order: p.sequence_no,
                name: p.name.clone(),
                is_folder: p.is_folder,
                tracks: p
                    .track_ids
                    .iter()
                    .map(|id| track_path(one_paths.get(id), *id))
                    .collect(),
            })
            .collect(),
    );

    let by_path = |nodes: &[Node]| -> HashMap<String, usize> {
        nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.path.clone(), i))
            .collect()
    };
    let (left_at, right_at) = (by_path(&left), by_path(&right));
    only_in(&left, &right_at, "Only in the Device Library", out);
    only_in(&right, &left_at, "Only in OneLibrary", out);
    for l in &left {
        let Some(r) = right_at.get(&l.path).map(|&i| &right[i]) else {
            continue;
        };
        if let Some(detail) = difference(l, r) {
            out.item(
                FindingKind::PlaylistDiffers,
                Severity::Warning,
                FindingItem {
                    subject: l.path.clone(),
                    detail: Some(detail),
                    path: None,
                },
            );
        }
    }
}

/// A missing track keeps its place in the comparison under its id.
fn track_path(found: Option<&String>, id: i64) -> String {
    found
        .cloned()
        .unwrap_or_else(|| format!("\u{0}missing track {id}"))
}

/// Nodes of `nodes` with no counterpart; a node whose parent is missing too
/// is covered by its parent.
fn only_in(nodes: &[Node], other: &HashMap<String, usize>, detail: &str, out: &mut Findings) {
    for n in nodes {
        let parent_missing = n.parent.as_ref().is_some_and(|p| !other.contains_key(p));
        if !other.contains_key(&n.path) && !parent_missing {
            out.item(
                FindingKind::PlaylistInOneLibrary,
                Severity::Warning,
                FindingItem {
                    subject: n.path.clone(),
                    detail: Some(detail.to_owned()),
                    path: None,
                },
            );
        }
    }
}

fn difference(l: &Node, r: &Node) -> Option<String> {
    if l.is_folder != r.is_folder {
        return Some("A folder in one library, a playlist in the other".into());
    }
    if l.is_folder || l.tracks == r.tracks {
        return None;
    }
    if l.tracks.len() != r.tracks.len() {
        return Some(format!(
            "{} in the Device Library, {} in OneLibrary",
            super::text::count(l.tracks.len(), "track", "tracks"),
            r.tracks.len()
        ));
    }
    let mut a = l.tracks.clone();
    let mut b = r.tracks.clone();
    a.sort_unstable();
    b.sort_unstable();
    if a == b {
        return Some("Same tracks in another order".into());
    }
    let first = l.tracks.iter().zip(&r.tracks).position(|(x, y)| x != y);
    Some(format!(
        "Different tracks from position {}",
        first.unwrap_or(0) + 1
    ))
}

/// Parents before children, siblings by their order then id. Nodes in a
/// parent cycle or under a missing parent are left out, as players can't
/// reach them either.
fn flatten(mut raw: Vec<Raw>) -> Vec<Node> {
    raw.sort_by_key(|r| (r.parent, r.order, r.id));
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    walk(&raw, 0, None, &mut seen, &mut out);
    out
}

fn walk(
    raw: &[Raw],
    parent: i64,
    parent_path: Option<&str>,
    seen: &mut HashSet<i64>,
    out: &mut Vec<Node>,
) {
    let mut names: HashMap<String, u32> = HashMap::new();
    for r in raw.iter().filter(|r| r.parent == parent) {
        if !seen.insert(r.id) {
            continue;
        }
        let n = names.entry(r.name.clone()).or_insert(0);
        *n += 1;
        let name = if *n == 1 {
            r.name.clone()
        } else {
            format!("{} ({n})", r.name)
        };
        let path = match parent_path {
            Some(p) => format!("{p}/{name}"),
            None => name,
        };
        out.push(Node {
            path: path.clone(),
            parent: parent_path.map(str::to_owned),
            is_folder: r.is_folder,
            tracks: r.tracks.clone(),
        });
        if r.is_folder {
            walk(raw, r.id, Some(&path), seen, out);
        }
    }
}
