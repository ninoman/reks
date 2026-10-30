//! Reading what an audio file contributes to its track row (size, mtime,
//! content hash, probe, tags, artwork). Shared by the folder scan and the
//! rekordbox XML import so both fill rows the same way.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use unicode_normalization::UnicodeNormalization;

use crate::db::Library;
use crate::keys::Key;
use crate::timefmt;

/// Bytes hashed from each end of a file for `content_hash`.
const HASH_EDGE: u64 = 1 << 20;

/// What a file contributes to its track row.
pub(crate) struct FileInfo {
    pub size: i64,
    pub mtime: String,
    pub hash: String,
    pub probe: reks_audio::Probe,
    pub tags: reks_audio::Tags,
    pub artwork_id: Option<i64>,
}

/// Size and mtime (ISO-8601 UTC).
pub(crate) fn stat(path: &str) -> std::io::Result<(i64, String)> {
    let meta = std::fs::metadata(path)?;
    let size = i64::try_from(meta.len()).unwrap_or(i64::MAX);
    let mtime = meta
        .modified()
        .map(timefmt::iso8601_utc)
        .unwrap_or_default();
    Ok((size, mtime))
}

/// Hash, probe, tags and artwork for a file already `stat`ed. Takes the
/// library lock only briefly to store artwork. `Err` carries a message for
/// the caller's report: the file is unreadable or not playable audio.
pub(crate) fn read_file(
    lib: &Library,
    path: &str,
    size: i64,
    mtime: String,
) -> Result<FileInfo, String> {
    let hash = content_hash(Path::new(path)).map_err(|e| format!("cannot read file: {e}"))?;
    read_file_hashed(lib, path, size, mtime, hash)
}

/// [`read_file`] with the content hash already computed.
pub(crate) fn read_file_hashed(
    lib: &Library,
    path: &str,
    size: i64,
    mtime: String,
    hash: String,
) -> Result<FileInfo, String> {
    let p = Path::new(path);
    let mut probe = reks_audio::probe_headers(p).map_err(|e| e.to_string())?;
    // One lofty parse for tags, artwork and bit rate (P3-9 of the perf audit).
    let meta = reks_audio::metadata(p).unwrap_or_else(|e| {
        tracing::warn!(path, error = %e, "no tags read");
        reks_audio::Metadata::default()
    });
    probe.bit_rate_kbps = probe.bit_rate_kbps.or(meta.bit_rate_kbps);
    let artwork_id = meta.artwork.and_then(|pic| {
        lib.store_artwork(&pic.data).unwrap_or_else(|e| {
            tracing::warn!(path, error = %e, "artwork not stored");
            None
        })
    });
    let tags = meta.tags;
    Ok(FileInfo {
        size,
        mtime,
        hash,
        probe,
        tags,
        artwork_id,
    })
}

/// Probe fields as SQL values.
pub(crate) struct ProbeCols {
    pub kind: &'static str,
    pub duration_ms: i64,
    pub sample_rate: u32,
    pub bit_depth: Option<u8>,
    pub bit_rate_kbps: Option<u32>,
    pub channels: u8,
}

pub(crate) fn probe_cols(p: &reks_audio::Probe) -> ProbeCols {
    ProbeCols {
        kind: p.kind.as_str(),
        duration_ms: i64::try_from(p.duration_ms).unwrap_or(i64::MAX),
        sample_rate: p.sample_rate,
        bit_depth: p.bit_depth,
        bit_rate_kbps: p.bit_rate_kbps,
        channels: p.channels,
    }
}

/// Canonical key from the tag, if it parses.
pub(crate) fn key_cols(tags: &reks_audio::Tags) -> (Option<&'static str>, Option<&'static str>) {
    match tags.key.as_deref().map(Key::parse) {
        Some(Ok(k)) => (Some(k.canonical()), Some("tag")),
        Some(Err(e)) => {
            tracing::debug!(error = %e, "ignoring key tag");
            (None, None)
        }
        None => (None, None),
    }
}

pub(crate) fn bpm_cols(tags: &reks_audio::Tags) -> (Option<u32>, Option<&'static str>) {
    (tags.bpm_x100, tags.bpm_x100.map(|_| "tag"))
}

/// The library form of a path: UTF-8 and NFC, as long as the NFC spelling
/// still opens the same file (true on macOS; on other systems an NFD name is
/// kept as it is on disk).
pub(crate) fn library_path(path: &Path) -> Option<String> {
    let s = path.to_str()?;
    let nfc: String = s.nfc().collect();
    if nfc != s && Path::new(&nfc).exists() {
        Some(nfc)
    } else {
        Some(s.to_owned())
    }
}

/// `track.kind` guessed from a file extension, for rows whose file could not
/// be probed. `None` for extensions we don't handle.
pub(crate) fn kind_from_extension(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "mp3" => "mp3",
        "m4a" | "aac" | "mp4" => "aac",
        "flac" => "flac",
        "wav" | "wave" => "wav",
        "aif" | "aiff" => "aiff",
        "ogg" => "ogg",
        _ => return None,
    })
}

/// blake3 over the first and last 1 MiB plus the file size: cheap, and
/// stable across renames and tag-free copies. Hex-encoded.
pub(crate) fn content_hash(path: &Path) -> std::io::Result<String> {
    let mut f = File::open(path)?;
    let len = f.metadata()?.len();
    let mut hasher = blake3::Hasher::new();
    let mut buf = Vec::new();
    (&mut f).take(HASH_EDGE).read_to_end(&mut buf)?;
    hasher.update(&buf);
    if len > HASH_EDGE {
        buf.clear();
        f.seek(SeekFrom::Start(
            len.saturating_sub(HASH_EDGE).max(HASH_EDGE),
        ))?;
        f.take(HASH_EDGE).read_to_end(&mut buf)?;
        hasher.update(&buf);
    }
    hasher.update(&len.to_le_bytes());
    Ok(hasher.finalize().to_hex().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_from_extensions() {
        assert_eq!(kind_from_extension(Path::new("/a/b.MP3")), Some("mp3"));
        assert_eq!(kind_from_extension(Path::new("/a/b.m4a")), Some("aac"));
        assert_eq!(kind_from_extension(Path::new("/a/b.aif")), Some("aiff"));
        assert_eq!(kind_from_extension(Path::new("/a/b.txt")), None);
        assert_eq!(kind_from_extension(Path::new("/a/b")), None);
    }
}
