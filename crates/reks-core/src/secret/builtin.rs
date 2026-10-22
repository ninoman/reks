//! The OneLibrary passphrase baked into release builds (D14, amended
//! 2026-10-21). It comes from `REKS_BUILD_ONELIBRARY_KEY` at compile time
//! (a CI secret in the release workflow), so it is never in the repository;
//! a build without that variable has no built-in key.
//!
//! It is stored masked, so the plain passphrase is not a string in the
//! binary that a text search finds. This is obfuscation, not protection:
//! anyone can recover it, as from every other app that ships it.

use zeroize::Zeroizing;

use super::OneLibraryKey;

/// Read when the crate is compiled; cargo rebuilds when it changes.
pub const BUILD_ENV: &str = "REKS_BUILD_ONELIBRARY_KEY";

// Read only by the consts below: any runtime use of `RAW` would emit the
// plain bytes into the binary (the release workflow checks it does not).
const RAW: &[u8] = match option_env!("REKS_BUILD_ONELIBRARY_KEY") {
    Some(key) => key.as_bytes(),
    None => b"",
};
const MASKED: [u8; RAW.len()] = mask(RAW);
const PRESENT: bool = !RAW.is_empty();

/// Whether this build has a built-in key.
pub const fn present() -> bool {
    PRESENT
}

/// The built-in key, if this build has one.
pub fn key() -> Option<OneLibraryKey> {
    let plain = Zeroizing::new(unmask(&MASKED));
    std::str::from_utf8(&plain)
        .ok()
        .and_then(OneLibraryKey::new)
}

/// XOR with an xorshift64 keystream; its own inverse.
const fn mask<const N: usize>(bytes: &[u8]) -> [u8; N] {
    let mut out = [0u8; N];
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut i = 0;
    while i < N {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        out[i] = bytes[i] ^ (state >> 24) as u8;
        i += 1;
    }
    out
}

fn unmask<const N: usize>(masked: &[u8; N]) -> Vec<u8> {
    mask::<N>(masked).to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_round_trips_and_hides_the_text() {
        let masked = mask::<12>(b"not-the-key!");
        assert_ne!(&masked, b"not-the-key!");
        assert_eq!(unmask(&masked), b"not-the-key!");
    }

    #[test]
    fn key_is_the_build_variable() {
        let expected = option_env!("REKS_BUILD_ONELIBRARY_KEY").and_then(OneLibraryKey::new);
        assert_eq!(key(), expected);
        assert_eq!(present(), expected.is_some());
    }
}
