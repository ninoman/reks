//! `reks parity`: the analysis parity harness (phase 3 task 12, method in
//! `docs/analysis-parity.md`).
//!
//! ```text
//! reks parity run    [--xml <collection.xml>] [--out <dir>] [--jobs N] [--every N] [--retry-failed]
//!                    [--limit N] [--only mp3,m4a,...] [--remap FROM=TO]... [--models <dir>]
//! reks parity report [--xml <collection.xml>] [--out <dir>] [--md <file>] [--tracks <file.tsv>]
//!                    [--min-coverage <0..1>] [--placement <name>] [--analyser N]
//!                    [--change-beats N] [--change-ratio x] [--kick-phase <ms>]
//!                    [--kick-gain x]
//! reks parity keyfinder --driver <bin> [run's flags]   (see `parity/keyfinder.rs`)
//! ```
//!
//! `--xml` and `--out` default to `$REKS_PARITY_XML` and `$REKS_PARITY_OUT`.
//!
//! `run` analyses every track of a rekordbox `collection.xml` whose file
//! exists, with the app's pipeline (decode, Beat This!, key, waveform
//! columns) on `--jobs` workers sharing one rayon pool, as the background
//! queue does. Each result is appended to `<out>/results.jsonl` (raw tracker
//! beats, key, decoder delay, timings), keyed by the XML `Location`, size and
//! mtime, so an interrupted run resumes and a re-run only analyses what
//! changed. `<out>/runs.jsonl` gets one line per run for the throughput
//! figure. Both files hold per-track data of a private library: keep `<out>`
//! outside the repository.
//!
//! `report` joins the results with the XML and prints aggregates only (the
//! markdown published in `docs/analysis-parity.md`); `--tracks` writes the
//! per-track rows to a local TSV for inspecting failures. The grid is rebuilt
//! from the stored raw beats with [`reks_analysis::grid::build_grid`] (what
//! the pipeline does), so grid-builder tuning is re-measured without
//! decoding again (`--min-coverage` overrides [`GridOptions::min_coverage`]
//! for a what-if); so is beat placement, from every placement of each beat
//! stored by `run` (`--placement`, one of [`BeatPlacement::name`]). `run`
//! also writes each track's low-band onset curve to `<out>/kick/` for
//! tuning the grid on the kick. Key changes need a new `run`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Instant, UNIX_EPOCH};

use anyhow::{bail, Context};
use reks_analysis::grid::{self, build_grid_with_pulse, Grid, GridOptions, TrackedBeat};
use reks_analysis::onset::{KickOnsets, KickPulse, PULSE_RATE};
use reks_analysis::{
    analyse_tracked, beat_candidates, place_beats, AnalysisInput, Beat, BeatCandidates,
    BeatPlacement, BeatTracker, ModelPaths, ANALYSER_VERSION,
};
use reks_analysis::{queue::default_workers, ANALYSIS_RATE};
use reks_core::rbxml::{self, XmlTrack};
use reks_core::Key;
use serde::{Deserialize, Serialize};

mod keyfinder;

const RESULTS: &str = "results.jsonl";
const RUNS: &str = "runs.jsonl";
const KICK_DIR: &str = "kick";

/// Acceptance thresholds (phase 3 "Acceptance criteria").
const BPM_TOLERANCE: f64 = 0.05;
const DOWNBEAT_TOLERANCE_MS: f64 = 30.0;
/// Offset estimate: a track needs this many raw beats near rekordbox's grid.
const MIN_OFFSET_BEATS: usize = 32;

pub fn run(args: &[String]) -> anyhow::Result<()> {
    match args {
        [cmd, rest @ ..] if cmd == "run" => analyse(&Flags::parse(rest)?),
        [cmd, rest @ ..] if cmd == "report" => report(&Flags::parse(rest)?),
        [cmd, rest @ ..] if cmd == "keyfinder" => keyfinder::run(&Flags::parse(rest)?),
        _ => bail!(
            "usage: reks parity run|report|keyfinder [flags] (see crates/reks-cli/src/parity.rs)"
        ),
    }
}

#[derive(Debug, Default)]
struct Flags {
    xml: Option<PathBuf>,
    out: Option<PathBuf>,
    jobs: Option<usize>,
    every: usize,
    /// `run`: analyse again tracks whose last result is an error.
    retry_failed: bool,
    limit: Option<usize>,
    only: Option<Vec<String>>,
    remap: Vec<(PathBuf, PathBuf)>,
    models: Option<PathBuf>,
    md: Option<PathBuf>,
    tracks: Option<PathBuf>,
    grid: GridOptions,
    /// `report`: rebuild beats with this placement (what-if).
    placement: Option<BeatPlacement>,
    /// `report`: the results of this analyser version (default: current).
    analyser: Option<u32>,
    /// `keyfinder`: the C++ libKeyFinder driver (`fixtures/keyfinder`).
    driver: Option<PathBuf>,
}

impl Flags {
    fn parse(args: &[String]) -> anyhow::Result<Self> {
        let mut f = Flags {
            every: 1,
            ..Flags::default()
        };
        let mut it = args.iter();
        while let Some(flag) = it.next() {
            let mut value = || it.next().with_context(|| format!("{flag} needs a value"));
            match flag.as_str() {
                "--xml" => f.xml = Some(value()?.into()),
                "--out" => f.out = Some(value()?.into()),
                "--jobs" => f.jobs = Some(value()?.parse().context("--jobs")?),
                "--retry-failed" => f.retry_failed = true,
                "--every" => f.every = value()?.parse::<usize>().context("--every")?.max(1),
                "--limit" => f.limit = Some(value()?.parse().context("--limit")?),
                "--only" => {
                    f.only = Some(
                        value()?
                            .split(',')
                            .map(|s| s.trim().to_ascii_lowercase())
                            .collect(),
                    )
                }
                "--remap" => {
                    let (from, to) = value()?.split_once('=').context("--remap needs FROM=TO")?;
                    f.remap.push((from.into(), to.into()));
                }
                "--models" => f.models = Some(value()?.into()),
                "--md" => f.md = Some(value()?.into()),
                "--tracks" => f.tracks = Some(value()?.into()),
                "--min-coverage" => {
                    f.grid.min_coverage = value()?.parse().context("--min-coverage")?
                }
                "--placement" => {
                    let name = value()?;
                    f.placement = Some(
                        BeatPlacement::ALL
                            .into_iter()
                            .find(|p| p.name() == name)
                            .with_context(|| format!("unknown placement {name}"))?,
                    )
                }
                "--change-beats" => {
                    f.grid.change_beats = value()?.parse().context("--change-beats")?
                }
                "--change-ratio" => {
                    f.grid.change_ratio = value()?.parse().context("--change-ratio")?
                }
                "--kick-phase" => {
                    f.grid.kick_phase_ms = value()?.parse().context("--kick-phase")?
                }
                "--kick-gain" => {
                    f.grid.kick_phase_gain = value()?.parse().context("--kick-gain")?
                }
                "--analyser" => f.analyser = Some(value()?.parse().context("--analyser")?),
                "--driver" => f.driver = Some(value()?.into()),
                other => bail!("unknown flag {other}"),
            }
        }
        Ok(f)
    }

