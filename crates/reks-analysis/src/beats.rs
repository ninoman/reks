//! Beat and downbeat tracking (phase 3 task 2, docs/decisions.md D10):
//! Beat This! through the `beat-this` crate on its pure-Rust `rten` backend.
//!
//! The model files are not compiled in. The small model and the mel front end
//! ship in `crates/reks-analysis/models/` (bundled as Tauri resources under
//! `models/`); [`ModelPaths::small`] finds them in a directory.
//!
//! The model emits beat and downbeat activations at 50 frames/s (20 ms), and
//! its peaks sit 5–15 ms after the transient on our fixtures. Each beat is
//! refined in two steps: a parabola through the activation at the peak frame
//! and its neighbours gives a sub-frame time, then [`snap_to_onset`] moves it
//! to the sharpest energy rise in the 44.1 kHz signal within ±30 ms, which is
//! where a DJ expects the grid line. Beats with no clear transient there keep
//! the model's time. Positions are samples at [`ANALYSIS_RATE`].
//!
//! [`BeatTracker::track`] blocks for about real time / 5 to real time / 60
//! depending on the CPU (rten uses its own rayon pool); run it in
//! `spawn_blocking` or a worker thread.

use std::path::{Path, PathBuf};

use beat_this::{BeatThis, RtenRuntime, Runtime};

use crate::input::{AnalysisInput, ANALYSIS_RATE, BEAT_RATE};

/// Activation frames per second of the Beat This! model.
const FPS: f64 = 50.0;

/// How far [`snap_to_onset`] looks either side of the model's beat.
const SNAP_RADIUS_MS: u32 = 30;
/// Energy windows after and before a candidate onset.
const AFTER_MS: u32 = 5;
const BEFORE_MS: u32 = 20;
/// Smallest energy ratio (after / before) that counts as a transient.
const MIN_ONSET_RATIO: f64 = 2.0;
/// Candidate step in samples (0.25 ms at 44.1 kHz).
const SNAP_STEP: usize = 11;

/// Errors from the beat tracker.
#[derive(Debug, thiserror::Error)]
pub enum BeatError {
    #[error("beat model file missing: {0}")]
    MissingModel(PathBuf),
    /// `beat-this` reports errors as `anyhow`; kept as text.
    #[error("cannot load beat model {path}: {reason}")]
    Load { path: PathBuf, reason: String },
    #[error("beat tracking failed: {0}")]
    Inference(String),
}

/// Where the ONNX model files are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelPaths {
    /// Log-mel front end (`mel_spectrogram.onnx`).
    pub mel: PathBuf,
    /// Beat model (`beat_this_small.onnx`, or the full `beat_this.onnx`).
    pub beat: PathBuf,
}

impl ModelPaths {
    /// File name of the mel front end.
    pub const MEL_FILE: &'static str = "mel_spectrogram.onnx";
    /// File name of the bundled small beat model.
    pub const SMALL_FILE: &'static str = "beat_this_small.onnx";

    /// The small model and mel front end in `dir` (the app's `models/`
    /// resource directory, or [`ModelPaths::dev_dir`]).
    ///
    /// TODO(phase 3): the full model (`beat_this.onnx`, ~83 MB, checksummed
    /// GitHub release asset of beat-this-rs) as an optional download into the
    /// app data directory, preferred over the small one when present.
    pub fn small(dir: &Path) -> Self {
        Self {
            mel: dir.join(Self::MEL_FILE),
            beat: dir.join(Self::SMALL_FILE),
        }
    }

    /// The model directory in this source tree, for tests and the CLI.
    pub fn dev_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("models")
    }
}

/// One beat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Beat {
    /// Position in samples at [`BeatList::sample_rate`], from the first
    /// decoded sample (after gapless trimming; see [`crate::input`]).
    pub sample: i64,
    /// The tracker marked this beat as the first of a bar.
    pub downbeat: bool,
}

/// The tracker's beats for one track, in time order.
#[derive(Debug, Clone, PartialEq)]
pub struct BeatList {
    /// Rate of [`Beat::sample`]: always [`ANALYSIS_RATE`].
    pub sample_rate: u32,
    pub beats: Vec<Beat>,
    /// Mean beat probability (sigmoid of the activation) at the beats, 0..=1;
    /// 0 when there are none. Low on noise, silence and beatless audio.
    pub confidence: f32,
}

impl BeatList {
    /// Time of `beat` in seconds.
    pub fn seconds(&self, beat: &Beat) -> f64 {
        beat.sample as f64 / f64::from(self.sample_rate)
    }

    /// All beat times in seconds.
    pub fn beats_s(&self) -> Vec<f64> {
        self.beats.iter().map(|b| self.seconds(b)).collect()
    }

    /// Downbeat times in seconds.
    pub fn downbeats_s(&self) -> Vec<f64> {
        self.beats
            .iter()
            .filter(|b| b.downbeat)
            .map(|b| self.seconds(b))
            .collect()
    }
}

