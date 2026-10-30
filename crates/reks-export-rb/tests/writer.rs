//! BakenWriter integration (Phase 2 task 7): plan a selection of generated
//! fixture audio, write it to a temp-dir "stick" with baken, and read the
//! result back with rekordcrate: every table present, one track row per
//! planned track, file paths equal to the plan's `dst_rel` (ASCII, D24) while
//! non-ASCII metadata stays UTF-16, ANLZ where the player looks with PPTH =
//! `dst_rel`. Plus what a refused, cancelled or failed write leaves behind.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::io::{Cursor, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use binrw::{BinRead, BinWrite, Endian};
use rekordcrate::pdb::string::DeviceSQLString;
use rekordcrate::pdb::{Header, PageType, Row};
use reks_core::rbxml::{self, ImportOptions, PrefixRemap};
use reks_core::{CancelToken, Library, PlaylistId};
use reks_export_rb::device::{DeviceInfo, FsKind};
use reks_export_rb::plan::{
    self, anlz_dir, AnlzSource, ExportOptions, ExportPlan, MissingAnalysis, NoAnlz, Selection,
    SourceTrack,
};
use reks_export_rb::writer::{
    BakenWriter, ExportEvent, RekordboxWriter, WriterError, WriterReport,
};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/audio");
const XML_PREFIX: &str = "/Users/reks/Music/reks writer";

/// (fixture, file name, title, artist, album)
const TRACKS: [(&str, &str, &str, &str, &str); 4] = [
    ("sine-click-128.mp3", "plain.mp3", "Plain", "Alpha", "One"),
    (
        "sine-click-128.flac",
        "Песня.flac",
        "Группа крови",
        "Кино",
        "Звезда",
    ),
    (
        "sine-click-128.aiff",
        "aiff.aiff",
        "Merry Christmas ✨",
        "坂本龍一",
        "",
    ),
    (
        "sine-click-128.wav",
        "Svefn-g-englar.wav",
        "Svefn-g-englar",
        "Sigur Rós",
        "Ágætis byrjun",
    ),
];

struct World {
    tmp: tempfile::TempDir,
    lib: Library,
    music: PathBuf,
    stick: PathBuf,
}

impl World {
    /// A library with [`TRACKS`] in playlists `Set/Peak` (1, 2, 3) and
    /// `Set/Chill` (4, 2), and an empty stick.
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let music = tmp.path().join("music");
        let stick = tmp.path().join("stick");
        fs::create_dir_all(&music).unwrap();
        fs::create_dir_all(&stick).unwrap();
        for (fixture, name, ..) in TRACKS {
            fs::copy(Path::new(FIXTURES).join(fixture), music.join(name)).unwrap();
        }
        let lib =
            Library::open(&tmp.path().join("reks.sqlite"), &tmp.path().join("cache")).unwrap();
        let c = rbxml::parse(Cursor::new(collection_xml())).unwrap();
        let opts = ImportOptions {
            remap: vec![PrefixRemap {
                from: XML_PREFIX.into(),
                to: music.clone(),
            }],
            ..ImportOptions::default()
        };
        rbxml::import(&c, &lib, opts, |_| {}).unwrap();
        World {
            tmp,
            lib,
            music,
            stick,
        }
    }

    fn node(&self, name: &str) -> PlaylistId {
        let tree = self.lib.playlists().unwrap();
        tree.nodes.iter().find(|n| n.name == name).unwrap().id
    }

    fn sel(&self, names: &[&str]) -> Selection {
        Selection {
            playlists: names.iter().map(|n| self.node(n)).collect(),
        }
    }

    fn dev(&self, stick: &Path) -> DeviceInfo {
        DeviceInfo {
            mount_point: stick.to_path_buf(),
            fs: FsKind::Fat32,
            fs_name: "msdos".into(),
            removable: true,
            total_bytes: 32 << 30,
            free_bytes: 16 << 30,
            label: "REKS".into(),
            os_name: "disk9s1".into(),
        }
    }

    fn plan_to(
        &self,
        sel: &Selection,
        stick: &Path,
        opts: &ExportOptions,
        anlz: &dyn plan::AnlzLocator,
    ) -> ExportPlan {
        let p = plan::plan(&self.lib, sel, &self.dev(stick), opts, anlz).unwrap();
        assert!(!p.is_blocked(), "{:?}", p.errors);
        p
    }

    fn plan(&self, sel: &Selection) -> ExportPlan {
        self.plan_to(sel, &self.stick, &generate(), &NoAnlz)
    }

    fn writer(&self, sel: &Selection, plan: &ExportPlan) -> BakenWriter {
        BakenWriter::prepare(&self.lib, sel, plan, &self.tmp.path().join("work")).unwrap()
    }

    fn write(&self, sel: &Selection, plan: &ExportPlan) -> (WriterReport, Vec<ExportEvent>) {
        self.write_to(sel, plan, &self.stick, &CancelToken::new())
            .unwrap()
    }

    fn write_to(
        &self,
        sel: &Selection,
        plan: &ExportPlan,
        stick: &Path,
        cancel: &CancelToken,
    ) -> Result<(WriterReport, Vec<ExportEvent>), WriterError> {
        let events = Mutex::new(Vec::new());
        let report = self.writer(sel, plan).write(
            plan,
            stick,
            &|e| events.lock().unwrap().push(e),
            cancel,
        )?;
        Ok((report, events.into_inner().unwrap()))
    }
}