    fn xml(&self) -> anyhow::Result<PathBuf> {
        self.xml
            .clone()
            .or_else(|| std::env::var_os("REKS_PARITY_XML").map(PathBuf::from))
            .context("give --xml <collection.xml> or set REKS_PARITY_XML")
    }

    fn out(&self) -> anyhow::Result<PathBuf> {
        self.out
            .clone()
            .or_else(|| std::env::var_os("REKS_PARITY_OUT").map(PathBuf::from))
            .context("give --out <dir> or set REKS_PARITY_OUT")
    }

    fn path_of(&self, t: &XmlTrack) -> Option<PathBuf> {
        let mut p = rbxml::location_to_path(t.location.as_deref()?).ok()?;
        for (from, to) in &self.remap {
            if let Ok(rest) = p.strip_prefix(from) {
                p = to.join(rest);
                break;
            }
        }
        Some(p)
    }
}

/// One analysed track, one line of `results.jsonl`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Record {
    /// XML `Location`: the join key.
    location: String,
    size: u64,
    mtime: i64,
    analyser: u32,
    /// Lower-case file extension.
    ext: String,
    #[serde(default)]
    source_rate: u32,
    #[serde(default)]
    encoder_delay: Option<u32>,
    #[serde(default)]
    encoder_padding: Option<u32>,
    /// `reks_audio::timing` rule and the silence it put first.
    #[serde(default)]
    timeline_rule: String,
    #[serde(default)]
    lead_ms: f64,
    #[serde(default)]
    duration_ms: u64,
    /// Wall time of decode + conditioning, and of the analysers.
    #[serde(default)]
    decode_ms: u64,
    #[serde(default)]
    analyse_ms: u64,
    #[serde(default)]
    beat_confidence: f32,
    /// Tracker beats placed with `BeatPlacement::DEFAULT`, samples at
    /// [`ANALYSIS_RATE`] on rekordbox's timeline (the decoder's lead
    /// included).
    #[serde(default)]
    beats: Vec<i64>,
    /// Every placement of each beat (`reks_analysis::BeatCandidates`), so
    /// beat placement is re-measured without decoding (`--placement`):
    /// the model's beat, the broadband onset, the low-band kick (−1: none)
    /// with its rise in whole dB, and the kick's attack (−1: none).
    #[serde(default)]
    model_beats: Vec<i64>,
    #[serde(default)]
    onset_beats: Vec<i64>,
    #[serde(default)]
    kick_beats: Vec<i64>,
    #[serde(default)]
    kick_db: Vec<u8>,
    #[serde(default)]
    kick_attack_beats: Vec<i64>,
    /// Indices into `model_beats` (and `beats` for records without them)
    /// the tracker marked as downbeats.
    #[serde(default)]
    downbeats: Vec<u32>,
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    key_confidence: f32,
    /// Octave-folded chroma (index 0 = C), for re-fitting key profiles
    /// without decoding.
    #[serde(default)]
    chroma: Vec<f32>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RunRecord {
    started: String,
    analyser: u32,
    jobs: usize,
    cores: usize,
    tracks: usize,
    failed: usize,
    audio_ms: u64,
    wall_ms: u64,
    /// Process CPU time (user + system) over the run; robust to other load
    /// on the machine, unlike `wall_ms`.
    #[serde(default)]
    cpu_ms: Option<u64>,
}

/// CPU time this process has used so far (user + system), in ms.
#[cfg(unix)]
fn process_cpu_ms() -> Option<u64> {
    // SAFETY: getrusage only writes the struct it is given.
    let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut ru) } != 0 {
        return None;
    }
    let ms = |t: libc::timeval| t.tv_sec as u64 * 1000 + t.tv_usec as u64 / 1000;
    Some(ms(ru.ru_utime) + ms(ru.ru_stime))
}

#[cfg(not(unix))]
fn process_cpu_ms() -> Option<u64> {
    None
}

fn file_stamp(path: &Path) -> Option<(u64, i64)> {
    let meta = fs::metadata(path).ok()?;
    let mtime = meta
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_secs() as i64;
    Some((meta.len(), mtime))
}

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn load_results(out: &Path) -> anyhow::Result<HashMap<String, Record>> {
    let path = out.join(RESULTS);
    let mut map = HashMap::new();
    let Ok(f) = File::open(&path) else {
        return Ok(map);
    };
    for (i, line) in BufReader::new(f).lines().enumerate() {
        let line = line.with_context(|| format!("reading {}", path.display()))?;
        if line.trim().is_empty() {
            continue;
        }
        // A line cut short by an interrupted run is skipped and redone.
        match serde_json::from_str::<Record>(&line) {
            Ok(r) => {
                map.insert(r.location.clone(), r);
            }
            Err(e) => eprintln!("{}:{}: skipped: {e}", path.display(), i + 1),
        }
    }
    Ok(map)
}

