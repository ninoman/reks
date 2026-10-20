//! Tauri commands. Each one hands its arguments to `reks-core` on a blocking
//! worker thread and returns the result; no logic lives here (D2).

use std::path::PathBuf;
use std::sync::Arc;

use reks_core::convert::{self, ConvertEvent, ConvertOptions, ConvertSpec, PlanView};
use reks_core::rbxml::{
    self, ImportEvent, ImportOptions, ImportPreview, ImportReport, PrefixRemap,
};
use reks_core::{
    ArtworkId, CancelToken, Facets, Library, LibraryRoot, PlaylistOp, PlaylistOutcome,
    PlaylistTree, ScanEvent, ScanReport, Settings, SettingsPatch, TrackDetail, TrackId, TrackPage,
    TrackPatch, TrackQuery,
};
use reks_export_rb::device::{self, DeviceInfo};
use reks_export_rb::devices::{self, Device, DeviceId, DeviceMatch, ProfileStatus};
use reks_export_rb::dryrun::{self, DryRun};
use reks_export_rb::plan::{ExportOptions, Selection};
use reks_export_rb::report::{self, ProgressView, ReportView};
use reks_export_rb::{ApplyEnv, StickCheck, TargetProfile};
use reks_mix::{PlayerState, Reopened};
use serde::Serialize;
use tauri::ipc::{Channel, Response};
use tauri::State;

use crate::state::{AppState, PlayerSlot};

/// Error returned to the UI: the message of the underlying error.
#[derive(Debug, Serialize)]
pub struct CmdError(pub(crate) String);

impl<E: std::fmt::Display> From<E> for CmdError {
    fn from(e: E) -> Self {
        CmdError(e.to_string())
    }
}

pub(crate) type CmdResult<T> = Result<T, CmdError>;

/// Runs `f` with the library on a blocking worker thread.
pub(crate) async fn blocking<T, E, F>(state: &State<'_, AppState>, f: F) -> CmdResult<T>
where
    T: Send + 'static,
    E: std::fmt::Display + Send + 'static,
    F: FnOnce(&Library) -> Result<T, E> + Send + 'static,
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

/// Scans `roots` (none: every library folder) into the library, streaming
/// progress to `on_event`.
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
        let roots = if roots.is_empty() {
            lib.library_roots()?
                .into_iter()
                .map(|r| PathBuf::from(r.path))
                .collect()
        } else {
            roots
        };
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

/// What importing `path` with `remap` would find; writes nothing.
#[tauri::command]
pub async fn rbxml_preview(
    state: State<'_, AppState>,
    path: PathBuf,
    remap: Vec<PrefixRemap>,
) -> CmdResult<ImportPreview> {
    blocking(&state, move |lib| {
        let collection = rbxml::parse_file(&path)?;
        let opts = ImportOptions {
            remap,
            ..ImportOptions::default()
        };
        rbxml::preview(&collection, lib, &opts)
    })
    .await
}

/// Imports a rekordbox `collection.xml`, streaming progress to `on_event`.
#[tauri::command]
pub async fn rbxml_import_start(
    state: State<'_, AppState>,
    path: PathBuf,
    remap: Vec<PrefixRemap>,
    on_event: Channel<ImportEvent>,
) -> CmdResult<ImportReport> {
    let cancel = CancelToken::new();
    {
        let mut running = state
            .import_cancel
            .lock()
            .map_err(|_| "import state poisoned")?;
        if running.is_some() {
            return Err(CmdError("an import is already running".into()));
        }
        *running = Some(cancel.clone());
    }
    let result = blocking(&state, move |lib| {
        let collection = rbxml::parse_file(&path)?;
        let opts = ImportOptions {
            remap,
            cancel: Some(cancel),
        };
        rbxml::import(&collection, lib, opts, |e| {
            if let Err(err) = on_event.send(e.clone()) {
                tracing::warn!(%err, "import progress channel closed");
            }
        })
    })
    .await;
    if let Ok(mut running) = state.import_cancel.lock() {
        *running = None;
    }
    result
}

/// Asks the running import, if any, to stop. Only takes effect while files
/// are read; the database stage always runs to the end.
#[tauri::command]
pub fn rbxml_import_cancel(state: State<'_, AppState>) {
    if let Ok(running) = state.import_cancel.lock() {
        if let Some(c) = running.as_ref() {
            c.cancel();
        }
    }
}

#[tauri::command]
pub async fn settings_get(state: State<'_, AppState>) -> CmdResult<Settings> {
    blocking(&state, Library::settings).await
}

/// Applies a partial update; returns the new settings.
#[tauri::command]
pub async fn settings_set(state: State<'_, AppState>, patch: SettingsPatch) -> CmdResult<Settings> {
    blocking(&state, move |lib| lib.update_settings(patch)).await
}

#[tauri::command]
pub async fn library_roots(state: State<'_, AppState>) -> CmdResult<Vec<LibraryRoot>> {
    blocking(&state, Library::library_roots).await
}

#[tauri::command]
pub async fn library_root_add(state: State<'_, AppState>, path: PathBuf) -> CmdResult<LibraryRoot> {
    blocking(&state, move |lib| lib.add_library_root(&path)).await
}

/// Stops watching a folder; its tracks stay.
#[tauri::command]
pub async fn library_root_remove(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    blocking(&state, move |lib| lib.remove_library_root(id)).await
}

/// Moves the cache to `path`. Refused while a scan or import writes to it.
#[tauri::command]
pub async fn cache_dir_move(state: State<'_, AppState>, path: PathBuf) -> CmdResult<Settings> {
    let busy = |m: &std::sync::Mutex<Option<CancelToken>>| m.lock().map_or(true, |g| g.is_some());
    if busy(&state.scan_cancel) || busy(&state.import_cancel) || busy(&state.convert_cancel) {
        return Err(CmdError(
            "wait for the scan, import or conversion to finish before moving the cache".into(),
        ));
    }
    blocking(&state, move |lib| {
        lib.move_cache_dir(&path)?;
        lib.settings()
    })
    .await
}

/// The player state plus the track on it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerStatus {
    #[serde(flatten)]
    pub state: PlayerState,
    pub track_id: Option<TrackId>,
}

