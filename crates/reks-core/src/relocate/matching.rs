//! How a missing track is recognised among the files of a folder. Pure:
//! the file system and the database stay in `plan`.
//!
//! Content is the only evidence that relinks without asking. A file name or
//! tag match proves nothing (two edits of one song share both), so it only
//! ever makes a *candidate* that the user confirms.

use std::path::Path;

use unicode_normalization::char::is_combining_mark;
use unicode_normalization::UnicodeNormalization;

/// Durations agree when they differ by at most this.
pub(super) const DURATION_TOLERANCE_MS: i64 = 1000;

/// Most candidates listed per track; more is noise for a picker.
pub(super) const MAX_CANDIDATES: usize = 8;

/// Lower case, accents and punctuation dropped, so `Café – Dub.mp3`,
/// `cafe dub.MP3` and `Cafe_Dub.mp3` agree.
pub(super) fn fold(s: &str) -> String {
    s.nfkd()
        .filter(|c| !is_combining_mark(*c))
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// [`fold`] of a file name without its extension and without a leading
/// track number (`01 - Title` → `title`), the usual rename done by tidy-up
/// tools. A name that is only digits is kept whole.
pub(super) fn name_key(path: &str) -> Option<String> {
    let stem = Path::new(path).file_stem()?.to_str()?;
    let after_digits = stem.trim_start_matches(|c: char| c.is_ascii_digit());
    let rest = after_digits.trim_start_matches(|c: char| " ._-)".contains(c));
    let numbered = after_digits.len() < stem.len() && rest.len() < after_digits.len();
    let key = fold(if numbered && !fold(rest).is_empty() {
        rest
    } else {
        stem
    });
    (!key.is_empty()).then_some(key)
}

/// `(artist, title)` folded; both must be present.
pub(super) fn tag_key(artist: Option<&str>, title: Option<&str>) -> Option<String> {
    let (a, t) = (fold(artist?), fold(title?));
    (!a.is_empty() && !t.is_empty()).then(|| format!("{a}\u{1}{t}"))
}

/// Unknown durations don't rule a candidate out; known ones must agree.
pub(super) fn durations_agree(a: Option<i64>, b: Option<i64>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => (a - b).abs() <= DURATION_TOLERANCE_MS,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_fold_case_accents_and_punctuation() {
        let a = name_key("/old/Café – Dub.mp3");
        assert_eq!(a, name_key("/new/cafe_dub.MP3"));
        assert_eq!(a.as_deref(), Some("cafedub"));
    }

    #[test]
    fn leading_track_numbers_are_ignored() {
        assert_eq!(name_key("/a/01 - Title.flac"), name_key("/b/Title.flac"));
        assert_eq!(name_key("/a/12. Title.flac"), name_key("/b/Title.flac"));
        assert_eq!(name_key("/a/3-Title.flac"), name_key("/b/title.flac"));
    }

    #[test]
    fn digit_only_names_are_kept() {
        assert_eq!(name_key("/a/808.wav").as_deref(), Some("808"));
        assert_ne!(name_key("/a/808.wav"), name_key("/a/909.wav"));
    }

    #[test]
    fn names_without_letters_have_no_key() {
        assert_eq!(name_key("/a/---.wav"), None);
    }

    #[test]
    fn tag_key_needs_both_fields() {
        assert_eq!(
            tag_key(Some("Artist"), Some("Title")),
            tag_key(Some(" artist "), Some("TITLE"))
        );
        assert_eq!(tag_key(Some("Artist"), None), None);
        assert_eq!(tag_key(Some("!!"), Some("Title")), None);
    }

    #[test]
    fn durations_within_a_second() {
        assert!(durations_agree(Some(245_900), Some(245_000)));
        assert!(!durations_agree(Some(246_100), Some(245_000)));
        assert!(durations_agree(None, Some(245_000)));
    }
}
