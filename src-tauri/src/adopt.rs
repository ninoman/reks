//! Adopt a foreign stick (Phase 2.5 task 12, D27): the dry run and its
//! apply. Logic lives in `reks_export_rb::adopt`; the stick is only read.

use std::sync::Arc;

use reks_export_rb::adopt::{self, AdoptChoices, AdoptPlan, AdoptReport};
use reks_export_rb::device::{self, DeviceInfo};
use serde::Serialize;
use tauri::State;

use crate::commands::{blocking, CmdError, CmdResult};
use crate::state::AppState;

/// The dry run, kept under `plan_id` for `adopt_apply`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdoptScan {
    pub plan_id: u64,
    pub plan: AdoptPlan,
}

/// Reads the mounted stick `info` (its volume ID from the OS) and plans
/// adopting its playlists and play history. Read-only on the stick and the
/// library. The newest scan wins.
#[tauri::command]
pub async fn adopt_scan(state: State<'_, AppState>, info: DeviceInfo) -> CmdResult<AdoptScan> {
    let plan_id = {
        let mut shown = state
            .shown_adopt
            .lock()
            .map_err(|_| "adopt state poisoned")?;
        shown.last_id += 1;
        shown.last_id
    };
    let lib = Arc::clone(&state.library);
    let plan = tauri::async_runtime::spawn_blocking(move || -> Result<_, CmdError> {
        let serial = device::volume_serial(&info);
        Ok(adopt::scan(&info.mount_point, serial.as_deref(), &lib)?)
    })
    .await??;
    let mut shown = state
        .shown_adopt
        .lock()
        .map_err(|_| "adopt state poisoned")?;
    if shown.last_id == plan_id {
        shown.plan = Some((plan_id, plan.clone()));
    }
    Ok(AdoptScan { plan_id, plan })
}

/// Applies the dry run `plan_id` with the dialog's choices: history first,
/// then the playlists and the stick's profile.
#[tauri::command]
pub async fn adopt_apply(
    state: State<'_, AppState>,
    plan_id: u64,
    choices: AdoptChoices,
) -> CmdResult<AdoptReport> {
    let plan = {
        let mut shown = state
            .shown_adopt
            .lock()
            .map_err(|_| "adopt state poisoned")?;
        match shown.plan.take() {
            Some((id, plan)) if id == plan_id => plan,
            other => {
                shown.plan = other;
                return Err(CmdError(
                    "this dry run is out of date or was already applied; scan again".into(),
                ));
            }
        }
    };
    blocking(&state, move |lib| adopt::apply(&plan, &choices, lib)).await
}
