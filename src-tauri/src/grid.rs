//! Grid editor commands (Phase 3 task 13). Logic lives in
//! `reks_core::beatgrid` (grid maths and storage) and
//! `reks_analysis::columns` (waveform columns).

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

/// Applies one editor edit; the result is stored as a `manual` grid.
#[tauri::command]
pub async fn beatgrid_edit(
    state: State<'_, AppState>,
    id: TrackId,
    edit: GridEdit,
) -> CmdResult<Beatgrid> {
    blocking(&state, move |lib| lib.edit_beatgrid(id, edit)).await
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