fn analyse(flags: &Flags) -> anyhow::Result<()> {
    let xml = flags.xml()?;
    let out = flags.out()?;
    fs::create_dir_all(out.join(KICK_DIR))
        .with_context(|| format!("creating {}", out.display()))?;
    let collection = rbxml::parse_file(&xml)?;
    let done = load_results(&out)?;

    let mut todo: Vec<(String, PathBuf, u64, i64)> = Vec::new();
    let mut skipped = 0usize;
    for (i, t) in collection.tracks.iter().enumerate() {
        if i % flags.every != 0 {
            continue;
        }
        let (Some(loc), Some(path)) = (t.location.clone(), flags.path_of(t)) else {
            continue;
        };
        if let Some(only) = &flags.only {
            if !only.contains(&extension(&path)) {
                continue;
            }
        }
        let Some((size, mtime)) = file_stamp(&path) else {
            continue;
        };
        let fresh = done.get(&loc).is_some_and(|r| {
            r.size == size
                && r.mtime == mtime
                && r.analyser == ANALYSER_VERSION
                && !(flags.retry_failed && r.error.is_some())
        });
        if fresh {
            skipped += 1;
            continue;
        }
        todo.push((loc, path, size, mtime));
    }
    if let Some(limit) = flags.limit {
        todo.truncate(limit);
    }
    let jobs = flags.jobs.unwrap_or_else(default_workers).max(1);
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    eprintln!(
        "{} to analyse ({skipped} up to date) on {jobs} workers ({cores} cores)",
        todo.len()
    );
    if todo.is_empty() {
        return Ok(());
    }

    let models = ModelPaths::small(&flags.models.clone().unwrap_or_else(ModelPaths::dev_dir));
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(jobs)
        .build()
        .context("starting the analysis pool")?;
    let writer = Mutex::new(
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(out.join(RESULTS))
            .context("opening results.jsonl")?,
    );
    let next = AtomicUsize::new(0);
    let finished = AtomicUsize::new(0);
    let failed = AtomicUsize::new(0);
    let audio_ms = AtomicUsize::new(0);
    let started = Instant::now();
    let started_at = reks_core::timefmt::now_utc();
    let cpu_before = process_cpu_ms();

    std::thread::scope(|s| -> anyhow::Result<()> {
        let mut handles = Vec::new();
        for _ in 0..jobs {
            handles.push(s.spawn(|| -> anyhow::Result<()> {
                let mut tracker = BeatTracker::load(&models)?;
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some((loc, path, size, mtime)) = todo.get(i) else {
                        return Ok(());
                    };
                    let rec =
                        pool.install(|| analyse_one(&mut tracker, &out, loc, path, *size, *mtime));
                    if rec.error.is_some() {
                        failed.fetch_add(1, Ordering::Relaxed);
                    }
                    audio_ms.fetch_add(rec.duration_ms as usize, Ordering::Relaxed);
                    let line = serde_json::to_string(&rec)?;
                    {
                        let mut w = writer
                            .lock()
                            .map_err(|_| anyhow::anyhow!("writer poisoned"))?;
                        writeln!(w, "{line}")?;
                    }
                    let n = finished.fetch_add(1, Ordering::Relaxed) + 1;
                    if n.is_multiple_of(25) || n == todo.len() {
                        let secs = started.elapsed().as_secs_f64();
                        let audio = audio_ms.load(Ordering::Relaxed) as f64 / 1000.0;
                        eprintln!(
                            "{n}/{} done, {} failed, {:.0} s, {:.1}× real time per worker",
                            todo.len(),
                            failed.load(Ordering::Relaxed),
                            secs,
                            audio / secs / jobs as f64
                        );
                    }
                }
            }));
        }
        for h in handles {
            h.join().map_err(|_| anyhow::anyhow!("worker panicked"))??;
        }
        Ok(())
    })?;

    let run = RunRecord {
        started: started_at,
        analyser: ANALYSER_VERSION,
        jobs,
        cores,
        tracks: finished.load(Ordering::Relaxed),
        failed: failed.load(Ordering::Relaxed),
        audio_ms: audio_ms.load(Ordering::Relaxed) as u64,
        wall_ms: started.elapsed().as_millis() as u64,
        cpu_ms: process_cpu_ms()
            .zip(cpu_before)
            .map(|(after, before)| after.saturating_sub(before)),
    };
    let mut runs = OpenOptions::new()
        .create(true)
        .append(true)
        .open(out.join(RUNS))?;
    writeln!(runs, "{}", serde_json::to_string(&run)?)?;
    eprintln!(
        "{} tracks, {} failed, {:.1}× real time per worker, {}",
        run.tracks,
        run.failed,
        run.audio_ms as f64 / run.wall_ms.max(1) as f64 / jobs as f64,
        run.cpu_ms.map_or("no CPU time".to_owned(), |c| format!(
            "{:.1}× real time per CPU second",
            run.audio_ms as f64 / c.max(1) as f64
        ))
    );
    Ok(())
}

/// Sidecar with a track's low-band onset curve: a [`KickPulse`] in quarter
/// dB, one byte per frame.
fn kick_file(out: &Path, location: &str) -> PathBuf {
    // FNV-1a: a stable file name per location.
    let hash = location.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    out.join(KICK_DIR).join(format!("{hash:016x}.u8"))
}

fn analyse_one(
    tracker: &mut BeatTracker,
    out: &Path,
    loc: &str,
    path: &Path,
    size: u64,
    mtime: i64,
) -> Record {
    let mut rec = Record {
        location: loc.to_owned(),
        size,
        mtime,
        analyser: ANALYSER_VERSION,
        ext: extension(path),
        source_rate: 0,
        encoder_delay: None,
        encoder_padding: None,
        timeline_rule: String::new(),
        lead_ms: 0.0,
        duration_ms: 0,
        decode_ms: 0,
        analyse_ms: 0,
        beat_confidence: 0.0,
        beats: Vec::new(),
        model_beats: Vec::new(),
        onset_beats: Vec::new(),
        kick_beats: Vec::new(),
        kick_db: Vec::new(),
        kick_attack_beats: Vec::new(),
        downbeats: Vec::new(),
        key: None,
        key_confidence: 0.0,
        chroma: Vec::new(),
        error: None,
    };
    let t0 = Instant::now();
    let input = match AnalysisInput::from_file(path) {
        Ok(i) => i,
        Err(e) => {
            rec.error = Some(e.to_string());
            return rec;
        }
    };
    rec.decode_ms = t0.elapsed().as_millis() as u64;
    rec.source_rate = input.source_rate;
    rec.encoder_delay = input.encoder_delay;
    rec.encoder_padding = input.encoder_padding;
    rec.timeline_rule = input.timeline.rule.label().to_owned();
    rec.lead_ms = input.timeline.lead_ms(input.source_rate);
    rec.duration_ms = input.duration_ms();
    let t1 = Instant::now();
    let tracked = tracker.track_model(&input).map(|mut list| {
        let kicks = KickOnsets::from_mono(&input.mono);
        let candidates: Vec<BeatCandidates> = list
            .beats
            .iter()
            .map(|b| beat_candidates(&input.mono, &kicks, b.sample))
            .collect();
        rec.downbeats = list
            .beats
            .iter()
            .enumerate()
            .filter(|(_, b)| b.downbeat)
            .map(|(i, _)| i as u32)
            .collect();
        place_beats(&mut list.beats, &candidates, BeatPlacement::DEFAULT);
        rec.model_beats = candidates.iter().map(|c| c.model).collect();
        rec.onset_beats = candidates.iter().map(|c| c.onset).collect();
        rec.kick_beats = candidates
            .iter()
            .map(|c| c.kick.map_or(-1, |k| k.0))
            .collect();
        rec.kick_db = candidates
            .iter()
            .map(|c| c.kick.map_or(0, |k| k.1.round().clamp(1.0, 255.0) as u8))
            .collect();
        rec.kick_attack_beats = candidates
            .iter()
            .map(|c| c.kick_onset.unwrap_or(-1))
            .collect();
        let curve = KickPulse::quantise(&kicks.downsample(PULSE_RATE));
        let pulse = KickPulse::from_quarter_db(curve.clone());
        if let Err(e) = fs::write(kick_file(out, loc), curve) {
            eprintln!("{loc}: cannot write the kick curve: {e}");
        }
        (list, pulse)
    });
    match tracked
        .map_err(reks_analysis::AnalyseError::from)
        .and_then(|(list, pulse)| analyse_tracked(&input, list, Some(&pulse)))
    {
        Ok(a) => {
            rec.analyse_ms = t1.elapsed().as_millis() as u64;
            rec.beat_confidence = a.beats.confidence;
            rec.beats = a.beats.beats.iter().map(|b| b.sample).collect();
            rec.key = a.key.key.map(|k| k.canonical().to_owned());
            rec.key_confidence = a.key.confidence;
            rec.chroma = a.key.chroma.to_vec();
        }
        Err(e) => rec.error = Some(e.to_string()),
    }
    rec
}

// ---------------------------------------------------------------------------
// rekordbox's grid

/// One `TEMPO` anchor, in ms.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Anchor {
    start_ms: f64,
    bpm: f64,
    /// Beat number of the anchor beat, 1–4.
    beat: u8,
}

/// rekordbox's grid from the XML: each anchor's tempo holds until the next
/// anchor; the first one also extends backwards.
#[derive(Debug, Clone, PartialEq)]
struct RbGrid {
    anchors: Vec<Anchor>,
}