fn generate() -> ExportOptions {
    ExportOptions {
        missing_analysis: MissingAnalysis::Generate,
        ..ExportOptions::default()
    }
}

/// Percent-encodes a file name for a `Location` URL.
fn url_name(name: &str) -> String {
    name.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

fn collection_xml() -> String {
    let prefix = XML_PREFIX.replace(' ', "%20");
    let mut tracks = String::new();
    for (i, (_, name, title, artist, album)) in TRACKS.iter().enumerate() {
        tracks += &format!(
            r#"<TRACK TrackID="{id}" Name="{title}" Artist="{artist}" Album="{album}" Genre="Techno" Kind="" TotalTime="30" AverageBpm="128.00" Tonality="Am" Location="file://localhost{prefix}/{loc}">
  <TEMPO Inizio="0.000" Bpm="128.00" Metro="4/4" Battito="1"/>
  <POSITION_MARK Name="Drop" Type="0" Start="7.500" Num="0" Red="230" Green="40" Blue="40"/>
</TRACK>
"#,
            id = i + 1,
            loc = url_name(name),
        );
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<DJ_PLAYLISTS Version="1.0.0">
<PRODUCT Name="rekordbox" Version="7.0.0" Company="AlphaTheta"/>
<COLLECTION Entries="4">
{tracks}</COLLECTION>
<PLAYLISTS>
<NODE Type="0" Name="ROOT" Count="1">
  <NODE Type="0" Name="Set" Count="2">
    <NODE Name="Peak" Type="1" KeyType="0" Entries="3"><TRACK Key="1"/><TRACK Key="2"/><TRACK Key="3"/></NODE>
    <NODE Name="Chill" Type="1" KeyType="0" Entries="2"><TRACK Key="4"/><TRACK Key="2"/></NODE>
  </NODE>
</NODE>
</PLAYLISTS>
</DJ_PLAYLISTS>
"#
    )
}

fn on_stick(stick: &Path, rel: &str) -> PathBuf {
    stick.join(rel.trim_start_matches('/'))
}

// --- export.pdb read-back -------------------------------------------------

/// A string field as rekordcrate decodes it, and whether it is stored as
/// UTF-16 (DeviceSQL long string, flags 0x90).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Str {
    text: String,
    utf16: bool,
}

