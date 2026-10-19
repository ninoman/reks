//! Header-only audio probe: format, duration, sample rate, channels, bit depth
//! and bit rate, without decoding the stream.

use std::fs::File;
use std::path::Path;

use symphonia::core::codecs::audio::well_known::{
    CODEC_ID_AAC, CODEC_ID_ALAC, CODEC_ID_FLAC, CODEC_ID_MP3,
};
use symphonia::core::codecs::audio::AudioCodecParameters;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, Track, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

use crate::error::{Error, Result};

/// Audio file kind, as stored in the library's `track.kind` column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Mp3,
    Aac,
    Alac,
    Flac,
    Wav,
    Aiff,
}

impl Kind {
    /// The `track.kind` string.
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Mp3 => "mp3",
            Kind::Aac => "aac",
            Kind::Alac => "alac",
            Kind::Flac => "flac",
            Kind::Wav => "wav",
            Kind::Aiff => "aiff",
        }
    }

    /// Lossless kinds report a bit depth; lossy ones do not.
    pub fn is_lossless(self) -> bool {
        !matches!(self, Kind::Mp3 | Kind::Aac)
    }
}

/// What [`probe`] learns from a file's headers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probe {
    pub kind: Kind,
    /// Playable length (encoder delay/padding excluded when the file says so).
    pub duration_ms: u64,
    pub sample_rate: u32,
    pub channels: u8,
    /// Lossless kinds only.
    pub bit_depth: Option<u8>,
    /// Audio-stream bit rate in kbit/s (average for VBR), not counting tags or artwork.
    pub bit_rate_kbps: Option<u32>,
}

/// Reads the headers of `path` and describes its first audio track.
pub fn probe(path: &Path) -> Result<Probe> {
    let file = File::open(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let format = symphonia::default::get_probe()
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
    let params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .ok_or_else(|| unsupported("no audio codec parameters"))?;

    let kind = kind_of(params, format.format_info().short_name)
        .ok_or_else(|| unsupported(&format!("codec {}", params.codec)))?;
    let sample_rate = params
        .sample_rate
        .filter(|&sr| sr > 0)
        .ok_or_else(|| unsupported("unknown sample rate"))?;
    let channels = params
        .channels
        .as_ref()
        .and_then(|c| u8::try_from(c.count()).ok())
        .filter(|&c| c > 0)
        .ok_or_else(|| unsupported("unknown channel count"))?;
    let duration_ms =
        duration_ms(track, sample_rate).ok_or_else(|| unsupported("unknown duration"))?;
    let bit_depth = if kind.is_lossless() {
        params.bits_per_sample.and_then(|b| u8::try_from(b).ok())
    } else {
        None
    };
    let bit_rate_kbps = match (kind, bit_depth) {
        // Uncompressed PCM: exact.
        (Kind::Wav | Kind::Aiff, Some(bits)) => Some(
            u32::try_from(u64::from(sample_rate) * u64::from(bits) * u64::from(channels) / 1000)
                .unwrap_or(u32::MAX),
        ),
        _ => stream_bit_rate(path),
    };

    Ok(Probe {
        kind,
        duration_ms,
        sample_rate,
        channels,
        bit_depth,
        bit_rate_kbps,
    })
}

fn kind_of(params: &AudioCodecParameters, container: &str) -> Option<Kind> {
    match params.codec {
        CODEC_ID_MP3 => Some(Kind::Mp3),
        CODEC_ID_AAC => Some(Kind::Aac),
        CODEC_ID_ALAC => Some(Kind::Alac),
        CODEC_ID_FLAC => Some(Kind::Flac),
        // Everything else Symphonia decodes here is PCM inside WAV or AIFF.
        _ => match container {
            "wave" => Some(Kind::Wav),
            "aiff" => Some(Kind::Aiff),
            _ => None,
        },
    }
}

fn duration_ms(track: &Track, sample_rate: u32) -> Option<u64> {
    if let Some(frames) = track.num_frames {
        return Some(frames.saturating_mul(1000) / u64::from(sample_rate));
    }
    let time = track.time_base?.calc_duration(track.duration?)?;
    u64::try_from(time.as_millis()).ok()
}

/// Compressed-stream bit rate from lofty's header parsing (Xing/VBRI, MP4
/// `esds`/`mdhd`, FLAC STREAMINFO). Optional: a missing value is not an error.
fn stream_bit_rate(path: &Path) -> Option<u32> {
    use lofty::config::ParseOptions;
    use lofty::file::AudioFile;

    let options = ParseOptions::new().read_tags(false).read_cover_art(false);
    let file = lofty::probe::Probe::open(path)
        .ok()?
        .options(options)
        .read()
        .map_err(|e| tracing::debug!(path = %path.display(), error = %e, "no bit rate"))
        .ok()?;
    file.properties().audio_bitrate().filter(|&b| b > 0)
}

fn decode_err(path: &Path, source: symphonia::core::errors::Error) -> Error {
    use symphonia::core::errors::Error as SymErr;
    match source {
        SymErr::Unsupported(what) => Error::Unsupported {
            path: path.to_path_buf(),
            reason: what.to_owned(),
        },
        source => Error::Decode {
            path: path.to_path_buf(),
            source,
        },
    }
}
