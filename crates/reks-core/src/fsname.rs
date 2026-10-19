//! FAT32-safe names for files written to removable media.
//!
//! Shared by the rekordbox legacy export (Phase 2) and the OneLibrary and
//! Engine DJ exporters (Phases 4/5). Every path these functions produce is:
//!
//! - **ASCII-only.** Non-ASCII text is transliterated (`deunicode`: Cyrillic
//!   `Кино` → `Kino`, kana → romaji, Han → Mandarin pinyin, emoji → words) after
//!   NFC normalisation. The displayed metadata keeps the original Unicode in
//!   the database; only the path is folded. This sidesteps NFC/NFD handling on
//!   players, which no Pioneer source documents (R§4, D24).
//! - **FAT32-legal.** No `\ / : * ? " < > |`, no `;` (R§4), no control
//!   characters, no leading or trailing dots/spaces, no reserved Windows device
//!   names (`CON`, `PRN`, `AUX`, `NUL`, `COM1`–`COM9`, `LPT1`–`LPT9`).
//! - **Within the player limits** of 255 characters for the full path and 8
//!   folder levels (R§4). [`contents_path`] produces
//!   `/Contents/<Artist>/<Album>/<file>` (3 levels) and leaves room for a
//!   [`dedupe`] suffix.
//!
//! All functions are pure and deterministic.

use std::collections::HashSet;

use unicode_normalization::{is_nfc, UnicodeNormalization};

/// Longest full path (`/Contents/...`) a player is known to accept, in characters.
pub const MAX_PATH_LEN: usize = 255;
/// Most folder levels below the stick root a player browses (XDJ-RR manual, R§4).
pub const MAX_DEPTH: usize = 8;
/// Longest single FAT32 long-file-name component, in UTF-16 units.
pub const MAX_COMPONENT_LEN: usize = 255;
/// Root folder for exported audio.
pub const CONTENTS_DIR: &str = "/Contents";
/// Longest artist folder name [`contents_path`] writes.
pub const ARTIST_MAX: usize = 64;
/// Longest album folder name [`contents_path`] writes.
pub const ALBUM_MAX: usize = 64;
/// Characters [`contents_path`] keeps free for a [`dedupe`] suffix (`-99999`).
pub const SUFFIX_RESERVE: usize = 6;
/// Longest extension [`contents_path`] keeps; longer ones stay part of the stem.
const EXT_MAX: usize = 8;

/// Folder used when the artist tag is empty or sanitises to nothing.
pub const UNKNOWN_ARTIST: &str = "Unknown Artist";
/// Folder used when the album tag is empty or sanitises to nothing.
pub const UNKNOWN_ALBUM: &str = "Unknown Album";
/// File stem used when the file name sanitises to nothing.
pub const UNKNOWN_TRACK: &str = "Unknown Track";

/// Characters FAT32 forbids, plus `;` (avoided on players, R§4).
const FORBIDDEN: &[char] = &['\\', '/', ':', '*', '?', '"', '<', '>', '|', ';'];

