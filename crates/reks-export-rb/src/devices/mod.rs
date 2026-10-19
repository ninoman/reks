//! Stick profiles (D27, Phase 2 task 15; spec `docs/stick-profiles.md`).
//!
//! Each known stick is a [`Device`] keyed by the OS's own volume ID (D28)
//! with a saved, live selection of playlists and folders (its profile). Rows
//! live in `reks-core` ([`reks_core::device`]); this module is the API the
//! app calls, here because it needs [`DeviceInfo`], [`Selection`], the
//! stick read-back and [`ExportReport`]:
//!
//! - [`identify`] a mounted stick: [`DeviceMatch::Known`] on a volume ID hit
//!   (no prompt), [`DeviceMatch::Ambiguous`] ("Is this '<name>'?") when an
//!   unknown or missing ID comes with a known label and size, otherwise
//!   [`DeviceMatch::New`] or [`DeviceMatch::Foreign`] (a rekordbox library
//!   reks did not write; adopting it is Phase 2.5).
//! - [`create`], [`link`] (confirming `Ambiguous`; replaces the stored ID),
//!   [`rename`], [`set_target`], [`set_selection`].
//! - [`selection`]: the profile as the export [`Selection`]. A Sync is the
//!   normal plan → dry run → backup → write → validate → manifest pipeline on
//!   it, then [`record_sync`].
//! - [`profile_status`]: per playlist [`SyncState`], "frees X MB" per profile
//!   entry and the capacity line, worded here so the UI only renders. Reads
//!   the stick when it is mounted; otherwise what the last sync recorded.
//!
//! Nothing here writes to a stick: the stick is only read
//! ([`validate::read`]), and no reks file is ever put on it (D27, D28).

mod status;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::path::Path;

use reks_core::{Library, NewDevice, PlaylistId, PlaylistNode, TrackId};
use serde::Serialize;

use crate::apply::{ExportReport, ExportStatus};
use crate::device::{self, DeviceInfo};
use crate::plan::{human_bytes, ExportPlan, Selection};
use crate::validate::{self, Finding, StickTrack, ValidationReport, PDB_PATH};
use crate::TargetProfile;

pub use reks_core::device::{Device, DeviceId, SyncedPlaylist, SyncedRecord};
pub use status::SyncState;

/// Errors of the profile API.
#[derive(Debug, thiserror::Error)]
pub enum DevicesError {
    #[error(transparent)]
    Library(#[from] reks_core::Error),
    #[error("cannot read the stick at {path}: {source}")]
    Stick {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// [`record_sync`] on a cancelled export: the stick kept its old library.
    #[error("the export was cancelled and the stick kept its previous library; nothing recorded")]
    NotWritten,
    /// [`record_sync`] without a read-back of the new database.
    #[error("the new stick database could not be read back, so what the stick holds is unknown; nothing recorded")]
    NotReadBack,
    /// [`record_sync`] with a report from another stick.
    #[error("this export went to another stick (volume ID {found}, '{name}' has {expected}); nothing recorded")]
    WrongStick {
        name: String,
        expected: String,
        found: String,
    },
}

pub type Result<T> = std::result::Result<T, DevicesError>;

/// What a mounted stick is to this library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum DeviceMatch {
    /// The volume ID is known: no prompt.
    Known { device: Device },
    /// No ID match, but known stick(s) with this label and size. Ask
    /// `prompt`; on yes call [`link`] with the chosen candidate and
    /// `volume_serial`. Without a volume ID this comes every time.
    Ambiguous {
        volume_serial: Option<String>,
        reason: AmbiguousReason,
        candidates: Vec<Device>,
        prompt: String,
        note: String,
    },
    /// A stick reks has not seen: offer "New stick" ([`create`]).
    New {
        volume_serial: Option<String>,
        suggested_name: String,
        prompt: String,
        note: String,
    },
    /// Unknown, and it holds a rekordbox library reks did not write (no
    /// export manifest for its ID). "New stick" replaces that library on the
    /// first sync; "Adopt" comes in Phase 2.5.
    Foreign {
        volume_serial: Option<String>,
        suggested_name: String,
        prompt: String,
        note: String,
    },
}

/// Why [`DeviceMatch::Ambiguous`] needs a confirm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AmbiguousReason {
    /// The stick has a volume ID this library does not know: reformatted, or
    /// the library moved to another OS (D28).
    SerialChanged,
    /// The OS gives no volume ID: label and size are all reks has.
    NoSerial,
}

