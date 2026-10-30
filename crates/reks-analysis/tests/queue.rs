//! Background analysis queue (phase 3 task 11) on generated fixtures: the
//! queue completes, pause and cancel work, the cache is reused and
//! invalidated, and manual grids and keys survive re-analysis.

// Helpers outside #[test] fns unwrap too.
#![allow(clippy::unwrap_used)]

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use reks_analysis::cache;
use reks_analysis::{AnalysisQueue, ModelPaths, QueueEvent, QueueOptions, ANALYSER_VERSION};
use reks_core::DataSource;
use reks_core::{CancelToken, Library, TrackId};

const LONG: Duration = Duration::from_secs(300);

struct Fixture {
    dir: tempfile::TempDir,
    lib: Arc<Library>,
    events: Arc<Mutex<Vec<QueueEvent>>>,
}

impl Fixture {
    /// A library with `n` copies of the 128 BPM click track, scanned in.
    fn new(n: usize) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let music = dir.path().join("music");
        std::fs::create_dir_all(&music).unwrap();
        for i in 0..n {
            reks_fixtures_gen::write_wav(File::create(music.join(format!("t{i}.wav"))).unwrap())
                .unwrap();
        }
        let lib =
            Library::open(&dir.path().join("reks.sqlite"), &dir.path().join("cache")).unwrap();
        let fx = Self {
            dir,
            lib: Arc::new(lib),
            events: Arc::default(),
        };
        fx.scan();
        fx
    }

    fn music(&self) -> PathBuf {
        self.dir.path().join("music")
    }

    fn scan(&self) {
        let report =
            reks_core::scan(&[self.music()], &self.lib, &CancelToken::new(), |_| {}).unwrap();
        assert!(report.errors.is_empty(), "{:?}", report.errors);
    }

    fn queue(&self, workers: usize) -> AnalysisQueue {
        let events = Arc::clone(&self.events);
        AnalysisQueue::new(
            Arc::clone(&self.lib),
            QueueOptions {
                workers,
                models: ModelPaths::small(&ModelPaths::dev_dir()),
            },
            move |e| events.lock().unwrap().push(e.clone()),
        )
        .unwrap()
    }

    fn ids(&self) -> Vec<TrackId> {
        query(&self.lib, "SELECT id FROM track ORDER BY id")
    }

    /// `(cached)` of every Analysed event so far, by track.
    fn analysed(&self) -> Vec<(TrackId, bool)> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                QueueEvent::Analysed {
                    track_id, cached, ..
                } => Some((*track_id, *cached)),
                _ => None,
            })
            .collect()
    }

    fn clear_events(&self) {
        self.events.lock().unwrap().clear();
    }
}

