//! Export planner: turns a playlist selection into an [`ExportPlan`], the pure
//! description of what an export will copy, delete and warn about. The plan is
//! the dry run shown to the user; `apply` (task 9) executes it. Nothing here
//! writes to the stick: the planner only lists directories, reads file
//! metadata and compares a few sampled bytes of candidate files.
//!
//! See `docs/phases/phase-2.md` task 4 for the rules, in short:
//!
//! - Audio goes to `/Contents/<Artist>/<Album>/<file>` ([`fsname::contents_path`],
//!   ASCII per D24). A track whose current source is already on the stick
//!   keeps that path ([`stick`] module docs say how a match is decided), so
//!   paths stay stable across re-exports and an unchanged re-export copies
//!   nothing.
//! - New copies never overwrite a file already on the stick: new paths are
//!   deduplicated against every path the stick holds. If the write fails, the
//!   old `export.pdb` still points at intact files.
//! - Stale files (not referenced by the plan) under `/Contents`,
//!   `/PIONEER/USBANLZ` and `/PIONEER/Artwork` are deleted unless
//!   [`ExportOptions::keep_stale`]. Nothing else on the stick is touched.
//! - [`ExportPlan::errors`] block the export; [`ExportPlan::warnings`] don't.
//! - History safeguard (D23, task 12): the plan carries what players left on
//!   the stick ([`ExportPlan::history`]) and the database files `apply` backs
//!   up before its first write ([`ExportPlan::backups`], see
//!   [`ExportPlan::backup_plan`]), with a warning whenever the stick holds or
//!   may hold history.
//! - Export guard (Phase 2.5 task 8): [`plan`] also checks the stick's history
//!   sessions against the library ([`guard_history`]); sessions not imported
//!   yet block the plan until [`ExportOptions::history_chosen`].
//!
//! - OneLibrary (Phase 4 task 8, D14): with [`ExportOptions::onelibrary`],
//!   [`plan_onelibrary`] asks a [`OneLibraryExport`] to plan
//!   `exportLibrary.db` ([`ExportPlan::onelibrary`]). Experimental, off by
//!   default; a missing key blocks, and history in the stick's existing
//!   `exportLibrary.db` goes through the export guard.
//! - Playlist dividers (D22) become planned tracks backed by a generated
//!   silent WAV when [`ExportOptions::dividers`] is on ([`crate::divider`]);
//!   otherwise they are left out.
//!
//! ANLZ comes from an [`AnlzLocator`] ([`CacheLocator`] for rekordbox's local
//! cache) or from reks's own analysis through a [`ReksLocator`]
//! ([`ReksCache`], built with [`crate::reksanlz`]); [`NoAnlz`] finds neither.
//! [`ExportOptions::anlz_preference`] says which comes first when both have
//! a track; tracks with neither follow [`ExportOptions::missing_analysis`].

mod guard;
mod locate;
mod onelibrary;
mod source;
mod stick;
mod text;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use reks_audio::{Kind, Probe};
use reks_core::fsname;
use reks_core::history::HistoryImportPlan;
use reks_core::{AnlzPreference, Library, PlaylistId, TrackId};
use serde::{Deserialize, Serialize};

use crate::audiocheck::{self, Severity};
use crate::device::{DeviceInfo, FsKind};
use crate::divider::{self, DividerInfo};
use crate::history::{self, BackupFile, BackupPlan, HistoryError, HistoryPolicy, StickHistory};
use crate::onelibrary::OneLibraryPlan;
use crate::reksanlz;
use crate::TargetProfile;

pub use guard::{guard_history, HistoryGuard, UnimportedSession};
pub use locate::CacheLocator;
pub use onelibrary::plan_onelibrary;
pub use reksanlz::ReksCache;
pub use source::{load, SourceTrack, Sources};
pub use stick::anlz_dir;
pub use text::human_bytes;

/// Playlists (and folders, meaning everything below them) to export.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Selection {
    pub playlists: Vec<PlaylistId>,
}