/// rekordcrate 0.3.0 keeps row fields private, so a row is written back out
/// with its `BinWrite` and the strings are read at the offsets in it.
fn str_at(row: &[u8], offset: usize) -> Str {
    let flags = row[offset];
    let mut c = Cursor::new(row);
    c.seek(SeekFrom::Start(offset as u64)).unwrap();
    let s = DeviceSQLString::read_options(&mut c, Endian::Little, ()).unwrap();
    Str {
        text: s.into_string().unwrap(),
        utf16: flags == 0x90,
    }
}

fn row_bytes<T: for<'a> BinWrite<Args<'a> = ()>>(row: &T) -> Vec<u8> {
    let mut c = Cursor::new(Vec::new());
    row.write_options(&mut c, Endian::Little, ()).unwrap();
    c.into_inner()
}

fn u16_at(b: &[u8], at: usize) -> usize {
    usize::from(u16::from_le_bytes([b[at], b[at + 1]]))
}

/// Track row strings by index (Deep Symmetry's track row: 21 offsets at 0x5E).
fn track_str(row: &[u8], index: usize) -> Str {
    str_at(row, u16_at(row, 0x5E + 2 * index))
}
const TITLE: usize = 17;
const ANALYZE_PATH: usize = 14;
const FILENAME: usize = 19;
const FILE_PATH: usize = 20;
/// `file_type` (u16 at 0x5A; rekordcrate's `unknown6`): Deep Symmetry's codes.
const FILE_TYPE_AT: usize = 0x5A;

fn page_type_code(t: &PageType) -> u32 {
    match t {
        PageType::Tracks => 0,
        PageType::Genres => 1,
        PageType::Artists => 2,
        PageType::Albums => 3,
        PageType::Labels => 4,
        PageType::Keys => 5,
        PageType::Colors => 6,
        PageType::PlaylistTree => 7,
        PageType::PlaylistEntries => 8,
        PageType::HistoryPlaylists => 11,
        PageType::HistoryEntries => 12,
        PageType::Artwork => 13,
        PageType::Columns => 16,
        PageType::History => 19,
        PageType::Unknown(n) => *n,
    }
}

struct Pdb {
    table_types: BTreeSet<u32>,
    tracks: Vec<Vec<u8>>,
    artists: Vec<Str>,
}

