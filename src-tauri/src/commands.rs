//! Tauri commands. Each one hands its arguments to `reks-core` on a blocking
//! worker thread and returns the result; no logic lives here (D2).

use std::path::PathBuf;
use std::sync::{Arc, PoisonError};

use reks_core::convert::{self, ConvertEvent, ConvertOptions, ConvertSpec, PlanView};
use reks_core::rbxml::{
    self, ImportEvent, ImportOptions, ImportPreview, ImportReport, PrefixRemap,
};
use reks_core::{
    ArtworkId, BulkOutcome, BulkPatch, CancelToken, CommentTagsPreview, CommentTagsReport, Facets,
    FilterPreset, FilterPresetOp, FilterPresetOutcome, Library, LibraryRoot, PlaylistId,
    PlaylistOp, PlaylistOutcome, PlaylistTree, RuleNode, ScanEvent, ScanReport, Settings,
    SettingsPatch, TagId, TagOp, TagOutcome, TagTree, TrackDetail, TrackId, TrackPage,
    TrackPageRows, TrackPatch, TrackQuery,
};
use reks_export_onelibrary::{OneLibraryWriter, RbVersion};
use reks_export_rb::device::{self, DeviceInfo};
use reks_export_rb::devices::{self, Device, DeviceId, DeviceMatch, ProfileStatus};
use reks_export_rb::dryrun::{self, DryRun};
use reks_export_rb::onelibrary as one;
use reks_export_rb::plan::{ExportOptions, Selection};
use reks_export_rb::report::{self, ProgressView, ReportView};
use reks_export_rb::{ApplyEnv, OneLibraryEnv, StickCheck, TargetProfile};
use serde::Serialize;
use tauri::ipc::{Channel, Response};
use tauri::State;

use crate::state::AppState;

/// Error returned to the UI: the message of the underlying error. Every one
/// is logged on creation, so a bug report's log tail shows the failures the
/// user saw (D34).
#[derive(Debug, Serialize)]
pub struct CmdError(String);

impl CmdError {
    pub(crate) fn new(message: String) -> Self {
        tracing::warn!(error = %message, "command failed");
        CmdError(message)
    }
}

