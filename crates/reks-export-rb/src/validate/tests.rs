//! A real stick is written by baken-export from the fixture audio and
//! `fixtures/rekordbox/collection-small.xml` (generated analysis, so no
//! rekordbox cache is needed), validated clean, then damaged one way at a
//! time.

use std::fs;
use std::path::{Path, PathBuf};

use reks_audio::{Kind, Probe};

use super::*;
use crate::device::{DeviceInfo, FsKind};
use crate::plan::{ExportOptions, PlannedTrack};

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures");
const XML_LOCATION: &str = "file://localhost/Users/reks/Music/reks%20fixtures/";

/// Fixture file, track id, cue count (POSITION_MARKs in the XML).
const TRACKS: [(&str, TrackId, u8); 3] = [
    ("sine-click-128.mp3", 1, 5),
    ("sine-click-128.flac", 2, 2),
    ("sine-click-128.m4a", 3, 3),
];

struct Stick {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    plan: ExportPlan,
}

impl Stick {
    fn path(&self, rel: &str) -> PathBuf {
        locate(&self.root, rel).unwrap().unwrap()
    }

    fn track(&self, id: TrackId) -> &PlannedTrack {
        self.plan.tracks.iter().find(|t| t.track_id == id).unwrap()
    }

    fn validate(&self) -> ValidationReport {
        validate(&self.root, &self.plan).unwrap()
    }

    /// `analyze_path` of a track, read back.
    fn analyze_path(&self, id: TrackId) -> String {
        let r = self.validate();
        let dst = &self.track(id).dst_rel;
        let row = r.tracks.iter().find(|t| &t.file_path == dst).unwrap();
        row.analyze_path.clone()
    }
}

/// Writes the stick with baken and builds the plan the planner would have
/// made for it (paths as baken chose them).
fn stick() -> Stick {
    let tmp = tempfile::tempdir().unwrap();
    let music = tmp.path().join("music");
    let root = tmp.path().join("stick");
    let anlz_roots = tmp.path().join("no-rekordbox");
    for d in [&music, &root, &anlz_roots] {
        fs::create_dir_all(d).unwrap();
    }
    for (file, ..) in TRACKS {
        fs::copy(Path::new(ROOT).join("audio").join(file), music.join(file)).unwrap();
    }
    let xml = fs::read_to_string(Path::new(ROOT).join("rekordbox/collection-small.xml")).unwrap();
    let location = format!("file://localhost{}/", music.display());
    let xml_path = tmp.path().join("collection.xml");
    fs::write(&xml_path, xml.replace(XML_LOCATION, &location)).unwrap();

    let opts = baken_export::Options {
        xml: xml_path,
        device: root.clone(),
        anlz_roots: vec![anlz_roots],
        no_settings: true,
        generate_analysis: true,
        ..Default::default()
    };
    let baken_plan = baken_export::plan(&opts).unwrap();
    assert!(baken_plan.skipped.is_empty(), "{:?}", baken_plan.skipped);
    let report = baken_export::export(&baken_plan, &(), &baken_core::CancelToken::new()).unwrap();
    assert!(report.failures.is_empty(), "{:?}", report.failures);

    let tracks = TRACKS
        .iter()
        .map(|&(file, id, cues)| {
            let dt = baken_plan
                .tracks
                .iter()
                .find(|t| t.source.ends_with(file))
                .unwrap();
            let kind = match file.rsplit('.').next().unwrap() {
                "mp3" => Kind::Mp3,
                "flac" => Kind::Flac,
                _ => Kind::Aac,
            };
            PlannedTrack {
                track_id: id,
                src: music.join(file),
                dst_rel: dt.device.usb_path.clone(),
                on_stick: false,
                kind,
                probe: Probe {
                    kind,
                    duration_ms: 30_000,
                    sample_rate: 44_100,
                    channels: 2,
                    bit_depth: None,
                    bit_rate_kbps: None,
                },
                has_grid: true,
                cue_count: cues,
                anlz_source: AnlzSource::Generated,
                divider: None,
            }
        })
        .collect();
    let playlist =
        |id, parent_id, name: &str, is_folder, track_ids: Vec<TrackId>| PlannedPlaylist {
            id,
            parent_id,
            name: name.into(),
            is_folder,
            track_ids,
        };
    let plan = ExportPlan {
        device: DeviceInfo {
            mount_point: root.clone(),
            fs: FsKind::Fat32,
            fs_name: "msdos".into(),
            removable: true,
            total_bytes: 32 << 30,
            free_bytes: 1 << 30,
            label: "REKS".into(),
            os_name: "disk9s1".into(),
        },
        options: ExportOptions {
            missing_analysis: crate::plan::MissingAnalysis::Generate,
            ..Default::default()
        },
        tracks,
        playlists: vec![
            playlist(10, None, "reks fixtures", true, vec![]),
            playlist(11, Some(10), "All fixtures", false, vec![1, 2, 3]),
            playlist(12, Some(10), "Lossless", false, vec![2]),
        ],
        copies: vec![],
        anlz: vec![],
        deletes: vec![],
        history: Default::default(),
        backups: vec![],
        guard: Default::default(),
        history_import: None,
        warnings: vec![],
        errors: vec![],
        bytes_to_write: 0,
        bytes_free_after: 0,
        convertible: Vec::new(),
        onelibrary: None,
    };
    Stick {
        _tmp: tmp,
        root,
        plan,
    }
}

