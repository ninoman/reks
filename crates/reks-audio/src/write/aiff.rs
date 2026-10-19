//! AIFF writer: `FORM`/`AIFF` with an 18-byte `COMM` chunk (80-bit extended
//! sample rate) and one `SSND` chunk, big-endian. Never AIFF-C.

use std::io::{Seek, SeekFrom, Write};

use super::{check_samples, encode, PcmSpec, PcmWriter, WriteError};

/// Header length: `FORM` (12) + `COMM` (8 + 18) + `SSND` header (8 + 8).
pub const AIFF_HEADER_LEN: usize = 54;

/// `FORM` size field offset.
const FORM_SIZE_AT: u64 = 4;
/// `COMM` `numSampleFrames` offset.
const FRAMES_AT: u64 = 22;
/// `SSND` size field offset.
const SSND_SIZE_AT: u64 = 42;
/// Largest `SSND` data length: the `FORM` size (46 + data + pad) must fit in a `u32`.
const MAX_DATA: u64 = u32::MAX as u64 - 47;

/// An integer sample rate as an IEEE 754 80-bit extended float, big-endian
/// (the `COMM` `sampleRate` field): 44,100 Hz is `40 0E AC 44 00 00 00 00 00 00`.
pub fn f80(rate: u32) -> [u8; 10] {
    let mut out = [0u8; 10];
    if rate == 0 {
        return out;
    }
    let msb = 31 - rate.leading_zeros();
    let exponent = 16_383 + msb as u16;
    // Explicit integer bit: the top set bit of the rate goes to mantissa bit 63.
    let mantissa = u64::from(rate) << (63 - msb);
    out[0..2].copy_from_slice(&exponent.to_be_bytes());
    out[2..10].copy_from_slice(&mantissa.to_be_bytes());
    out
}

/// The 54 header bytes for `frames` frames in `data_len` bytes of samples.
/// The `FORM` size counts the pad byte that follows odd-length data.
pub fn aiff_header(spec: &PcmSpec, frames: u32, data_len: u32) -> [u8; AIFF_HEADER_LEN] {
    let pad = data_len & 1;
    let form_size = 46u32.saturating_add(data_len).saturating_add(pad);
    let ssnd_size = 8u32.saturating_add(data_len);

    let mut h = [0u8; AIFF_HEADER_LEN];
    h[0..4].copy_from_slice(b"FORM");
    h[4..8].copy_from_slice(&form_size.to_be_bytes());
    h[8..12].copy_from_slice(b"AIFF");
    h[12..16].copy_from_slice(b"COMM");
    h[16..20].copy_from_slice(&18u32.to_be_bytes());
    h[20..22].copy_from_slice(&spec.channels.to_be_bytes());
    h[22..26].copy_from_slice(&frames.to_be_bytes());
    h[26..28].copy_from_slice(&u16::from(spec.bits_per_sample).to_be_bytes());
    h[28..38].copy_from_slice(&f80(spec.sample_rate));
    h[38..42].copy_from_slice(b"SSND");
    h[42..46].copy_from_slice(&ssnd_size.to_be_bytes());
    // offset (46..50) and blockSize (50..54) stay 0.
    h
}

/// Streams an AIFF file into `W`; sizes are written by [`PcmWriter::finish`].
#[derive(Debug)]
pub struct AiffWriter<W: Write + Seek> {
    inner: W,
    spec: PcmSpec,
    /// Stream position of the `FORM` tag.
    start: u64,
    data_len: u64,
    buf: Vec<u8>,
}

impl<W: Write + Seek> AiffWriter<W> {
    /// Checks `spec` and writes a header with zero sizes at the current position.
    pub fn new(mut inner: W, spec: PcmSpec) -> Result<Self, WriteError> {
        spec.check()?;
        let start = inner.stream_position()?;
        inner.write_all(&aiff_header(&spec, 0, 0))?;
        Ok(Self {
            inner,
            spec,
            start,
            data_len: 0,
            buf: Vec::new(),
        })
    }
}

impl<W: Write + Seek> PcmWriter<W> for AiffWriter<W> {
    fn spec(&self) -> PcmSpec {
        self.spec
    }

    fn write(&mut self, samples: &[i32]) -> Result<(), WriteError> {
        check_samples(&self.spec, samples)?;
        self.buf.clear();
        encode(&self.spec, samples, true, &mut self.buf);
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
        let frames = u32::try_from(self.frames()).map_err(|_| WriteError::TooLarge)?;
        if data_len & 1 == 1 {
            self.inner.write_all(&[0])?;
        }
        let end = self.inner.stream_position()?;
        let header = aiff_header(&self.spec, frames, data_len);
        for at in [FORM_SIZE_AT, FRAMES_AT, SSND_SIZE_AT] {
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

    #[test]
    fn extended_sample_rates() {
        let cases: [(u32, [u8; 10]); 4] = [
            (44_100, [0x40, 0x0E, 0xAC, 0x44, 0, 0, 0, 0, 0, 0]),
            (48_000, [0x40, 0x0E, 0xBB, 0x80, 0, 0, 0, 0, 0, 0]),
            (96_000, [0x40, 0x0F, 0xBB, 0x80, 0, 0, 0, 0, 0, 0]),
            (1, [0x3F, 0xFF, 0x80, 0, 0, 0, 0, 0, 0, 0]),
        ];
        for (rate, bytes) in cases {
            assert_eq!(f80(rate), bytes, "{rate}");
        }
        assert_eq!(f80(0), [0; 10]);
    }

    #[test]
    fn exact_bytes_of_a_short_file() {
        let spec = PcmSpec {
            sample_rate: 44_100,
            channels: 2,
            bits_per_sample: 16,
        };
        let mut w = AiffWriter::new(Cursor::new(Vec::new()), spec).unwrap();
        w.write(&[1, -2, 0x1234, -32_768]).unwrap();
        assert_eq!(w.frames(), 2);
        let bytes = w.finish().unwrap().into_inner();
        #[rustfmt::skip]
        let expected: [u8; 62] = [
            b'F', b'O', b'R', b'M', 0, 0, 0, 54, b'A', b'I', b'F', b'F',
            b'C', b'O', b'M', b'M', 0, 0, 0, 18,
            0, 2, 0, 0, 0, 2, 0, 16, 0x40, 0x0E, 0xAC, 0x44, 0, 0, 0, 0, 0, 0,
            b'S', b'S', b'N', b'D', 0, 0, 0, 16, 0, 0, 0, 0, 0, 0, 0, 0,
            0x00, 0x01, 0xFF, 0xFE, 0x12, 0x34, 0x80, 0x00,
        ];
        assert_eq!(bytes, expected);
    }

    #[test]
    fn odd_data_gets_a_pad_byte() {
        let spec = PcmSpec {
            sample_rate: 48_000,
            channels: 1,
            bits_per_sample: 24,
        };
        let mut w = AiffWriter::new(Cursor::new(Vec::new()), spec).unwrap();
        w.write(&[-0x80_0000]).unwrap();
        let bytes = w.finish().unwrap().into_inner();
        assert_eq!(bytes.len(), 54 + 3 + 1);
        assert_eq!(u32::from_be_bytes(bytes[4..8].try_into().unwrap()), 50);
        assert_eq!(u32::from_be_bytes(bytes[22..26].try_into().unwrap()), 1);
        assert_eq!(u16::from_be_bytes([bytes[26], bytes[27]]), 24);
        assert_eq!(u32::from_be_bytes(bytes[42..46].try_into().unwrap()), 11);
        assert_eq!(&bytes[54..], &[0x80, 0x00, 0x00, 0x00]);
    }
}
