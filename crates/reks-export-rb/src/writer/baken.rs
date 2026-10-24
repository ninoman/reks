//! [`BakenWriter`]: the legacy export through `baken-export` 4.6.1 (D7).
//!
//! baken takes a `collection.xml` (ours, from [`reks_core::rbxml::export_file`])
//! and works in two steps, `plan` then `export`. Its plan decides the stick
//! paths itself (`layout::Layout`: NFC Unicode, names cut at 48 characters)
//! and finds ANLZ in rekordbox's cache by itself. Ours decides both already
//! (ASCII paths, D24; the cache lookup of task 5). So between the two steps
//! [`reconcile`] rewrites baken's plan to match ours, through baken's public
//! `Plan` fields:
//!
//! - tracks: exactly the planned ones, in plan order (baken's ANLZ slot
//!   numbers and PDB track ids follow that order);
//! - `usb_path` = the plan's `dst_rel`, and `anlz_dir`/`anlz_index` computed
//!   from it with baken's own `AnlzSlots`, so `export.pdb`, PPTH and the
//!   USBANLZ layout all carry our path;
//! - `anlz` = the plan's [`AnlzSource`]: the planned cache `.DAT`, the
//!   `.DAT` [`BakenWriter::prepare`] built from reks's analysis
//!   ([`crate::reksanlz::write_all`], in the scratch folder), or generation.
//!   baken treats a built set like rekordbox's own files (rewrites `PPTH`,
//!   splices the cues from the XML). baken is always called with
//!   `generate_analysis` on and an empty ANLZ root, so its plan keeps every
//!   track and copies nothing we did not plan.
//!
//! A planned track baken's plan lacks (source gone, XML out of date) or whose
//! source differs is a [`WriterError::PlanMismatch`] before anything is
//! written. [`MissingAnalysis::ExportWithout`] is refused: baken has no way
//! to write a track without ANLZ.
//!
//! Playlist dividers (D22) reach baken as rows of the temp XML
//! ([`rbxml::DividerTrack`], built by [`BakenWriter::prepare`] from the
//! plan's divider tracks) and are reconciled like any track: the synthetic
//! id is their `TrackID`, their analysis is generated.
//!
//! baken's own space estimate (`space_needed`) is made with its paths and
//! counts every track's analysis as generated; it is an estimate from above
//! and only refuses a stick that is already nearly full.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use baken_export::anlz::hash::AnlzSlots;
use baken_export::anlz::locate::Entry;
use reks_core::{rbxml, CancelToken, Library, TrackId};

use super::{ExportEvent, RekordboxWriter, TrackFailure, WriterError, WriterReport};
use crate::plan::{AnlzSource, ExportPlan, MissingAnalysis, Selection};
use crate::reksanlz;

/// Where My Settings (`MYSETTING*.DAT`, `DJMMYSETTING.DAT`, `DEVSETTING.DAT`)
/// come from. reks never synthesises them (phase 2 non-goals).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum SettingsSource {
    /// Write none (baken `no_settings`): the player keeps its own, and files
    /// already on the stick stay as they are.
    #[default]
    Skip,
    /// rekordbox's folder on this machine (baken's default lookup).
    Rekordbox,
    /// A folder holding the files, e.g. the `PIONEER` folder of a stick
    /// rekordbox exported.
    Dir(PathBuf),
}

/// [`RekordboxWriter`] over `baken-export`.
#[derive(Debug, Clone)]
pub struct BakenWriter {
    xml: PathBuf,
    playlists: Vec<String>,
    work_dir: PathBuf,
    settings: SettingsSource,
    device_name: Option<String>,
    /// `.DAT` of each track's set built from reks's analysis.
    reks_anlz: HashMap<TrackId, PathBuf>,
}

impl BakenWriter {
    /// A writer for an existing `collection.xml` and its baken playlist
    /// paths. `work_dir` is a private scratch directory (an empty ANLZ root
    /// is made there).
    pub fn new(xml: PathBuf, playlists: Vec<String>, work_dir: PathBuf) -> Self {
        Self {
            xml,
            playlists,
            work_dir,
            settings: SettingsSource::Skip,
            device_name: None,
            reks_anlz: HashMap::new(),
        }
    }

