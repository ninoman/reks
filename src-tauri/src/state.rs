//! App state shared by the commands.

use std::path::Path;
use std::sync::{Arc, Mutex};

use reks_core::{CancelToken, Library, TrackId};
use reks_mix::Player;

/// File name of the library database (docs/decisions.md D19).
const DB_FILE: &str = "reks.sqlite";

#[derive(Debug)]
pub struct AppState {
    pub library: Arc<Library>,
    /// Cancels the running scan, if any.
    pub scan_cancel: Mutex<Option<CancelToken>>,
    pub player: Mutex<PlayerSlot>,
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
        Ok(Self {
            library: Arc::new(library),
            scan_cancel: Mutex::new(None),
            // Output device setting: Phase 1 task 13. The system default until then.
            player: Mutex::new(PlayerSlot {
                player: Player::new(None),
                track: None,
                generation: 0,
            }),
        })
    }
}
