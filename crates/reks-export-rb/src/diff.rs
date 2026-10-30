//! Structural diff of two stick roots (Phase 2 task 11, `reks rb diff`):
//! our export against a rekordbox reference export, or any two exports.
//!
//! [`diff`] compares, side "ours" against side "theirs":
//!
//! - `export.pdb`: presence, the table types in the header and the present
//!   rows per table; damage [`validate::read`] finds on one side only.
//! - Track rows, matched on `file_path`: every field of the row (crate-digger
//!   `track_row`, unknown fields named by offset), with artist, album, genre,
//!   key, label, colour and artwork ids resolved to their names, so row ids
//!   that differ between writers don't show up as differences.
//! - The playlist tree (`Folder/Name` paths, folder or playlist, sibling
//!   order) and each playlist's entries in order, as `file_path`s.
//! - For each matched track, its `.DAT`, `.EXT` and `.2EX` (beside
//!   `analyze_path`): presence, `PPTH`, the section tags in order, and per
//!   section its lengths and its count where the tag has one (`PQTZ` beats,
//!   `PCOB`/`PCO2` cues, waveform entries, `PSSI` phrases).
//! - A byte summary of every file under `PIONEER/` ([`FileDiff`]); ANLZ
//!   files of matched tracks are paired by track, everything else by path.
//!
//! Read-only on both sides. The PDB is read with [`crate::pdb`]; row fields
//! are decoded from the raw row bytes at crate-digger offsets, so the diff
//! sees what is on disk. ANLZ goes through [`reks_anlz::read`] (D8).
//! `exportExt.pdb` is only byte-compared: its table types differ from
//! `export.pdb`'s and its rows are undecoded.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use reks_anlz::read as anlz;
use serde::Serialize;

use crate::pdb::rows::{self as pdb_rows, STR_ANALYZE_PATH, STR_FILE_PATH, TRACK_STRINGS};
use crate::pdb::{self, u16_at, u32_at};
use crate::validate::{self, locate, Finding, StickPlaylist, ValidateError, PDB_PATH};

/// ANLZ files compared per track, beside `analyze_path` (`.DAT`).
pub const ANLZ_EXTENSIONS: [&str; 3] = ["DAT", "EXT", "2EX"];

#[derive(Debug, thiserror::Error)]
pub enum DiffError {
    #[error("cannot read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(transparent)]
    Validate(#[from] ValidateError),
}

/// Which stick a difference is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Side {
    Ours,
    Theirs,
}

impl fmt::Display for Side {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Side::Ours => "ours",
            Side::Theirs => "theirs",
        })
    }
}

/// One structural difference between the sticks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Difference {
    /// One side has no `export.pdb`.
    PdbMissing { side: Side },
    /// One side's `export.pdb` does not parse (header or a page chain).
    PdbUnreadable { side: Side, message: String },
    /// Damage [`validate::read`] reports on this side only.
    Damage { side: Side, finding: Finding },
    /// A table type only this side's header lists.
    TableOnlyIn {
        side: Side,
        table: u32,
        name: &'static str,
    },
    /// Present rows of a table.
    RowCount {
        table: u32,
        name: &'static str,
        ours: usize,
        theirs: usize,
    },
    /// A track row (by `file_path`) only this side has.
    TrackOnlyIn { side: Side, file_path: String },
    /// A field of a track row both sides have.
    TrackField {
        file_path: String,
        field: &'static str,
        ours: String,
        theirs: String,
    },
    /// A folder or playlist (`Folder/Name`) only this side has.
    PlaylistOnlyIn {
        side: Side,
        path: String,
        is_folder: bool,
    },
    /// Folder on one side, playlist on the other.
    PlaylistKind { path: String, ours_folder: bool },
    /// Siblings under `parent` (empty at the root) both sides have, in
    /// another order.
    PlaylistOrder {
        parent: String,
        ours: Vec<String>,
        theirs: Vec<String>,
    },
    /// A playlist's entries (`file_path`s, `#id` for a missing track row).
    PlaylistEntries {
        path: String,
        ours: Vec<String>,
        theirs: Vec<String>,
    },
    /// The ANLZ file `ext` of a matched track is only on this side; `path`
    /// is where it is.
    AnlzOnlyIn {
        side: Side,
        file_path: String,
        ext: &'static str,
        path: String,
    },
    /// The ANLZ file does not parse on this side.
    AnlzUnreadable {
        side: Side,
        file_path: String,
        ext: &'static str,
        message: String,
    },
    /// `PPTH` differs.
    AnlzPath {
        file_path: String,
        ext: &'static str,
        ours: Option<String>,
        theirs: Option<String>,
    },
    /// The section tags, in file order, differ.
    AnlzTags {
        file_path: String,
        ext: &'static str,
        ours: Vec<String>,
        theirs: Vec<String>,
    },
    /// A value of the `index`th section with `tag` on both sides: `field`
    /// is `lenHeader`, `lenTag` or `count` ([`SectionInfo`]).
    AnlzSection {
        file_path: String,
        ext: &'static str,
        tag: String,
        index: usize,
        field: &'static str,
        ours: Option<u32>,
        theirs: Option<u32>,
    },
}