    /// Writes `work_dir/collection.xml` for `sel` (the selection `plan` was
    /// made from), with a row for each of the plan's dividers, builds the
    /// ANLZ of every [`AnlzSource::Reks`] track into `work_dir/reks-anlz/`,
    /// and returns a writer for it. Nothing goes to the stick; a set that
    /// can't be built fails here ([`WriterError::ReksAnlz`]).
    pub fn prepare(
        lib: &Library,
        sel: &Selection,
        plan: &ExportPlan,
        work_dir: &Path,
    ) -> Result<Self, WriterError> {
        std::fs::create_dir_all(work_dir).map_err(|source| WriterError::Io {
            path: work_dir.to_path_buf(),
            source,
        })?;
        let xml = work_dir.join("collection.xml");
        let sel = rbxml::Selection {
            playlists: sel.playlists.clone(),
        };
        let dividers = divider_rows(plan)?;
        let active_loops = if plan.options.active_loops {
            rbxml::ActiveLoops::BakenMarker
        } else {
            rbxml::ActiveLoops::Leave
        };
        let out = rbxml::export_file_with_dividers(lib, &sel, &dividers, active_loops, &xml)?;
        let reks_anlz = reksanlz::write_all(lib, plan, &work_dir.join("reks-anlz"))?;
        Ok(Self {
            reks_anlz,
            ..Self::new(xml, out.baken_playlists(), work_dir.to_path_buf())
        })
    }

    pub fn with_settings(mut self, settings: SettingsSource) -> Self {
        self.settings = settings;
        self
    }

    /// Name stored in `export.pdb`; defaults to the plan's device label.
    pub fn with_device_name(mut self, name: impl Into<String>) -> Self {
        self.device_name = Some(name.into());
        self
    }

    /// The temp `collection.xml` this writer feeds to baken.
    pub fn xml(&self) -> &Path {
        &self.xml
    }

    /// baken's options for `plan` on `device_root`.
    fn options(
        &self,
        plan: &ExportPlan,
        device_root: &Path,
        anlz_root: PathBuf,
    ) -> baken_export::Options {
        let (no_settings, settings_dir) = match &self.settings {
            SettingsSource::Skip => (true, None),
            SettingsSource::Rekordbox => (false, None),
            SettingsSource::Dir(d) => (false, Some(d.clone())),
        };
        let label = plan.device.label.trim();
        baken_export::Options {
            xml: self.xml.clone(),
            device: device_root.to_path_buf(),
            playlists: self.playlists.clone(),
            anlz_roots: vec![anlz_root],
            settings_dir,
            no_settings,
            device_name: self
                .device_name
                .clone()
                .or_else(|| (!label.is_empty()).then(|| label.to_owned())),
            cdjsafe: false,
            // Always on: which tracks get generated ANLZ is the plan's call,
            // made in `reconcile`; off, baken would drop them instead.
            generate_analysis: true,
            prune: !plan.options.keep_stale,
            onelibrary: false,
            artwork: plan.options.artwork,
        }
    }

    /// An empty directory to give baken as its only ANLZ root.
    fn empty_anlz_root(&self) -> Result<PathBuf, WriterError> {
        let dir = self.work_dir.join("no-anlz");
        let io = |source| WriterError::Io {
            path: dir.clone(),
            source,
        };
        std::fs::create_dir_all(&dir).map_err(io)?;
        if std::fs::read_dir(&dir).map_err(io)?.next().is_some() {
            return Err(WriterError::Io {
                path: dir.clone(),
                source: std::io::Error::other("scratch ANLZ root is not empty"),
            });
        }
        Ok(dir)
    }
}

impl RekordboxWriter for BakenWriter {
    fn write(
        &self,
        plan: &ExportPlan,
        device_root: &Path,
        progress: &(dyn Fn(ExportEvent) + Sync),
        cancel: &CancelToken,
    ) -> Result<WriterReport, WriterError> {
        if plan.is_blocked() {
            return Err(WriterError::Blocked(plan.errors.len()));
        }
        if plan.tracks.is_empty() {
            return Err(WriterError::NothingToExport);
        }
        if plan.options.missing_analysis == MissingAnalysis::ExportWithout
            || plan
                .tracks
                .iter()
                .any(|t| t.anlz_source == AnlzSource::None)
        {
            return Err(WriterError::Unsupported(
                "tracks without analysis (baken writes ANLZ for every track: copy or generate)"
                    .to_owned(),
            ));
        }

        let opts = self.options(plan, device_root, self.empty_anlz_root()?);
        let mut bplan = baken_export::plan(&opts).map_err(plan_error)?;
        reconcile(&mut bplan, plan, &self.reks_anlz)?;
        tracing::info!(
            tracks = bplan.tracks.len(),
            device = %device_root.display(),
            "writing rekordbox export with baken"
        );

        let before = fingerprint(device_root);
        let ids: Vec<TrackId> = plan.tracks.iter().map(|t| t.track_id).collect();
        let bridge = Bridge {
            ids: &ids,
            progress,
            cancel,
            baken_cancel: baken_core::CancelToken::new(),
        };
        if cancel.is_cancelled() {
            bridge.baken_cancel.cancel();
        }
        progress(ExportEvent::Started { total: ids.len() });
        let result = baken_export::export(&bplan, &bridge, &bridge.baken_cancel);
        match result {
            Ok(report) => {
                if !report.cancelled {
                    progress(ExportEvent::Finished);
                }
                Ok(translate(report, &bplan))
            }
            Err(e) => Err(export_error(e, fingerprint(device_root) != before)),
        }
    }
}

