//! reks audio: Symphonia decoding, resampling, overview waveform
//!
//! Phase 1: header probe ([`probe()`]), tags ([`tags()`]), embedded artwork
//! ([`embedded_artwork`]), whole-track decode for playback ([`decode_stereo`])
//! and the overview waveform ([`overview`]). See `docs/phases/phase-1.md`.

pub mod decode;
pub mod error;
pub mod overview;
pub mod probe;
pub mod tags;

pub use decode::{decode_stereo, Decoded, StereoPcm, MAX_DECODE_MS};
pub use error::{Error, Result};
pub use probe::{probe, Kind, Probe};
pub use tags::{embedded_artwork, tags, Picture, Tags};

/// Crate version, from Cargo metadata.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_set() {
        assert!(!super::VERSION.is_empty());
    }
}
