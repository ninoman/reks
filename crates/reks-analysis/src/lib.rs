//! reks analysis: BPM/beatgrid, key and waveform generation (Phase 3)
//!
//! See `docs/phases/phase-3.md`.

pub mod beats;
pub mod grid;
pub mod input;
pub mod key;
pub mod waveform;

pub use beats::{Beat, BeatError, BeatList, BeatTracker, ModelPaths};
pub use input::{AnalysisInput, InputError, ANALYSIS_RATE, BEAT_RATE};

/// Crate version, from Cargo metadata.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_set() {
        assert!(!super::VERSION.is_empty());
    }
}
