//! Musical keys: parsing every common notation into one canonical form.
//!
//! The library stores keys as canonical classical names (`track.key`), one
//! fixed spelling per key, following the Camelot wheel's labels:
//!
//! | Camelot | Open Key | canonical | Camelot | Open Key | canonical |
//! |---------|----------|-----------|---------|----------|-----------|
//! | 1A  | 6m  | Abm | 1B  | 6d  | B  |
//! | 2A  | 7m  | Ebm | 2B  | 7d  | F# |
//! | 3A  | 8m  | Bbm | 3B  | 8d  | Db |
//! | 4A  | 9m  | Fm  | 4B  | 9d  | Ab |
//! | 5A  | 10m | Cm  | 5B  | 10d | Eb |
//! | 6A  | 11m | Gm  | 6B  | 11d | Bb |
//! | 7A  | 12m | Dm  | 7B  | 12d | F  |
//! | 8A  | 1m  | Am  | 8B  | 1d  | C  |
//! | 9A  | 2m  | Em  | 9B  | 2d  | G  |
//! | 10A | 3m  | Bm  | 10B | 3d  | D  |
//! | 11A | 4m  | F#m | 11B | 4d  | A  |
//! | 12A | 5m  | Dbm | 12B | 5d  | E  |
//!
//! [`Key::parse`] accepts Camelot (`8A`, `08a`), Open Key (`1m`, `1d`) and
//! classical names (`Am`, `A minor`, `Amin`, `C#`, `Db major`, `G♭m`,
//! enharmonics such as `Cb` or `E#`).
//!
//! rekordbox's XML `Tonality` attribute follows the user's key-display
//! preference (classical or alphanumeric). Both are accepted here; the exact
//! set of strings rekordbox emits is UNVERIFIED until checked against a real
//! `collection.xml` (phase 1 task 7).

use std::fmt;
use std::str::FromStr;

/// Major or minor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Mode {
    Major,
    Minor,
}

/// One of the 24 major/minor keys.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(into = "String", try_from = "String")]
pub struct Key {
    /// Pitch class of the tonic, 0 = C … 11 = B.
    tonic: u8,
    mode: Mode,
}

/// A string that is not a recognised key notation.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unrecognised musical key {0:?}")]
pub struct KeyParseError(pub String);

const MAJOR_NAMES: [&str; 12] = [
    "C", "Db", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B",
];
const MINOR_NAMES: [&str; 12] = [
    "Cm", "Dbm", "Dm", "Ebm", "Em", "Fm", "F#m", "Gm", "Abm", "Am", "Bbm", "Bm",
];

/// Pitch class of Camelot `1A` (A♭ minor) and `1B` (B major). Each step
/// round the wheel adds a fifth (7 semitones).
const CAMELOT_1A: u8 = 8;
const CAMELOT_1B: u8 = 11;

impl Key {
    /// Builds a key from a tonic pitch class (taken mod 12) and mode.
    pub fn new(tonic: u8, mode: Mode) -> Self {
        Self {
            tonic: tonic % 12,
            mode,
        }
    }

    /// All 24 keys, majors then minors, ordered by pitch class.
    pub fn all() -> impl Iterator<Item = Key> {
        [Mode::Major, Mode::Minor]
            .into_iter()
            .flat_map(|mode| (0..12).map(move |pc| Key::new(pc, mode)))
    }

    /// Pitch class of the tonic, 0 = C … 11 = B.
    pub fn tonic(self) -> u8 {
        self.tonic
    }

    pub fn mode(self) -> Mode {
        self.mode
    }

    /// Parses any supported notation. Surrounding whitespace is ignored.
    pub fn parse(s: &str) -> Result<Self, KeyParseError> {
        let t = s.trim();
        parse_wheel(t)
            .or_else(|| parse_classical(t))
            .ok_or_else(|| KeyParseError(s.to_owned()))
    }

