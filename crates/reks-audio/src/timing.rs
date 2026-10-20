//! Per-codec timing convention (phase 3 task 4, docs/decisions.md D4).
//!
//! Symphonia's gapless decode starts at the first sample after the encoder
//! delay. rekordbox counts some files differently, and Pioneer players
//! follow rekordbox's ANLZ/PDB times, so the decoders in this crate
//! ([`crate::decode_f32`], [`crate::decode_stereo`]) put sample 0 where
//! rekordbox puts it: they prepend [`Timeline::lead_frames`] of silence.
//! Analysis, waveforms, preview and export then share rekordbox's timeline,
//! the one imported cues and grids are already on.
//!
//! Rules, measured on the maintainer's rekordbox 6 library against
//! Symphonia (which matches ffmpeg's gapless decode sample for sample);
//! numbers in `docs/analysis-parity.md`, "Timing offset per codec":
//!
//! - MP3 whose Xing/Info frame carries LAME's own tag (encoder `LAME…`,
//!   valid tag CRC): rekordbox trims neither the delay nor the Info frame,
//!   so sample 0 moves back by delay + one frame (1105 + 1152 samples,
//!   51.2 ms at 44.1 kHz).
//! - MP3 with any other Info tag (ffmpeg's `Lavf`/`Lavc`, a LAME tag with a
//!   bad CRC) and a delay Symphonia trimmed: the delay only (25.1 ms).
//! - MP3 without an Info tag, FLAC, WAV, AIFF, ALAC: as decoded.
//! - AAC in MP4: Symphonia 0.6 applies neither the edit list nor
//!   `iTunSMPB`, and one measured track suggests rekordbox does not trim
//!   the priming either; as decoded, **UNVERIFIED**.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Bytes read after the ID3v2 tag to find the first MPEG frame.
const SCAN_BYTES: usize = 64 * 1024;

/// Which timing rule a file falls under.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Rule {
    /// No offset: lossless formats, MP3 without a gapless tag, and codecs
    /// whose convention is unmeasured.
    AsDecoded,
    /// MP3 with an Info tag not written by LAME: the delay Symphonia trimmed
    /// is put back.
    Mp3UntrimmedDelay,
    /// MP3 with LAME's own Info tag: the delay and the Info frame.
    Mp3LameInfoFrame,
}

impl Rule {
    pub fn label(self) -> &'static str {
        match self {
            Rule::AsDecoded => "as decoded",
            Rule::Mp3UntrimmedDelay => "MP3, non-LAME Info tag: + delay",
            Rule::Mp3LameInfoFrame => "MP3, LAME Info tag: + delay + 1 frame",
        }
    }
}

/// How a file's decode is shifted onto rekordbox's timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timeline {
    pub rule: Rule,
    /// Frames of silence (at the file's rate) put before the first decoded
    /// sample.
    pub lead_frames: u32,
}

impl Timeline {
    pub const NONE: Timeline = Timeline {
        rule: Rule::AsDecoded,
        lead_frames: 0,
    };

    /// [`Self::lead_frames`] in ms at `sample_rate`.
    pub fn lead_ms(&self, sample_rate: u32) -> f64 {
        f64::from(self.lead_frames) * 1000.0 / f64::from(sample_rate.max(1))
    }
}

/// The timeline of an MP3 from its Info tag and the delay Symphonia
/// reported (and trimmed): `trimmed_delay` frames.
pub fn mp3_timeline(info: Option<&Mp3Info>, trimmed_delay: Option<u32>) -> Timeline {
    let Some(info) = info.filter(|i| i.tag == XingKind::Info || i.tag == XingKind::Xing) else {
        return Timeline::NONE;
    };
    let delay = trimmed_delay.unwrap_or(0);
    if info.is_lame() {
        Timeline {
            rule: Rule::Mp3LameInfoFrame,
            lead_frames: delay + info.samples_per_frame,
        }
    } else if delay > 0 {
        Timeline {
            rule: Rule::Mp3UntrimmedDelay,
            lead_frames: delay,
        }
    } else {
        Timeline::NONE
    }
}