/// The temp XML's rows for the plan's dividers.
fn divider_rows(plan: &ExportPlan) -> Result<Vec<rbxml::DividerTrack>, WriterError> {
    plan.tracks
        .iter()
        .filter_map(|t| t.divider.as_ref().map(|d| (t, d)))
        .map(|(t, d)| {
            let p = &t.probe;
            let track_id = u32::try_from(t.track_id).map_err(|_| WriterError::PlanMismatch {
                track_id: Some(t.track_id),
                message: "divider id does not fit a TrackID".to_owned(),
            })?;
            let size = std::fs::metadata(&t.src)
                .map_err(|source| WriterError::Io {
                    path: t.src.clone(),
                    source,
                })?
                .len();
            let bits = u32::from(p.bit_depth.unwrap_or(16));
            let pcm_kbps = p.sample_rate * u32::from(p.channels) * bits / 1000;
            Ok(rbxml::DividerTrack {
                playlist_id: d.playlist_id,
                ordinal: d.ordinal,
                track_id,
                label: d.label.clone(),
                path: t.src.clone(),
                size,
                duration_ms: p.duration_ms,
                sample_rate: p.sample_rate,
                bit_rate_kbps: p.bit_rate_kbps.unwrap_or(pcm_kbps),
            })
        })
        .collect()
}

/// Makes baken's plan carry out ours (module docs). `reks_anlz`: the built
/// `.DAT` of each [`AnlzSource::Reks`] track.
fn reconcile(
    bplan: &mut baken_export::Plan,
    plan: &ExportPlan,
    reks_anlz: &HashMap<TrackId, PathBuf>,
) -> Result<(), WriterError> {
    let mut by_id: HashMap<u64, baken_export::PlanTrack> = std::mem::take(&mut bplan.tracks)
        .into_iter()
        .map(|t| (t.device.track.id, t))
        .collect();
    let mut slots = AnlzSlots::default();
    let mut seen = HashSet::new();
    let mut tracks = Vec::with_capacity(plan.tracks.len());
    for t in &plan.tracks {
        let mismatch = |message: String| WriterError::PlanMismatch {
            track_id: Some(t.track_id),
            message,
        };
        if !seen.insert(t.dst_rel.to_ascii_lowercase()) {
            return Err(mismatch(format!("{} is planned twice", t.dst_rel)));
        }
        let mut bt = u64::try_from(t.track_id)
            .ok()
            .and_then(|id| by_id.remove(&id))
            .ok_or_else(|| {
                let skipped = bplan
                    .skipped
                    .iter()
                    .map(|s| format!("{}: {}", s.name, s.reason))
                    .collect::<Vec<_>>();
                let mut message = format!("track {} is not in the writer's input", t.track_id);
                if !skipped.is_empty() {
                    message += &format!(" (baken skipped {})", skipped.join("; "));
                }
                mismatch(message)
            })?;
        if !same_file(&bt.source, &t.src) {
            return Err(mismatch(format!(
                "source is {} in the writer's input, {} in the plan",
                bt.source.display(),
                t.src.display()
            )));
        }
        let (anlz_dir, anlz_index) = slots.assign(&t.dst_rel);
        bt.device.usb_path = t.dst_rel.clone();
        bt.device.anlz_dir = anlz_dir;
        bt.device.anlz_index = anlz_index;
        // `export` reads only `dat` (and its siblings); the summary fields
        // are for baken's own index lookup.
        let entry = |dat: &Path| Entry {
            dat: dat.to_path_buf(),
            beats: 0,
            first_beat_ms: 0,
            first_tempo_x100: 0,
        };
        bt.anlz = match &t.anlz_source {
            AnlzSource::RekordboxCache { dat } => Some(entry(dat)),
            AnlzSource::Reks | AnlzSource::ReksGrid { .. } => match reks_anlz.get(&t.track_id) {
                Some(dat) => Some(entry(dat)),
                None => {
                    return Err(mismatch(
                        "reks's analysis was not built (writer not prepared for this plan)"
                            .to_owned(),
                    ))
                }
            },
            AnlzSource::Generated => None,
            AnlzSource::None => {
                return Err(WriterError::Unsupported(
                    "tracks without analysis".to_owned(),
                ))
            }
        };
        bt.stale_seek_table = false; // re-checked by `export` itself
        tracks.push(bt);
    }
    if !by_id.is_empty() {
        tracing::debug!(
            left_out = by_id.len(),
            "writer input tracks not in the plan are left out"
        );
    }
    bplan.tracks = tracks;
    Ok(())
}

