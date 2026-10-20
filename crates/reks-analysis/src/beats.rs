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
//! and its neighbours gives a sub-frame time, then the beat is placed on the
//! audio ([`BeatPlacement::DEFAULT`]): on the sharpest broadband rise within
//! ±30 ms ([`snap_to_onset`], task 2), which is where a DJ expects the grid
//! line; without one, the model's time. [`BeatCandidates`] also has the
//! low-band kick ([`KickOnsets`]) for each beat (task 12a, measured, not the
//! default). Positions are samples at [`ANALYSIS_RATE`].
//!
//! [`BeatTracker::track`] blocks for about real time / 5 to real time / 60
//! depending on the CPU (rten uses its own rayon pool); run it in
//! `spawn_blocking` or a worker thread.

use std::path::{Path, PathBuf};

use beat_this::{BeatThis, RtenRuntime, Runtime};

use crate::input::{AnalysisInput, ANALYSIS_RATE, BEAT_RATE};
use crate::onset::{KickOnsets, MIN_KICK_DB};

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

    /// Tracks beats and downbeats in `input` and places each beat with
    /// [`BeatPlacement::DEFAULT`]. Blocking.
    pub fn track(&mut self, input: &AnalysisInput) -> Result<BeatList, BeatError> {
        self.track_with(input, &KickOnsets::from_mono(&input.mono))
    }

    /// [`BeatTracker::track`] with the track's [`KickOnsets`] already made.
    pub fn track_with(
        &mut self,
        input: &AnalysisInput,
        kicks: &KickOnsets,
    ) -> Result<BeatList, BeatError> {
        let mut list = self.track_model(input)?;
        let candidates: Vec<BeatCandidates> = list
            .beats
            .iter()
            .map(|b| beat_candidates(&input.mono, kicks, b.sample))
            .collect();
        place_beats(&mut list.beats, &candidates, BeatPlacement::DEFAULT);
        tracing::debug!(
            beats = list.beats.len(),
            downbeats = list.beats.iter().filter(|b| b.downbeat).count(),
            confidence = list.confidence,
            "beats tracked"
        );
        Ok(list)
    }

    /// The model's beats, refined on the activation only (not placed on
    /// the audio): what [`beat_candidates`] starts from. Blocking.
    pub fn track_model(&mut self, input: &AnalysisInput) -> Result<BeatList, BeatError> {
        if input.beat_mono.is_empty() {
            return Ok(to_beat_list(&[], &[], &[]));
        }
        let analysis = self
            .model
            .analyze_audio(&input.beat_mono, BEAT_RATE)
            .map_err(|e| BeatError::Inference(format!("{e:#}")))?;
        Ok(to_beat_list(
            &analysis.beats,
            &analysis.downbeats,
            &analysis.beat_logits,
        ))
    }
}

/// Where one beat could go (samples at [`ANALYSIS_RATE`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BeatCandidates {
    /// The model's beat ([`BeatTracker::track_model`]).
    pub model: i64,
    /// [`snap_to_onset`]: sharpest broadband rise within ±30 ms (the model
    /// beat when there is none).
    pub onset: i64,
    /// Strongest low-band ([`KickOnsets`]) rise within ±30 ms of the model
    /// beat and its rise in dB, when it reaches [`MIN_KICK_DB`].
    pub kick: Option<(i64, f32)>,
    /// `kick` moved to the sharpest broadband rise from
    /// [`KICK_REFINE_BEFORE_MS`] before to [`KICK_REFINE_AFTER_MS`] after it
    /// (the attack; the band-pass smears it late), or `kick` itself.
    pub kick_onset: Option<i64>,
}

/// How far before and after a low-band kick its attack is looked for.
pub const KICK_REFINE_BEFORE_MS: u32 = 25;
pub const KICK_REFINE_AFTER_MS: u32 = 5;

/// Every placement of the beat the model put at `model`.
pub fn beat_candidates(mono: &[f32], kicks: &KickOnsets, model: i64) -> BeatCandidates {
    let kick = kicks.kick_near(model, SNAP_RADIUS_MS, MIN_KICK_DB);
    BeatCandidates {
        model,
        onset: snap_to_onset(mono, model),
        kick,
        kick_onset: kick
            .map(|(k, _)| snap_to_onset_in(mono, k, KICK_REFINE_BEFORE_MS, KICK_REFINE_AFTER_MS)),
    }
}