/// One file's place on one stick.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSide {
    /// Stick path (`/PIONEER/...`).
    pub path: String,
    pub len: u64,
}

/// Raw byte comparison of one file (or one file pair).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    pub ours: Option<FileSide>,
    pub theirs: Option<FileSide>,
    /// First differing byte offset; a length difference counts from the
    /// shorter file's end. `None` when equal or on one side only.
    pub first_difference: Option<u64>,
    /// Bytes that differ over the common length plus the length difference.
    pub differing_bytes: u64,
}

impl FileDiff {
    pub fn is_same(&self) -> bool {
        self.ours.is_some() && self.theirs.is_some() && self.differing_bytes == 0
    }
}

/// What [`diff`] found.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffReport {
    pub differences: Vec<Difference>,
    /// Every file under `PIONEER/` on either side.
    pub files: Vec<FileDiff>,
}

impl DiffReport {
    /// No structural difference and every file byte-identical.
    pub fn is_same(&self) -> bool {
        self.differences.is_empty() && self.files.iter().all(FileDiff::is_same)
    }
}

/// One ANLZ section as the diff sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionInfo {
    pub tag: String,
    pub len_header: u32,
    /// The whole section, header included.
    pub len_tag: u32,
    /// `PQTZ` beats, `PCOB`/`PCO2` cues, `PWAV`/`PWV2` preview bytes,
    /// `PWV3`–`PWV7` entries, `PSSI` phrases; `None` for other tags.
    pub count: Option<u32>,
}

/// The sections of an ANLZ file, with their counts.
pub fn sections(bytes: &[u8]) -> Result<Vec<SectionInfo>, anlz::ReadError> {
    Ok(anlz::sections(bytes)?
        .iter()
        .map(|s| SectionInfo {
            tag: s.tag_str(),
            len_header: clamp_u32(s.len_header),
            len_tag: clamp_u32(s.bytes.len()),
            count: match &s.tag {
                b"PQTZ" => s.u32(20).ok(),
                b"PCOB" => s.u16(18).ok().map(u32::from),
                b"PCO2" | b"PSSI" => s.u16(16).ok().map(u32::from),
                b"PWAV" | b"PWV2" => s.u32(12).ok(),
                b"PWV3" | b"PWV4" | b"PWV5" | b"PWV6" | b"PWV7" => s.u32(16).ok(),
                _ => None,
            },
        })
        .collect())
}

/// Compares the sticks at `ours` and `theirs` (their roots). `Err` only for
/// I/O that stops the comparison; anything unreadable on a stick is a
/// [`Difference`].
pub fn diff(ours: &Path, theirs: &Path) -> Result<DiffReport, DiffError> {
    let a = Stick::read(ours, Side::Ours)?;
    let b = Stick::read(theirs, Side::Theirs)?;
    let mut out = Vec::new();
    diff_pdb(&a, &b, &mut out);
    diff_findings(&a, &b, &mut out);
    let matched = diff_tracks(&a, &b, &mut out);
    diff_playlists(&a, &b, &mut out);
    let mut files = Vec::new();
    let mut used = (HashSet::new(), HashSet::new());
    for (file_path, ta, tb) in matched {
        diff_anlz(&a, &b, file_path, ta, tb, &mut out, &mut files, &mut used)?;
    }
    diff_other_files(&a, &b, &used, &mut files)?;
    Ok(DiffReport {
        differences: out,
        files,
    })
}

// ---------------------------------------------------------------------------
// Reading one stick

struct Stick {
    root: PathBuf,
    side: Side,
    pdb: Pdb,
    /// From [`validate::read`]: the playlist tree and damage findings.
    read: validate::ValidationReport,
}

enum Pdb {
    Missing,
    Unreadable(String),
    Read {
        /// Header table types, in header order.
        tables: Vec<u32>,
        /// Present rows per table type.
        rows: BTreeMap<u32, usize>,
        /// Track rows in file order.
        tracks: Vec<TrackRow>,
    },
}

struct TrackRow {
    id: u32,
    file_path: String,
    analyze_path: String,
    fields: Vec<(&'static str, String)>,
}

impl Stick {
    fn read(root: &Path, side: Side) -> Result<Self, DiffError> {
        let pdb = match locate(root, PDB_PATH).map_err(io_err(root))? {
            None => Pdb::Missing,
            Some(path) => {
                let bytes = fs::read(&path).map_err(io_err(&path))?;
                read_pdb(&bytes).unwrap_or_else(Pdb::Unreadable)
            }
        };
        Ok(Stick {
            root: root.to_path_buf(),
            side,
            pdb,
            read: validate::read(root)?,
        })
    }

