//! Background analysis queue (phase 3 task 11).
//!
//! N workers (default: cores − 1), each a plain thread with its own
//! [`BeatTracker`] (loaded on its first track), run their analyses on one
//! dedicated rayon pool of N threads through `install`, so the beat model's
//! own rayon work never spreads past N cores. The worker loops are not rayon
//! jobs themselves: a pool thread waiting inside the model's `join` may run
//! any queued job, and a never-ending loop picked up there would hang it.
//!
//! Per track: hash the file; reuse the cache entry if it matches this
//! analyser version and hash (unless the track was queued by
//! [`AnalysisQueue::reanalyse`]); otherwise run [`analyse_file`] and write the
//! cache; then store the result in the library under the precedence rules of
//! `reks_core::analysis` (manual and rekordbox data are never overwritten).
//!
//! - [`AnalysisQueue::pause`]: workers finish their current track and wait.
//! - [`AnalysisQueue::cancel`]: drops everything queued; tracks being
//!   analysed run to the end (decode and inference cannot be interrupted) but
//!   their results are discarded.
//! - A file that cannot be read (unplugged drive) fails without a trace in
//!   the library, so it is retried next time; a file that reads but cannot
//!   be decoded or analysed is recorded as failed (`analysis_error`) and not
//!   retried until the file or the analyser version changes.

use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use reks_core::analysis::file_content_hash;
use reks_core::{AnalysisJob, Library, StoreOutcome, TrackId};
use serde::Serialize;

use crate::beats::{BeatTracker, ModelPaths};
use crate::cache;
use crate::pipeline::{analyse_file, ANALYSER_VERSION};

#[derive(Debug, thiserror::Error)]
pub enum QueueError {
    #[error("beat model file missing: {0}")]
    MissingModel(PathBuf),
    #[error("cannot start analysis workers: {0}")]
    Pool(String),
    #[error(transparent)]
    Library(#[from] reks_core::Error),
    #[error("analysis queue state poisoned")]
    Poisoned,
}

/// Queue settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueOptions {
    /// Worker threads, at least 1.
    pub workers: usize,
    pub models: ModelPaths,
}

impl QueueOptions {
    /// Cores − 1 workers (at least 1) with the small model in `model_dir`.
    pub fn new(model_dir: &Path) -> Self {
        Self {
            workers: default_workers(),
            models: ModelPaths::small(model_dir),
        }
    }
}

/// Cores − 1, at least 1: one core stays free for the UI and playback.
pub fn default_workers() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get().saturating_sub(1))
        .unwrap_or(1)
        .max(1)
}

/// Counts for the progress indicator. `done`, `failed` and `total` cover the
/// current batch: they reset when work arrives at an idle queue.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueStatus {
    /// Waiting for a worker.
    pub queued: usize,
    /// Being analysed now.
    pub running: usize,
    pub done: usize,
    pub failed: usize,
    /// Tracks added to this batch (done + failed + queued + running, plus
    /// any dropped by a cancel).
    pub total: usize,
    pub paused: bool,
}

impl QueueStatus {
    pub fn is_idle(&self) -> bool {
        self.queued == 0 && self.running == 0
    }
}

/// Progress reported to the sink given to [`AnalysisQueue::new`], from the
/// worker threads.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum QueueEvent {
    /// A track finished and its result is in the library.
    #[serde(rename_all = "camelCase")]
    Analysed {
        track_id: TrackId,
        /// Taken from the cache instead of re-running the analyser.
        cached: bool,
        stored: StoreOutcome,
    },
    #[serde(rename_all = "camelCase")]
    Failed { track_id: TrackId, message: String },
    /// Counts changed.
    Status { status: QueueStatus },
}

type Sink = Box<dyn Fn(&QueueEvent) + Send + Sync>;

#[derive(Debug, Clone, Copy)]
struct Item {
    track_id: TrackId,
    /// Ignore the cache (re-analyse).
    force: bool,
}