/// Identifies the stick `info`, asking the OS for its volume ID
/// ([`device::volume_serial`]). Reads the stick only to see whether it holds
/// a foreign library.
pub fn identify(info: &DeviceInfo, lib: &Library) -> Result<DeviceMatch> {
    let serial = device::volume_serial(info);
    identify_serial(info, serial.as_deref(), lib)
}

/// [`identify`] with the volume ID already known (`None`: the OS gives none).
pub fn identify_serial(
    info: &DeviceInfo,
    serial: Option<&str>,
    lib: &Library,
) -> Result<DeviceMatch> {
    let serial = serial.map(str::trim).filter(|s| !s.is_empty());
    if let Some(s) = serial {
        if let Some(device) = lib.device_by_serial(s)? {
            return Ok(DeviceMatch::Known { device });
        }
    }
    let volume_serial = serial.map(str::to_owned);
    let candidates = lib.devices_by_label_size(&info.label, info.total_bytes)?;
    if !candidates.is_empty() {
        let reason = if serial.is_some() {
            AmbiguousReason::SerialChanged
        } else {
            AmbiguousReason::NoSerial
        };
        let prompt = match candidates.as_slice() {
            [one] => format!("Is this '{}'?", one.name),
            _ => "Which of your sticks is this?".to_owned(),
        };
        let note = match reason {
            AmbiguousReason::SerialChanged => {
                "Its volume ID is new to reks, but its label and size match: it was reformatted, \
                 or this library moved to another computer. Confirming links it and keeps its \
                 playlists."
            }
            AmbiguousReason::NoSerial => {
                "This computer does not report the stick's volume ID, so reks can only match it \
                 by label and size and asks every time."
            }
        };
        return Ok(DeviceMatch::Ambiguous {
            volume_serial,
            reason,
            candidates,
            prompt,
            note: note.to_owned(),
        });
    }

    let label = info.label.trim();
    let suggested_name = if label.is_empty() { "USB stick" } else { label }.to_owned();
    let root = &info.mount_point;
    let has_library = validate::locate(root, PDB_PATH)
        .map_err(|source| DevicesError::Stick {
            path: root.clone(),
            source,
        })?
        .is_some();
    let written_by_reks = match serial {
        Some(s) => !lib.manifest(s)?.is_empty(),
        None => false,
    };
    if has_library && !written_by_reks {
        let note = if serial.is_some() {
            "It holds a rekordbox library that reks did not write. As a new stick, its \
             playlists are replaced on the first sync (after the dry run and the history \
             backup). Adopting a stick's playlists comes in a later version."
        } else {
            "It holds a rekordbox library, and without a volume ID reks can't tell whether it \
             wrote it. As a new stick, its playlists are replaced on the first sync (after the \
             dry run and the history backup)."
        };
        return Ok(DeviceMatch::Foreign {
            volume_serial,
            suggested_name,
            prompt: "This stick is new to reks.".to_owned(),
            note: note.to_owned(),
        });
    }
    Ok(DeviceMatch::New {
        volume_serial,
        suggested_name,
        prompt: "This stick is new to reks.".to_owned(),
        note: "Name it and tick the playlists it should carry.".to_owned(),
    })
}

/// The stored name of a [`TargetProfile`] (its serde name, `"Cdj3000"`).
pub fn target_name(target: TargetProfile) -> String {
    match serde_json::to_value(target) {
        Ok(serde_json::Value::String(s)) => s,
        _ => format!("{target:?}"),
    }
}