/// Runs `sql` through a second connection to the fixture's DB file (WAL).
fn query(lib: &Library, sql: &str) -> Vec<i64> {
    let c = rusqlite_conn(lib);
    let mut s = c.prepare(sql).unwrap();
    s.query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn rusqlite_conn(lib: &Library) -> rusqlite::Connection {
    let db = lib.cache_dir().parent().unwrap().join("reks.sqlite");
    rusqlite::Connection::open(db).unwrap()
}

fn exec(lib: &Library, sql: &str, id: TrackId) {
    rusqlite_conn(lib).execute(sql, [id]).unwrap();
}

fn analysed_ids(lib: &Library) -> Vec<i64> {
    query(
        lib,
        &format!("SELECT id FROM track WHERE analysis_version = {ANALYSER_VERSION} AND analysis_error IS NULL ORDER BY id"),
    )
}

fn wait_until(what: &str, mut f: impl FnMut() -> bool) {
    let deadline = Instant::now() + LONG;
    while !f() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn queue_analyses_everything_and_caches_it() {
    let fx = Fixture::new(3);
    let q = fx.queue(2);
    assert_eq!(q.enqueue_pending().unwrap(), 3);
    // Queuing again while they wait adds nothing.
    assert_eq!(q.enqueue_pending().unwrap(), 0);
    assert!(q.wait_idle(LONG).unwrap());

    let ids = fx.ids();
    assert_eq!(analysed_ids(&fx.lib), ids);
    let status = q.status().unwrap();
    assert_eq!((status.done, status.failed, status.total), (3, 0, 3));
    let mut got = fx.analysed();
    got.sort_unstable();
    assert_eq!(got, ids.iter().map(|&i| (i, false)).collect::<Vec<_>>());

    for &id in &ids {
        let d = fx.lib.track(id).unwrap();
        assert_eq!(d.track.row.bpm_x100, Some(12_800));
        assert_eq!(d.track.bpm_source, Some(DataSource::Analysis));
        let grid = d.beatgrid.unwrap();
        assert_eq!((grid.markers, grid.bpm_min_x100), (1, 12_800));
        // On a click (every 468.75 ms from sample 0); the clicks have no
        // accents, so which one starts the grid depends on the bar.
        let off = grid.first_beat_ms.rem_euclid(468.75);
        assert!(off.min(468.75 - off) < 10.0, "{}", grid.first_beat_ms);
        assert!(!d.track.row.needs_analysis);
        let entry = cache::entry_dir(&fx.lib.cache_dir(), id);
        assert!(entry.join("analysis.json").is_file());
        assert!(entry.join("bands.bin").is_file());
    }
    // Nothing left to do.
    assert_eq!(q.enqueue_pending().unwrap(), 0);
}

#[test]
fn cache_is_reused_until_the_file_changes() {
    let fx = Fixture::new(2);
    let q = fx.queue(2);
    q.enqueue_pending().unwrap();
    assert!(q.wait_idle(LONG).unwrap());
    let ids = fx.ids();

    // A new analyser version would re-queue everything; the cache entries
    // are still valid for this version and these files, so they are reused.
    rusqlite_conn(&fx.lib)
        .execute("UPDATE track SET analysis_version = NULL", [])
        .unwrap();
    fx.clear_events();
    assert_eq!(q.enqueue_pending().unwrap(), 2);
    assert!(q.wait_idle(LONG).unwrap());
    let mut got = fx.analysed();
    got.sort_unstable();
    assert_eq!(got, ids.iter().map(|&i| (i, true)).collect::<Vec<_>>());

    // Change one file: the scan stores its new hash, the track is pending
    // again and its stale cache entry is not used.
    let path = fx.music().join("t0.wav");
    let bars = reks_fixtures_gen::bars::render_bars(44_100, 8);
    let mut w = hound::WavWriter::create(&path, reks_fixtures_gen::wav_spec()).unwrap();
    for s in bars {
        w.write_sample((s * 32_000.0) as i16).unwrap();
    }
    w.finalize().unwrap();
    fx.scan();
    fx.clear_events();
    assert_eq!(q.enqueue_pending().unwrap(), 1);
    assert!(q.wait_idle(LONG).unwrap());
    assert_eq!(fx.analysed(), [(ids[0], false)]);
    let fresh = cache::load_valid(
        &fx.lib.cache_dir(),
        ids[0],
        &reks_core::analysis::file_content_hash(&path).unwrap(),
    )
    .unwrap();
    assert_eq!(
        fresh.columns.len(),
        (fresh.duration_ms as usize * 150).div_ceil(1000)
    );

    // Re-analyse ignores a valid cache entry.
    fx.clear_events();
    assert_eq!(q.reanalyse(&[ids[1]]).unwrap(), 1);
    assert!(q.wait_idle(LONG).unwrap());
    assert_eq!(fx.analysed(), [(ids[1], false)]);
}

#[test]
fn pause_holds_work_and_resume_finishes_it() {
    let fx = Fixture::new(2);
    let q = fx.queue(2);
    q.pause().unwrap();
    assert_eq!(q.enqueue_pending().unwrap(), 2);
    assert!(!q.wait_idle(Duration::from_millis(300)).unwrap());
    let s = q.status().unwrap();
    assert!(s.paused);
    assert_eq!((s.queued, s.running, s.done), (2, 0, 0));
    assert!(analysed_ids(&fx.lib).is_empty());

    q.resume().unwrap();
    assert!(q.wait_idle(LONG).unwrap());
    assert_eq!(analysed_ids(&fx.lib), fx.ids());
}

#[test]
fn cancel_drops_queued_and_running_tracks() {
    let fx = Fixture::new(3);
    let q = fx.queue(1);
    // Queued only.
    q.pause().unwrap();
    q.enqueue_pending().unwrap();
    let s = q.cancel().unwrap();
    assert_eq!((s.queued, s.running), (0, 0));
    q.resume().unwrap();
    assert!(q.wait_idle(LONG).unwrap());
    assert!(analysed_ids(&fx.lib).is_empty());

    // One running, two queued: the running result is discarded too.
    q.enqueue_pending().unwrap();
    wait_until("a running track", || q.status().unwrap().running == 1);
    q.cancel().unwrap();
    assert!(q.wait_idle(LONG).unwrap());
    assert!(analysed_ids(&fx.lib).is_empty());
    assert!(fx.analysed().is_empty());
    let s = q.status().unwrap();
    assert_eq!((s.done, s.failed, s.queued, s.running), (0, 0, 0, 0));

    // The queue still works afterwards.
    assert_eq!(q.enqueue_pending().unwrap(), 3);
    assert!(q.wait_idle(LONG).unwrap());
    assert_eq!(analysed_ids(&fx.lib), fx.ids());
}

#[test]
fn manual_grid_and_key_survive_reanalysis() {
    let fx = Fixture::new(1);
    let id = fx.ids()[0];
    // What the grid editor writes (task 13).
    exec(
        &fx.lib,
        "INSERT INTO beatgrid (track_id, position_ms, bpm_x100, beat_in_bar, source)
         VALUES (?1, 123.0, 12650, 2, 'manual')",
        id,
    );
    exec(
        &fx.lib,
        "UPDATE track SET key = 'Dm', key_source = 'manual',
                          bpm_x100 = 12650, bpm_source = 'manual' WHERE id = ?1",
        id,
    );
    let q = fx.queue(1);
    for run in 0..2 {
        if run == 0 {
            q.enqueue_pending().unwrap();
        } else {
            q.reanalyse(&[id]).unwrap();
        }
        assert!(q.wait_idle(LONG).unwrap());
        let d = fx.lib.track(id).unwrap();
        assert_eq!(d.track.row.key.as_deref(), Some("Dm"));
        assert_eq!(d.track.key_source, Some(DataSource::Manual));
        assert_eq!(d.track.row.bpm_x100, Some(12_650));
        let grid = d.beatgrid.unwrap();
        assert_eq!(
            (grid.markers, grid.first_beat_ms, grid.bpm_min_x100),
            (1, 123.0, 12_650)
        );
        let sources = query(
            &fx.lib,
            "SELECT count(*) FROM beatgrid WHERE source <> 'manual'",
        );
        assert_eq!(sources, [0]);
    }
    // The run is recorded; reks's own grid is in the cache.
    assert_eq!(analysed_ids(&fx.lib), [id]);
    assert!(cache::entry_dir(&fx.lib.cache_dir(), id)
        .join("analysis.json")
        .is_file());
}

#[test]
fn unreadable_file_fails_without_a_trace_and_garbage_is_recorded() {
    let fx = Fixture::new(2);
    let ids = fx.ids();
    // Gone without a rescan (unplugged drive): retried later.
    std::fs::remove_file(fx.music().join("t0.wav")).unwrap();
    // Reads, but is not audio: recorded, not retried.
    std::fs::write(fx.music().join("t1.wav"), b"not a wav file at all").unwrap();
    let q = fx.queue(2);
    q.enqueue_pending().unwrap();
    assert!(q.wait_idle(LONG).unwrap());
    assert_eq!(q.status().unwrap().failed, 2);
    let pending: Vec<i64> = fx
        .lib
        .analysis_pending(ANALYSER_VERSION)
        .unwrap()
        .into_iter()
        .map(|j| j.track_id)
        .collect();
    assert_eq!(pending, [ids[0]]);
    let errors = query(
        &fx.lib,
        "SELECT id FROM track WHERE analysis_error IS NOT NULL",
    );
    assert_eq!(errors, [ids[1]]);
}

#[test]
fn missing_model_is_an_error() {
    let fx = Fixture::new(0);
    let err = AnalysisQueue::new(
        Arc::clone(&fx.lib),
        QueueOptions::new(Path::new("/nonexistent")),
        |_| {},
    )
    .unwrap_err();
    assert!(err.to_string().contains("missing"), "{err}");
}

#[test]
fn a_running_track_is_not_queued_again() {
    let fx = Fixture::new(2);
    let q = fx.queue(1);
    assert_eq!(q.enqueue_pending().unwrap(), 2);
    wait_until("a running track", || q.status().unwrap().running == 1);
    // Both are still pending in the library; one is running, one queued.
    assert_eq!(q.enqueue_pending().unwrap(), 0);
    assert!(q.wait_idle(LONG).unwrap());
    assert_eq!(fx.analysed().len(), 2);
    assert_eq!(q.status().unwrap().total, 2);
}
