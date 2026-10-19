//! Streaming decode at the source's own precision, for the converter.
//!
//! The preview path ([`crate::decode_stereo`]) decodes whole tracks to stereo
//! `i16` (docs/decisions.md D5); this one keeps every channel and every bit:
//! integer samples right-aligned at the source's bit depth (a 24-bit sample is
//! in `-2^23..2^23`), one decoded packet at a time, so memory stays bounded.
//!
//! Unlike playback, a packet that fails to decode is an error, not a skip:
//! dropping audio would shift every cue after it. FLAC files are checked
//! against their STREAMINFO MD5 at the end of the stream.

use std::path::{Path, PathBuf};

use symphonia::core::codecs::audio::well_known::{
    CODEC_ID_PCM_F32BE, CODEC_ID_PCM_F32LE, CODEC_ID_PCM_F64BE, CODEC_ID_PCM_F64LE,
};
use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::errors::Error as SymErr;
use symphonia::core::formats::FormatReader;

use crate::decode::{open_track, OpenTrack};
use crate::error::{Error, Result};
use crate::probe::decode_err;

/// What a [`NativeStream`] yields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeFormat {
    pub sample_rate: u32,
    pub channels: u16,
    /// Significant bits per sample (16, 24, 20 for some ALAC, …); 32 for float.
    pub bits_per_sample: u8,
    /// IEEE float source. Its samples come out as 32-bit integers, with
    /// anything beyond full scale clipped.
    pub float: bool,
    /// Frame count from the header, when known.
    pub frames: Option<u64>,
}

/// Packet-by-packet decoder of one file's default audio track.
pub struct NativeStream {
    path: PathBuf,
    reader: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    format: NativeFormat,
    /// Right shift from Symphonia's full-scale `i32` to `bits_per_sample`.
    shift: u32,
    frames_read: u64,
    done: bool,
}

impl std::fmt::Debug for NativeStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeStream")
            .field("path", &self.path)
            .field("format", &self.format)
            .field("frames_read", &self.frames_read)
            .finish_non_exhaustive()
    }
}

impl NativeStream {
    /// Opens `path` and reads its stream parameters.
    pub fn open(path: &Path) -> Result<Self> {
        let OpenTrack {
            format: reader,
            track_id,
            params,
            sample_rate,
            num_frames,
        } = open_track(path)?;
        let unsupported = |reason: String| Error::Unsupported {
            path: path.to_path_buf(),
            reason,
        };
        let channels = params
            .channels
            .as_ref()
            .and_then(|c| u16::try_from(c.count()).ok())
            .filter(|&c| c > 0)
            .ok_or_else(|| unsupported("unknown channel count".to_owned()))?;
        let float = matches!(
            params.codec,
            CODEC_ID_PCM_F32LE | CODEC_ID_PCM_F32BE | CODEC_ID_PCM_F64LE | CODEC_ID_PCM_F64BE
        );
        let bits_per_sample = if float {
            32
        } else {
            params
                .bits_per_sample
                .and_then(|b| u8::try_from(b).ok())
                .filter(|b| (1..=32).contains(b))
                .ok_or_else(|| unsupported("unknown bit depth".to_owned()))?
        };
        let options = AudioDecoderOptions::default().verify(true);
        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(&params, &options)
            .map_err(|source| decode_err(path, source))?;
        Ok(Self {
            path: path.to_path_buf(),
            reader,
            decoder,
            track_id,
            format: NativeFormat {
                sample_rate,
                channels,
                bits_per_sample,
                float,
                frames: num_frames,
            },
            shift: 32 - u32::from(bits_per_sample),
            frames_read: 0,
            done: false,
        })
    }

    /// The stream's parameters.
    pub fn format(&self) -> &NativeFormat {
        &self.format
    }

    /// Frames returned so far.
    pub fn frames_read(&self) -> u64 {
        self.frames_read
    }

    /// Replaces `out` with the next decoded packet: interleaved, whole frames,
    /// at [`NativeFormat::bits_per_sample`]. Returns `false` (and an empty
    /// `out`) at the end of the stream.
    pub fn read(&mut self, out: &mut Vec<i32>) -> Result<bool> {
        out.clear();
        while !self.done {
            let packet = match self.reader.next_packet() {
                Ok(Some(p)) => p,
                Ok(None) => {
                    self.end()?;
                    break;
                }
                // Truncated file: what decoded is the stream. The caller
                // compares `frames_read` with the header's count.
                Err(SymErr::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    self.end()?;
                    break;
                }
                Err(source) => return Err(decode_err(&self.path, source)),
            };
            if packet.track_id != self.track_id {
                continue;
            }
            let buf = self
                .decoder
                .decode(&packet)
                .map_err(|source| decode_err(&self.path, source))?;
            let channels = buf.spec().channels().count();
            if channels != usize::from(self.format.channels) {
                return Err(Error::Unsupported {
                    path: self.path.clone(),
                    reason: format!(
                        "channel count changed from {} to {channels}",
                        self.format.channels
                    ),
                });
            }
            buf.copy_to_vec_interleaved::<i32>(out);
            if out.is_empty() {
                continue;
            }
            if self.shift > 0 {
                // Exact: Symphonia scales integer sources up by this shift.
                out.iter_mut().for_each(|s| *s >>= self.shift);
            }
            self.frames_read += (out.len() / channels) as u64;
            return Ok(true);
        }
        Ok(false)
    }

    /// Marks the end and checks the decoder's verification (FLAC MD5).
    fn end(&mut self) -> Result<()> {
        self.done = true;
        if self.decoder.finalize().verify_ok == Some(false) {
            return Err(Error::Corrupt {
                path: self.path.clone(),
                reason: "decoded audio does not match the file's MD5".to_owned(),
            });
        }
        Ok(())
    }
}
