//! Relocate missing files and change a library folder (Phase 1 task 15,
//! D37): the dry runs and their applies. Logic lives in `reks_core::relocate`.

use std::path::PathBuf;

use reks_core::relocate::{
    self, RelocateChoices, RelocateEvent, RelocatePlan, RelocateReport, RelocateScope, RemapPlan,
    RemapReport,
};
use reks_core::CancelToken;
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::State;

use crate::commands::{blocking, CmdError, CmdResult};
use crate::state::AppState;

/// The dry run, kept under `plan_id` for `relocate_apply`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelocateScan {
    pub plan_id: u64,
    pub plan: RelocatePlan,
}

/// Marks a relocate run as started; one at a time.
fn begin(state: &AppState) -> Result<CancelToken, CmdError> {
    let mut running = state
        .relocate_cancel
        .lock()
        .map_err(|_| "relocate state poisoned")?;
    if running.is_some() {
        return Err(CmdError::new("a relocate is already running".into()));
    }
    let cancel = CancelToken::new();
    *running = Some(cancel.clone());
    Ok(cancel)
}

fn end(state: &AppState) {
    if let Ok(mut running) = state.relocate_cancel.lock() {
        *running = None;
    }
}

/// The scan rewrites the same rows; never at the same time.
fn refuse_during_scan(state: &AppState) -> Result<(), CmdError> {
    let scanning = state.scan_cancel.lock().map_or(true, |g| g.is_some());
    if scanning {
        return Err(CmdError::new(
            "wait for the scan to finish (or stop it) first".into(),
        ));
    }
    Ok(())
}

fn send(on_event: &Channel<RelocateEvent>, e: RelocateEvent) {
    if let Err(err) = on_event.send(e) {
        tracing::warn!(%err, "relocate progress channel closed");
    }
}

/// Looks for the missing tracks in `folder`; writes nothing. The newest
/// finished plan is the one `relocate_apply` accepts.
#[tauri::command]
pub async fn relocate_plan(
    state: State<'_, AppState>,
    scope: RelocateScope,
    folder: PathBuf,
    on_event: Channel<RelocateEvent>,
) -> CmdResult<RelocateScan> {
    refuse_during_scan(&state)?;
    let cancel = begin(&state)?;
    let plan_id = {
        let mut shown = state
            .shown_relocate
            .lock()
            .map_err(|_| "relocate state poisoned")?;
        shown.last_id += 1;
        shown.plan = None;
        shown.last_id
    };
    let result = blocking(&state, move |lib| {
        relocate::plan(lib, &scope, &folder, &cancel, |e| send(&on_event, e))
    })
    .await;
    end(&state);
    let plan = result?;
    if !plan.cancelled {
        let mut shown = state
            .shown_relocate
            .lock()
            .map_err(|_| "relocate state poisoned")?;
        if shown.last_id == plan_id {
            shown.plan = Some((plan_id, plan.clone()));
        }
    }
    Ok(RelocateScan { plan_id, plan })
}

/// Applies the dry run `plan_id` with the dialog's choices.
#[tauri::command]
pub async fn relocate_apply(
    state: State<'_, AppState>,
    plan_id: u64,
    choices: RelocateChoices,
    on_event: Channel<RelocateEvent>,
) -> CmdResult<RelocateReport> {
    refuse_during_scan(&state)?;
    let plan = {
        let shown = state
            .shown_relocate
            .lock()
            .map_err(|_| "relocate state poisoned")?;
        match &shown.plan {
            Some((id, plan)) if *id == plan_id => plan.clone(),
            _ => {
                return Err(CmdError::new(
                    "this dry run is out of date or was already applied; run it again".into(),
                ))
            }
        }
    };
    let cancel = begin(&state)?;
    let result = blocking(&state, move |lib| {
        relocate::apply(lib, &plan, &choices, &cancel, |e| send(&on_event, e))
    })
    .await;
    end(&state);
    let report = result?;
    // A cancelled apply wrote nothing, so the dry run stays valid.
    if !report.cancelled {
        if let Ok(mut shown) = state.shown_relocate.lock() {
            if shown.plan.as_ref().is_some_and(|(id, _)| *id == plan_id) {
                shown.plan = None;
            }
        }
    }
    state.analysis.queue_pending();
    Ok(report)
}

/// Asks the running relocate, if any, to stop.
#[tauri::command]
pub fn relocate_cancel(state: State<'_, AppState>) {
    if let Ok(running) = state.relocate_cancel.lock() {
        if let Some(c) = running.as_ref() {
            c.cancel();
        }
    }
}

/// What changing library folder `root_id` to `folder` would do; writes nothing.
#[tauri::command]
pub async fn remap_plan(
    state: State<'_, AppState>,
    root_id: i64,
    folder: PathBuf,
) -> CmdResult<RemapPlan> {
    blocking(&state, move |lib| {
        relocate::plan_remap(lib, root_id, &folder)
    })
    .await
}

/// Changes library folder `root_id` to `folder` (checked again, in one
/// transaction).
#[tauri::command]
pub async fn remap_apply(
    state: State<'_, AppState>,
    root_id: i64,
    folder: PathBuf,
) -> CmdResult<RemapReport> {
    refuse_during_scan(&state)?;
    blocking(&state, move |lib| relocate::remap(lib, root_id, &folder)).await
}
