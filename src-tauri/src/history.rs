//! Play history commands (Phase 2.5 task 9): imported sessions, their edits,
//! and the stick/backup import dry run. Logic lives in `reks_core::history`
//! and `reks_export_rb::history`.

use std::path::PathBuf;
use std::sync::Arc;

use reks_core::history::{
    self, HistoryImportPlan, ImportChoices, ImportReport, SessionDetail, SessionId, SessionPatch,
    SessionQuery, SessionRef, SessionRow,
};
use reks_core::{OneLibraryKey, TrackId};
use reks_export_onelibrary::history::{read_both, OneLibraryStatus};
use reks_export_rb::device::{self, DeviceInfo};
use reks_export_rb::history::{HistoryReadOptions, HistorySource};
use reks_export_rb::onelibrary::resolve_key;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::commands::{blocking, CmdError, CmdResult};
use crate::state::AppState;

/// Imported sessions, newest first.
#[tauri::command]
pub async fn history_sessions(
    state: State<'_, AppState>,
    query: SessionQuery,
) -> CmdResult<Vec<SessionRow>> {
    blocking(&state, move |lib| lib.sessions(&query)).await
}

/// One session with its entries in play order.
#[tauri::command]
pub async fn history_session(
    state: State<'_, AppState>,
    id: SessionId,
) -> CmdResult<SessionDetail> {
    blocking(&state, move |lib| lib.session(id)).await
}

/// Renames a session, sets venue or notes, or toggles Practice (which
/// recomputes its tracks' play counts).
#[tauri::command]
pub async fn history_session_update(
    state: State<'_, AppState>,
    id: SessionId,
    patch: SessionPatch,
) -> CmdResult<()> {
    blocking(&state, move |lib| lib.session_update(id, patch)).await
}

/// Deletes a session; its tracks' play counts are recomputed.
#[tauri::command]
pub async fn history_session_delete(state: State<'_, AppState>, id: SessionId) -> CmdResult<()> {
    blocking(&state, move |lib| lib.session_delete(id)).await
}

/// "Locate…": links an entry to a library track.
#[tauri::command]
pub async fn history_link_entry(
    state: State<'_, AppState>,
    session: SessionId,
    position: u32,
    track: TrackId,
) -> CmdResult<()> {
    blocking(&state, move |lib| {
        lib.session_link_entry(session, position, track)
    })
    .await
}

/// The sessions a track was played in ("Played in").
#[tauri::command]
pub async fn history_track_sessions(
    state: State<'_, AppState>,
    id: TrackId,
) -> CmdResult<Vec<SessionRef>> {
    blocking(&state, move |lib| lib.track_sessions(id)).await
}

/// What to read history from.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum ScanSource {
    /// A mounted stick; its serial is read from the OS.
    Stick { device: DeviceInfo },
    /// A Phase 2 backup folder (has `manifest.json`), or any folder standing
    /// in for a stick (no serial: manifest matches can't apply).
    Folder { path: PathBuf },
}

/// The dry run, kept under `plan_id` for `history_import`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryScan {
    pub plan_id: u64,
    pub source: HistorySource,
    /// The source's `exportLibrary.db`, read with the OneLibrary key.
    pub onelibrary: OneLibraryStatus,
    pub plan: HistoryImportPlan,
}

/// Reads the stick's or backup's history, from `export.pdb` and (with the
/// OneLibrary key) `exportLibrary.db`, and matches it (read-only on both the
/// source and the library). The newest scan wins.
#[tauri::command]
pub async fn history_scan(
    state: State<'_, AppState>,
    source: ScanSource,
) -> CmdResult<HistoryScan> {
    let plan_id = {
        let mut shown = state
            .shown_history
            .lock()
            .map_err(|_| "history state poisoned")?;
        shown.last_id += 1;
        shown.last_id
    };
    let (src, stick_serial) = match source {
        ScanSource::Stick { device } => {
            let serial = device::volume_serial(&device);
            (HistorySource::Stick(device.mount_point), serial)
        }
        ScanSource::Folder { path } => (HistorySource::from_folder(&path)?, None),
    };
    let lib = Arc::clone(&state.library);
    let read_src = src.clone();
    let (plan, onelibrary) =
        tauri::async_runtime::spawn_blocking(move || -> Result<_, CmdError> {
            let key = resolve_key();
            let (read, onelibrary) = read_both(
                &read_src,
                &HistoryReadOptions {
                    stick_serial,
                    hist1_dates: false,
                },
                key.as_ref().map(OneLibraryKey::expose),
            )?;
            Ok((history::scan(&read, &lib)?, onelibrary))
        })
        .await??;
    let mut shown = state
        .shown_history
        .lock()
        .map_err(|_| "history state poisoned")?;
    if shown.last_id == plan_id {
        shown.plan = Some((plan_id, plan.clone()));
    }
    Ok(HistoryScan {
        plan_id,
        source: src,
        onelibrary,
        plan,
    })
}

/// Imports the sessions of the dry run `plan_id` with the dialog's Practice
/// choices and recomputes the play counts they touch.
#[tauri::command]
pub async fn history_import(
    state: State<'_, AppState>,
    plan_id: u64,
    choices: ImportChoices,
) -> CmdResult<ImportReport> {
    let plan = {
        let mut shown = state
            .shown_history
            .lock()
            .map_err(|_| "history state poisoned")?;
        match shown.plan.take() {
            Some((id, plan)) if id == plan_id => plan,
            other => {
                shown.plan = other;
                return Err(CmdError::new(
                    "this dry run is out of date or was already imported; scan again".into(),
                ));
            }
        }
    };
    blocking(&state, move |lib| history::import(&plan, &choices, lib)).await
}
