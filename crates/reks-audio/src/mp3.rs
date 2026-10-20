//! MPEG audio frame count of an MP3, as rekordbox puts it in the ANLZ `PVBR`
//! trailer (frames × 1152, phase 3 "Encoding rules").
//!
//! The rule follows baken-export 4.6.1's `rewrite::mp3_audio` (MIT), which
//! matches rekordbox's own files: skip the ID3v2 tag, skip the first frame
//! when it is a Xing/Info (or VBRI) header, then count MPEG-1 Layer III
//! frames until the first byte that is not a frame header (ID3v1, APE tags).
//! Reimplemented here, not copied. Only MPEG-1 Layer III counts: for anything
//! else (MPEG-2/2.5, other layers) the count is `None` and `PVBR` keeps a zero
//! trailer, which is UNVERIFIED on players.

use std::path::Path;

use crate::{Error, Result};

/// Kbps by bitrate index, MPEG-1 Layer III; 0 = free format or invalid.
const BITRATES: [u32; 16] = [
    0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 0,
];
/// Hz by sample-rate index, MPEG-1.
const RATES: [u32; 4] = [44_100, 48_000, 32_000, 0];

/// MPEG audio frames in the MP3 at `path`, without the Xing/Info header
/// frame; `None` when the file holds no MPEG-1 Layer III frames.
pub fn mp3_audio_frames(path: &Path) -> Result<Option<u32>> {
    let data = std::fs::read(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(count_frames(&data))
}

/// One MPEG-1 Layer III frame header at the start of `h`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Header {
    /// Whole frame, header included.
    len: usize,
    mono: bool,
}

fn header(h: &[u8]) -> Option<Header> {
    let [b0, b1, b2, b3] = *h.get(..4)? else {
        return None;
    };
    let sync = b0 == 0xFF && b1 & 0xE0 == 0xE0;
    // Version bits 11 = MPEG-1, layer bits 01 = Layer III.
    let mpeg1_layer3 = (b1 >> 3) & 3 == 3 && (b1 >> 1) & 3 == 1;
    let kbps = BITRATES[usize::from(b2 >> 4)];
    let rate = RATES[usize::from((b2 >> 2) & 3)];
    if !sync || !mpeg1_layer3 || kbps == 0 || rate == 0 {
        return None;
    }
    let padding = u32::from((b2 >> 1) & 1);
    let len = usize::try_from(144_000 * kbps / rate + padding).ok()?;
    Some(Header {
        len,
        mono: b3 >> 6 == 3,
    })
}

/// Bytes of the ID3v2 tag at the start of `data` (0 without one).
fn id3v2_len(data: &[u8]) -> usize {
    match data.get(..10) {
        Some(h) if &h[..3] == b"ID3" => {
            let size = h[6..10]
                .iter()
                .fold(0usize, |acc, &b| (acc << 7) | usize::from(b & 0x7F));
            let footer = if h[5] & 0x10 != 0 { 10 } else { 0 };
            10 + size + footer
        }
        _ => 0,
    }
}

/// Whether the frame at the start of `frame` is a Xing/Info or VBRI header.
fn is_info_frame(frame: &[u8], h: Header) -> bool {
    // Xing/Info sits after the side information (17 bytes mono, 32 stereo).
    let xing_at = 4 + if h.mono { 17 } else { 32 };
    let tag = |at: usize| frame.get(at..at + 4);
    matches!(tag(xing_at), Some(b"Xing" | b"Info")) || tag(36) == Some(b"VBRI")
}

fn count_frames(data: &[u8]) -> Option<u32> {
    let mut pos = id3v2_len(data);
    // The first frame: the first header that the next one follows (or the
    // end of the file), so stray sync bytes in junk before it don't count.
    let first = loop {
        let rest = data.get(pos..)?;
        if rest.len() < 4 {
            return None;
        }
        if let Some(h) = header(rest) {
            let next = rest.get(h.len..).unwrap_or(&[]);
            if next.is_empty() || header(next).is_some() {
                break h;
            }
        }
        pos += 1;
    };
    let mut frames = 0u32;
    if is_info_frame(&data[pos..], first) {
        pos += first.len;
    }
    while let Some(h) = data.get(pos..).and_then(header) {
        frames = frames.saturating_add(1);
        pos += h.len;
    }
    (frames > 0).then_some(frames)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 128 kbps 44.1 kHz stereo frame (417 bytes, no padding).
    fn frame() -> Vec<u8> {
        let mut f = vec![0u8; 417];
        f[..4].copy_from_slice(&[0xFF, 0xFB, 0x90, 0x00]);
        f
    }

    fn info_frame() -> Vec<u8> {
        let mut f = frame();
        f[36..40].copy_from_slice(b"Info");
        f
    }

    #[test]
    fn header_lengths() {
        assert_eq!(header(&frame()).map(|h| h.len), Some(417));
        // Padding bit adds a byte; 320 kbps 48 kHz is 960 bytes.
        assert_eq!(header(&[0xFF, 0xFB, 0x92, 0x00]).map(|h| h.len), Some(418));
        assert_eq!(header(&[0xFF, 0xFB, 0xE4, 0x00]).map(|h| h.len), Some(960));
        // MPEG-2 Layer III, Layer II, free format, bad rate: not counted.
        assert_eq!(header(&[0xFF, 0xF3, 0x90, 0x00]), None);
        assert_eq!(header(&[0xFF, 0xFD, 0x90, 0x00]), None);
        assert_eq!(header(&[0xFF, 0xFB, 0x00, 0x00]), None);
        assert_eq!(header(&[0xFF, 0xFB, 0x9C, 0x00]), None);
    }

    #[test]
    fn counts_audio_frames_only() {
        let mut data = Vec::new();
        // ID3v2 tag of 20 bytes (syncsafe size), with a sync-like byte pair.
        data.extend_from_slice(b"ID3\x04\x00\x00\x00\x00\x00\x14");
        data.extend_from_slice(&[0xFF, 0xFB, 0x90, 0x00]);
        data.extend_from_slice(&[0; 16]);
        data.extend(info_frame());
        for _ in 0..5 {
            data.extend(frame());
        }
        data.extend_from_slice(b"TAG");
        data.extend_from_slice(&[0; 125]);
        assert_eq!(count_frames(&data), Some(5));

        // Without the Info frame every frame counts; junk before the first
        // frame is skipped.
        let mut plain = vec![0x00, 0xFF, 0x12];
        for _ in 0..3 {
            plain.extend(frame());
        }
        assert_eq!(count_frames(&plain), Some(3));
        assert_eq!(count_frames(b"not an mp3 at all"), None);
        assert_eq!(count_frames(&[]), None);
    }

    #[test]
    fn fixture_matches_its_info_header() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/audio/sine-click-128.mp3");
        let data = std::fs::read(&path).unwrap();
        let frames = mp3_audio_frames(&path).unwrap().unwrap();
        // LAME's Info header holds the frame count after the flags word.
        let at = data.windows(4).position(|w| w == b"Info" || w == b"Xing");
        let at = at.unwrap();
        let flags = u32::from_be_bytes(data[at + 4..at + 8].try_into().unwrap());
        assert_eq!(flags & 1, 1, "frame count present");
        let info = u32::from_be_bytes(data[at + 8..at + 12].try_into().unwrap());
        assert_eq!(frames, info);
    }
}