/// The export target of `dev`; an unknown stored name gives the
/// conservative [`TargetProfile::Generic`].
pub fn target(dev: &Device) -> TargetProfile {
    serde_json::from_value(serde_json::Value::String(dev.target.clone())).unwrap_or_else(|_| {
        tracing::warn!(device = dev.id, target = %dev.target, "unknown export target; using Generic");
        TargetProfile::Generic
    })
}

/// Stores the mounted stick `info` as a new device with an empty profile.
/// `serial` is the volume ID [`identify`] reported.
pub fn create(
    lib: &Library,
    info: &DeviceInfo,
    serial: Option<&str>,
    name: &str,
    target: TargetProfile,
) -> Result<Device> {
    Ok(lib.create_device(&NewDevice {
        volume_serial: serial.map(str::to_owned),
        label: info.label.clone(),
        size_bytes: info.total_bytes,
        name: name.to_owned(),
        target: target_name(target),
    })?)
}

/// Confirms [`DeviceMatch::Ambiguous`]: `dev` is the stick `info`. Replaces
/// the stored volume ID with `serial` (kept when `None`) and refreshes label
/// and size.
pub fn link(
    lib: &Library,
    dev: DeviceId,
    info: &DeviceInfo,
    serial: Option<&str>,
) -> Result<Device> {
    Ok(lib.relink_device(dev, serial, &info.label, info.total_bytes)?)
}

/// Renames `dev`.
pub fn rename(lib: &Library, dev: DeviceId, name: &str) -> Result<Device> {
    Ok(lib.rename_device(dev, name)?)
}

/// Sets the player family `dev`'s exports are checked against.
pub fn set_target(lib: &Library, dev: DeviceId, target: TargetProfile) -> Result<Device> {
    Ok(lib.set_device_target(dev, &target_name(target))?)
}

/// Replaces `dev`'s profile with playlists and folders `sel` (a folder means
/// everything below it, live). An unknown id is refused.
pub fn set_selection(lib: &Library, dev: DeviceId, sel: &[PlaylistId]) -> Result<()> {
    Ok(lib.set_device_selection(dev, sel)?)
}

/// `dev`'s profile as the export [`Selection`], in library tree order. Plan
/// it like any selection; the result is the plan of the same hand-picked
/// playlists.
pub fn selection(lib: &Library, dev: DeviceId) -> Result<Selection> {
    let ids: HashSet<PlaylistId> = lib.device_selection(dev)?.into_iter().collect();
    let nodes = lib.playlists()?.nodes;
    Ok(Selection {
        playlists: nodes
            .iter()
            .map(|n| n.id)
            .filter(|id| ids.contains(id))
            .collect(),
    })
}

/// Where [`ProfileStatus`] got the stick's contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StatusSource {
    /// Read from the mounted stick (ground truth).
    Stick,
    /// The last recorded sync.
    LastSync,
    /// Not mounted and never synced: everything is new.
    NeverSynced,
}

/// One row of the status list: a profile playlist, or a stick playlist the
/// next sync removes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistStatus {
    /// The library playlist; `None` for a stick playlist whose library
    /// playlist is gone (or that reks can't place).
    pub playlist_id: Option<PlaylistId>,
    /// Folder names, then the playlist name, root first.
    pub path: Vec<String>,
    /// `Set / Peak`.
    pub path_text: String,
    /// Entries: the library's, or the stick's for `WillRemove`.
    pub tracks: u32,
    pub state: SyncState,
    /// `In sync`, `2 tracks added, 1 removed`, `New`, `Removed from the stick at the next sync`.
    pub text: String,
}

/// One ticked profile entry (playlist or folder).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryStatus {
    pub playlist_id: PlaylistId,
    pub name: String,
    pub path_text: String,
    pub is_folder: bool,
    /// Playlists it puts on the stick (itself, or every playlist below the folder).
    pub playlists: u32,
    /// Distinct tracks in them.
    pub tracks: u32,
    /// Audio bytes unticking only this entry takes off the planned stick:
    /// its tracks that no other entry holds.
    pub frees_bytes: u64,
    /// `Unticking frees 120.4 MB`.
    pub frees_text: String,
}