type RtenModel = <RtenRuntime as Runtime>::Model;

/// A loaded Beat This! model. Loading takes a moment; keep one per worker
/// thread and reuse it across tracks.
pub struct BeatTracker {
    model: BeatThis<RtenModel>,
}

impl std::fmt::Debug for BeatTracker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BeatTracker").finish_non_exhaustive()
    }
}

impl BeatTracker {
    /// Loads both models.
    pub fn load(paths: &ModelPaths) -> Result<Self, BeatError> {
        for path in [&paths.mel, &paths.beat] {
            if !path.is_file() {
                return Err(BeatError::MissingModel(path.clone()));
            }
        }
        let model =
            BeatThis::new(&RtenRuntime, &paths.mel, &paths.beat).map_err(|e| BeatError::Load {
                path: paths.beat.clone(),
                reason: format!("{e:#}"),
            })?;
        tracing::debug!(beat = %paths.beat.display(), "beat model loaded");
        Ok(Self { model })
    }

    /// Tracks beats and downbeats in `input`. Blocking.
    pub fn track(&mut self, input: &AnalysisInput) -> Result<BeatList, BeatError> {
        if input.beat_mono.is_empty() {
            return Ok(to_beat_list(&[], &[], &[]));
        }
        let analysis = self
            .model
            .analyze_audio(&input.beat_mono, BEAT_RATE)
            .map_err(|e| BeatError::Inference(format!("{e:#}")))?;
        let mut list = to_beat_list(&analysis.beats, &analysis.downbeats, &analysis.beat_logits);
        for beat in &mut list.beats {
            beat.sample = snap_to_onset(&input.mono, beat.sample);
        }
        tracing::debug!(
            beats = list.beats.len(),
            downbeats = list.beats.iter().filter(|b| b.downbeat).count(),
            confidence = list.confidence,
            "beats tracked"
        );
        Ok(list)
    }
}

/// Joins the crate's beat and downbeat times (seconds; downbeats are already
/// snapped onto beat times) into one list, refining each beat on `logits`.
fn to_beat_list(beats_s: &[f32], downbeats_s: &[f32], logits: &[f32]) -> BeatList {
    let rate = f64::from(ANALYSIS_RATE);
    let mut probability_sum = 0.0f64;
    let beats: Vec<Beat> = beats_s
        .iter()
        .map(|&t| {
            let frame = f64::from(t) * FPS;
            if let Some(&l) = logits.get(frame.round() as usize) {
                probability_sum += 1.0 / (1.0 + (-f64::from(l)).exp());
            }
            let time = refine(frame, logits) / FPS;
            Beat {
                sample: (time * rate).round() as i64,
                // Downbeat times are copies of beat times.
                downbeat: downbeats_s.iter().any(|&d| (d - t).abs() < 1e-4),
            }
        })
        .collect();
    let confidence = if beats.is_empty() {
        0.0
    } else {
        (probability_sum / beats.len() as f64) as f32
    };
    BeatList {
        sample_rate: ANALYSIS_RATE,
        beats,
        confidence,
    }
}

/// Sub-frame position of the activation peak near `frame`: the vertex of the
/// parabola through the nearest frame and its two neighbours, kept within
/// half a frame. Unchanged where the nearest frame is not a local maximum
/// (merged double peaks, track edges).
fn refine(frame: f64, logits: &[f32]) -> f64 {
    let i = frame.round();
    if i < 1.0 {
        return frame;
    }
    let i = i as usize;
    let (Some(&a), Some(&b), Some(&c)) = (logits.get(i - 1), logits.get(i), logits.get(i + 1))
    else {
        return frame;
    };
    let (a, b, c) = (f64::from(a), f64::from(b), f64::from(c));
    let curvature = a - 2.0 * b + c;
    if b < a || b < c || curvature >= 0.0 {
        return frame;
    }
    i as f64 + (0.5 * (a - c) / curvature).clamp(-0.5, 0.5)
}

