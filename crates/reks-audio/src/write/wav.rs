//! WAV writer: `RIFF`/`WAVE` with a 16-byte `fmt ` chunk (format tag 1,
//! integer PCM) and one `data` chunk, little-endian.
//!
//! `hound` writes `WAVE_FORMAT_EXTENSIBLE` (tag 0xFFFE) above 16 bits, which
//! older CDJs reject (E-8305, docs/research.md §4); this writer never does.

use std::io::{Seek, SeekFrom, Write};

use super::{check_samples, encode, PcmSpec, PcmWriter, WriteError};

/// Header length: `RIFF` (12) + `fmt ` (8 + 16) + `data` header (8).
pub const WAV_HEADER_LEN: usize = 44;

/// `RIFF` size field offset.
const RIFF_SIZE_AT: u64 = 4;
/// `data` size field offset.
const DATA_SIZE_AT: u64 = 40;
/// Largest `data` length: the `RIFF` size (36 + data + pad byte) must fit in a `u32`.
const MAX_DATA: u64 = u32::MAX as u64 - 37;

/// The 44 header bytes for `data_len` bytes of samples. The `RIFF` size
/// counts the pad byte that follows odd-length data.
pub fn wav_header(spec: &PcmSpec, data_len: u32) -> [u8; WAV_HEADER_LEN] {
    let pad = data_len & 1;
    let riff_size = 36u32.saturating_add(data_len).saturating_add(pad);
    let block_align = spec.block_align();
    let byte_rate = spec.sample_rate.saturating_mul(u32::from(block_align));

    let mut h = [0u8; WAV_HEADER_LEN];
    h[0..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&riff_size.to_le_bytes());
    h[8..12].copy_from_slice(b"WAVE");
    h[12..16].copy_from_slice(b"fmt ");
    h[16..20].copy_from_slice(&16u32.to_le_bytes());
    h[20..22].copy_from_slice(&1u16.to_le_bytes()); // WAVE_FORMAT_PCM
    h[22..24].copy_from_slice(&spec.channels.to_le_bytes());
    h[24..28].copy_from_slice(&spec.sample_rate.to_le_bytes());
    h[28..32].copy_from_slice(&byte_rate.to_le_bytes());
    h[32..34].copy_from_slice(&block_align.to_le_bytes());
    h[34..36].copy_from_slice(&u16::from(spec.bits_per_sample).to_le_bytes());
    h[36..40].copy_from_slice(b"data");
    h[40..44].copy_from_slice(&data_len.to_le_bytes());
    h
}

/// Streams a WAV file into `W`; sizes are written by [`PcmWriter::finish`].
#[derive(Debug)]
pub struct WavWriter<W: Write + Seek> {
    inner: W,
    spec: PcmSpec,
    /// Stream position of the `RIFF` tag.
    start: u64,
    data_len: u64,
    buf: Vec<u8>,
}

impl<W: Write + Seek> WavWriter<W> {
    /// Checks `spec` and writes a header with zero sizes at the current position.
    pub fn new(mut inner: W, spec: PcmSpec) -> Result<Self, WriteError> {
        spec.check()?;
        let start = inner.stream_position()?;
        inner.write_all(&wav_header(&spec, 0))?;
        Ok(Self {
            inner,
            spec,
            start,
            data_len: 0,
            buf: Vec::new(),
        })
    }
}

impl<W: Write + Seek> PcmWriter<W> for WavWriter<W> {
    fn spec(&self) -> PcmSpec {
        self.spec
    }

    fn write(&mut self, samples: &[i32]) -> Result<(), WriteError> {
        check_samples(&self.spec, samples)?;
        self.buf.clear();
        encode(&self.spec, samples, false, &mut self.buf);
        let len = self.data_len + self.buf.len() as u64;
        if len > MAX_DATA {
            return Err(WriteError::TooLarge);
        }
        self.inner.write_all(&self.buf)?;
        self.data_len = len;
        Ok(())
    }

    fn frames(&self) -> u64 {
        self.data_len / u64::from(self.spec.block_align())
    }

