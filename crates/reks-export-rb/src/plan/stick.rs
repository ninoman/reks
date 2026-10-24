//! What is on the stick already: files under the three folders an export
//! owns, signs that rekordbox wrote it, which existing audio file still holds
//! a track's current source, and what has gone stale.
//!
//! Read-only: directories are listed and candidate files opened for reading,
//! so a dry run leaves every modification time as it was.
//!
//! **Reuse rule.** A stick file holds a track's current audio when it has the
//! same size and extension, was written no earlier than the source last
//! changed (with FAT's 2-second slack), and [`SAMPLES`] pieces of
//! [`SAMPLE_LEN`] bytes at the start, middle and end are identical. The time
//! test is "not older" rather than "equal" because the writer's copy gets the
//! time it was written (baken does not carry times over). Candidates at the
//! track's natural path win, then the rest in path order. Only ASCII paths
//! that pass `fsname::check_path` are reused (D24), so files rekordbox wrote
//! with Unicode names are copied afresh and the old ones go stale.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use reks_core::fsname;
use unicode_normalization::UnicodeNormalization;

use super::{on_stick, PlanError};

/// Bytes compared per sample when matching a stick file to its source.
pub(crate) const SAMPLE_LEN: u64 = 64 * 1024;
/// Samples per file: start, middle, end.
pub(crate) const SAMPLES: u64 = 3;
/// FAT stores modification times in 2-second steps.
const FAT_TIME_SLACK: Duration = Duration::from_secs(2);

const CONTENTS: &str = "/Contents";
const USBANLZ: &str = "/PIONEER/USBANLZ";
const ARTWORK: &str = "/PIONEER/Artwork";
const ANLZ_EXTS: [&str; 3] = ["DAT", "EXT", "2EX"];

/// One file on the stick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StickFile {
    /// `/`-separated from the stick root, NFC (macOS lists FAT names in NFD).
    pub rel: String,
    pub abs: PathBuf,
    pub size: u64,
    pub mtime: Option<SystemTime>,
}

/// The export-owned folders of a stick.
#[derive(Debug, Default)]
pub(crate) struct Stick {
    pub contents: Vec<StickFile>,
    pub anlz: Vec<StickFile>,
    pub artwork: Vec<StickFile>,
}

/// Lists `/Contents`, `/PIONEER/USBANLZ` and `/PIONEER/Artwork`, sorted by
/// path. macOS `._*` AppleDouble files are left out: they belong to their
/// file and the writer removes them with it.
pub(crate) fn scan(root: &Path) -> Result<Stick, PlanError> {
    Ok(Stick {
        contents: list(root, CONTENTS)?,
        anlz: list(root, USBANLZ)?,
        artwork: list(root, ARTWORK)?,
    })
}

fn list(root: &Path, rel: &str) -> Result<Vec<StickFile>, PlanError> {
    let mut out = Vec::new();
    let dir = on_stick(root, rel);
    if dir.is_dir() {
        walk(&dir, rel, &mut out)?;
    }
    out.sort_by(|a, b| a.rel.cmp(&b.rel));
    Ok(out)
}

fn walk(dir: &Path, rel: &str, out: &mut Vec<StickFile>) -> Result<(), PlanError> {
    let err = |source| PlanError::Stick {
        path: dir.to_path_buf(),
        source,
    };
    for entry in std::fs::read_dir(dir).map_err(err)? {
        let entry = entry.map_err(err)?;
        let name: String = entry.file_name().to_string_lossy().nfc().collect();
        if name.starts_with("._") {
            continue;
        }
        let child = format!("{rel}/{name}");
        let path = entry.path();
        let meta = entry.metadata().map_err(|source| PlanError::Stick {
            path: path.clone(),
            source,
        })?;
        if meta.is_dir() {
            walk(&path, &child, out)?;
        } else {
            out.push(StickFile {
                rel: child,
                abs: path,
                size: meta.len(),
                mtime: meta.modified().ok(),
            });
        }
    }
    Ok(())
}

/// Files only rekordbox writes: `exportLibrary.db` (OneLibrary; baken writes
/// it only on request, which we never make), `exportExt.pdb`, and `*.sync`
/// anywhere under `/PIONEER` (location UNVERIFIED, rbxport notes, R§1).
pub(crate) fn rekordbox_markers(root: &Path) -> Result<Vec<String>, PlanError> {
    let mut found = Vec::new();
    for rel in [
        "/PIONEER/rekordbox/exportLibrary.db",
        "/PIONEER/rekordbox/exportExt.pdb",
    ] {
        if on_stick(root, rel).is_file() {
            found.push(rel.to_owned());
        }
    }
    let mut syncs: Vec<String> = list(root, "/PIONEER")?
        .into_iter()
        .map(|f| f.rel)
        .filter(|rel| {
            !rel.starts_with(&format!("{USBANLZ}/"))
                && !rel.starts_with(&format!("{ARTWORK}/"))
                && rel.to_ascii_lowercase().ends_with(".sync")
        })
        .collect();
    syncs.sort();
    found.extend(syncs);
    Ok(found)
}

/// Hands out existing `/Contents` files to the tracks whose audio they hold,
/// each file at most once.
pub(crate) struct Matcher<'a> {
    by_size: HashMap<u64, Vec<&'a StickFile>>,
    claimed: HashSet<String>,
}

