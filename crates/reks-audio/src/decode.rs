//! Whole-track decode to stereo `i16` for playback, with optional resampling
//! to the output device's rate and the overview waveform computed on the way.
//!
//! Mono is copied to both channels; channels past the second are dropped.
//! Resampling runs here, off the audio thread (docs/decisions.md D5).

use std::fs::File;
use std::path::Path;

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Indexing, Resampler};
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error as SymErr;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

use crate::error::{Error, Result};
use crate::overview::Overview;
use crate::probe::decode_err;

/// Longest track [`decode_stereo`] accepts: about 690 MB of PCM at 48 kHz.
pub const MAX_DECODE_MS: u64 = 60 * 60 * 1000;

/// Frames per resampler chunk.
const CHUNK: usize = 1024;

/// Interleaved stereo PCM.
#[derive(Clone, PartialEq, Eq)]
pub struct StereoPcm {
    /// `L R L R …`; always an even length.
    pub samples: Vec<i16>,
    pub sample_rate: u32,
}

impl StereoPcm {
    /// Number of stereo frames.
    pub fn frames(&self) -> usize {
        self.samples.len() / 2
    }

    /// Length in milliseconds, rounded down.
    pub fn duration_ms(&self) -> u64 {
        self.frames() as u64 * 1000 / u64::from(self.sample_rate.max(1))
    }
}

impl std::fmt::Debug for StereoPcm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StereoPcm")
            .field("frames", &self.frames())
            .field("sample_rate", &self.sample_rate)
            .finish()
    }
}

/// What [`decode_stereo`] returns.
#[derive(Debug, Clone, PartialEq)]
pub struct Decoded {
    /// At `target_rate` when one was given, else at the file's rate.
    pub pcm: StereoPcm,
    /// The file's own sample rate.
    pub source_rate: u32,
    /// Overview waveform at the file's rate; see [`crate::overview`].
    pub overview: Vec<f32>,
}

/// Decodes all of `path`. With `target_rate`, the PCM is resampled to it;
/// the overview is always computed at the file's rate, so it does not depend
/// on the device.
pub fn decode_stereo(path: &Path, target_rate: Option<u32>) -> Result<Decoded> {
    let file = File::open(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|source| decode_err(path, source))?;

    let unsupported = |reason: &str| Error::Unsupported {
        path: path.to_path_buf(),
        reason: reason.to_owned(),
    };
    let track = format
        .default_track(TrackType::Audio)
        .ok_or_else(|| unsupported("no audio track"))?;
    let track_id = track.id;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .ok_or_else(|| unsupported("no audio codec parameters"))?
        .clone();
    let source_rate = params
        .sample_rate
        .filter(|&sr| sr > 0)
        .ok_or_else(|| unsupported("unknown sample rate"))?;
    let max_frames = MAX_DECODE_MS * u64::from(source_rate) / 1000;
    if let Some(frames) = track.num_frames.filter(|&f| f > max_frames) {
        return Err(too_long(path, frames, source_rate));
    }

    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(&params, &AudioDecoderOptions::default())
        .map_err(|source| decode_err(path, source))?;

    let target = target_rate.unwrap_or(source_rate);
    let mut sink = Sink::new(path, source_rate, target, track.num_frames)?;
    let mut overview = Overview::new();
    let mut frame_count: u64 = 0;
    let mut scratch: Vec<f32> = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) => break,
            // Truncated file: keep what decoded.
            Err(SymErr::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(source) => return Err(decode_err(path, source)),
        };
        if packet.track_id != track_id {
            continue;
        }
        let buf = match decoder.decode(&packet) {
            Ok(buf) => buf,
            // A corrupt packet: skip it, as players do.
            Err(SymErr::DecodeError(msg)) => {
                tracing::debug!(path = %path.display(), msg, "skipped undecodable packet");
                continue;
            }
            Err(source) => return Err(decode_err(path, source)),
        };
        let channels = buf.spec().channels().count();
        if channels == 0 {
            continue;
        }
        buf.copy_to_vec_interleaved::<f32>(&mut scratch);
        for frame in scratch.chunks_exact(channels) {
            let l = frame[0];
            let r = if channels > 1 { frame[1] } else { l };
            overview.push(l, r);
            sink.push(l, r)?;
        }
        frame_count += (scratch.len() / channels) as u64;
        if frame_count > max_frames {
            return Err(too_long(path, frame_count, source_rate));
        }
    }

    Ok(Decoded {
        pcm: sink.finish(frame_count)?,
        source_rate,
        overview: overview.finish(),
    })
}