impl RbGrid {
    fn from_track(t: &XmlTrack) -> Option<Self> {
        let mut anchors: Vec<Anchor> = t
            .tempos
            .iter()
            .filter(|a| a.bpm > 0.0)
            .map(|a| Anchor {
                start_ms: a.inizio * 1000.0,
                bpm: a.bpm,
                beat: a.battito.filter(|b| (1..=4).contains(b)).unwrap_or(1),
            })
            .collect();
        anchors.sort_by(|a, b| a.start_ms.total_cmp(&b.start_ms));
        (!anchors.is_empty()).then_some(Self { anchors })
    }

    /// The tempo when every anchor has the same one (re-phased anchors from
    /// grid edits keep it constant), else `None`.
    fn constant_bpm(&self) -> Option<f64> {
        let first = self.anchors[0].bpm;
        self.anchors
            .iter()
            .all(|a| (a.bpm - first).abs() < 0.005)
            .then_some(first)
    }

    /// Grid beats within `window_ms` of `t_ms`: `(time, beat number)`.
    fn beats_near(&self, t_ms: f64, window_ms: f64) -> Vec<(f64, u8)> {
        let mut out = Vec::new();
        for (i, a) in self.anchors.iter().enumerate() {
            let len = 60_000.0 / a.bpm;
            let end = self
                .anchors
                .get(i + 1)
                .map_or(f64::INFINITY, |n| n.start_ms);
            let lo = (t_ms - window_ms).max(if i == 0 {
                f64::NEG_INFINITY
            } else {
                a.start_ms
            });
            let hi = (t_ms + window_ms).min(end);
            if lo > hi {
                continue;
            }
            let k0 = ((lo - a.start_ms) / len).ceil() as i64;
            let k1 = ((hi - a.start_ms) / len).floor() as i64;
            for k in k0..=k1 {
                let time = a.start_ms + k as f64 * len;
                if time >= end {
                    break;
                }
                let beat = (i64::from(a.beat) - 1 + k).rem_euclid(4) as u8 + 1;
                out.push((time, beat));
            }
        }
        out
    }

    fn window(&self) -> f64 {
        // Two bars at the slowest tempo always hold a downbeat.
        let slowest = self
            .anchors
            .iter()
            .map(|a| a.bpm)
            .fold(f64::INFINITY, f64::min);
        8.0 * 60_000.0 / slowest
    }

    /// Signed distance from `t_ms` to the nearest grid beat, and its number.
    fn nearest_beat(&self, t_ms: f64) -> Option<(f64, u8)> {
        self.beats_near(t_ms, self.window())
            .into_iter()
            .map(|(time, beat)| (time - t_ms, beat))
            .min_by(|a, b| a.0.abs().total_cmp(&b.0.abs()))
    }

    /// Signed distance from `t_ms` to the nearest grid downbeat.
    fn nearest_downbeat(&self, t_ms: f64) -> Option<f64> {
        self.beats_near(t_ms, self.window())
            .into_iter()
            .filter(|&(_, beat)| beat == 1)
            .map(|(time, _)| time - t_ms)
            .min_by(|a, b| a.abs().total_cmp(&b.abs()))
    }
}

// ---------------------------------------------------------------------------
// Per-track comparison

/// How our BPM relates to rekordbox's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum BpmClass {
    /// |Δ| ≤ 0.05.
    Match,
    /// |Δ| ≤ 0.5: same tempo, different rounding.
    Close,
    /// Double or half.
    Octave,
    /// 3/2 or 2/3.
    Third,
    Other,
    /// We found no grid.
    NoGrid,
}

impl BpmClass {
    fn of(ours: f64, rb: f64) -> Self {
        let near = |r: f64, tol: f64| (ours - rb * r).abs() <= tol * r.max(1.0);
        if near(1.0, BPM_TOLERANCE) {
            Self::Match
        } else if near(1.0, 0.5) {
            Self::Close
        } else if near(2.0, 0.1) || near(0.5, 0.1) {
            Self::Octave
        } else if near(1.5, 0.1) || near(2.0 / 3.0, 0.1) {
            Self::Third
        } else {
            Self::Other
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Match => "match (|Δ| ≤ 0.05)",
            Self::Close => "close (|Δ| ≤ 0.5)",
            Self::Octave => "double/half",
            Self::Third => "3/2 or 2/3",
            Self::Other => "other",
            Self::NoGrid => "no grid",
        }
    }
}

/// How our key relates to rekordbox's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum KeyClass {
    Exact,
    /// Relative major/minor (same Camelot number).
    Relative,
    /// One step round the wheel, same mode (a fifth).
    Neighbour,
    /// Same tonic, other mode.
    Parallel,
    Other,
    /// We found no key (silence).
    None,
}

impl KeyClass {
    fn of(ours: Option<Key>, rb: Key) -> Self {
        let Some(o) = ours else {
            return Self::None;
        };
        let step = (i32::from(o.camelot_number()) - i32::from(rb.camelot_number())).rem_euclid(12);
        if o == rb {
            Self::Exact
        } else if o.camelot_number() == rb.camelot_number() {
            Self::Relative
        } else if o.mode() == rb.mode() && (step == 1 || step == 11) {
            Self::Neighbour
        } else if o.tonic() == rb.tonic() {
            Self::Parallel
        } else {
            Self::Other
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Relative => "relative",
            Self::Neighbour => "neighbour (±1 Camelot)",
            Self::Parallel => "parallel",
            Self::Other => "other",
            Self::None => "none detected",
        }
    }
}

/// Codec group for the offset table: the timing rule for MP3, else the
/// XML `Kind` (which tells AAC from ALAC in `.m4a`).
fn codec_group(t: &XmlTrack, rec: &Record) -> String {
    if rec.ext == "mp3" {
        format!("MP3: {}", rec.timeline_rule)
    } else {
        match t.kind.as_deref() {
            Some("WAV File" | "AIFF File") => "WAV/AIFF".to_owned(),
            Some(kind) => kind.trim_end_matches(" File").to_owned(),
            None => rec.ext.to_uppercase(),
        }
    }
}

#[derive(Debug, Clone)]
struct Row {
    genre: &'static str,
    codec: String,
    /// Silence the decoder put first (ms).
    lead_ms: f64,
    rb_bpm: f64,
    /// rekordbox grid has more than one `TEMPO` anchor: mostly hand-edited.
    rb_edited: bool,
    ours_bpm: Option<f64>,
    /// Our grid fell back to the tracker's beats (no constant tempo fits).
    dynamic: bool,
    bpm: BpmClass,
    /// Signed ms from our first downbeat to rekordbox's nearest downbeat /
    /// beat, and that beat's number.
    downbeat_ms: Option<f64>,
    beat_ms: Option<(f64, u8)>,
    key: Option<KeyClass>,
    /// rekordbox − reks raw beat times, median (residual after the lead).
    raw_offset_ms: Option<f64>,
}

impl Row {
    fn downbeat_ok(&self) -> bool {
        self.downbeat_ms
            .is_some_and(|d| d.abs() <= DOWNBEAT_TOLERANCE_MS)
    }
}

fn ms(samples: f64) -> f64 {
    samples * 1000.0 / f64::from(ANALYSIS_RATE)
}