fn read_pdb(stick: &Path) -> Pdb {
    let bytes = fs::read(stick.join("PIONEER/rekordbox/export.pdb")).unwrap();
    let mut r = Cursor::new(bytes.as_slice());
    let header = Header::read(&mut r).unwrap();
    let mut pdb = Pdb {
        table_types: header
            .tables
            .iter()
            .map(|t| page_type_code(&t.page_type))
            .collect(),
        tracks: Vec::new(),
        artists: Vec::new(),
    };
    assert_eq!(header.tables.len(), 20, "table count");
    for table in &header.tables {
        let pages = header
            .read_pages(
                &mut r,
                Endian::Little,
                (&table.first_page, &table.last_page),
            )
            .unwrap();
        for page in pages {
            for group in &page.row_groups {
                for row in group.present_rows() {
                    match row {
                        Row::Track(t) => pdb.tracks.push(row_bytes(&t)),
                        Row::Artist(a) => {
                            // name at the near (u8 at 9) or far (u16 at 10) offset
                            let b = row_bytes(&a);
                            let ofs = if u16_at(&b, 0) == 0x64 {
                                u16_at(&b, 10)
                            } else {
                                usize::from(b[9])
                            };
                            pdb.artists.push(str_at(&b, ofs));
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    pdb
}

/// PPTH of an ANLZ file. Sections are walked by their headers rather than
/// with `rekordcrate::anlz::ANLZ`: rekordcrate 0.3.0 knows PCPT cue types 0
/// and 2 only and rejects the 1 that baken writes for a point cue.
fn ppth(path: &Path) -> String {
    let b = fs::read(path).unwrap();
    let be = |at: usize| u32::from_be_bytes(b[at..at + 4].try_into().unwrap()) as usize;
    assert_eq!(&b[..4], b"PMAI");
    let mut at = be(4);
    while at + 12 <= b.len() {
        let (header_len, total_len) = (be(at + 4), be(at + 8));
        if &b[at..at + 4] == b"PPTH" {
            let len = be(at + 12);
            let start = at + header_len;
            let units: Vec<u16> = b[start..start + len]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_be_bytes(*c))
                .take_while(|&u| u != 0)
                .collect();
            return String::from_utf16(&units).unwrap();
        }
        at += total_len;
    }
    panic!("no PPTH in {}", path.display())
}

/// Every file under `dir`, stick-relative with `/`.
fn files_under(stick: &Path, dir: &str) -> BTreeSet<String> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeSet<String>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for e in entries {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(root, &p, out);
            } else {
                let rel = p
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(format!("/{rel}"));
            }
        }
    }
    let mut out = BTreeSet::new();
    walk(stick, &on_stick(stick, dir), &mut out);
    out
}

// --- tests -----------------------------------------------------------------

#[test]
fn stick_matches_the_plan() {
    let w = World::new();
    let sel = w.sel(&["Set"]);
    let plan = w.plan(&sel);
    assert_eq!(plan.tracks.len(), 4);
    let (report, events) = w.write(&sel, &plan);

    assert_eq!(report.tracks_in_database, 4);
    assert_eq!(report.audio_copied, 4);
    assert_eq!(report.anlz_generated, 4);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert!(!report.cancelled);
    assert_eq!(events.first(), Some(&ExportEvent::Started { total: 4 }));
    assert_eq!(events.last(), Some(&ExportEvent::Finished));
    let done: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            ExportEvent::TrackDone { done, track_id, .. } => Some((*done, *track_id)),
            _ => None,
        })
        .collect();
    let ids: Vec<_> = plan.tracks.iter().map(|t| Some(t.track_id)).collect();
    assert_eq!(done.iter().map(|d| d.1).collect::<Vec<_>>(), ids);

    // Audio: exactly the plan's paths, all ASCII.
    let planned: BTreeSet<String> = plan.tracks.iter().map(|t| t.dst_rel.clone()).collect();
    assert_eq!(files_under(&w.stick, "/Contents"), planned);
    assert!(planned.iter().all(|p| p.is_ascii()), "{planned:?}");
    assert!(
        planned.contains("/Contents/Kino/Zvezda/Pesnia.flac"),
        "{planned:?}"
    );
    // baken on its own would have used the Unicode names.
    assert!(!on_stick(&w.stick, "/Contents/Кино").exists());

    // export.pdb: all 20 tables, one row per planned track.
    let pdb = read_pdb(&w.stick);
    let expected_tables: BTreeSet<u32> = (0..20).collect();
    assert_eq!(pdb.table_types, expected_tables);
    assert_eq!(pdb.tracks.len(), plan.tracks.len());

    let by_path: HashMap<String, &Vec<u8>> = pdb
        .tracks
        .iter()
        .map(|r| (track_str(r, FILE_PATH).text, r))
        .collect();
    assert_eq!(by_path.keys().cloned().collect::<BTreeSet<_>>(), planned);
    for t in &plan.tracks {
        let row = by_path[&t.dst_rel];
        let path = track_str(row, FILE_PATH);
        assert!(!path.utf16, "{path:?}");
        assert!(on_stick(&w.stick, &path.text).is_file());
        let name = t.dst_rel.rsplit('/').next().unwrap();
        assert_eq!(track_str(row, FILENAME).text, name);
        let ext = name.rsplit('.').next().unwrap();
        let code = match ext {
            "mp3" => 1,
            "flac" => 5,
            "wav" => 11,
            "aiff" => 12,
            other => panic!("{other}"),
        };
        assert_eq!(u16_at(row, FILE_TYPE_AT), code, "{name}");
        // ANLZ where the player computes it from the path, PPTH = dst_rel.
        let dat = format!("{}/ANLZ0000.DAT", anlz_dir(&t.dst_rel));
        assert_eq!(track_str(row, ANALYZE_PATH).text, dat);
        for ext in ["DAT", "EXT"] {
            let f = on_stick(&w.stick, &dat.replace(".DAT", &format!(".{ext}")));
            assert_eq!(ppth(&f), t.dst_rel, "{}", f.display());
        }
    }

    // Metadata keeps its Unicode, stored as UTF-16.
    let titles: HashMap<String, Str> = pdb
        .tracks
        .iter()
        .map(|r| (track_str(r, FILE_PATH).text, track_str(r, TITLE)))
        .collect();
    let kino = &titles["/Contents/Kino/Zvezda/Pesnia.flac"];
    assert_eq!(kino.text, "Группа крови");
    assert!(kino.utf16);
    let plain = titles.values().find(|s| s.text == "Plain").unwrap();
    assert!(!plain.utf16);
    for artist in ["Кино", "坂本龍一", "Sigur Rós"] {
        let a = pdb
            .artists
            .iter()
            .find(|a| a.text == artist)
            .unwrap_or_else(|| panic!("{artist}"));
        assert!(a.utf16, "{a:?}");
    }
    assert!(pdb.artists.iter().any(|a| a.text == "Alpha" && !a.utf16));
}

