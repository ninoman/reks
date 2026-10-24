//! `device_*` browse commands: a plugged-in stick's library and its tracks'
//! grids and waveforms for the sidebar's Devices section
//! (`reks_export_rb::browse`), and its pre-gig check
//! (`reks_export_rb::check`). Playing one is `mix::mix_load_stick`. Only
//! ever read from the stick.

use std::path::PathBuf;
use std::sync::Arc;

use reks_core::Beatgrid;
use reks_export_onelibrary::check::CheckReader;
use reks_export_rb::browse::{self, StickLibrary};
use reks_export_rb::check::{check_stick, CheckOptions, StickReport};
use reks_export_rb::device::{self, DeviceInfo};
use reks_export_rb::devices;
use reks_export_rb::onelibrary::resolve_key;
use tauri::ipc::Response;
use tauri::State;

use crate::commands::CmdResult;
use crate::state::AppState;

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

/// The read-only report on the stick `info` before a gig: audio is checked
/// against its profile's players when the stick is known, else the
/// strictest; play history against the library.
#[tauri::command]
pub async fn device_check(state: State<'_, AppState>, info: DeviceInfo) -> CmdResult<StickReport> {
    let lib = Arc::clone(&state.library);
    Ok(tauri::async_runtime::spawn_blocking(move || {
        let serial = device::volume_serial(&info);
        let target = match serial.as_deref().map(|s| lib.device_by_serial(s)) {
            Some(Ok(Some(dev))) => Some(devices::target(&dev)),
            Some(Err(e)) => {
                tracing::warn!(error = %e, "stick profile not read; checking against the strictest players");
                None
            }
            _ => None,
        };
        let opts = CheckOptions {
            target,
            volume_serial: serial,
            directory: false,
        };
        let reader = CheckReader { key: resolve_key() };
        check_stick(&info, &opts, &reader, Some(&lib))
    })
    .await?)
}
