//! The reference stick is rekordbox 6's export of its demo tracks
//! (`fixtures/rekordbox/ref-rb6-demo`, no audio). Ours is written by baken
//! from the fixture audio, as in the validator's tests.

use std::fs;
use std::path::{Path, PathBuf};

use super::*;

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures");
const XML_LOCATION: &str = "file://localhost/Users/reks/Music/reks%20fixtures/";
const DEMO_2: &str = "/Contents/Loopmasters/UnknownAlbum/Demo Track 2.mp3";

fn reference() -> PathBuf {
    Path::new(ROOT).join("rekordbox/ref-rb6-demo")
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let dst = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &dst);
        } else {
            fs::copy(entry.path(), dst).unwrap();
        }
    }
}

/// A stick written by baken from the fixture audio and
/// `collection-small.xml` (generated analysis).
fn baken_stick(tmp: &Path) -> PathBuf {
    let music = tmp.join("music");
    let root = tmp.join("stick");
    let anlz_roots = tmp.join("no-rekordbox");
    for d in [&music, &root, &anlz_roots] {
        fs::create_dir_all(d).unwrap();
    }
    for file in [
        "sine-click-128.mp3",
        "sine-click-128.flac",
        "sine-click-128.m4a",
    ] {
        fs::copy(Path::new(ROOT).join("audio").join(file), music.join(file)).unwrap();
    }
    let xml = fs::read_to_string(Path::new(ROOT).join("rekordbox/collection-small.xml")).unwrap();
    let location = format!("file://localhost{}/", music.display());
    let xml_path = tmp.join("collection.xml");
    fs::write(&xml_path, xml.replace(XML_LOCATION, &location)).unwrap();
    let opts = baken_export::Options {
        xml: xml_path,
        device: root.clone(),
        anlz_roots: vec![anlz_roots],
        no_settings: true,
        generate_analysis: true,
        ..Default::default()
    };
    let plan = baken_export::plan(&opts).unwrap();
    let report = baken_export::export(&plan, &(), &baken_core::CancelToken::new()).unwrap();
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    root
}

#[test]
fn reference_against_itself_is_empty() {
    let r = diff(&reference(), &reference()).unwrap();
    assert!(r.is_same(), "{r}");
    assert!(r.differences.is_empty());
    // export.pdb, exportExt.pdb and 2 × 3 ANLZ files.
    assert_eq!(r.files.len(), 8, "{r}");
    // The ANLZ files were paired through the two matched tracks.
    let anlz = r
        .files
        .iter()
        .filter(|f| f.ours.as_ref().is_some_and(|s| s.path.contains("USBANLZ")))
        .count();
    assert_eq!(anlz, 6);
}

#[test]
fn baken_stick_against_reference_reports_differences() {
    let tmp = tempfile::tempdir().unwrap();
    let ours = baken_stick(tmp.path());
    let r = diff(&ours, &reference()).unwrap();
    assert!(!r.is_same());
    // Different tracks: every row is on one side only, so no ANLZ pairs.
    let only = |side: Side| {
        r.differences
            .iter()
            .filter(|d| matches!(d, Difference::TrackOnlyIn { side: s, .. } if *s == side))
            .count()
    };
    assert_eq!(only(Side::Ours), 3, "{r}");
    assert_eq!(only(Side::Theirs), 2, "{r}");
    assert!(r.differences.iter().any(|d| matches!(
        d,
        Difference::RowCount {
            table: 0,
            ours: 3,
            ..
        }
    )));
    assert!(r.differences.iter().any(|d| matches!(
        d,
        Difference::PlaylistOnlyIn { side: Side::Ours, path, is_folder: true } if path == "reks fixtures"
    )));
    let pdb = r
        .files
        .iter()
        .find(|f| {
            f.ours
                .as_ref()
                .is_some_and(|s| s.path.ends_with("export.pdb"))
        })
        .unwrap();
    assert!(pdb.theirs.is_some() && !pdb.is_same());
    // Text and JSON output work on a real difference list.
    assert!(r.to_string().contains("structural difference(s)"));
    serde_json::to_string(&r).unwrap();
}

#[test]
fn baken_stick_against_itself_is_empty() {
    let tmp = tempfile::tempdir().unwrap();
    let ours = baken_stick(tmp.path());
    let r = diff(&ours, &ours).unwrap();
    assert!(r.is_same(), "{r}");
}

