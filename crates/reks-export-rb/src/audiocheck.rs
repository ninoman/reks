//! Pre-export audio checks: finds files a player would refuse or mishandle
//! (E-8305 "unsupported file format" and friends, R§4) before anything is
//! written to a stick.
//!
//! [`check`] reads only headers: the WAV `fmt ` chunk, the AIFF `COMM` chunk
//! and form type, and up to [`MP3_SCAN_BYTES`] of MP3 frames. Sample rate,
//! channels and the FLAC/ALAC bit depth come from the library's [`Probe`].
//! Each [`Issue`] has a [`Severity`] for the target: errors block the track,
//! warnings are shown in the dry run.

use std::fmt;
use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use reks_audio::{Kind, Probe};

use crate::TargetProfile;

/// Something about a file that the target player may not play.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Issue {
    /// WAV with `WAVE_FORMAT_EXTENSIBLE` (0xFFFE) in `fmt `.
    WavExtensible,
    /// IEEE float WAV (format tag 3, or the float sub-format of EXTENSIBLE).
    FloatWav,
    /// WAV format tag (or EXTENSIBLE sub-format) that is neither PCM nor float,
    /// e.g. A-law (6).
    WavFormat(u16),
    /// PCM bit depth other than 16 or 24.
    BitDepth(u8),
    SampleRateUnsupported(u32),
    /// More than two channels.
    Channels(u8),
    /// AIFF-C form (`AIFC`), whatever its compression type.
    AiffC,
    /// MP3 whose frame bit rates vary but that has no Xing or VBRI header.
    VbrNoXing,
    /// Larger than FAT32's 4 GiB − 1 byte file limit.
    Over4GiB,
    /// Codec the target does not play, or plays only with some firmware.
    Codec(Kind),
}

/// How an [`Issue`] affects the export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Severity {
    /// Exported; the dry run explains the risk.
    Warning,
    /// Not exported.
    Error,
}

impl Issue {
    /// Severity of this issue on `target`.
    ///
    /// Warnings: FLAC/ALAC on players where support is partial or
    /// firmware-dependent; VBR MP3 without a seek header (plays, but seeking
    /// and time display can be off); 88.2/96 kHz on CDJ-3000 (UNVERIFIED).
    /// Everything else is an error.
    pub fn severity(self, target: TargetProfile) -> Severity {
        match self {
            Issue::Codec(Kind::Flac | Kind::Alac) if target != TargetProfile::Cdj2000Nxs => {
                Severity::Warning
            }
            Issue::SampleRateUnsupported(88_200 | 96_000) if target == TargetProfile::Cdj3000 => {
                Severity::Warning
            }
            Issue::VbrNoXing => Severity::Warning,
            _ => Severity::Error,
        }
    }

    /// True when the converter's Older players preset (Phase 2.6) fixes this
    /// issue on a file of `kind`: lossless sources only (D25), and rates
    /// above 48 kHz only (the preset keeps lower rates).
    pub fn converter_fixes(self, kind: Kind) -> bool {
        kind.is_lossless()
            && match self {
                Issue::Codec(k) => matches!(k, Kind::Flac | Kind::Alac),
                Issue::WavExtensible | Issue::FloatWav | Issue::BitDepth(_) | Issue::AiffC => true,
                Issue::SampleRateUnsupported(rate) => rate > 48_000,
                Issue::WavFormat(_) | Issue::Channels(_) | Issue::VbrNoXing | Issue::Over4GiB => {
                    false
                }
            }
    }
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Issue::WavExtensible => {
                f.write_str("WAV uses WAVE_FORMAT_EXTENSIBLE, which players reject")
            }
            Issue::FloatWav => {
                f.write_str("WAV holds floating-point samples; players need 16- or 24-bit PCM")
            }
            Issue::WavFormat(tag) => write!(f, "WAV format 0x{tag:04X} is not PCM"),
            Issue::BitDepth(bits) => write!(f, "{bits}-bit audio; players need 16 or 24 bits"),
            Issue::SampleRateUnsupported(rate) => {
                write!(
                    f,
                    "{rate} Hz sample rate is not supported by the target player"
                )
            }
            Issue::Channels(n) => write!(f, "{n} channels; players play mono or stereo"),
            Issue::AiffC => f.write_str("AIFF-C file; players need plain AIFF"),
            Issue::VbrNoXing => f.write_str(
                "VBR MP3 without a Xing/VBRI header; seeking and time display may be wrong",
            ),
            Issue::Over4GiB => f.write_str("file is larger than 4 GiB, the FAT32 limit"),
            Issue::Codec(kind) => write!(
                f,
                "{} playback depends on the player model and firmware",
                kind.as_str().to_ascii_uppercase()
            ),
        }
    }
}