#[derive(Debug, Default)]
struct State {
    pending: VecDeque<Item>,
    /// Track ids in `pending`.
    queued: HashSet<TrackId>,
    /// Track ids a worker has taken; not queued again unless forced.
    running_ids: HashSet<TrackId>,
    running: usize,
    done: usize,
    failed: usize,
    total: usize,
    paused: bool,
    /// Bumped by `cancel`; results from an older epoch are dropped.
    epoch: u64,
    shutdown: bool,
}

impl State {
    fn status(&self) -> QueueStatus {
        QueueStatus {
            queued: self.pending.len(),
            running: self.running,
            done: self.done,
            failed: self.failed,
            total: self.total,
            paused: self.paused,
        }
    }
}

struct Shared {
    lib: Arc<Library>,
    models: ModelPaths,
    state: Mutex<State>,
    /// Workers wait here for work or resume.
    work: Condvar,
    /// `wait_idle` waits here.
    idle: Condvar,
    sink: Sink,
    /// Where the analyses run (see the module docs).
    pool: rayon::ThreadPool,
}

impl Shared {
    fn lock(&self) -> Result<MutexGuard<'_, State>, QueueError> {
        self.state.lock().map_err(|_| QueueError::Poisoned)
    }

    fn emit_status(&self, status: QueueStatus) {
        (self.sink)(&QueueEvent::Status { status });
    }
}

/// The background analysis queue. Dropping it stops the workers after their
/// current track.
pub struct AnalysisQueue {
    shared: Arc<Shared>,
}

impl std::fmt::Debug for AnalysisQueue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnalysisQueue")
            .field("models", &self.shared.models)
            .finish_non_exhaustive()
    }
}

impl AnalysisQueue {
    /// Starts `opts.workers` idle workers. `sink` receives every
    /// [`QueueEvent`] on a worker (or the calling) thread; keep it cheap.
    pub fn new(
        lib: Arc<Library>,
        opts: QueueOptions,
        sink: impl Fn(&QueueEvent) + Send + Sync + 'static,
    ) -> Result<Self, QueueError> {
        for path in [&opts.models.mel, &opts.models.beat] {
            if !path.is_file() {
                return Err(QueueError::MissingModel(path.clone()));
            }
        }
        let workers = opts.workers.max(1);
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .thread_name(|i| format!("reks-analysis-pool-{i}"))
            .build()
            .map_err(|e| QueueError::Pool(e.to_string()))?;
        let shared = Arc::new(Shared {
            lib,
            models: opts.models,
            state: Mutex::new(State::default()),
            work: Condvar::new(),
            idle: Condvar::new(),
            sink: Box::new(sink),
            pool,
        });
        for i in 0..workers {
            let shared = Arc::clone(&shared);
            std::thread::Builder::new()
                .name(format!("reks-analysis-worker-{i}"))
                .spawn(move || worker(&shared))
                .map_err(|e| QueueError::Pool(e.to_string()))?;
        }
        tracing::info!(workers, "analysis queue started");
        Ok(Self { shared })
    }

    /// Queues every track the library says needs analysis (new, changed, or
    /// analysed by another analyser version). Returns how many were added.
    pub fn enqueue_pending(&self) -> Result<usize, QueueError> {
        let jobs = self.shared.lib.analysis_pending(ANALYSER_VERSION)?;
        self.push(&jobs, false)
    }

    /// Queues `ids` for a fresh analysis that ignores the cache, ahead of
    /// everything else. Missing files are skipped. Returns how many were added.
    pub fn reanalyse(&self, ids: &[TrackId]) -> Result<usize, QueueError> {
        let jobs = self.shared.lib.analysis_jobs(ids)?;
        self.push(&jobs, true)
    }

