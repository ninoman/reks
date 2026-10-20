//! reks ANLZ: Pioneer ANLZ model plus writer/reader helpers (Phase 2/3)
//!
//! So far: [`read`], the section reader export validation uses (Phase 2
//! task 8). [`write`] frames sections; [`waveform`] encodes reks waveform
//! columns into PWAV/PWV2–7 (Phase 3 task 7). The full file builder lands
//! in Phase 3 task 9.

pub mod read;
pub mod waveform;
pub mod write;

/// Crate version, from Cargo metadata.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_set() {
        assert!(!super::VERSION.is_empty());
    }
}