/// Why [`check`] could not inspect a file.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    /// The header does not match the probed kind, or is truncated.
    #[error("malformed header in {path}: {reason}")]
    Malformed { path: PathBuf, reason: String },
}

/// FAT32's largest file: 4 GiB − 1 byte.
const FAT32_MAX_FILE: u64 = u32::MAX as u64;
/// Chunks walked before giving up on finding `fmt ` / `COMM`.
const MAX_CHUNKS: usize = 64;
/// MP3 bytes (after ID3v2) scanned for varying frame bit rates.
pub const MP3_SCAN_BYTES: u64 = 1 << 20;
/// Bytes after ID3v2 searched for the first frame sync.
const MP3_SYNC_WINDOW: usize = 64 * 1024;

const WAVE_FORMAT_PCM: u16 = 0x0001;
const WAVE_FORMAT_IEEE_FLOAT: u16 = 0x0003;
const WAVE_FORMAT_EXTENSIBLE: u16 = 0xFFFE;

/// Checks `path` (already probed as `probe`) against `target`. An empty list
/// means nothing is known to be wrong.
pub fn check(path: &Path, probe: &Probe, target: TargetProfile) -> Result<Vec<Issue>, Error> {
    let to_err = |source: io::Error| match source.kind() {
        io::ErrorKind::InvalidData | io::ErrorKind::UnexpectedEof => Error::Malformed {
            path: path.to_path_buf(),
            reason: if source.kind() == io::ErrorKind::UnexpectedEof {
                "truncated".to_owned()
            } else {
                source.to_string()
            },
        },
        _ => Error::Io {
            path: path.to_path_buf(),
            source,
        },
    };
    let file = File::open(path).map_err(to_err)?;
    let size = file.metadata().map_err(to_err)?.len();
    let mut r = BufReader::new(file);

    let mut issues = Vec::new();
    if size > FAT32_MAX_FILE {
        issues.push(Issue::Over4GiB);
    }

    // Bit depth from the container header where we parse one; it is what the
    // player sees, and float WAV has no meaningful PCM depth.
    let mut float = false;
    let header_bits = match probe.kind {
        Kind::Wav => {
            let fmt = read_wav_fmt(&mut r).map_err(to_err)?;
            float = wav_format_issues(&fmt, &mut issues);
            Some(fmt.bits)
        }
        Kind::Aiff => {
            let comm = read_aiff_comm(&mut r).map_err(to_err)?;
            if comm.aifc {
                issues.push(Issue::AiffC);
            }
            Some(comm.bits)
        }
        Kind::Mp3 => {
            if mp3_vbr_without_header(&mut r).map_err(to_err)? {
                issues.push(Issue::VbrNoXing);
            }
            None
        }
        Kind::Aac | Kind::Alac | Kind::Flac => None,
    };

    if matches!(probe.kind, Kind::Flac | Kind::Alac) && target != TargetProfile::Cdj3000 {
        issues.push(Issue::Codec(probe.kind));
    }
    if !sample_rate_ok(probe.kind, probe.sample_rate, target) {
        issues.push(Issue::SampleRateUnsupported(probe.sample_rate));
    }
    if probe.channels > 2 {
        issues.push(Issue::Channels(probe.channels));
    }
    if probe.kind.is_lossless() && !float {
        let bits = header_bits
            .map(|b| u8::try_from(b).unwrap_or(u8::MAX))
            .or(probe.bit_depth);
        if let Some(bits) = bits.filter(|b| !matches!(b, 16 | 24)) {
            issues.push(Issue::BitDepth(bits));
        }
    }
    Ok(issues)
}