    fn push(&self, jobs: &[AnalysisJob], force: bool) -> Result<usize, QueueError> {
        let status = {
            let mut st = self.shared.lock()?;
            let was_idle = st.status().is_idle();
            let mut added = 0;
            let mut front = Vec::new();
            for job in jobs {
                let item = Item {
                    track_id: job.track_id,
                    force,
                };
                if !force && st.running_ids.contains(&job.track_id) {
                    // Being analysed; its result is about to land.
                    continue;
                }
                if st.queued.contains(&job.track_id) {
                    if force {
                        // Already waiting: move it to the front as a forced run.
                        st.pending.retain(|i| i.track_id != job.track_id);
                        front.push(item);
                    }
                    continue;
                }
                if was_idle && added == 0 {
                    // A new batch: the counters start over.
                    st.done = 0;
                    st.failed = 0;
                    st.total = 0;
                }
                st.queued.insert(job.track_id);
                st.total += 1;
                added += 1;
                if force {
                    front.push(item);
                } else {
                    st.pending.push_back(item);
                }
            }
            for item in front.into_iter().rev() {
                st.pending.push_front(item);
            }
            if added == 0 && !force {
                return Ok(0);
            }
            self.shared.work.notify_all();
            tracing::debug!(added, force, queued = st.pending.len(), "analysis queued");
            (added, st.status())
        };
        self.shared.emit_status(status.1);
        Ok(status.0)
    }

    /// Workers finish their current track, then wait for [`Self::resume`].
    pub fn pause(&self) -> Result<QueueStatus, QueueError> {
        self.set_paused(true)
    }

    pub fn resume(&self) -> Result<QueueStatus, QueueError> {
        self.set_paused(false)
    }

    fn set_paused(&self, paused: bool) -> Result<QueueStatus, QueueError> {
        let status = {
            let mut st = self.shared.lock()?;
            st.paused = paused;
            self.shared.work.notify_all();
            st.status()
        };
        self.shared.emit_status(status);
        Ok(status)
    }

    /// Drops every queued track and discards the results of those being
    /// analysed. Pause state is kept.
    pub fn cancel(&self) -> Result<QueueStatus, QueueError> {
        let status = {
            let mut st = self.shared.lock()?;
            st.pending.clear();
            st.queued.clear();
            st.epoch += 1;
            if st.running == 0 {
                self.shared.idle.notify_all();
            }
            st.status()
        };
        tracing::info!("analysis queue cancelled");
        self.shared.emit_status(status);
        Ok(status)
    }

    pub fn status(&self) -> Result<QueueStatus, QueueError> {
        Ok(self.shared.lock()?.status())
    }

    /// Blocks until nothing is queued or running (a paused queue with work
    /// left is not idle), or `timeout` passes. True when idle.
    pub fn wait_idle(&self, timeout: Duration) -> Result<bool, QueueError> {
        let deadline = Instant::now() + timeout;
        let mut st = self.shared.lock()?;
        loop {
            if st.status().is_idle() {
                return Ok(true);
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Ok(false);
            }
            st = self
                .shared
                .idle
                .wait_timeout(st, left)
                .map_err(|_| QueueError::Poisoned)?
                .0;
        }
    }
}

impl Drop for AnalysisQueue {
    fn drop(&mut self) {
        if let Ok(mut st) = self.shared.state.lock() {
            st.shutdown = true;
            st.pending.clear();
            st.queued.clear();
            st.epoch += 1;
        }
        self.shared.work.notify_all();
    }
}

/// Outcome of one track, before it is counted.
enum Outcome {
    Analysed {
        cached: bool,
        stored: StoreOutcome,
    },
    Failed(String),
    /// Cancelled while running: not counted, not reported.
    Dropped,
}

