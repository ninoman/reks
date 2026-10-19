//! rekordbox's local analysis cache (`USBANLZ`) for imported tracks.
//!
//! rekordbox keeps one `ANLZ0000.DAT` (plus `.EXT` and `.2EX`) per analysed
//! track under `share/PIONEER/USBANLZ/<dir>/<dir>/`. The `PPTH` tag in the
//! `.DAT` names the audio file: rekordbox 7 writes `?/<file name>`, older
//! versions the full path. Matching is baken's own
//! ([`baken_export::anlz::locate::AnlzIndex`]): same file name, then the beat
//! grid must agree with the XML's first `TEMPO` and, to break ties, the
//! length with `TotalTime`. We delegate to it so the dry run predicts exactly
//! the files baken copies (D7); this module adds the scan-once [`RbCache`],
//! sibling paths and an explicit [`Missing`] reason.
//!
//! The planner (Phase 2 task 4) builds an [`RbCache`] once per export and
//! calls [`RbCache::locate`] per track. Build the [`AnlzQuery`] from the
//! temp `collection.xml` handed to baken (`AnlzQuery::from(&baken Track)`),
//! so both see the same `Location`, `TotalTime` and `TEMPO` values.

use std::path::{Path, PathBuf};

use baken_export::anlz::locate::{AnlzIndex, Entry};
use baken_export::collection::{Tempo, Track};

/// Where rekordbox keeps its analysis on this machine, existing directories
/// only. Delegates to baken's resolution:
///
/// - macOS: `~/Library/Pioneer/rekordbox/share/PIONEER/USBANLZ` (checked
///   2026-10-19 with rekordbox installed: `USBANLZ/<3 hex>/<id>/ANLZ0000.*`).
/// - Windows: `%APPDATA%\Pioneer\rekordbox\share\PIONEER\USBANLZ`
///   (**UNVERIFIED**, from baken).
/// - Every mounted volume's `PIONEER/Master/share/PIONEER/USBANLZ` (a
///   rekordbox library on an external drive; under `/Volumes` on macOS,
///   `/media`, `/mnt`, `/run/media/$USER` on Linux; none on Windows).
///   **UNVERIFIED**.
pub fn default_roots() -> Vec<PathBuf> {
    baken_export::anlz::locate::default_roots()
}

/// The analysis files rekordbox made for one track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnlzSet {
    /// `.../ANLZ0000.DAT` (always present).
    pub dat: PathBuf,
    /// `ANLZ0000.EXT` next to it, if it exists (colour waveforms, NXS2).
    pub ext: Option<PathBuf>,
    /// `ANLZ0000.2EX` next to it, if it exists (3-band waveforms, CDJ-3000).
    pub two_ex: Option<PathBuf>,
    /// Beats in the `PQTZ` grid; 0 when rekordbox stored no grid.
    pub beats: u32,
}

/// Why a track has no rekordbox analysis to copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Missing {
    /// None of the roots exists (rekordbox not installed, or not here).
    NoRoots,
    /// No analysis file names this track's file.
    NotAnalysed,
    /// Files with this file name exist, but their grid or length disagrees
    /// with the track's (another file of the same name, or re-analysed since
    /// the XML was exported). baken refuses those (issue #180), so do we.
    Mismatch,
}

impl std::fmt::Display for Missing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NoRoots => "rekordbox's analysis folder was not found on this computer",
            Self::NotAnalysed => "rekordbox has no analysis for this file",
            Self::Mismatch => {
                "rekordbox's analysis for a file of this name does not match this track's beat grid"
            }
        })
    }
}

/// Result of a cache lookup: found, or missing with a reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lookup {
    Found(AnlzSet),
    Missing(Missing),
}

impl Lookup {
    pub fn found(&self) -> Option<&AnlzSet> {
        match self {
            Self::Found(set) => Some(set),
            Self::Missing(_) => None,
        }
    }
}

/// What the lookup needs from a track: the values baken reads from the XML.
#[derive(Debug, Clone, PartialEq)]
pub struct AnlzQuery {
    /// Decoded `Location` path (only the file name is matched).
    pub path: String,
    /// `TotalTime`, whole seconds.
    pub total_time_s: u32,
    /// First `TEMPO`: (`Inizio` seconds, `Bpm`); `None` without a grid.
    pub first_tempo: Option<(f64, f64)>,
}

impl From<&Track> for AnlzQuery {
    fn from(t: &Track) -> Self {
        Self {
            path: t.location.clone(),
            total_time_s: t.total_time,
            first_tempo: t.tempos.first().map(|tp| (tp.inizio, tp.bpm)),
        }
    }
}

