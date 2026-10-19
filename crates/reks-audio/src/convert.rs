//! The file converter's audio half (docs/phases/phase-2.6.md, task 4):
//! one lossless source file to a WAV, AIFF or FLAC temp file.
//!
//! [`convert`] streams [`NativeStream`] → optional [`StreamResampler`] →
//! [`Requantiser`] (TPDF dither at 16 bits) → writer, one decoded packet at a
//! time, so memory stays bounded whatever the track length. It then copies
//! tags and artwork with lofty, fsyncs, and decodes the file back with
//! Symphonia: the frame count and the hash of the PCM must match what was
//! written. Any error or a cancel removes the temp file.
//!
//! The library side (destination, rename, DB update) is `reks-core::convert`.

use std::collections::HashSet;
use std::fs::{File, OpenOptions};
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use lofty::config::WriteOptions;
use lofty::tag::{Tag, TagExt, TagType};
use serde::{Deserialize, Serialize};

use crate::dither::{to_float, Requantiser};
use crate::error::{Error, Result};
use crate::probe::probe;
use crate::write::{AiffWriter, FlacWriter, PcmSpec, PcmWriter, WavWriter, WriteError};
use crate::{NativeStream, StreamResampler};

/// Output container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Target {
    /// Plain PCM `fmt` tag 1, ID3v2 chunk for tags.
    Wav,
    /// Plain `AIFF` (never AIFF-C), ID3v2 chunk for tags.
    Aiff,
    /// FLAC, Vorbis comments + PICTURE for tags.
    Flac,
}

/// Output bit depth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Depth {
    /// The source's depth: 16 for 16 bits or fewer, else 24 (32-bit and
    /// float sources become 24-bit).
    Keep,
    /// 16 bits, TPDF-dithered when precision is lost.
    Bits16,
    /// 24 bits.
    Bits24,
}

/// Output sample rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Rate {
    Keep,
    Hz44100,
    Hz48000,
}

/// What to convert to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConvertSpec {
    pub target: Target,
    pub depth: Depth,
    pub rate: Rate,
}

/// The verified result of a conversion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    /// Frames in the output file.
    pub frames: u64,
    pub sample_rate: u32,
    pub bit_depth: u8,
    /// [`pcm_hash`] of the output file's audio.
    pub pcm_hash: [u8; 32],
}

/// Cooperative cancellation. Clones share one flag.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// Share of the progress bar for writing; the read-back check gets the rest.
const WRITE_SHARE: f32 = 0.85;