/// Collects stereo frames, resampling in chunks when the rates differ.
struct Sink<'p> {
    path: &'p Path,
    out: Vec<i16>,
    target_rate: u32,
    resampler: Option<Chunked>,
}

struct Chunked {
    fft: Fft<f32>,
    input: Vec<f32>,
    output: Vec<f32>,
    /// Output frames still to drop for the resampler's delay.
    skip: usize,
    from: u64,
    to: u64,
}

impl<'p> Sink<'p> {
    fn new(path: &'p Path, from: u32, to: u32, frames_hint: Option<u64>) -> Result<Self> {
        let ratio = f64::from(to) / f64::from(from);
        let capacity = frames_hint
            .map(|f| (f as f64 * ratio) as usize * 2 + CHUNK * 2)
            .unwrap_or(0);
        let resampler = if from == to {
            None
        } else {
            let fft = Fft::<f32>::new(from as usize, to as usize, CHUNK, 2, FixedSync::Input)
                .map_err(|e| resample_err(path, e))?;
            let output = vec![0.0; fft.output_frames_max() * 2];
            let skip = fft.output_delay();
            Some(Chunked {
                fft,
                input: Vec::with_capacity(CHUNK * 2),
                output,
                skip,
                from: u64::from(from),
                to: u64::from(to),
            })
        };
        Ok(Self {
            path,
            out: Vec::with_capacity(capacity),
            target_rate: to,
            resampler,
        })
    }

    fn push(&mut self, l: f32, r: f32) -> Result<()> {
        let Some(rs) = self.resampler.as_mut() else {
            self.out.push(to_i16(l));
            self.out.push(to_i16(r));
            return Ok(());
        };
        rs.input.push(l);
        rs.input.push(r);
        if rs.input.len() == CHUNK * 2 {
            run_chunk(self.path, rs, &mut self.out, None)?;
        }
        Ok(())
    }

    fn finish(mut self, input_frames: u64) -> Result<StereoPcm> {
        if let Some(rs) = self.resampler.as_mut() {
            let expected =
                usize::try_from((input_frames * rs.to).div_ceil(rs.from)).unwrap_or(usize::MAX);
            // The last partial chunk, then silence until the delay is flushed.
            let partial = rs.input.len() / 2;
            rs.input.resize(CHUNK * 2, 0.0);
            run_chunk(self.path, rs, &mut self.out, Some(partial))?;
            while self.out.len() / 2 < expected {
                rs.input.resize(CHUNK * 2, 0.0);
                run_chunk(self.path, rs, &mut self.out, Some(0))?;
            }
            self.out.truncate(expected * 2);
        }
        Ok(StereoPcm {
            samples: self.out,
            sample_rate: self.target_rate,
        })
    }
}

fn run_chunk(
    path: &Path,
    rs: &mut Chunked,
    out: &mut Vec<i16>,
    partial: Option<usize>,
) -> Result<()> {
    let input =
        InterleavedSlice::new(&rs.input[..], 2, CHUNK).map_err(|e| resample_err(path, e))?;
    let frames_out = rs.output.len() / 2;
    let mut output = InterleavedSlice::new_mut(&mut rs.output[..], 2, frames_out)
        .map_err(|e| resample_err(path, e))?;
    let indexing = partial.map(|n| Indexing::new().partial_len(n));
    let (_, written) = rs
        .fft
        .process_into_buffer(&input, &mut output, indexing.as_ref())
        .map_err(|e| resample_err(path, e))?;
    let drop = rs.skip.min(written);
    rs.skip -= drop;
    out.extend(rs.output[drop * 2..written * 2].iter().map(|&s| to_i16(s)));
    rs.input.clear();
    Ok(())
}

fn to_i16(s: f32) -> i16 {
    // Rounds to nearest; clamps overs from lossy decoders and the resampler.
    (s * 32_768.0).round().clamp(-32_768.0, 32_767.0) as i16
}

fn too_long(path: &Path, frames: u64, rate: u32) -> Error {
    Error::TooLong {
        path: path.to_path_buf(),
        duration_ms: frames * 1000 / u64::from(rate),
        max_ms: MAX_DECODE_MS,
    }
}

fn resample_err(path: &Path, e: impl std::fmt::Display) -> Error {
    Error::Resample {
        path: path.to_path_buf(),
        reason: e.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_i16_rounds_and_clamps() {
        assert_eq!(to_i16(0.0), 0);
        assert_eq!(to_i16(1.0), 32_767);
        assert_eq!(to_i16(-1.0), -32_768);
        assert_eq!(to_i16(1.5), 32_767);
        assert_eq!(to_i16(0.5 / 32_768.0), 1);
    }
}