/// The sample within ±[`SNAP_RADIUS_MS`] of `sample` where the energy of
/// the next [`AFTER_MS`] most exceeds that of the previous [`BEFORE_MS`], if
/// that ratio reaches [`MIN_ONSET_RATIO`]; otherwise `sample` unchanged.
/// Energy is of the first difference of `mono` (a +6 dB/octave tilt), so the
/// attack of a kick or snare counts, not the sustained bass under it. `mono`
/// is at [`ANALYSIS_RATE`].
fn snap_to_onset(mono: &[f32], sample: i64) -> i64 {
    let ms = |n: u32| (ANALYSIS_RATE / 1000 * n) as usize;
    let (radius, after, before) = (ms(SNAP_RADIUS_MS), ms(AFTER_MS), ms(BEFORE_MS));
    let Ok(centre) = usize::try_from(sample) else {
        return sample;
    };
    let lo = centre.saturating_sub(radius);
    let hi = (centre + radius).min(mono.len().saturating_sub(after));
    if lo > hi {
        return sample;
    }
    // Prefix sums of the squared first difference over the span the windows
    // touch; before the track's first sample is silence.
    let base = lo.saturating_sub(before);
    let mut energy = Vec::with_capacity(hi + after - base + 1);
    energy.push(0.0f64);
    let mut acc = 0.0f64;
    let mut prev = base.checked_sub(1).map_or(0.0, |i| f64::from(mono[i]));
    for &x in &mono[base..hi + after] {
        let d = f64::from(x) - prev;
        prev = f64::from(x);
        acc += d * d;
        energy.push(acc);
    }
    let sum = |from: usize, to: usize| energy[to - base] - energy[from - base];
    // Floor at -80 dBFS so silence before a quiet onset cannot divide by zero.
    let floor = 1e-8;
    let mut best = (sample, MIN_ONSET_RATIO);
    for p in (lo..=hi).step_by(SNAP_STEP) {
        let rise = (sum(p, p + after) / after as f64 + floor)
            / (sum(p.saturating_sub(before).max(base), p) / before as f64 + floor);
        if rise > best.1 {
            best = (p as i64, rise);
        }
    }
    best.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refine_finds_the_parabola_vertex() {
        // Samples of -(x - 10.3)^2 at x = 9, 10, 11.
        let f = |x: f64| -((x - 10.3) * (x - 10.3)) as f32;
        let mut logits = vec![-100.0f32; 20];
        for (x, slot) in logits.iter_mut().enumerate().take(12).skip(9) {
            *slot = f(x as f64);
        }
        assert!((refine(10.0, &logits) - 10.3).abs() < 1e-6);
        // Not a local maximum: unchanged.
        logits[11] = 5.0;
        assert_eq!(refine(10.0, &logits), 10.0);
        // Edges: unchanged.
        assert_eq!(refine(0.0, &logits), 0.0);
        assert_eq!(refine(19.0, &logits), 19.0);
        // A merged peak half-way between frames rounds, then refines.
        assert_eq!(refine(30.5, &logits), 30.5);
    }

    #[test]
    fn beat_list_marks_downbeats_and_converts_to_samples() {
        let beats = [0.5f32, 1.0, 1.5, 2.0];
        let downbeats = [0.5f32, 2.0];
        // Flat activations: no refinement.
        let list = to_beat_list(&beats, &downbeats, &[0.0; 200]);
        assert_eq!(list.sample_rate, 44_100);
        let samples: Vec<i64> = list.beats.iter().map(|b| b.sample).collect();
        assert_eq!(samples, [22_050, 44_100, 66_150, 88_200]);
        let flags: Vec<bool> = list.beats.iter().map(|b| b.downbeat).collect();
        assert_eq!(flags, [true, false, false, true]);
        assert_eq!(list.downbeats_s(), [0.5, 2.0]);
        assert_eq!(list.beats_s()[1], 1.0);
        assert!((list.confidence - 0.5).abs() < 1e-6);
    }

    #[test]
    fn empty_beat_list_has_zero_confidence() {
        let list = to_beat_list(&[], &[], &[]);
        assert!(list.beats.is_empty());
        assert_eq!(list.confidence, 0.0);
    }

    #[test]
    fn snap_moves_to_the_transient_nearby() {
        // Quiet tone with a loud burst starting at sample 10_000.
        let mono: Vec<f32> = (0..20_000)
            .map(|i| {
                let level = if (10_000..10_400).contains(&i) {
                    0.8
                } else {
                    0.05
                };
                level * if i % 20 < 10 { 1.0 } else { -1.0 }
            })
            .collect();
        // 12 ms late, and 12 ms early: both land within one step.
        for guess in [10_529, 9_471] {
            let got = snap_to_onset(&mono, guess);
            assert!((got - 10_000).abs() <= SNAP_STEP as i64, "{guess} → {got}");
        }
        // Out of reach (50 ms away): unchanged.
        assert_eq!(snap_to_onset(&mono, 12_205), 12_205);
        // A hit on the first sample: the time before the track is silence.
        let mut start = mono.clone();
        start[..400].fill(0.8);
        assert_eq!(snap_to_onset(&start, 300), 0);
        // Track end and silence: unchanged.
        assert_eq!(snap_to_onset(&mono, 19_999), 19_999);
        assert_eq!(snap_to_onset(&vec![0.0; 20_000], 10_000), 10_000);
    }

    #[test]
    fn missing_model_is_reported() {
        let err = BeatTracker::load(&ModelPaths::small(Path::new("/nonexistent"))).unwrap_err();
        assert!(matches!(err, BeatError::MissingModel(_)), "{err}");
    }

    #[test]
    fn tracker_can_move_to_a_worker_thread() {
        fn is_send<T: Send>() {}
        is_send::<BeatTracker>();
        is_send::<AnalysisInput>();
    }
}