impl<E: std::fmt::Display> From<E> for CmdError {
    fn from(e: E) -> Self {
        CmdError::new(e.to_string())
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

/// Pages `pages` of the track table in one pass (for a selection that spans
/// pages not loaded yet).
#[tauri::command]
pub async fn library_pages(
    state: State<'_, AppState>,
    query: TrackQuery,
    page_size: u32,
    pages: Vec<u32>,
) -> CmdResult<Vec<TrackPageRows>> {
    blocking(&state, move |lib| {
        lib.track_pages(&query, page_size, &pages)
    })
    .await
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

/// Applies one metadata edit to many tracks in a single transaction.
#[tauri::command]
pub async fn library_bulk_update_tracks(
    state: State<'_, AppState>,
    track_ids: Vec<TrackId>,
    patch: BulkPatch,
) -> CmdResult<BulkOutcome> {
    blocking(&state, move |lib| {
        lib.bulk_update_tracks(&track_ids, &patch)
    })
    .await
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

/// A smart playlist's saved rule, for editing it.
#[tauri::command]
pub async fn playlist_rule(state: State<'_, AppState>, id: PlaylistId) -> CmdResult<RuleNode> {
    blocking(&state, move |lib| lib.playlist_rule(id)).await
}

/// The My Tags tree, with a track count per tag.
#[tauri::command]
pub async fn tags_get(state: State<'_, AppState>) -> CmdResult<TagTree> {
    blocking(&state, |lib| lib.tags()).await
}

/// Applies one tag edit (tree or track links).
#[tauri::command]
pub async fn tags_mutate(state: State<'_, AppState>, op: TagOp) -> CmdResult<TagOutcome> {
    blocking(&state, move |lib| lib.tag_mutate(op)).await
}

/// Groups of tags whose names differ only in case, spacing or punctuation.
#[tauri::command]
pub async fn tags_duplicates(state: State<'_, AppState>) -> CmdResult<Vec<Vec<TagId>>> {
    blocking(&state, |lib| lib.tag_duplicates()).await
}

/// What reading My Tags from the library's comments would do (D39).
#[tauri::command]
pub async fn comment_tags_preview(state: State<'_, AppState>) -> CmdResult<CommentTagsPreview> {
    blocking(&state, |lib| lib.comment_tags_preview()).await
}

/// Reads My Tags from the library's comments (D39).
#[tauri::command]
pub async fn comment_tags_import(state: State<'_, AppState>) -> CmdResult<CommentTagsReport> {
    blocking(&state, |lib| lib.comment_tags_import()).await
}

/// Rewrites (or with `apply` false, counts) the tag block of every tagged
/// track's comment.
#[tauri::command]
pub async fn comment_tags_sync_all(state: State<'_, AppState>, apply: bool) -> CmdResult<u32> {
    blocking(&state, move |lib| lib.comment_tags_sync_all(apply)).await
}

/// Saved filters (D35), in display order.
#[tauri::command]
pub async fn filter_presets_get(state: State<'_, AppState>) -> CmdResult<Vec<FilterPreset>> {
    blocking(&state, |lib| lib.filter_presets()).await
}

/// Applies one saved-filter edit.
#[tauri::command]
pub async fn filter_presets_mutate(
    state: State<'_, AppState>,
    op: FilterPresetOp,
) -> CmdResult<FilterPresetOutcome> {
    blocking(&state, move |lib| lib.filter_preset_mutate(op)).await
}

/// Artwork image bytes, sent raw (an `ArrayBuffer` in the UI).
#[tauri::command]
pub async fn artwork_get(state: State<'_, AppState>, id: ArtworkId) -> CmdResult<Response> {
    blocking(&state, move |lib| lib.artwork(id))
        .await
        .map(Response::new)
}

/// A small JPEG of an artwork for lists, cached on disk; sent raw.
#[tauri::command]
pub async fn artwork_thumb(state: State<'_, AppState>, id: ArtworkId) -> CmdResult<Response> {
    blocking(&state, move |lib| lib.artwork_thumb(id))
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
            return Err(CmdError::new("a scan is already running".into()));
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
    state.analysis.queue_pending();
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
    let cache = Arc::clone(&state.parsed_xml);
    blocking(&state, move |lib| {
        let collection = cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&path)?;
        let opts = ImportOptions {
            remap,
            ..ImportOptions::default()
        };
        rbxml::preview(&collection, lib, &opts)
    })
    .await
}

/// Drops the parsed XML kept for the preview (the import wizard closed).
#[tauri::command]
pub fn rbxml_forget(state: State<'_, AppState>) {
    state
        .parsed_xml
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clear();
}

/// Imports a rekordbox `collection.xml`, streaming progress to `on_event`.
#[tauri::command]
pub async fn rbxml_import_start(
    state: State<'_, AppState>,
    path: PathBuf,
    remap: Vec<PrefixRemap>,
    comment_tags: bool,
    on_event: Channel<ImportEvent>,
) -> CmdResult<ImportReport> {
    let cancel = CancelToken::new();
    {
        let mut running = state
            .import_cancel
            .lock()
            .map_err(|_| "import state poisoned")?;
        if running.is_some() {
            return Err(CmdError::new("an import is already running".into()));
        }
        *running = Some(cancel.clone());
    }
    let cache = Arc::clone(&state.parsed_xml);
    let result = blocking(&state, move |lib| {
        let collection = cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&path)?;
        let opts = ImportOptions {
            remap,
            cancel: Some(cancel),
            comment_tags,
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
    state
        .parsed_xml
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clear();
    state.analysis.queue_pending();
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

/// Moves the cache to `path`. Refused while a scan, import, conversion or
/// analysis writes to it.
#[tauri::command]
pub async fn cache_dir_move(state: State<'_, AppState>, path: PathBuf) -> CmdResult<Settings> {
    let busy = |m: &std::sync::Mutex<Option<CancelToken>>| m.lock().map_or(true, |g| g.is_some());
    if busy(&state.scan_cancel)
        || busy(&state.import_cancel)
        || busy(&state.convert_cancel)
        || state.analysis.busy()
    {
        return Err(CmdError::new(
            "wait for the scan, import, conversion or analysis to finish (or cancel it) before moving the cache"
                .into(),
        ));
    }
    blocking(&state, move |lib| {
        lib.move_cache_dir(&path)?;
        lib.settings()
    })
    .await
}

/// Output device names, the system default first.
#[tauri::command]
pub async fn audio_devices() -> CmdResult<Vec<String>> {
    Ok(tauri::async_runtime::spawn_blocking(reks_mix::output_devices).await??)
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

/// Plans and writes `exportLibrary.db` for exports with the OneLibrary
/// option (experimental, D14).
const ONELIBRARY: OneLibraryWriter = OneLibraryWriter {
    version: RbVersion::LATEST,
};

/// A dry run and the id `export_apply` takes to write exactly that plan.

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedExport {
    plan_id: u64,
    #[serde(flatten)]
    dry_run: DryRun,
}

/// `ExportOptions::default()`, so the export wizard opens with the planner's
/// defaults instead of a copy of them.
#[tauri::command]
pub fn export_default_options() -> ExportOptions {
    ExportOptions::default()
}

/// Dry run of exporting `selection` to the stick mounted at `mount_point`
/// (phase 2 task 10): plans, writes nothing, returns the plan as the UI shows
/// it. Scans rekordbox's analysis folders, checks reks's analysis cache
/// (`options.anlz_preference`) and samples file bytes, so it runs
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
        dryrun::plan_at(&lib, &sel, &mount_point, &options, &ONELIBRARY).map(|plan| {
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
                return Err(CmdError::new(
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
            return Err(CmdError::new("an export is already running".into()));
        }
        *running = Some(cancel.clone());
    }
    let lib = Arc::clone(&state.library);
    let data_dir = state.data_dir.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let labels = dryrun::plan_track_labels(&lib, &plan);
        // Read again rather than kept from the dry run, so the key is in
        // memory only while an export needs it. Without one, apply refuses
        // a plan that writes exportLibrary.db.
        let key = if plan.onelibrary.is_some() {
            one::resolve_key()
        } else {
            None
        };
        let env = ApplyEnv {
            library: &lib,
            app_data_dir: &data_dir,
            stick: StickCheck::Mounted,
            onelibrary: key.as_ref().map(|key| OneLibraryEnv {
                export: &ONELIBRARY,
                key,
            }),
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
                return Err(CmdError::new(
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
            return Err(CmdError::new("a conversion is already running".into()));
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
    state.analysis.queue_pending();
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
