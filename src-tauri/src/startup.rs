//! Opens the library off the main thread. Done in `setup`, it blocked the
//! event loop for seconds (migrations, analyser start-up), so the window sat
//! blank and unthemed. Now the splash in `index.html` animates meanwhile and
//! the UI mounts once `app_ready` resolves; no other command runs before that,
//! so `State<AppState>` is always managed by the time one does.

use std::path::PathBuf;
use std::sync::{Condvar, Mutex};

use tauri::{AppHandle, Manager};

use crate::commands::{CmdError, CmdResult};
use crate::state::AppState;

/// The outcome of opening the library, once known.
#[derive(Default)]
pub struct Startup {
    outcome: Mutex<Option<Result<(), String>>>,
    done: Condvar,
}

impl Startup {
    fn finish(&self, outcome: Result<(), String>) {
        if let Ok(mut slot) = self.outcome.lock() {
            *slot = Some(outcome);
        }
        self.done.notify_all();
    }

    fn wait(&self) -> Result<(), String> {
        let slot = self.outcome.lock().map_err(|e| e.to_string())?;
        let slot = self
            .done
            .wait_while(slot, |o| o.is_none())
            .map_err(|e| e.to_string())?;
        slot.clone().unwrap_or(Ok(()))
    }
}

/// Starts opening the library on its own thread.
pub fn begin(
    app: &AppHandle,
    data_dir: PathBuf,
    cache_dir: PathBuf,
    model_dir: PathBuf,
) -> std::io::Result<()> {
    app.manage(Startup::default());
    let handle = app.clone();
    std::thread::Builder::new()
        .name("reks-startup".into())
        .spawn(move || {
            let outcome = match AppState::open(&data_dir, &cache_dir, &model_dir) {
                Ok(state) => {
                    // Tracks added or changed while the app was closed, or
                    // analysed by an older analyser version.
                    state.analysis.queue_pending();
                    handle.manage(state);
                    Ok(())
                }
                Err(err) => {
                    tracing::error!(%err, "library failed to open");
                    Err(err.to_string())
                }
            };
            handle.state::<Startup>().finish(outcome);
        })?;
    Ok(())
}

/// Resolves once the library is open; fails with the reason it could not be.
#[tauri::command]
pub async fn app_ready(app: AppHandle) -> CmdResult<()> {
    tauri::async_runtime::spawn_blocking(move || app.state::<Startup>().wait())
        .await
        .map_err(CmdError::from)?
        .map_err(CmdError::new)
}