/// Converts `src` into a new file at `tmp` (which must not exist) and
/// verifies it. `progress` gets `0.0..=1.0`, at most about once per percent.
///
/// Refuses lossy sources (MP3, AAC) and more than 2 channels
/// ([`Error::Unsupported`]). On any error, including [`Error::Cancelled`],
/// `tmp` is removed; `src` is only ever read.
pub fn convert(
    src: &Path,
    tmp: &Path,
    spec: &ConvertSpec,
    progress: &dyn Fn(f32),
    cancel: &CancelToken,
) -> Result<Output> {
    let source = probe(src)?;
    if !source.kind.is_lossless() {
        return Err(Error::Unsupported {
            path: src.to_path_buf(),
            reason: format!(
                "{} is lossy; only lossless sources convert",
                source.kind.as_str()
            ),
        });
    }
    let mut stream = NativeStream::open(src)?;
    let input = *stream.format();
    if input.channels > 2 {
        return Err(Error::Unsupported {
            path: src.to_path_buf(),
            reason: format!("{} channels; at most 2 convert", input.channels),
        });
    }
    let pcm = PcmSpec {
        sample_rate: match spec.rate {
            Rate::Keep => input.sample_rate,
            Rate::Hz44100 => 44_100,
            Rate::Hz48000 => 48_000,
        },
        channels: input.channels,
        bits_per_sample: match spec.depth {
            Depth::Keep if input.bits_per_sample <= 16 => 16,
            Depth::Keep | Depth::Bits24 => 24,
            Depth::Bits16 => 16,
        },
    };
    let resample_err = |reason: String| Error::Resample {
        path: src.to_path_buf(),
        reason,
    };
    let mut resampler = if pcm.sample_rate == input.sample_rate {
        None
    } else {
        Some(
            StreamResampler::<f64>::new(
                input.sample_rate,
                pcm.sample_rate,
                usize::from(input.channels),
            )
            .map_err(|e| resample_err(e.to_string()))?,
        )
    };
    tracing::debug!(src = %src.display(), tmp = %tmp.display(), ?spec, ?input, ?pcm, "convert");

    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(tmp)
        .map_err(|source| Error::Io {
            path: tmp.to_path_buf(),
            source,
        })?;
    // From here on, every early return removes `tmp`.
    let guard = TempFile(Some(tmp.to_path_buf()));
    let write_err = |source: WriteError| Error::Write {
        path: tmp.to_path_buf(),
        source,
    };
    let mut sink = Sink::new(spec.target, BufWriter::new(file), pcm).map_err(write_err)?;

    let mut requant = Requantiser::new(pcm.bits_per_sample);
    let mut hash = PcmHasher::default();
    let mut report = Progress::new(progress);
    let total = input.frames.filter(|&n| n > 0);
    let (mut chunk, mut floats, mut resampled, mut out) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    loop {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled {
                path: src.to_path_buf(),
            });
        }
        if !stream.read(&mut chunk)? {
            break;
        }
        out.clear();
        match resampler.as_mut() {
            Some(rs) => {
                floats.clear();
                floats.extend(chunk.iter().map(|&s| to_float(s, input.bits_per_sample)));
                resampled.clear();
                rs.push(&floats, &mut resampled)
                    .map_err(|e| resample_err(e.to_string()))?;
                out.extend(resampled.iter().map(|&x| requant.float(x)));
            }
            None => out.extend(chunk.iter().map(|&s| requant.int(s, input.bits_per_sample))),
        }
        sink.write(&out).map_err(write_err)?;
        hash.update(&out);
        if let Some(total) = total {
            report.set(WRITE_SHARE * (stream.frames_read() as f32 / total as f32));
        }
    }
    let frames_in = stream.frames_read();
    if let Some(total) = total {
        if frames_in < total {
            return Err(Error::Corrupt {
                path: src.to_path_buf(),
                reason: format!("decoded {frames_in} of {total} frames"),
            });
        }
    }
    let expected = match resampler.as_mut() {
        Some(rs) => {
            resampled.clear();
            rs.finish(&mut resampled)
                .map_err(|e| resample_err(e.to_string()))?;
            out.clear();
            out.extend(resampled.iter().map(|&x| requant.float(x)));
            sink.write(&out).map_err(write_err)?;
            hash.update(&out);
            rs.output_len(frames_in)
        }
        None => frames_in,
    };
    let written = sink.frames();
    if written != expected {
        return Err(Error::Verify {
            path: tmp.to_path_buf(),
            reason: format!("wrote {written} frames, expected {expected}"),
        });
    }
    let file = sink
        .finish()
        .map_err(write_err)?
        .into_inner()
        .map_err(|e| write_err(WriteError::Io(e.into_error())))?;
    sync(&file, tmp)?;
    drop(file);
    report.set(WRITE_SHARE);

    copy_tags(src, tmp, spec.target)?;
    sync(&open(tmp)?, tmp)?;

    let check = read_back(tmp, cancel, &mut |done| {
        report.set(WRITE_SHARE + (1.0 - WRITE_SHARE) * (done as f32 / written.max(1) as f32));
    })?;
    let pcm_hash = hash.finish();
    let fail = |reason: String| {
        Err(Error::Verify {
            path: tmp.to_path_buf(),
            reason,
        })
    };
    if check.frames != written {
        return fail(format!(
            "read back {} frames, wrote {written}",
            check.frames
        ));
    }
    if (check.sample_rate, check.channels, check.bit_depth)
        != (pcm.sample_rate, pcm.channels, pcm.bits_per_sample)
    {
        return fail(format!(
            "read back {} Hz, {} ch, {} bits; wrote {:?}",
            check.sample_rate, check.channels, check.bit_depth, pcm
        ));
    }
    if check.hash != pcm_hash {
        return fail("audio read back differs from what was written".to_owned());
    }
    guard.keep();
    report.set(1.0);
    Ok(Output {
        frames: written,
        sample_rate: pcm.sample_rate,
        bit_depth: pcm.bits_per_sample,
        pcm_hash,
    })
}

/// BLAKE3 of a file's decoded audio: every sample at the file's bit depth
/// as a little-endian `i32`, interleaved. Two files with the same rate,
/// depth and channels hash the same exactly when their samples match.
pub fn pcm_hash(path: &Path) -> Result<[u8; 32]> {
    Ok(read_back(path, &CancelToken::new(), &mut |_| {})?.hash)
}

/// What a file decodes to.
struct ReadBack {
    frames: u64,
    sample_rate: u32,
    channels: u16,
    bit_depth: u8,
    hash: [u8; 32],
}