/// `Xing` (VBR) or `Info` (CBR): the same structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XingKind {
    Xing,
    Info,
}

/// The first MPEG audio frame's Xing/Info header and its LAME extension.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mp3Info {
    pub tag: XingKind,
    /// Samples per frame of the stream (1152 for MPEG-1 layer III, 576 for
    /// MPEG-2/2.5).
    pub samples_per_frame: u32,
    /// The LAME extension's 9-byte encoder string, NULs and spaces trimmed
    /// (`LAME3.100`, `Lavf`, `Lavc59.37`); empty when absent.
    pub encoder: String,
    /// Encoder delay and padding from the LAME extension (12 bits each).
    pub delay: Option<u16>,
    pub padding: Option<u16>,
    /// The LAME extension's CRC-16 (over the frame up to the CRC field)
    /// matches.
    pub tag_crc_ok: bool,
}

impl Mp3Info {
    /// A valid tag written by the LAME encoder itself (ffmpeg writes
    /// `Lavf`/`Lavc` even when it encodes with libmp3lame).
    pub fn is_lame(&self) -> bool {
        self.encoder.starts_with("LAME") && self.tag_crc_ok
    }
}

/// Reads the Xing/Info header of the MP3 at `path`; `None` when the first
/// frame has none (or no frame is found).
pub fn read_mp3_info(path: &Path) -> std::io::Result<Option<Mp3Info>> {
    let mut f = File::open(path)?;
    let mut start = 0u64;
    // Skip any ID3v2 tags (several may follow each other).
    loop {
        let mut h = [0u8; 10];
        f.seek(SeekFrom::Start(start))?;
        if f.read(&mut h)? < 10 || &h[..3] != b"ID3" {
            break;
        }
        start += id3v2_len(&h);
    }
    f.seek(SeekFrom::Start(start))?;
    let mut buf = Vec::with_capacity(SCAN_BYTES);
    f.take(SCAN_BYTES as u64).read_to_end(&mut buf)?;
    Ok(parse_first_frame(&buf))
}

/// Total length of an ID3v2 tag from its 10-byte header.
fn id3v2_len(h: &[u8; 10]) -> u64 {
    let size = h[6..10]
        .iter()
        .fold(0u64, |acc, &b| (acc << 7) | u64::from(b & 0x7f));
    let footer = if h[5] & 0x10 != 0 { 10 } else { 0 };
    10 + size + footer
}

/// An MPEG audio layer III frame header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FrameHeader {
    mpeg1: bool,
    crc: bool,
    mono: bool,
    len: usize,
    samples: u32,
}

fn frame_header(b: &[u8]) -> Option<FrameHeader> {
    let h = u32::from_be_bytes(b.get(..4)?.try_into().ok()?);
    if h >> 21 != 0x7ff {
        return None;
    }
    let version = (h >> 19) & 3; // 0 = 2.5, 2 = 2, 3 = 1
    let layer = (h >> 17) & 3; // 1 = III
    let bitrate = ((h >> 12) & 15) as usize;
    let rate = ((h >> 10) & 3) as usize;
    if version == 1 || layer != 1 || bitrate == 0 || bitrate == 15 || rate == 3 {
        return None;
    }
    const V1: [u32; 15] = [
        0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320,
    ];
    const V2: [u32; 15] = [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160];
    let mpeg1 = version == 3;
    let sample_rate = match version {
        3 => [44_100, 48_000, 32_000][rate],
        2 => [22_050, 24_000, 16_000][rate],
        _ => [11_025, 12_000, 8_000][rate],
    };
    let (kbps, samples) = if mpeg1 {
        (V1[bitrate], 1152)
    } else {
        (V2[bitrate], 576)
    };
    let padding = (h >> 9) & 1;
    let len = (samples / 8 * kbps * 1000 / sample_rate + padding) as usize;
    Some(FrameHeader {
        mpeg1,
        crc: (h >> 16) & 1 == 0,
        mono: (h >> 6) & 3 == 3,
        len,
        samples,
    })
}

