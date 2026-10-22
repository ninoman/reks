//! The OneLibrary passphrase (Phase 4 key policy, D14): built into release
//! builds ([`builtin`]), or from `REKS_ONELIBRARY_KEY`; never committed,
//! never logged.
//!
//! [`OneLibraryKey`] prints as `<redacted>` in `Debug`, has no `Serialize`
//! (it can't reach the UI or a JSON plan) and wipes its bytes on drop.

pub mod builtin;

use std::fmt;

use zeroize::Zeroizing;

/// The `exportLibrary.db` passphrase. Surrounding whitespace is trimmed; an
/// empty key is not a key ([`Self::new`] returns `None`).
#[derive(Clone, PartialEq, Eq)]
pub struct OneLibraryKey(Zeroizing<String>);

impl OneLibraryKey {
    /// `None` when `raw` is empty or only whitespace.
    pub fn new(raw: &str) -> Option<Self> {
        let trimmed = raw.trim();
        (!trimmed.is_empty()).then(|| Self(Zeroizing::new(trimmed.to_owned())))
    }

    /// The passphrase, for `PRAGMA key` and nothing else.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for OneLibraryKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("OneLibraryKey(<redacted>)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_and_refuses_empty() {
        assert_eq!(OneLibraryKey::new("  abc \n").unwrap().expose(), "abc");
        assert!(OneLibraryKey::new(" \t").is_none());
    }

    #[test]
    fn debug_never_shows_the_key() {
        let key = OneLibraryKey::new("s3cret").unwrap();
        let shown = format!("{key:?} {:?}", Some(&key));
        assert!(!shown.contains("s3cret"), "{shown}");
    }
}
