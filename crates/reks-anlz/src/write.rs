//! ANLZ section writers: big-endian, explicit offsets. Layout from
//! crate-digger `rekordbox_anlz.ksy` / djl-analysis "Analysis Files" (R§3.3)
//! and checked against the rekordbox 6 export that rekordcrate 0.3.0 ships
//! as test data (`data/complete_export/demo_tracks`).
//!
//! Every section is `tag[4]`, `len_header: u32` (12 + the tag-specific
//! header fields), `len_tag: u32` (`len_header` + payload), header fields,
//! payload. [`file`] wraps sections in the `PMAI` file header.
//!
//! Why hand-written instead of `rekordcrate::anlz` types (D9 amendment,
//! proposed): in 0.3.0 the waveform structs keep their `unknown` header
//! fields private, so they can't be constructed outside the crate, and there
//! are no PWV6/PWV7/PWVC types at all. rekordcrate stays the independent
//! parser for round-trip tests.

/// File header tag.
pub const PMAI: [u8; 4] = *b"PMAI";
/// Monochrome preview, 400 columns (`.DAT`).
pub const PWAV: [u8; 4] = *b"PWAV";
/// Tiny monochrome preview, 100 columns (`.DAT`, CDJ-900).
pub const PWV2: [u8; 4] = *b"PWV2";
/// Monochrome detail, 150 columns/s (`.EXT`).
pub const PWV3: [u8; 4] = *b"PWV3";
/// Colour preview, 1200 × 6 bytes (`.EXT`).
pub const PWV4: [u8; 4] = *b"PWV4";
/// Colour detail, 150 columns/s (`.EXT`).
pub const PWV5: [u8; 4] = *b"PWV5";
/// 3-band preview, 1200 × 3 bytes (`.2EX`).
pub const PWV6: [u8; 4] = *b"PWV6";
/// 3-band detail, 150 columns/s (`.2EX`).
pub const PWV7: [u8; 4] = *b"PWV7";
/// 3-band calibration (`.2EX`), semantics UNVERIFIED.
pub const PWVC: [u8; 4] = *b"PWVC";

/// Bytes of the common section header (tag, `len_header`, `len_tag`).
pub const SECTION_HEADER_LEN: usize = 12;
/// `PMAI` `len_header`.
pub const PMAI_HEADER_LEN: usize = 0x1C;
/// Bytes 12..28 of `PMAI` in every rekordbox file we have seen
/// (rekordbox 6 reference export); meaning unknown.
pub const PMAI_HEADER_TAIL: [u8; 16] = [0, 0, 0, 1, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0];

/// PWAV/PWV2 header word after `len_preview`. djl-analysis and crate-digger
/// say 0x00100000; the rekordbox 6 reference files hold 0x00010000.
pub const PREVIEW_UNKNOWN: u32 = 0x0001_0000;
/// PWV3 and PWV7 header word after `len_entries` (0x96 = 150 columns/s?).
pub const DETAIL_UNKNOWN: u32 = 0x0096_0000;
/// PWV5 header word after `len_entries`, as in the reference files.
pub const COLOR_DETAIL_UNKNOWN: u32 = 0x0096_0305;
/// PWV4 header word after `len_entries`, as in the reference files.
pub const COLOR_PREVIEW_UNKNOWN: u32 = 0;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WriteError {
    #[error("{tag} section is too large for a 32-bit length ({len} bytes)")]
    TooLarge { tag: String, len: usize },
}

fn tag_name(tag: [u8; 4]) -> String {
    String::from_utf8_lossy(&tag).into_owned()
}

fn to_u32(tag: [u8; 4], len: usize) -> Result<u32, WriteError> {
    u32::try_from(len).map_err(|_| WriteError::TooLarge {
        tag: tag_name(tag),
        len,
    })
}

fn put_u32(buf: &mut [u8], at: usize, v: u32) {
    buf[at..at + 4].copy_from_slice(&v.to_be_bytes());
}

/// One section: common header, `header_fields`, `payload`.
pub fn section(tag: [u8; 4], header_fields: &[u8], payload: &[u8]) -> Result<Vec<u8>, WriteError> {
    let len_header = SECTION_HEADER_LEN + header_fields.len();
    let len_tag = len_header + payload.len();
    let (len_header_u32, len_tag_u32) = (to_u32(tag, len_header)?, to_u32(tag, len_tag)?);
    let mut out = vec![0u8; len_tag];
    out[0..4].copy_from_slice(&tag);
    put_u32(&mut out, 4, len_header_u32);
    put_u32(&mut out, 8, len_tag_u32);
    out[SECTION_HEADER_LEN..len_header].copy_from_slice(header_fields);
    out[len_header..].copy_from_slice(payload);
    Ok(out)
}