/// Our first downbeat, in samples.
fn first_downbeat(g: &Grid) -> Option<f64> {
    match g {
        Grid::Constant {
            bpm,
            first_beat,
            first_beat_in_bar,
        } => {
            let len = 60.0 * f64::from(ANALYSIS_RATE) / bpm;
            let ahead = (5 - i64::from(*first_beat_in_bar)).rem_euclid(4);
            Some(*first_beat as f64 + ahead as f64 * len)
        }
        Grid::Dynamic { beats } => beats
            .iter()
            .find(|b| b.beat_in_bar == 1)
            .map(|b| b.position as f64),
    }
}

fn grid_bpm(g: &Grid) -> Option<f64> {
    match g {
        Grid::Constant { bpm, .. } => Some(*bpm),
        Grid::Dynamic { beats } => {
            let mut gaps: Vec<i64> = beats
                .windows(2)
                .map(|w| w[1].position - w[0].position)
                .filter(|&g| g > 0)
                .collect();
            gaps.sort_unstable();
            gaps.get(gaps.len() / 2)
                .map(|&g| 60.0 * f64::from(ANALYSIS_RATE) / g as f64)
        }
    }
}

fn median(v: &mut [f64]) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(f64::total_cmp);
    let n = v.len();
    Some(if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    })
}

fn quantile(sorted: &[f64], q: f64) -> f64 {
    let pos = q * (sorted.len() - 1) as f64;
    let (lo, hi) = (pos.floor() as usize, pos.ceil() as usize);
    sorted[lo] + (sorted[hi] - sorted[lo]) * (pos - lo as f64)
}

/// rekordbox − reks for raw beats on a constant rekordbox grid: the median
/// signed distance from each raw beat to rekordbox's nearest beat, over the
/// beats within a quarter beat of it.
fn raw_offset(rb: &RbGrid, beats: &[i64]) -> Option<f64> {
    let bpm = rb.constant_bpm()?;
    let quarter = 15_000.0 / bpm;
    let mut d: Vec<f64> = beats
        .iter()
        .filter_map(|&b| rb.nearest_beat(ms(b as f64)))
        .map(|(d, _)| d)
        .filter(|d| d.abs() < quarter)
        .collect();
    if d.len() < MIN_OFFSET_BEATS {
        return None;
    }
    median(&mut d)
}

/// The record's beats placed with `placement` (default: the pipeline's,
/// `BeatPlacement::DEFAULT`); the stored beats when it has no candidates.
fn placed_beats(rec: &Record, placement: Option<BeatPlacement>) -> Vec<TrackedBeat> {
    let n = rec.model_beats.len();
    let stored = || {
        rec.beats
            .iter()
            .enumerate()
            .map(|(i, &position)| TrackedBeat {
                position,
                downbeat: rec.downbeats.binary_search(&(i as u32)).is_ok(),
            })
            .collect()
    };
    let complete = [
        rec.model_beats.len(),
        rec.onset_beats.len(),
        rec.kick_beats.len(),
        rec.kick_db.len(),
        rec.kick_attack_beats.len(),
    ]
    .iter()
    .all(|&l| l == n && n > 0);
    let Some(placement) = placement
        .or(Some(BeatPlacement::DEFAULT))
        .filter(|_| complete)
    else {
        return stored();
    };
    let some = |v: i64| (v >= 0).then_some(v);
    let candidates: Vec<BeatCandidates> = (0..n)
        .map(|i| BeatCandidates {
            model: rec.model_beats[i],
            onset: rec.onset_beats[i],
            kick: some(rec.kick_beats[i]).map(|k| (k, f32::from(rec.kick_db[i]))),
            kick_onset: some(rec.kick_attack_beats[i]),
        })
        .collect();
    // Downbeat flags index the model's beats (`run` stores them before
    // placement drops any).
    let mut beats: Vec<Beat> = (0..n)
        .map(|i| Beat {
            sample: rec.model_beats[i],
            downbeat: rec.downbeats.binary_search(&(i as u32)).is_ok(),
        })
        .collect();
    place_beats(&mut beats, &candidates, placement);
    beats
        .iter()
        .map(|b| TrackedBeat {
            position: b.sample,
            downbeat: b.downbeat,
        })
        .collect()
}

fn compare(
    t: &XmlTrack,
    rec: &Record,
    pulse: Option<&KickPulse>,
    opts: &GridOptions,
    placement: Option<BeatPlacement>,
) -> Option<Row> {
    if rec.error.is_some() {
        return None;
    }
    let rb = RbGrid::from_track(t)?;
    let tracked = grid::BeatList {
        sample_rate: ANALYSIS_RATE,
        beats: placed_beats(rec, placement),
    };
    let beats: Vec<i64> = tracked.beats.iter().map(|b| b.position).collect();
    let g = build_grid_with_pulse(&tracked, pulse, opts).ok();
    let rb_bpm = rb
        .constant_bpm()
        .or(t.average_bpm)
        .unwrap_or(rb.anchors[0].bpm);
    let ours_bpm = g.as_ref().and_then(grid_bpm);
    let bpm = ours_bpm.map_or(BpmClass::NoGrid, |o| BpmClass::of(o, rb_bpm));
    let first = g.as_ref().and_then(first_downbeat).map(ms);
    let key = t
        .tonality
        .as_deref()
        .and_then(|k| Key::parse(k).ok())
        .map(|rbk| KeyClass::of(rec.key.as_deref().and_then(|k| Key::parse(k).ok()), rbk));
    Some(Row {
        genre: genre_bucket(t.genre.as_deref()),
        codec: codec_group(t, rec),
        lead_ms: rec.lead_ms,
        rb_bpm,
        rb_edited: rb.anchors.len() > 1,
        ours_bpm,
        dynamic: matches!(g, Some(Grid::Dynamic { .. })),
        bpm,
        downbeat_ms: first.and_then(|f| rb.nearest_downbeat(f)),
        beat_ms: first.and_then(|f| rb.nearest_beat(f)),
        key,
        raw_offset_ms: (bpm == BpmClass::Match)
            .then(|| raw_offset(&rb, &beats))
            .flatten(),
    })
}

/// Coarse genre buckets from free-text tags (first match wins).
fn genre_bucket(genre: Option<&str>) -> &'static str {
    let g = genre.unwrap_or("").to_lowercase();
    if g.trim().is_empty() {
        return "(no genre tag)";
    }
    const RULES: [(&[&str], &str); 8] = [
        (
            &["psy", "goa", "psichedelic", "psychadelic"],
            "Psytrance / Goa",
        ),
        (&["trance"], "Trance"),
        (&["techno"], "Techno"),
        (&["house"], "House"),
        (&["electronic", "electronica"], "Electronic"),
        (&["electro"], "Electro"),
        (&["dance", "edm"], "Dance"),
        (&["ambient", "downtempo", "chill"], "Ambient / Downtempo"),
    ];
    RULES
        .iter()
        .find(|(words, _)| words.iter().any(|w| g.contains(w)))
        .map_or("Other", |(_, bucket)| bucket)
}

// ---------------------------------------------------------------------------
// Report

fn pct(n: usize, of: usize) -> String {
    if of == 0 {
        "–".to_owned()
    } else {
        format!("{:.1}%", 100.0 * n as f64 / of as f64)
    }
}