    fn tracks(&self) -> &[TrackRow] {
        match &self.pdb {
            Pdb::Read { tracks, .. } => tracks,
            _ => &[],
        }
    }
}

/// Row bytes of the tables whose rows we decode.
#[derive(Default)]
struct RawTables {
    tracks: Vec<Vec<u8>>,
    names: HashMap<u32, Vec<Vec<u8>>>,
}

fn read_pdb(bytes: &[u8]) -> Result<Pdb, String> {
    let file = pdb::Pdb::parse(bytes).map_err(|e| e.to_string())?;
    let tables: Vec<u32> = file.tables().iter().map(|t| t.table.code()).collect();
    let mut rows: BTreeMap<u32, usize> = tables.iter().map(|&t| (t, 0)).collect();
    let mut raw = RawTables::default();
    for t in file.tables() {
        let code = t.table.code();
        let present = file.rows(t).map_err(|e| e.to_string())?;
        *rows.entry(code).or_default() += present.len();
        let present = present.into_iter().map(<[u8]>::to_vec);
        match code {
            TRACKS => raw.tracks.extend(present),
            GENRES | ARTISTS | ALBUMS | LABELS | KEYS | COLORS | ARTWORK => {
                raw.names.entry(code).or_default().extend(present)
            }
            _ => {}
        }
    }
    let names = Names::decode(&raw.names);
    let tracks = raw
        .tracks
        .iter()
        .filter_map(|row| decode_track(row, &names))
        .collect();
    Ok(Pdb::Read {
        tables,
        rows,
        tracks,
    })
}

const TRACKS: u32 = 0;
const GENRES: u32 = 1;
const ARTISTS: u32 = 2;
const ALBUMS: u32 = 3;
const LABELS: u32 = 4;
const KEYS: u32 = 5;
const COLORS: u32 = 6;
const ARTWORK: u32 = 13;

/// `export.pdb` table names (crate-digger `page_type`).
pub fn table_name(table: u32) -> &'static str {
    match table {
        0 => "tracks",
        1 => "genres",
        2 => "artists",
        3 => "albums",
        4 => "labels",
        5 => "keys",
        6 => "colors",
        7 => "playlist_tree",
        8 => "playlist_entries",
        11 => "history_playlists",
        12 => "history_entries",
        13 => "artwork",
        16 => "columns",
        19 => "history",
        _ => "unknown",
    }
}

/// Names of the rows track rows point at, by table and id.
struct Names(HashMap<u32, HashMap<u32, String>>);

impl Names {
    /// Row layouts from crate-digger `pdb.ksy`. Rows that don't decode are
    /// left out; a track pointing at one shows `#id (no row)`.
    fn decode(raw: &HashMap<u32, Vec<Vec<u8>>>) -> Self {
        let mut out: HashMap<u32, HashMap<u32, String>> = HashMap::new();
        for (&table, rows) in raw {
            let names = out.entry(table).or_default();
            for r in rows {
                let decoded = match table {
                    GENRES | LABELS | ARTWORK => pdb_rows::named(r, 4),
                    KEYS => pdb_rows::named(r, 8),
                    COLORS => pdb_rows::color(r),
                    ARTISTS => pdb_rows::artist(r),
                    ALBUMS => pdb_rows::album(r).map(|(id, a)| (id, a.name)),
                    _ => continue,
                };
                if let Ok((id, name)) = decoded {
                    names.entry(id).or_insert(name);
                }
            }
        }
        Names(out)
    }

    fn resolve(&self, table: u32, id: u32) -> String {
        if id == 0 {
            return String::new();
        }
        match self.0.get(&table).and_then(|m| m.get(&id)) {
            Some(name) => name.clone(),
            None => format!("#{id} (no row)"),
        }
    }
}

/// A track row field: a number at an offset, or an id into another table.
enum Field {
    U8(usize),
    U16(usize),
    U32(usize),
    Ref8(usize, u32),
    Ref32(usize, u32),
}

/// `track_row` (crate-digger) up to the string offsets at 0x5E. `id` (0x48)
/// is left out: it is the writer's row number.
const TRACK_FIELDS: &[(&str, Field)] = &[
    ("subtype", Field::U16(0x00)),
    ("index_shift", Field::U16(0x02)),
    ("bitmask", Field::U32(0x04)),
    ("sample_rate", Field::U32(0x08)),
    ("composer", Field::Ref32(0x0C, ARTISTS)),
    ("file_size", Field::U32(0x10)),
    ("unknown_0x14", Field::U32(0x14)),
    ("unknown_0x18", Field::U16(0x18)),
    ("unknown_0x1a", Field::U16(0x1A)),
    ("artwork", Field::Ref32(0x1C, ARTWORK)),
    ("key", Field::Ref32(0x20, KEYS)),
    ("original_artist", Field::Ref32(0x24, ARTISTS)),
    ("label", Field::Ref32(0x28, LABELS)),
    ("remixer", Field::Ref32(0x2C, ARTISTS)),
    ("bitrate", Field::U32(0x30)),
    ("track_number", Field::U32(0x34)),
    ("tempo", Field::U32(0x38)),
    ("genre", Field::Ref32(0x3C, GENRES)),
    ("album", Field::Ref32(0x40, ALBUMS)),
    ("artist", Field::Ref32(0x44, ARTISTS)),
    ("disc_number", Field::U16(0x4C)),
    ("play_count", Field::U16(0x4E)),
    ("year", Field::U16(0x50)),
    ("sample_depth", Field::U16(0x52)),
    ("duration", Field::U16(0x54)),
    ("unknown_0x56", Field::U16(0x56)),
    ("color", Field::Ref8(0x58, COLORS)),
    ("rating", Field::U8(0x59)),
    ("file_type", Field::U16(0x5A)),
    ("unknown_0x5c", Field::U16(0x5C)),
];