/// Same path, or the same file reached another way.
fn same_file(a: &Path, b: &Path) -> bool {
    a == b
        || matches!(
            (std::fs::canonicalize(a), std::fs::canonicalize(b)),
            (Ok(x), Ok(y)) if x == y
        )
}

/// Passes baken's per-track progress on and forwards our cancel token.
struct Bridge<'a> {
    ids: &'a [TrackId],
    progress: &'a (dyn Fn(ExportEvent) + Sync),
    cancel: &'a CancelToken,
    baken_cancel: baken_core::CancelToken,
}

impl baken_core::Progress for Bridge<'_> {
    fn on_file_done(&self, done: usize, total: usize, _file: &Path) {
        (self.progress)(ExportEvent::TrackDone {
            done,
            total,
            track_id: done.checked_sub(1).and_then(|i| self.ids.get(i)).copied(),
        });
        // baken checks its token before each track.
        if self.cancel.is_cancelled() {
            self.baken_cancel.cancel();
        }
    }
}

fn translate(r: baken_export::Report, bplan: &baken_export::Plan) -> WriterReport {
    let mut by_title: HashMap<&str, Vec<TrackId>> = HashMap::new();
    for t in &bplan.tracks {
        if let Ok(id) = TrackId::try_from(t.device.track.id) {
            by_title
                .entry(t.device.track.name.as_str())
                .or_default()
                .push(id);
        }
    }
    let failures = r
        .failures
        .into_iter()
        .map(|(title, message)| TrackFailure {
            track_id: match by_title.get(title.as_str()).map(Vec::as_slice) {
                Some([id]) => Some(*id),
                _ => None,
            },
            title,
            message,
        })
        .collect();
    WriterReport {
        tracks_in_database: r.tracks_in_database,
        audio_copied: r.copied,
        audio_kept: r.kept,
        anlz_written: r.anlz_files,
        anlz_unchanged: r.anlz_unchanged,
        anlz_generated: r.anlz_generated,
        artwork: r.artwork,
        pruned: r.pruned,
        prune_skipped: r.prune_skipped,
        onelibrary_removed: r.onelibrary_removed,
        onelibrary_kept: r.onelibrary_kept,
        seek_tables_rebuilt: r.seek_tables_rebuilt,
        seek_tables_dropped: r.seek_tables_dropped,
        failures,
        cancelled: r.cancelled,
    }
}

fn plan_error(e: baken_export::Error) -> WriterError {
    use baken_export::Error as E;
    match e {
        E::DeviceWrite { path, err } => WriterError::DeviceWrite { path, source: err },
        E::NotEnoughSpace { needed, available } => {
            WriterError::NotEnoughSpace { needed, available }
        }
        E::NothingToExport => WriterError::PlanMismatch {
            track_id: None,
            message: "baken found none of the planned tracks".to_owned(),
        },
        other => WriterError::Baken(other.to_string()),
    }
}

/// `pdb_changed`: `export.pdb` differs from before the run, so the error
/// came after the database was replaced.
fn export_error(e: baken_export::Error, pdb_changed: bool) -> WriterError {
    if pdb_changed {
        return WriterError::AfterDatabase(e.to_string());
    }
    plan_error(e)
}

/// Length and hash of the stick's `export.pdb`, if there is one.
fn fingerprint(device_root: &Path) -> Option<(u64, blake3::Hash)> {
    let bytes = std::fs::read(device_root.join("PIONEER/rekordbox/export.pdb")).ok()?;
    Some((bytes.len() as u64, blake3::hash(&bytes)))
}
