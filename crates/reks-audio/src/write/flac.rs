//! FLAC writer on `flacenc` (Apache-2.0, docs/decisions.md D25).
//!
//! Encodes one fixed-size frame at a time with `encode_fixed_size_frame` and
//! writes it straight out, so neither the PCM nor the encoded stream is ever
//! held whole (flacenc's own `encode_with_fixed_block_size` builds the whole
//! stream in memory). The file is `fLaC`, one STREAMINFO block (marked
//! last) and the frames; `finish` writes the final STREAMINFO with the sample
//! count, frame sizes and the MD5 of the samples. Tags are added afterwards (lofty).

use std::io::{Seek, SeekFrom, Write};

use flacenc::bitsink::MemSink;
use flacenc::component::{BitRepr, StreamInfo};
use flacenc::config;
use flacenc::error::{Verified, Verify};
use flacenc::source::{Context, Fill, FrameBuf};

use super::{check_samples, PcmSpec, PcmWriter, WriteError};

/// Frames per FLAC block: the reference encoder's default.
pub const FLAC_BLOCK_SIZE: usize = 4096;

/// `fLaC` + metadata block header.
const STREAMINFO_AT: u64 = 8;
/// STREAMINFO body length.
const STREAMINFO_LEN: usize = 34;

/// Streams a FLAC file into `W`.
pub struct FlacWriter<W: Write + Seek> {
    inner: W,
    spec: PcmSpec,
    /// Stream position of `fLaC`.
    start: u64,
    config: Verified<config::Encoder>,
    info: StreamInfo,
    framebuf: FrameBuf,
    context: Context,
    /// Interleaved samples of the block being filled.
    pending: Vec<i32>,
    sink: MemSink<u8>,
    frames: u64,
}

impl<W: Write + Seek> std::fmt::Debug for FlacWriter<W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FlacWriter")
            .field("spec", &self.spec)
            .field("frames", &self.frames)
            .finish_non_exhaustive()
    }
}

impl<W: Write + Seek> FlacWriter<W> {
    /// Checks `spec` and writes `fLaC` and a placeholder STREAMINFO at the
    /// current position.
    pub fn new(mut inner: W, spec: PcmSpec) -> Result<Self, WriteError> {
        spec.check()?;
        let channels = usize::from(spec.channels);
        let bits = usize::from(spec.bits_per_sample);
        let mut config = config::Encoder::default();
        config.block_size = FLAC_BLOCK_SIZE;
        let config = config.into_verified().map_err(|(_, e)| flac_err(e))?;
        let info = StreamInfo::new(spec.sample_rate as usize, channels, bits).map_err(flac_err)?;
        let framebuf = FrameBuf::with_size(channels, FLAC_BLOCK_SIZE).map_err(flac_err)?;

        let start = inner.stream_position()?;
        let mut writer = Self {
            inner,
            spec,
            start,
            config,
            info,
            framebuf,
            context: Context::new(bits, channels),
            pending: Vec::with_capacity(FLAC_BLOCK_SIZE * channels),
            sink: MemSink::new(),
            frames: 0,
        };
        let header = writer.header()?;
        writer.inner.write_all(&header)?;
        Ok(writer)
    }

    /// `fLaC`, the STREAMINFO block header (last block, length 34) and body.
    fn header(&mut self) -> Result<Vec<u8>, WriteError> {
        self.sink.clear();
        self.info.write(&mut self.sink).map_err(flac_err)?;
        let body = self.sink.as_slice();
        if body.len() != STREAMINFO_LEN {
            return Err(WriteError::Flac(format!(
                "STREAMINFO is {} bytes",
                body.len()
            )));
        }
        let mut out = Vec::with_capacity(STREAMINFO_AT as usize + STREAMINFO_LEN);
        out.extend_from_slice(b"fLaC");
        out.extend_from_slice(&[0x80, 0x00, 0x00, STREAMINFO_LEN as u8]);
        out.extend_from_slice(body);
        Ok(out)
    }

    /// Encodes and writes the samples in `pending` as one frame.
    fn flush_block(&mut self) -> Result<(), WriteError> {
        if self.pending.is_empty() {
            return Ok(());
        }
        self.framebuf
            .fill_interleaved(&self.pending)
            .map_err(flac_err)?;
        self.context
            .fill_interleaved(&self.pending)
            .map_err(flac_err)?;
        let number = self
            .context
            .current_frame_number()
            .ok_or_else(|| WriteError::Flac("no frame number".to_owned()))?;
        let frame =
            flacenc::encode_fixed_size_frame(&self.config, &self.framebuf, number, &self.info)
                .map_err(flac_err)?;
        self.info.update_frame_info(&frame);
        self.sink.clear();
        frame.write(&mut self.sink).map_err(flac_err)?;
        self.inner.write_all(self.sink.as_slice())?;
        self.pending.clear();
        Ok(())
    }
}