    /// Canonical name stored in the library, e.g. `Am`, `F#m`, `Db`.
    pub fn canonical(self) -> &'static str {
        match self.mode {
            Mode::Major => MAJOR_NAMES[usize::from(self.tonic)],
            Mode::Minor => MINOR_NAMES[usize::from(self.tonic)],
        }
    }

    /// Camelot wheel number, 1..=12.
    pub fn camelot_number(self) -> u8 {
        let base = match self.mode {
            Mode::Major => CAMELOT_1B,
            Mode::Minor => CAMELOT_1A,
        };
        // 7 is its own inverse mod 12, so stepping back by fifths is `* 7`.
        ((self.tonic + 12 - base) * 7) % 12 + 1
    }

    /// Camelot notation, e.g. `8A`.
    pub fn camelot(self) -> String {
        let letter = match self.mode {
            Mode::Major => 'B',
            Mode::Minor => 'A',
        };
        format!("{}{letter}", self.camelot_number())
    }

    /// Open Key notation, e.g. `1m`. Open Key `1` is Camelot `8`.
    pub fn open_key(self) -> String {
        let n = (self.camelot_number() - 1 + 5) % 12 + 1;
        let letter = match self.mode {
            Mode::Major => 'd',
            Mode::Minor => 'm',
        };
        format!("{n}{letter}")
    }

    fn from_camelot(n: u8, mode: Mode) -> Self {
        let base = match mode {
            Mode::Major => CAMELOT_1B,
            Mode::Minor => CAMELOT_1A,
        };
        Self::new(base + 7 * (n - 1), mode)
    }
}

/// Camelot (`8A`) or Open Key (`1m`): 1–12 then one letter.
fn parse_wheel(t: &str) -> Option<Key> {
    let split = t.find(|c: char| !c.is_ascii_digit())?;
    let (digits, letter) = t.split_at(split);
    if digits.is_empty() || digits.len() > 2 {
        return None;
    }
    let n: u8 = digits.parse().ok()?;
    if !(1..=12).contains(&n) {
        return None;
    }
    match letter {
        "A" | "a" => Some(Key::from_camelot(n, Mode::Minor)),
        "B" | "b" => Some(Key::from_camelot(n, Mode::Major)),
        // Open Key n == Camelot n + 7 (mod 12).
        "m" | "M" => Some(Key::from_camelot((n - 1 + 7) % 12 + 1, Mode::Minor)),
        "d" | "D" => Some(Key::from_camelot((n - 1 + 7) % 12 + 1, Mode::Major)),
        _ => None,
    }
}

/// Classical names: letter, optional accidental, optional mode word.
fn parse_classical(t: &str) -> Option<Key> {
    let mut chars = t.chars();
    let natural: u8 = match chars.next()?.to_ascii_uppercase() {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };
    let rest = chars.as_str();
    let (tonic, rest) = match rest.chars().next() {
        Some(c @ ('#' | '♯')) => (natural + 1, &rest[c.len_utf8()..]),
        Some(c @ ('b' | '♭')) => (natural + 11, &rest[c.len_utf8()..]),
        _ => (natural, rest),
    };
    let mode = match rest.trim_start().to_ascii_lowercase().as_str() {
        "" | "maj" | "major" => Mode::Major,
        "m" | "min" | "minor" => Mode::Minor,
        _ => return None,
    };
    Some(Key::new(tonic, mode))
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.canonical())
    }
}

impl FromStr for Key {
    type Err = KeyParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Key::parse(s)
    }
}

impl From<Key> for String {
    fn from(k: Key) -> Self {
        k.canonical().to_owned()
    }
}

