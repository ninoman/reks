//! Tauri commands. Each one hands its arguments to `reks-core` on a blocking
//! worker thread and returns the result; no logic lives here (D2).

use std::path::PathBuf;
use std::sync::Arc;

use reks_core::rbxml::{
    self, ImportEvent, ImportOptions, ImportPreview, ImportReport, PrefixRemap,
};
use reks_core::{
    ArtworkId, CancelToken, Facets, Library, PlaylistOp, PlaylistOutcome, PlaylistTree, ScanEvent,
    ScanReport, TrackDetail, TrackId, TrackPage, TrackPatch, TrackQuery,
};
use reks_mix::{PlayerState, Reopened};
use serde::Serialize;
use tauri::ipc::{Channel, Response};
use tauri::State;

use crate::state::{AppState, PlayerSlot};

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
        let r = slot.player.set_device(device)?;
        Ok((r, slot.track))
    })?;
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