/// Reserved Windows device names, compared case-insensitively against the
/// part of a component before its first dot.
const RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Why a path is not safe to write to a stick. Returned by [`check_path`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PathProblem {
    #[error("path does not start with '/'")]
    NotAbsolute,
    #[error("path is {len} characters, limit is {MAX_PATH_LEN}")]
    TooLong { len: usize },
    #[error("path is {depth} folders deep, limit is {MAX_DEPTH}")]
    TooDeep { depth: usize },
    #[error("path has an empty component")]
    EmptyComponent,
    #[error("component {component:?} is longer than {MAX_COMPONENT_LEN} characters")]
    ComponentTooLong { component: String },
    #[error("component {component:?} contains {ch:?}")]
    IllegalChar { component: String, ch: char },
    #[error("component {component:?} ends with a dot or space")]
    TrailingDotOrSpace { component: String },
    #[error("component {component:?} is a reserved device name")]
    Reserved { component: String },
    #[error("path is not NFC-normalised")]
    NotNfc,
}

/// Turns arbitrary text into one FAT32-legal, ASCII-only path component of at
/// most `max_len` characters (`max_len` is clamped to 1..=255).
///
/// NFC-normalises, transliterates non-ASCII, replaces forbidden characters
/// with `_`, turns control characters into spaces, collapses runs of spaces,
/// trims leading and trailing dots/spaces, and appends `_` to reserved device
/// names (`CON` → `CON_`). Returns `"_"` when nothing usable remains.
/// Idempotent: sanitising the output again returns it unchanged.
pub fn sanitize_component(s: &str, max_len: usize) -> String {
    sanitize_opt(s, max_len).unwrap_or_else(|| "_".to_owned())
}

/// Builds `/Contents/<Artist>/<Album>/<file>` from tag values and a file name
/// (with extension). Each part goes through [`sanitize_component`]; empty
/// parts become [`UNKNOWN_ARTIST`], [`UNKNOWN_ALBUM`] or [`UNKNOWN_TRACK`].
/// The extension (up to 8 ASCII alphanumerics) is kept as is. The result is
/// at most [`MAX_PATH_LEN`]` - `[`SUFFIX_RESERVE`] characters, so [`dedupe`]
/// can add a suffix without truncating.
pub fn contents_path(artist: &str, album: &str, file: &str) -> String {
    let artist = sanitize_opt(artist, ARTIST_MAX).unwrap_or_else(|| UNKNOWN_ARTIST.to_owned());
    let album = sanitize_opt(album, ALBUM_MAX).unwrap_or_else(|| UNKNOWN_ALBUM.to_owned());

    let (stem, ext) = split_extension(file);
    let ext = ext.filter(|e| {
        !e.is_empty() && e.len() <= EXT_MAX && e.chars().all(|c| c.is_ascii_alphanumeric())
    });
    let stem = match ext {
        Some(_) => stem,
        None => file,
    };
    let ext_len = ext.map_or(0, |e| e.len() + 1);
    // "/Contents" + "/" + artist + "/" + album + "/" + stem + ext, plus the dedupe reserve.
    let used = CONTENTS_DIR.len() + 3 + artist.len() + album.len() + ext_len + SUFFIX_RESERVE;
    let stem_max = MAX_PATH_LEN
        .saturating_sub(used)
        .min(MAX_COMPONENT_LEN - ext_len);
    let stem = sanitize_opt(stem, stem_max).unwrap_or_else(|| truncate(UNKNOWN_TRACK, stem_max));

    let mut out = format!("{CONTENTS_DIR}/{artist}/{album}/{stem}");
    if let Some(ext) = ext {
        out.push('.');
        out.push_str(ext);
    }
    out
}

/// Case-folded key for `existing` in [`dedupe`]. Use it to seed the set with
/// paths that already exist on the stick.
pub fn fold_key(path: &str) -> String {
    path.nfc().collect::<String>().to_lowercase()
}

/// Returns `path`, or `path` with `-1`, `-2`, … inserted before the extension,
/// whichever is first free in `existing` (compared case-insensitively, as
/// FAT32 does), and records it. `existing` holds [`fold_key`] values.
///
/// If a suffix would push the path over [`MAX_PATH_LEN`], the stem is
/// shortened instead. Folder names are not deduplicated: two folders that
/// differ only in case are the same folder on FAT32, which is harmless.
pub fn dedupe(existing: &mut HashSet<String>, path: String) -> String {
    if existing.insert(fold_key(&path)) {
        return path;
    }
    let (dir, name) = match path.rfind('/') {
        Some(i) => path.split_at(i + 1),
        None => ("", path.as_str()),
    };
    let (stem, ext) = match split_extension(name) {
        (stem, Some(ext)) if !stem.is_empty() => (stem, Some(ext)),
        _ => (name, None),
    };
    let ext_len = ext.map_or(0, |e| e.chars().count() + 1);
    let dir_len = dir.chars().count();
    let mut n: u64 = 1;
    loop {
        let suffix = format!("-{n}");
        let room = MAX_PATH_LEN.saturating_sub(dir_len + ext_len + suffix.len());
        let stem = truncate(stem, room);
        let stem = stem.trim_end_matches(['.', ' ']);
        let mut candidate = format!("{dir}{stem}{suffix}");
        if let Some(ext) = ext {
            candidate.push('.');
            candidate.push_str(ext);
        }
        if existing.insert(fold_key(&candidate)) {
            return candidate;
        }
        n += 1;
    }
}

/// Checks that `path` (absolute, `/`-separated) is safe to write to a FAT32
/// stick and to store in `export.pdb` and PPTH: NFC, total length, folder
/// depth and every component. Does not require ASCII, so it also accepts
/// paths rekordbox wrote.
pub fn check_path(path: &str) -> Result<(), PathProblem> {
    if !is_nfc(path) {
        return Err(PathProblem::NotNfc);
    }
    let rest = path.strip_prefix('/').ok_or(PathProblem::NotAbsolute)?;
    let len = path.chars().count();
    if len > MAX_PATH_LEN {
        return Err(PathProblem::TooLong { len });
    }
    let components: Vec<&str> = rest.split('/').collect();
    let depth = components.len().saturating_sub(1);
    if depth > MAX_DEPTH {
        return Err(PathProblem::TooDeep { depth });
    }
    components.into_iter().try_for_each(check_component)
}

/// Checks one path component; see [`check_path`].
pub fn check_component(component: &str) -> Result<(), PathProblem> {
    let owned = || component.to_owned();
    if component.is_empty() {
        return Err(PathProblem::EmptyComponent);
    }
    if component.encode_utf16().count() > MAX_COMPONENT_LEN {
        return Err(PathProblem::ComponentTooLong { component: owned() });
    }
    if let Some(ch) = component
        .chars()
        .find(|&c| FORBIDDEN.contains(&c) || c.is_control())
    {
        return Err(PathProblem::IllegalChar {
            component: owned(),
            ch,
        });
    }
    if component.ends_with(['.', ' ']) {
        return Err(PathProblem::TrailingDotOrSpace { component: owned() });
    }
    if is_reserved(component) {
        return Err(PathProblem::Reserved { component: owned() });
    }
    Ok(())
}

/// [`sanitize_component`] without the `"_"` fallback.
fn sanitize_opt(s: &str, max_len: usize) -> Option<String> {
    let max_len = max_len.clamp(1, MAX_COMPONENT_LEN);
    let nfc: String = s.nfc().collect();
    let ascii = deunicode::deunicode_with_tofu(&nfc, "_");

    let mut out = String::with_capacity(ascii.len());
    for c in ascii.chars() {
        let c = if FORBIDDEN.contains(&c) || !c.is_ascii() {
            '_'
        } else if c.is_ascii_control() {
            ' '
        } else {
            c
        };
        if c == ' ' && out.ends_with(' ') {
            continue;
        }
        out.push(c);
    }

    let trimmed = out.trim_matches(['.', ' ']);
    let mut out = truncate(trimmed, max_len);
    out.truncate(out.trim_end_matches(['.', ' ']).len());
    if out.is_empty() {
        return None;
    }
    if is_reserved(&out) {
        // Mark the device-name stem with '_' (it ends in a letter or digit,
        // never a dot or space); replace its last character if there is no room.
        let at = stem_end(&out);
        if out.len() < max_len {
            out.insert(at, '_');
        } else {
            out.replace_range(at - 1..at, "_");
        }
    }
    Some(out)
}

/// Whether the part of `component` before its first dot, with trailing spaces
/// dropped, is a reserved device name.
fn is_reserved(component: &str) -> bool {
    let stem = component[..stem_end(component)].trim_end_matches(' ');
    RESERVED.iter().any(|r| r.eq_ignore_ascii_case(stem))
}

/// Byte index of the end of a reserved-name stem: the first dot, or the end.
/// Trailing spaces before that dot are excluded.
fn stem_end(component: &str) -> usize {
    let end = component.find('.').unwrap_or(component.len());
    component[..end].trim_end_matches(' ').len()
}

/// Splits `name` at its last dot into (stem, extension).
fn split_extension(name: &str) -> (&str, Option<&str>) {
    match name.rsplit_once('.') {
        Some((stem, ext)) => (stem, Some(ext)),
        None => (name, None),
    }
}

/// First `max` characters of `s`.
fn truncate(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn legal_ascii_component(c: &str) -> bool {
        check_component(c).is_ok() && c.is_ascii() && !c.starts_with(['.', ' '])
    }

    #[test]
    fn plain_ascii_unchanged() {
        assert_eq!(sanitize_component("Daft Punk", 64), "Daft Punk");
        assert_eq!(
            contents_path("Daft Punk", "Discovery", "One More Time.mp3"),
            "/Contents/Daft Punk/Discovery/One More Time.mp3"
        );
    }

    #[test]
    fn forbidden_chars_replaced() {
        assert_eq!(sanitize_component("AC/DC", 64), "AC_DC");
        assert_eq!(
            sanitize_component(r#"a\b:c*d?e"f<g>h|i;j"#, 64),
            "a_b_c_d_e_f_g_h_i_j"
        );
    }

    #[test]
    fn control_chars_become_single_spaces() {
        assert_eq!(sanitize_component("a\tb\n\u{7}c\u{7f}", 64), "a b c");
        assert_eq!(sanitize_component("a   b", 64), "a b");
    }

    #[test]
    fn cyrillic_transliterated() {
        assert_eq!(sanitize_component("Кино", 64), "Kino");
        assert_eq!(sanitize_component("Земфира", 64), "Zemfira");
    }

    #[test]
    fn japanese_transliterated() {
        // Kana become romaji; Han characters become Mandarin readings.
        assert_eq!(
            sanitize_component("ひらがな カタカナ", 64),
            "hiragana katakana"
        );
        assert_eq!(sanitize_component("坂本龍一", 64), "Ban Ben Long Yi");
    }

    #[test]
    fn emoji_transliterated_or_replaced() {
        for s in ["🔥", "Fire 🔥 Track", "🏳️‍🌈", "👩‍👩‍👧"] {
            let out = sanitize_component(s, 64);
            assert!(legal_ascii_component(&out), "{s:?} -> {out:?}");
        }
        assert_eq!(sanitize_component("🔥", 64), "fire");
        assert_eq!(sanitize_component("Fire 🔥 Track", 64), "Fire fire Track");
        assert_eq!(sanitize_component("👩‍👩‍👧", 64), "woman woman girl");
    }

    #[test]
    fn nfd_and_nfc_give_the_same_name() {
        let nfc = sanitize_component("Beyonc\u{e9}", 64);
        let nfd = sanitize_component("Beyonce\u{301}", 64);
        assert_eq!(nfc, "Beyonce");
        assert_eq!(nfc, nfd);
        assert_eq!(sanitize_component("Sigur Rós", 64), "Sigur Ros");
        assert_eq!(sanitize_component("Mø ß", 64), "Mo ss");
    }

    #[test]
    fn dots_and_spaces_trimmed() {
        assert_eq!(sanitize_component("Mr. Oizo...", 64), "Mr. Oizo");
        assert_eq!(sanitize_component("  name . . ", 64), "name");
        assert_eq!(sanitize_component("...hidden", 64), "hidden");
        assert_eq!(sanitize_component("...", 64), "_");
        assert_eq!(sanitize_component("", 64), "_");
    }

    #[test]
    fn reserved_names_marked() {
        assert_eq!(sanitize_component("CON", 64), "CON_");
        assert_eq!(sanitize_component("con", 64), "con_");
        assert_eq!(sanitize_component("Com1", 64), "Com1_");
        assert_eq!(sanitize_component("LPT9.mp3", 64), "LPT9_.mp3");
        assert_eq!(sanitize_component("AUX .txt", 64), "AUX_ .txt");
        assert_eq!(sanitize_component("nul.", 64), "nul_");
        assert_eq!(sanitize_component("COM\u{b9}", 64), "COM1_"); // superscript one
        assert_eq!(sanitize_component("CON", 3), "CO_");
        assert_eq!(sanitize_component("CONCERT", 3), "CO_");
        assert_eq!(sanitize_component("COM10", 64), "COM10");
        assert_eq!(sanitize_component("Console", 64), "Console");
        for out in ["CON_", "LPT9_.mp3", "AUX_ .txt", "CO_"] {
            assert!(check_component(out).is_ok(), "{out}");
        }
    }

    #[test]
    fn truncates_and_retrims() {
        assert_eq!(sanitize_component("abcdef", 3), "abc");
        assert_eq!(sanitize_component("ab. cdef", 4), "ab");
        assert_eq!(sanitize_component("x", 0), "x"); // max_len clamps to 1
    }

    #[test]
    fn empty_tags_become_unknown() {
        assert_eq!(
            contents_path("", " ", "track.mp3"),
            "/Contents/Unknown Artist/Unknown Album/track.mp3"
        );
        assert_eq!(
            contents_path("...", "?", ".flac"),
            "/Contents/Unknown Artist/_/Unknown Track.flac"
        );
    }

    #[test]
    fn extension_kept_or_folded_into_stem() {
        assert_eq!(contents_path("A", "B", "x.MP3"), "/Contents/A/B/x.MP3");
        assert_eq!(contents_path("A", "B", "noext"), "/Contents/A/B/noext");
        assert_eq!(contents_path("A", "B", "x.y z"), "/Contents/A/B/x.y z");
        assert_eq!(contents_path("A", "B", "CON.wav"), "/Contents/A/B/CON_.wav");
    }

    #[test]
    fn long_names_fit_with_room_for_suffix() {
        let long = "x".repeat(400);
        let p = contents_path(&long, &long, &format!("{long}.aiff"));
        assert_eq!(p.len(), MAX_PATH_LEN - SUFFIX_RESERVE);
        assert!(p.ends_with(".aiff"));
        assert!(check_path(&p).is_ok());
    }

    #[test]
    fn dedupe_is_case_insensitive_and_suffixes_before_extension() {
        let mut seen = HashSet::new();
        assert_eq!(
            dedupe(&mut seen, "/Contents/A/B/t.mp3".into()),
            "/Contents/A/B/t.mp3"
        );
        assert_eq!(
            dedupe(&mut seen, "/Contents/A/B/T.MP3".into()),
            "/Contents/A/B/T-1.MP3"
        );
        assert_eq!(
            dedupe(&mut seen, "/Contents/a/b/t.mp3".into()),
            "/Contents/a/b/t-2.mp3"
        );
        assert_eq!(
            dedupe(&mut seen, "/Contents/A/B/t-1.mp3".into()),
            "/Contents/A/B/t-1-1.mp3"
        );
        assert_eq!(
            dedupe(&mut seen, "/Contents/A/B/noext".into()),
            "/Contents/A/B/noext"
        );
        assert_eq!(
            dedupe(&mut seen, "/Contents/A/B/noext".into()),
            "/Contents/A/B/noext-1"
        );
    }

    #[test]
    fn dedupe_seeded_with_stick_paths() {
        let mut seen: HashSet<String> = ["/Contents/A/B/t.mp3"]
            .iter()
            .map(|p| fold_key(p))
            .collect();
        assert_eq!(
            dedupe(&mut seen, "/Contents/A/B/t.mp3".into()),
            "/Contents/A/B/t-1.mp3"
        );
    }

    #[test]
    fn dedupe_shortens_stem_at_the_limit() {
        let dir = "/Contents/A/B/";
        let path = format!("{dir}{}.mp3", "s".repeat(MAX_PATH_LEN - dir.len() - 4));
        assert_eq!(path.len(), MAX_PATH_LEN);
        let mut seen = HashSet::new();
        dedupe(&mut seen, path.clone());
        let next = dedupe(&mut seen, path);
        assert_eq!(next.len(), MAX_PATH_LEN);
        assert!(next.ends_with("s-1.mp3"));
    }

    #[test]
    fn check_path_reports_problems() {
        assert_eq!(check_path("Contents/a"), Err(PathProblem::NotAbsolute));
        assert_eq!(check_path("/a//b"), Err(PathProblem::EmptyComponent));
        assert!(matches!(
            check_path("/a/b:c"),
            Err(PathProblem::IllegalChar { ch: ':', .. })
        ));
        assert!(matches!(
            check_path("/a./b"),
            Err(PathProblem::TrailingDotOrSpace { .. })
        ));
        assert!(matches!(
            check_path("/a/prn.mp3"),
            Err(PathProblem::Reserved { .. })
        ));
        assert_eq!(check_path("/e\u{301}"), Err(PathProblem::NotNfc));
        assert!(check_path("/Contents/Кино/\u{e9}.mp3").is_ok());
        let deep = "/a".repeat(MAX_DEPTH + 1) + "/f";
        assert_eq!(
            check_path(&deep),
            Err(PathProblem::TooDeep {
                depth: MAX_DEPTH + 1
            })
        );
        assert!(check_path(&("/a".repeat(MAX_DEPTH) + "/f")).is_ok());
        let long = format!("/{}", "x".repeat(MAX_PATH_LEN));
        assert_eq!(
            check_path(&long),
            Err(PathProblem::TooLong {
                len: MAX_PATH_LEN + 1
            })
        );
    }

    /// Arbitrary Unicode plus the characters most likely to break things.
    fn nasty_string() -> impl Strategy<Value = String> {
        prop_oneof![
            any::<String>(),
            "[ .\\\\/:*?\"<>|;\t\nA-Za-z0-9éКино坂本ひら🔥\u{301}\u{200d}]{0,80}",
            "(CON|prn|Aux|NUL|com[1-9]|LPT[1-9])[ .]{0,3}(\\.[a-z]{0,4})?",
        ]
    }

    proptest! {
        #[test]
        fn component_is_legal_ascii_and_bounded(s in nasty_string(), max in 0usize..300) {
            let out = sanitize_component(&s, max);
            prop_assert!(legal_ascii_component(&out), "{:?} -> {:?}", s, out);
            prop_assert!(is_nfc(&out));
            prop_assert!(out.len() <= max.clamp(1, MAX_COMPONENT_LEN));
            prop_assert_eq!(&sanitize_component(&s, max), &out);
            prop_assert_eq!(sanitize_component(&out, max), out);
        }

        #[test]
        fn contents_path_is_legal_and_bounded(a in nasty_string(), b in nasty_string(), f in nasty_string()) {
            let p = contents_path(&a, &b, &f);
            prop_assert!(check_path(&p).is_ok(), "{:?}: {:?}", p, check_path(&p));
            prop_assert!(p.is_ascii());
            prop_assert!(p.len() <= MAX_PATH_LEN - SUFFIX_RESERVE);
            prop_assert!(p.starts_with("/Contents/"));
            prop_assert_eq!(p.matches('/').count(), 4);
            prop_assert!(!p.split('/').skip(1).any(|c| c.starts_with(['.', ' '])));
            prop_assert_eq!(contents_path(&a, &b, &f), p);
        }

        #[test]
        fn dedupe_outputs_are_distinct_and_legal(
            names in prop::collection::vec((nasty_string(), any::<bool>()), 1..40)
        ) {
            let mut seen = HashSet::new();
            let mut folded = HashSet::new();
            for (name, upper) in names {
                let mut p = contents_path("Artist", "Album", &name);
                if upper {
                    p = p.to_uppercase();
                }
                let out = dedupe(&mut seen, p);
                prop_assert!(check_path(&out).is_ok(), "{:?}", out);
                prop_assert!(out.len() <= MAX_PATH_LEN);
                prop_assert!(folded.insert(fold_key(&out)), "duplicate {:?}", out);
            }
        }
    }
}