impl TryFrom<String> for Key {
    type Error = KeyParseError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Key::parse(&s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// (Camelot, Open Key, canonical) — the table in the module docs.
    const WHEEL: [(&str, &str, &str); 24] = [
        ("1A", "6m", "Abm"),
        ("2A", "7m", "Ebm"),
        ("3A", "8m", "Bbm"),
        ("4A", "9m", "Fm"),
        ("5A", "10m", "Cm"),
        ("6A", "11m", "Gm"),
        ("7A", "12m", "Dm"),
        ("8A", "1m", "Am"),
        ("9A", "2m", "Em"),
        ("10A", "3m", "Bm"),
        ("11A", "4m", "F#m"),
        ("12A", "5m", "Dbm"),
        ("1B", "6d", "B"),
        ("2B", "7d", "F#"),
        ("3B", "8d", "Db"),
        ("4B", "9d", "Ab"),
        ("5B", "10d", "Eb"),
        ("6B", "11d", "Bb"),
        ("7B", "12d", "F"),
        ("8B", "1d", "C"),
        ("9B", "2d", "G"),
        ("10B", "3d", "D"),
        ("11B", "4d", "A"),
        ("12B", "5d", "E"),
    ];

    fn k(s: &str) -> Key {
        Key::parse(s).unwrap()
    }

    #[test]
    fn wheel_table_round_trips() {
        for (camelot, open, canonical) in WHEEL {
            let key = k(camelot);
            assert_eq!(key.canonical(), canonical, "{camelot}");
            assert_eq!(key.camelot(), camelot);
            assert_eq!(key.open_key(), open, "{camelot}");
            assert_eq!(k(open), key, "{open}");
            assert_eq!(k(canonical), key, "{canonical}");
        }
    }

    #[test]
    fn all_24_keys_are_distinct_and_round_trip() {
        let keys: Vec<Key> = Key::all().collect();
        assert_eq!(keys.len(), 24);
        let mut canon: Vec<&str> = keys.iter().map(|k| k.canonical()).collect();
        canon.sort_unstable();
        canon.dedup();
        assert_eq!(canon.len(), 24);
        for key in keys {
            assert_eq!(k(key.canonical()), key);
            assert_eq!(k(&key.camelot()), key);
            assert_eq!(k(&key.open_key()), key);
        }
    }

    #[test]
    fn spelling_variants() {
        let cases = [
            // Camelot / Open Key variants
            ("08A", "Am"),
            ("8a", "Am"),
            (" 8A ", "Am"),
            ("12b", "E"),
            ("1M", "Am"),
            ("1D", "C"),
            // classical
            ("A", "A"),
            ("a", "A"),
            ("am", "Am"),
            ("Amin", "Am"),
            ("A minor", "Am"),
            ("A Minor", "Am"),
            ("Amaj", "A"),
            ("A major", "A"),
            ("A Major", "A"),
            ("C#m", "Dbm"),
            ("C# minor", "Dbm"),
            ("C♯m", "Dbm"),
            ("Dbm", "Dbm"),
            ("D♭m", "Dbm"),
            ("G#m", "Abm"),
            ("Gb", "F#"),
            ("G♭", "F#"),
            ("F#", "F#"),
            ("Bb", "Bb"),
            ("bb", "Bb"),
            ("bbm", "Bbm"),
            ("A#", "Bb"),
            ("D#m", "Ebm"),
            // enharmonic edge spellings
            ("Cb", "B"),
            ("B#", "C"),
            ("E#", "F"),
            ("Fb", "E"),
        ];
        for (input, canonical) in cases {
            assert_eq!(k(input).canonical(), canonical, "{input:?}");
        }
    }

    #[test]
    fn rekordbox_tonality_samples() {
        // From fixtures/rekordbox/collection-small.xml, plus both notations
        // rekordbox can emit (see module docs; exact set UNVERIFIED).
        for (tonality, canonical) in [("Am", "Am"), ("F#m", "F#m"), ("Db", "Db"), ("8A", "Am")] {
            assert_eq!(k(tonality).canonical(), canonical, "{tonality:?}");
        }
    }

    #[test]
    fn rejects_garbage() {
        for bad in [
            "",
            "  ",
            "0A",
            "13A",
            "0B",
            "100A",
            "8C",
            "8",
            "H",
            "Hm",
            "Xm",
            "Am7",
            "A##",
            "A minor key",
            "m",
            "#",
            "1x",
        ] {
            assert!(Key::parse(bad).is_err(), "{bad:?} should not parse");
        }
    }

    #[test]
    fn serde_uses_canonical_string() {
        let json = serde_json::to_string(&k("8A")).unwrap();
        assert_eq!(json, "\"Am\"");
        let back: Key = serde_json::from_str("\"11A\"").unwrap();
        assert_eq!(back.canonical(), "F#m");
        assert!(serde_json::from_str::<Key>("\"nope\"").is_err());
    }
}