impl<'a> Matcher<'a> {
    pub fn new(files: &'a [StickFile]) -> Self {
        let mut by_size: HashMap<u64, Vec<&StickFile>> = HashMap::new();
        for f in files {
            if f.rel.is_ascii() && fsname::check_path(&f.rel).is_ok() {
                by_size.entry(f.size).or_default().push(f);
            }
        }
        Self {
            by_size,
            claimed: HashSet::new(),
        }
    }

    /// The stick path of an unclaimed file holding `src`'s current audio,
    /// claimed for it; `None` when the track needs a copy.
    pub fn claim(
        &mut self,
        src: &Path,
        size: u64,
        mtime: SystemTime,
        natural: &str,
    ) -> Option<String> {
        let ext = extension(&src.to_string_lossy());
        let natural = fsname::fold_key(natural);
        let mut candidates: Vec<&StickFile> = self
            .by_size
            .get(&size)?
            .iter()
            .copied()
            .filter(|f| !self.claimed.contains(&fsname::fold_key(&f.rel)))
            .filter(|f| extension(&f.rel) == ext)
            .filter(|f| f.mtime.is_some_and(|m| m + FAT_TIME_SLACK >= mtime))
            .collect();
        // Natural path first; `files` is already in path order.
        candidates.sort_by_key(|f| fsname::fold_key(&f.rel) != natural);
        let hit = candidates
            .into_iter()
            .find(|f| same_samples(src, &f.abs, size))?;
        self.claimed.insert(fsname::fold_key(&hit.rel));
        Some(hit.rel.clone())
    }

    pub fn is_claimed(&self, rel: &str) -> bool {
        self.claimed.contains(&fsname::fold_key(rel))
    }
}

fn extension(path: &str) -> String {
    path.rsplit_once('.')
        .filter(|(_, ext)| !ext.contains('/'))
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default()
}

/// Compares [`SAMPLES`] pieces of `a` and `b` (both `size` bytes long).
/// Any read error counts as different.
fn same_samples(a: &Path, b: &Path, size: u64) -> bool {
    let read = |path: &Path| -> std::io::Result<Vec<u8>> {
        let mut f = File::open(path)?;
        let len = SAMPLE_LEN.min(size);
        let mut out = Vec::new();
        for i in 0..SAMPLES {
            let at = (size - len) * i / (SAMPLES - 1);
            f.seek(SeekFrom::Start(at))?;
            let mut buf = vec![0; usize::try_from(len).unwrap_or(0)];
            f.read_exact(&mut buf)?;
            out.extend(buf);
        }
        Ok(out)
    };
    matches!((read(a), read(b)), (Ok(x), Ok(y)) if x == y)
}

impl Stick {
    /// Files not referenced by an export of `keep` (stick paths of the
    /// planned audio). ANLZ files are referenced through [`anlz_dir`]: the
    /// `n` tracks sharing a directory own `ANLZ0000`..`ANLZ{n-1}`. With
    /// `artwork` on, the writer owns `/PIONEER/Artwork` and nothing there is
    /// listed; with it off, all of it is stale.
    pub fn stale(&self, keep: &[&str], artwork: bool) -> Vec<&StickFile> {
        let audio: HashSet<String> = keep.iter().map(|p| fsname::fold_key(p)).collect();
        let mut slots: HashMap<String, u16> = HashMap::new();
        for p in keep {
            *slots.entry(anlz_dir(p)).or_default() += 1;
        }
        let mut anlz: HashSet<String> = HashSet::new();
        for (dir, n) in slots {
            for i in 0..n {
                for ext in ANLZ_EXTS {
                    anlz.insert(fsname::fold_key(&format!("{dir}/ANLZ{i:04}.{ext}")));
                }
            }
        }
        let mut out: Vec<&StickFile> = self
            .contents
            .iter()
            .filter(|f| !audio.contains(&fsname::fold_key(&f.rel)))
            .chain(
                self.anlz
                    .iter()
                    .filter(|f| !anlz.contains(&fsname::fold_key(&f.rel))),
            )
            .collect();
        if !artwork {
            out.extend(self.artwork.iter());
        }
        out
    }
}

/// The `USBANLZ` directory players derive from a stick audio path: a hash of
/// its UTF-16 units modulo 200003, with `P` built from 7 bits of it. Players
/// compute this themselves rather than trusting `analyze_path` (R§4; the
/// algorithm as documented there, checked by baken against a rekordbox
/// export).
pub fn anlz_dir(usb_path: &str) -> String {
    let mut h: u32 = 0;
    for unit in usb_path.encode_utf16() {
        let c = u32::from(unit);
        h = h.wrapping_mul(0x5BC9).wrapping_add(c);
        h = h.wrapping_mul(0x93B5).wrapping_add(c);
    }
    let r = h % 200_003;
    let p = (r & 1)
        | ((r >> 1) & 2)
        | ((r >> 4) & 4)
        | ((r >> 4) & 8)
        | ((r >> 5) & 0x10)
        | ((r >> 8) & 0x20)
        | ((r >> 10) & 0x40);
    format!("{USBANLZ}/P{p:03X}/{r:08X}")
}

/// Bytes of an ANLZ set: the `.DAT` plus `.EXT`/`.2EX` beside it, if present.
/// The `.DAT` has its `.EXT` and `.2EX` beside it.
pub(crate) fn has_full_set(dat: &Path) -> bool {
    ANLZ_EXTS
        .iter()
        .all(|ext| dat.with_extension(ext).is_file())
}

pub(crate) fn anlz_set_bytes(dat: &Path) -> u64 {
    ANLZ_EXTS
        .iter()
        .filter_map(|ext| std::fs::metadata(dat.with_extension(ext)).ok())
        .map(|m| m.len())
        .sum()
}