fn words(ws: &[u32]) -> Vec<u8> {
    ws.iter().flat_map(|w| w.to_be_bytes()).collect()
}

/// `PMAI` file: header (`len_header` 0x1C, `len_file`) then the sections.
pub fn file(sections: &[Vec<u8>]) -> Result<Vec<u8>, WriteError> {
    let len_file = PMAI_HEADER_LEN + sections.iter().map(Vec::len).sum::<usize>();
    let len_file_u32 = to_u32(PMAI, len_file)?;
    let mut out = Vec::with_capacity(len_file);
    out.extend_from_slice(&PMAI);
    out.extend_from_slice(&words(&[PMAI_HEADER_LEN as u32, len_file_u32]));
    out.extend_from_slice(&PMAI_HEADER_TAIL);
    for s in sections {
        out.extend_from_slice(s);
    }
    Ok(out)
}

fn preview(tag: [u8; 4], data: &[u8]) -> Result<Vec<u8>, WriteError> {
    let len = to_u32(tag, data.len())?;
    section(tag, &words(&[len, PREVIEW_UNKNOWN]), data)
}

/// PWAV: header `len_preview`, unknown; one packed byte per column.
pub fn pwav(columns: &[u8]) -> Result<Vec<u8>, WriteError> {
    preview(PWAV, columns)
}

/// PWV2: same layout as PWAV, 4-bit heights.
pub fn pwv2(columns: &[u8]) -> Result<Vec<u8>, WriteError> {
    preview(PWV2, columns)
}

/// Header `len_entry_bytes`, `len_entries` and (when given) the unknown word.
fn entries(
    tag: [u8; 4],
    entry_bytes: u32,
    count: usize,
    unknown: Option<u32>,
    payload: &[u8],
) -> Result<Vec<u8>, WriteError> {
    let count = to_u32(tag, count)?;
    let header = match unknown {
        Some(u) => words(&[entry_bytes, count, u]),
        None => words(&[entry_bytes, count]),
    };
    section(tag, &header, payload)
}

/// PWV3: 1-byte entries at 150/s.
pub fn pwv3(columns: &[u8]) -> Result<Vec<u8>, WriteError> {
    entries(PWV3, 1, columns.len(), Some(DETAIL_UNKNOWN), columns)
}

/// PWV4: 6-byte entries (normally 1200).
pub fn pwv4(columns: &[[u8; 6]]) -> Result<Vec<u8>, WriteError> {
    let payload: Vec<u8> = columns.iter().flatten().copied().collect();
    entries(
        PWV4,
        6,
        columns.len(),
        Some(COLOR_PREVIEW_UNKNOWN),
        &payload,
    )
}

/// PWV5: big-endian u16 entries at 150/s.
pub fn pwv5(columns: &[u16]) -> Result<Vec<u8>, WriteError> {
    let payload: Vec<u8> = columns.iter().flat_map(|c| c.to_be_bytes()).collect();
    entries(PWV5, 2, columns.len(), Some(COLOR_DETAIL_UNKNOWN), &payload)
}

/// PWV6: 3-byte entries (normally 1200); no unknown header word.
pub fn pwv6(columns: &[[u8; 3]]) -> Result<Vec<u8>, WriteError> {
    let payload: Vec<u8> = columns.iter().flatten().copied().collect();
    entries(PWV6, 3, columns.len(), None, &payload)
}

/// PWV7: 3-byte entries at 150/s.
pub fn pwv7(columns: &[[u8; 3]]) -> Result<Vec<u8>, WriteError> {
    let payload: Vec<u8> = columns.iter().flatten().copied().collect();
    entries(PWV7, 3, columns.len(), Some(DETAIL_UNKNOWN), &payload)
}

