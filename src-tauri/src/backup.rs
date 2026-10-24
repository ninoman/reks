//! Library backups (D36): commands over `reks_core::backup`, and the guard
//! that snapshots before a bulk operation.

use std::path::Path;
use std::process::Command;

use reks_core::{BackupInfo, BackupReason, BackupStatus, Library};
use tauri::{AppHandle, State};

use crate::commands::{blocking, CmdError, CmdResult};
use crate::state::AppState;

/// Snapshots the library before a bulk operation. A failure is logged and
/// shown in Settings → Library → Backups (core records it), but does not
/// stop the operation: a full disk should not lock the user out of an import.
pub fn before_bulk(lib: &Library, reason: BackupReason) {
    if let Err(err) = lib.snapshot(reason) {
        tracing::warn!(%err, reason = ?reason, "continuing without a pre-operation backup");
    }
}

/// The snapshots, newest first.
#[tauri::command]
pub async fn backups_list(state: State<'_, AppState>) -> CmdResult<Vec<BackupInfo>> {
    blocking(&state, Library::backups).await
}

/// Where backups live and the last failure, if any.
#[tauri::command]
pub async fn backup_status(state: State<'_, AppState>) -> CmdResult<BackupStatus> {
    Ok(state.library.backup_status())
}

/// "Back up now".
#[tauri::command]
pub async fn backup_now(state: State<'_, AppState>) -> CmdResult<BackupInfo> {
    blocking(&state, |lib| lib.snapshot(BackupReason::Manual)).await
}

/// Replaces the library with snapshot `name` (after snapshotting the current
/// one). Refused while a scan, import, export or conversion runs. The UI
/// then calls [`backup_restart`]: the player, the analysis queue and every
/// loaded track id belong to the library that was replaced.
#[tauri::command]
pub async fn backup_restore(state: State<'_, AppState>, name: String) -> CmdResult<()> {
    if let Some(running) = running_operation(&state) {
        return Err(CmdError::new(format!(
            "wait for the {running} to finish before restoring a backup"
        )));
    }
    blocking(&state, move |lib| lib.restore_backup(&name)).await
}

/// Restarts the app so it starts on the restored library.
#[tauri::command]
pub fn backup_restart(app: AppHandle) {
    app.restart();
}

/// Opens the backup folder in the system file manager.
#[tauri::command]
pub async fn backup_show_folder(state: State<'_, AppState>) -> CmdResult<()> {
    let dir = state
        .library
        .backup_status()
        .dir
        .ok_or_else(|| CmdError::new("this library has no backup folder".into()))?;
    std::fs::create_dir_all(&dir).map_err(|e| CmdError::new(e.to_string()))?;
    open_folder(&dir).map_err(|e| CmdError::new(format!("could not open {}: {e}", dir.display())))
}

fn running_operation(state: &AppState) -> Option<&'static str> {
    let busy = |m: &std::sync::Mutex<Option<reks_core::CancelToken>>| {
        m.lock().map_or(true, |slot| slot.is_some())
    };
    [
        (&state.scan_cancel, "scan"),
        (&state.import_cancel, "import"),
        (&state.export_cancel, "export"),
        (&state.convert_cancel, "conversion"),
    ]
    .into_iter()
    .find_map(|(m, what)| busy(m).then_some(what))
}

/// No opener plugin: the three desktop file managers each have a one-word launcher.
fn open_folder(dir: &Path) -> std::io::Result<()> {
    let launcher = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    };
    // explorer exits 1 even when it opens the folder, so only a failure to
    // start counts.
    Command::new(launcher).arg(dir).spawn().map(drop)
}
