//! reks legacy rekordbox USB export: plan, dry-run, write, validate
//!
//! Phase 2 lands here piece by piece; see `docs/phases/phase-2.md`.

pub mod adopt;
pub mod apply;
pub mod audiocheck;
pub mod device;
pub mod devices;
pub mod divider;
pub mod dryrun;
pub mod history;
pub mod plan;
pub mod rbcache;
pub mod reksanlz;
pub mod report;
pub mod target;
#[cfg(any(test, feature = "test-util"))]
pub mod testutil;
pub mod validate;
pub mod writer;

pub use apply::{apply, ApplyEnv, ApplyError, ApplyEvent, ExportReport, ExportStatus, StickCheck};
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
