//! Adopt a foreign stick (D27, Phase 2.5 task 12; spec
//! `docs/stick-profiles.md` §4.3).
//!
//! A stick written by rekordbox (or another tool) holds playlists reks did
//! not make. Instead of replacing them on the first sync, [`scan`] reads the
//! stick and proposes, per stick playlist:
//!
//! - **Link** to a library playlist with the same name and exactly the same
//!   tracks in the same order (offered only then, and only when every stick
//!   track matched),
//! - **Create** a library playlist under `From stick/<stick name>/`, keeping
//!   the stick's folders below that (the default), or
//! - **Skip** (the next reks sync removes it from the stick).
//!
//! Stick tracks are matched with the play history matcher
//! ([`reks_core::history::match_entries`]: export manifest, path suffix,
//! artist + title + duration). The play history on the stick is planned with
//! [`reks_core::history::scan`] and [`apply`] imports it first.
//!
//! The stick is only read (`validate::read`, [`read_stick_tracks`],
//! [`read_play_history`]); nothing is written to it. [`apply`] writes the
//! library: history first (its own transaction), then the playlists and the
//! stick's profile in one transaction
//! ([`reks_core::Library::adopt_playlists`]).
//!
//! rekordcrate reads the stick. Whether it reads a current rekordbox 7
//! stick is open item S4: it reads rekordcrate's own real exports cleanly,
//! but no rekordbox 7 fixture exists yet (**UNVERIFIED**).

#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use reks_core::adopt::{AdoptCreate, AdoptLink, AdoptWrite};
use reks_core::history::{
    self, EntryKind, HistoryImportPlan, ImportChoices, ImportReport, MatchKind, SessionAction,
    SourceEntry,
};
use reks_core::{DeviceId, Library, PlaylistId, TrackId};
use serde::{Deserialize, Serialize};

use crate::devices::plural;
use crate::history::{
    read_play_history, read_stick_tracks, HistoryError, HistoryReadOptions, HistorySource, PdbTrack,
};
use crate::validate::{self, Finding, StickPlaylist, ValidateError};

/// Errors of [`scan`] and [`apply`].
#[derive(Debug, thiserror::Error)]
pub enum AdoptError {
    #[error(transparent)]
    Library(#[from] reks_core::Error),
    #[error("cannot read the stick: {0}")]
    Read(#[from] ValidateError),
    #[error("cannot read the stick's tracks: {0}")]
    Tracks(#[from] HistoryError),
    /// No `PIONEER/rekordbox/export.pdb`.
    #[error("{path} holds no rekordbox library (no PIONEER/rekordbox/export.pdb)")]
    NoLibrary { path: PathBuf },
    #[error("the stick's rekordbox library can't be read: {message}")]
    Unreadable { message: String },
    /// A choice names a playlist the plan does not have.
    #[error("the plan has no stick playlist {pdb_id}")]
    UnknownPlaylist { pdb_id: u32 },
    /// Link to a playlist the plan did not offer.
    #[error("stick playlist {pdb_id} can't be linked to library playlist {playlist_id}: only an exact match can")]
    LinkNotOffered {
        pdb_id: u32,
        playlist_id: PlaylistId,
    },
    /// The chosen device is another stick.
    #[error("'{name}' is another stick (volume ID {expected}, this one {found})")]
    WrongStick {
        name: String,
        expected: String,
        found: String,
    },
}

pub type Result<T> = std::result::Result<T, AdoptError>;

/// The dry run of adopting a stick. Pure data; nothing is written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdoptPlan {
    pub volume_serial: Option<String>,
    /// The library has an export manifest for this volume ID: reks wrote the
    /// stick (adopting it still works, matches are exact).
    pub written_by_reks: bool,
    /// The play history import [`apply`] runs first; `None` when the
    /// history could not be read (see `history_text`).
    pub history: Option<HistoryImportPlan>,
    /// Sessions [`apply`] imports (new or extended).
    pub history_sessions: u32,
    /// `2 play history sessions are imported first.`
    pub history_text: String,
    /// Stick playlists (not folders), in stick tree order, depth first.
    pub playlists: Vec<AdoptPlaylist>,
    /// Distinct stick tracks in playlists with no library track, in order of
    /// first appearance.
    pub unmatched: Vec<UnmatchedTrack>,
    /// `3 playlists, 40 tracks: 37 in your library, 3 not`.
    pub summary: String,
    /// Shown with `unmatched`; empty when everything matched.
    pub unmatched_text: String,
    /// Damage the read-back found, and other caveats.
    pub notes: Vec<String>,
}

/// One stick playlist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdoptPlaylist {
    /// The playlist's row id in `export.pdb`; the key of [`AdoptChoices`].
    pub pdb_id: u32,
    /// Stick folders, then the playlist name, root first.
    pub path: Vec<String>,
    /// `Sets / Peak`.
    pub path_text: String,
    pub entries: Vec<AdoptEntry>,
    /// Track entries with a library track.
    pub matched: u32,
    pub unmatched: u32,
    /// reks dividers (known from the export manifest); not adopted.
    pub dividers: u32,
    /// Library playlists Link may name: same name, same tracks in order.
    pub link: Vec<LinkCandidate>,
    /// [`AdoptChoice::Create`].
    pub default_choice: AdoptChoice,
    /// `12 tracks, 2 not in your library`.
    pub text: String,
}

/// One playlist entry, as on the stick, with its match.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdoptEntry {
    /// `None` when the entry's track row is missing.
    pub stick_path: Option<String>,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub kind: EntryKind,
    pub track_id: Option<TrackId>,
    pub matched_by: Option<MatchKind>,
}