/// Official rates (R§4): 44.1/48 kHz everywhere, 32 kHz for lossy codecs,
/// FLAC/ALAC up to 96 kHz on CDJ-2000NXS2.
fn sample_rate_ok(kind: Kind, rate: u32, target: TargetProfile) -> bool {
    match rate {
        44_100 | 48_000 => true,
        32_000 => !kind.is_lossless(),
        88_200 | 96_000 => {
            matches!(kind, Kind::Flac | Kind::Alac) && target == TargetProfile::Cdj2000Nxs2
        }
        _ => false,
    }
}

fn invalid(reason: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, reason.to_owned())
}

fn u16_le(b: &[u8]) -> u16 {
    u16::from_le_bytes([b[0], b[1]])
}

fn u32_le(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

fn u32_be(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}

/// Walks RIFF-style chunks from the current position until `id`; returns its
/// body size, positioned at the body. Bodies are padded to even length.
fn find_chunk<R: Read + Seek>(r: &mut R, id: &[u8; 4], big_endian: bool) -> io::Result<u32> {
    for _ in 0..MAX_CHUNKS {
        let mut head = [0u8; 8];
        r.read_exact(&mut head)?;
        let size = if big_endian {
            u32_be(&head[4..])
        } else {
            u32_le(&head[4..])
        };
        if &head[..4] == id {
            return Ok(size);
        }
        r.seek(SeekFrom::Current(i64::from(size) + i64::from(size & 1)))?;
    }
    Err(invalid("chunk not found"))
}

#[derive(Debug)]
struct WavFmt {
    tag: u16,
    /// EXTENSIBLE only: the first two bytes of the sub-format GUID.
    sub_tag: Option<u16>,
    bits: u16,
}

fn read_wav_fmt<R: Read + Seek>(r: &mut R) -> io::Result<WavFmt> {
    let mut riff = [0u8; 12];
    r.read_exact(&mut riff)?;
    if &riff[..4] != b"RIFF" || &riff[8..] != b"WAVE" {
        return Err(invalid("not a RIFF/WAVE file"));
    }
    let size = find_chunk(r, b"fmt ", false).map_err(|e| missing(e, "no fmt chunk"))?;
    if size < 16 {
        return Err(invalid("fmt chunk too short"));
    }
    // WAVEFORMATEXTENSIBLE is 40 bytes; the sub-format GUID starts at 24.
    let mut body = [0u8; 40];
    let n = usize::try_from(size.min(40)).unwrap_or(40);
    r.read_exact(&mut body[..n])?;
    let tag = u16_le(&body[0..]);
    let sub_tag = if tag == WAVE_FORMAT_EXTENSIBLE {
        if n < 40 {
            return Err(invalid("WAVE_FORMAT_EXTENSIBLE fmt chunk too short"));
        }
        Some(u16_le(&body[24..]))
    } else {
        None
    };
    Ok(WavFmt {
        tag,
        sub_tag,
        bits: u16_le(&body[14..]),
    })
}

/// Pushes the format-tag issues; returns whether the samples are float.
fn wav_format_issues(fmt: &WavFmt, issues: &mut Vec<Issue>) -> bool {
    let effective = match fmt.sub_tag {
        Some(sub) => {
            issues.push(Issue::WavExtensible);
            sub
        }
        None => fmt.tag,
    };
    match effective {
        WAVE_FORMAT_PCM => false,
        WAVE_FORMAT_IEEE_FLOAT => {
            issues.push(Issue::FloatWav);
            true
        }
        other => {
            issues.push(Issue::WavFormat(other));
            false
        }
    }
}

/// A chunk walk that hits EOF means the chunk is missing, not that the file
/// is truncated.
fn missing(e: io::Error, reason: &str) -> io::Error {
    if e.kind() == io::ErrorKind::UnexpectedEof {
        invalid(reason)
    } else {
        e
    }
}

#[derive(Debug)]
struct AiffComm {
    aifc: bool,
    bits: u16,
}

fn read_aiff_comm<R: Read + Seek>(r: &mut R) -> io::Result<AiffComm> {
    let mut form = [0u8; 12];
    r.read_exact(&mut form)?;
    let aifc = match (&form[..4], &form[8..]) {
        (b"FORM", b"AIFF") => false,
        (b"FORM", b"AIFC") => true,
        _ => return Err(invalid("not an AIFF/AIFF-C file")),
    };
    let size = find_chunk(r, b"COMM", true).map_err(|e| missing(e, "no COMM chunk"))?;
    if size < 18 {
        return Err(invalid("COMM chunk too short"));
    }
    // numChannels i16, numSampleFrames u32, sampleSize i16, sampleRate f80.
    let mut body = [0u8; 8];
    r.read_exact(&mut body)?;
    Ok(AiffComm {
        aifc,
        bits: u16::from_be_bytes([body[6], body[7]]),
    })
}

/// MPEG audio Layer III frame header, as far as frame walking needs it.
#[derive(Debug, Clone, Copy)]
struct FrameHeader {
    mpeg1: bool,
    mono: bool,
    bitrate_index: u8,
    len: usize,
}

const BITRATES_V1_L3: [u32; 15] = [
    0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320,
];
const BITRATES_V2_L3: [u32; 15] = [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160];

fn frame_header(b: &[u8]) -> Option<FrameHeader> {
    let h = b.get(..4)?;
    if h[0] != 0xFF || h[1] & 0xE0 != 0xE0 {
        return None;
    }
    let version = (h[1] >> 3) & 0b11; // 0 = MPEG 2.5, 2 = MPEG 2, 3 = MPEG 1
    let layer = (h[1] >> 1) & 0b11; // 1 = Layer III
    let bitrate_index = h[2] >> 4;
    let rate_index = (h[2] >> 2) & 0b11;
    if version == 1 || layer != 1 || bitrate_index == 0 || bitrate_index == 15 || rate_index == 3 {
        return None;
    }
    let mpeg1 = version == 3;
    let rates: [u32; 3] = match version {
        3 => [44_100, 48_000, 32_000],
        2 => [22_050, 24_000, 16_000],
        _ => [11_025, 12_000, 8_000],
    };
    let table = if mpeg1 {
        &BITRATES_V1_L3
    } else {
        &BITRATES_V2_L3
    };
    let kbps = *table.get(usize::from(bitrate_index))?;
    let rate = *rates.get(usize::from(rate_index))?;
    let samples_factor = if mpeg1 { 144 } else { 72 };
    let padding = u32::from((h[2] >> 1) & 1);
    let len = usize::try_from(samples_factor * kbps * 1000 / rate + padding).ok()?;
    Some(FrameHeader {
        mpeg1,
        mono: h[3] >> 6 == 3,
        bitrate_index,
        len,
    })
}

/// Skips ID3v2 tags (there may be more than one) from the start of the file.
fn skip_id3v2<R: Read + Seek>(r: &mut R) -> io::Result<u64> {
    let mut pos = 0u64;
    for _ in 0..4 {
        r.seek(SeekFrom::Start(pos))?;
        let mut h = [0u8; 10];
        if r.read_exact(&mut h).is_err() || &h[..3] != b"ID3" {
            break;
        }
        // Syncsafe size: 7 bits per byte.
        let size = h[6..10]
            .iter()
            .fold(0u64, |acc, &b| (acc << 7) | u64::from(b & 0x7F));
        let footer = if h[5] & 0x10 != 0 { 10 } else { 0 };
        pos += 10 + size + footer;
    }
    r.seek(SeekFrom::Start(pos))?;
    Ok(pos)
}

/// True when the frame bit rates within the scanned range vary and the first
/// frame carries no Xing/Info or VBRI header.
fn mp3_vbr_without_header<R: Read + Seek>(r: &mut R) -> io::Result<bool> {
    skip_id3v2(r)?;
    let mut buf = Vec::new();
    r.take(MP3_SCAN_BYTES).read_to_end(&mut buf)?;

    // First sync whose successor is also a frame header (or the end of the
    // scan), so a stray 0xFFE in junk is not taken for a frame.
    let start = (0..buf.len().min(MP3_SYNC_WINDOW))
        .find(|&i| {
            frame_header(&buf[i..]).is_some_and(|h| {
                let next = i + h.len;
                next >= buf.len() || frame_header(&buf[next..]).is_some()
            })
        })
        .ok_or_else(|| invalid("no MPEG Layer III frame found"))?;
    let Some(first) = frame_header(&buf[start..]) else {
        return Err(invalid("no MPEG Layer III frame found"));
    };

    // Xing/Info sits after the side info; VBRI at a fixed 32 bytes.
    let side_info = match (first.mpeg1, first.mono) {
        (true, false) => 32,
        (true, true) | (false, false) => 17,
        (false, true) => 9,
    };
    let tag_at = |off: usize| buf.get(start + off..start + off + 4);
    if matches!(tag_at(4 + side_info), Some(b"Xing" | b"Info")) || tag_at(36) == Some(b"VBRI") {
        return Ok(false);
    }

    let mut pos = start;
    while let Some(h) = buf.get(pos..).and_then(frame_header) {
        if h.bitrate_index != first.bitrate_index {
            return Ok(true);
        }
        pos += h.len;
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    const NXS: TargetProfile = TargetProfile::Cdj2000Nxs;
    const NXS2: TargetProfile = TargetProfile::Cdj2000Nxs2;
    const CDJ3000: TargetProfile = TargetProfile::Cdj3000;
    const ALL: [TargetProfile; 5] = [
        NXS,
        NXS2,
        CDJ3000,
        TargetProfile::XdjXz,
        TargetProfile::Generic,
    ];

    fn probe(kind: Kind, sample_rate: u32, channels: u8, bit_depth: Option<u8>) -> Probe {
        Probe {
            kind,
            duration_ms: 1_000,
            sample_rate,
            channels,
            bit_depth,
            bit_rate_kbps: None,
        }
    }

    #[test]
    fn converter_fixes_lossless_issues_only() {
        let fixed = [
            Issue::Codec(Kind::Flac),
            Issue::Codec(Kind::Alac),
            Issue::WavExtensible,
            Issue::FloatWav,
            Issue::BitDepth(32),
            Issue::SampleRateUnsupported(96_000),
            Issue::AiffC,
        ];
        for issue in fixed {
            assert!(issue.converter_fixes(Kind::Wav), "{issue:?}");
            assert!(!issue.converter_fixes(Kind::Mp3), "{issue:?} on MP3");
        }
        let not_fixed = [
            Issue::Codec(Kind::Aac),
            Issue::WavFormat(6),
            Issue::Channels(6),
            Issue::VbrNoXing,
            Issue::Over4GiB,
            // The Older players preset keeps rates up to 48 kHz.
            Issue::SampleRateUnsupported(22_050),
        ];
        for issue in not_fixed {
            assert!(!issue.converter_fixes(Kind::Wav), "{issue:?}");
        }
    }

    fn write_temp(bytes: &[u8]) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(bytes).unwrap();
        f.flush().unwrap();
        f
    }

    fn run(bytes: &[u8], p: &Probe, target: TargetProfile) -> Result<Vec<Issue>, Error> {
        let f = write_temp(bytes);
        check(f.path(), p, target)
    }

    /// RIFF/WAVE with an odd-sized chunk before `fmt ` (exercises padding).
    fn wav(tag: u16, channels: u16, rate: u32, bits: u16, sub_tag: Option<u16>) -> Vec<u8> {
        let mut fmt = Vec::new();
        fmt.extend(tag.to_le_bytes());
        fmt.extend(channels.to_le_bytes());
        fmt.extend(rate.to_le_bytes());
        let block = channels * bits / 8;
        fmt.extend((rate * u32::from(block)).to_le_bytes());
        fmt.extend(block.to_le_bytes());
        fmt.extend(bits.to_le_bytes());
        if let Some(sub) = sub_tag {
            fmt.extend(22u16.to_le_bytes()); // cbSize
            fmt.extend(bits.to_le_bytes()); // valid bits
            fmt.extend(3u32.to_le_bytes()); // channel mask
            fmt.extend(sub.to_le_bytes());
            fmt.extend([
                0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0xAA, 0x00, 0x38, 0x9B, 0x71,
            ]);
        }
        let mut body = b"WAVE".to_vec();
        body.extend(b"junk");
        body.extend(3u32.to_le_bytes());
        body.extend([1, 2, 3, 0]); // 3 bytes + pad
        body.extend(b"fmt ");
        body.extend(u32::try_from(fmt.len()).unwrap().to_le_bytes());
        body.extend(&fmt);
        body.extend(b"data");
        body.extend(4u32.to_le_bytes());
        body.extend([0u8; 4]);
        let mut out = b"RIFF".to_vec();
        out.extend(u32::try_from(body.len()).unwrap().to_le_bytes());
        out.extend(body);
        out
    }

    fn aiff(aifc: bool, bits: u16) -> Vec<u8> {
        let mut body = if aifc { b"AIFC" } else { b"AIFF" }.to_vec();
        if aifc {
            body.extend(b"FVER");
            body.extend(4u32.to_be_bytes());
            body.extend(0xA280_5140u32.to_be_bytes());
        }
        let mut comm = Vec::new();
        comm.extend(2u16.to_be_bytes());
        comm.extend(1000u32.to_be_bytes());
        comm.extend(bits.to_be_bytes());
        // 44100 as an 80-bit extended float.
        comm.extend([0x40, 0x0E, 0xAC, 0x44, 0, 0, 0, 0, 0, 0]);
        if aifc {
            comm.extend(b"NONE");
            comm.extend([0, 0]); // empty pstring + pad
        }
        body.extend(b"COMM");
        body.extend(u32::try_from(comm.len()).unwrap().to_be_bytes());
        body.extend(comm);
        body.extend(b"SSND");
        body.extend(8u32.to_be_bytes());
        body.extend([0u8; 8]);
        let mut out = b"FORM".to_vec();
        out.extend(u32::try_from(body.len()).unwrap().to_be_bytes());
        out.extend(body);
        out
    }

    /// ID3v2 tag + MPEG-1 Layer III, 44.1 kHz stereo frames with the given
    /// bit-rate indices; `tag` (with its offset) goes into the first frame.
    fn mp3(bitrate_indices: &[u8], tag: Option<(usize, &[u8; 4])>) -> Vec<u8> {
        let mut out = b"ID3\x04\x00\x00".to_vec();
        out.extend([0, 0, 0, 100]); // syncsafe 100
        out.extend([0u8; 100]);
        for (i, &idx) in bitrate_indices.iter().enumerate() {
            let len = 144 * BITRATES_V1_L3[usize::from(idx)] as usize * 1000 / 44_100;
            let mut frame = vec![0u8; len];
            frame[..4].copy_from_slice(&[0xFF, 0xFB, idx << 4, 0x00]);
            if let (0, Some((off, t))) = (i, tag) {
                frame[off..off + 4].copy_from_slice(t);
            }
            out.extend(frame);
        }
        out
    }

    #[test]
    fn clean_wav_has_no_issues_on_any_target() {
        let bytes = wav(1, 2, 44_100, 16, None);
        for target in ALL {
            let p = probe(Kind::Wav, 44_100, 2, Some(16));
            assert_eq!(run(&bytes, &p, target).unwrap(), vec![], "{target:?}");
        }
        let bytes = wav(1, 1, 48_000, 24, None);
        let p = probe(Kind::Wav, 48_000, 1, Some(24));
        assert_eq!(run(&bytes, &p, NXS).unwrap(), vec![]);
    }

    #[test]
    fn wav_extensible_is_an_error() {
        let bytes = wav(WAVE_FORMAT_EXTENSIBLE, 2, 44_100, 16, Some(1));
        let p = probe(Kind::Wav, 44_100, 2, Some(16));
        let issues = run(&bytes, &p, CDJ3000).unwrap();
        assert_eq!(issues, vec![Issue::WavExtensible]);
        assert_eq!(Issue::WavExtensible.severity(CDJ3000), Severity::Error);
    }

    #[test]
    fn float_wav_is_an_error_plain_or_extensible() {
        let p = probe(Kind::Wav, 44_100, 2, Some(32));
        let plain = wav(WAVE_FORMAT_IEEE_FLOAT, 2, 44_100, 32, None);
        assert_eq!(run(&plain, &p, NXS2).unwrap(), vec![Issue::FloatWav]);
        let ext = wav(WAVE_FORMAT_EXTENSIBLE, 2, 44_100, 32, Some(3));
        assert_eq!(
            run(&ext, &p, NXS2).unwrap(),
            vec![Issue::WavExtensible, Issue::FloatWav]
        );
        assert_eq!(Issue::FloatWav.severity(CDJ3000), Severity::Error);
    }

    #[test]
    fn non_pcm_wav_format_is_reported() {
        let bytes = wav(6, 1, 44_100, 8, None); // A-law
        let p = probe(Kind::Wav, 44_100, 1, Some(8));
        let issues = run(&bytes, &p, CDJ3000).unwrap();
        assert_eq!(issues, vec![Issue::WavFormat(6), Issue::BitDepth(8)]);
    }

    #[test]
    fn bit_depth_other_than_16_or_24_is_an_error() {
        let bytes = wav(1, 2, 44_100, 32, None);
        let p = probe(Kind::Wav, 44_100, 2, Some(32));
        assert_eq!(run(&bytes, &p, CDJ3000).unwrap(), vec![Issue::BitDepth(32)]);
        // FLAC depth comes from the probe.
        let p = probe(Kind::Flac, 44_100, 2, Some(8));
        assert_eq!(run(b"fLaC", &p, CDJ3000).unwrap(), vec![Issue::BitDepth(8)]);
    }

    #[test]
    fn sample_rate_limits_depend_on_kind_and_target() {
        let bytes = wav(1, 2, 96_000, 24, None);
        let p = probe(Kind::Wav, 96_000, 2, Some(24));
        let issue = Issue::SampleRateUnsupported(96_000);
        assert_eq!(run(&bytes, &p, NXS2).unwrap(), vec![issue]);
        assert_eq!(issue.severity(NXS2), Severity::Error);
        assert_eq!(issue.severity(CDJ3000), Severity::Warning);

        // 22.05 kHz is an error everywhere.
        let p = probe(Kind::Mp3, 22_050, 2, None);
        let issues = run(&mp3(&[9; 4], None), &p, CDJ3000).unwrap();
        assert_eq!(issues, vec![Issue::SampleRateUnsupported(22_050)]);
        assert_eq!(issues[0].severity(CDJ3000), Severity::Error);

        // 32 kHz: fine for MP3, not for WAV.
        let p = probe(Kind::Mp3, 32_000, 2, None);
        assert_eq!(run(&mp3(&[9; 4], None), &p, NXS).unwrap(), vec![]);
        assert!(!sample_rate_ok(Kind::Wav, 32_000, CDJ3000));

        // 96 kHz FLAC is supported on NXS2 (only the codec warning remains).
        let p = probe(Kind::Flac, 96_000, 2, Some(24));
        assert_eq!(
            run(b"fLaC", &p, NXS2).unwrap(),
            vec![Issue::Codec(Kind::Flac)]
        );
    }

    #[test]
    fn more_than_two_channels_is_an_error() {
        let bytes = wav(1, 6, 48_000, 16, None);
        let p = probe(Kind::Wav, 48_000, 6, Some(16));
        let issues = run(&bytes, &p, CDJ3000).unwrap();
        assert_eq!(issues, vec![Issue::Channels(6)]);
        assert_eq!(issues[0].severity(CDJ3000), Severity::Error);
    }

    #[test]
    fn aiff_plain_is_clean_and_aiff_c_is_an_error() {
        let p = probe(Kind::Aiff, 44_100, 2, Some(16));
        assert_eq!(run(&aiff(false, 16), &p, NXS).unwrap(), vec![]);
        assert_eq!(run(&aiff(true, 16), &p, NXS).unwrap(), vec![Issue::AiffC]);
        assert_eq!(Issue::AiffC.severity(CDJ3000), Severity::Error);
        let p = probe(Kind::Aiff, 44_100, 2, Some(24));
        assert_eq!(
            run(&aiff(false, 20), &p, NXS).unwrap(),
            vec![Issue::BitDepth(20)]
        );
    }

    #[test]
    fn mp3_vbr_needs_a_seek_header() {
        let p = probe(Kind::Mp3, 44_100, 2, None);
        let vbr = [9, 9, 11, 5, 14, 9];
        assert_eq!(run(&mp3(&[9; 6], None), &p, NXS).unwrap(), vec![]);
        assert_eq!(
            run(&mp3(&vbr, None), &p, NXS).unwrap(),
            vec![Issue::VbrNoXing]
        );
        assert_eq!(Issue::VbrNoXing.severity(NXS), Severity::Warning);
        // Xing/Info after 32 bytes of stereo MPEG-1 side info, VBRI at 36.
        for tag in [(36, b"Xing"), (36, b"Info"), (36, b"VBRI")] {
            let bytes = mp3(&vbr, Some(tag));
            assert_eq!(run(&bytes, &p, NXS).unwrap(), vec![], "{:?}", tag.1);
        }
        // A Xing tag at the wrong offset does not count.
        let bytes = mp3(&vbr, Some((40, b"Xing")));
        assert_eq!(run(&bytes, &p, NXS).unwrap(), vec![Issue::VbrNoXing]);
    }

    #[test]
    fn codec_severity_depends_on_target() {
        for kind in [Kind::Flac, Kind::Alac] {
            let p = probe(kind, 44_100, 2, Some(16));
            assert_eq!(run(b"x", &p, CDJ3000).unwrap(), vec![]);
            for target in [NXS, NXS2, TargetProfile::XdjXz, TargetProfile::Generic] {
                assert_eq!(run(b"x", &p, target).unwrap(), vec![Issue::Codec(kind)]);
            }
            assert_eq!(Issue::Codec(kind).severity(NXS), Severity::Error);
            assert_eq!(Issue::Codec(kind).severity(NXS2), Severity::Warning);
            assert_eq!(
                Issue::Codec(kind).severity(TargetProfile::Generic),
                Severity::Warning
            );
        }
        let p = probe(Kind::Aac, 44_100, 2, None);
        assert_eq!(run(b"x", &p, NXS).unwrap(), vec![]);
    }

    #[test]
    fn over_4_gib_is_an_error() {
        let f = write_temp(&wav(1, 2, 44_100, 16, None));
        f.as_file().set_len(FAT32_MAX_FILE + 1).unwrap(); // sparse
        let p = probe(Kind::Wav, 44_100, 2, Some(16));
        assert_eq!(check(f.path(), &p, NXS2).unwrap(), vec![Issue::Over4GiB]);
        f.as_file().set_len(FAT32_MAX_FILE).unwrap();
        assert_eq!(check(f.path(), &p, NXS2).unwrap(), vec![]);
        assert_eq!(Issue::Over4GiB.severity(CDJ3000), Severity::Error);
    }

    #[test]
    fn malformed_headers_are_errors_not_panics() {
        let cases: [(Kind, Vec<u8>); 3] = [
            (
                Kind::Wav,
                wav(WAVE_FORMAT_EXTENSIBLE, 2, 44_100, 16, Some(1)),
            ),
            (Kind::Aiff, aiff(true, 16)),
            (Kind::Mp3, mp3(&[9, 11], None)),
        ];
        for (kind, bytes) in cases {
            let p = probe(kind, 44_100, 2, Some(16));
            // Every truncation of a valid header either parses or errors.
            for n in 0..bytes.len() {
                let _ = run(&bytes[..n], &p, NXS2);
            }
            assert!(matches!(
                run(b"garbage garbage garbage", &p, NXS2),
                Err(Error::Malformed { .. })
            ));
        }
        let p = probe(Kind::Wav, 44_100, 2, Some(16));
        assert!(matches!(
            run(b"RIFF\x00\x00\x00\x00WAVEdata", &p, NXS2),
            Err(Error::Malformed { .. })
        ));
        let missing = Path::new("/nonexistent/reks/audiocheck.wav");
        assert!(matches!(check(missing, &p, NXS2), Err(Error::Io { .. })));
    }

    #[test]
    fn generated_fixtures_pass_with_the_real_probe() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/audio");
        for ext in ["wav", "aiff", "mp3", "m4a", "flac"] {
            let path = dir.join(format!("sine-click-128.{ext}"));
            let p = reks_audio::probe(&path).unwrap();
            assert_eq!(check(&path, &p, CDJ3000).unwrap(), vec![], "{ext}");
            for target in ALL {
                let issues = check(&path, &p, target).unwrap();
                assert!(
                    issues.iter().all(|i| matches!(i, Issue::Codec(_))),
                    "{ext} on {target:?}: {issues:?}"
                );
            }
        }
    }
}