/// Status of a stick profile, for the Devices panel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileStatus {
    pub device: Device,
    pub source: StatusSource,
    /// `Read from the stick`, `As of the last sync, 2026-10-19 14:03 UTC`, `Not synced yet`.
    pub source_text: String,
    /// Profile playlists in library tree order, then stick playlists to remove.
    pub playlists: Vec<PlaylistStatus>,
    /// Ticked entries in library tree order.
    pub entries: Vec<EntryStatus>,
    /// Audio bytes of the profile, each track once (analysis files and the
    /// database add a little).
    pub planned_bytes: u64,
    /// Stick size when last seen.
    pub capacity_bytes: u64,
    pub over_capacity: bool,
    /// `1.2 GB of 32.0 GB`.
    pub capacity_text: String,
    /// `In sync`, `2 new, 1 changed, 1 to remove`.
    pub summary: String,
    /// Why the status may be less exact (stick unreadable, no volume ID, ...).
    pub notes: Vec<String>,
}

/// The library playlist tree, for paths and folder expansion.
struct Tree<'a> {
    nodes: &'a [PlaylistNode],
    index: HashMap<PlaylistId, usize>,
}

impl<'a> Tree<'a> {
    fn new(nodes: &'a [PlaylistNode]) -> Self {
        Self {
            nodes,
            index: nodes.iter().enumerate().map(|(i, n)| (n.id, i)).collect(),
        }
    }

    fn path(&self, id: PlaylistId) -> Vec<String> {
        let mut path = Vec::new();
        let mut cur = self.index.get(&id).copied();
        while let Some(i) = cur {
            if path.len() > self.nodes.len() {
                break; // a parent cycle can't loop us
            }
            path.push(self.nodes[i].name.clone());
            cur = self.nodes[i]
                .parent_id
                .and_then(|p| self.index.get(&p).copied());
        }
        path.reverse();
        path
    }

    /// The playlists `id` puts on the stick: itself, or every playlist below
    /// the folder (`nodes` is depth first).
    fn playlists_under(&self, id: PlaylistId) -> Vec<PlaylistId> {
        let Some(&i) = self.index.get(&id) else {
            return Vec::new();
        };
        let root = &self.nodes[i];
        if !root.is_folder {
            return vec![root.id];
        }
        self.nodes[i + 1..]
            .iter()
            .take_while(|n| n.depth > root.depth)
            .filter(|n| !n.is_folder)
            .map(|n| n.id)
            .collect()
    }
}

fn path_text(path: &[String]) -> String {
    path.join(" / ")
}

