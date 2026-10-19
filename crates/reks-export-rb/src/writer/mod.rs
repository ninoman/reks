//! Writing a planned export to the stick: the [`RekordboxWriter`] trait and
//! its baken implementation, [`BakenWriter`] (D7; Phase 2 task 7).
//!
//! A writer executes an [`ExportPlan`] exactly: the stick ends up with the
//! plan's tracks at the plan's `dst_rel` paths, their ANLZ files where the
//! player computes them from those paths ([`crate::plan::anlz_dir`]), and an
//! `export.pdb` listing them. Orchestration (backups, validation, manifest)
//! is [`crate::apply`]'s job (task 9), not the writer's.
//!
//! What stays on the stick when a write fails (baken 4.6.1, read from its
//! source and covered by tests where a test can provoke it):
//!
//! - Refused before any write ([`WriterError::Blocked`], [`WriterError::Unsupported`],
//!   [`WriterError::PlanMismatch`], baken's own plan errors): nothing changed.
//! - Cancelled ([`WriterReport::cancelled`]), or the stick failed as a whole
//!   ([`WriterError::DeviceWrite`], [`WriterError::NotEnoughSpace`]): the old
//!   `export.pdb` is untouched. Audio and ANLZ written so far stay; new audio
//!   only ever goes to paths the old database does not use (the planner never
//!   reuses a path for different audio), ANLZ files of tracks already on the
//!   stick are rewritten in place for the same track.
//! - `export.pdb` itself is written beside the old one and renamed over it,
//!   so a crash leaves one or the other.
//! - A track that fails alone (its source unreadable mid-run) does **not**
//!   stop the run: baken writes `export.pdb` without it and skips the prune.
//!   The track is in [`WriterReport::failures`]; the validator then reports
//!   the plan/stick difference.
//! - An error after `export.pdb` was replaced (settings copy, prune):
//!   [`WriterError::AfterDatabase`].

mod baken;

use std::path::{Path, PathBuf};

use reks_core::{CancelToken, TrackId};
use serde::Serialize;

use crate::plan::ExportPlan;

pub use baken::{BakenWriter, SettingsSource};

/// Writes an [`ExportPlan`] to a stick.
pub trait RekordboxWriter {
    /// Writes PDB + ANLZ + audio for `plan` into `device_root`. `progress`
    /// gets [`ExportEvent`]s from the writing thread; `cancel` is checked
    /// between tracks. See the module docs for what a failure leaves behind.
    fn write(
        &self,
        plan: &ExportPlan,
        device_root: &Path,
        progress: &(dyn Fn(ExportEvent) + Sync),
        cancel: &CancelToken,
    ) -> Result<WriterReport, WriterError>;
}

/// Progress of a write, for the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ExportEvent {
    /// The writer starts on `total` tracks.
    Started { total: usize },
    /// Track `done` of `total` is finished (written, kept or failed).
    TrackDone {
        done: usize,
        total: usize,
        track_id: Option<TrackId>,
    },
    /// Every track is done; the database is written and stale files go.
    Finished,
}

/// What a write did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriterReport {
    /// Tracks listed in the new `export.pdb` (0 when cancelled).
    pub tracks_in_database: usize,
    /// Audio files copied from the source.
    pub audio_copied: usize,
    /// Audio files already on the stick with the same bytes, not copied again.
    pub audio_kept: usize,
    /// ANLZ files written, and those already on the stick byte for byte.
    pub anlz_written: usize,
    pub anlz_unchanged: usize,
    /// Tracks whose ANLZ was generated from the audio.
    pub anlz_generated: usize,
    /// Tracks that got artwork.
    pub artwork: usize,
    /// Stale files removed.
    pub pruned: usize,
    /// Pruning was due but skipped because tracks failed.
    pub prune_skipped: bool,
    /// rekordbox's OneLibrary files removed, and those the OS refused to remove.
    pub onelibrary_removed: usize,
    pub onelibrary_kept: usize,
    /// FLAC seek tables rebuilt or dropped (the file changed after rekordbox
    /// analysed it).
    pub seek_tables_rebuilt: usize,
    pub seek_tables_dropped: usize,
    /// Tracks that failed alone; the run went on without them.
    pub failures: Vec<TrackFailure>,
    /// Stopped by the cancel token: `export.pdb` was not written.
    pub cancelled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackFailure {
    /// `None` when the title matched more than one planned track.
    pub track_id: Option<TrackId>,
    pub title: String,
    pub message: String,
}

#[derive(Debug, thiserror::Error)]
pub enum WriterError {
    /// The plan has errors; `apply` must not get this far.
    #[error("the export plan has {0} blocking problems")]
    Blocked(usize),
    #[error("the export plan has no tracks")]
    NothingToExport,
    /// The plan asks for something this writer cannot do.
    #[error("not supported by this writer: {0}")]
    Unsupported(String),
    /// The writer's input disagrees with the plan (the library or a source
    /// file changed since planning). Nothing was written; plan again.
    #[error("the export plan is out of date: {message}")]
    PlanMismatch {
        track_id: Option<TrackId>,
        message: String,
    },
    /// The stick as a whole failed (read-only, full, unplugged). The old
    /// `export.pdb` is still there.
    #[error("cannot write to the stick at {}: {source}", path.display())]
    DeviceWrite {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("not enough space on the stick: {needed} bytes needed, {available} free")]
    NotEnoughSpace { needed: u64, available: u64 },
    /// `export.pdb` was replaced, then a later step failed (copying settings,
    /// pruning). The new database is on the stick; validate it.
    #[error("export.pdb was written, then: {0}")]
    AfterDatabase(String),
    /// Preparing the writer's input (temp `collection.xml`).
    #[error(transparent)]
    Library(#[from] reks_core::Error),
    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// Any other baken error, before `export.pdb` was touched.
    #[error("baken: {0}")]
    Baken(String),
}