fn read_back(path: &Path, cancel: &CancelToken, done: &mut dyn FnMut(u64)) -> Result<ReadBack> {
    let mut stream = NativeStream::open(path)?;
    let format = *stream.format();
    let mut hash = PcmHasher::default();
    let mut chunk = Vec::new();
    while stream.read(&mut chunk)? {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled {
                path: path.to_path_buf(),
            });
        }
        hash.update(&chunk);
        done(stream.frames_read());
    }
    Ok(ReadBack {
        frames: stream.frames_read(),
        sample_rate: format.sample_rate,
        channels: format.channels,
        bit_depth: format.bits_per_sample,
        hash: hash.finish(),
    })
}

/// Copies `src`'s tags and pictures into `dst` as the target's tag type.
/// Items from the primary tag win; other tags fill keys it lacks. Items the
/// target tag cannot hold are dropped.
fn copy_tags(src: &Path, dst: &Path, target: Target) -> Result<()> {
    let file = crate::tags::read(src)?;
    let mut out = Tag::new(match target {
        Target::Wav | Target::Aiff => TagType::Id3v2,
        Target::Flac => TagType::VorbisComments,
    });
    let mut seen = HashSet::new();
    for tag in crate::tags::ordered_tags(&file) {
        let keys: Vec<_> = tag.items().map(|i| i.key()).collect();
        for item in tag.items().filter(|i| !seen.contains(&i.key())) {
            out.push(item.clone());
        }
        seen.extend(keys);
        if out.picture_count() == 0 {
            tag.pictures()
                .iter()
                .for_each(|p| out.push_picture(p.clone()));
        }
    }
    if out.is_empty() {
        return Ok(());
    }
    out.save_to_path(dst, WriteOptions::default())
        .map_err(|source| Error::TagWrite {
            path: dst.to_path_buf(),
            source,
        })
}

fn open(path: &Path) -> Result<File> {
    OpenOptions::new()
        .write(true)
        .open(path)
        .map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })
}

fn sync(file: &File, path: &Path) -> Result<()> {
    file.sync_all().map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// Removes the file on drop unless [`TempFile::keep`] was called.
struct TempFile(Option<PathBuf>);

impl TempFile {
    fn keep(mut self) {
        self.0 = None;
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            if let Err(e) = std::fs::remove_file(&path) {
                tracing::warn!(path = %path.display(), error = %e, "cannot remove temp file");
            }
        }
    }
}

/// The writer for one [`Target`].
enum Sink {
    Wav(WavWriter<BufWriter<File>>),
    Aiff(AiffWriter<BufWriter<File>>),
    Flac(Box<FlacWriter<BufWriter<File>>>),
}

impl Sink {
    fn new(
        target: Target,
        inner: BufWriter<File>,
        spec: PcmSpec,
    ) -> std::result::Result<Self, WriteError> {
        Ok(match target {
            Target::Wav => Self::Wav(WavWriter::new(inner, spec)?),
            Target::Aiff => Self::Aiff(AiffWriter::new(inner, spec)?),
            Target::Flac => Self::Flac(Box::new(FlacWriter::new(inner, spec)?)),
        })
    }

    fn write(&mut self, samples: &[i32]) -> std::result::Result<(), WriteError> {
        match self {
            Self::Wav(w) => w.write(samples),
            Self::Aiff(w) => w.write(samples),
            Self::Flac(w) => w.write(samples),
        }
    }

    fn frames(&self) -> u64 {
        match self {
            Self::Wav(w) => w.frames(),
            Self::Aiff(w) => w.frames(),
            Self::Flac(w) => w.frames(),
        }
    }

    fn finish(self) -> std::result::Result<BufWriter<File>, WriteError> {
        match self {
            Self::Wav(w) => w.finish(),
            Self::Aiff(w) => w.finish(),
            Self::Flac(w) => w.finish(),
        }
    }
}

/// [`pcm_hash`]'s hasher.
#[derive(Default)]
struct PcmHasher {
    hasher: blake3::Hasher,
    bytes: Vec<u8>,
}

impl PcmHasher {
    fn update(&mut self, samples: &[i32]) {
        self.bytes.clear();
        self.bytes
            .extend(samples.iter().flat_map(|s| s.to_le_bytes()));
        self.hasher.update(&self.bytes);
    }

    fn finish(&self) -> [u8; 32] {
        *self.hasher.finalize().as_bytes()
    }
}

/// Calls the progress callback when the value has moved by a percent.
struct Progress<'a> {
    callback: &'a dyn Fn(f32),
    last: f32,
}

impl<'a> Progress<'a> {
    fn new(callback: &'a dyn Fn(f32)) -> Self {
        callback(0.0);
        Self {
            callback,
            last: 0.0,
        }
    }

    fn set(&mut self, value: f32) {
        let value = value.clamp(0.0, 1.0);
        if value >= self.last + 0.01 || (value == 1.0 && self.last < 1.0) {
            self.last = value;
            (self.callback)(value);
        }
    }
}
