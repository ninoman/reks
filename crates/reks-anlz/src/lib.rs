//! reks ANLZ: Pioneer ANLZ model plus writer/reader helpers (Phase 2/3)
//!
//! So far: [`read`], the section reader export validation uses (Phase 2
//! task 8). The writer lands in Phase 3 (D9).

pub mod read;

/// Crate version, from Cargo metadata.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_set() {
        assert!(!super::VERSION.is_empty());
    }
}