/// `None` when the row is too short or its `file_path` does not decode
/// ([`validate::read`] reports those rows as damage).
fn decode_track(row: &[u8], names: &Names) -> Option<TrackRow> {
    let mut fields = Vec::with_capacity(TRACK_FIELDS.len() + TRACK_STRINGS.len());
    for (name, field) in TRACK_FIELDS {
        let value = match *field {
            Field::U8(at) => row.get(at).map(|v| v.to_string()),
            Field::U16(at) => u16_at(row, at).map(|v| v.to_string()),
            Field::U32(at) => u32_at(row, at).map(|v| v.to_string()),
            Field::Ref8(at, table) => row.get(at).map(|&v| names.resolve(table, v.into())),
            Field::Ref32(at, table) => u32_at(row, at).map(|v| names.resolve(table, v)),
        }?;
        fields.push((*name, value));
    }
    // The whole offset table must be there.
    u16_at(row, 0x5E + 2 * (TRACK_STRINGS.len() - 1))?;
    let strings: Vec<Result<String, String>> = (0..TRACK_STRINGS.len())
        .map(|i| pdb_rows::track_string(row, i).map_err(|e| e.to_string()))
        .collect();
    let file_path = strings[STR_FILE_PATH].clone().ok()?;
    let analyze_path = strings[STR_ANALYZE_PATH].clone().unwrap_or_default();
    for (name, s) in TRACK_STRINGS.iter().zip(strings) {
        fields.push((*name, s.unwrap_or_else(|e| format!("<undecodable: {e}>"))));
    }
    Some(TrackRow {
        id: u32_at(row, 0x48)?,
        file_path,
        analyze_path,
        fields,
    })
}

fn clamp_u32(v: usize) -> u32 {
    u32::try_from(v).unwrap_or(u32::MAX)
}

// ---------------------------------------------------------------------------
// Comparing

fn diff_pdb(a: &Stick, b: &Stick, out: &mut Vec<Difference>) {
    let (ta, ra) = match &a.pdb {
        Pdb::Read { tables, rows, .. } => (tables, rows),
        _ => {
            side_problem(a, b, out);
            return;
        }
    };
    let (tb, rb) = match &b.pdb {
        Pdb::Read { tables, rows, .. } => (tables, rows),
        _ => {
            side_problem(a, b, out);
            return;
        }
    };
    for (stick, mine, other) in [(a, ta, tb), (b, tb, ta)] {
        for &t in mine.iter().filter(|t| !other.contains(t)) {
            out.push(Difference::TableOnlyIn {
                side: stick.side,
                table: t,
                name: table_name(t),
            });
        }
    }
    for (&t, &ours) in ra {
        if let Some(&theirs) = rb.get(&t) {
            if ours != theirs {
                out.push(Difference::RowCount {
                    table: t,
                    name: table_name(t),
                    ours,
                    theirs,
                });
            }
        }
    }
}

/// Missing or unreadable PDBs, when they differ between the sides.
fn side_problem(a: &Stick, b: &Stick, out: &mut Vec<Difference>) {
    let problem = |s: &Stick| match &s.pdb {
        Pdb::Missing => Some(Difference::PdbMissing { side: s.side }),
        Pdb::Unreadable(message) => Some(Difference::PdbUnreadable {
            side: s.side,
            message: message.clone(),
        }),
        Pdb::Read { .. } => None,
    };
    let (pa, pb) = (problem(a), problem(b));
    let same = match (&a.pdb, &b.pdb) {
        (Pdb::Missing, Pdb::Missing) => true,
        (Pdb::Unreadable(x), Pdb::Unreadable(y)) => x == y,
        _ => false,
    };
    if !same {
        out.extend(pa);
        out.extend(pb);
    }
}

fn diff_findings(a: &Stick, b: &Stick, out: &mut Vec<Difference>) {
    let (fa, fb) = (damage(a), damage(b));
    for (stick, mine, other) in [(a, &fa, &fb), (b, &fb, &fa)] {
        let mut other: Vec<&Finding> = other.to_vec();
        for f in mine {
            match other.iter().position(|o| o == f) {
                Some(i) => {
                    other.swap_remove(i);
                }
                None => out.push(Difference::Damage {
                    side: stick.side,
                    finding: (*f).clone(),
                }),
            }
        }
    }
}

