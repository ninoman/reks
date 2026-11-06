//! Grid editor commands (Phase 3 tasks 13, 18). Logic lives in
//! `reks_core::beatgrid` (grid maths and storage),
//! `reks_analysis::columns` (waveform columns) and
//! `reks_analysis::transients` (snapping).

use reks_analysis::transients::{self, track_transients};
use reks_core::{beatgrid, Beatgrid, GridEdit, Library, TrackId};
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
        apply_edit(lib, id, &mut edit, snap == Some(true))
    })
    .await
}

/// One grid edit on track `id`, snapped first with `snap` (shared by the
/// deck's grid mode, `mix_grid_edit`).
pub(crate) fn apply_edit(
    lib: &Library,
    id: TrackId,
    edit: &mut GridEdit,
    snap: bool,
) -> Result<Beatgrid, String> {
    if let (true, Some(at)) = (snap, edit.snap_target()) {
        let found = track_transients(lib, id).map_err(|e| e.to_string())?;
        *at = transients::snap(&found, *at);
    }
    lib.edit_beatgrid_with(id, edit.clone(), || {
        reks_analysis::pipeline::cached_grid(lib, id)
    })
    .map_err(|e| e.to_string())
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

/// Track `id`'s colour preview for the track list's waveform thumbnail
/// (`reks_anlz::preview`: 1200 columns of `[height, low, mid, high]`).
/// Without `decode`, only from analyses and caches already on disk: empty
/// when none has them.
#[tauri::command]
pub async fn waveform_overview(
    state: State<'_, AppState>,
    id: TrackId,
    decode: bool,
) -> CmdResult<Response> {
    blocking(&state, move |lib| {
        reks_anlz::preview::track_preview(lib, id, decode)
    })
    .await
    .map(Response::new)
}
