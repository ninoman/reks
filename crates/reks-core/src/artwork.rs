//! Artwork cache: embedded pictures de-duplicated by SHA-256 into
//! `<cache_dir>/artwork/<sha256>.<ext>`, one `artwork` row per distinct image.
//!
//! `artwork.blob_path` is relative to the cache dir so the library survives a
//! moved cache. The cache is disposable: a missing file is rewritten from a
//! track's audio file when it is read, or the next time the picture is stored.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use imagesize::ImageType;
use rusqlite::OptionalExtension;
use sha2::{Digest, Sha256};

use crate::db::Library;
use crate::error::{Error, Result};

/// Row id in the `artwork` table.
pub type ArtworkId = i64;

const ARTWORK_DIR: &str = "artwork";
const THUMB_DIR: &str = "thumb";
/// Twice the largest list thumbnail, for high-DPI screens.
const THUMB_PX: u32 = 96;

impl Library {
    /// Stores picture bytes (as embedded in an audio file) and returns the
    /// artwork id, reusing the existing row for identical bytes. Returns
    /// `None` when the bytes are not a recognised image format.
    pub fn store_artwork(&self, data: &[u8]) -> Result<Option<ArtworkId>> {
        let Some((mime, ext)) = image_format(data) else {
            tracing::debug!(len = data.len(), "skipping unrecognised artwork");
            return Ok(None);
        };
        let sha = sha256_hex(data);
        let rel = format!("{ARTWORK_DIR}/{sha}.{ext}");
        let abs = self.cache_dir().join(&rel);

        let conn = self.conn()?;
        let existing: Option<(ArtworkId, String)> = conn
            .query_row(
                "SELECT id, blob_path FROM artwork WHERE sha256 = ?1",
                [&sha],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((id, blob_path)) = existing {
            let path = self.cache_dir().join(blob_path);
            if !path.is_file() {
                write_atomic(&path, data)?;
            }
            return Ok(Some(id));
        }

        write_atomic(&abs, data)?;
        let (width, height) = match imagesize::blob_size(data) {
            Ok(s) => (i64::try_from(s.width).ok(), i64::try_from(s.height).ok()),
            Err(_) => (None, None),
        };
        conn.execute(
            "INSERT INTO artwork (sha256, mime, width, height, blob_path)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            (&sha, mime, width, height, &rel),
        )?;
        Ok(Some(conn.last_insert_rowid()))
    }

    /// Absolute path of a stored picture, or `None` for an unknown id.
    pub fn artwork_path(&self, id: ArtworkId) -> Result<Option<PathBuf>> {
        let rel: Option<String> = self
            .conn()?
            .query_row("SELECT blob_path FROM artwork WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .optional()?;
        Ok(rel.map(|r| self.cache_dir().join(r)))
    }

    /// The stored picture's bytes. `ArtworkNotFound` for an unknown id. A
    /// missing cache file is re-extracted from a track that uses the picture;
    /// an I/O error if none of them still embeds it.
    pub fn artwork(&self, id: ArtworkId) -> Result<Vec<u8>> {
        let path = self.artwork_path(id)?.ok_or(Error::ArtworkNotFound(id))?;
        match std::fs::read(&path) {
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                match self.reextract_artwork(id, &path)? {
                    Some(data) => Ok(data),
                    None => Err(Error::Io { path, source }),
                }
            }
            other => other.map_err(|source| Error::Io { path, source }),
        }
    }

    /// A small JPEG of the picture for lists, cached beside the original so
    /// the full-size image is decoded once, not per row or per launch.
    pub fn artwork_thumb(&self, id: ArtworkId) -> Result<Vec<u8>> {
        let original = self.artwork_path(id)?.ok_or(Error::ArtworkNotFound(id))?;
        let name = original.file_stem().unwrap_or_default();
        let thumb = self
            .cache_dir()
            .join(ARTWORK_DIR)
            .join(THUMB_DIR)
            .join(Path::new(name).with_extension("jpg"));
        if let Ok(data) = std::fs::read(&thumb) {
            return Ok(data);
        }
        let data = make_thumb(&self.artwork(id)?).ok_or(Error::InvalidField {
            field: "artwork",
            reason: "not a decodable image".into(),
        })?;
        write_atomic(&thumb, &data)?;
        Ok(data)
    }

    /// Reads the picture again from the audio files of the tracks that use
    /// it and rewrites the cache file. Only bytes matching the row's hash are
    /// taken: a file whose cover changed since the scan is not this picture.
    fn reextract_artwork(&self, id: ArtworkId, path: &Path) -> Result<Option<Vec<u8>>> {
        let (sha, sources) = {
            let conn = self.conn()?;
            let sha: String =
                conn.query_row("SELECT sha256 FROM artwork WHERE id = ?1", [id], |r| {
                    r.get(0)
                })?;
            let mut stmt =
                conn.prepare("SELECT path FROM track WHERE artwork_id = ?1 ORDER BY missing, id")?;
            let sources = stmt
                .query_map([id], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            (sha, sources)
        };
        // The lock is released: reading audio files can be slow.
        for source in sources {
            let data = match reks_audio::embedded_artwork(Path::new(&source)) {
                Ok(Some(pic)) => pic.data,
                Ok(None) => continue,
                Err(e) => {
                    tracing::debug!(path = source, error = %e, "artwork not re-read");
                    continue;
                }
            };
            if sha256_hex(&data) == sha {
                write_atomic(path, &data)?;
                tracing::info!(id, from = source, "missing artwork re-extracted");
                return Ok(Some(data));
            }
        }
        Ok(None)
    }
}

fn make_thumb(data: &[u8]) -> Option<Vec<u8>> {
    let img = image::load_from_memory(data).ok()?;
    let small = img.thumbnail(THUMB_PX, THUMB_PX).to_rgb8();
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 82)
        .encode_image(&small)
        .ok()?;
    Some(out)
}

/// MIME type and file extension, sniffed from the bytes (tag MIME fields
/// are often wrong).
fn image_format(data: &[u8]) -> Option<(&'static str, &'static str)> {
    match imagesize::image_type(data).ok()? {
        ImageType::Jpeg => Some(("image/jpeg", "jpg")),
        ImageType::Png => Some(("image/png", "png")),
        ImageType::Gif => Some(("image/gif", "gif")),
        ImageType::Bmp => Some(("image/bmp", "bmp")),
        ImageType::Webp => Some(("image/webp", "webp")),
        ImageType::Tiff => Some(("image/tiff", "tif")),
        _ => None,
    }
}

fn sha256_hex(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    let mut hex = String::with_capacity(digest.len() * 2);
    for b in digest.iter() {
        // Writing to a String cannot fail.
        let _ = write!(hex, "{b:02x}");
    }
    hex
}

/// Writes via a temp file + rename so a crash never leaves a truncated image.
pub(crate) fn write_atomic(path: &Path, data: &[u8]) -> Result<()> {
    let io = |source| Error::Io {
        path: path.to_path_buf(),
        source,
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(io)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, data).map_err(io)?;
    std::fs::rename(&tmp, path).map_err(io)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A valid 1×1 RGBA PNG.
    const PNG_1X1: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    /// JPEG header up to a 3×2 SOF0 frame (enough for type and size sniffing).
    const JPEG_3X2: &[u8] = &[
        0xFF, 0xD8, 0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x02, 0x00, 0x03, 0x01, 0x01, 0x11, 0x00,
        0xFF, 0xD9,
    ];

    fn lib() -> (tempfile::TempDir, Library) {
        let dir = tempfile::tempdir().unwrap();
        let lib =
            Library::open(&dir.path().join("reks.sqlite"), &dir.path().join("cache")).unwrap();
        (dir, lib)
    }

    fn artwork_rows(lib: &Library) -> i64 {
        lib.conn()
            .unwrap()
            .query_row("SELECT count(*) FROM artwork", [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn thumb_is_a_cached_jpeg() {
        let (_dir, lib) = lib();
        let id = lib.store_artwork(PNG_1X1).unwrap().unwrap();
        let thumb = lib.artwork_thumb(id).unwrap();
        assert_eq!(&thumb[..2], &[0xFF, 0xD8]);
        // Served from the cache the second time, even without the original.
        std::fs::remove_file(lib.artwork_path(id).unwrap().unwrap()).unwrap();
        assert_eq!(lib.artwork_thumb(id).unwrap(), thumb);
    }

    #[test]
    fn thumb_of_an_unknown_id_is_not_found() {
        let (_dir, lib) = lib();
        assert!(matches!(
            lib.artwork_thumb(99),
            Err(Error::ArtworkNotFound(99))
        ));
    }

    #[test]
    fn stores_png_with_metadata() {
        let (_dir, lib) = lib();
        let id = lib.store_artwork(PNG_1X1).unwrap().unwrap();
        let (sha, mime, w, h, rel): (String, String, i64, i64, String) = lib
            .conn()
            .unwrap()
            .query_row(
                "SELECT sha256, mime, width, height, blob_path FROM artwork WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .unwrap();
        assert_eq!(sha.len(), 64);
        assert_eq!(mime, "image/png");
        assert_eq!((w, h), (1, 1));
        assert_eq!(rel, format!("artwork/{sha}.png"));
        let path = lib.artwork_path(id).unwrap().unwrap();
        assert_eq!(path, lib.cache_dir().join(&rel));
        assert_eq!(std::fs::read(path).unwrap(), PNG_1X1);
    }

    #[test]
    fn artwork_bytes_round_trip() {
        let (_dir, lib) = lib();
        let id = lib.store_artwork(JPEG_3X2).unwrap().unwrap();
        assert_eq!(lib.artwork(id).unwrap(), JPEG_3X2);
        assert!(matches!(
            lib.artwork(id + 1),
            Err(Error::ArtworkNotFound(_))
        ));
        std::fs::remove_file(lib.artwork_path(id).unwrap().unwrap()).unwrap();
        assert!(matches!(lib.artwork(id), Err(Error::Io { .. })));
    }

    #[test]
    fn identical_bytes_are_deduplicated() {
        let (_dir, lib) = lib();
        let a = lib.store_artwork(PNG_1X1).unwrap();
        let b = lib.store_artwork(PNG_1X1).unwrap();
        assert_eq!(a, b);
        let c = lib.store_artwork(JPEG_3X2).unwrap();
        assert_ne!(a, c);
        assert_eq!(artwork_rows(&lib), 2);
        let files = std::fs::read_dir(lib.cache_dir().join(ARTWORK_DIR))
            .unwrap()
            .count();
        assert_eq!(files, 2);
    }

    #[test]
    fn jpeg_dimensions_and_extension() {
        let (_dir, lib) = lib();
        let id = lib.store_artwork(JPEG_3X2).unwrap().unwrap();
        let path = lib.artwork_path(id).unwrap().unwrap();
        assert_eq!(path.extension().unwrap(), "jpg");
        let (w, h): (i64, i64) = lib
            .conn()
            .unwrap()
            .query_row(
                "SELECT width, height FROM artwork WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((w, h), (3, 2));
    }

    #[test]
    fn missing_cache_file_is_rewritten() {
        let (_dir, lib) = lib();
        let id = lib.store_artwork(PNG_1X1).unwrap().unwrap();
        let path = lib.artwork_path(id).unwrap().unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(lib.store_artwork(PNG_1X1).unwrap(), Some(id));
        assert_eq!(std::fs::read(path).unwrap(), PNG_1X1);
    }

    #[test]
    fn unrecognised_bytes_are_skipped() {
        let (_dir, lib) = lib();
        assert_eq!(lib.store_artwork(b"not an image").unwrap(), None);
        assert_eq!(lib.store_artwork(&[]).unwrap(), None);
        assert_eq!(artwork_rows(&lib), 0);
        assert_eq!(lib.artwork_path(42).unwrap(), None);
    }
}