fn report(flags: &Flags) -> anyhow::Result<()> {
    let xml = flags.xml()?;
    let out = flags.out()?;
    let collection = rbxml::parse_file(&xml)?;
    let results = load_results(&out)?;
    let version = flags.analyser.unwrap_or(ANALYSER_VERSION);
    let runs: Vec<RunRecord> = fs::read_to_string(out.join(RUNS))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();

    let mut rows = Vec::new();
    let mut tsv = String::from(
        "location\tgenre\tcodec\trb_bpm\tours_bpm\tgrid\tbpm\tdownbeat_ms\tbeat_ms\tbeat_no\tkey\traw_offset_ms\n",
    );
    let (mut analysed, mut failed, mut no_grid_rb, mut stale) = (0, 0, 0, 0);
    let mut no_kick_curve = 0usize;
    let mut seen = HashSet::new();
    for t in &collection.tracks {
        let Some(loc) = t.location.as_deref() else {
            continue;
        };
        let Some(rec) = results.get(loc) else {
            continue;
        };
        if !seen.insert(loc) {
            continue;
        }
        if rec.analyser != version {
            stale += 1;
            continue;
        }
        analysed += 1;
        if rec.error.is_some() {
            failed += 1;
            continue;
        }
        // The curve `run` stored with this record's candidates (older
        // results have neither): the grid then matches the pipeline's.
        let pulse = if flags.grid.kick_phase_ms > 0 && !rec.model_beats.is_empty() {
            let p = fs::read(kick_file(&out, loc)).ok();
            if p.is_none() {
                no_kick_curve += 1;
            }
            p.map(KickPulse::from_quarter_db)
        } else {
            None
        };
        let Some(row) = compare(t, rec, pulse.as_ref(), &flags.grid, flags.placement) else {
            no_grid_rb += 1;
            continue;
        };
        let f = |v: Option<f64>| v.map_or(String::new(), |v| format!("{v:.1}"));
        let _ = writeln!(
            tsv,
            "{loc}\t{}\t{}\t{:.2}\t{}\t{}\t{:?}\t{}\t{}\t{}\t{}\t{}",
            row.genre,
            row.codec,
            row.rb_bpm,
            row.ours_bpm.map_or(String::new(), |b| format!("{b:.2}")),
            if row.dynamic { "dynamic" } else { "constant" },
            row.bpm,
            f(row.downbeat_ms),
            f(row.beat_ms.map(|b| b.0)),
            row.beat_ms.map_or(String::new(), |b| b.1.to_string()),
            row.key.map_or(String::new(), |k| format!("{k:?}")),
            f(row.raw_offset_ms),
        );
        rows.push(row);
    }
    if no_kick_curve > 0 {
        eprintln!(
            "{no_kick_curve} tracks without a kick curve in {}: their grids skip the kick \
             phase (re-run them)",
            out.join(KICK_DIR).display()
        );
    }
    if let Some(path) = &flags.tracks {
        fs::write(path, &tsv).with_context(|| format!("writing {}", path.display()))?;
    }

    let mut md = String::new();
    if let Some(p) = flags.placement {
        let _ = writeln!(md, "What-if beat placement: {}.\n", p.name());
    }
    if flags.grid != GridOptions::default() {
        let _ = writeln!(md, "What-if grid options: {:?}.\n", flags.grid);
    }
    let _ = writeln!(
        md,
        "Analyser version {version}; {} tracks in the XML, {analysed} analysed \
         ({failed} failed to decode or analyse), {} compared ({no_grid_rb} without a rekordbox \
         grid){}.\n",
        collection.tracks.len(),
        rows.len(),
        if stale > 0 {
            format!(", {stale} results from an older analyser ignored")
        } else {
            String::new()
        }
    );
    // Several anchors are the maintainer's edits (tempo changes, re-phased
    // sections): reported, but not held to the acceptance criteria.
    let scored: Vec<Row> = rows.iter().filter(|r| !r.rb_edited).cloned().collect();
    let _ = writeln!(
        md,
        "Scored: the {} tracks whose rekordbox grid has one anchor; the {} with several \
         anchors (hand-edited) are listed separately and not scored.\n",
        scored.len(),
        rows.len() - scored.len()
    );
    summary_table(&mut md, &scored);
    edited_table(&mut md, &rows);
    bpm_table(&mut md, &scored);
    downbeat_table(&mut md, &scored);
    key_table(&mut md, &scored);
    genre_table(&mut md, &scored);
    offset_table(&mut md, &rows);
    throughput(&mut md, &runs, version);

    match &flags.md {
        Some(path) => {
            fs::write(path, &md).with_context(|| format!("writing {}", path.display()))?
        }
        None => print!("{md}"),
    }
    Ok(())
}

fn summary_table(md: &mut String, rows: &[Row]) {
    let n = rows.len();
    let bpm = rows.iter().filter(|r| r.bpm == BpmClass::Match).count();
    let down = rows.iter().filter(|r| r.downbeat_ok()).count();
    let keyed: Vec<KeyClass> = rows.iter().filter_map(|r| r.key).collect();
    let exact = keyed.iter().filter(|&&k| k == KeyClass::Exact).count();
    let rel = keyed
        .iter()
        .filter(|&&k| matches!(k, KeyClass::Exact | KeyClass::Relative))
        .count();
    let _ = writeln!(
        md,
        "| Criterion | Target | Measured | n |\n| --- | --- | --- | --- |"
    );
    let line = |md: &mut String, name: &str, target: &str, hit: usize, of: usize| {
        let _ = writeln!(md, "| {name} | {target} | {} | {of} |", pct(hit, of));
    };
    line(md, "BPM \\|Δ\\| ≤ 0.05", "≥ 90%", bpm, n);
    line(md, "Downbeat within 30 ms (mod bar)", "≥ 80%", down, n);
    line(md, "Key exact", "≥ 70%", exact, keyed.len());
    line(md, "Key exact or relative", "≥ 85%", rel, keyed.len());
    md.push('\n');
}

/// Agreement split by whether rekordbox's grid has several anchors (the
/// maintainer's hand edits) or one (rekordbox's analysis, possibly re-phased).
fn edited_table(md: &mut String, rows: &[Row]) {
    let _ = writeln!(
        md,
        "| rekordbox grid | Tracks | BPM | Downbeat | Key exact |\n| --- | --- | --- | --- | --- |"
    );
    for (name, edited) in [("one anchor", false), ("several anchors (edited)", true)] {
        let v: Vec<&Row> = rows.iter().filter(|r| r.rb_edited == edited).collect();
        let keyed: Vec<KeyClass> = v.iter().filter_map(|r| r.key).collect();
        let _ = writeln!(
            md,
            "| {name} | {} | {} | {} | {} |",
            v.len(),
            pct(
                v.iter().filter(|r| r.bpm == BpmClass::Match).count(),
                v.len()
            ),
            pct(v.iter().filter(|r| r.downbeat_ok()).count(), v.len()),
            pct(
                keyed.iter().filter(|&&k| k == KeyClass::Exact).count(),
                keyed.len()
            ),
        );
    }
    md.push('\n');
}