#[test]
fn re_export_copies_nothing_and_the_writer_deletes_nothing() {
    let w = World::new();
    let all = w.sel(&["Set"]);
    let first = w.plan(&all);
    w.write(&all, &first);

    // Same selection again: every track is on the stick already.
    let again = w.plan(&all);
    assert!(again.tracks.iter().all(|t| t.on_stick));
    assert_eq!(dsts(&again), dsts(&first));
    let (report, _) = w.write(&all, &again);
    assert_eq!((report.audio_copied, report.audio_kept), (0, 4));
    assert_eq!(report.anlz_written, 0, "ANLZ bytes unchanged");

    // Only Peak: the WAV (only in Chill) and its ANLZ are the plan's
    // deletes, which only `apply` carries out (review F16).
    let peak = w.sel(&["Peak"]);
    let smaller = w.plan(&peak);
    assert_eq!(smaller.tracks.len(), 3);
    let before: BTreeSet<String> = files_under(&w.stick, "/Contents")
        .into_iter()
        .chain(files_under(&w.stick, "/PIONEER/USBANLZ"))
        .collect();
    let (report, _) = w.write(&peak, &smaller);
    let after: BTreeSet<String> = files_under(&w.stick, "/Contents")
        .into_iter()
        .chain(files_under(&w.stick, "/PIONEER/USBANLZ"))
        .collect();
    assert!(!smaller.deletes.is_empty());
    assert!(after.is_superset(&before), "the writer removed files");
    assert_eq!(report.tracks_in_database, 3);
    assert_eq!(read_pdb(&w.stick).tracks.len(), 3);
}

#[test]
fn cached_analysis_is_copied_with_the_stick_path() {
    // A first export's generated ANLZ serves as "rekordbox's cache".
    let w = World::new();
    let sel = w.sel(&["Peak"]);
    let first = w.plan(&sel);
    w.write(&sel, &first);
    let cache: HashMap<PathBuf, PathBuf> = first
        .tracks
        .iter()
        .map(|t| {
            let dat = on_stick(&w.stick, &format!("{}/ANLZ0000.DAT", anlz_dir(&t.dst_rel)));
            (t.src.clone(), dat)
        })
        .collect();

    let stick2 = w.tmp.path().join("stick2");
    fs::create_dir_all(&stick2).unwrap();
    let locate = |t: &SourceTrack| cache.get(&t.src).cloned();
    let plan = w.plan_to(&sel, &stick2, &ExportOptions::default(), &locate);
    assert!(plan
        .tracks
        .iter()
        .all(|t| matches!(t.anlz_source, AnlzSource::RekordboxCache { .. })));
    let (report, _) = w
        .write_to(&sel, &plan, &stick2, &CancelToken::new())
        .unwrap();
    assert_eq!(report.anlz_generated, 0);
    assert_eq!(report.tracks_in_database, 3);
    for t in &plan.tracks {
        let dat = on_stick(&stick2, &format!("{}/ANLZ0000.DAT", anlz_dir(&t.dst_rel)));
        assert_eq!(ppth(&dat), t.dst_rel);
    }
}