impl AnlzQuery {
    fn to_track(&self) -> Track {
        Track {
            // baken splits on '/' only; normalise Windows separators.
            location: self.path.replace('\\', "/"),
            total_time: self.total_time_s,
            tempos: self
                .first_tempo
                .map(|(inizio, bpm)| Tempo {
                    inizio,
                    bpm,
                    metro: "4/4".to_owned(),
                    battito: 1,
                })
                .into_iter()
                .collect(),
            ..Track::default()
        }
    }
}

/// Errors from scanning the cache.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A root could not be listed (permissions, I/O). The message is baken's.
    #[error("could not scan rekordbox's analysis folders {roots:?}: {message}")]
    Scan {
        roots: Vec<PathBuf>,
        message: String,
    },
}

/// An index of rekordbox's analysis files, scanned once.
#[derive(Debug)]
pub struct RbCache {
    roots: Vec<PathBuf>,
    index: AnlzIndex,
}

impl RbCache {
    /// Scans `roots` (missing ones are skipped) and indexes every
    /// `ANLZ0000.DAT` by the file name in its `PPTH`. Unreadable or
    /// malformed `.DAT` files are left out, as baken does.
    pub fn scan(roots: &[PathBuf]) -> Result<Self, Error> {
        let roots: Vec<PathBuf> = roots.iter().filter(|r| r.is_dir()).cloned().collect();
        let index = AnlzIndex::build(&roots).map_err(|e| Error::Scan {
            roots: roots.clone(),
            message: format!("{e:#}"),
        })?;
        tracing::debug!(roots = ?roots, files = index.files, "rekordbox analysis cache indexed");
        Ok(Self { roots, index })
    }

    /// Roots that existed when scanned.
    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    /// Number of `.DAT` files indexed.
    pub fn files_indexed(&self) -> usize {
        self.index.files
    }

    /// rekordbox's analysis for `q`, or why there is none.
    pub fn locate(&self, q: &AnlzQuery) -> Lookup {
        if self.roots.is_empty() {
            return Lookup::Missing(Missing::NoRoots);
        }
        let track = q.to_track();
        match self.index.find(&track) {
            Some(entry) => Lookup::Found(anlz_set(entry)),
            None if self.index.has_name(&track) => Lookup::Missing(Missing::Mismatch),
            None => Lookup::Missing(Missing::NotAnalysed),
        }
    }
}

/// One-off lookup: scans `roots` and locates `q`. Use [`RbCache`] for more
/// than one track.
pub fn locate(q: &AnlzQuery, roots: &[PathBuf]) -> Result<Lookup, Error> {
    Ok(RbCache::scan(roots)?.locate(q))
}

