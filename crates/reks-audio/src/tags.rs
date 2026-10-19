//! Tag and embedded-artwork reading via `lofty` (decision D6). Read-only:
//! reks never writes tags to audio files in Phase 1.

use std::path::Path;

use lofty::file::{TaggedFile, TaggedFileExt};
use lofty::picture::{Picture as LoftyPicture, PictureType};
use lofty::tag::{Accessor, ItemKey, Tag};

use crate::error::{Error, Result};

/// Metadata read from a file's tags. Empty strings are reported as `None`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tags {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub genre: Option<String>,
    pub composer: Option<String>,
    pub remixer: Option<String>,
    pub label: Option<String>,
    pub grouping: Option<String>,
    pub comment: Option<String>,
    pub track_no: Option<u32>,
    pub disc_no: Option<u32>,
    pub year: Option<u16>,
    /// Tagged BPM × 100, rounded.
    pub bpm_x100: Option<u32>,
    /// Raw initial-key tag (`TKEY` etc.), unparsed; `reks-core::keys` normalises it.
    pub key: Option<String>,
}

/// An embedded picture, bytes as stored in the file.
#[derive(Clone, PartialEq, Eq)]
pub struct Picture {
    /// MIME type declared by the tag, if any. Callers should sniff `data`.
    pub mime: Option<String>,
    pub data: Vec<u8>,
}

impl std::fmt::Debug for Picture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Picture")
            .field("mime", &self.mime)
            .field("len", &self.data.len())
            .finish()
    }
}

/// Reads the tags of `path`. Fields missing from the primary tag fall back
/// to the file's other tags (e.g. ID3v1, APE).
pub fn tags(path: &Path) -> Result<Tags> {
    let file = read(path)?;
    let tags = ordered_tags(&file);
    let text = |f: &dyn Fn(&Tag) -> Option<String>| tags.iter().find_map(|t| f(t).and_then(clean));
    let item = |key: ItemKey| text(&|t: &Tag| t.get_string(key).map(str::to_owned));

    Ok(Tags {
        title: text(&|t| t.title().map(|s| s.into_owned())),
        artist: text(&|t| t.artist().map(|s| s.into_owned())),
        album: text(&|t| t.album().map(|s| s.into_owned())),
        genre: text(&|t| t.genre().map(|s| s.into_owned())),
        comment: text(&|t| t.comment().map(|s| s.into_owned())),
        album_artist: item(ItemKey::AlbumArtist),
        composer: item(ItemKey::Composer),
        remixer: item(ItemKey::Remixer),
        label: item(ItemKey::Label).or_else(|| item(ItemKey::Publisher)),
        grouping: item(ItemKey::ContentGroup).or_else(|| item(ItemKey::AppleId3v2ContentGroup)),
        key: item(ItemKey::InitialKey),
        track_no: tags.iter().find_map(|t| t.track()).filter(|&n| n > 0),
        disc_no: tags.iter().find_map(|t| t.disk()).filter(|&n| n > 0),
        year: tags
            .iter()
            .find_map(|t| t.date().map(|d| d.year))
            .or_else(|| item(ItemKey::Year).and_then(|y| leading_year(&y)))
            .filter(|&y| y > 0),
        bpm_x100: item(ItemKey::Bpm)
            .or_else(|| item(ItemKey::IntegerBpm))
            .and_then(|b| parse_bpm_x100(&b)),
    })
}

/// The file's cover art: the front-cover picture if tagged as one, else the
/// first picture. `None` when the file has no pictures.
pub fn embedded_artwork(path: &Path) -> Result<Option<Picture>> {
    let file = read(path)?;
    let tags = ordered_tags(&file);
    let pictures = || tags.iter().flat_map(|t| t.pictures());
    let chosen = pictures()
        .find(|p| p.pic_type() == PictureType::CoverFront)
        .or_else(|| pictures().next());
    Ok(chosen.filter(|p| !p.data().is_empty()).map(to_picture))
}

fn read(path: &Path) -> Result<TaggedFile> {
    lofty::read_from_path(path).map_err(|source| Error::Tags {
        path: path.to_path_buf(),
        source,
    })
}

/// Primary tag first, then the rest in file order.
fn ordered_tags(file: &TaggedFile) -> Vec<&Tag> {
    let primary = file.primary_tag();
    primary
        .into_iter()
        .chain(
            file.tags()
                .iter()
                .filter(|t| primary.is_none_or(|p| !std::ptr::eq(*t, p))),
        )
        .collect()
}

fn to_picture(p: &LoftyPicture) -> Picture {
    Picture {
        mime: p.mime_type().map(|m| m.as_str().to_owned()),
        data: p.data().to_vec(),
    }
}

fn clean(s: String) -> Option<String> {
    let t = s.trim_matches(|c: char| c.is_whitespace() || c == '\0');
    (!t.is_empty()).then(|| t.to_owned())
}

/// `"2019"`, `"2019-04-12"` → 2019.
fn leading_year(s: &str) -> Option<u16> {
    let digits: String = s.chars().take_while(char::is_ascii_digit).collect();
    if digits.len() == 4 {
        digits.parse().ok()
    } else {
        None
    }
}

/// `"128"`, `"127.95"`, `"128,00"` → BPM × 100. Rejects values outside 1–999.
fn parse_bpm_x100(s: &str) -> Option<u32> {
    let v: f64 = s.trim().replace(',', ".").parse().ok()?;
    if !(1.0..1000.0).contains(&v) {
        return None;
    }
    // In range by the check above, so the cast cannot truncate.
    Some((v * 100.0).round() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bpm_parsing() {
        assert_eq!(parse_bpm_x100("128"), Some(12800));
        assert_eq!(parse_bpm_x100(" 127.95 "), Some(12795));
        assert_eq!(parse_bpm_x100("128,5"), Some(12850));
        assert_eq!(parse_bpm_x100("0"), None);
        assert_eq!(parse_bpm_x100("abc"), None);
        assert_eq!(parse_bpm_x100("12800"), None);
    }

    #[test]
    fn year_parsing() {
        assert_eq!(leading_year("2019"), Some(2019));
        assert_eq!(leading_year("2019-04-12"), Some(2019));
        assert_eq!(leading_year("19"), None);
        assert_eq!(leading_year(""), None);
    }

    #[test]
    fn clean_trims_and_drops_empty() {
        assert_eq!(clean("  a \0".into()), Some("a".into()));
        assert_eq!(clean(" \0 ".into()), None);
    }
}
