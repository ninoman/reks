//! `reks analyse <audio>`: reks's analysis of one file, headless (Phase 3
//! task 10).
//!
//! ```text
//! reks analyse <audio> [--model-dir <dir>] [--json]
//!     decode, track beats, build the grid, detect the key, compute the
//!     waveform columns; print the result (nothing is stored)
//! ```
//!
//! `--model-dir` holds the beat model files (`ModelPaths::small`); default:
//! the source tree's `crates/reks-analysis/models`.

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{bail, Context};
use reks_analysis::grid::Grid;
use reks_analysis::{analyse_file, BeatTracker, ModelPaths, TrackAnalysis, ANALYSIS_RATE};
use serde::Serialize;

const USAGE: &str = "usage: reks analyse <audio> [--model-dir <dir>] [--json]";

pub fn run(args: &[String]) -> anyhow::Result<()> {
    let mut file = None;
    let mut model_dir = None;
    let mut json = false;
    let mut args = args.iter();
    while let Some(a) = args.next() {
        match a.as_str() {
            "--model-dir" => model_dir = Some(PathBuf::from(args.next().context(USAGE)?)),
            "--json" => json = true,
            flag if flag.starts_with("--") => bail!("unknown flag {flag}\n{USAGE}"),
            path if file.is_none() => file = Some(PathBuf::from(path)),
            _ => bail!("{USAGE}"),
        }
    }
    let Some(file) = file else { bail!("{USAGE}") };
    let started = Instant::now();
    let analysis = analyse(&file, model_dir.as_deref())?;
    let summary = Summary::new(&analysis, started.elapsed().as_secs_f64());
    if json {
        println!("{}", serde_json::to_string_pretty(&summary)?);
    } else {
        print!("{}", summary.text());
    }
    Ok(())
}

/// Analyses `file` with the small beat model from `model_dir` (default:
/// [`ModelPaths::dev_dir`]). Blocking and CPU-heavy.
pub fn analyse(file: &Path, model_dir: Option<&Path>) -> anyhow::Result<TrackAnalysis> {
    let dir = model_dir.map_or_else(ModelPaths::dev_dir, Path::to_path_buf);
    let mut tracker = BeatTracker::load(&ModelPaths::small(&dir))
        .with_context(|| format!("loading the beat model from {}", dir.display()))?;
    analyse_file(&mut tracker, file).with_context(|| format!("analysing {}", file.display()))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Summary {
    duration_ms: u64,
    beats: usize,
    downbeats: usize,
    beat_confidence: f32,
    /// `constant`, `dynamic` or `none`.
    grid: &'static str,
    bpm: Option<f64>,
    first_beat_ms: Option<f64>,
    /// `None` = silence.
    key: Option<String>,
    camelot: Option<String>,
    key_confidence: f32,
    columns: usize,
    seconds: f64,
}

impl Summary {
    fn new(a: &TrackAnalysis, seconds: f64) -> Self {
        let ms = |samples: i64| samples as f64 * 1000.0 / f64::from(ANALYSIS_RATE);
        let (grid, bpm, first_beat_ms) = match &a.grid {
            Some(Grid::Constant {
                bpm, first_beat, ..
            }) => ("constant", Some(*bpm), Some(ms(*first_beat))),
            Some(Grid::Dynamic { beats }) => {
                let span = match (beats.first(), beats.last()) {
                    (Some(f), Some(l)) if beats.len() > 1 => Some((f, l)),
                    _ => None,
                };
                let mean_bpm = span.map(|(f, l)| {
                    60.0 * (beats.len() - 1) as f64 / ((ms(l.position) - ms(f.position)) / 1000.0)
                });
                ("dynamic", mean_bpm, beats.first().map(|b| ms(b.position)))
            }
            None => ("none", None, None),
        };
        Self {
            duration_ms: a.duration_ms,
            beats: a.beats.beats.len(),
            downbeats: a.beats.beats.iter().filter(|b| b.downbeat).count(),
            beat_confidence: a.beats.confidence,
            grid,
            bpm,
            first_beat_ms,
            key: a.key.key.map(|k| k.canonical().to_owned()),
            camelot: a.key.key.map(|k| k.camelot()),
            key_confidence: a.key.confidence,
            columns: a.columns.len(),
            seconds,
        }
    }

    fn text(&self) -> String {
        let realtime = self.duration_ms as f64 / 1000.0 / self.seconds.max(1e-9);
        let bpm = self.bpm.map_or("-".to_owned(), |b| format!("{b:.2}"));
        let first = self
            .first_beat_ms
            .map_or("-".to_owned(), |f| format!("{f:.1} ms"));
        let key = match (&self.key, &self.camelot) {
            (Some(k), Some(c)) => format!("{k} ({c})"),
            _ => "silence".to_owned(),
        };
        format!(
            "Duration  {:.3} s\n\
             Beats     {} ({} downbeats), confidence {:.2}\n\
             Grid      {}, {bpm} BPM, first beat {first}\n\
             Key       {key}, confidence {:.2}\n\
             Columns   {} (150/s)\n\
             Took      {:.2} s ({realtime:.1}x real time)\n",
            self.duration_ms as f64 / 1000.0,
            self.beats,
            self.downbeats,
            self.beat_confidence,
            self.grid,
            self.key_confidence,
            self.columns,
            self.seconds,
        )
    }
}