/// [`validate::read`]'s findings, without the ones our own PDB read reports
/// (a missing or unusable database).
fn damage(s: &Stick) -> Vec<&Finding> {
    s.read
        .findings
        .iter()
        .filter(|f| match f {
            Finding::PdbMissing => false,
            Finding::PdbUnreadable { .. } => matches!(s.pdb, Pdb::Read { .. }),
            _ => true,
        })
        .collect()
}

/// Track rows matched on `file_path`, in our row order.
type Matched<'a> = Vec<(&'a str, &'a TrackRow, &'a TrackRow)>;

fn diff_tracks<'a>(a: &'a Stick, b: &'a Stick, out: &mut Vec<Difference>) -> Matched<'a> {
    let by_path = |s: &'a Stick| {
        let mut m: HashMap<&'a str, &'a TrackRow> = HashMap::new();
        for t in s.tracks() {
            m.entry(t.file_path.as_str()).or_insert(t);
        }
        m
    };
    let (ma, mb) = (by_path(a), by_path(b));
    let mut matched = Vec::new();
    let mut seen = HashSet::new();
    for t in a.tracks() {
        if !seen.insert(t.file_path.as_str()) {
            continue;
        }
        let Some(other) = mb.get(t.file_path.as_str()) else {
            out.push(Difference::TrackOnlyIn {
                side: Side::Ours,
                file_path: t.file_path.clone(),
            });
            continue;
        };
        for ((field, ours), (_, theirs)) in t.fields.iter().zip(&other.fields) {
            if ours != theirs {
                out.push(Difference::TrackField {
                    file_path: t.file_path.clone(),
                    field,
                    ours: ours.clone(),
                    theirs: theirs.clone(),
                });
            }
        }
        matched.push((t.file_path.as_str(), t, *other));
    }
    let mut seen = HashSet::new();
    for t in b.tracks() {
        if !ma.contains_key(t.file_path.as_str()) && seen.insert(t.file_path.as_str()) {
            out.push(Difference::TrackOnlyIn {
                side: Side::Theirs,
                file_path: t.file_path.clone(),
            });
        }
    }
    matched
}

/// A stick's playlist tree by `Folder/Name` path.
struct Tree {
    /// Path → (is_folder, entries as `file_path`s).
    nodes: BTreeMap<String, (bool, Vec<String>)>,
    /// Parent path → child names in sibling order.
    children: BTreeMap<String, Vec<String>>,
}

fn tree(s: &Stick) -> Tree {
    let paths: HashMap<u32, &str> = s
        .tracks()
        .iter()
        .map(|t| (t.id, t.file_path.as_str()))
        .collect();
    let by_id: HashMap<u32, &StickPlaylist> =
        s.read.playlists.iter().map(|p| (p.pdb_id, p)).collect();
    let mut kids: BTreeMap<u32, Vec<&StickPlaylist>> = BTreeMap::new();
    for p in &s.read.playlists {
        kids.entry(p.parent_id).or_default().push(p);
    }
    for list in kids.values_mut() {
        list.sort_by_key(|p| (p.sort_order, p.pdb_id));
    }
    let mut t = Tree {
        nodes: BTreeMap::new(),
        children: BTreeMap::new(),
    };
    // Roots: parent 0, or a parent that is not in the table (orphans keep
    // their parent id in the path so they don't merge into the root).
    let mut stack: Vec<(u32, String)> = kids
        .keys()
        .filter(|&&parent| parent == 0 || !by_id.contains_key(&parent))
        .map(|&parent| {
            let prefix = if parent == 0 {
                String::new()
            } else {
                format!("#{parent}")
            };
            (parent, prefix)
        })
        .collect();
    let mut visited = HashSet::new();
    while let Some((parent, prefix)) = stack.pop() {
        if !visited.insert(parent) {
            continue; // a cycle in parent ids
        }
        let mut names = Vec::new();
        for p in kids.get(&parent).map(Vec::as_slice).unwrap_or_default() {
            let mut name = p.name.clone();
            let mut n = 2;
            while names.contains(&name) {
                name = format!("{} ({n})", p.name);
                n += 1;
            }
            let path = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            let entries = p
                .entries
                .iter()
                .map(|id| {
                    paths
                        .get(id)
                        .map_or_else(|| format!("#{id}"), |s| s.to_string())
                })
                .collect();
            t.nodes.insert(path.clone(), (p.is_folder, entries));
            names.push(name);
            stack.push((p.pdb_id, path));
        }
        t.children.insert(prefix, names);
    }
    t
}