/// Status of `dev`'s profile. `mounted`: the stick's mount point when it is
/// plugged in (already [`identify`]d as `dev`); its database is then read
/// (read-only) and matched to library tracks through the export manifest.
/// Otherwise, or when the stick can't be read, the last recorded sync is used.
pub fn profile_status(
    lib: &Library,
    dev: DeviceId,
    mounted: Option<&Path>,
) -> Result<ProfileStatus> {
    let device = lib.device(dev)?;
    let selected: HashSet<PlaylistId> = lib.device_selection(dev)?.into_iter().collect();
    let nodes = lib.playlists()?.nodes;
    let tree = Tree::new(&nodes);

    // Ticked entries in tree order, and the playlists they cover.
    let ticked: Vec<&PlaylistNode> = nodes.iter().filter(|n| selected.contains(&n.id)).collect();
    let covered: Vec<Vec<PlaylistId>> = ticked.iter().map(|n| tree.playlists_under(n.id)).collect();
    let wanted_ids: HashSet<PlaylistId> = covered.iter().flatten().copied().collect();
    let wanted_order: Vec<PlaylistId> = nodes
        .iter()
        .map(|n| n.id)
        .filter(|id| wanted_ids.contains(id))
        .collect();
    let contents = lib.playlist_entry_sizes(&wanted_order)?;
    let mut size: HashMap<TrackId, u64> = HashMap::new();
    for (t, bytes) in contents.values().flatten() {
        size.insert(*t, *bytes);
    }
    let entries_of = |id: &PlaylistId| -> Vec<TrackId> {
        contents
            .get(id)
            .map(|v| v.iter().map(|(t, _)| *t).collect())
            .unwrap_or_default()
    };
    let wanted: Vec<status::Wanted> = wanted_order
        .iter()
        .map(|id| status::Wanted {
            id: *id,
            path: tree.path(*id),
            entries: entries_of(id),
        })
        .collect();

    let mut notes = Vec::new();
    let (source, have) = stick_contents(lib, &device, mounted, &mut notes)?;
    let rows = status::compare(&wanted, &have);

    let entry_tracks: Vec<HashSet<TrackId>> = covered
        .iter()
        .map(|ids| ids.iter().flat_map(&entries_of).collect())
        .collect();
    let freed = status::frees(&entry_tracks, &size);
    let planned_bytes = status::planned_bytes(&entry_tracks, &size);

    let entries = ticked
        .iter()
        .zip(&covered)
        .zip(&entry_tracks)
        .zip(freed)
        .map(|(((n, ids), tracks), frees_bytes)| EntryStatus {
            playlist_id: n.id,
            name: n.name.clone(),
            path_text: path_text(&tree.path(n.id)),
            is_folder: n.is_folder,
            playlists: u32::try_from(ids.len()).unwrap_or(u32::MAX),
            tracks: u32::try_from(tracks.len()).unwrap_or(u32::MAX),
            frees_bytes,
            frees_text: frees_text(frees_bytes, tracks.is_empty()),
        })
        .collect();

    let playlists: Vec<PlaylistStatus> = rows
        .into_iter()
        .map(|r| PlaylistStatus {
            text: state_text(&r.state),
            path_text: path_text(&r.path),
            playlist_id: r.playlist_id,
            path: r.path,
            tracks: r.tracks,
            state: r.state,
        })
        .collect();

    let capacity_bytes = device.size_bytes;
    let over_capacity = planned_bytes > capacity_bytes;
    let capacity_text = if over_capacity {
        format!(
            "{} planned, more than the stick's {}",
            human_bytes(planned_bytes),
            human_bytes(capacity_bytes)
        )
    } else {
        format!(
            "{} of {}",
            human_bytes(planned_bytes),
            human_bytes(capacity_bytes)
        )
    };
    let source_text = match source {
        StatusSource::Stick => "Read from the stick".to_owned(),
        StatusSource::LastSync => match &device.last_synced_at {
            Some(t) => format!("As of the last sync, {}", time_text(t)),
            None => "As of the last sync".to_owned(),
        },
        StatusSource::NeverSynced => "Not synced yet".to_owned(),
    };
    Ok(ProfileStatus {
        summary: summary(&playlists),
        device,
        source,
        source_text,
        playlists,
        entries,
        planned_bytes,
        capacity_bytes,
        over_capacity,
        capacity_text,
        notes,
    })
}