/// A library playlist Link may name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkCandidate {
    pub playlist_id: PlaylistId,
    /// Library folders and name: `Sets / Peak`.
    pub path_text: String,
}

/// A stick track no library track matched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnmatchedTrack {
    pub stick_path: Option<String>,
    pub title: Option<String>,
    pub artist: Option<String>,
}

/// What to do with one stick playlist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum AdoptChoice {
    /// Use this library playlist; it must be one of the plan's candidates.
    Link { playlist_id: PlaylistId },
    /// A new playlist under `From stick/<stick name>/` with the matched
    /// tracks in stick order.
    Create,
    /// Leave it out; the next reks sync removes it from the stick.
    Skip,
}

/// The user's choices in the adopt dialog.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdoptChoices {
    /// The stick's device (created or linked first through `devices`); its
    /// name is the folder under `From stick`, and its profile gets the
    /// created and linked playlists.
    pub device_id: DeviceId,
    /// Stick playlist (`pdb_id`) → choice. Absent: the plan's default.
    #[serde(default)]
    pub playlists: HashMap<u32, AdoptChoice>,
    /// Practice marks for the history import.
    #[serde(default)]
    pub history: ImportChoices,
}

/// What [`apply`] did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdoptReport {
    /// `None` when there was no history to import.
    pub history: Option<ImportReport>,
    /// `From stick/<name>`, when something was created.
    pub folder_id: Option<PlaylistId>,
    pub created: Vec<PlaylistId>,
    pub linked: Vec<PlaylistId>,
    pub skipped: u32,
    /// Stick tracks the adopted playlists leave out.
    pub unmatched: Vec<UnmatchedTrack>,
    /// `2 playlists created, 1 linked, 1 skipped; 2 play history sessions imported.`
    pub summary: String,
}

