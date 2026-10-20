//! reks analysis: BPM/beatgrid, key and waveform generation (Phase 3)
//!
//! Scaffold only (Phase 1 task 1). See `docs/phases/` for what lands here.

pub mod key;

/// Crate version, from Cargo metadata.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_set() {
        assert!(!super::VERSION.is_empty());
    }
}