fn class_table<C: Ord + Copy>(
    md: &mut String,
    title: &str,
    items: impl Iterator<Item = C>,
    label: impl Fn(C) -> &'static str,
) {
    let mut counts: BTreeMap<C, usize> = BTreeMap::new();
    let mut n = 0;
    for c in items {
        *counts.entry(c).or_default() += 1;
        n += 1;
    }
    let _ = writeln!(md, "| {title} | Tracks | Share |\n| --- | --- | --- |");
    for (c, k) in counts {
        let _ = writeln!(md, "| {} | {k} | {} |", label(c), pct(k, n));
    }
    md.push('\n');
}

fn bpm_table(md: &mut String, rows: &[Row]) {
    let _ = writeln!(md, "### BPM\n");
    class_table(
        md,
        "reks vs rekordbox",
        rows.iter().map(|r| r.bpm),
        BpmClass::label,
    );
    let _ = writeln!(
        md,
        "| reks grid | Tracks | BPM match | Downbeat |\n| --- | --- | --- | --- |"
    );
    for (name, dynamic) in [("constant", false), ("dynamic (fallback)", true)] {
        let v: Vec<&Row> = rows
            .iter()
            .filter(|r| r.dynamic == dynamic && r.bpm != BpmClass::NoGrid)
            .collect();
        let _ = writeln!(
            md,
            "| {name} | {} | {} | {} |",
            v.len(),
            pct(
                v.iter().filter(|r| r.bpm == BpmClass::Match).count(),
                v.len()
            ),
            pct(v.iter().filter(|r| r.downbeat_ok()).count(), v.len()),
        );
    }
    md.push('\n');
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum DownClass {
    Ok,
    /// On a rekordbox beat (within 30 ms) but not its downbeat; beats off.
    BeatsOff(u8),
    OffBeat,
    NoGrid,
}

fn down_class(r: &Row) -> DownClass {
    if r.downbeat_ok() {
        return DownClass::Ok;
    }
    match r.beat_ms {
        None => DownClass::NoGrid,
        Some((d, beat)) if d.abs() <= DOWNBEAT_TOLERANCE_MS => {
            // Our downbeat sits on rekordbox's beat `beat`: we are
            // (beat − 1) beats into rekordbox's bar.
            DownClass::BeatsOff(beat - 1)
        }
        Some(_) => DownClass::OffBeat,
    }
}

fn downbeat_table(md: &mut String, rows: &[Row]) {
    let _ = writeln!(md, "### Downbeat (first downbeat, modulo bar)\n");
    let label = |c: DownClass| match c {
        DownClass::Ok => "within 30 ms of a rekordbox downbeat",
        DownClass::BeatsOff(1) => "on beat 2 of rekordbox's bar",
        DownClass::BeatsOff(2) => "on beat 3 of rekordbox's bar",
        DownClass::BeatsOff(_) => "on beat 4 of rekordbox's bar",
        DownClass::OffBeat => "more than 30 ms from any rekordbox beat",
        DownClass::NoGrid => "no grid",
    };
    class_table(
        md,
        "reks's first downbeat",
        rows.iter().map(down_class),
        label,
    );
    let matched: Vec<&Row> = rows.iter().filter(|r| r.bpm == BpmClass::Match).collect();
    let ok = matched.iter().filter(|r| r.downbeat_ok()).count();
    let _ = writeln!(
        md,
        "Among tracks whose BPM matches: {} of {} within 30 ms.\n",
        pct(ok, matched.len()),
        matched.len()
    );
    // Grid phase, whatever the bar: how far our grid lines sit from
    // rekordbox's (the first downbeat stands for all of a constant grid).
    let near = |ms: f64| {
        matched
            .iter()
            .filter(|r| r.beat_ms.is_some_and(|(d, _)| d.abs() <= ms))
            .count()
    };
    let _ = writeln!(
        md,
        "Grid phase, same tracks: our first downbeat within 30 ms of any rekordbox \
         beat {}, within 10 ms {}.\n",
        pct(near(DOWNBEAT_TOLERANCE_MS), matched.len()),
        pct(near(10.0), matched.len())
    );
}

fn key_table(md: &mut String, rows: &[Row]) {
    let _ = writeln!(md, "### Key\n");
    class_table(
        md,
        "reks vs rekordbox",
        rows.iter().filter_map(|r| r.key),
        KeyClass::label,
    );
}

fn genre_table(md: &mut String, rows: &[Row]) {
    let _ = writeln!(md, "### Per genre\n");
    let mut by: BTreeMap<&str, Vec<&Row>> = BTreeMap::new();
    for r in rows {
        by.entry(r.genre).or_default().push(r);
    }
    let mut groups: Vec<(&str, Vec<&Row>)> = by.into_iter().collect();
    groups.sort_by_key(|(_, v)| std::cmp::Reverse(v.len()));
    let _ = writeln!(
        md,
        "| Genre (tag bucket) | Tracks | BPM | Downbeat | Key exact | Key exact/relative |\n\
         | --- | --- | --- | --- | --- | --- |"
    );
    for (g, v) in groups {
        let n = v.len();
        let keyed: Vec<KeyClass> = v.iter().filter_map(|r| r.key).collect();
        let _ = writeln!(
            md,
            "| {g} | {n} | {} | {} | {} | {} |",
            pct(v.iter().filter(|r| r.bpm == BpmClass::Match).count(), n),
            pct(v.iter().filter(|r| r.downbeat_ok()).count(), n),
            pct(
                keyed.iter().filter(|&&k| k == KeyClass::Exact).count(),
                keyed.len()
            ),
            pct(
                keyed
                    .iter()
                    .filter(|&&k| matches!(k, KeyClass::Exact | KeyClass::Relative))
                    .count(),
                keyed.len()
            ),
        );
    }
    md.push('\n');
}

fn offset_table(md: &mut String, rows: &[Row]) {
    let _ = writeln!(
        md,
        "### Timing offset per codec (task 4)\n\n\
         Residual: rekordbox − reks beat time in ms after the decoder's lead \
         (`reks_audio::timing`), per track the median over raw tracker beats within a \
         quarter beat of rekordbox's constant grid, BPM-matched tracks only. \
         Raw = residual + lead: rekordbox − Symphonia's gapless decode. A codec's \
         convention is right when its residual matches the lossless formats' (the \
         tracker's own bias against rekordbox).\n"
    );
    let mut by: BTreeMap<&str, (Vec<f64>, Vec<f64>)> = BTreeMap::new();
    for r in rows {
        if let Some(o) = r.raw_offset_ms {
            let e = by.entry(r.codec.as_str()).or_default();
            e.0.push(o);
            e.1.push(o + r.lead_ms);
        }
    }
    let _ = writeln!(
        md,
        "| Codec / rule | Tracks | Residual median | Residual 25–75% | Raw median |\n\
         | --- | --- | --- | --- | --- |"
    );
    for (label, (mut res, mut raw)) in by {
        res.sort_by(f64::total_cmp);
        raw.sort_by(f64::total_cmp);
        let _ = writeln!(
            md,
            "| {label} | {} | {:+.1} | {:+.1} … {:+.1} | {:+.1} |",
            res.len(),
            quantile(&res, 0.5),
            quantile(&res, 0.25),
            quantile(&res, 0.75),
            quantile(&raw, 0.5),
        );
    }
    md.push('\n');
}

fn throughput(md: &mut String, runs: &[RunRecord], version: u32) {
    let _ = writeln!(md, "### Throughput\n");
    let runs: Vec<&RunRecord> = runs
        .iter()
        .filter(|r| r.analyser == version && r.tracks > 0)
        .collect();
    let audio: u64 = runs.iter().map(|r| r.audio_ms).sum();
    let cpu: u64 = runs.iter().filter_map(|r| r.cpu_ms).sum();
    let cpu_audio: u64 = runs
        .iter()
        .filter(|r| r.cpu_ms.is_some())
        .map(|r| r.audio_ms)
        .sum();
    if cpu > 0 {
        let _ = writeln!(
            md,
            "Per core: {:.1}× real time ({:.1} h of audio over {:.1} CPU hours, decode, \
             beats, key and waveform columns; process user + system time).",
            cpu_audio as f64 / cpu as f64,
            cpu_audio as f64 / 3.6e6,
            cpu as f64 / 3.6e6,
        );
    }
    for r in &runs {
        let _ = writeln!(
            md,
            "- run {}: {} tracks ({:.1} h audio) on {} workers / {} cores, {:.0} min wall, \
             {:.1}× real time per worker by wall clock",
            r.started,
            r.tracks,
            r.audio_ms as f64 / 3.6e6,
            r.jobs,
            r.cores,
            r.wall_ms as f64 / 60_000.0,
            r.audio_ms as f64 / r.wall_ms.max(1) as f64 / r.jobs as f64,
        );
    }
    if audio == 0 {
        let _ = writeln!(md, "No run of this analyser version recorded.");
    }
    md.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;
    use reks_core::rbxml::{Extra, Tempo};

    fn tempo(inizio: f64, bpm: f64, battito: u8) -> Tempo {
        Tempo {
            inizio,
            bpm,
            metro: Some("4/4".into()),
            battito: Some(battito),
            extra: Extra::new(),
        }
    }

    fn grid(tempos: Vec<Tempo>) -> RbGrid {
        RbGrid::from_track(&XmlTrack {
            tempos,
            ..XmlTrack::default()
        })
        .unwrap()
    }

    #[test]
    fn rb_grid_beats_and_downbeats() {
        // 120 BPM: a beat every 500 ms; the anchor at 100 ms is beat 3.
        let g = grid(vec![tempo(0.1, 120.0, 3)]);
        assert_eq!(g.constant_bpm(), Some(120.0));
        let (d, beat) = g.nearest_beat(1_090.0).unwrap();
        assert!((d - 10.0).abs() < 1e-9 && beat == 1, "{d} {beat}");
        // Downbeats at 1100, 3100, …; the grid also extends back (−900).
        assert!((g.nearest_downbeat(3_000.0).unwrap() - 100.0).abs() < 1e-9);
        assert!((g.nearest_downbeat(-850.0).unwrap() + 50.0).abs() < 1e-9);
    }

    #[test]
    fn rb_grid_follows_each_anchor() {
        // Re-phased at 10.25 s (beat 1), same tempo: not a tempo change.
        let g = grid(vec![tempo(0.0, 120.0, 1), tempo(10.25, 120.0, 1)]);
        assert_eq!(g.constant_bpm(), Some(120.0));
        let (d, beat) = g.nearest_beat(10_260.0).unwrap();
        assert!((d + 10.0).abs() < 1e-9 && beat == 1);
        // The first anchor's beat at 10.0 s is cut off by the second anchor.
        assert!((g.nearest_beat(9_990.0).unwrap().0 - 10.0).abs() < 1e-9);
        let changed = grid(vec![tempo(0.0, 120.0, 1), tempo(8.0, 126.0, 1)]);
        assert_eq!(changed.constant_bpm(), None);
    }

    #[test]
    fn bpm_classes() {
        assert_eq!(BpmClass::of(128.0, 128.04), BpmClass::Match);
        assert_eq!(BpmClass::of(128.0, 128.2), BpmClass::Close);
        assert_eq!(BpmClass::of(87.0, 174.0), BpmClass::Octave);
        assert_eq!(BpmClass::of(140.0, 70.0), BpmClass::Octave);
        assert_eq!(BpmClass::of(90.0, 135.0), BpmClass::Third);
        assert_eq!(BpmClass::of(100.0, 128.0), BpmClass::Other);
    }

    #[test]
    fn key_classes() {
        let k = |s: &str| Key::parse(s).unwrap();
        assert_eq!(KeyClass::of(Some(k("Am")), k("Am")), KeyClass::Exact);
        assert_eq!(KeyClass::of(Some(k("C")), k("Am")), KeyClass::Relative);
        assert_eq!(KeyClass::of(Some(k("Em")), k("Am")), KeyClass::Neighbour);
        assert_eq!(KeyClass::of(Some(k("Dm")), k("Am")), KeyClass::Neighbour);
        assert_eq!(KeyClass::of(Some(k("A")), k("Am")), KeyClass::Parallel);
        assert_eq!(KeyClass::of(Some(k("F#")), k("Am")), KeyClass::Other);
        assert_eq!(KeyClass::of(None, k("Am")), KeyClass::None);
    }

    #[test]
    fn first_downbeat_of_a_constant_grid() {
        let g = Grid::Constant {
            bpm: 120.0,
            first_beat: 1_000,
            first_beat_in_bar: 3,
        };
        // Beats 3 and 4 come first: the downbeat is two beats later.
        assert_eq!(first_downbeat(&g), Some(1_000.0 + 2.0 * 22_050.0));
    }

    #[test]
    fn raw_offset_is_the_median_distance() {
        let g = grid(vec![tempo(0.0, 120.0, 1)]);
        // Beats 20 ms early (rekordbox later by +20), one outlier.
        let mut beats: Vec<i64> = (1..60)
            .map(|i| ((i as f64 * 0.5 - 0.02) * 44_100.0) as i64)
            .collect();
        beats.push((30.3 * 44_100.0) as i64);
        let off = raw_offset(&g, &beats).unwrap();
        assert!((off - 20.0).abs() < 0.1, "{off}");
        assert_eq!(raw_offset(&g, &beats[..10]), None);
    }

    #[test]
    fn genre_buckets() {
        assert_eq!(
            genre_bucket(Some("Psy-Trance, Progressive Trance")),
            "Psytrance / Goa"
        );
        assert_eq!(genre_bucket(Some("Progressive Trance")), "Trance");
        assert_eq!(genre_bucket(Some("Electronica")), "Electronic");
        assert_eq!(genre_bucket(Some("Electro Dance")), "Electro");
        assert_eq!(genre_bucket(Some("Tech House")), "House");
        assert_eq!(genre_bucket(Some("")), "(no genre tag)");
        assert_eq!(genre_bucket(Some("Jazz")), "Other");
    }

    #[test]
    fn flags_parse() {
        let args: Vec<String> = [
            "--xml",
            "a.xml",
            "--every",
            "0",
            "--only",
            "MP3,m4a",
            "--retry-failed",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let f = Flags::parse(&args).unwrap();
        assert_eq!(f.xml().unwrap(), PathBuf::from("a.xml"));
        assert_eq!(f.every, 1);
        assert!(f.retry_failed);
        assert_eq!(f.only, Some(vec!["mp3".to_owned(), "m4a".to_owned()]));
        assert!(Flags::parse(&["--bogus".to_owned()]).is_err());
    }
}