/// Runs `f` with the player locked. Player calls only queue controls, so
/// the lock is short; decoding happens outside it.
fn with_player<T>(
    state: &State<'_, AppState>,
    f: impl FnOnce(&mut PlayerSlot) -> reks_mix::Result<T>,
) -> CmdResult<T> {
    let mut slot = state.player.lock().map_err(|_| "player state poisoned")?;
    Ok(f(&mut slot)?)
}

fn status(slot: &mut PlayerSlot) -> PlayerStatus {
    PlayerStatus {
        state: slot.player.state(),
        track_id: slot.track,
    }
}

/// Decodes `id` at the output rate, then loads it unless a newer load
/// request came in meanwhile. Seeks to `position_ms` and plays if asked.
async fn load_track(
    state: &State<'_, AppState>,
    id: TrackId,
    position_ms: u64,
    play: bool,
) -> CmdResult<PlayerStatus> {
    let (generation, rate) = with_player(state, |slot| {
        slot.generation += 1;
        Ok((slot.generation, slot.player.sample_rate()?))
    })?;
    let audio = blocking(state, move |lib| lib.preview_audio(id, rate)).await?;
    with_player(state, |slot| {
        if slot.generation == generation {
            slot.player.load(audio.pcm.samples, audio.pcm.sample_rate)?;
            slot.track = Some(id);
            if position_ms > 0 {
                slot.player.seek_ms(position_ms)?;
            }
            if play {
                slot.player.play()?;
            }
        }
        Ok(status(slot))
    })
}

/// Output device names, the system default first.
#[tauri::command]
pub async fn player_devices() -> CmdResult<Vec<String>> {
    Ok(tauri::async_runtime::spawn_blocking(reks_mix::output_devices).await??)
}

/// Loads track `id` stopped at the start, or playing with `play`.
#[tauri::command]
pub async fn player_load(
    state: State<'_, AppState>,
    id: TrackId,
    play: bool,
) -> CmdResult<PlayerStatus> {
    load_track(&state, id, 0, play).await
}

