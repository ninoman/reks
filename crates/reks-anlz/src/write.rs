//! ANLZ section writers: big-endian, explicit offsets. Layout from
//! crate-digger `rekordbox_anlz.ksy` / djl-analysis "Analysis Files" (R§3.3)
//! and checked against the rekordbox 6 export that rekordcrate 0.3.0 ships
//! as test data (`data/complete_export/demo_tracks`).
//!
//! Every section is `tag[4]`, `len_header: u32` (12 + the tag-specific
//! header fields), `len_tag: u32` (`len_header` + payload), header fields,
//! payload. [`file`] wraps sections in the `PMAI` file header.
//!
//! Why hand-written instead of `rekordcrate::anlz` types (D9 amendment):
//! in 0.3.0 the waveform and cue structs keep their `unknown` fields private,
//! so they can't be constructed outside the crate; there are no PWV6/PWV7/
//! PWVC/PQT2 types at all, and its `CueType` has no value 1 (the point cue
//! rekordbox writes). rekordcrate stays the independent parser for
//! round-trip tests.
//!
//! Cue layouts (`PCOB`/`PCPT`, `PCO2`/`PCP2`) and the empty `PQT2` follow
//! `baken-export` 4.6.1 (MIT, `src/anlz/cues.rs`, `generate/grid.rs`), whose
//! tests pin them byte for byte against real rekordbox 7 exports.

/// File header tag.
pub const PMAI: [u8; 4] = *b"PMAI";
/// Audio file path (first section of every file).
pub const PPTH: [u8; 4] = *b"PPTH";
/// MP3 seek table (`.DAT`).
pub const PVBR: [u8; 4] = *b"PVBR";
/// Beat grid (`.DAT`).
pub const PQTZ: [u8; 4] = *b"PQTZ";
/// Extended beat grid (`.EXT`); we only write the empty form.
pub const PQT2: [u8; 4] = *b"PQT2";
/// Cue list (`.DAT` and `.EXT`).
pub const PCOB: [u8; 4] = *b"PCOB";
/// Extended cue list (`.EXT`).
pub const PCO2: [u8; 4] = *b"PCO2";
/// Cue entry inside `PCOB`.
pub const PCPT: [u8; 4] = *b"PCPT";
/// Cue entry inside `PCO2`.
pub const PCP2: [u8; 4] = *b"PCP2";
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

/// `PQTZ` header word after the zero word (crate-digger: 0x00080000).
pub const PQTZ_UNKNOWN: u32 = 0x0008_0000;
/// `PVBR` seek entries.
pub const PVBR_ENTRIES: usize = 400;
/// `PCPT` entry length.
pub const PCPT_LEN: usize = 0x38;
/// `PCP2` entry length without a comment.
pub const PCP2_BASE_LEN: usize = 0x58;
/// Cue list type: memory cues and loops.
pub const LIST_MEMORY: u32 = 0;
/// Cue list type: hot cues and hot loops.
pub const LIST_HOT: u32 = 1;
/// `loop_time` of a cue that is not a loop, and `memory_count` of a list
/// without memory cues.
pub const NONE_U32: u32 = 0xFFFF_FFFF;
/// `PCPT` status of the active loop.
pub const STATUS_ACTIVE_LOOP: u32 = 4;
/// `PCPT` word after the status (rekordbox writes 0x00010000; rekordcrate
/// and djl-analysis say 0x00100000).
pub const PCPT_UNKNOWN: u32 = 0x0001_0000;
/// The three bytes after the cue type in `PCPT`/`PCP2`.
pub const CUE_TYPE_TAIL: [u8; 3] = [0x00, 0x03, 0xE8];
/// Bytes after the hot cue colour in a `PCP2` entry (zeros; rekordbox may
/// put FLAC seek data there).
pub const PCP2_TAIL_LEN: usize = 40;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WriteError {
    #[error("{tag} section is too large for a 32-bit length ({len} bytes)")]
    TooLarge { tag: String, len: usize },
    #[error("{tag} section has too many entries for a 16-bit count ({count})")]
    TooManyEntries { tag: String, count: usize },
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

/// PPTH: header `len_path`; UTF-16BE path plus a NUL.
pub fn ppth(path: &str) -> Result<Vec<u8>, WriteError> {
    let mut payload: Vec<u8> = path.encode_utf16().flat_map(u16::to_be_bytes).collect();
    payload.extend([0, 0]);
    let len = to_u32(PPTH, payload.len())?;
    section(PPTH, &len.to_be_bytes(), &payload)
}

/// PVBR: one zero header word, [`PVBR_ENTRIES`] zero seek entries and a
/// trailer word. For MP3 the trailer is `audio frames × 1152` (the form
/// rekordbox writes for CBR files, which baken's `--cdjsafe` writes for every
/// MP3; both reference tracks have it); other formats are all zero. A real
/// VBR seek table is not written (UNVERIFIED whether players need one).
pub fn pvbr(mp3_audio_frames: Option<u32>) -> Result<Vec<u8>, WriteError> {
    let mut payload = vec![0u8; 4 * PVBR_ENTRIES + 4];
    if let Some(frames) = mp3_audio_frames {
        let at = 4 * PVBR_ENTRIES;
        payload[at..].copy_from_slice(&frames.saturating_mul(1152).to_be_bytes());
    }
    section(PVBR, &0u32.to_be_bytes(), &payload)
}

/// One `PQTZ` entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PqtzBeat {
    /// Beat in the bar, 1–4.
    pub beat_number: u16,
    /// BPM × 100.
    pub tempo: u16,
    /// Milliseconds from the start of the track.
    pub time_ms: u32,
}

