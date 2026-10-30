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
//!
//! The file is streamed through a small window, never read whole.

use std::fs::File;
use std::io::{self, Read};
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
    let io_err = |source| Error::Io {
        path: path.to_path_buf(),
        source,
    };
    let file = File::open(path).map_err(io_err)?;
    count_frames(file).map_err(io_err)
}

/// Bytes read from the file at a time.
const CHUNK: usize = 64 * 1024;
/// More than the longest frame (1441 bytes).
const BACK: usize = 4 * 1024;

/// Forward-only view of a stream by absolute offset, keeping only the bytes
/// from the last requested offset on.
struct Window<R> {
    r: R,
    buf: Vec<u8>,
    /// Stream offset of `buf[0]`.
    base: usize,
    eof: bool,
}

impl<R: Read> Window<R> {
    fn new(r: R) -> Self {
        Self {
            r,
            buf: Vec::new(),
            base: 0,
            eof: false,
        }
    }

    /// Up to `n` bytes at `pos` (fewer only at the end of the stream).
    /// `pos` never goes below an earlier call's.
    fn at(&mut self, pos: usize, n: usize) -> io::Result<&[u8]> {
        // Keep `BACK` bytes before `pos`: the first-frame search steps back
        // from its look at the next frame.
        let skip = pos.saturating_sub(self.base + BACK).min(self.buf.len());
        if skip >= CHUNK {
            self.buf.drain(..skip);
            self.base += skip;
        }
        while !self.eof && self.base + self.buf.len() < pos + n {
            let len = self.buf.len();
            self.buf.resize(len + CHUNK, 0);
            let read = loop {
                match self.r.read(&mut self.buf[len..]) {
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    other => break other,
                }
            };
            let read = read.inspect_err(|_| self.buf.truncate(len))?;
            self.buf.truncate(len + read);
            self.eof = read == 0;
        }
        let from = pos.saturating_sub(self.base).min(self.buf.len());
        let to = (from + n).min(self.buf.len());
        Ok(&self.buf[from..to])
    }
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

/// Bytes of the ID3v2 tag at the start of a file whose first bytes are
/// `head` (0 without one).
fn id3v2_len(head: &[u8]) -> usize {
    match head.get(..10) {
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

fn count_frames(r: impl Read) -> io::Result<Option<u32>> {
    let mut data = Window::new(r);
    let mut pos = id3v2_len(data.at(0, 10)?);
    // The first frame: the first header that the next one follows (or the
    // end of the file), so stray sync bytes in junk before it don't count.
    let first = loop {
        let rest = data.at(pos, 4)?;
        if rest.len() < 4 {
            return Ok(None);
        }
        if let Some(h) = header(rest) {
            let next = data.at(pos + h.len, 4)?;
            if next.is_empty() || header(next).is_some() {
                break h;
            }
        }
        pos += 1;
    };
    let mut frames = 0u32;
    if is_info_frame(data.at(pos, 40)?, first) {
        pos += first.len;
    }
    while let Some(h) = header(data.at(pos, 4)?) {
        frames = frames.saturating_add(1);
        pos += h.len;
    }
    Ok((frames > 0).then_some(frames))
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
        assert_eq!(count_frames(&data[..]).unwrap(), Some(5));

        // Without the Info frame every frame counts; junk before the first
        // frame is skipped.
        let mut plain = vec![0x00, 0xFF, 0x12];
        for _ in 0..3 {
            plain.extend(frame());
        }
        assert_eq!(count_frames(&plain[..]).unwrap(), Some(3));
        assert_eq!(count_frames(&b"not an mp3 at all"[..]).unwrap(), None);
        assert_eq!(count_frames(&[][..]).unwrap(), None);
    }

    /// Hands out at most 1000 bytes per read.
    struct Trickle<'a>(&'a [u8]);

    impl Read for Trickle<'_> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let n = buf.len().min(self.0.len()).min(1000);
            buf[..n].copy_from_slice(&self.0[..n]);
            self.0 = &self.0[n..];
            Ok(n)
        }
    }

    #[test]
    fn streams_past_the_window() {
        // Junk longer than the window before the first frame, then ~1.2 MB
        // of frames read in short pieces.
        let mut data = vec![0x00; 3 * CHUNK + 7];
        data.extend(info_frame());
        for _ in 0..3000 {
            data.extend(frame());
        }
        data.extend_from_slice(b"TAG");
        assert_eq!(count_frames(Trickle(&data)).unwrap(), Some(3000));
        assert_eq!(count_frames(&data[..]).unwrap(), Some(3000));
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