fn diff_playlists(a: &Stick, b: &Stick, out: &mut Vec<Difference>) {
    let (ta, tb) = (tree(a), tree(b));
    for (path, (folder, entries)) in &ta.nodes {
        match tb.nodes.get(path) {
            None => out.push(Difference::PlaylistOnlyIn {
                side: Side::Ours,
                path: path.clone(),
                is_folder: *folder,
            }),
            Some((other_folder, other_entries)) => {
                if folder != other_folder {
                    out.push(Difference::PlaylistKind {
                        path: path.clone(),
                        ours_folder: *folder,
                    });
                } else if entries != other_entries {
                    out.push(Difference::PlaylistEntries {
                        path: path.clone(),
                        ours: entries.clone(),
                        theirs: other_entries.clone(),
                    });
                }
            }
        }
    }
    for (path, (folder, _)) in &tb.nodes {
        if !ta.nodes.contains_key(path) {
            out.push(Difference::PlaylistOnlyIn {
                side: Side::Theirs,
                path: path.clone(),
                is_folder: *folder,
            });
        }
    }
    for (parent, names) in &ta.children {
        let Some(other) = tb.children.get(parent) else {
            continue;
        };
        let ours: Vec<String> = names
            .iter()
            .filter(|n| other.contains(n))
            .cloned()
            .collect();
        let theirs: Vec<String> = other
            .iter()
            .filter(|n| names.contains(n))
            .cloned()
            .collect();
        if ours != theirs {
            out.push(Difference::PlaylistOrder {
                parent: parent.clone(),
                ours,
                theirs,
            });
        }
    }
}

/// Stick paths already paired for the byte summary, lowercased (ours, theirs).
type Used = (HashSet<String>, HashSet<String>);

#[allow(clippy::too_many_arguments)]
fn diff_anlz(
    a: &Stick,
    b: &Stick,
    file_path: &str,
    ta: &TrackRow,
    tb: &TrackRow,
    out: &mut Vec<Difference>,
    files: &mut Vec<FileDiff>,
    used: &mut Used,
) -> Result<(), DiffError> {
    for ext in ANLZ_EXTENSIONS {
        let fa = anlz_file(a, &ta.analyze_path, ext)?;
        let fb = anlz_file(b, &tb.analyze_path, ext)?;
        if fa.is_none() && fb.is_none() {
            continue;
        }
        for (f, set) in [(&fa, &mut used.0), (&fb, &mut used.1)] {
            if let Some((rel, _)) = f {
                set.insert(rel.to_ascii_lowercase());
            }
        }
        files.push(byte_diff(
            fa.as_ref().map(|(p, b)| (p.as_str(), b.as_slice())),
            fb.as_ref().map(|(p, b)| (p.as_str(), b.as_slice())),
        ));
        let (ours, theirs) = match (fa, fb) {
            (Some(x), Some(y)) => (x, y),
            (x, y) => {
                let (side, (path, _)) = match (x, y) {
                    (Some(x), None) => (Side::Ours, x),
                    (None, Some(y)) => (Side::Theirs, y),
                    _ => continue,
                };
                out.push(Difference::AnlzOnlyIn {
                    side,
                    file_path: file_path.to_owned(),
                    ext,
                    path,
                });
                continue;
            }
        };
        let read = |side: Side, bytes: &[u8], out: &mut Vec<Difference>| {
            let parsed = sections(bytes).and_then(|s| Ok((s, anlz::summary(bytes)?.path)));
            parsed
                .map_err(|e| {
                    out.push(Difference::AnlzUnreadable {
                        side,
                        file_path: file_path.to_owned(),
                        ext,
                        message: e.to_string(),
                    })
                })
                .ok()
        };
        let (Some((sa, pa)), Some((sb, pb))) = (
            read(Side::Ours, &ours.1, out),
            read(Side::Theirs, &theirs.1, out),
        ) else {
            continue;
        };
        if pa != pb {
            out.push(Difference::AnlzPath {
                file_path: file_path.to_owned(),
                ext,
                ours: pa,
                theirs: pb,
            });
        }
        diff_sections(file_path, ext, &sa, &sb, out);
    }
    Ok(())
}

fn diff_sections(
    file_path: &str,
    ext: &'static str,
    a: &[SectionInfo],
    b: &[SectionInfo],
    out: &mut Vec<Difference>,
) {
    let tags = |s: &[SectionInfo]| s.iter().map(|s| s.tag.clone()).collect::<Vec<_>>();
    if tags(a) != tags(b) {
        out.push(Difference::AnlzTags {
            file_path: file_path.to_owned(),
            ext,
            ours: tags(a),
            theirs: tags(b),
        });
    }
    // Pair the n-th section of each tag.
    let mut index: HashMap<&str, usize> = HashMap::new();
    for s in a {
        let i = index.entry(&s.tag).or_default();
        let n = *i;
        *i += 1;
        let Some(t) = b.iter().filter(|t| t.tag == s.tag).nth(n) else {
            continue;
        };
        let values = [
            ("lenHeader", Some(s.len_header), Some(t.len_header)),
            ("lenTag", Some(s.len_tag), Some(t.len_tag)),
            ("count", s.count, t.count),
        ];
        for (field, ours, theirs) in values {
            if ours != theirs {
                out.push(Difference::AnlzSection {
                    file_path: file_path.to_owned(),
                    ext,
                    tag: s.tag.clone(),
                    index: n,
                    field,
                    ours,
                    theirs,
                });
            }
        }
    }
}