/// What to do with a track that has no analysis: none in rekordbox's cache
/// and none of reks's own (or [`AnlzPreference::RekordboxOnly`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MissingAnalysis {
    /// Leave the track out, with a warning (phase 2 default).
    #[default]
    Skip,
    /// Let the writer generate approximate ANLZ from the audio (baken).
    Generate,
    /// Export without ANLZ: plays, but no waveform, beatgrid or cues.
    ExportWithout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ExportOptions {
    /// The oldest player the stick must work on.
    pub target: TargetProfile,
    /// Leave stale files on the stick instead of deleting them.
    pub keep_stale: bool,
    pub missing_analysis: MissingAnalysis,
    /// Write artwork. When on, `/PIONEER/Artwork` belongs to the writer (it
    /// numbers pictures while writing), so the plan lists no deletes there;
    /// when off, everything in it is stale.
    pub artwork: bool,
    /// Leave out tracks with blocking problems (unsupported audio, missing
    /// source) instead of blocking the whole export.
    pub skip_unsupported: bool,
    /// What to do with the play history on the stick (D23).
    pub history: HistoryPolicy,
    /// The user explicitly confirmed [`HistoryPolicy::Discard`]; without it a
    /// stick that may hold history blocks the plan.
    pub discard_history_confirmed: bool,
    /// The user picked [`Self::history`] in the export guard, knowing the
    /// stick holds sessions the library has not imported
    /// ([`guard_history`]). Without it such a stick blocks the plan.
    pub history_chosen: bool,
    /// The `export.dividers` flag: write playlist dividers as silent tracks
    /// ([`crate::divider`]). Off by default and UNVERIFIED until the Phase
    /// 2.5 hardware session passes on CDJ-2000NXS2 and CDJ-3000.
    pub dividers: bool,
    /// rekordbox's analysis or reks's first, when both have a track (the
    /// app setting of the same name, [`reks_core::settings::ANLZ_PREFERENCE`]).
    pub anlz_preference: AnlzPreference,
    /// Also write `exportLibrary.db` for OneLibrary players
    /// ([`plan_onelibrary`]). Off by default; experimental and UNVERIFIED
    /// on players until a Phase 4 hardware test passes.
    pub onelibrary: bool,
    /// The `export.active_loops` flag: the earliest memory loop marked
    /// active in the library (task 17) goes to the stick as the track's
    /// active loop (`PCPT` status 4, through baken's XML marker,
    /// [`reks_core::rbxml::ActiveLoops::BakenMarker`]). Off by default and
    /// UNVERIFIED until a player enters it in a hardware test.
    pub active_loops: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            target: TargetProfile::Generic,
            keep_stale: false,
            missing_analysis: MissingAnalysis::Skip,
            artwork: false,
            skip_unsupported: false,
            history: HistoryPolicy::BackUp,
            discard_history_confirmed: false,
            history_chosen: false,
            dividers: false,
            anlz_preference: AnlzPreference::RekordboxFirst,
            onelibrary: false,
            active_loops: false,
        }
    }
}

/// Where a track's ANLZ files come from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum AnlzSource {
    /// rekordbox's local analysis: the `.DAT` file; `.EXT`/`.2EX` sit beside it.
    RekordboxCache {
        dat: PathBuf,
    },
    /// Built from reks's own analysis ([`crate::reksanlz`]) by the writer.
    Reks,
    /// Built by the writer from rekordbox's cached waveforms (this `.DAT`
    /// and the `.EXT`/`.2EX` beside it) with the library's grid and cues: a
    /// grid the user owns when reks has no analysis of the track (D10,
    /// "Grid editing"), so the edit reaches the stick.
    ReksGrid {
        dat: PathBuf,
    },
    /// Generated by the writer from the audio (approximate).
    Generated,
    None,
}

impl AnlzSource {
    /// reks builds the set ([`crate::reksanlz`]).
    pub fn reks_built(&self) -> bool {
        matches!(self, Self::Reks | Self::ReksGrid { .. })
    }
}

/// Finds existing analysis for a track: [`CacheLocator`] for rekordbox's
/// local cache; [`NoAnlz`] finds nothing.
pub trait AnlzLocator {
    /// The track's `.DAT`, if there is analysis to copy.
    fn locate(&self, track: &SourceTrack) -> Option<PathBuf>;

    /// Why [`Self::locate`] found nothing, for the dry run.
    fn why_missing(&self, _track: &SourceTrack) -> String {
        "no rekordbox analysis found".to_owned()
    }
}

/// Locator that never finds anything.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoAnlz;

impl AnlzLocator for NoAnlz {
    fn locate(&self, _track: &SourceTrack) -> Option<PathBuf> {
        None
    }
}

