//! reks legacy rekordbox USB export: plan, dry-run, write, validate
//!
//! Phase 2 lands here piece by piece; see `docs/phases/phase-2.md`.

pub mod audiocheck;
pub mod device;
pub mod history;
pub mod plan;
pub mod rbcache;
pub mod target;

pub use target::TargetProfile;

/// Crate version, from Cargo metadata.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_set() {
        assert!(!super::VERSION.is_empty());
    }
}
