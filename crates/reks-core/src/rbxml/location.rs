//! `Location` URLs (`file://localhost/...`) ↔ filesystem paths.
//!
//! rekordbox writes absolute paths as `file://localhost/` + percent-encoded
//! UTF-8, with Windows paths as `file://localhost/C:/...`. Which characters it
//! leaves unencoded is UNVERIFIED (see `docs/phases/phase-1.md`, task 6 notes),
//! so decoding accepts any mix and encoding is conservative.

use std::path::{Path, PathBuf};

use percent_encoding::{percent_decode_str, utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use unicode_normalization::UnicodeNormalization;

use super::RbXmlError;

/// Bytes left as-is by [`path_to_location`]: RFC 3986 unreserved plus `/`.
const PATH_SET: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~')
    .remove(b'/');

/// Convert a rekordbox `Location` to a path: percent-decoded, NFC-normalised.
///
/// Accepts `file://localhost/...` and `file:///...`. `#`, `?` and `+` are
/// literal filename characters, not URL syntax. A drive-letter form
/// (`file://localhost/C:/Music/x.mp3`) yields `C:/Music/x.mp3`.
pub fn location_to_path(url: &str) -> crate::Result<PathBuf> {
    let invalid = |reason| RbXmlError::InvalidLocation {
        url: url.to_owned(),
        reason,
    };
    let rest = strip_prefix_ci(url, "file://localhost/")
        .or_else(|| strip_prefix_ci(url, "file:///"))
        .ok_or_else(|| {
            if strip_prefix_ci(url, "file://").is_some() {
                invalid("host must be localhost")
            } else {
                invalid("not a file:// URL")
            }
        })?;
    let decoded = percent_decode_str(rest)
        .decode_utf8()
        .map_err(|_| invalid("not UTF-8 after percent-decoding"))?;
    let path: String = decoded.nfc().collect();
    if path.is_empty() {
        return Err(invalid("empty path").into());
    }
    if is_drive_form(&path) {
        Ok(PathBuf::from(path))
    } else {
        Ok(PathBuf::from(format!("/{path}")))
    }
}

/// Convert an absolute path to a rekordbox `Location` URL (NFC, `/`
/// separators, everything but `A–Z a–z 0–9 - . _ ~ /` percent-encoded; the
/// colon of a drive letter is kept).
pub fn path_to_location(path: &Path) -> crate::Result<String> {
    let invalid = |reason| RbXmlError::InvalidLocation {
        url: path.display().to_string(),
        reason,
    };
    let s = path
        .to_str()
        .ok_or_else(|| invalid("path is not valid Unicode"))?;
    let mut s: String = s.nfc().collect();
    if cfg!(windows) {
        s = s.replace('\\', "/");
    }
    let mut out = String::from("file://localhost/");
    if is_drive_form(&s) {
        out.push_str(&s[..2]);
        out.extend(utf8_percent_encode(&s[2..], PATH_SET));
    } else if let Some(rest) = s.strip_prefix('/') {
        out.extend(utf8_percent_encode(rest, PATH_SET));
    } else {
        return Err(invalid("path is not absolute").into());
    }
    Ok(out)
}

fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let head = s.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then(|| &s[prefix.len()..])
}

/// `C:` or `C:/...`.
fn is_drive_form(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b.len() == 2 || b[2] == b'/')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(url: &str) -> String {
        location_to_path(url).unwrap().to_str().unwrap().to_owned()
    }

    #[test]
    fn spaces_and_reserved_characters() {
        assert_eq!(
            p("file://localhost/Users/reks/Music/reks%20fixtures/a.mp3"),
            "/Users/reks/Music/reks fixtures/a.mp3"
        );
        // '#', '?' and '+' are filename characters, encoded or not.
        assert_eq!(
            p("file://localhost/M/Track%20%2301.mp3"),
            "/M/Track #01.mp3"
        );
        assert_eq!(p("file://localhost/M/Track #01?.mp3"), "/M/Track #01?.mp3");
        assert_eq!(p("file://localhost/M/a+b.mp3"), "/M/a+b.mp3");
        // A literal percent sign, encoded, and a stray one that isn't an escape.
        assert_eq!(
            p("file://localhost/M/100%25%20Pure.mp3"),
            "/M/100% Pure.mp3"
        );
        assert_eq!(p("file://localhost/M/50%zz.mp3"), "/M/50%zz.mp3");
        assert_eq!(p("file://localhost/M/a&b%27s.mp3"), "/M/a&b's.mp3");
    }

    #[test]
    fn unicode_and_nfd() {
        // "Beyoncé" with precomposed é (U+00E9).
        assert_eq!(
            p("file://localhost/M/Beyonc%C3%A9.mp3"),
            "/M/Beyonc\u{e9}.mp3"
        );
        // Same name in NFD (e + U+0301), as macOS often writes it: comes out NFC.
        assert_eq!(
            p("file://localhost/M/Beyonce%CC%81.mp3"),
            "/M/Beyonc\u{e9}.mp3"
        );
        // Raw (unencoded) NFD input is normalised too.
        assert_eq!(
            p("file://localhost/M/Beyonce\u{301}.mp3"),
            "/M/Beyonc\u{e9}.mp3"
        );
        assert_eq!(
            p("file://localhost/M/%E6%97%A5%E6%9C%AC.flac"),
            "/M/日本.flac"
        );
    }

    #[test]
    fn windows_drive_letter() {
        assert_eq!(p("file://localhost/C:/Music/a%20b.mp3"), "C:/Music/a b.mp3");
        assert_eq!(p("file://localhost/d:/x.wav"), "d:/x.wav");
        assert_eq!(p("file:///C:/x.wav"), "C:/x.wav");
        // Not a drive: a directory literally named "C:x" on Unix.
        assert_eq!(p("file://localhost/C:x/y.wav"), "/C:x/y.wav");
    }

    #[test]
    fn accepted_and_rejected_forms() {
        assert_eq!(p("file:///Users/a.mp3"), "/Users/a.mp3");
        assert_eq!(p("FILE://LOCALHOST/Users/a.mp3"), "/Users/a.mp3");
        for bad in [
            "http://localhost/a.mp3",
            "file://server/share/a.mp3",
            "/Users/a.mp3",
            "file://localhost/",
            "file://localhost/M/%FF.mp3",
            "",
        ] {
            assert!(location_to_path(bad).is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn path_to_location_round_trip() {
        for path in [
            "/Users/reks/Music/reks fixtures/sine-click-128.mp3",
            "/M/Track #01 (100% Pure) [a&b] + 'x'?.mp3",
            "/M/Beyonc\u{e9}/日本.flac",
            "C:/Music/a b.mp3",
        ] {
            let url = path_to_location(Path::new(path)).unwrap();
            assert!(url.is_ascii(), "{url}");
            assert_eq!(p(&url), path);
        }
        assert_eq!(
            path_to_location(Path::new("/Users/a b/c#.mp3")).unwrap(),
            "file://localhost/Users/a%20b/c%23.mp3"
        );
        assert_eq!(
            path_to_location(Path::new("C:/a b.mp3")).unwrap(),
            "file://localhost/C:/a%20b.mp3"
        );
        // NFD input is written as NFC.
        assert_eq!(
            path_to_location(Path::new("/e\u{301}")).unwrap(),
            "file://localhost/%C3%A9"
        );
        assert!(path_to_location(Path::new("relative/a.mp3")).is_err());
    }
}