fn has(r: &ValidationReport, pred: impl Fn(&Finding) -> bool) -> bool {
    r.findings.iter().any(pred)
}

#[test]
fn clean_stick_has_no_findings() {
    let s = stick();
    let r = s.validate();
    assert!(r.is_valid(), "{:#?}", r.findings);
    assert_eq!(r.tracks.len(), 3);
    assert!(r.tracks.iter().all(|t| t.track_id.is_some()));
    assert_eq!(r.playlists.len(), 3);
    let all = r
        .playlists
        .iter()
        .find(|p| p.name == "All fixtures")
        .unwrap();
    assert_eq!(all.entries.len(), 3);

    let rows = r.manifest_rows();
    assert_eq!(rows.len(), 3);
    for (_, id, _) in TRACKS {
        let row = rows.iter().find(|m| m.track_id == Some(id)).unwrap();
        assert_eq!(row.stick_path, s.track(id).dst_rel);
        assert_eq!(row.item_kind, ManifestItemKind::Track);
    }

    // Serialisable for the UI and the CLI.
    let json = serde_json::to_string(&r).unwrap();
    let back: ValidationReport = serde_json::from_str(&json).unwrap();
    assert_eq!(back, r);
}

#[test]
fn missing_pdb_is_reported() {
    let s = stick();
    fs::remove_file(s.path(PDB_PATH)).unwrap();
    assert_eq!(s.validate().findings, vec![Finding::PdbMissing]);
}

#[test]
fn garbage_pdb_is_reported() {
    let s = stick();
    fs::write(s.path(PDB_PATH), b"not a pdb").unwrap();
    let r = s.validate();
    assert!(matches!(r.findings[..], [Finding::PdbUnreadable { .. }]));
}

#[test]
fn deleted_anlz_is_reported() {
    let s = stick();
    let dat = s.analyze_path(2);
    fs::remove_file(s.path(&dat)).unwrap();
    let r = s.validate();
    assert_eq!(
        r.findings,
        vec![Finding::AnlzMissing {
            track_id: 2,
            path: dat
        }]
    );
    assert_eq!(r.track_findings(2).count(), 1);
    assert_eq!(r.track_findings(1).count(), 0);
}

#[test]
fn deleted_audio_is_reported() {
    let s = stick();
    let dst = s.track(3).dst_rel.clone();
    fs::remove_file(s.path(&dst)).unwrap();
    let r = s.validate();
    assert_eq!(
        r.findings,
        vec![Finding::AudioMissing {
            track_id: 3,
            path: dst
        }]
    );
}

/// Replaces the first occurrence of `from` (as UTF-16BE, the way PPTH stores
/// it) with `to` of the same length.
fn patch_utf16be(path: &Path, from: &str, to: &str) {
    assert_eq!(from.len(), to.len());
    let enc = |s: &str| -> Vec<u8> { s.encode_utf16().flat_map(u16::to_be_bytes).collect() };
    let (from, to) = (enc(from), enc(to));
    let mut bytes = fs::read(path).unwrap();
    let at = bytes
        .windows(from.len())
        .position(|w| w == from)
        .expect("PPTH path in file");
    bytes[at..at + to.len()].copy_from_slice(&to);
    fs::write(path, bytes).unwrap();
}