/// Finds the first frame in `buf` (a header followed by another header
/// where its length says) and reads its Xing/Info header.
fn parse_first_frame(buf: &[u8]) -> Option<Mp3Info> {
    let (at, fh) = (0..buf.len().saturating_sub(4)).find_map(|i| {
        let fh = frame_header(&buf[i..])?;
        frame_header(buf.get(i + fh.len..)?).map(|_| (i, fh))
    })?;
    let frame = buf.get(at..at + fh.len)?;
    let side = match (fh.mpeg1, fh.mono) {
        (true, false) => 32,
        (true, true) => 17,
        (false, false) => 17,
        (false, true) => 9,
    };
    let x = 4 + if fh.crc { 2 } else { 0 } + side;
    let tag = match frame.get(x..x + 4)? {
        b"Xing" => XingKind::Xing,
        b"Info" => XingKind::Info,
        _ => return None,
    };
    let flags = u32::from_be_bytes(frame.get(x + 4..x + 8)?.try_into().ok()?);
    let mut lame = x + 8;
    for (bit, len) in [(1, 4), (2, 4), (4, 100), (8, 4)] {
        if flags & bit != 0 {
            lame += len;
        }
    }
    let encoder = frame
        .get(lame..lame + 9)
        .map(|e| {
            String::from_utf8_lossy(e)
                .trim_end_matches(['\0', ' '])
                .to_owned()
        })
        .filter(|e| e.chars().all(|c| c.is_ascii_graphic() || c == ' '))
        .unwrap_or_default();
    let (delay, padding) = match frame.get(lame + 21..lame + 24) {
        Some(d) if !encoder.is_empty() => (
            Some((u16::from(d[0]) << 4) | u16::from(d[1] >> 4)),
            Some((u16::from(d[1] & 0x0f) << 8) | u16::from(d[2])),
        ),
        _ => (None, None),
    };
    // Over everything before the CRC field: the first 190 bytes of a stereo
    // MPEG-1 frame, fewer when the side info is shorter.
    let tag_crc_ok = match (frame.get(..lame + 34), frame.get(lame + 34..lame + 36)) {
        (Some(head), Some(crc)) => crc16(head) == u16::from_be_bytes([crc[0], crc[1]]),
        _ => false,
    };
    Some(Mp3Info {
        tag,
        samples_per_frame: fh.samples,
        encoder,
        delay,
        padding,
        tag_crc_ok,
    })
}