impl<W: Write + Seek> PcmWriter<W> for FlacWriter<W> {
    fn spec(&self) -> PcmSpec {
        self.spec
    }

    fn write(&mut self, samples: &[i32]) -> Result<(), WriteError> {
        check_samples(&self.spec, samples)?;
        let block = FLAC_BLOCK_SIZE * usize::from(self.spec.channels);
        let mut rest = samples;
        while !rest.is_empty() {
            let take = (block - self.pending.len()).min(rest.len());
            let (head, tail) = rest.split_at(take);
            self.pending.extend_from_slice(head);
            rest = tail;
            if self.pending.len() == block {
                self.flush_block()?;
            }
        }
        self.frames += (samples.len() / usize::from(self.spec.channels)) as u64;
        Ok(())
    }

    fn frames(&self) -> u64 {
        self.frames
    }

    fn finish(mut self) -> Result<W, WriteError> {
        self.flush_block()?;
        // Fixed block size; STREAMINFO's minimum excludes the short last block.
        self.info
            .set_block_sizes(FLAC_BLOCK_SIZE, FLAC_BLOCK_SIZE)
            .map_err(flac_err)?;
        if self.frames == 0 {
            self.info.set_frame_sizes(0, 0).map_err(flac_err)?;
        }
        self.info.set_md5_digest(&self.context.md5_digest());
        if self.info.total_samples() as u64 != self.frames {
            return Err(WriteError::Flac(format!(
                "encoded {} of {} frames",
                self.info.total_samples(),
                self.frames
            )));
        }
        let header = self.header()?;
        let end = self.inner.stream_position()?;
        self.inner.seek(SeekFrom::Start(self.start))?;
        self.inner.write_all(&header)?;
        self.inner.seek(SeekFrom::Start(end))?;
        self.inner.flush()?;
        Ok(self.inner)
    }
}

fn flac_err(e: impl std::fmt::Display) -> WriteError {
    WriteError::Flac(e.to_string())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    fn spec(channels: u16, bits: u8) -> PcmSpec {
        PcmSpec {
            sample_rate: 96_000,
            channels,
            bits_per_sample: bits,
        }
    }

    #[test]
    fn streaminfo_after_finish() {
        let mut w = FlacWriter::new(Cursor::new(Vec::new()), spec(2, 24)).unwrap();
        let samples: Vec<i32> = (0..10_000 * 2)
            .map(|i| (i * 37) % 4_000_000 - 2_000_000)
            .collect();
        // Odd-sized writes cross block boundaries.
        for chunk in samples.chunks(2 * 777) {
            w.write(chunk).unwrap();
        }
        assert_eq!(w.frames(), 10_000);
        let bytes = w.finish().unwrap().into_inner();
        assert_eq!(&bytes[0..4], b"fLaC");
        // Last metadata block, type 0 (STREAMINFO), 34 bytes.
        assert_eq!(&bytes[4..8], &[0x80, 0, 0, 34]);
        let si = &bytes[8..42];
        assert_eq!(u16::from_be_bytes([si[0], si[1]]), 4096);
        assert_eq!(u16::from_be_bytes([si[2], si[3]]), 4096);
        // 20-bit rate, 3-bit channels - 1, 5-bit bits - 1, 36-bit total samples.
        let packed = u64::from_be_bytes(si[10..18].try_into().unwrap());
        assert_eq!(packed >> 44, 96_000);
        assert_eq!((packed >> 41) & 0x7, 1);
        assert_eq!((packed >> 36) & 0x1F, 23);
        assert_eq!(packed & 0xF_FFFF_FFFF, 10_000);
        assert_ne!(&si[18..34], &[0; 16], "MD5 is set");
        // First frame sync code.
        assert_eq!(&bytes[42..44], &[0xFF, 0xF8]);
    }

    #[test]
    fn empty_stream_is_valid() {
        let w = FlacWriter::new(Cursor::new(Vec::new()), spec(1, 16)).unwrap();
        let bytes = w.finish().unwrap().into_inner();
        assert_eq!(bytes.len(), 42);
        let packed = u64::from_be_bytes(bytes[18..26].try_into().unwrap());
        assert_eq!(packed & 0xF_FFFF_FFFF, 0);
    }

    #[test]
    fn rejects_out_of_range_samples() {
        let mut w = FlacWriter::new(Cursor::new(Vec::new()), spec(1, 16)).unwrap();
        assert!(matches!(w.write(&[-32_769]), Err(WriteError::Range { .. })));
    }
}
