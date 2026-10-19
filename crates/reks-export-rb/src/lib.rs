//! reks legacy rekordbox USB export: plan, dry-run, write, validate
//!
//! Phase 2 in progress: [`audiocheck`] and [`TargetProfile`] so far. See
//! `docs/phases/phase-2.md` for what lands here.

pub mod audiocheck;
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