/// Switches the output device (`None`: system default), keeping the track,
/// position and play state.
#[tauri::command]
pub async fn player_set_device(
    state: State<'_, AppState>,
    device: Option<String>,
) -> CmdResult<PlayerStatus> {
    let (reopened, track) = with_player(&state, |slot| {
        let r = slot.player.set_device(device.clone())?;
        Ok((r, slot.track))
    })?;
    blocking(&state, move |lib| {
        lib.update_settings(SettingsPatch {
            audio_device: Some(device),
            ..SettingsPatch::default()
        })
    })
    .await?;
    match (reopened, track) {
        (
            Reopened::Reload {
                position_ms,
                playing,
            },
            Some(id),
        ) => load_track(&state, id, position_ms, playing).await,
        _ => with_player(&state, |slot| Ok(status(slot))),
    }
}

#[tauri::command]
pub fn player_play(state: State<'_, AppState>) -> CmdResult<PlayerStatus> {
    with_player(&state, |slot| {
        slot.player.play()?;
        Ok(status(slot))
    })
}

#[tauri::command]
pub fn player_pause(state: State<'_, AppState>) -> CmdResult<PlayerStatus> {
    with_player(&state, |slot| {
        slot.player.pause()?;
        Ok(status(slot))
    })
}

#[tauri::command]
pub fn player_seek(state: State<'_, AppState>, ms: u64) -> CmdResult<PlayerStatus> {
    with_player(&state, |slot| {
        slot.player.seek_ms(ms)?;
        Ok(status(slot))
    })
}

/// Volume 0..=1 like a knob.
#[tauri::command]
pub fn player_volume(state: State<'_, AppState>, volume: f32) -> CmdResult<PlayerStatus> {
    with_player(&state, |slot| {
        slot.player.set_volume(volume)?;
        Ok(status(slot))
    })
}

/// Polled by the transport bar (about 10 times a second while playing).
#[tauri::command]
pub fn player_state(state: State<'_, AppState>) -> CmdResult<PlayerStatus> {
    with_player(&state, |slot| Ok(status(slot)))
}

/// Overview waveform bytes (little-endian f32 `[min, max]` pairs per 1,000
/// frames), sent raw (an `ArrayBuffer` in the UI).
#[tauri::command]
pub async fn overview_waveform(state: State<'_, AppState>, id: TrackId) -> CmdResult<Response> {
    blocking(&state, move |lib| lib.overview_waveform(id))
        .await
        .map(Response::new)
}

/// Removable volumes an export can target (phase 2 task 1).
#[tauri::command]
pub async fn devices_list() -> CmdResult<Vec<DeviceInfo>> {
    Ok(tauri::async_runtime::spawn_blocking(device::list_removable).await?)
}

/// Unmounts and ejects the removable volume mounted at `mount_point`.
#[tauri::command]
pub async fn device_eject(mount_point: PathBuf) -> CmdResult<()> {
    Ok(tauri::async_runtime::spawn_blocking(move || device::eject_at(&mount_point)).await??)
}

/// Known sticks (stick profiles, D27), in the order they were added.
#[tauri::command]
pub async fn profiles_list(state: State<'_, AppState>) -> CmdResult<Vec<Device>> {
    blocking(&state, Library::devices).await
}

/// What the mounted stick `info` is to this library: known, or a prompt.
#[tauri::command]
pub async fn profile_identify(
    state: State<'_, AppState>,
    info: DeviceInfo,
) -> CmdResult<DeviceMatch> {
    blocking(&state, move |lib| devices::identify(&info, lib)).await
}

/// Remembers the mounted stick `info` as a new stick with an empty profile.
#[tauri::command]
pub async fn profile_create(
    state: State<'_, AppState>,
    info: DeviceInfo,
    volume_serial: Option<String>,
    name: String,
    target: TargetProfile,
) -> CmdResult<Device> {
    blocking(&state, move |lib| {
        devices::create(lib, &info, volume_serial.as_deref(), &name, target)
    })
    .await
}

/// Confirms an "Is this '<name>'?" prompt: the stick `info` is `id`.
#[tauri::command]
pub async fn profile_link(
    state: State<'_, AppState>,
    id: DeviceId,
    info: DeviceInfo,
    volume_serial: Option<String>,
) -> CmdResult<Device> {
    blocking(&state, move |lib| {
        devices::link(lib, id, &info, volume_serial.as_deref())
    })
    .await
}