    fn finish(mut self) -> Result<W, WriteError> {
        let data_len = u32::try_from(self.data_len).map_err(|_| WriteError::TooLarge)?;
        if data_len & 1 == 1 {
            self.inner.write_all(&[0])?;
        }
        let end = self.inner.stream_position()?;
        let header = wav_header(&self.spec, data_len);
        for at in [RIFF_SIZE_AT, DATA_SIZE_AT] {
            self.inner.seek(SeekFrom::Start(self.start + at))?;
            let at = at as usize;
            self.inner.write_all(&header[at..at + 4])?;
        }
        self.inner.seek(SeekFrom::Start(end))?;
        self.inner.flush()?;
        Ok(self.inner)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    fn spec(channels: u16, bits: u8) -> PcmSpec {
        PcmSpec {
            sample_rate: 44_100,
            channels,
            bits_per_sample: bits,
        }
    }

    #[test]
    fn header_fields_at_their_offsets() {
        let h = wav_header(&spec(2, 24), 600);
        assert_eq!(&h[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(h[4..8].try_into().unwrap()), 636);
        assert_eq!(&h[8..16], b"WAVEfmt ");
        assert_eq!(u32::from_le_bytes(h[16..20].try_into().unwrap()), 16);
        // Format tag 1 (PCM) even at 24 bits, never 0xFFFE (EXTENSIBLE).
        assert_eq!(&h[20..22], &[0x01, 0x00]);
        assert_eq!(u16::from_le_bytes([h[22], h[23]]), 2);
        assert_eq!(u32::from_le_bytes(h[24..28].try_into().unwrap()), 44_100);
        assert_eq!(u32::from_le_bytes(h[28..32].try_into().unwrap()), 264_600);
        assert_eq!(u16::from_le_bytes([h[32], h[33]]), 6);
        assert_eq!(u16::from_le_bytes([h[34], h[35]]), 24);
        assert_eq!(&h[36..40], b"data");
        assert_eq!(u32::from_le_bytes(h[40..44].try_into().unwrap()), 600);
    }

    #[test]
    fn exact_bytes_of_a_short_file() {
        let mut w = WavWriter::new(Cursor::new(Vec::new()), spec(1, 16)).unwrap();
        w.write(&[1, -1]).unwrap();
        assert_eq!(w.frames(), 2);
        let bytes = w.finish().unwrap().into_inner();
        #[rustfmt::skip]
        let expected: [u8; 48] = [
            b'R', b'I', b'F', b'F', 40, 0, 0, 0, b'W', b'A', b'V', b'E',
            b'f', b'm', b't', b' ', 16, 0, 0, 0,
            1, 0, 1, 0, 0x44, 0xAC, 0, 0, 0x88, 0x58, 0x01, 0, 2, 0, 16, 0,
            b'd', b'a', b't', b'a', 4, 0, 0, 0,
            0x01, 0x00, 0xFF, 0xFF,
        ];
        assert_eq!(bytes, expected);
    }

    #[test]
    fn odd_data_gets_a_pad_byte() {
        let mut w = WavWriter::new(Cursor::new(Vec::new()), spec(1, 24)).unwrap();
        w.write(&[0x7F_FFFF]).unwrap();
        let bytes = w.finish().unwrap().into_inner();
        assert_eq!(bytes.len(), 44 + 3 + 1);
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 40);
        assert_eq!(u32::from_le_bytes(bytes[40..44].try_into().unwrap()), 3);
        assert_eq!(&bytes[44..], &[0xFF, 0xFF, 0x7F, 0x00]);
    }

    #[test]
    fn starts_at_the_current_position() {
        let mut cursor = Cursor::new(Vec::new());
        cursor.write_all(b"xx").unwrap();
        let mut w = WavWriter::new(cursor, spec(2, 16)).unwrap();
        w.write(&[3, 4]).unwrap();
        let bytes = w.finish().unwrap().into_inner();
        assert_eq!(&bytes[2..6], b"RIFF");
        assert_eq!(u32::from_le_bytes(bytes[6..10].try_into().unwrap()), 40);
        assert_eq!(u32::from_le_bytes(bytes[42..46].try_into().unwrap()), 4);
    }

    #[test]
    fn rejects_out_of_range_samples() {
        let mut w = WavWriter::new(Cursor::new(Vec::new()), spec(1, 16)).unwrap();
        assert!(matches!(w.write(&[40_000]), Err(WriteError::Range { .. })));
        assert_eq!(w.frames(), 0);
    }
}