/// PQTZ: header zero word, [`PQTZ_UNKNOWN`], `len_beats`; 8 bytes a beat.
pub fn pqtz(beats: &[PqtzBeat]) -> Result<Vec<u8>, WriteError> {
    let count = to_u32(PQTZ, beats.len())?;
    let mut payload = Vec::with_capacity(8 * beats.len());
    for b in beats {
        payload.extend(b.beat_number.to_be_bytes());
        payload.extend(b.tempo.to_be_bytes());
        payload.extend(b.time_ms.to_be_bytes());
    }
    section(PQTZ, &words(&[0, PQTZ_UNKNOWN, count]), &payload)
}

/// PQT2 as rekordbox writes it without an extended grid: `len_header` =
/// `len_tag` = 0x38, header word 2 = 0x01000002, the rest zero.
pub fn pqt2_empty() -> Result<Vec<u8>, WriteError> {
    let mut fields = [0u8; 0x38 - SECTION_HEADER_LEN];
    fields[4..8].copy_from_slice(&0x0100_0002u32.to_be_bytes());
    section(PQT2, &fields, &[])
}

/// One cue as the cue sections store it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CueEntry {
    /// 0 for a memory cue, 1 = A … 8 = H.
    pub hot_cue: u32,
    /// The active loop (`PCPT` status 4).
    pub active_loop: bool,
    /// Start, ms.
    pub time_ms: u32,
    /// Loop end, ms; `None` for a point cue.
    pub loop_end_ms: Option<u32>,
    /// `PCP2` memory cue colour id (0 = none).
    pub color_id: u8,
    /// Quantised loop length in beats as numerator/denominator, or (0, 0).
    pub loop_fraction: (u16, u16),
    /// `PCP2` comment; empty writes no string.
    pub comment: String,
    /// `PCP2` hot cue colour: palette code then the player's RGB bytes.
    pub hot_color: [u8; 4],
}

impl CueEntry {
    /// `PCPT`/`PCP2` cue type: 1 point, 2 loop.
    pub fn cue_type(&self) -> u8 {
        if self.loop_end_ms.is_some() {
            2
        } else {
            1
        }
    }

    fn loop_time(&self) -> u32 {
        self.loop_end_ms.unwrap_or(NONE_U32)
    }
}

fn to_u16(tag: [u8; 4], count: usize) -> Result<u16, WriteError> {
    u16::try_from(count).map_err(|_| WriteError::TooManyEntries {
        tag: tag_name(tag),
        count,
    })
}

