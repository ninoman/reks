//! `device_*` browse commands: a plugged-in stick's library and its tracks'
//! grids and waveforms for the sidebar's Devices section
//! (`reks_export_rb::browse`). Playing one is `mix::mix_load_stick`. Only
//! ever read from the stick.

use std::path::PathBuf;

use reks_core::Beatgrid;
use reks_export_rb::browse::{self, StickLibrary};
use tauri::ipc::Response;

use crate::commands::CmdResult;

/// The library on the stick at `mount_point`; `None` when it has none.
#[tauri::command]
pub async fn device_library(mount_point: PathBuf) -> CmdResult<Option<StickLibrary>> {
    Ok(tauri::async_runtime::spawn_blocking(move || browse::read_library(&mount_point)).await??)
}

/// Grid of stick track `track_id`, from its ANLZ files.
#[tauri::command]
pub async fn device_beatgrid(mount_point: PathBuf, track_id: u32) -> CmdResult<Beatgrid> {
    Ok(
        tauri::async_runtime::spawn_blocking(move || browse::beatgrid(&mount_point, track_id))
            .await??,
    )
}

/// Waveform columns of stick track `track_id`, laid out as
/// `waveform_columns` sends them. Not cached: the stick may change.
#[tauri::command]
pub async fn device_waveform_columns(mount_point: PathBuf, track_id: u32) -> CmdResult<Response> {
    let columns = tauri::async_runtime::spawn_blocking(move || {
        browse::waveform_columns(&mount_point, track_id)
    })
    .await??;
    Ok(Response::new(columns))
}