/// Moves each of `beats` to `placement`'s choice of its `candidates`
/// (same order and length). A beat that lands on or before the one before
/// it (two beats snapped onto one transient) is dropped; its downbeat flag
/// passes to the beat kept.
pub fn place_beats(beats: &mut Vec<Beat>, candidates: &[BeatCandidates], placement: BeatPlacement) {
    let mut out: Vec<Beat> = Vec::with_capacity(beats.len());
    for (beat, c) in beats.iter().zip(candidates) {
        let sample = placement.choose(c);
        match out.last_mut() {
            Some(prev) if sample <= prev.sample => prev.downbeat |= beat.downbeat,
            _ => out.push(Beat {
                sample,
                downbeat: beat.downbeat,
            }),
        }
    }
    *beats = out;
}

/// Which of the [`BeatCandidates`] becomes the beat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BeatPlacement {
    /// The model's beat.
    Model,
    /// The broadband onset (analyser version 2).
    Onset,
    /// The low-band kick, else the model's beat.
    KickOrModel,
    /// The low-band kick, else the broadband onset.
    KickOrOnset,
    /// The kick's attack, else the model's beat.
    KickOnsetOrModel,
    /// The kick's attack, else the broadband onset.
    KickOnsetOrOnset,
}

impl BeatPlacement {
    /// What [`BeatTracker::track`] uses.
    ///
    /// The broadband onset: on the parity library the kick placements put
    /// more beats on rekordbox's grid (76% vs 74% within 10 ms) but fewer
    /// grids in phase with it (91% vs 94% within 30 ms) (task 12a). The kick
    /// is used for the grid's phase instead ([`crate::grid::kick_phase`]).
    pub const DEFAULT: Self = Self::Onset;

    pub const ALL: [Self; 6] = [
        Self::Model,
        Self::Onset,
        Self::KickOrModel,
        Self::KickOrOnset,
        Self::KickOnsetOrModel,
        Self::KickOnsetOrOnset,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Model => "model",
            Self::Onset => "onset",
            Self::KickOrModel => "kick-model",
            Self::KickOrOnset => "kick-onset",
            Self::KickOnsetOrModel => "kickattack-model",
            Self::KickOnsetOrOnset => "kickattack-onset",
        }
    }

    pub fn choose(self, c: &BeatCandidates) -> i64 {
        let kick = c.kick.map(|(k, _)| k);
        match self {
            Self::Model => c.model,
            Self::Onset => c.onset,
            Self::KickOrModel => kick.unwrap_or(c.model),
            Self::KickOrOnset => kick.unwrap_or(c.onset),
            Self::KickOnsetOrModel => c.kick_onset.unwrap_or(c.model),
            Self::KickOnsetOrOnset => c.kick_onset.unwrap_or(c.onset),
        }
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
pub fn snap_to_onset(mono: &[f32], sample: i64) -> i64 {
    snap_to_onset_in(mono, sample, SNAP_RADIUS_MS, SNAP_RADIUS_MS)
}

/// [`snap_to_onset`] over `before_ms` before to `after_ms` after `sample`.
fn snap_to_onset_in(mono: &[f32], sample: i64, before_ms: u32, after_ms: u32) -> i64 {
    let ms = |n: u32| (ANALYSIS_RATE / 1000 * n) as usize;
    let (after, before) = (ms(AFTER_MS), ms(BEFORE_MS));
    let Ok(centre) = usize::try_from(sample) else {
        return sample;
    };
    let lo = centre.saturating_sub(ms(before_ms));
    let hi = (centre + ms(after_ms)).min(mono.len().saturating_sub(after));
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
    fn placement_keeps_beats_in_order() {
        let beat = |sample, downbeat| Beat { sample, downbeat };
        let cand = |model, onset| BeatCandidates {
            model,
            onset,
            kick: None,
            kick_onset: None,
        };
        let mut beats = vec![beat(100, false), beat(130, true), beat(500, false)];
        // The second beat snaps back onto the first one's transient.
        let c = [cand(100, 110), cand(130, 110), cand(500, 505)];
        place_beats(&mut beats, &c, BeatPlacement::Onset);
        assert_eq!(beats, [beat(110, true), beat(505, false)]);
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