fn anlz_set(e: &Entry) -> AnlzSet {
    let existing = |ext: &str| Some(e.sibling(ext)).filter(|p| Path::is_file(p));
    AnlzSet {
        dat: e.dat.clone(),
        ext: existing("EXT"),
        two_ex: existing("2EX"),
        beats: e.beats,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use baken_export::anlz::section::{section, AnlzFile};

    /// A minimal `.DAT`: `PPTH` = `ppth`, plus a constant `PQTZ` grid of
    /// `beats` beats at `bpm_x100` from `first_ms` (none if `beats == 0`).
    fn dat(ppth: &str, beats: u32, bpm_x100: u16, first_ms: u32) -> Vec<u8> {
        let mut file = AnlzFile {
            header_tail: [0, 0, 0, 1, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0],
            sections: Vec::new(),
        };
        if beats > 0 {
            let mut payload = Vec::new();
            payload.extend_from_slice(&0u32.to_be_bytes());
            payload.extend_from_slice(&0x0008_0000u32.to_be_bytes());
            payload.extend_from_slice(&beats.to_be_bytes());
            let step = 60_000.0 / (f64::from(bpm_x100) / 100.0);
            for i in 0..beats {
                payload.extend_from_slice(&((i % 4 + 1) as u16).to_be_bytes());
                payload.extend_from_slice(&bpm_x100.to_be_bytes());
                let t = f64::from(first_ms) + f64::from(i) * step;
                payload.extend_from_slice(&(t.round() as u32).to_be_bytes());
            }
            file.sections.push(section(b"PQTZ", 0x18, &payload));
        }
        file.set_path(ppth);
        file.to_bytes()
    }

    /// Writes `USBANLZ/<a>/<b>/ANLZ0000.DAT` (+ `.EXT` / `.2EX`).
    fn put(root: &Path, a: &str, b: &str, bytes: &[u8], ext: bool, two_ex: bool) -> PathBuf {
        let dir = root.join(a).join(b);
        std::fs::create_dir_all(&dir).unwrap();
        let dat = dir.join("ANLZ0000.DAT");
        std::fs::write(&dat, bytes).unwrap();
        if ext {
            std::fs::write(dir.join("ANLZ0000.EXT"), bytes).unwrap();
        }
        if two_ex {
            std::fs::write(dir.join("ANLZ0000.2EX"), bytes).unwrap();
        }
        dat
    }

    fn query(path: &str, total: u32, tempo: Option<(f64, f64)>) -> AnlzQuery {
        AnlzQuery {
            path: path.to_owned(),
            total_time_s: total,
            first_tempo: tempo,
        }
    }

    /// 126 BPM for 300 s from 0.05 s: 630 beats.
    const BEATS_300S: u32 = 630;

    #[test]
    fn finds_rekordbox_7_and_older_layouts() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("USBANLZ");
        // rekordbox 7: `?/<name>`, uuid-like folder; with EXT and 2EX.
        let new = put(
            &root,
            "000",
            "e520f-6e7f",
            &dat("?/Track One.mp3", BEATS_300S, 12600, 50),
            true,
            true,
        );
        // rekordbox 6: full path, P0xx/0000xxxx; DAT and EXT only.
        let old = put(
            &root,
            "P016",
            "0000875E",
            &dat("/Users/dj/Music/Ü – two.aiff", BEATS_300S, 12600, 50),
            true,
            false,
        );
        // Not an analysis file: skipped, not an error.
        put(&root, "P017", "00000001", b"garbage", false, false);

        let cache = RbCache::scan(std::slice::from_ref(&root)).unwrap();
        assert_eq!(cache.files_indexed(), 2);

        let one = cache.locate(&query(
            "/Volumes/Music/Track One.mp3",
            300,
            Some((0.05, 126.0)),
        ));
        assert_eq!(
            one,
            Lookup::Found(AnlzSet {
                dat: new.clone(),
                ext: Some(new.with_extension("EXT")),
                two_ex: Some(new.with_extension("2EX")),
                beats: BEATS_300S,
            })
        );
        let two = cache.locate(&query(r"C:\Music\Ü – two.aiff", 300, Some((0.05, 126.0))));
        let set = two.found().unwrap();
        assert_eq!(set.dat, old);
        assert_eq!(set.ext, Some(old.with_extension("EXT")));
        assert_eq!(set.two_ex, None);
    }

    #[test]
    fn missing_is_explicit() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("USBANLZ");
        put(
            &root,
            "000",
            "a",
            &dat("?/x.mp3", BEATS_300S, 12600, 50),
            false,
            false,
        );
        let cache = RbCache::scan(std::slice::from_ref(&root)).unwrap();

        let never = cache.locate(&query("/m/y.mp3", 300, Some((0.05, 126.0))));
        assert_eq!(never, Lookup::Missing(Missing::NotAnalysed));
        // Same name, grid one beat later: another file's analysis.
        let other = cache.locate(&query("/m/x.mp3", 300, Some((0.526, 126.0))));
        assert_eq!(other, Lookup::Missing(Missing::Mismatch));
        // Same name, no grid in the library: only a grid-less analysis fits.
        let no_grid = cache.locate(&query("/m/x.mp3", 300, None));
        assert_eq!(no_grid, Lookup::Missing(Missing::Mismatch));

        let none = RbCache::scan(&[tmp.path().join("nope")]).unwrap();
        assert!(none.roots().is_empty());
        assert_eq!(
            none.locate(&query("/m/x.mp3", 300, Some((0.05, 126.0)))),
            Lookup::Missing(Missing::NoRoots)
        );
    }

    #[test]
    fn duplicate_names_pick_the_matching_grid_and_length() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("USBANLZ");
        let edit = put(
            &root,
            "000",
            "edit",
            &dat("?/x.wav", 400, 12600, 50),
            false,
            false,
        );
        let full = put(
            &root,
            "000",
            "full",
            &dat("?/x.wav", BEATS_300S, 12600, 50),
            false,
            false,
        );
        let cache = RbCache::scan(std::slice::from_ref(&root)).unwrap();
        let at = |total| {
            cache
                .locate(&query("/m/x.wav", total, Some((0.05, 126.0))))
                .found()
                .map(|s| s.dat.clone())
        };
        assert_eq!(at(300), Some(full));
        assert_eq!(at(190), Some(edit));
    }

    #[test]
    fn one_off_locate_and_query_from_baken_track() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("USBANLZ");
        let dat_path = put(
            &root,
            "000",
            "a",
            &dat("?/a b.flac", BEATS_300S, 12600, 50),
            false,
            false,
        );
        let track = Track {
            location: "/Music/a b.flac".into(),
            total_time: 300,
            tempos: vec![Tempo {
                inizio: 0.05,
                bpm: 126.0,
                metro: "4/4".into(),
                battito: 1,
            }],
            ..Track::default()
        };
        let found = locate(&AnlzQuery::from(&track), &[root]).unwrap();
        assert_eq!(found.found().map(|s| &s.dat), Some(&dat_path));
    }

    #[test]
    fn default_roots_exist() {
        for root in default_roots() {
            assert!(root.is_dir(), "{}", root.display());
        }
    }
}
