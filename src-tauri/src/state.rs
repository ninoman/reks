//! App state shared by the commands.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use reks_core::convert::ConvertPlan;
use reks_core::history::HistoryImportPlan;
use reks_core::relocate::RelocatePlan;
use reks_core::{CancelToken, Library};
use reks_export_rb::adopt::AdoptPlan;
use reks_export_rb::devices::DeviceId;
use reks_export_rb::plan::{ExportPlan, Selection};

use crate::analysis::Analysis;
use crate::mix::Mix;
use crate::xml_cache::XmlCache;

/// File name of the library database (docs/decisions.md D19).
const DB_FILE: &str = "reks.sqlite";

#[derive(Debug)]
pub struct AppState {
    pub library: Arc<Library>,
    /// Cancels the running scan, if any.
    pub scan_cancel: Mutex<Option<CancelToken>>,
    /// Cancels the running rekordbox XML import, if any.
    pub import_cancel: Mutex<Option<CancelToken>>,
    /// The parsed rekordbox XML shared by the import preview and the import.
    pub parsed_xml: Arc<Mutex<XmlCache>>,
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
    /// The adopt plan the adopt dialog's dry run shows; Adopt applies exactly it.
    pub shown_adopt: Mutex<ShownAdopt>,
    /// Cancels the running relocate (dry run or apply), if any.
    pub relocate_cancel: Mutex<Option<CancelToken>>,
    /// The relocate plan the dialog's dry run shows; Apply relinks exactly it.
    pub shown_relocate: Mutex<ShownRelocate>,
    /// App data dir (D19): stick backups go under it.
    pub data_dir: PathBuf,
    /// The audio engine: the player deck and the mixer (D30).
    pub mix: Mix,
    /// Background analysis queue (Phase 3 task 11).
    pub analysis: Analysis,
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

/// The last adopt plan returned to the UI, under the id the UI sends back.
#[derive(Debug, Default)]
pub struct ShownAdopt {
    /// Id of the newest `adopt_scan` call; ids only grow.
    pub last_id: u64,
    /// `(id, plan)` of the newest scan that finished; taken by `adopt_apply`.
    pub plan: Option<(u64, AdoptPlan)>,
}

/// The last relocate plan returned to the UI, under the id the UI sends back.
#[derive(Debug, Default)]
pub struct ShownRelocate {
    /// Id of the newest `relocate_plan` call; ids only grow.
    pub last_id: u64,
    /// `(id, plan)` of the newest plan that finished; read by `relocate_apply`.
    pub plan: Option<(u64, RelocatePlan)>,
}

impl AppState {
    /// Opens (creating and migrating) the library in `data_dir`, with
    /// derived files under `cache_dir`, and starts the analysis queue with
    /// the beat model in `model_dir`.
    pub fn open(data_dir: &Path, cache_dir: &Path, model_dir: &Path) -> reks_core::Result<Self> {
        let library = Library::open(&data_dir.join(DB_FILE), cache_dir)?;
        tracing::info!(db = %data_dir.join(DB_FILE).display(), "library opened");
        // A chosen cache dir that cannot be created (an unplugged drive):
        // run on the default one rather than not at all.
        match library.use_stored_cache_dir() {
            Ok(dir) => tracing::info!(cache = %dir.display(), "cache dir"),
            Err(err) => tracing::warn!(%err, "stored cache dir unusable, using the default"),
        }
        // At most one a day; a failure is recorded for Settings and never stops start-up.
        match library.snapshot_if_due() {
            Ok(Some(info)) => tracing::info!(backup = %info.name, "library backed up"),
            Ok(None) => {}
            Err(err) => tracing::warn!(%err, "start-up library backup failed"),
        }
        let library = Arc::new(library);
        let analysis = Analysis::start(&library, model_dir);
        Ok(Self {
            library,
            scan_cancel: Mutex::new(None),
            import_cancel: Mutex::new(None),
            parsed_xml: Arc::new(Mutex::new(XmlCache::default())),
            export_cancel: Mutex::new(None),
            shown_plan: Mutex::new(ShownPlan::default()),
            convert_cancel: Mutex::new(None),
            shown_convert: Mutex::new(ShownConvert::default()),
            shown_history: Mutex::new(ShownHistory::default()),
            shown_adopt: Mutex::new(ShownAdopt::default()),
            relocate_cancel: Mutex::new(None),
            shown_relocate: Mutex::new(ShownRelocate::default()),
            data_dir: data_dir.to_path_buf(),
            mix: Mix::default(),
            analysis,
        })
    }
}