/// Reads the stick at `root` (read-only) and plans the adoption.
/// `volume_serial` is the OS's volume ID ([`crate::device::volume_serial`]);
/// it lets the export manifest and history fingerprints work.
pub fn scan(root: &Path, volume_serial: Option<&str>, lib: &Library) -> Result<AdoptPlan> {
    let serial = volume_serial.map(str::trim).filter(|s| !s.is_empty());
    let report = validate::read(root)?;
    let mut notes = Vec::new();
    for f in &report.findings {
        match f {
            Finding::PdbMissing => {
                return Err(AdoptError::NoLibrary {
                    path: root.to_path_buf(),
                })
            }
            Finding::PdbUnreadable { message } => {
                return Err(AdoptError::Unreadable {
                    message: message.clone(),
                })
            }
            other => notes.push(format!("The stick's database: {other}")),
        }
    }
    let rows = read_stick_tracks(root)?;
    let written_by_reks = match serial {
        Some(s) => !lib.manifest(s)?.is_empty(),
        None => false,
    };

    // History first (D23): planned here, imported first by `apply`.
    let (history, history_sessions, history_text) = match read_play_history(
        &HistorySource::Stick(root.to_path_buf()),
        &HistoryReadOptions {
            stick_serial: serial.map(str::to_owned),
            hist1_dates: false,
        },
    ) {
        Ok(h) => {
            let plan = history::scan(&h.to_source_history(), lib)?;
            let n = count_to_import(&plan);
            let text = if n == 0 {
                "No play history to import.".to_owned()
            } else {
                format!(
                    "{} imported first (mark practice sessions in History afterwards).",
                    plural(n, "play history session is", "play history sessions are")
                )
            };
            (Some(plan), n, text)
        }
        Err(e) => {
            tracing::warn!(error = %e, "adopt: play history unreadable");
            (
                None,
                0,
                format!(
                    "The play history could not be read ({e}). The stick is not changed; \
                     the first export asks what to do with it."
                ),
            )
        }
    };

    // The stick tree, depth first; playlists only, with their folder path.
    let ordered = tree_order(&report.playlists);
    let mut ids: Vec<u32> = Vec::new();
    let mut seen_ids = HashSet::new();
    for (p, _) in &ordered {
        for &e in &p.entries {
            if seen_ids.insert(e) {
                ids.push(e);
            }
        }
    }
    let sources: Vec<SourceEntry> = ids
        .iter()
        .map(|id| match rows.get(id) {
            Some(t) => source_entry(t),
            None => SourceEntry::default(),
        })
        .collect();
    let matches = history::match_entries(serial, &sources, lib)?;
    let by_id: HashMap<u32, (&SourceEntry, history::EntryMatch)> = ids
        .iter()
        .zip(sources.iter().zip(matches))
        .map(|(&id, (s, m))| (id, (s, m)))
        .collect();

    let tree = lib.playlists()?.nodes;
    let lib_paths = library_paths(&tree);
    let names: Vec<String> = ordered
        .iter()
        .filter(|(p, _)| !p.is_folder)
        .map(|(p, _)| p.name.clone())
        .collect();
    let same_name: Vec<PlaylistId> = tree
        .iter()
        .filter(|n| !n.is_folder && names.contains(&n.name))
        .map(|n| n.id)
        .collect();
    let lib_tracks = lib.playlist_track_ids(&same_name)?;

    let mut playlists = Vec::new();
    let mut unmatched = Vec::new();
    let mut unmatched_seen = HashSet::new();
    let mut total_tracks = 0u32;
    let mut total_unmatched = 0u32;
    for (p, folders) in ordered.into_iter().filter(|(p, _)| !p.is_folder) {
        let mut entries = Vec::with_capacity(p.entries.len());
        let (mut matched, mut missing, mut dividers) = (0u32, 0u32, 0u32);
        for id in &p.entries {
            let Some((src, m)) = by_id.get(id) else {
                continue;
            };
            match (m.kind, m.track_id) {
                (EntryKind::Divider, _) => dividers += 1,
                (EntryKind::Track, Some(_)) => matched += 1,
                (EntryKind::Track, None) => {
                    missing += 1;
                    if unmatched_seen.insert(*id) {
                        unmatched.push(UnmatchedTrack {
                            stick_path: src.stick_path.clone(),
                            title: src.title.clone(),
                            artist: src.artist.clone(),
                        });
                    }
                }
            }
            entries.push(AdoptEntry {
                stick_path: src.stick_path.clone(),
                title: src.title.clone(),
                artist: src.artist.clone(),
                kind: m.kind,
                track_id: m.track_id,
                matched_by: m.matched_by,
            });
        }
        total_tracks += matched + missing;
        total_unmatched += missing;
        let tracks: Vec<TrackId> = entries.iter().filter_map(|e| e.track_id).collect();
        let link = if missing == 0 {
            tree.iter()
                .filter(|n| !n.is_folder && n.name == p.name)
                .filter(|n| lib_tracks.get(&n.id) == Some(&tracks))
                .map(|n| LinkCandidate {
                    playlist_id: n.id,
                    path_text: lib_paths.get(&n.id).cloned().unwrap_or_default(),
                })
                .collect()
        } else {
            Vec::new()
        };
        let mut path = folders;
        path.push(p.name.clone());
        let n = matched + missing;
        let mut text = plural(n, "track", "tracks");
        if missing > 0 {
            text.push_str(&format!(", {missing} not in your library"));
        }
        if dividers > 0 {
            text.push_str(&format!(
                ", {} left out",
                plural(dividers, "divider", "dividers")
            ));
        }
        if !link.is_empty() {
            text.push_str("; the same playlist is in your library");
        }
        playlists.push(AdoptPlaylist {
            pdb_id: p.pdb_id,
            path_text: path.join(" / "),
            path,
            entries,
            matched,
            unmatched: missing,
            dividers,
            link,
            default_choice: AdoptChoice::Create,
            text,
        });
    }

    let n_playlists = u32::try_from(playlists.len()).unwrap_or(u32::MAX);
    let summary = format!(
        "{}, {}: {} in your library, {} not",
        plural(n_playlists, "playlist", "playlists"),
        plural(total_tracks, "track entry", "track entries"),
        total_tracks - total_unmatched,
        total_unmatched
    );
    let unmatched_text = if unmatched.is_empty() {
        String::new()
    } else {
        format!(
            "{} not in your library: removed from the stick on the first reks sync \
             unless you add the files to your library.",
            plural(
                u32::try_from(unmatched.len()).unwrap_or(u32::MAX),
                "track is",
                "tracks are"
            )
        )
    };
    if written_by_reks {
        notes.push("reks has exported to this stick before; its tracks match exactly.".to_owned());
    }
    if serial.is_none() {
        notes.push(
            "This computer gives no volume ID for the stick, so tracks match by path and \
             tags only."
                .to_owned(),
        );
    }
    Ok(AdoptPlan {
        volume_serial: serial.map(str::to_owned),
        written_by_reks,
        history,
        history_sessions,
        history_text,
        playlists,
        unmatched,
        summary,
        unmatched_text,
        notes,
    })
}