impl<F: Fn(&SourceTrack) -> Option<PathBuf>> AnlzLocator for F {
    fn locate(&self, track: &SourceTrack) -> Option<PathBuf> {
        self(track)
    }
}

/// Says whether reks's own analysis of a track is there to build ANLZ from:
/// [`ReksCache`] for the library's analysis cache; [`NoAnlz`] has none.
pub trait ReksLocator {
    fn has_analysis(&self, track: &SourceTrack) -> bool;

    /// Why [`Self::has_analysis`] is false, for the dry run.
    fn why_missing(&self, _track: &SourceTrack) -> String {
        "reks has not analysed it yet".to_owned()
    }
}

impl ReksLocator for NoAnlz {
    fn has_analysis(&self, _track: &SourceTrack) -> bool {
        false
    }
}

/// Pure description of what an export will do. Serialisable, shown to the user.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPlan {
    pub device: DeviceInfo,
    pub options: ExportOptions,
    /// Every exported track once, in the order it first appears in the
    /// selected playlists.
    pub tracks: Vec<PlannedTrack>,
    /// The selected playlists and the folders above them, depth first.
    pub playlists: Vec<PlannedPlaylist>,
    /// Audio to copy. Tracks already on the stick are not here.
    pub copies: Vec<FileOp>,
    /// One per exported track.
    pub anlz: Vec<AnlzOp>,
    /// Stale files to remove (empty with `keep_stale`).
    pub deletes: Vec<FileOp>,
    /// What players left on the stick (D23).
    pub history: StickHistory,
    /// Stick database files `apply` copies to the app data dir before any
    /// write (`dst_rel` is the stick file, `reason` [`OpReason::HistoryBackup`]).
    /// Empty when the stick holds no history or with a confirmed `Discard`.
    pub backups: Vec<FileOp>,
    /// Stick sessions the library has not imported (export guard).
    #[serde(default)]
    pub guard: HistoryGuard,
    /// The history scan behind [`Self::guard`]; `apply` imports it under
    /// [`HistoryPolicy::Import`]. Not serialised: a plan read back from JSON
    /// can't import.
    #[serde(skip)]
    pub history_import: Option<HistoryImportPlan>,
    /// The scan of the stick's `exportLibrary.db` history behind
    /// [`HistoryGuard::onelibrary_sessions`] ([`plan_onelibrary`]); imported
    /// with [`Self::history_import`]. Not serialised either.
    #[serde(skip)]
    pub onelibrary_history_import: Option<HistoryImportPlan>,
    pub warnings: Vec<Warning>,
    /// Blockers: `apply` refuses a plan with errors.
    pub errors: Vec<Problem>,
    /// Audio copies, ANLZ copied from the cache and (estimated) ANLZ built
    /// from reks's analysis (generated ANLZ, the PDB and artwork are not
    /// counted).
    pub bytes_to_write: u64,
    /// Free space after the export: free now, minus writes, plus deletes.
    pub bytes_free_after: i64,
    /// Tracks with an audio check the converter fixes
    /// ([`Issue::converter_fixes`](crate::audiocheck::Issue::converter_fixes)),
    /// in plan order: the dry run offers to convert them (Phase 2.6).
    #[serde(default)]
    pub convertible: Vec<TrackId>,
    /// The OneLibrary step ([`plan_onelibrary`]); `None` when it is off or
    /// has no key.
    #[serde(default)]
    pub onelibrary: Option<OneLibraryPlan>,
}

impl ExportPlan {
    /// True when [`Self::errors`] holds anything.
    pub fn is_blocked(&self) -> bool {
        !self.errors.is_empty()
    }

