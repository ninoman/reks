//! Header-only audio probe: format, duration, sample rate, channels, bit depth
//! and bit rate, without decoding the stream.

use std::fs::File;
use std::path::Path;

use serde::{Deserialize, Serialize};

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
    let mut p = probe_headers(path)?;
    if p.bit_rate_kbps.is_none() {
        p.bit_rate_kbps = stream_bit_rate(path);
    }
    Ok(p)
}

/// [`probe`] without the separate lofty pass for a compressed stream's bit
/// rate: `bit_rate_kbps` is set for PCM only. For callers that take it from
/// [`crate::metadata`] instead.
pub fn probe_headers(path: &Path) -> Result<Probe> {
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
        bits_per_sample(params)
    } else {
        None
    };
    let bit_rate_kbps = match (kind, bit_depth) {
        // Uncompressed PCM: exact.
        (Kind::Wav | Kind::Aiff, Some(bits)) => Some(
            u32::try_from(u64::from(sample_rate) * u64::from(bits) * u64::from(channels) / 1000)
                .unwrap_or(u32::MAX),
        ),
        _ => None,
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

/// Significant bits per sample from the codec parameters, or for ALAC (whose
/// parameters Symphonia leaves without it) from the magic cookie.
pub(crate) fn bits_per_sample(params: &AudioCodecParameters) -> Option<u8> {
    match params.bits_per_sample {
        Some(b) => u8::try_from(b).ok(),
        None if params.codec == CODEC_ID_ALAC => {
            params.extra_data.as_deref().and_then(alac_bit_depth)
        }
        None => None,
    }
}

/// `bitDepth` of an ALAC magic cookie (`ALACSpecificConfig`: frame length
/// u32, compatible version u8, bit depth u8, …), skipping the optional
/// `frma` and `alac` atom headers as Symphonia's ALAC decoder does.
fn alac_bit_depth(mut cookie: &[u8]) -> Option<u8> {
    for atom in [b"frma", b"alac"] {
        if cookie.get(4..8) == Some(&atom[..]) {
            cookie = cookie.get(12..)?;
        }
    }
    if cookie.len() != 24 && cookie.len() != 48 {
        return None;
    }
    cookie.get(5).copied().filter(|&b| b > 0)
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

pub(crate) fn decode_err(path: &Path, source: symphonia::core::errors::Error) -> Error {
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

#[cfg(test)]
mod alac_tests {
    use super::alac_bit_depth;

    #[test]
    fn reads_the_cookie_bit_depth() {
        let mut cookie = vec![0u8; 24];
        cookie[5] = 24;
        assert_eq!(alac_bit_depth(&cookie), Some(24));
        let mut atom = vec![0, 0, 0, 36];
        atom.extend_from_slice(b"alac");
        atom.extend_from_slice(&[0; 4]);
        atom.extend_from_slice(&cookie);
        assert_eq!(alac_bit_depth(&atom), Some(24));
        assert_eq!(alac_bit_depth(&cookie[..20]), None);
    }
}