/// PCOB: header list type, zero u16, `len_cues` u16, `memory_count`
/// (memory list: cues − 1; otherwise [`NONE_U32`]); 56-byte `PCPT` entries.
/// Memory cues chain to their neighbours by list index (`0xFFFF` at the
/// ends); hot cues carry `0xFFFF` both ways.
pub fn pcob(list_type: u32, cues: &[CueEntry]) -> Result<Vec<u8>, WriteError> {
    let n = to_u16(PCOB, cues.len())?;
    let memory_count = if list_type == LIST_MEMORY && n > 0 {
        u32::from(n) - 1
    } else {
        NONE_U32
    };
    let mut header = list_type.to_be_bytes().to_vec();
    header.extend(0u16.to_be_bytes());
    header.extend(n.to_be_bytes());
    header.extend(memory_count.to_be_bytes());
    let mut payload = Vec::with_capacity(PCPT_LEN * cues.len());
    for (i, c) in cues.iter().enumerate() {
        let (prev, next) = if list_type == LIST_MEMORY {
            // i < n ≤ u16::MAX, so both indices fit.
            let prev = if i == 0 { 0xFFFF } else { (i - 1) as u16 };
            let next = if i + 1 == cues.len() {
                0xFFFF
            } else {
                (i + 1) as u16
            };
            (prev, next)
        } else {
            (0xFFFF, 0xFFFF)
        };
        let status = if c.active_loop { STATUS_ACTIVE_LOOP } else { 0 };
        payload.extend(PCPT);
        payload.extend(words(&[
            0x1C,
            PCPT_LEN as u32,
            c.hot_cue,
            status,
            PCPT_UNKNOWN,
        ]));
        payload.extend(prev.to_be_bytes());
        payload.extend(next.to_be_bytes());
        payload.push(c.cue_type());
        payload.extend(CUE_TYPE_TAIL);
        payload.extend(words(&[c.time_ms, c.loop_time(), 0, 0, 0, 0]));
    }
    section(PCOB, &header, &payload)
}