    /// [`Self::backups`] as the input for [`history::run_backups`].
    pub fn backup_plan(&self) -> BackupPlan {
        BackupPlan {
            device_root: self.device.mount_point.clone(),
            volume_serial: self.history.volume_serial.clone(),
            files: self
                .backups
                .iter()
                .map(|f| BackupFile {
                    rel_path: f.dst_rel.trim_start_matches('/').to_owned(),
                    bytes: f.bytes,
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedTrack {
    pub track_id: TrackId,
    pub src: PathBuf,
    /// `/Contents/...`, as stored in `export.pdb` and PPTH.
    pub dst_rel: String,
    /// The stick already holds this file at `dst_rel`: nothing to copy.
    pub on_stick: bool,
    pub kind: Kind,
    pub probe: Probe,
    pub has_grid: bool,
    pub cue_count: u8,
    pub anlz_source: AnlzSource,
    /// Set for a playlist divider (synthetic `track_id`, generated `src`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub divider: Option<DividerInfo>,
}

impl PlannedTrack {
    pub fn is_divider(&self) -> bool {
        self.divider.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedPlaylist {
    pub id: PlaylistId,
    pub parent_id: Option<PlaylistId>,
    pub name: String,
    pub is_folder: bool,
    /// Entries in order (duplicates kept); tracks left out are dropped. Empty
    /// for folders. A smart playlist's: the tracks its rule matches now, in
    /// its own sort.
    pub track_ids: Vec<TrackId>,
    /// A smart playlist (D29), written as a plain playlist of `track_ids`;
    /// it won't update on the stick.
    #[serde(default)]
    pub smart: bool,
}

/// One file to copy or delete.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileOp {
    /// Source on the computer (copies only).
    pub src: Option<PathBuf>,
    /// Path on the stick, `/`-separated from the stick root.
    pub dst_rel: String,
    pub bytes: u64,
    pub track_id: Option<TrackId>,
    pub reason: OpReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OpReason {
    /// The track's current audio is not on the stick.
    NotOnStick,
    /// Not referenced by the new export.
    Stale,
    /// A stick database file copied off before writing (history safeguard).
    HistoryBackup,
    /// `exportLibrary.db`, written by the OneLibrary step ([`crate::onelibrary`]).
    OneLibrary,
    /// rekordbox's file beside `exportLibrary.db` that the OneLibrary step removes.
    OneLibraryAuxiliary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum AnlzOp {
    /// Copy `.DAT` (and `.EXT`/`.2EX` when present) from the cache.
    CopyFromCache {
        track_id: TrackId,
        dat: PathBuf,
        bytes: u64,
    },
    /// Built from reks's analysis; `bytes` is an estimate from above.
    Build {
        track_id: TrackId,
        bytes: u64,
    },
    Generate {
        track_id: TrackId,
    },
    /// Exported without analysis ([`MissingAnalysis::ExportWithout`]).
    Missing {
        track_id: TrackId,
    },
}

/// Something the user should know; the export still runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Warning {
    /// exFAT: only newer players read it (D16).
    ExFat { target: TargetProfile },
    /// HFS+: only some players read it.
    Hfs,
    /// The OS does not report the volume as removable.
    NotRemovable,
    /// rekordbox wrote this stick (`found` lists the evidence, stick-relative).
    /// reks rewrites its Device Library completely and rekordbox will show
    /// its "out of matching" caution afterwards.
    RekordboxStick { found: Vec<String> },
    /// An audio check that does not block the track.
    Audio { track_id: TrackId, message: String },
    /// The track is left out of the export.
    Skipped { track_id: TrackId, reason: String },
    /// Stale files left in place because of `keep_stale`.
    StaleKept { files: usize, bytes: u64 },
    /// A playlist ended up empty after tracks were left out.
    EmptyPlaylist { id: PlaylistId },
    /// A smart playlist is written as a fixed list of the tracks it matches
    /// now (D29); players have no smart playlists.
    SmartPlaylistFixed { id: PlaylistId },
    /// The stick holds, or may hold, play history that the export erases
    /// (baken rebuilds `export.pdb` and removes `exportLibrary.db`). With
    /// `BackUp` the files in [`ExportPlan::backups`] are copied first.
    StickHistory {
        sessions: u32,
        entries: u32,
        /// `export.pdb` could not be read: it may hold history.
        unreadable: bool,
        /// `exportLibrary.db` is present: players may have logged there.
        onelibrary_db: bool,
        policy: HistoryPolicy,
    },
    /// The OS gave no volume serial: backups go under `unknown-serial` and
    /// later history import cannot tell this stick from others.
    NoVolumeSerial,
    /// The OneLibrary export is on: experimental, unverified on players and
    /// unsupported by AlphaTheta (D14).
    OneLibraryExperimental,
}

/// A blocker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Problem {
    NothingSelected,
    PlaylistNotFound {
        id: PlaylistId,
    },
    /// Players cannot read this file system.
    UnsupportedFilesystem {
        fs_name: String,
    },
    SourceMissing {
        track_id: TrackId,
        path: PathBuf,
    },
    /// The source could not be probed or its header checked.
    SourceUnreadable {
        track_id: TrackId,
        path: PathBuf,
        message: String,
    },
    /// An audio check that the target player fails on.
    Unsupported {
        track_id: TrackId,
        message: String,
    },
    /// The planned stick path breaks a FAT32/player rule.
    BadPath {
        track_id: TrackId,
        path: String,
        message: String,
    },
    NotEnoughSpace {
        needed: u64,
        free: u64,
    },
    /// [`HistoryPolicy::Discard`] on a stick that may hold history, without
    /// [`ExportOptions::discard_history_confirmed`].
    DiscardNotConfirmed,
    /// The stick holds history sessions the library has not imported and
    /// the user has not chosen what happens to them
    /// ([`ExportOptions::history_chosen`]).
    HistoryNotImported {
        sessions: u32,
    },
    /// [`HistoryPolicy::Import`] chosen, but the stick's history sessions
    /// could not be read.
    HistoryImportUnavailable {
        message: String,
    },
    /// The OneLibrary export is on but this build has no key built in and
    /// `REKS_ONELIBRARY_KEY` is unset ([`crate::onelibrary::resolve_key`]).
    OneLibraryKeyMissing,
    /// The OneLibrary export is on but this build has no writer
    /// ([`crate::onelibrary::NoWriter`]).
    OneLibraryUnavailable,
    /// The stick's `exportLibrary.db` holds play history sessions the
    /// library has not imported, and the user has not chosen what happens
    /// to them ([`ExportOptions::history_chosen`]; Phase 4 task 11).
    OneLibraryHistoryNotImported {
        sessions: u32,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum PlanError {
    #[error(transparent)]
    Library(#[from] reks_core::Error),
    #[error("cannot read {path} on the stick: {source}")]
    Stick {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// Listing the stick's database files for the backup failed.
    #[error(transparent)]
    History(#[from] HistoryError),
    /// baken could not parse the XML we would hand it.
    #[error("baken cannot parse the exported XML: {0}")]
    Xml(String),
    /// A generated divider file could not be written.
    #[error("cannot write the divider file {}: {source}", path.display())]
    DividerFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// The synthetic divider ids would collide with library track ids.
    #[error("no free track ids for playlist dividers")]
    DividerIds,
}

/// Plans exporting `sel` from `lib` to `dev`. ANLZ lookup goes through
/// `anlz` (usually a [`CacheLocator`] for the same selection) and the
/// library's analysis cache ([`ReksCache`]). Asks the OS for
/// the volume serial ([`crate::device::volume_serial`]). With
/// [`ExportOptions::dividers`], writes the divider files into the library's
/// cache dir ([`divider::expand`]); nothing else is written. Runs the
/// export guard ([`guard_history`]).
pub fn plan(
    lib: &Library,
    sel: &Selection,
    dev: &DeviceInfo,
    opts: &ExportOptions,
    anlz: &dyn AnlzLocator,
) -> Result<ExportPlan, PlanError> {
    let mut sources = load(lib, sel)?;
    if opts.dividers {
        divider::expand(&mut sources, &lib.cache_dir().join(divider::DIVIDER_DIR))?;
    }
    let serial = crate::device::volume_serial(dev);
    let reks = ReksCache::for_library(lib)?;
    let mut plan = plan_sources(sources, dev, serial, opts, anlz, &reks)?;
    guard_history(&mut plan, lib)?;
    Ok(plan)
}

/// [`plan`] on already loaded sources and a known volume serial (also the
/// entry point for tests). Dividers count only once [`divider::expand`]ed;
/// `sources.dividers` is ignored. Without a library there is no export
/// guard: call [`guard_history`] on the result.
pub fn plan_sources(
    sources: Sources,
    dev: &DeviceInfo,
    volume_serial: Option<String>,
    opts: &ExportOptions,
    anlz: &dyn AnlzLocator,
    reks: &dyn ReksLocator,
) -> Result<ExportPlan, PlanError> {
    let mut warnings = Vec::new();
    let mut errors = sources.problems;
    let target = opts.target;

    match &dev.fs {
        FsKind::Fat32 => {}
        FsKind::ExFat => warnings.push(Warning::ExFat { target }),
        FsKind::Hfs => warnings.push(Warning::Hfs),
        FsKind::Other(_) => errors.push(Problem::UnsupportedFilesystem {
            fs_name: dev.fs_name.clone(),
        }),
    }
    if !dev.removable {
        warnings.push(Warning::NotRemovable);
    }

    let root = dev.mount_point.as_path();
    let stick = stick::scan(root)?;
    let found = stick::rekordbox_markers(root)?;
    if !found.is_empty() {
        warnings.push(Warning::RekordboxStick { found });
    }
    let (history, backups) = plan_history(root, volume_serial, opts, &mut warnings, &mut errors)?;

    // Per-track checks; tracks that fail are left out (with a warning) or block.
    let mut ok: Vec<(SourceTrack, Probe, u64, std::time::SystemTime, AnlzSource)> = Vec::new();
    let mut convertible = Vec::new();
    for t in sources.tracks {
        match check_track(&t, opts, anlz, reks, &mut warnings, &mut convertible) {
            Ok(Some((probe, size, mtime, anlz_source))) => {
                ok.push((t, probe, size, mtime, anlz_source));
            }
            Ok(None) => {}
            Err(problem) if opts.skip_unsupported => warnings.push(Warning::Skipped {
                track_id: t.track_id,
                reason: problem_text(&problem),
            }),
            Err(problem) => errors.push(problem),
        }
    }

    // Paths: first every track whose audio is already on the stick claims its
    // file, then new tracks get free paths.
    let mut matcher = stick::Matcher::new(&stick.contents);
    let natural: Vec<String> = ok.iter().map(|(t, ..)| natural_path(t)).collect();
    let mut dst: Vec<Option<String>> = ok
        .iter()
        .zip(&natural)
        .map(|((t, _, size, mtime, _), nat)| matcher.claim(&t.src, *size, *mtime, nat))
        .collect();
    let mut taken: HashSet<String> = stick
        .contents
        .iter()
        .map(|f| fsname::fold_key(&f.rel))
        .collect();
    for (slot, nat) in dst.iter_mut().zip(natural) {
        if slot.is_none() {
            *slot = Some(fsname::dedupe(&mut taken, nat));
        }
    }

    let mut tracks = Vec::new();
    let mut copies = Vec::new();
    let mut anlz_ops = Vec::new();
    let mut bytes_to_write = 0u64;
    for ((t, probe, size, _, anlz_source), dst) in ok.into_iter().zip(dst) {
        let Some(dst_rel) = dst else { continue };
        if let Err(p) = fsname::check_path(&dst_rel) {
            errors.push(Problem::BadPath {
                track_id: t.track_id,
                path: dst_rel,
                message: p.to_string(),
            });
            continue;
        }
        let on_stick = matcher.is_claimed(&dst_rel);
        if !on_stick {
            bytes_to_write += size;
            copies.push(FileOp {
                src: Some(t.src.clone()),
                dst_rel: dst_rel.clone(),
                bytes: size,
                track_id: Some(t.track_id),
                reason: OpReason::NotOnStick,
            });
        }
        anlz_ops.push(match &anlz_source {
            AnlzSource::RekordboxCache { dat } => {
                let bytes = stick::anlz_set_bytes(dat);
                bytes_to_write += bytes;
                AnlzOp::CopyFromCache {
                    track_id: t.track_id,
                    dat: dat.clone(),
                    bytes,
                }
            }
            AnlzSource::Reks | AnlzSource::ReksGrid { .. } => {
                let bytes =
                    reksanlz::estimated_bytes(probe.duration_ms, reksanlz::anlz_target(target));
                bytes_to_write += bytes;
                AnlzOp::Build {
                    track_id: t.track_id,
                    bytes,
                }
            }
            AnlzSource::Generated => AnlzOp::Generate {
                track_id: t.track_id,
            },
            AnlzSource::None => AnlzOp::Missing {
                track_id: t.track_id,
            },
        });
        tracks.push(PlannedTrack {
            track_id: t.track_id,
            src: t.src,
            dst_rel,
            on_stick,
            kind: probe.kind,
            probe,
            has_grid: t.has_grid,
            cue_count: t.cue_count,
            anlz_source,
            divider: t.divider,
        });
    }

    // Playlists keep only exported tracks.
    let exported: HashSet<TrackId> = tracks.iter().map(|t| t.track_id).collect();
    let mut playlists = sources.playlists;
    for p in &mut playlists {
        let before = p.track_ids.len();
        p.track_ids.retain(|id| exported.contains(id));
        if !p.is_folder && before > 0 && p.track_ids.is_empty() {
            warnings.push(Warning::EmptyPlaylist { id: p.id });
        }
        if p.smart {
            warnings.push(Warning::SmartPlaylistFixed { id: p.id });
        }
    }

    // Stale files.
    let keep: Vec<&str> = tracks.iter().map(|t| t.dst_rel.as_str()).collect();
    let stale = stick.stale(&keep, opts.artwork);
    let mut deletes = Vec::new();
    let mut freed = 0u64;
    if opts.keep_stale {
        if !stale.is_empty() {
            warnings.push(Warning::StaleKept {
                files: stale.len(),
                bytes: stale.iter().map(|f| f.size).sum(),
            });
        }
    } else {
        for f in stale {
            freed += f.size;
            deletes.push(FileOp {
                src: None,
                dst_rel: f.rel.clone(),
                bytes: f.size,
                track_id: None,
                reason: OpReason::Stale,
            });
        }
    }

    // The writer deletes stale files only after writing the new ones (baken
    // prunes last), so the new files must fit in today's free space.
    if bytes_to_write > dev.free_bytes {
        errors.push(Problem::NotEnoughSpace {
            needed: bytes_to_write,
            free: dev.free_bytes,
        });
    }
    let bytes_free_after = to_i64(dev.free_bytes)
        .saturating_add(to_i64(freed))
        .saturating_sub(to_i64(bytes_to_write));

    Ok(ExportPlan {
        device: dev.clone(),
        options: *opts,
        tracks,
        playlists,
        copies,
        anlz: anlz_ops,
        deletes,
        history,
        backups,
        guard: HistoryGuard::default(),
        history_import: None,
        onelibrary_history_import: None,
        warnings,
        errors,
        bytes_to_write,
        bytes_free_after,
        convertible,
        onelibrary: None,
    })
}

/// Reads the stick's history and plans its backups under `opts.history`.
fn plan_history(
    root: &Path,
    volume_serial: Option<String>,
    opts: &ExportOptions,
    warnings: &mut Vec<Warning>,
    errors: &mut Vec<Problem>,
) -> Result<(StickHistory, Vec<FileOp>), PlanError> {
    let history = history::read_history(root, volume_serial);
    let at_risk = history.may_hold_history();
    let files =
        match history::plan_backups(root, &history, opts.history, opts.discard_history_confirmed) {
            Ok(plan) => plan.files,
            // Nothing to lose: no confirmation needed.
            Err(HistoryError::DiscardNotConfirmed) if !at_risk => Vec::new(),
            Err(HistoryError::DiscardNotConfirmed) => {
                errors.push(Problem::DiscardNotConfirmed);
                Vec::new()
            }
            Err(e) => return Err(e.into()),
        };
    if at_risk {
        warnings.push(Warning::StickHistory {
            sessions: history.pdb_sessions,
            entries: history.pdb_entries,
            unreadable: history.pdb_unreadable,
            onelibrary_db: history.onelibrary_db,
            policy: opts.history,
        });
    }
    if history.volume_serial.is_none() {
        warnings.push(Warning::NoVolumeSerial);
    }
    let backups = files
        .into_iter()
        .map(|f| FileOp {
            src: None,
            dst_rel: format!("/{}", f.rel_path),
            bytes: f.bytes,
            track_id: None,
            reason: OpReason::HistoryBackup,
        })
        .collect();
    Ok((history, backups))
}

/// Probes and checks one track. `Ok(None)`: left out with a warning already
/// recorded. `Err`: a problem that blocks it.
fn check_track(
    t: &SourceTrack,
    opts: &ExportOptions,
    anlz: &dyn AnlzLocator,
    reks: &dyn ReksLocator,
    warnings: &mut Vec<Warning>,
    convertible: &mut Vec<TrackId>,
) -> Result<Option<(Probe, u64, std::time::SystemTime, AnlzSource)>, Problem> {
    let target = opts.target;
    let missing = || Problem::SourceMissing {
        track_id: t.track_id,
        path: t.src.clone(),
    };
    let meta = std::fs::metadata(&t.src).map_err(|_| missing())?;
    if !meta.is_file() {
        return Err(missing());
    }
    let unreadable = |message: String| Problem::SourceUnreadable {
        track_id: t.track_id,
        path: t.src.clone(),
        message,
    };
    let mtime = meta.modified().map_err(|e| unreadable(e.to_string()))?;
    let probe = match &t.probe {
        Some(p) => p.clone(),
        None => reks_audio::probe(&t.src).map_err(|e| unreadable(e.to_string()))?,
    };
    let issues =
        audiocheck::check(&t.src, &probe, target).map_err(|e| unreadable(e.to_string()))?;
    if issues.iter().any(|i| i.converter_fixes(probe.kind)) {
        convertible.push(t.track_id);
    }
    // Errors first so the reported blocker is the most serious one.
    if let Some(issue) = issues
        .iter()
        .find(|i| i.severity(target) == Severity::Error)
    {
        return Err(Problem::Unsupported {
            track_id: t.track_id,
            message: issue.to_string(),
        });
    }
    for issue in &issues {
        warnings.push(Warning::Audio {
            track_id: t.track_id,
            message: issue.to_string(),
        });
    }

    // A divider's analysis is always generated (D22): rekordbox never saw it.
    if t.divider.is_some() {
        return Ok(Some((probe, meta.len(), mtime, AnlzSource::Generated)));
    }
    let found = choose_anlz(t, opts.anlz_preference, anlz, reks);
    let anlz_source = match found {
        Some(source) => source,
        None => match opts.missing_analysis {
            MissingAnalysis::Skip => {
                let mut reason = anlz.why_missing(t);
                if opts.anlz_preference != AnlzPreference::RekordboxOnly {
                    reason = format!("{reason}; {}", reks.why_missing(t));
                }
                warnings.push(Warning::Skipped {
                    track_id: t.track_id,
                    reason,
                });
                return Ok(None);
            }
            MissingAnalysis::Generate => AnlzSource::Generated,
            MissingAnalysis::ExportWithout => AnlzSource::None,
        },
    };
    Ok(Some((probe, meta.len(), mtime, anlz_source)))
}

/// The track's analysis under `pref`; `None` when neither source has it.
/// The second source is only asked when the first has nothing. A grid the
/// user owns is built by reks under both `RekordboxFirst` and `ReksFirst`:
/// rekordbox's cached `PQTZ` is copied as it is, so the edit would not
/// reach the stick. Without reks analysis, rekordbox's waveforms are rebuilt
/// with the library's grid ([`AnlzSource::ReksGrid`]) when its cache has
/// all three files; else rekordbox's set is copied as before.
/// `RekordboxOnly` never writes reks's.
fn choose_anlz(
    t: &SourceTrack,
    pref: AnlzPreference,
    anlz: &dyn AnlzLocator,
    reks: &dyn ReksLocator,
) -> Option<AnlzSource> {
    let rekordbox = || anlz.locate(t).map(|dat| AnlzSource::RekordboxCache { dat });
    let ours = || reks.has_analysis(t).then_some(AnlzSource::Reks);
    let regrid = || {
        anlz.locate(t).map(|dat| {
            if stick::has_full_set(&dat) {
                AnlzSource::ReksGrid { dat }
            } else {
                AnlzSource::RekordboxCache { dat }
            }
        })
    };
    match pref {
        AnlzPreference::RekordboxOnly => rekordbox(),
        _ if t.grid_user_owned => ours().or_else(regrid),
        AnlzPreference::RekordboxFirst => rekordbox().or_else(ours),
        AnlzPreference::ReksFirst => ours().or_else(rekordbox),
    }
}

/// `/Contents/<Artist>/<Album>/<file name>` for a fresh copy.
fn natural_path(t: &SourceTrack) -> String {
    let file = t
        .src
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    fsname::contents_path(
        t.artist.as_deref().unwrap_or(""),
        t.album.as_deref().unwrap_or(""),
        &file,
    )
}

fn problem_text(p: &Problem) -> String {
    match p {
        Problem::SourceMissing { path, .. } => format!("file not found: {}", path.display()),
        Problem::SourceUnreadable { message, .. } | Problem::Unsupported { message, .. } => {
            message.clone()
        }
        other => other.to_string(),
    }
}

fn to_i64(n: u64) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

/// Joins a stick-relative `/`-path onto the mount point.
pub(crate) fn on_stick(root: &Path, rel: &str) -> PathBuf {
    rel.trim_start_matches('/')
        .split('/')
        .fold(root.to_path_buf(), |p, c| p.join(c))
}

#[cfg(test)]
mod tests;
