//! Background analysis (Phase 3 task 11): commands over
//! `reks_analysis::AnalysisQueue`. The queue runs on its own threads; these
//! commands only queue, pause, cancel and report. Before queueing, tracks
//! rekordbox analysed are mapped into the cache instead (D10,
//! `reks_export_rb::rbmap`).

use std::path::Path;
use std::sync::{Arc, Mutex};

use reks_analysis::{AnalysisQueue, ModelPaths, QueueEvent, QueueOptions, QueueStatus};
use reks_core::{Library, TrackId};
use reks_export_rb::rbmap;
use tauri::ipc::Channel;
use tauri::State;

use crate::commands::{CmdError, CmdResult};
use crate::state::AppState;

/// The queue (absent when the beat model is missing) and where its events go.
pub struct Analysis {
    pub queue: Option<Arc<AnalysisQueue>>,
    lib: Arc<Library>,
    /// Held while a mapping pass runs, so two triggers don't map at once.
    mapping: Arc<Mutex<()>>,
    /// The UI's subscription; events are dropped while there is none.
    events: Arc<Mutex<Option<Channel<QueueEvent>>>>,
}

impl std::fmt::Debug for Analysis {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Analysis")
            .field("queue", &self.queue)
            .finish_non_exhaustive()
    }
}

impl Analysis {
    /// Starts the queue with the beat model in `model_dir`. A missing model
    /// is logged and leaves analysis off rather than stopping the app.
    pub fn start(lib: &Arc<Library>, model_dir: &Path) -> Self {
        let events: Arc<Mutex<Option<Channel<QueueEvent>>>> = Arc::default();
        let sink = Arc::clone(&events);
        let queue = AnalysisQueue::new(Arc::clone(lib), QueueOptions::new(model_dir), move |e| {
            if let Ok(channel) = sink.lock() {
                if let Some(c) = channel.as_ref() {
                    if let Err(err) = c.send(e.clone()) {
                        tracing::debug!(%err, "analysis channel closed");
                    }
                }
            }
        });
        match queue {
            Ok(queue) => Self {
                queue: Some(Arc::new(queue)),
                lib: Arc::clone(lib),
                mapping: Arc::default(),
                events,
            },
            Err(err) => {
                tracing::error!(%err, dir = %model_dir.display(), "analysis disabled");
                Self {
                    queue: None,
                    lib: Arc::clone(lib),
                    mapping: Arc::default(),
                    events,
                }
            }
        }
    }

    /// Maps rekordbox's analysis into the cache for the tracks that have it
    /// (D10), then queues every track still needing analysis (after start,
    /// scan, import, conversion). Runs on its own thread; errors are logged
    /// and the next trigger tries again.
    pub fn queue_pending(&self) {
        let lib = Arc::clone(&self.lib);
        let queue = self.queue.clone();
        let mapping = Arc::clone(&self.mapping);
        let spawned = std::thread::Builder::new()
            .name("rekordbox-map".into())
            .spawn(move || {
                if let Ok(_guard) = mapping.lock() {
                    map_rekordbox(&lib);
                }
                if let Some(q) = queue {
                    match q.enqueue_pending() {
                        Ok(n) if n > 0 => tracing::info!(tracks = n, "queued for analysis"),
                        Ok(_) => {}
                        Err(err) => tracing::warn!(%err, "cannot queue tracks for analysis"),
                    }
                }
            });
        if let Err(err) = spawned {
            tracing::warn!(%err, "cannot start the analysis queueing thread");
        }
    }

    fn queue(&self) -> CmdResult<&Arc<AnalysisQueue>> {
        self.queue
            .as_ref()
            .ok_or_else(|| CmdError::new("analysis is off: the beat model was not found".into()))
    }

    /// True while tracks are queued or being analysed.
    pub fn busy(&self) -> bool {
        self.queue
            .as_ref()
            .and_then(|q| q.status().ok())
            .is_some_and(|s| !s.is_idle())
    }
}

/// One mapping pass over rekordbox's local analysis folders.
fn map_rekordbox(lib: &Library) {
    match rbmap::map_local(lib, &[]) {
        Ok(run) if !run.roots.is_empty() && run.report.candidates > 0 => tracing::info!(
            candidates = run.report.candidates,
            mapped = run.report.mapped.len(),
            ms = run.total_ms as u64,
            "rekordbox analysis mapped instead of analysed"
        ),
        Ok(_) => {}
        Err(err) => tracing::warn!(%err, "cannot map rekordbox's analysis"),
    }
}

/// The bundled model directory, or this source tree's in debug builds.
pub fn model_dir(resource_dir: Option<&Path>) -> std::path::PathBuf {
    match resource_dir.map(|d| d.join("models")) {
        Some(dir) if dir.join(ModelPaths::SMALL_FILE).is_file() => dir,
        _ => ModelPaths::dev_dir(),
    }
}

/// Sends every queue event to `on_event` from now on (replacing an earlier
/// subscriber) and returns the current counts. `None` = analysis is off.
#[tauri::command]
pub fn analysis_subscribe(
    state: State<'_, AppState>,
    on_event: Channel<QueueEvent>,
) -> CmdResult<Option<QueueStatus>> {
    *state
        .analysis
        .events
        .lock()
        .map_err(|_| "analysis state poisoned")? = Some(on_event);
    match &state.analysis.queue {
        Some(q) => Ok(Some(q.status()?)),
        None => Ok(None),
    }
}

#[tauri::command]
pub fn analysis_pause(state: State<'_, AppState>) -> CmdResult<QueueStatus> {
    Ok(state.analysis.queue()?.pause()?)
}

#[tauri::command]
pub fn analysis_resume(state: State<'_, AppState>) -> CmdResult<QueueStatus> {
    Ok(state.analysis.queue()?.resume()?)
}

/// Drops the queued tracks; the ones being analysed finish but are not stored.
#[tauri::command]
pub fn analysis_cancel(state: State<'_, AppState>) -> CmdResult<QueueStatus> {
    Ok(state.analysis.queue()?.cancel()?)
}

/// Re-analyses `ids` ahead of everything else, ignoring the cache. Manual
/// grids and keys stay. Returns how many were queued.
#[tauri::command]
pub async fn analysis_reanalyse(state: State<'_, AppState>, ids: Vec<TrackId>) -> CmdResult<usize> {
    let queue = Arc::clone(state.analysis.queue()?);
    Ok(tauri::async_runtime::spawn_blocking(move || queue.reanalyse(&ids)).await??)
}
