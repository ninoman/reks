//! App state shared by the commands.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use reks_core::convert::ConvertPlan;
use reks_core::history::HistoryImportPlan;
use reks_core::{CancelToken, Library, TrackId};
use reks_export_rb::devices::DeviceId;
use reks_export_rb::plan::{ExportPlan, Selection};
use reks_mix::Player;

/// File name of the library database (docs/decisions.md D19).
const DB_FILE: &str = "reks.sqlite";

#[derive(Debug)]
pub struct AppState {
    pub library: Arc<Library>,
    /// Cancels the running scan, if any.
    pub scan_cancel: Mutex<Option<CancelToken>>,
    /// Cancels the running rekordbox XML import, if any.
    pub import_cancel: Mutex<Option<CancelToken>>,
    /// Cancels the running USB export, if any.
    pub export_cancel: Mutex<Option<CancelToken>>,
    /// The export plan the wizard's dry run shows; Export writes exactly it.
    pub shown_plan: Mutex<ShownPlan>,
    /// Cancels the running file conversion, if any.
    pub convert_cancel: Mutex<Option<CancelToken>>,
    /// The conversion plan the converter's dry run shows; Convert runs exactly it.
    pub shown_convert: Mutex<ShownConvert>,
    /// The history import plan the import dialog's dry run shows; Import
    /// applies exactly it.
    pub shown_history: Mutex<ShownHistory>,
    /// App data dir (D19): stick backups go under it.
    pub data_dir: PathBuf,
    pub player: Mutex<PlayerSlot>,
}

/// The last export plan returned to the UI, under the id the UI sends back.
#[derive(Debug, Default)]
pub struct ShownPlan {
    /// Id of the newest `export_plan` call; ids only grow.
    pub last_id: u64,
    /// `(id, plan, selection, stick profile)` of the newest plan that
    /// finished; taken by `export_apply`.
    pub plan: Option<(u64, ExportPlan, Selection, Option<DeviceId>)>,
}

/// The last conversion plan returned to the UI, under the id the UI sends back.
#[derive(Debug, Default)]
pub struct ShownConvert {
    /// Id of the newest `convert_plan` call; ids only grow.
    pub last_id: u64,
    /// `(id, plan)` of the newest plan that finished; taken by `convert_apply`.
    pub plan: Option<(u64, ConvertPlan)>,
}

/// The last history import plan returned to the UI, under the id the UI sends back.
#[derive(Debug, Default)]
pub struct ShownHistory {
    /// Id of the newest `history_scan` call; ids only grow.
    pub last_id: u64,
    /// `(id, plan)` of the newest scan that finished; taken by `history_import`.
    pub plan: Option<(u64, HistoryImportPlan)>,
}

/// The preview player and the track on it.
#[derive(Debug)]
pub struct PlayerSlot {
    pub player: Player,
    pub track: Option<TrackId>,
    /// Bumped by every load request; a decode that finishes after a newer
    /// request is dropped.
    pub generation: u64,
}

impl AppState {
    /// Opens (creating and migrating) the library in `data_dir`, with
    /// derived files under `cache_dir`.
    pub fn open(data_dir: &Path, cache_dir: &Path) -> reks_core::Result<Self> {
        let library = Library::open(&data_dir.join(DB_FILE), cache_dir)?;
        tracing::info!(db = %data_dir.join(DB_FILE).display(), "library opened");
        // A chosen cache dir that cannot be created (an unplugged drive):
        // run on the default one rather than not at all.
        match library.use_stored_cache_dir() {
            Ok(dir) => tracing::info!(cache = %dir.display(), "cache dir"),
            Err(err) => tracing::warn!(%err, "stored cache dir unusable, using the default"),
        }
        let settings = library.settings()?;
        Ok(Self {
            library: Arc::new(library),
            scan_cancel: Mutex::new(None),
            import_cancel: Mutex::new(None),
            export_cancel: Mutex::new(None),
            shown_plan: Mutex::new(ShownPlan::default()),
            convert_cancel: Mutex::new(None),
            shown_convert: Mutex::new(ShownConvert::default()),
            shown_history: Mutex::new(ShownHistory::default()),
            data_dir: data_dir.to_path_buf(),
            player: Mutex::new(PlayerSlot {
                player: Player::new(settings.audio_device).with_volume(settings.ui.volume),
                track: None,
                generation: 0,
            }),
        })
    }
}