/// What the stick holds: read from it when mounted and readable, else the
/// last recorded sync.
fn stick_contents(
    lib: &Library,
    device: &Device,
    mounted: Option<&Path>,
    notes: &mut Vec<String>,
) -> Result<(StatusSource, Vec<SyncedPlaylist>)> {
    let recorded = lib.device_synced(device.id)?;
    let fallback = |recorded: Vec<SyncedRecord>| {
        let source = if device.last_synced_at.is_some() {
            StatusSource::LastSync
        } else {
            StatusSource::NeverSynced
        };
        (source, recorded.into_iter().map(|r| r.playlist).collect())
    };
    let Some(root) = mounted else {
        return Ok(fallback(recorded));
    };
    let Some(serial) = device.volume_serial.as_deref() else {
        notes.push(
            "Without a volume ID reks can't match the stick's tracks to the library; showing the \
             last recorded sync."
                .to_owned(),
        );
        return Ok(fallback(recorded));
    };
    let report = match validate::read(root) {
        Ok(r) => r,
        Err(e) => {
            notes.push(format!(
                "Could not read the stick ({e}); showing the last recorded sync."
            ));
            return Ok(fallback(recorded));
        }
    };
    if report.findings.contains(&Finding::PdbMissing) {
        notes.push("The stick holds no rekordbox library yet.".to_owned());
        return Ok((StatusSource::Stick, Vec::new()));
    }
    if report.tracks.is_empty() && report.playlists.is_empty() && !report.findings.is_empty() {
        notes
            .push("The stick's database can't be read; showing the last recorded sync.".to_owned());
        return Ok(fallback(recorded));
    }
    if !report.findings.is_empty() {
        notes.push(
            "Parts of the stick's database are damaged; this status may be incomplete.".to_owned(),
        );
    }
    let manifest: HashMap<String, Option<TrackId>> = lib
        .manifest(serial)?
        .into_iter()
        .map(|m| (m.stick_path, m.track_id))
        .collect();
    let mut have = stick_playlists(&report, |t| manifest.get(&t.file_path).copied().flatten());
    // The stick doesn't know library playlist ids: take them from the last
    // recorded sync, by path.
    assign_ids(
        &mut have,
        recorded
            .iter()
            .filter_map(|r| Some((r.playlist.path.clone(), r.playlist.playlist_id?))),
    );
    Ok((StatusSource::Stick, have))
}

/// The stick's playlists (not folders) in tree order, entries mapped to
/// library tracks with `track_of`. `playlist_id` is left `None`.
fn stick_playlists(
    report: &ValidationReport,
    track_of: impl Fn(&StickTrack) -> Option<TrackId>,
) -> Vec<SyncedPlaylist> {
    let rows: HashMap<u32, &StickTrack> = report.tracks.iter().map(|t| (t.pdb_id, t)).collect();
    let pls = &report.playlists;
    let index: HashMap<u32, usize> = pls.iter().enumerate().map(|(i, p)| (p.pdb_id, i)).collect();
    // Children in sort order, table order breaking ties (as players show them).
    let mut order: Vec<usize> = (0..pls.len()).collect();
    order.sort_by_key(|&i| (pls[i].sort_order, i));
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); pls.len()];
    let mut roots = Vec::new();
    for i in order {
        match index.get(&pls[i].parent_id) {
            Some(&parent) if pls[i].parent_id != 0 && parent != i => children[parent].push(i),
            _ => roots.push(i),
        }
    }

    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut stack: Vec<(usize, Vec<String>)> =
        roots.into_iter().rev().map(|i| (i, Vec::new())).collect();
    while let Some((i, mut path)) = stack.pop() {
        if !seen.insert(i) {
            continue; // a damaged tree with a cycle
        }
        let p = &pls[i];
        path.push(p.name.clone());
        if p.is_folder {
            for &c in children[i].iter().rev() {
                stack.push((c, path.clone()));
            }
        } else {
            out.push(SyncedPlaylist {
                playlist_id: None,
                path,
                entries: p
                    .entries
                    .iter()
                    .map(|id| rows.get(id).and_then(|t| track_of(t)))
                    .collect(),
            });
        }
    }
    out
}

/// Gives each playlist in `have` the id of the first unused `(path, id)`
/// with its path.
fn assign_ids(have: &mut [SyncedPlaylist], ids: impl Iterator<Item = (Vec<String>, PlaylistId)>) {
    let mut by_path: HashMap<Vec<String>, std::collections::VecDeque<PlaylistId>> = HashMap::new();
    for (path, id) in ids {
        by_path.entry(path).or_default().push_back(id);
    }
    for p in have {
        p.playlist_id = by_path.get_mut(&p.path).and_then(|q| q.pop_front());
    }
}