#[tauri::command]
pub async fn profile_rename(
    state: State<'_, AppState>,
    id: DeviceId,
    name: String,
) -> CmdResult<Device> {
    blocking(&state, move |lib| devices::rename(lib, id, &name)).await
}

#[tauri::command]
pub async fn profile_set_target(
    state: State<'_, AppState>,
    id: DeviceId,
    target: TargetProfile,
) -> CmdResult<Device> {
    blocking(&state, move |lib| devices::set_target(lib, id, target)).await
}

/// The stick's profile as the export selection.
#[tauri::command]
pub async fn profile_selection(state: State<'_, AppState>, id: DeviceId) -> CmdResult<Selection> {
    blocking(&state, move |lib| devices::selection(lib, id)).await
}

/// Replaces the stick's profile with `playlists` (playlists and folders).
#[tauri::command]
pub async fn profile_set_selection(
    state: State<'_, AppState>,
    id: DeviceId,
    playlists: Vec<reks_core::PlaylistId>,
) -> CmdResult<()> {
    blocking(&state, move |lib| {
        devices::set_selection(lib, id, &playlists)
    })
    .await
}

/// Per-playlist status, "frees" and capacity of the stick's profile; reads
/// the stick at `mount_point` when it is plugged in.
#[tauri::command]
pub async fn profile_status(
    state: State<'_, AppState>,
    id: DeviceId,
    mount_point: Option<PathBuf>,
) -> CmdResult<ProfileStatus> {
    blocking(&state, move |lib| {
        devices::profile_status(lib, id, mount_point.as_deref())
    })
    .await
}

/// A dry run and the id `export_apply` takes to write exactly that plan.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedExport {
    plan_id: u64,
    #[serde(flatten)]
    dry_run: DryRun,
}

/// Dry run of exporting `selection` to the stick mounted at `mount_point`
/// (phase 2 task 10): plans, writes nothing, returns the plan as the UI shows
/// it. Scans rekordbox's analysis folders and samples file bytes, so it runs
/// on a blocking worker. The plan is kept under the returned id (the newest
/// call wins) for `export_apply`, with `device_id`: the stick profile the
/// export records its sync in.
#[tauri::command]
pub async fn export_plan(
    state: State<'_, AppState>,
    mount_point: PathBuf,
    selection: Selection,
    options: ExportOptions,
    device_id: Option<DeviceId>,
) -> CmdResult<PlannedExport> {
    let plan_id = {
        let mut shown = state
            .shown_plan
            .lock()
            .map_err(|_| "export state poisoned")?;
        shown.last_id += 1;
        shown.last_id
    };
    let lib = Arc::clone(&state.library);
    let sel = selection.clone();
    let (plan, dry_run) = tauri::async_runtime::spawn_blocking(move || {
        dryrun::plan_at(&lib, &sel, &mount_point, &options).map(|plan| {
            let dry_run = DryRun::build(&lib, &plan);
            (plan, dry_run)
        })
    })
    .await??;
    let mut shown = state
        .shown_plan
        .lock()
        .map_err(|_| "export state poisoned")?;
    if shown.last_id == plan_id {
        shown.plan = Some((plan_id, plan, selection, device_id));
    }
    Ok(PlannedExport { plan_id, dry_run })
}

/// Writes the plan the dry run `plan_id` showed to its stick
/// (`reks_export_rb::apply`), streaming progress to `on_event`. Refused when
/// a newer dry run replaced it or it was already exported; `apply` itself
/// refuses a stick that changed since. Problems with the export are in the
/// returned report, not errors.
#[tauri::command]
pub async fn export_apply(
    state: State<'_, AppState>,
    plan_id: u64,
    on_event: Channel<ProgressView>,
) -> CmdResult<ReportView> {
    let (_, plan, selection, device_id) = {
        let mut shown = state
            .shown_plan
            .lock()
            .map_err(|_| "export state poisoned")?;
        match shown.plan.take() {
            Some(p) if p.0 == plan_id => p,
            other => {
                shown.plan = other;
                return Err(CmdError(
                    "this dry run is out of date or was already exported; plan again".into(),
                ));
            }
        }
    };
    let cancel = CancelToken::new();
    {
        let mut running = state
            .export_cancel
            .lock()
            .map_err(|_| "export state poisoned")?;
        if running.is_some() {
            return Err(CmdError("an export is already running".into()));
        }
        *running = Some(cancel.clone());
    }
    let lib = Arc::clone(&state.library);
    let data_dir = state.data_dir.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let labels = dryrun::plan_track_labels(&lib, &plan);
        let env = ApplyEnv {
            library: &lib,
            app_data_dir: &data_dir,
            stick: StickCheck::Mounted,
        };
        let progress = |v: ProgressView| {
            if let Err(err) = on_event.send(v) {
                tracing::warn!(%err, "export progress channel closed");
            }
        };
        report::export(
            &env, &plan, &selection, labels, &progress, &cancel, device_id,
        )
    })
    .await;
    if let Ok(mut running) = state.export_cancel.lock() {
        *running = None;
    }
    Ok(result?)
}

