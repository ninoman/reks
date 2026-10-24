//! Grid editor commands (Phase 3 tasks 13, 18). Logic lives in
//! `reks_core::beatgrid` (grid maths and storage),
//! `reks_analysis::columns` (waveform columns) and
//! `reks_analysis::transients` (snapping).

use reks_analysis::transients::{self, track_transients};
use reks_core::{beatgrid, Beatgrid, GridEdit, TrackId};
use tauri::ipc::Response;
use tauri::State;

use crate::commands::{blocking, CmdResult};
use crate::state::AppState;

/// Track `id`'s grid with every beat line.
#[tauri::command]
pub async fn beatgrid_get(state: State<'_, AppState>, id: TrackId) -> CmdResult<Beatgrid> {
    blocking(&state, move |lib| lib.beatgrid(id)).await
}

/// Applies one editor edit; the result is stored as a `manual` grid. With
/// `snap`, the position the edit places moves to the nearest transient. A
/// reset of a grid edited before its base was kept falls back to reks's
/// cached analysis.
#[tauri::command]
pub async fn beatgrid_edit(
    state: State<'_, AppState>,
    id: TrackId,
    mut edit: GridEdit,
    snap: Option<bool>,
) -> CmdResult<Beatgrid> {
    blocking(&state, move |lib| {
        if let (Some(true), Some(at)) = (snap, edit.snap_target()) {
            let found = track_transients(lib, id).map_err(|e| e.to_string())?;
            *at = transients::snap(&found, *at);
        }
        lib.edit_beatgrid_with(id, edit, || reks_analysis::pipeline::cached_grid(lib, id))
            .map_err(|e| e.to_string())
    })
    .await
}

/// Track `id`'s transients (ms), for the editor's snap marks. Computed and
/// cached on first use.
#[tauri::command]
pub async fn waveform_transients(state: State<'_, AppState>, id: TrackId) -> CmdResult<Vec<u32>> {
    blocking(&state, move |lib| track_transients(lib, id)).await
}

/// Tempo (×100) of the newest run of taps (ms timestamps), if there is one.
#[tauri::command]
pub fn beatgrid_tap_tempo(taps_ms: Vec<i64>) -> Option<i64> {
    beatgrid::tap_tempo(&taps_ms)
}

/// Waveform columns at 150/s, 4 bytes each (`[low, mid, high, mono]`), sent
/// raw (an `ArrayBuffer` in the UI). Computed and cached on first use.
#[tauri::command]
pub async fn waveform_columns(state: State<'_, AppState>, id: TrackId) -> CmdResult<Response> {
    blocking(&state, move |lib| {
        reks_analysis::columns::track_columns(lib, id)
    })
    .await
    .map(Response::new)
}

/// Most columns a `waveform_overview` request may ask for.
const MAX_OVERVIEW_WIDTH: usize = 2048;

/// Track `id`'s columns reduced to at most `width` (the track list's
/// waveform thumbnails), in the `waveform_columns` layout.
#[tauri::command]
pub async fn waveform_overview(
    state: State<'_, AppState>,
    id: TrackId,
    width: usize,
) -> CmdResult<Response> {
    let width = width.min(MAX_OVERVIEW_WIDTH);
    blocking(&state, move |lib| {
        reks_analysis::columns::track_overview(lib, id, width)
    })
    .await
    .map(Response::new)
}
