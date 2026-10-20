//! reks analysis: BPM/beatgrid, key and waveform generation (Phase 3)
//!
//! See `docs/phases/phase-3.md`.

pub mod beats;
pub mod cache;
pub mod calib;
pub mod columns;
pub mod grid;
pub mod input;
pub mod key;
pub mod pipeline;
pub mod queue;
pub mod waveform;

pub use beats::{Beat, BeatError, BeatList, BeatTracker, ModelPaths};
pub use input::{AnalysisInput, InputError, ANALYSIS_RATE, BEAT_RATE};
pub use pipeline::{analyse_file, analyse_input, AnalyseError, TrackAnalysis, ANALYSER_VERSION};
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
