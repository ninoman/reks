//! reks audio: Symphonia decoding, resampling, overview waveform
//!
//! Phase 1: header probe ([`probe()`]), tags ([`tags()`]), embedded artwork
//! ([`embedded_artwork`]), whole-track decode for playback ([`decode_stereo`])
//! and the overview waveform ([`overview`]). See `docs/phases/phase-1.md`.
//!
//! Phase 2.6 converter building blocks: streaming decode at the source's
//! precision ([`NativeStream`]), streaming resampling ([`resample`]),
//! bit-depth conversion with TPDF dither ([`dither`]) and the WAV, AIFF and
//! FLAC writers ([`write`]), and the pipeline that joins them ([`convert()`]).
//! See `docs/phases/phase-2.6.md`.

pub mod convert;
pub mod decode;
pub mod dither;
pub mod error;
pub mod native;
pub mod overview;
pub mod probe;
pub mod resample;
pub mod tags;
pub mod write;

pub use convert::{convert, pcm_hash, CancelToken, ConvertSpec, Depth, Output, Rate, Target};
pub use decode::{decode_stereo, Decoded, StereoPcm, MAX_DECODE_MS};
pub use dither::Requantiser;
pub use error::{Error, Result};
pub use native::{NativeFormat, NativeStream};
pub use probe::{probe, Kind, Probe};
pub use resample::StreamResampler;
pub use tags::{embedded_artwork, tags, Picture, Tags};
pub use write::{AiffWriter, FlacWriter, PcmSpec, PcmWriter, WavWriter, WriteError};

/// Crate version, from Cargo metadata.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_set() {
        assert!(!super::VERSION.is_empty());
    }
}
