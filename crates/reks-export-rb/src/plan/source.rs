//! Reads what the planner needs from the library: the selected playlists
//! (with the folders above them) and each track once.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use reks_audio::{Kind, Probe};
use reks_core::query::MAX_PAGE;
use reks_core::{Filter, Library, PlaylistId, PlaylistNode, Sort, SortColumn, TrackId, TrackQuery};

use super::{PlannedPlaylist, Problem, Selection};
use crate::divider::{DividerInfo, SourceDivider};

/// One library track as the planner sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceTrack {
    pub track_id: TrackId,
    pub src: PathBuf,
    pub artist: Option<String>,
    pub album: Option<String>,
    /// From the library's columns; `None` makes the planner probe the file.
    pub probe: Option<Probe>,
    pub has_grid: bool,
    /// The user owns the library grid (locked or edited by hand,
    /// [`Library::grid_is_user_owned`]): the planner prefers reks's analysis,
    /// whose `PQTZ` is that grid, over rekordbox's cached one.
    pub grid_user_owned: bool,
    /// Hot cues, memory cues and loops (capped at 255).
    pub cue_count: u8,
    /// Set for a playlist divider ([`crate::divider::expand`]).
    pub divider: Option<DividerInfo>,
}

/// The planner's input.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sources {
    /// Each track once, in the order it first appears in `playlists`.
    pub tracks: Vec<SourceTrack>,
    /// Selected playlists plus their ancestors and descendants, depth first.
    pub playlists: Vec<PlannedPlaylist>,
    /// Selection problems (unknown ids, nothing selected).
    pub problems: Vec<Problem>,
    /// The playlists' dividers, in playlist then entry order; not in
    /// `tracks` or `playlists` until [`crate::divider::expand`].
    pub dividers: Vec<SourceDivider>,
}

/// Loads `sel` from `lib`.
pub fn load(lib: &Library, sel: &Selection) -> reks_core::Result<Sources> {
    let nodes = lib.playlists()?.nodes;
    let mut out = Sources::default();
    if sel.playlists.is_empty() {
        out.problems.push(Problem::NothingSelected);
        return Ok(out);
    }

    let by_id: HashMap<PlaylistId, &PlaylistNode> = nodes.iter().map(|n| (n.id, n)).collect();
    let mut wanted: HashSet<PlaylistId> = HashSet::new();
    for &id in &sel.playlists {
        if !by_id.contains_key(&id) {
            out.problems.push(Problem::PlaylistNotFound { id });
            continue;
        }
        // The node, its ancestors (to keep the tree) and, below, its subtree.
        let mut cur = Some(id);
        while let Some(c) = cur {
            wanted.insert(c);
            cur = by_id.get(&c).and_then(|n| n.parent_id);
        }
    }
    // `nodes` is depth first, so a parent comes before its children.
    let selected: HashSet<PlaylistId> = sel.playlists.iter().copied().collect();
    let mut inside: HashSet<PlaylistId> = HashSet::new();
    for n in &nodes {
        if selected.contains(&n.id) || n.parent_id.is_some_and(|p| inside.contains(&p)) {
            inside.insert(n.id);
            wanted.insert(n.id);
        }
    }

    let user_owned = lib.user_owned_grids()?;
    let mut seen: HashSet<TrackId> = HashSet::new();
    for n in nodes.iter().filter(|n| wanted.contains(&n.id)) {
        let mut track_ids = Vec::new();
        // Ancestors that were not selected themselves contribute no tracks.
        if !n.is_folder && inside.contains(&n.id) {
            let dividers;
            (track_ids, dividers) = playlist_entries(lib, n.id)?;
            out.dividers.extend(dividers);
            for &tid in &track_ids {
                if seen.insert(tid) {
                    out.tracks
                        .push(source_track(lib, tid, user_owned.contains(&tid))?);
                }
            }
        }
        out.playlists.push(PlannedPlaylist {
            id: n.id,
            parent_id: n.parent_id,
            name: n.name.clone(),
            is_folder: n.is_folder,
            track_ids,
        });
    }
    Ok(out)
}

/// The playlist's track entries in order, duplicates included, and its
/// dividers.
fn playlist_entries(
    lib: &Library,
    id: PlaylistId,
) -> reks_core::Result<(Vec<TrackId>, Vec<SourceDivider>)> {
    let mut ids = Vec::new();
    let mut dividers = Vec::new();
    let mut offset = 0;
    loop {
        let page = lib.tracks(&TrackQuery {
            filters: vec![Filter::Playlist { id }],
            sort: Sort {
                column: SortColumn::PlaylistPosition,
                descending: false,
            },
            offset,
            limit: MAX_PAGE,
            ..TrackQuery::default()
        })?;
        if offset == 0 {
            // Every page lists all dividers by entry position (dense, from 0),
            // so a divider at position p has p minus earlier dividers tracks
            // before it.
            dividers = page
                .dividers
                .iter()
                .enumerate()
                .map(|(i, d)| SourceDivider {
                    info: DividerInfo {
                        playlist_id: id,
                        ordinal: u32::try_from(i + 1).unwrap_or(u32::MAX),
                        label: d.label.clone(),
                    },
                    tracks_before: usize::try_from(d.position).unwrap_or(0).saturating_sub(i),
                })
                .collect();
        }
        let n = page.rows.len();
        ids.extend(page.rows.into_iter().map(|r| r.id));
        if n < MAX_PAGE as usize {
            return Ok((ids, dividers));
        }
        offset += MAX_PAGE;
    }
}

fn source_track(
    lib: &Library,
    id: TrackId,
    grid_user_owned: bool,
) -> reks_core::Result<SourceTrack> {
    let d = lib.track(id)?;
    let t = &d.track;
    let probe = match (
        parse_kind(&t.row.kind),
        t.sample_rate.and_then(|v| u32::try_from(v).ok()),
        t.channels.and_then(|v| u8::try_from(v).ok()),
    ) {
        (Some(kind), Some(sample_rate), Some(channels)) => Some(Probe {
            kind,
            duration_ms: t
                .row
                .duration_ms
                .and_then(|v| u64::try_from(v).ok())
                .unwrap_or(0),
            sample_rate,
            channels,
            bit_depth: t.bit_depth.and_then(|v| u8::try_from(v).ok()),
            bit_rate_kbps: t.row.bit_rate_kbps.and_then(|v| u32::try_from(v).ok()),
        }),
        _ => None,
    };
    Ok(SourceTrack {
        track_id: id,
        src: PathBuf::from(&t.path),
        artist: t.row.artist.clone(),
        album: t.row.album.clone(),
        probe,
        has_grid: d.beatgrid.is_some(),
        grid_user_owned,
        cue_count: u8::try_from(d.cues.len()).unwrap_or(u8::MAX),
        divider: None,
    })
}

fn parse_kind(s: &str) -> Option<Kind> {
    [
        Kind::Mp3,
        Kind::Aac,
        Kind::Alac,
        Kind::Flac,
        Kind::Wav,
        Kind::Aiff,
    ]
    .into_iter()
    .find(|k| k.as_str() == s)
}
