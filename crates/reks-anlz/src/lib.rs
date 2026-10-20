//! reks ANLZ: Pioneer ANLZ files from reks analysis (Phase 3 tasks 7 and 9).
//!
//! [`build`] assembles a track's `.DAT`/`.EXT`/`.2EX` from its beats
//! ([`grid::pqtz_beats`]), cues ([`cue`]) and encoded waveforms
//! ([`waveform::Waveforms`]); [`write`] frames the sections; [`validate`]
//! checks files; [`read`] decodes them (export validation, Phase 2 task 8);
//! [`compare`] measures our files against rekordbox's. Formats:
//! `docs/research.md` §3.3 and the D9 finding in `docs/decisions.md`.

pub mod build;
pub mod compare;
pub mod cue;
pub mod grid;
pub mod read;
pub mod validate;
pub mod waveform;
pub mod write;

pub use build::{build, AnlzInput, AnlzSet, AnlzTarget, BuildError};
pub use validate::{validate_file, validate_set, Expect, FileKind, Issue};
pub use waveform::{WaveformCalibration, Waveforms, PLACEHOLDER_CALIBRATION};
pub use write::{CueEntry, PqtzBeat};

/// Crate version, from Cargo metadata.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_set() {
        assert!(!super::VERSION.is_empty());
    }
}
