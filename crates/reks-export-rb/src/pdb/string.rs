//! DeviceSQL strings (djl-analysis "DeviceSQL Strings"), decoded with every
//! length checked against the bytes there are.
//!
//! - Short ASCII: header byte `h` with bit 0 set; `h >> 1` is the length
//!   including the header, so the text is `(h >> 1) - 1` bytes. `0x01`
//!   (length 0) is invalid.
//! - Long: a flags byte with bit 0 clear, a u16 length including the
//!   4-byte header, a pad byte, then the body: `0x40` ASCII; `0x90` UCS-2LE,
//!   or an ISRC when the body starts with `0x03` (text up to a NUL).

/// Why a string does not decode.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StringError {
    #[error("string offset {0} is outside the row")]
    Offset(usize),
    #[error("string header {0:#04x} gives no length")]
    EmptyHeader(u8),
    #[error("string of {len} bytes runs past the row ({room} left)")]
    TooLong { len: usize, room: usize },
    #[error("long string length {0} is shorter than its header")]
    ShortLong(u16),
    #[error("unknown string flags {0:#04x}")]
    Flags(u8),
    #[error("string text is not valid {0}")]
    Encoding(&'static str),
}

/// The string at byte `at` of `raw`.
pub fn decode(raw: &[u8], at: usize) -> Result<String, StringError> {
    let tail = raw.get(at..).ok_or(StringError::Offset(at))?;
    let &header = tail.first().ok_or(StringError::Offset(at))?;
    if header & 1 == 1 {
        let len = usize::from(header >> 1);
        if len == 0 {
            return Err(StringError::EmptyHeader(header));
        }
        let text = body(tail, 1, len - 1)?;
        return String::from_utf8(text.to_vec()).map_err(|_| StringError::Encoding("ASCII"));
    }
    let len = tail
        .get(1..3)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .ok_or(StringError::TooLong {
            len: 4,
            room: tail.len(),
        })?;
    let body_len = usize::from(len)
        .checked_sub(4)
        .ok_or(StringError::ShortLong(len))?;
    let text = body(tail, 4, body_len)?;
    match header {
        0x40 => String::from_utf8(text.to_vec()).map_err(|_| StringError::Encoding("ASCII")),
        0x90 if text.first() == Some(&0x03) => {
            let isrc = &text[1..];
            let isrc = isrc.split(|&b| b == 0).next().unwrap_or_default();
            String::from_utf8(isrc.to_vec()).map_err(|_| StringError::Encoding("ISRC"))
        }
        0x90 => {
            if text.len() % 2 != 0 {
                return Err(StringError::Encoding("UCS-2"));
            }
            let units: Vec<u16> = text
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&c| u16::from_le_bytes(c))
                .collect();
            String::from_utf16(&units).map_err(|_| StringError::Encoding("UCS-2"))
        }
        other => Err(StringError::Flags(other)),
    }
}

/// `len` bytes of `tail` from `from`.
fn body(tail: &[u8], from: usize, len: usize) -> Result<&[u8], StringError> {
    let room = tail.len().saturating_sub(from);
    from.checked_add(len)
        .and_then(|end| tail.get(from..end))
        .ok_or(StringError::TooLong { len, room })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_long_ucs2_and_isrc_decode() {
        assert_eq!(decode(&[0x03], 0).unwrap(), "");
        assert_eq!(decode(&[0, 0x09, b'f', b'o', b'o'], 1).unwrap(), "foo");
        let mut long = vec![0x40, 7, 0, 0];
        long.extend_from_slice(b"abc");
        assert_eq!(decode(&long, 0).unwrap(), "abc");
        let ucs2 = [0x90, 8, 0, 0, 0x64, 0x27, b'!', 0];
        assert_eq!(decode(&ucs2, 0).unwrap(), "❤!");
        let isrc = [0x90, 9, 0, 0, 0x03, b'G', b'B', 0, 0];
        assert_eq!(decode(&isrc, 0).unwrap(), "GB");
    }

    #[test]
    fn malformed_headers_are_errors_not_panics() {
        // rekordcrate 0.3.0 underflows on this one (`string.rs:164`).
        assert_eq!(decode(&[0x01], 0), Err(StringError::EmptyHeader(0x01)));
        assert_eq!(decode(&[0x03], 1), Err(StringError::Offset(1)));
        assert_eq!(
            decode(&[0x03], usize::MAX),
            Err(StringError::Offset(usize::MAX))
        );
        assert!(matches!(
            decode(&[0xFF, b'a'], 0),
            Err(StringError::TooLong { len: 126, .. })
        ));
        assert_eq!(decode(&[0x40, 3, 0, 0], 0), Err(StringError::ShortLong(3)));
        assert!(matches!(
            decode(&[0x40, 0xFF, 0xFF, 0], 0),
            Err(StringError::TooLong { .. })
        ));
        assert!(matches!(
            decode(&[0x40], 0),
            Err(StringError::TooLong { .. })
        ));
        assert_eq!(decode(&[0x20, 4, 0, 0], 0), Err(StringError::Flags(0x20)));
        assert_eq!(
            decode(&[0x90, 5, 0, 0, b'a'], 0),
            Err(StringError::Encoding("UCS-2"))
        );
        assert_eq!(
            decode(&[0x05, 0xFF], 0),
            Err(StringError::Encoding("ASCII"))
        );
    }

    proptest::proptest! {
        #[test]
        fn never_panics(raw in proptest::collection::vec(proptest::num::u8::ANY, 0..64), at in 0usize..80) {
            let _ = decode(&raw, at);
        }

        #[test]
        fn short_strings_round_trip(s in "[ -~]{0,120}") {
            let mut raw = vec![(((s.len() + 1) << 1) | 1) as u8];
            raw.extend_from_slice(s.as_bytes());
            proptest::prop_assert_eq!(decode(&raw, 0).unwrap(), s);
        }
    }
}
