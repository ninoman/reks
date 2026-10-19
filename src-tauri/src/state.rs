//! App state shared by the commands.

use std::path::Path;
use std::sync::{Arc, Mutex};

use reks_core::{CancelToken, Library};

/// File name of the library database (docs/decisions.md D19).
const DB_FILE: &str = "reks.sqlite";

#[derive(Debug)]
pub struct AppState {
    pub library: Arc<Library>,
    /// Cancels the running scan, if any.
    pub scan_cancel: Mutex<Option<CancelToken>>,
}

impl AppState {
    /// Opens (creating and migrating) the library in `data_dir`, with
    /// derived files under `cache_dir`.
    pub fn open(data_dir: &Path, cache_dir: &Path) -> reks_core::Result<Self> {
        let library = Library::open(&data_dir.join(DB_FILE), cache_dir)?;
        tracing::info!(db = %data_dir.join(DB_FILE).display(), "library opened");
        Ok(Self {
            library: Arc::new(library),
            scan_cancel: Mutex::new(None),
        })
    }
}