/// PCO2: header list type, `len_cues` u16, zero u16; `PCP2` entries of
/// [`PCP2_BASE_LEN`] bytes plus the comment (UTF-16BE with NUL; length 0
/// and no string when empty, as rekordbox writes it).
pub fn pco2(list_type: u32, cues: &[CueEntry]) -> Result<Vec<u8>, WriteError> {
    let n = to_u16(PCO2, cues.len())?;
    let mut header = list_type.to_be_bytes().to_vec();
    header.extend(n.to_be_bytes());
    header.extend(0u16.to_be_bytes());
    let mut payload = Vec::new();
    for c in cues {
        let comment: Vec<u8> = if c.comment.is_empty() {
            Vec::new()
        } else {
            c.comment
                .encode_utf16()
                .chain(std::iter::once(0))
                .flat_map(u16::to_be_bytes)
                .collect()
        };
        let len = PCP2_BASE_LEN + comment.len();
        let start = payload.len();
        payload.extend(PCP2);
        payload.extend(words(&[0x10, to_u32(PCO2, len)?, c.hot_cue]));
        payload.push(c.cue_type());
        payload.extend(CUE_TYPE_TAIL);
        payload.extend(words(&[c.time_ms, c.loop_time()]));
        payload.extend([c.color_id, 0x01, 0, 0, 0, 0, 0, 0]);
        payload.extend(c.loop_fraction.0.to_be_bytes());
        payload.extend(c.loop_fraction.1.to_be_bytes());
        payload.extend(to_u32(PCO2, comment.len())?.to_be_bytes());
        payload.extend(&comment);
        payload.extend(c.hot_color);
        payload.resize(start + len, 0);
    }
    section(PCO2, &header, &payload)
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

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    fn point(hot_cue: u32, time_ms: u32) -> CueEntry {
        CueEntry {
            hot_cue,
            active_loop: false,
            time_ms,
            loop_end_ms: None,
            color_id: 0,
            loop_fraction: (0, 0),
            comment: String::new(),
            hot_color: [0, 0x1A, 0xFF, 0x00],
        }
    }

    /// Hot cues B@15.280 and C@30.280 from a real rekordbox 7 export
    /// (bytes as pinned by baken-export's `unreal_hot_cues_match_fixture_bytes`).
    #[test]
    fn hot_cue_sections_match_rekordbox_bytes() {
        let cues = [point(3, 30_280), point(2, 15_280)];
        let s = pcob(LIST_HOT, &cues).unwrap();
        assert_eq!(
            &s[..24],
            &hex("50434f4200000018000000880000000100000002ffffffff")[..]
        );
        assert_eq!(&s[24..80], &hex("504350540000001c00000038000000030000000000010000ffffffff010003e800007648ffffffff00000000000000000000000000000000")[..]);
        let e = pco2(LIST_HOT, &cues).unwrap();
        assert_eq!(
            &e[..20],
            &hex("50434f3200000014000000c40000000100020000")[..]
        );
        assert_eq!(&e[20..108], &hex("50435032000000100000005800000003010003e800007648ffffffff00010000000000000000000000000000001aff0000000000000000000000000000000000000000000000000000000000000000000000000000000000")[..]);
    }

    /// A memory cue at 0.253 and the active loop 121.155–126.904 from a real
    /// rekordbox 7 export (baken-export `active_loop_matches_fixture_bytes`).
    #[test]
    fn memory_list_with_active_loop_matches_rekordbox_bytes() {
        let lp = CueEntry {
            active_loop: true,
            loop_end_ms: Some(126_904),
            ..point(0, 121_155)
        };
        let s = pcob(LIST_MEMORY, &[lp, point(0, 253)]).unwrap();
        assert_eq!(s, hex("50434f420000001800000088000000000000000200000001504350540000001c00000038000000000000000400010000ffff0001020003e80001d9430001efb800000000000000000000000000000000504350540000001c000000380000000000000000000100000000ffff010003e8000000fdffffffff00000000000000000000000000000000"));
    }

    #[test]
    fn empty_cue_lists_match_reference_files() {
        // Both rekordbox 6 reference tracks: no cues.
        assert_eq!(
            pcob(LIST_HOT, &[]).unwrap(),
            hex("50434f42000000180000001800000001 00000000ffffffff"
                .replace(' ', "")
                .as_str())
        );
        assert_eq!(
            pcob(LIST_MEMORY, &[]).unwrap(),
            hex("50434f4200000018000000180000000000000000ffffffff")
        );
        assert_eq!(
            pco2(LIST_HOT, &[]).unwrap(),
            hex("50434f32000000140000001400000001 00000000"
                .replace(' ', "")
                .as_str())
        );
    }

    #[test]
    fn memory_chain_and_comment() {
        let cues: Vec<CueEntry> = [281, 15_281, 150_281].map(|t| point(0, t)).into();
        let s = pcob(LIST_MEMORY, &cues).unwrap();
        assert_eq!(&s[12..24], &hex("000000000000000300000002")[..]);
        let entry = |i: usize| &s[24 + 56 * i..24 + 56 * (i + 1)];
        assert_eq!(&entry(0)[24..28], &hex("ffff0001")[..]);
        assert_eq!(&entry(1)[24..28], &hex("00000002")[..]);
        assert_eq!(&entry(2)[24..28], &hex("0001ffff")[..]);
        let c = CueEntry {
            comment: "1.1Bars".into(),
            hot_color: [0; 4],
            ..point(0, 80)
        };
        let e = pco2(LIST_MEMORY, &[c]).unwrap();
        assert_eq!(e.len(), 20 + 104);
        assert_eq!(
            &e[20 + 40..20 + 64],
            &hex("000000100031002e00310042006100720073000000000000")[..]
        );
    }

    #[test]
    fn grid_path_and_seek_sections() {
        let s = pqtz(&[
            PqtzBeat {
                beat_number: 1,
                tempo: 12_800,
                time_ms: 25,
            },
            PqtzBeat {
                beat_number: 2,
                tempo: 12_800,
                time_ms: 494,
            },
        ])
        .unwrap();
        // Header and first two entries of reference track 1's PQTZ (368 beats there).
        assert_eq!(&s[..12], &hex("5051545a000000180000002800")[..12]);
        assert_eq!(&s[12..24], &hex("000000000008000000000002")[..]);
        assert_eq!(
            &s[24..],
            &hex("0001320000000019000232000000 01ee"
                .replace(' ', "")
                .as_str())[..]
        );

        let p = ppth("/Contents/Loopmasters/UnknownAlbum/Demo Track 1.mp3").unwrap();
        assert_eq!(
            &p[..16],
            &hex("505054480000001000000078 00000068"
                .replace(' ', "")
                .as_str())[..]
        );
        assert_eq!(&p[p.len() - 4..], &[0, b'3', 0, 0]);

        let v = pvbr(Some(6601)).unwrap();
        assert_eq!(v.len(), 1620);
        assert_eq!(
            &v[..16],
            &hex("5056425200000010000006540000 0000"
                .replace(' ', "")
                .as_str())[..]
        );
        assert_eq!(&v[1616..], &hex("00740880")[..]);
        assert!(v[16..1616].iter().all(|&b| b == 0));
        assert!(pvbr(None).unwrap()[16..].iter().all(|&b| b == 0));

        let q = pqt2_empty().unwrap();
        assert_eq!(q.len(), 56);
        assert_eq!(&q[..20], b"PQT2\0\0\0\x38\0\0\0\x38\0\0\0\0\x01\0\0\x02");
        assert!(q[20..].iter().all(|&b| b == 0));
    }
}
