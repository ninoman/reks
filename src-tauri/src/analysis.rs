//! Background analysis (Phase 3 task 11): commands over
//! `reks_analysis::AnalysisQueue`. The queue runs on its own threads; these
//! commands only queue, pause, cancel and report.

use std::path::Path;
use std::sync::{Arc, Mutex};

use reks_analysis::{AnalysisQueue, ModelPaths, QueueEvent, QueueOptions, QueueStatus};
use reks_core::{Library, TrackId};
use tauri::ipc::Channel;
use tauri::State;

use crate::commands::{CmdError, CmdResult};
use crate::state::AppState;

/// The queue (absent when the beat model is missing) and where its events go.
pub struct Analysis {
    pub queue: Option<Arc<AnalysisQueue>>,
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
                events,
            },
            Err(err) => {
                tracing::error!(%err, dir = %model_dir.display(), "analysis disabled");
                Self {
                    queue: None,
                    events,
                }
            }
        }
    }

    /// Queues every track that needs analysis (after start, scan, import,
    /// conversion). Errors are logged: the next trigger tries again.
    pub fn queue_pending(&self) {
        if let Some(q) = &self.queue {
            match q.enqueue_pending() {
                Ok(n) if n > 0 => tracing::info!(tracks = n, "queued for analysis"),
                Ok(_) => {}
                Err(err) => tracing::warn!(%err, "cannot queue tracks for analysis"),
            }
        }
    }

    fn queue(&self) -> CmdResult<&Arc<AnalysisQueue>> {
        self.queue
            .as_ref()
            .ok_or_else(|| CmdError("analysis is off: the beat model was not found".into()))
    }

    /// True while tracks are queued or being analysed.
    pub fn busy(&self) -> bool {
        self.queue
            .as_ref()
            .and_then(|q| q.status().ok())
            .is_some_and(|s| !s.is_idle())
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
