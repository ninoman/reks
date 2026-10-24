//! reks analysis: BPM/beatgrid, key and waveform generation (Phase 3)
//!
//! See `docs/phases/phase-3.md`.

pub mod bar;
pub mod beats;
pub mod cache;
pub mod calib;
pub mod columns;
pub mod detail;
pub mod grid;
pub mod input;
pub mod key;
pub mod onset;
pub mod pipeline;
pub mod queue;
pub mod transients;
pub mod waveform;

pub use beats::{
    beat_candidates, place_beats, Beat, BeatCandidates, BeatError, BeatList, BeatPlacement,
    BeatTracker, ModelPaths,
};
pub use input::{AnalysisInput, InputError, ANALYSIS_RATE, BEAT_RATE};
pub use pipeline::{
    analyse_file, analyse_input, analyse_tracked, AnalyseError, TrackAnalysis, ANALYSER_VERSION,
};
pub use queue::{AnalysisQueue, QueueError, QueueEvent, QueueOptions, QueueStatus};

/// Crate version, from Cargo metadata.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_set() {
        assert!(!super::VERSION.is_empty());
    }
}