/// PWVC: `len_header` 14 (one zero u16 header field), three big-endian u16
/// values. Semantics UNVERIFIED.
pub fn pwvc(values: [u16; 3]) -> Result<Vec<u8>, WriteError> {
    let payload: Vec<u8> = values.iter().flat_map(|v| v.to_be_bytes()).collect();
    section(PWVC, &[0, 0], &payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hdr(s: &[u8]) -> (&[u8], u32, u32, &[u8]) {
        let lh = u32::from_be_bytes([s[4], s[5], s[6], s[7]]);
        let lt = u32::from_be_bytes([s[8], s[9], s[10], s[11]]);
        (&s[0..4], lh, lt, &s[12..lh as usize])
    }

    /// Section headers of the rekordbox 6 reference export (rekordcrate
    /// test data, track P016/0000875E: 25866 detail columns).
    #[test]
    fn headers_match_rekordbox_reference() {
        let n = 25_866;
        type Case = (Vec<u8>, &'static [u8; 4], u32, u32, &'static str);
        let cases: [Case; 8] = [
            (
                pwav(&[0; 400]).unwrap(),
                b"PWAV",
                20,
                420,
                "0000019000010000",
            ),
            (
                pwv2(&[0; 100]).unwrap(),
                b"PWV2",
                20,
                120,
                "0000006400010000",
            ),
            (
                pwv3(&vec![0; n]).unwrap(),
                b"PWV3",
                24,
                25_890,
                "000000010000650a00960000",
            ),
            (
                pwv5(&vec![0; n]).unwrap(),
                b"PWV5",
                24,
                51_756,
                "000000020000650a00960305",
            ),
            (
                pwv4(&[[0; 6]; 1200]).unwrap(),
                b"PWV4",
                24,
                7_224,
                "00000006000004b000000000",
            ),
            (
                pwv7(&vec![[0; 3]; n]).unwrap(),
                b"PWV7",
                24,
                77_622,
                "000000030000650a00960000",
            ),
            (
                pwv6(&[[0; 3]; 1200]).unwrap(),
                b"PWV6",
                20,
                3_620,
                "00000003000004b0",
            ),
            (pwvc([0x5b, 0x8a, 0x18c]).unwrap(), b"PWVC", 14, 20, "0000"),
        ];
        for (bytes, tag, len_header, len_tag, fields) in cases {
            let (t, lh, lt, f) = hdr(&bytes);
            assert_eq!(t, tag);
            assert_eq!((lh, lt), (len_header, len_tag), "{}", tag_name(*tag));
            assert_eq!(lt as usize, bytes.len());
            let hex: String = f.iter().map(|b| format!("{b:02x}")).collect();
            assert_eq!(hex, fields, "{}", tag_name(*tag));
        }
    }

    #[test]
    fn len_tag_arithmetic() {
        for n in [0usize, 1, 2, 149, 150, 151, 40_000] {
            let s = pwv3(&vec![7; n]).unwrap();
            let (_, lh, lt, _) = hdr(&s);
            assert_eq!((lh, lt as usize), (24, 24 + n));
            let s = pwv5(&vec![0xabcd; n]).unwrap();
            assert_eq!(hdr(&s).2 as usize, 24 + 2 * n);
            let s = pwv7(&vec![[1, 2, 3]; n]).unwrap();
            assert_eq!(hdr(&s).2 as usize, 24 + 3 * n);
            let s = pwv6(&vec![[1, 2, 3]; n]).unwrap();
            assert_eq!(hdr(&s).2 as usize, 20 + 3 * n);
            let s = pwv4(&vec![[0; 6]; n]).unwrap();
            assert_eq!(hdr(&s).2 as usize, 24 + 6 * n);
        }
        let s = section(*b"TEST", &[1, 2, 3], &[9; 5]).unwrap();
        assert_eq!(
            s,
            [
                b"TEST".as_slice(),
                &[0, 0, 0, 15, 0, 0, 0, 20, 1, 2, 3],
                &[9; 5]
            ]
            .concat()
        );
    }

    #[test]
    fn payload_byte_order() {
        let s = pwv5(&[0xe448, 0x0102]).unwrap();
        assert_eq!(&s[24..], &[0xe4, 0x48, 0x01, 0x02]);
        let s = pwv7(&[[1, 2, 3], [4, 5, 6]]).unwrap();
        assert_eq!(&s[24..], &[1, 2, 3, 4, 5, 6]);
        let s = pwvc([0x005b, 0x008a, 0x018c]).unwrap();
        assert_eq!(&s[14..], &[0x00, 0x5b, 0x00, 0x8a, 0x01, 0x8c]);
    }

    #[test]
    fn file_header() {
        let a = pwv2(&[1; 100]).unwrap();
        let f = file(std::slice::from_ref(&a)).unwrap();
        assert_eq!(&f[0..4], b"PMAI");
        assert_eq!(u32::from_be_bytes([f[4], f[5], f[6], f[7]]), 0x1C);
        assert_eq!(
            u32::from_be_bytes([f[8], f[9], f[10], f[11]]) as usize,
            f.len()
        );
        assert_eq!(&f[12..28], &PMAI_HEADER_TAIL);
        assert_eq!(&f[28..], a.as_slice());
    }
}
