//! Tauri commands. Each one hands its arguments to `reks-core` on a blocking
//! worker thread and returns the result; no logic lives here (D2).

use std::path::PathBuf;
use std::sync::Arc;

use reks_core::rbxml::{self, ImportEvent, ImportOptions, ImportReport, PrefixRemap};
use reks_core::{
    ArtworkId, CancelToken, Facets, Library, PlaylistOp, PlaylistOutcome, PlaylistTree, ScanEvent,
    ScanReport, TrackDetail, TrackId, TrackPage, TrackPatch, TrackQuery,
};
use serde::Serialize;
use tauri::ipc::{Channel, Response};
use tauri::State;

use crate::state::AppState;

/// Error returned to the UI: the message of the underlying error.
#[derive(Debug, Serialize)]
pub struct CmdError(String);

impl<E: std::fmt::Display> From<E> for CmdError {
    fn from(e: E) -> Self {
        CmdError(e.to_string())
    }
}

type CmdResult<T> = Result<T, CmdError>;

/// Runs `f` with the library on a blocking worker thread.
async fn blocking<T, F>(state: &State<'_, AppState>, f: F) -> CmdResult<T>
where
    T: Send + 'static,
    F: FnOnce(&Library) -> reks_core::Result<T> + Send + 'static,
{
    let lib = Arc::clone(&state.library);
    let result = tauri::async_runtime::spawn_blocking(move || f(&lib)).await?;
    Ok(result?)
}

/// One page of the track table.
#[tauri::command]
pub async fn library_query(state: State<'_, AppState>, query: TrackQuery) -> CmdResult<TrackPage> {
    blocking(&state, move |lib| lib.tracks(&query)).await
}

/// Option lists for the filter sidebar.
#[tauri::command]
pub async fn library_facets(state: State<'_, AppState>) -> CmdResult<Facets> {
    blocking(&state, |lib| lib.facets()).await
}

/// One track in full, with its cues and beatgrid summary.
#[tauri::command]
pub async fn library_track(state: State<'_, AppState>, id: TrackId) -> CmdResult<TrackDetail> {
    blocking(&state, move |lib| lib.track(id)).await
}

/// Applies a metadata edit and returns the updated track.
#[tauri::command]
pub async fn library_update_track(
    state: State<'_, AppState>,
    id: TrackId,
    patch: TrackPatch,
) -> CmdResult<TrackDetail> {
    blocking(&state, move |lib| lib.update_track(id, &patch)).await
}

/// The playlist tree.
#[tauri::command]
pub async fn playlists_get(state: State<'_, AppState>) -> CmdResult<PlaylistTree> {
    blocking(&state, |lib| lib.playlists()).await
}

/// Applies one playlist edit. Duplicates found by "add tracks" come back as
/// an outcome, not an error.
#[tauri::command]
pub async fn playlists_mutate(
    state: State<'_, AppState>,
    op: PlaylistOp,
) -> CmdResult<PlaylistOutcome> {
    blocking(&state, move |lib| lib.playlist_mutate(op)).await
}

/// Artwork image bytes, sent raw (an `ArrayBuffer` in the UI).
#[tauri::command]
pub async fn artwork_get(state: State<'_, AppState>, id: ArtworkId) -> CmdResult<Response> {
    blocking(&state, move |lib| lib.artwork(id))
        .await
        .map(Response::new)
}

/// Scans `roots` into the library, streaming progress to `on_event`.
#[tauri::command]
pub async fn scan_start(
    state: State<'_, AppState>,
    roots: Vec<PathBuf>,
    on_event: Channel<ScanEvent>,
) -> CmdResult<ScanReport> {
    let cancel = CancelToken::new();
    {
        let mut running = state
            .scan_cancel
            .lock()
            .map_err(|_| "scan state poisoned")?;
        if running.is_some() {
            return Err(CmdError("a scan is already running".into()));
        }
        *running = Some(cancel.clone());
    }
    let result = blocking(&state, move |lib| {
        reks_core::scan(&roots, lib, &cancel, |e| {
            if let Err(err) = on_event.send(e.clone()) {
                tracing::warn!(%err, "scan progress channel closed");
            }
        })
    })
    .await;
    if let Ok(mut running) = state.scan_cancel.lock() {
        *running = None;
    }
    result
}

/// Asks the running scan, if any, to stop.
#[tauri::command]
pub fn scan_cancel(state: State<'_, AppState>) {
    if let Ok(running) = state.scan_cancel.lock() {
        if let Some(c) = running.as_ref() {
            c.cancel();
        }
    }
}

/// Imports a rekordbox `collection.xml`, streaming progress to `on_event`.
#[tauri::command]
pub async fn rbxml_import_start(
    state: State<'_, AppState>,
    path: PathBuf,
    remap: Vec<PrefixRemap>,
    on_event: Channel<ImportEvent>,
) -> CmdResult<ImportReport> {
    blocking(&state, move |lib| {
        let collection = rbxml::parse_file(&path)?;
        rbxml::import(&collection, lib, ImportOptions { remap }, |e| {
            if let Err(err) = on_event.send(e.clone()) {
                tracing::warn!(%err, "import progress channel closed");
            }
        })
    })
    .await
}