/// Records what a sync wrote to `dev`, after `apply` returned `report` for
/// `plan`: the read-back's playlist tree and entries (ground truth, also for
/// [`ExportStatus::Problems`]), with library playlist ids from the plan.
/// Refuses a cancelled export (the old library is still on the stick), one
/// without a read-back, and a report from a stick with another volume ID.
/// Returns the number of playlists recorded.
pub fn record_sync(
    lib: &Library,
    dev: DeviceId,
    plan: &ExportPlan,
    report: &ExportReport,
) -> Result<usize> {
    let device = lib.device(dev)?;
    if report.status == ExportStatus::Cancelled {
        return Err(DevicesError::NotWritten);
    }
    if let (Some(expected), Some(found)) = (&device.volume_serial, &report.volume_serial) {
        if expected != found {
            return Err(DevicesError::WrongStick {
                name: device.name,
                expected: expected.clone(),
                found: found.clone(),
            });
        }
    }
    let read_back = report
        .validation
        .as_ref()
        .ok_or(DevicesError::NotReadBack)?;
    if read_back.tracks.is_empty()
        && read_back.playlists.is_empty()
        && read_back
            .findings
            .iter()
            .any(|f| matches!(f, Finding::PdbMissing | Finding::PdbUnreadable { .. }))
    {
        return Err(DevicesError::NotReadBack);
    }

    let mut synced = stick_playlists(read_back, |t| t.track_id);
    let nodes: Vec<PlaylistNode> = plan
        .playlists
        .iter()
        .map(|p| PlaylistNode {
            id: p.id,
            parent_id: p.parent_id,
            name: p.name.clone(),
            is_folder: p.is_folder,
            depth: 0,
            position: 0,
            entries: 0,
        })
        .collect();
    let tree = Tree::new(&nodes);
    assign_ids(
        &mut synced,
        plan.playlists
            .iter()
            .filter(|p| !p.is_folder)
            .map(|p| (tree.path(p.id), p.id)),
    );
    Ok(lib.record_device_sync(dev, &synced)?)
}

fn plural(n: u32, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

fn state_text(state: &SyncState) -> String {
    match state {
        SyncState::InSync => "In sync".to_owned(),
        SyncState::New => "New on the stick at the next sync".to_owned(),
        SyncState::WillRemove => "Removed from the stick at the next sync".to_owned(),
        SyncState::Changed {
            added,
            removed,
            reordered,
            moved_from,
        } => {
            let mut parts = Vec::new();
            if *added > 0 {
                parts.push(format!("{} added", plural(*added, "track", "tracks")));
            }
            if *removed > 0 {
                parts.push(format!("{} removed", plural(*removed, "track", "tracks")));
            }
            if *reordered {
                parts.push("order changed".to_owned());
            }
            if let Some(from) = moved_from {
                parts.push(format!("was '{}' on the stick", path_text(from)));
            }
            let text = parts.join(", ");
            let mut chars = text.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().chain(chars).collect(),
                None => "Changed".to_owned(),
            }
        }
    }
}

fn frees_text(bytes: u64, empty: bool) -> String {
    if empty {
        "No tracks".to_owned()
    } else if bytes == 0 {
        "Unticking frees nothing: other ticked playlists hold all its tracks".to_owned()
    } else {
        format!("Unticking frees {}", human_bytes(bytes))
    }
}

fn summary(rows: &[PlaylistStatus]) -> String {
    let (mut new, mut changed, mut removed) = (0u32, 0u32, 0u32);
    for r in rows {
        match r.state {
            SyncState::InSync => {}
            SyncState::New => new += 1,
            SyncState::Changed { .. } => changed += 1,
            SyncState::WillRemove => removed += 1,
        }
    }
    if rows.is_empty() {
        return "No playlists in this profile".to_owned();
    }
    let mut parts = Vec::new();
    if new > 0 {
        parts.push(format!("{new} new"));
    }
    if changed > 0 {
        parts.push(format!("{changed} changed"));
    }
    if removed > 0 {
        parts.push(format!("{removed} to remove"));
    }
    if parts.is_empty() {
        "In sync".to_owned()
    } else {
        parts.join(", ")
    }
}

/// `2026-10-19T14:03:07.123Z` → `2026-10-19 14:03 UTC`.
fn time_text(iso: &str) -> String {
    match (iso.get(..10), iso.get(11..16)) {
        (Some(date), Some(time)) => format!("{date} {time} UTC"),
        _ => iso.to_owned(),
    }
}