#[test]
fn corrupt_ppth_is_reported() {
    let s = stick();
    let dat = s.analyze_path(1);
    let dst = s.track(1).dst_rel.clone();
    let wrong = dst.replace("sine", "sinf");
    patch_utf16be(&s.path(&dat), &dst, &wrong);
    let r = s.validate();
    assert_eq!(
        r.findings,
        vec![Finding::PpthMismatch {
            track_id: 1,
            path: dat,
            expected: dst,
            found: Some(wrong),
        }]
    );
}

/// Clears the presence bit of the first row on the first tracks page that
/// has rows, the way a deleted row looks in `export.pdb`.
fn drop_first_track_row(pdb: &Path) {
    let mut bytes = fs::read(pdb).unwrap();
    let mut reader = Cursor::new(&bytes[..]);
    let header = Header::read(&mut reader).unwrap();
    let size = header.page_size as usize;
    let table = header
        .tables
        .iter()
        .find(|t| t.page_type == PageType::Tracks)
        .unwrap();
    let mut index = table.first_page.clone();
    let page_start = loop {
        let offset = index.offset(header.page_size);
        reader.seek(SeekFrom::Start(offset)).unwrap();
        let page = Page::read_options(&mut reader, Endian::Little, (header.page_size,)).unwrap();
        if page
            .row_groups
            .iter()
            .any(|g| g.present_rows().next().is_some())
        {
            break offset as usize;
        }
        assert!(page.page_index != table.last_page, "no track rows");
        index = page.next_page;
    };
    // Row group 0 ends the page: presence flags (u16) at the last 4 bytes.
    let at = page_start + size - 4;
    let flags = u16::from_le_bytes([bytes[at], bytes[at + 1]]);
    assert_eq!(flags & 1, 1);
    bytes[at..at + 2].copy_from_slice(&(flags & !1).to_le_bytes());
    fs::write(pdb, bytes).unwrap();
}

#[test]
fn dropped_track_row_is_reported() {
    let s = stick();
    drop_first_track_row(&s.path(PDB_PATH));
    let r = s.validate();
    assert_eq!(r.tracks.len(), 2);
    assert!(has(&r, |f| *f
        == Finding::TrackCount {
            expected: 3,
            found: 2
        }));
    let dropped: Vec<_> = r
        .findings
        .iter()
        .filter(|f| matches!(f, Finding::TrackNotInPdb { .. }))
        .collect();
    assert_eq!(dropped.len(), 1, "{:#?}", r.findings);
    // Its playlist entries now point at a missing row.
    assert!(has(&r, |f| matches!(f, Finding::PlaylistEntries { .. })));
}

#[test]
fn planned_track_without_row_is_reported() {
    let mut s = stick();
    let mut extra = s.track(1).clone();
    extra.track_id = 4;
    extra.dst_rel = "/Contents/reks fixtures/reks fixtures/gone.mp3".into();
    s.plan.tracks.push(extra);
    s.plan.playlists[1].track_ids.push(4);
    let r = s.validate();
    assert!(has(&r, |f| *f
        == Finding::TrackCount {
            expected: 4,
            found: 3
        }));
    assert!(has(&r, |f| matches!(
        f,
        Finding::TrackNotInPdb { track_id: 4, .. }
    )));
    assert!(has(&r, |f| matches!(
        f,
        Finding::PlaylistEntries { path, expected_len: 4, found_len: 3, first_difference: 3 }
            if path == "reks fixtures/All fixtures"
    )));
}

#[test]
fn unexpected_track_row_is_reported() {
    let mut s = stick();
    s.plan.tracks.retain(|t| t.track_id != 3);
    s.plan.playlists[1].track_ids.retain(|&id| id != 3);
    let r = s.validate();
    assert!(has(&r, |f| matches!(
        f,
        Finding::TrackCount {
            expected: 2,
            found: 3
        }
    )));
    assert!(has(&r, |f| matches!(f, Finding::UnexpectedTrack { .. })));
    let unknown = r.tracks.iter().filter(|t| t.track_id.is_none()).count();
    assert_eq!(unknown, 1);
    assert!(r.manifest_rows().iter().any(|m| m.track_id.is_none()));
}

#[test]
fn reordered_playlist_is_reported() {
    let mut s = stick();
    s.plan.playlists[1].track_ids = vec![2, 1, 3];
    let r = s.validate();
    assert_eq!(
        r.findings,
        vec![Finding::PlaylistEntries {
            path: "reks fixtures/All fixtures".into(),
            expected_len: 3,
            found_len: 3,
            first_difference: 0,
        }]
    );
}