/// CRC-16/ARC (reflected polynomial 0xA001, initial 0), as the LAME tag uses.
fn crc16(data: &[u8]) -> u16 {
    data.iter().fold(0u16, |mut crc, &b| {
        crc ^= u16::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xa001
            } else {
                crc >> 1
            };
        }
        crc
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/audio")
            .join(name)
    }

    /// A 128 kb/s 44.1 kHz stereo frame (417 bytes) with an Info header,
    /// LAME extension `encoder`, delay 576 and padding 1234, then a second
    /// frame header.
    fn info_frame(encoder: &[u8; 9]) -> Vec<u8> {
        let mut f = vec![0u8; 417 + 4];
        f[..4].copy_from_slice(&[0xff, 0xfb, 0x90, 0x00]);
        f[417..].copy_from_slice(&[0xff, 0xfb, 0x90, 0x00]);
        let x = 4 + 32;
        f[x..x + 4].copy_from_slice(b"Info");
        f[x + 4..x + 8].copy_from_slice(&0x0fu32.to_be_bytes());
        let lame = x + 8 + 4 + 4 + 100 + 4;
        f[lame..lame + 9].copy_from_slice(encoder);
        // delay 576 = 0x240, padding 1234 = 0x4d2 → 24 04 d2
        f[lame + 21..lame + 24].copy_from_slice(&[0x24, 0x04, 0xd2]);
        let crc = crc16(&f[..lame + 34]);
        f[lame + 34..lame + 36].copy_from_slice(&crc.to_be_bytes());
        f
    }

    #[test]
    fn reads_a_lame_extension() {
        let mut buf = vec![0u8; 3];
        buf.extend(info_frame(b"LAME3.100"));
        let info = parse_first_frame(&buf).unwrap();
        assert_eq!(info.tag, XingKind::Info);
        assert_eq!(info.encoder, "LAME3.100");
        assert!(info.tag_crc_ok && info.is_lame());
        assert_eq!((info.delay, info.padding), (Some(576), Some(1234)));
        assert_eq!(info.samples_per_frame, 1152);
    }

    #[test]
    fn a_bad_crc_is_not_lame() {
        let mut f = info_frame(b"LAME3.100");
        f[100] ^= 1;
        let info = parse_first_frame(&f).unwrap();
        assert!(!info.tag_crc_ok && !info.is_lame());
    }

    #[test]
    fn crc16_check_value() {
        assert_eq!(crc16(b"123456789"), 0xbb3d);
    }

    #[test]
    fn ffmpeg_tag_is_not_lame() {
        let info = parse_first_frame(&info_frame(b"Lavf\0\0\0\0\0")).unwrap();
        assert_eq!(info.encoder, "Lavf");
        assert!(!info.is_lame());
    }

    #[test]
    fn plain_frames_have_no_info() {
        let mut f = info_frame(b"LAME3.100");
        f[36..40].copy_from_slice(b"\0\0\0\0");
        assert_eq!(parse_first_frame(&f), None);
        assert_eq!(parse_first_frame(&[0u8; 100]), None);
    }

    #[test]
    fn id3_length_is_syncsafe() {
        let h = [b'I', b'D', b'3', 4, 0, 0, 0, 0, 0x02, 0x01];
        assert_eq!(id3v2_len(&h), 10 + 257);
        let footer = [b'I', b'D', b'3', 4, 0, 0x10, 0, 0, 0, 0x01];
        assert_eq!(id3v2_len(&footer), 21);
    }

    #[test]
    fn timeline_rules() {
        let lame = parse_first_frame(&info_frame(b"LAME3.100")).unwrap();
        let ffmpeg = parse_first_frame(&info_frame(b"Lavc59.37")).unwrap();
        let t = mp3_timeline(Some(&lame), Some(1105));
        assert_eq!(
            (t.rule, t.lead_frames),
            (Rule::Mp3LameInfoFrame, 1105 + 1152)
        );
        assert!((t.lead_ms(44_100) - 51.18).abs() < 0.01);
        let t = mp3_timeline(Some(&ffmpeg), Some(1105));
        assert_eq!((t.rule, t.lead_frames), (Rule::Mp3UntrimmedDelay, 1105));
        assert_eq!(mp3_timeline(None, None), Timeline::NONE);
        assert_eq!(mp3_timeline(Some(&ffmpeg), None), Timeline::NONE);
    }

    #[test]
    fn fixtures() {
        let ffmpeg = read_mp3_info(&fixture("sine-click-128.mp3"))
            .unwrap()
            .unwrap();
        // ffmpeg 7.1 with libmp3lame writes "Lavf lame" (9 bytes).
        assert_eq!(
            (ffmpeg.tag, ffmpeg.encoder.as_str()),
            (XingKind::Info, "Lavf lame")
        );
        assert!(!ffmpeg.is_lame());
        assert_eq!(ffmpeg.delay, Some(576));
        let lame = read_mp3_info(&fixture("sine-click-128-lame.mp3"))
            .unwrap()
            .unwrap();
        assert!(lame.is_lame(), "{lame:?}");
        assert_eq!(lame.delay, Some(576));
        assert_eq!(
            read_mp3_info(&fixture("sine-click-128.flac")).unwrap(),
            None
        );
    }
}