#[test]
fn mutated_copy_reports_exactly_the_mutations() {
    let tmp = tempfile::tempdir().unwrap();
    let ours = tmp.path().join("ours");
    copy_dir(&reference(), &ours);

    // Demo Track 2's title (a short ASCII DeviceSQL string, header 0x1B)
    // becomes "Demo Track 9", in place.
    let pdb = ours.join("PIONEER/rekordbox/export.pdb");
    let mut bytes = fs::read(&pdb).unwrap();
    let needle = b"\x1BDemo Track 2";
    let hits: Vec<usize> = bytes
        .windows(needle.len())
        .enumerate()
        .filter(|(_, w)| *w == needle)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(hits.len(), 1);
    let at = hits[0] + needle.len() - 1;
    bytes[at] = b'9';
    fs::write(&pdb, &bytes).unwrap();
    // One ANLZ file removed.
    let ext = ours.join("PIONEER/USBANLZ/P053/0001D21F/ANLZ0000.EXT");
    fs::remove_file(&ext).unwrap();

    let r = diff(&ours, &reference()).unwrap();
    assert_eq!(
        r.differences,
        vec![
            Difference::TrackField {
                file_path: DEMO_2.into(),
                field: "title",
                ours: "Demo Track 9".into(),
                theirs: "Demo Track 2".into(),
            },
            Difference::AnlzOnlyIn {
                side: Side::Theirs,
                file_path: DEMO_2.into(),
                ext: "EXT",
                path: "/PIONEER/USBANLZ/P053/0001D21F/ANLZ0000.EXT".into(),
            },
        ],
        "{r}"
    );
    let changed: Vec<&FileDiff> = r.files.iter().filter(|f| !f.is_same()).collect();
    assert_eq!(changed.len(), 2, "{r}");
    let pdb = changed
        .iter()
        .find(|f| {
            f.ours
                .as_ref()
                .is_some_and(|s| s.path.ends_with("export.pdb"))
        })
        .unwrap();
    assert_eq!(pdb.differing_bytes, 1);
    assert_eq!(pdb.first_difference, Some(at as u64));
    assert!(changed
        .iter()
        .any(|f| f.ours.is_none() && f.theirs.as_ref().is_some_and(|s| s.path.ends_with(".EXT"))));
}

#[test]
fn missing_pdb_is_a_difference() {
    let tmp = tempfile::tempdir().unwrap();
    let r = diff(tmp.path(), &reference()).unwrap();
    assert_eq!(
        r.differences[0],
        Difference::PdbMissing { side: Side::Ours }
    );
    assert!(r.differences.iter().any(|d| matches!(
        d,
        Difference::TrackOnlyIn { side: Side::Theirs, file_path } if file_path == DEMO_2
    )));
    assert!(r.files.iter().all(|f| f.ours.is_none()));
    // Two empty sticks are the same.
    assert!(diff(tmp.path(), tmp.path()).unwrap().is_same());
}

#[test]
fn reference_fields_decode() {
    let s = Stick::read(&reference(), Side::Theirs).unwrap();
    let t = s.tracks().iter().find(|t| t.file_path == DEMO_2).unwrap();
    let field = |name: &str| {
        t.fields
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v.as_str())
            .unwrap()
    };
    assert_eq!(field("title"), "Demo Track 2");
    assert_eq!(field("filename"), "Demo Track 2.mp3");
    assert_eq!(field("tempo"), "12000");
    assert_eq!(field("artist"), "Loopmasters");
    assert_eq!(
        t.analyze_path,
        "/PIONEER/USBANLZ/P053/0001D21F/ANLZ0000.DAT"
    );
    let Pdb::Read { tables, .. } = &s.pdb else {
        panic!("reference PDB unreadable");
    };
    assert_eq!(tables.len(), 20);
}

#[test]
fn section_counts() {
    let dat = fs::read(reference().join("PIONEER/USBANLZ/P053/0001D21F/ANLZ0000.DAT")).unwrap();
    let s = sections(&dat).unwrap();
    let pqtz = s.iter().find(|s| s.tag == "PQTZ").unwrap();
    assert_eq!(pqtz.count, Some(257));
    assert!(s.iter().any(|s| s.tag == "PPTH" && s.count.is_none()));
}

#[test]
fn byte_diff_counts_length_difference() {
    let d = byte_diff(Some(("/a", b"abcd")), Some(("/a", b"abXdef")));
    assert_eq!(d.first_difference, Some(2));
    assert_eq!(d.differing_bytes, 1 + 2);
    let d = byte_diff(Some(("/a", b"ab")), Some(("/a", b"abc")));
    assert_eq!(d.first_difference, Some(2));
    assert!(byte_diff(Some(("/a", b"ab")), Some(("/a", b"ab"))).is_same());
    assert!(!byte_diff(Some(("/a", b"ab")), None).is_same());
}