#[test]
fn playlist_tree_differences_are_reported() {
    let mut s = stick();
    s.plan.playlists.swap(1, 2);
    s.plan.playlists[0].name = "Renamed".into();
    s.plan.playlists.push(PlannedPlaylist {
        id: 13,
        parent_id: None,
        name: "Extra".into(),
        is_folder: false,
        track_ids: vec![],
    });
    let r = s.validate();
    assert!(has(
        &r,
        |f| matches!(f, Finding::PlaylistMissing { path } if path == "Renamed")
    ));
    assert!(has(
        &r,
        |f| matches!(f, Finding::PlaylistMissing { path } if path == "Extra")
    ));
    assert!(has(
        &r,
        |f| matches!(f, Finding::UnexpectedPlaylist { path } if path == "reks fixtures")
    ));

    // Sibling order alone.
    let mut s = stick();
    s.plan.playlists.swap(1, 2);
    let r = s.validate();
    assert_eq!(
        r.findings,
        vec![Finding::PlaylistOrder {
            parent: "reks fixtures".into(),
            expected: vec!["Lossless".into(), "All fixtures".into()],
            found: vec!["All fixtures".into(), "Lossless".into()],
        }]
    );
}

#[test]
fn cue_and_grid_expectations_are_checked() {
    let mut s = stick();
    s.plan.tracks[0].cue_count = 9;
    let r = s.validate();
    assert!(!r.findings.is_empty());
    assert!(r.findings.iter().all(|f| matches!(
        f,
        Finding::CueCount {
            track_id: 1,
            expected: 9,
            ..
        }
    )));

    // Exported without analysis: no grid or cue expectations.
    let mut s = stick();
    s.plan.tracks[0].cue_count = 9;
    s.plan.tracks[0].anlz_source = AnlzSource::None;
    assert!(s.validate().is_valid());
}

#[test]
fn empty_grid_is_reported() {
    let s = stick();
    let dat = s.path(&s.analyze_path(1));
    // Zero the PQTZ beat count (big-endian u32 at section offset 20) and
    // shrink the section to its header, the way an empty grid is written.
    let mut bytes = fs::read(&dat).unwrap();
    let at = bytes.windows(4).position(|w| w == b"PQTZ").unwrap();
    let len_header = u32::from_be_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
    let len = u32::from_be_bytes(bytes[at + 8..at + 12].try_into().unwrap()) as usize;
    let removed = len - len_header;
    bytes[at + 20..at + 24].copy_from_slice(&0u32.to_be_bytes());
    bytes[at + 8..at + 12].copy_from_slice(&(len_header as u32).to_be_bytes());
    bytes.drain(at + len_header..at + len);
    let total = u32::from_be_bytes(bytes[8..12].try_into().unwrap()) as usize - removed;
    bytes[8..12].copy_from_slice(&(total as u32).to_be_bytes());
    fs::write(&dat, bytes).unwrap();
    let r = s.validate();
    assert!(
        has(&r, |f| matches!(f, Finding::GridEmpty { track_id: 1, .. })),
        "{:#?}",
        r.findings
    );
}

#[test]
fn table_list_is_checked() {
    let mut findings = Vec::new();
    check_tables(&(0..20).collect::<Vec<_>>(), &mut findings);
    assert!(findings.is_empty());
    check_tables(&[0, 1, 1, 2, 25], &mut findings);
    assert!(findings.contains(&Finding::TableUnexpected { page_type: 1 }));
    assert!(findings.contains(&Finding::TableUnexpected { page_type: 25 }));
    assert!(findings.contains(&Finding::TableMissing { page_type: 19 }));
    assert!(!findings.contains(&Finding::TableMissing { page_type: 2 }));
}

#[test]
fn locate_ignores_case() {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir_all(tmp.path().join("PIONEER/rekordbox")).unwrap();
    fs::write(tmp.path().join("PIONEER/rekordbox/export.pdb"), b"x").unwrap();
    assert!(locate(tmp.path(), "/pioneer/REKORDBOX/export.pdb")
        .unwrap()
        .is_some());
    assert!(locate(tmp.path(), "/PIONEER/rekordbox").unwrap().is_none());
    assert!(locate(tmp.path(), "/nope/x").unwrap().is_none());
}