/// Asks the running export, if any, to stop. Takes effect until the new
/// `export.pdb` is written; the progress says when that is.
#[tauri::command]
pub fn export_cancel(state: State<'_, AppState>) {
    if let Ok(running) = state.export_cancel.lock() {
        if let Some(c) = running.as_ref() {
            c.cancel();
        }
    }
}

/// A converter dry run and the id `convert_apply` takes to run exactly that plan.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedConvert {
    plan_id: u64,
    #[serde(flatten)]
    view: PlanView,
}

/// Dry run of converting `tracks` (phase 2.6 task 6): reads file headers,
/// writes nothing. The plan is kept under the returned id (the newest call
/// wins) for `convert_apply`.
#[tauri::command]
pub async fn convert_plan(
    state: State<'_, AppState>,
    tracks: Vec<TrackId>,
    spec: ConvertSpec,
    options: ConvertOptions,
) -> CmdResult<PlannedConvert> {
    let plan_id = {
        let mut shown = state
            .shown_convert
            .lock()
            .map_err(|_| "convert state poisoned")?;
        shown.last_id += 1;
        shown.last_id
    };
    let (plan, view) = blocking(&state, move |lib| {
        convert::plan(lib, &tracks, &spec, &options).map(|plan| {
            let view = convert::plan_view(lib, &plan);
            (plan, view)
        })
    })
    .await?;
    let mut shown = state
        .shown_convert
        .lock()
        .map_err(|_| "convert state poisoned")?;
    if shown.last_id == plan_id {
        shown.plan = Some((plan_id, plan));
    }
    Ok(PlannedConvert { plan_id, view })
}

/// Converts the plan the dry run `plan_id` showed, streaming progress to
/// `on_event`. Refused when a newer dry run replaced it or it already ran.
/// Per-file failures are in the report, not errors.
#[tauri::command]
pub async fn convert_apply(
    state: State<'_, AppState>,
    plan_id: u64,
    on_event: Channel<ConvertEvent>,
) -> CmdResult<convert::ReportView> {
    let plan = {
        let mut shown = state
            .shown_convert
            .lock()
            .map_err(|_| "convert state poisoned")?;
        match shown.plan.take() {
            Some((id, plan)) if id == plan_id => plan,
            other => {
                shown.plan = other;
                return Err(CmdError(
                    "this dry run is out of date or was already converted; plan again".into(),
                ));
            }
        }
    };
    let cancel = CancelToken::new();
    {
        let mut running = state
            .convert_cancel
            .lock()
            .map_err(|_| "convert state poisoned")?;
        if running.is_some() {
            return Err(CmdError("a conversion is already running".into()));
        }
        *running = Some(cancel.clone());
    }
    let result = blocking(&state, move |lib| {
        let events = |e: ConvertEvent| {
            if let Err(err) = on_event.send(e) {
                tracing::warn!(%err, "convert progress channel closed");
            }
        };
        convert::apply(lib, &plan, &events, &cancel)
            .map(|report| convert::report_view(lib, &plan, &report))
    })
    .await;
    if let Ok(mut running) = state.convert_cancel.lock() {
        *running = None;
    }
    result
}

/// Asks the running conversion, if any, to stop after cleaning up its
/// current file.
#[tauri::command]
pub fn convert_cancel(state: State<'_, AppState>) {
    if let Ok(running) = state.convert_cancel.lock() {
        if let Some(c) = running.as_ref() {
            c.cancel();
        }
    }
}