/// The ANLZ file `ext` beside `analyze_path` (stick path, found ignoring
/// case) and its bytes.
fn anlz_file(
    s: &Stick,
    analyze_path: &str,
    ext: &str,
) -> Result<Option<(String, Vec<u8>)>, DiffError> {
    let Some((stem, _)) = analyze_path.rsplit_once('.') else {
        return Ok(None);
    };
    let rel = format!("{stem}.{ext}");
    let Some(path) = locate(&s.root, &rel).map_err(io_err(&s.root))? else {
        return Ok(None);
    };
    let bytes = fs::read(&path).map_err(io_err(&path))?;
    Ok(Some((stick_path(&s.root, &path), bytes)))
}

/// Byte summary of every `PIONEER/` file not paired by track, paired by
/// path (ignoring ASCII case).
fn diff_other_files(
    a: &Stick,
    b: &Stick,
    used: &Used,
    files: &mut Vec<FileDiff>,
) -> Result<(), DiffError> {
    let list =
        |s: &Stick, used: &HashSet<String>| -> Result<BTreeMap<String, PathBuf>, DiffError> {
            let mut found = Vec::new();
            if let Some(dir) = find_dir(&s.root, "PIONEER")? {
                walk(&dir, &mut found)?;
            }
            Ok(found
                .into_iter()
                .map(|p| (stick_path(&s.root, &p), p))
                .filter(|(rel, _)| !used.contains(&rel.to_ascii_lowercase()))
                .map(|(rel, p)| (rel.to_ascii_lowercase(), p))
                .collect())
        };
    let (la, lb) = (list(a, &used.0)?, list(b, &used.1)?);
    let keys: std::collections::BTreeSet<&String> = la.keys().chain(lb.keys()).collect();
    for key in keys {
        let read =
            |s: &Stick, p: Option<&PathBuf>| -> Result<Option<(String, Vec<u8>)>, DiffError> {
                p.map(|p| Ok((stick_path(&s.root, p), fs::read(p).map_err(io_err(p))?)))
                    .transpose()
            };
        let fa = read(a, la.get(key))?;
        let fb = read(b, lb.get(key))?;
        files.push(byte_diff(
            fa.as_ref().map(|(p, b)| (p.as_str(), b.as_slice())),
            fb.as_ref().map(|(p, b)| (p.as_str(), b.as_slice())),
        ));
    }
    Ok(())
}

fn byte_diff(a: Option<(&str, &[u8])>, b: Option<(&str, &[u8])>) -> FileDiff {
    let side = |f: Option<(&str, &[u8])>| {
        f.map(|(path, bytes)| FileSide {
            path: path.to_owned(),
            len: bytes.len() as u64,
        })
    };
    let (first_difference, differing_bytes) = match (a, b) {
        (Some((_, x)), Some((_, y))) => {
            let common = x.len().min(y.len());
            let mut first = None;
            let mut n = 0u64;
            for (i, (p, q)) in x.iter().zip(y).enumerate() {
                if p != q {
                    first.get_or_insert(i as u64);
                    n += 1;
                }
            }
            let extra = x.len().abs_diff(y.len()) as u64;
            if extra > 0 {
                first.get_or_insert(common as u64);
            }
            (first, n + extra)
        }
        _ => (None, 0),
    };
    FileDiff {
        ours: side(a),
        theirs: side(b),
        first_difference,
        differing_bytes,
    }
}

fn find_dir(root: &Path, name: &str) -> Result<Option<PathBuf>, DiffError> {
    let entries = match fs::read_dir(root) {
        Ok(e) => e,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io_err(root)(e)),
    };
    for entry in entries {
        let entry = entry.map_err(io_err(root))?;
        if entry
            .file_name()
            .to_string_lossy()
            .eq_ignore_ascii_case(name)
            && entry.file_type().map_err(io_err(root))?.is_dir()
        {
            return Ok(Some(entry.path()));
        }
    }
    Ok(None)
}

/// Every file below `dir` (symlinks are not followed).
fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), DiffError> {
    for entry in fs::read_dir(dir).map_err(io_err(dir))? {
        let entry = entry.map_err(io_err(dir))?;
        let kind = entry.file_type().map_err(io_err(dir))?;
        if kind.is_dir() {
            walk(&entry.path(), out)?;
        } else if kind.is_file() {
            out.push(entry.path());
        }
    }
    Ok(())
}

/// `/`-separated stick path of `path` under `root`.
fn stick_path(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    let mut s = String::new();
    for part in rel.components() {
        s.push('/');
        s.push_str(&part.as_os_str().to_string_lossy());
    }
    s
}

fn io_err(path: &Path) -> impl FnOnce(io::Error) -> DiffError + '_ {
    move |source| DiffError::Io {
        path: path.to_path_buf(),
        source,
    }
}

// ---------------------------------------------------------------------------
// Text output

