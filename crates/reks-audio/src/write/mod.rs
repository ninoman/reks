//! Streaming PCM file writers for the converter (docs/decisions.md D25):
//! WAV ([`WavWriter`]), AIFF ([`AiffWriter`]) and FLAC ([`FlacWriter`]).
//!
//! Every writer takes interleaved integer samples at the output bit depth
//! (a 24-bit sample is in `-2^23..2^23`), writes as it goes and patches the
//! header sizes in `finish()`. Only the forms older players load are written:
//! 1 or 2 channels, 16 or 24 bits; WAV is always `fmt` tag 1 (never
//! `WAVE_FORMAT_EXTENSIBLE` or float), AIFF is always plain `AIFF` (never AIFF-C).

mod aiff;
mod flac;
mod wav;

use std::io;

pub use aiff::{aiff_header, f80, AiffWriter, AIFF_HEADER_LEN};
pub use flac::{FlacWriter, FLAC_BLOCK_SIZE};
pub use wav::{wav_header, WavWriter, WAV_HEADER_LEN};

/// Output format of a writer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PcmSpec {
    pub sample_rate: u32,
    /// 1 or 2.
    pub channels: u16,
    /// 16 or 24.
    pub bits_per_sample: u8,
}

impl PcmSpec {
    /// Rejects anything but 1–2 channels at 16 or 24 bits and a non-zero rate.
    pub fn check(&self) -> Result<(), WriteError> {
        if !(1..=2).contains(&self.channels) {
            return Err(WriteError::Spec(format!("{} channels", self.channels)));
        }
        if !matches!(self.bits_per_sample, 16 | 24) {
            return Err(WriteError::Spec(format!("{} bits", self.bits_per_sample)));
        }
        if self.sample_rate == 0 {
            return Err(WriteError::Spec("sample rate 0".to_owned()));
        }
        Ok(())
    }

    /// Bytes per sample in WAV and AIFF: 2 or 3.
    pub fn bytes_per_sample(&self) -> u16 {
        u16::from(self.bits_per_sample).div_ceil(8)
    }

    /// Bytes per frame (all channels).
    pub fn block_align(&self) -> u16 {
        self.channels * self.bytes_per_sample()
    }
}

/// Errors from the writers.
#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    #[error(transparent)]
    Io(#[from] io::Error),

    /// The [`PcmSpec`] is not one we write.
    #[error("unsupported output format: {0}")]
    Spec(String),

    /// A sample outside the range of the output bit depth.
    #[error("sample {value} does not fit in {bits} bits")]
    Range { value: i32, bits: u8 },

    /// The buffer does not hold whole frames.
    #[error("{samples} samples is not a whole number of {channels}-channel frames")]
    PartialFrame { samples: usize, channels: u16 },

    /// WAV and AIFF sizes are 32-bit: about 6 hours of 24-bit stereo at 48 kHz.
    #[error("audio data is larger than the 4 GiB the format allows")]
    TooLarge,

    /// flacenc rejected a frame or its parameters.
    #[error("FLAC encoder: {0}")]
    Flac(String),
}

/// Common interface of the three writers.
pub trait PcmWriter<W> {
    /// The format being written.
    fn spec(&self) -> PcmSpec;

    /// Appends interleaved samples; `samples.len()` must be a whole number of frames.
    fn write(&mut self, samples: &[i32]) -> Result<(), WriteError>;

    /// Frames written so far.
    fn frames(&self) -> u64;

    /// Writes the final sizes into the header, flushes and returns the sink.
    fn finish(self) -> Result<W, WriteError>;
}

/// Checks that `samples` holds whole frames in range of the spec's bit depth.
fn check_samples(spec: &PcmSpec, samples: &[i32]) -> Result<(), WriteError> {
    if !samples.len().is_multiple_of(usize::from(spec.channels)) {
        return Err(WriteError::PartialFrame {
            samples: samples.len(),
            channels: spec.channels,
        });
    }
    let max = (1i32 << (spec.bits_per_sample - 1)) - 1;
    let min = -max - 1;
    match samples.iter().find(|&&s| s < min || s > max) {
        Some(&value) => Err(WriteError::Range {
            value,
            bits: spec.bits_per_sample,
        }),
        None => Ok(()),
    }
}

/// Appends `samples` to `out` as 2- or 3-byte integers; `spec` and the
/// range are already checked.
fn encode(spec: &PcmSpec, samples: &[i32], big_endian: bool, out: &mut Vec<u8>) {
    let width = usize::from(spec.bytes_per_sample());
    out.reserve(samples.len() * width);
    for &s in samples {
        let le = s.to_le_bytes();
        let bytes = &le[..width];
        if big_endian {
            out.extend(bytes.iter().rev());
        } else {
            out.extend_from_slice(bytes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_check() {
        let ok = PcmSpec {
            sample_rate: 44_100,
            channels: 2,
            bits_per_sample: 24,
        };
        assert!(ok.check().is_ok());
        assert_eq!(ok.block_align(), 6);
        for bad in [
            PcmSpec { channels: 0, ..ok },
            PcmSpec { channels: 3, ..ok },
            PcmSpec {
                bits_per_sample: 32,
                ..ok
            },
            PcmSpec {
                bits_per_sample: 8,
                ..ok
            },
            PcmSpec {
                sample_rate: 0,
                ..ok
            },
        ] {
            assert!(matches!(bad.check(), Err(WriteError::Spec(_))), "{bad:?}");
        }
    }

    #[test]
    fn sample_checks() {
        let spec = PcmSpec {
            sample_rate: 48_000,
            channels: 2,
            bits_per_sample: 16,
        };
        assert!(check_samples(&spec, &[32_767, -32_768]).is_ok());
        assert!(matches!(
            check_samples(&spec, &[32_768, 0]),
            Err(WriteError::Range { value: 32_768, .. })
        ));
        assert!(matches!(
            check_samples(&spec, &[0, 0, 0]),
            Err(WriteError::PartialFrame { samples: 3, .. })
        ));
    }

    #[test]
    fn encodes_both_byte_orders() {
        let spec = PcmSpec {
            sample_rate: 48_000,
            channels: 1,
            bits_per_sample: 24,
        };
        let mut le = Vec::new();
        encode(&spec, &[0x12_3456, -2], false, &mut le);
        assert_eq!(le, [0x56, 0x34, 0x12, 0xFE, 0xFF, 0xFF]);
        let mut be = Vec::new();
        encode(&spec, &[0x12_3456, -2], true, &mut be);
        assert_eq!(be, [0x12, 0x34, 0x56, 0xFF, 0xFF, 0xFE]);
    }
}