/// Applies `plan` with `choices`: imports the history first, then creates
/// and links playlists and adds them to the device's profile. Writes only
/// the library; a choice the plan does not allow writes nothing.
pub fn apply(plan: &AdoptPlan, choices: &AdoptChoices, lib: &Library) -> Result<AdoptReport> {
    let device = lib.device(choices.device_id)?;
    if let (Some(expected), Some(found)) = (&device.volume_serial, &plan.volume_serial) {
        if expected != found {
            return Err(AdoptError::WrongStick {
                name: device.name,
                expected: expected.clone(),
                found: found.clone(),
            });
        }
    }
    for &pdb_id in choices.playlists.keys() {
        if !plan.playlists.iter().any(|p| p.pdb_id == pdb_id) {
            return Err(AdoptError::UnknownPlaylist { pdb_id });
        }
    }
    let mut write = AdoptWrite {
        device_id: device.id,
        stick_name: device.name.clone(),
        ..AdoptWrite::default()
    };
    let mut skipped = 0u32;
    let mut unmatched_seen = HashSet::new();
    let mut unmatched = Vec::new();
    for p in &plan.playlists {
        let choice = choices
            .playlists
            .get(&p.pdb_id)
            .copied()
            .unwrap_or(p.default_choice);
        let tracks: Vec<TrackId> = p.entries.iter().filter_map(|e| e.track_id).collect();
        match choice {
            AdoptChoice::Skip => {
                skipped += 1;
                continue;
            }
            AdoptChoice::Link { playlist_id } => {
                if !p.link.iter().any(|c| c.playlist_id == playlist_id) {
                    return Err(AdoptError::LinkNotOffered {
                        pdb_id: p.pdb_id,
                        playlist_id,
                    });
                }
                write.links.push(AdoptLink {
                    playlist_id,
                    tracks,
                });
            }
            AdoptChoice::Create => write.creates.push(AdoptCreate {
                folders: p.path[..p.path.len().saturating_sub(1)].to_vec(),
                name: p.path.last().cloned().unwrap_or_default(),
                tracks,
            }),
        }
        for e in &p.entries {
            if e.kind == EntryKind::Track
                && e.track_id.is_none()
                && unmatched_seen.insert((&e.stick_path, &e.title))
            {
                unmatched.push(UnmatchedTrack {
                    stick_path: e.stick_path.clone(),
                    title: e.title.clone(),
                    artist: e.artist.clone(),
                });
            }
        }
    }

    // History first (D23). Its own transaction: an imported session stays
    // imported if the playlist step fails, and a rerun skips it.
    let history = match &plan.history {
        Some(h) if count_to_import(h) > 0 => Some(history::import(h, &choices.history, lib)?),
        _ => None,
    };
    let written = lib.adopt_playlists(&write)?;

    let mut parts = Vec::new();
    if !written.created.is_empty() {
        parts.push(format!(
            "{} created",
            plural(len32(&written.created), "playlist", "playlists")
        ));
    }
    if !written.linked.is_empty() {
        parts.push(format!("{} linked", len32(&written.linked)));
    }
    if skipped > 0 {
        parts.push(format!("{skipped} skipped"));
    }
    let mut summary = if parts.is_empty() {
        "No playlists adopted".to_owned()
    } else {
        parts.join(", ")
    };
    if let Some(h) = &history {
        let n = len32(&h.created) + len32(&h.extended);
        summary.push_str(&format!(
            "; {} imported",
            plural(n, "play history session", "play history sessions")
        ));
    }
    summary.push('.');
    tracing::info!(
        created = written.created.len(),
        linked = written.linked.len(),
        skipped,
        "stick adopted"
    );
    Ok(AdoptReport {
        history,
        folder_id: written.folder_id,
        created: written.created,
        linked: written.linked,
        skipped,
        unmatched,
        summary,
    })
}