#[test]
fn cancel_before_the_first_track_leaves_the_old_database() {
    let w = World::new();
    let sel = w.sel(&["Set"]);
    let plan = w.plan(&sel);
    let pdb = w.stick.join("PIONEER/rekordbox/export.pdb");
    fs::create_dir_all(pdb.parent().unwrap()).unwrap();
    fs::write(&pdb, b"old database").unwrap();

    let cancel = CancelToken::new();
    cancel.cancel();
    let (report, events) = w.write_to(&sel, &plan, &w.stick, &cancel).unwrap();
    assert!(report.cancelled);
    assert_eq!(report.tracks_in_database, 0);
    assert!(!events.contains(&ExportEvent::Finished));
    assert_eq!(fs::read(&pdb).unwrap(), b"old database");
    assert!(files_under(&w.stick, "/Contents").is_empty());
}

#[cfg(unix)]
#[test]
fn read_only_stick_is_a_device_error_and_keeps_the_old_database() {
    use std::os::unix::fs::PermissionsExt;
    let w = World::new();
    let sel = w.sel(&["Set"]);
    let plan = w.plan(&sel);
    let rb = w.stick.join("PIONEER/rekordbox");
    fs::create_dir_all(&rb).unwrap();
    fs::write(rb.join("export.pdb"), b"old database").unwrap();
    fs::set_permissions(&rb, fs::Permissions::from_mode(0o555)).unwrap();
    if fs::write(rb.join("probe"), b"").is_ok() {
        // Running as root: permissions don't bite.
        fs::set_permissions(&rb, fs::Permissions::from_mode(0o755)).unwrap();
        return;
    }
    let result = w.write_to(&sel, &plan, &w.stick, &CancelToken::new());
    fs::set_permissions(&rb, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        matches!(result, Err(WriterError::DeviceWrite { .. })),
        "{result:?}"
    );
    assert_eq!(fs::read(rb.join("export.pdb")).unwrap(), b"old database");
    assert!(files_under(&w.stick, "/Contents").is_empty());
}

#[test]
fn source_gone_since_planning_is_refused_before_writing() {
    let w = World::new();
    let sel = w.sel(&["Set"]);
    let plan = w.plan(&sel);
    fs::remove_file(w.music.join("plain.mp3")).unwrap();
    let result = w.write_to(&sel, &plan, &w.stick, &CancelToken::new());
    assert!(
        matches!(result, Err(WriterError::PlanMismatch { .. })),
        "{result:?}"
    );
    assert!(!w.stick.join("PIONEER").exists());
    assert!(!w.stick.join("Contents").exists());
}

#[test]
fn export_without_analysis_is_not_supported() {
    let w = World::new();
    let sel = w.sel(&["Set"]);
    let opts = ExportOptions {
        missing_analysis: MissingAnalysis::ExportWithout,
        ..ExportOptions::default()
    };
    let plan = w.plan_to(&sel, &w.stick, &opts, &NoAnlz);
    let result = w.write_to(&sel, &plan, &w.stick, &CancelToken::new());
    assert!(
        matches!(result, Err(WriterError::Unsupported(_))),
        "{result:?}"
    );
    assert!(!w.stick.join("PIONEER").exists());
}

#[test]
fn blocked_plan_is_refused() {
    let w = World::new();
    let sel = w.sel(&["Set"]);
    let mut plan = w.plan(&sel);
    plan.errors.push(plan::Problem::NothingSelected);
    let result = w.write_to(&sel, &plan, &w.stick, &CancelToken::new());
    assert!(matches!(result, Err(WriterError::Blocked(1))), "{result:?}");
}

fn dsts(p: &ExportPlan) -> Vec<&str> {
    p.tracks.iter().map(|t| t.dst_rel.as_str()).collect()
}