fn worker(shared: &Shared) {
    let mut tracker: Option<BeatTracker> = None;
    loop {
        let (item, epoch) = {
            let Ok(mut st) = shared.state.lock() else {
                return;
            };
            loop {
                if st.shutdown {
                    return;
                }
                if !st.paused {
                    if let Some(item) = st.pending.pop_front() {
                        st.queued.remove(&item.track_id);
                        st.running_ids.insert(item.track_id);
                        st.running += 1;
                        break (item, st.epoch);
                    }
                }
                st = match shared.work.wait(st) {
                    Ok(st) => st,
                    Err(_) => return,
                };
            }
        };

        let outcome = run(shared, &mut tracker, item, epoch);

        let event = {
            let Ok(mut st) = shared.state.lock() else {
                return;
            };
            st.running_ids.remove(&item.track_id);
            match outcome {
                Outcome::Analysed { cached, stored } => {
                    st.done += 1;
                    Some(QueueEvent::Analysed {
                        track_id: item.track_id,
                        cached,
                        stored,
                    })
                }
                Outcome::Failed(message) => {
                    st.failed += 1;
                    Some(QueueEvent::Failed {
                        track_id: item.track_id,
                        message,
                    })
                }
                Outcome::Dropped => None,
            }
        };
        // Deliver the event before this job stops counting as running, so
        // whoever `wait_idle` wakes has seen every result. Not under the lock:
        // the sink may call back into the queue.
        if let Some(event) = event {
            (shared.sink)(&event);
        }
        let status = {
            let Ok(mut st) = shared.state.lock() else {
                return;
            };
            st.running -= 1;
            let status = st.status();
            if status.is_idle() {
                shared.idle.notify_all();
            }
            status
        };
        shared.emit_status(status);
    }
}

fn cancelled(shared: &Shared, epoch: u64) -> bool {
    shared
        .state
        .lock()
        .map(|st| st.epoch != epoch)
        .unwrap_or(true)
}

fn run(shared: &Shared, tracker: &mut Option<BeatTracker>, item: Item, epoch: u64) -> Outcome {
    let id = item.track_id;
    // The path may have changed (moved file) since the track was queued.
    let path = match shared.lib.analysis_jobs(&[id]) {
        Ok(jobs) => match jobs.into_iter().next() {
            Some(job) => PathBuf::from(job.path),
            None => return Outcome::Failed("track is missing or was removed".into()),
        },
        Err(e) => return Outcome::Failed(e.to_string()),
    };
    // Unreadable file: report, but leave no trace so it is retried later.
    let hash = match file_content_hash(&path) {
        Ok(h) => h,
        Err(e) => return Outcome::Failed(e.to_string()),
    };
    let cache_dir = shared.lib.cache_dir();

    let cached = if item.force {
        None
    } else {
        cache::load_valid(&cache_dir, id, &hash)
    };
    let (analysis, from_cache) = match cached {
        Some(a) => (a, true),
        None => {
            if tracker.is_none() {
                match BeatTracker::load(&shared.models) {
                    Ok(t) => *tracker = Some(t),
                    // Not the track's fault: nothing goes into the library.
                    Err(e) => return Outcome::Failed(e.to_string()),
                }
            }
            let Some(t) = tracker.as_mut() else {
                return Outcome::Failed("beat tracker unavailable".into());
            };
            let started = Instant::now();
            match shared.pool.install(|| analyse_file(t, &path)) {
                Ok(a) => {
                    tracing::debug!(
                        track = id,
                        ms = started.elapsed().as_millis() as u64,
                        "analysed"
                    );
                    (a, false)
                }
                Err(e) => {
                    let message = e.to_string();
                    if cancelled(shared, epoch) {
                        return Outcome::Dropped;
                    }
                    tracing::warn!(track = id, path = %path.display(), %message, "analysis failed");
                    if let Err(err) =
                        shared
                            .lib
                            .store_analysis_error(id, ANALYSER_VERSION, Some(&hash), &message)
                    {
                        tracing::warn!(track = id, %err, "cannot record analysis failure");
                    }
                    return Outcome::Failed(message);
                }
            }
        }
    };
    if cancelled(shared, epoch) {
        return Outcome::Dropped;
    }
    if !from_cache {
        if let Err(err) = cache::write(&cache_dir, id, &analysis, &hash) {
            // The library still gets the result; the next run re-analyses.
            tracing::warn!(track = id, %err, "cannot write analysis cache");
        }
    }
    match shared.lib.store_analysis(id, &analysis.record(hash)) {
        Ok(stored) => Outcome::Analysed {
            cached: from_cache,
            stored,
        },
        Err(e) => Outcome::Failed(e.to_string()),
    }
}