impl fmt::Display for Difference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let opt = |v: &Option<u32>| v.map_or_else(|| "-".to_owned(), |v| v.to_string());
        match self {
            Difference::PdbMissing { side } => write!(f, "export.pdb missing ({side})"),
            Difference::PdbUnreadable { side, message } => {
                write!(f, "export.pdb unreadable ({side}): {message}")
            }
            Difference::Damage { side, finding } => write!(f, "damage ({side} only): {finding}"),
            Difference::TableOnlyIn { side, table, name } => {
                write!(f, "table {table} ({name}) only in {side}")
            }
            Difference::RowCount {
                table,
                name,
                ours,
                theirs,
            } => write!(
                f,
                "table {table} ({name}): {ours} row(s) ours, {theirs} theirs"
            ),
            Difference::TrackOnlyIn { side, file_path } => {
                write!(f, "track {file_path}: only in {side}")
            }
            Difference::TrackField {
                file_path,
                field,
                ours,
                theirs,
            } => write!(
                f,
                "track {file_path}: {field}: ours {ours:?}, theirs {theirs:?}"
            ),
            Difference::PlaylistOnlyIn {
                side,
                path,
                is_folder,
            } => {
                let kind = if *is_folder { "folder" } else { "playlist" };
                write!(f, "{kind} {path:?}: only in {side}")
            }
            Difference::PlaylistKind { path, ours_folder } => {
                let (o, t) = if *ours_folder {
                    ("folder", "playlist")
                } else {
                    ("playlist", "folder")
                };
                write!(f, "{path:?}: {o} ours, {t} theirs")
            }
            Difference::PlaylistOrder {
                parent,
                ours,
                theirs,
            } => write!(
                f,
                "order under {:?}: ours {ours:?}, theirs {theirs:?}",
                if parent.is_empty() { "/" } else { parent }
            ),
            Difference::PlaylistEntries { path, ours, theirs } => {
                let first = ours.iter().zip(theirs).take_while(|(x, y)| x == y).count();
                write!(
                    f,
                    "playlist {path:?}: {} entries ours, {} theirs, first difference at {first}",
                    ours.len(),
                    theirs.len()
                )
            }
            Difference::AnlzOnlyIn {
                side,
                file_path,
                ext,
                path,
            } => write!(f, "track {file_path}: .{ext} only in {side} ({path})"),
            Difference::AnlzUnreadable {
                side,
                file_path,
                ext,
                message,
            } => write!(
                f,
                "track {file_path}: .{ext} unreadable ({side}): {message}"
            ),
            Difference::AnlzPath {
                file_path,
                ext,
                ours,
                theirs,
            } => write!(
                f,
                "track {file_path}: .{ext} PPTH: ours {ours:?}, theirs {theirs:?}"
            ),
            Difference::AnlzTags {
                file_path,
                ext,
                ours,
                theirs,
            } => write!(
                f,
                "track {file_path}: .{ext} sections: ours {}, theirs {}",
                ours.join(" "),
                theirs.join(" ")
            ),
            Difference::AnlzSection {
                file_path,
                ext,
                tag,
                index,
                field,
                ours,
                theirs,
            } => {
                let nth = if *index == 0 {
                    String::new()
                } else {
                    format!("#{}", index + 1)
                };
                write!(
                    f,
                    "track {file_path}: .{ext} {tag}{nth} {field}: ours {}, theirs {}",
                    opt(ours),
                    opt(theirs)
                )
            }
        }
    }
}

impl fmt::Display for FileDiff {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.ours, &self.theirs) {
            (Some(a), Some(b)) => {
                let path = if a.path == b.path {
                    a.path.clone()
                } else {
                    format!("{} <> {}", a.path, b.path)
                };
                match self.first_difference {
                    None => write!(f, "same     {path} ({} B)", a.len),
                    Some(at) => write!(
                        f,
                        "differs  {path} ({} B ours, {} B theirs; {} byte(s) differ, first at 0x{at:X})",
                        a.len, b.len, self.differing_bytes
                    ),
                }
            }
            (Some(a), None) => write!(f, "ours     {} ({} B)", a.path, a.len),
            (None, Some(b)) => write!(f, "theirs   {} ({} B)", b.path, b.len),
            (None, None) => Ok(()),
        }
    }
}

impl fmt::Display for DiffReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for d in &self.differences {
            writeln!(f, "{d}")?;
        }
        let same = self.files.iter().filter(|x| x.is_same()).count();
        writeln!(
            f,
            "Files under PIONEER/: {} compared, {same} identical",
            self.files.len()
        )?;
        for file in self.files.iter().filter(|x| !x.is_same()) {
            writeln!(f, "  {file}")?;
        }
        if self.is_same() {
            writeln!(f, "No differences.")
        } else {
            writeln!(
                f,
                "{} structural difference(s), {} file(s) not identical.",
                self.differences.len(),
                self.files.len() - same
            )
        }
    }
}

#[cfg(test)]
mod tests;
