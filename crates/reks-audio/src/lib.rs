//! reks audio: Symphonia decoding, PCM cache, preview player
//!
//! Phase 1 so far: header probe ([`probe()`]), tags ([`tags()`]) and embedded
//! artwork ([`embedded_artwork`]). See `docs/phases/phase-1.md`.

pub mod error;
pub mod probe;
pub mod tags;

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