fn len32<T>(v: &[T]) -> u32 {
    u32::try_from(v.len()).unwrap_or(u32::MAX)
}

fn count_to_import(plan: &HistoryImportPlan) -> u32 {
    let n = plan
        .sessions
        .iter()
        .filter(|s| !matches!(s.action, SessionAction::Skip(_)))
        .count();
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn source_entry(t: &PdbTrack) -> SourceEntry {
    SourceEntry {
        stick_path: Some(t.stick_path.clone()),
        title: Some(t.title.clone()),
        artist: t.artist.clone(),
        duration_ms: t.duration_ms,
    }
}

/// Stick tree nodes depth first, siblings by `sort_order` (table order
/// breaks ties), each with the names of the folders above it. A node whose
/// parent is missing counts as top level; a parent cycle is cut.
fn tree_order(nodes: &[StickPlaylist]) -> Vec<(&StickPlaylist, Vec<String>)> {
    let ids: HashSet<u32> = nodes.iter().map(|p| p.pdb_id).collect();
    let mut children: HashMap<u32, Vec<usize>> = HashMap::new();
    for (i, p) in nodes.iter().enumerate() {
        let parent = if ids.contains(&p.parent_id) && p.parent_id != p.pdb_id {
            p.parent_id
        } else {
            0
        };
        children.entry(parent).or_default().push(i);
    }
    for list in children.values_mut() {
        list.sort_by_key(|&i| (nodes[i].sort_order, i));
    }
    let mut out = Vec::new();
    let mut visited = HashSet::new();
    let mut stack: Vec<(usize, Vec<String>)> = children
        .get(&0)
        .map(|l| l.iter().rev().map(|&i| (i, Vec::new())).collect())
        .unwrap_or_default();
    while let Some((i, folders)) = stack.pop() {
        if !visited.insert(i) {
            continue;
        }
        let p = &nodes[i];
        if p.is_folder {
            let mut below = folders.clone();
            below.push(p.name.clone());
            if let Some(kids) = children.get(&p.pdb_id) {
                stack.extend(kids.iter().rev().map(|&k| (k, below.clone())));
            }
        }
        out.push((p, folders));
    }
    out
}

/// `Folder / Name` of every library node.
fn library_paths(nodes: &[reks_core::PlaylistNode]) -> HashMap<PlaylistId, String> {
    let by_id: HashMap<PlaylistId, &reks_core::PlaylistNode> =
        nodes.iter().map(|n| (n.id, n)).collect();
    nodes
        .iter()
        .map(|n| {
            let mut path = vec![n.name.as_str()];
            let mut cur = n.parent_id;
            while let Some(p) = cur.and_then(|id| by_id.get(&id)) {
                if path.len() > nodes.len() {
                    break;
                }
                path.push(&p.name);
                cur = p.parent_id;
            }
            path.reverse();
            (n.id, path.join(" / "))
        })
        .collect()
}
