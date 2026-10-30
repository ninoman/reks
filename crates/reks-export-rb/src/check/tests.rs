//! Synthetic sticks in temp dirs (both libraries, one only, missing audio
//! or ANLZ, a bad WAV header, damaged rows), a stick baken writes, and
//! rekordbox 7.2.19's reference export. Every check leaves the stick's
//! bytes and times as they were.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use baken_export::anlz::section::{section, AnlzFile};
use reks_core::history::{HistoryLibrary, ImportChoices, SourceEntry};

use super::*;
use crate::history::tests::{snapshot, stick};
use crate::testutil::*;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures");

fn fixture_audio(name: &str) -> Vec<u8> {
    fs::read(Path::new(FIXTURES).join("audio").join(name)).unwrap()
}

fn device(root: &Path, fs: FsKind) -> DeviceInfo {
    DeviceInfo {
        mount_point: root.to_owned(),
        fs,
        fs_name: "msdos".into(),
        removable: true,
        total_bytes: 16_000_000_000,
        free_bytes: 3_200_000_000,
        label: "GIG".into(),
        os_name: "GIG".into(),
    }
}

/// Reads no `exportLibrary.db`: a stick without one never asks.
struct NoReader;

impl OneLibraryRead for NoReader {
    fn read(&self, _db: &Path) -> Result<OneLibraryContents, String> {
        Err("no OneLibrary key".into())
    }
}

/// Hands back fixed contents, as the real reader would for the stick.
struct Fake(OneLibraryContents);

impl OneLibraryRead for Fake {
    fn read(&self, _db: &Path) -> Result<OneLibraryContents, String> {
        Ok(self.0.clone())
    }
}

fn run(root: &Path, one: &dyn OneLibraryRead, target: Option<TargetProfile>) -> StickReport {
    let before = snapshot(root);
    let report = check_stick(
        &device(root, FsKind::Fat32),
        &CheckOptions {
            target,
            volume_serial: None,
            directory: false,
        },
        one,
        None,
    );
    assert_eq!(snapshot(root), before, "the check changed the stick");
    report
}

fn groups(r: &StickReport) -> Vec<&FindingGroup> {
    r.sections.iter().flat_map(|s| &s.groups).collect()
}

fn group(r: &StickReport, kind: FindingKind) -> &FindingGroup {
    groups(r)
        .into_iter()
        .find(|g| g.kind == kind)
        .unwrap_or_else(|| panic!("no {kind:?} in {:#?}", kinds(r)))
}

fn kinds(r: &StickReport) -> Vec<(FindingKind, Severity, usize)> {
    groups(r)
        .iter()
        .map(|g| (g.kind, g.severity, g.items.len()))
        .collect()
}

fn has(r: &StickReport, kind: FindingKind) -> bool {
    groups(r).iter().any(|g| g.kind == kind)
}

/// An `export.pdb` with all 20 tables, filled from `rows` (≤ 16 per table).
fn full_pdb(rows: &[(u32, Vec<Vec<u8>>)]) -> Vec<u8> {
    let by_table: BTreeMap<u32, Vec<Vec<u8>>> = rows.iter().cloned().collect();
    let tables: Vec<(u32, Vec<Vec<Vec<u8>>>)> = (0..20)
        .map(|t| (t, vec![by_table.get(&t).cloned().unwrap_or_default()]))
        .collect();
    synthetic_pdb(&tables)
}

/// `/PIONEER/USBANLZ/P001/0000000<n>/ANLZ0000.<ext>`.
fn anlz_path(n: u32, ext: &str) -> String {
    format!("/PIONEER/USBANLZ/P001/0000000{n}/ANLZ0000.{ext}")
}

/// An ANLZ file naming `ppth`, with a `PWAV` preview when `preview`.
fn anlz(ppth: &str, preview: bool) -> Vec<u8> {
    let mut file = AnlzFile {
        header_tail: [0, 0, 0, 1, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0],
        sections: Vec::new(),
    };
    if preview {
        let mut payload = 400u32.to_be_bytes().to_vec();
        payload.extend_from_slice(&0x0001_0000u32.to_be_bytes());
        payload.extend(std::iter::repeat_n(0x15, 400));
        file.sections.push(section(b"PWAV", 0x14, &payload));
    }
    file.set_path(ppth);
    file.to_bytes()
}

const A: &str = "/Contents/Artist/Album/a.wav";
const B: &str = "/Contents/Artist/Album/b.wav";

/// Track rows 1 (`A`) and 2 (`B`) with analysis paths, artist 1, and
/// playlist `Set` holding both.
fn pdb_rows() -> Vec<(u32, Vec<Vec<u8>>)> {
    let mut a = track_row(1, 1, 30, "Track A", A);
    let mut b = track_row(2, 1, 30, "Track B", B);
    set_analyze_path(&mut a, &anlz_path(1, "DAT"));
    set_analyze_path(&mut b, &anlz_path(2, "DAT"));
    vec![
        (T_TRACKS, vec![a, b]),
        (T_ARTISTS, vec![artist_row(1, "Artist")]),
        (T_PLAYLIST_TREE, vec![playlist_node(10, 0, 0, false, "Set")]),
        (
            T_PLAYLIST_ENTRIES,
            vec![playlist_entry(1, 1, 10), playlist_entry(2, 2, 10)],
        ),
    ]
}

/// Rewrites string 14 (`analyze_path`) of a [`track_row`] by appending it.
fn set_analyze_path(row: &mut Vec<u8>, path: &str) {
    let at = row.len() as u16;
    put_u16(row, 0x5E + 2 * 14, at);
    row.extend_from_slice(&short_string(path));
}

/// Both tracks' audio and full ANLZ sets (DAT with preview, EXT, 2EX).
fn track_files() -> Vec<(String, Vec<u8>)> {
    let wav = fixture_audio("sine-click-128.wav");
    let mut files = vec![(A.to_owned(), wav.clone()), (B.to_owned(), wav)];
    for (n, path) in [(1, A), (2, B)] {
        files.push((anlz_path(n, "DAT"), anlz(path, true)));
        files.push((anlz_path(n, "EXT"), anlz(path, false)));
        files.push((anlz_path(n, "2EX"), anlz(path, false)));
    }
    files
}

fn write_stick(files: &[(String, Vec<u8>)]) -> tempfile::TempDir {
    let refs: Vec<(&str, &[u8])> = files
        .iter()
        .map(|(p, b)| (p.trim_start_matches('/'), b.as_slice()))
        .collect();
    stick(&refs)
}

fn with_pdb(mut files: Vec<(String, Vec<u8>)>, pdb: Vec<u8>) -> Vec<(String, Vec<u8>)> {
    files.push((crate::validate::PDB_PATH.to_owned(), pdb));
    files
}

fn with_onelibrary(mut files: Vec<(String, Vec<u8>)>) -> Vec<(String, Vec<u8>)> {
    files.push((crate::onelibrary::DB_PATH.to_owned(), b"encrypted".to_vec()));
    files
}

/// What a reader returns for a OneLibrary that mirrors [`pdb_rows`].
fn mirror() -> OneLibraryContents {
    let track = |id: i64, title: &str, path: &str, n: u32| OneLibraryTrack {
        id,
        title: title.into(),
        artist: Some("Artist".into()),
        path: path.into(),
        analysis_path: anlz_path(n, "DAT"),
    };
    OneLibraryContents {
        tracks: vec![track(1, "Track A", A, 1), track(2, "Track B", B, 2)],
        playlists: vec![OneLibraryPlaylist {
            id: 10,
            parent_id: 0,
            sequence_no: 1,
            name: "Set".into(),
            is_folder: false,
            track_ids: vec![1, 2],
        }],
        sessions: Vec::new(),
        integrity: "ok".into(),
    }
}

#[test]
fn a_sound_stick_with_both_libraries_is_ok() {
    let dir = write_stick(&with_onelibrary(with_pdb(
        track_files(),
        full_pdb(&pdb_rows()),
    )));
    let r = run(dir.path(), &Fake(mirror()), Some(TargetProfile::Cdj3000));
    assert_eq!(kinds(&r), []);
    assert_eq!(r.summary.status, Status::Ok);
    assert_eq!(r.summary.headline, "Nothing wrong found");
    assert_eq!(r.tracks_checked, 2);
    let present: Vec<(LibraryFormat, bool, &str)> = r
        .formats
        .iter()
        .map(|f| (f.format, f.present, f.status.as_str()))
        .collect();
    assert_eq!(
        present,
        [
            (LibraryFormat::DeviceLibrary, true, "2 tracks in 1 playlist"),
            (LibraryFormat::DeviceLibraryExt, false, "Not on this drive"),
            (LibraryFormat::OneLibrary, true, "2 tracks in 1 playlist"),
        ]
    );
    assert!(r.formats.iter().all(|f| !f.players_verified));
    assert!(r.players_source.contains("UNVERIFIED"));
    assert_eq!(r.filesystem.text, "FAT32 · 3.2 GB free of 16.0 GB");
    assert_eq!(
        r.target_text,
        "Audio checked against CDJ-3000, the players this stick is for."
    );
}

#[test]
fn one_library_only_is_a_warning() {
    let dir = write_stick(&with_pdb(track_files(), full_pdb(&pdb_rows())));
    let r = run(dir.path(), &NoReader, None);
    assert_eq!(
        kinds(&r),
        [(FindingKind::NoOneLibrary, Severity::Warning, 0)]
    );
    assert_eq!(r.summary.status, Status::Warnings);
    assert_eq!(r.summary.headline, "1 warning");

    let dir = write_stick(&with_onelibrary(track_files()));
    let r = run(dir.path(), &Fake(mirror()), None);
    assert_eq!(
        kinds(&r),
        [(FindingKind::NoDeviceLibrary, Severity::Warning, 0)]
    );
    assert_eq!(r.tracks_checked, 2, "OneLibrary's tracks are checked too");
    // A chosen target is a Device Library player.
    let r = run(dir.path(), &Fake(mirror()), Some(TargetProfile::Cdj3000));
    assert_eq!(
        kinds(&r),
        [(FindingKind::NoDeviceLibrary, Severity::Problem, 0)]
    );
}

#[test]
fn a_drive_without_a_library_is_a_problem() {
    let dir = tempfile::tempdir().unwrap();
    let r = run(dir.path(), &NoReader, None);
    assert_eq!(kinds(&r), [(FindingKind::NoLibrary, Severity::Problem, 0)]);
    assert_eq!(r.summary.status, Status::Problems);
    assert_eq!(r.tracks_checked, 0);
}

#[test]
fn missing_and_damaged_files_are_found_per_track() {
    let mut rows = pdb_rows();
    let mut c = track_row(3, 1, 30, "Track C", "/Contents/Artist/Album/c.wav");
    set_analyze_path(&mut c, "");
    let mut d = track_row(4, 1, 30, "Track D", "/Contents/../../etc/passwd");
    set_analyze_path(&mut d, &anlz_path(4, "DAT"));
    rows[0].1.extend([c, d]);
    let mut files: Vec<(String, Vec<u8>)> = track_files()
        .into_iter()
        // B: no audio, no .DAT.
        .filter(|(p, _)| p != B && *p != anlz_path(2, "DAT"))
        .collect();
    // A: .DAT without a preview, .EXT naming another file, damaged .2EX.
    for (p, bytes) in &mut files {
        if *p == anlz_path(1, "DAT") {
            *bytes = anlz(A, false);
        } else if *p == anlz_path(1, "EXT") {
            *bytes = anlz(B, false);
        } else if *p == anlz_path(1, "2EX") {
            *bytes = b"not an ANLZ file".to_vec();
        }
    }
    files.push((
        "/Contents/Artist/Album/c.wav".into(),
        fixture_audio("sine-click-128.wav"),
    ));
    let dir = write_stick(&with_pdb(files, full_pdb(&rows)));
    let r = run(dir.path(), &NoReader, Some(TargetProfile::Cdj3000));

    let subjects = |kind| -> Vec<String> {
        group(&r, kind)
            .items
            .iter()
            .map(|i| i.subject.clone())
            .collect()
    };
    assert_eq!(subjects(FindingKind::AudioMissing), ["Artist – Track B"]);
    // D's audio path leaves the stick; its analysis path is simply missing.
    assert_eq!(
        subjects(FindingKind::AnlzMissing),
        ["Artist – Track B", "Artist – Track D"]
    );
    assert_eq!(subjects(FindingKind::NoAnalysis), ["Artist – Track C"]);
    assert_eq!(
        subjects(FindingKind::NoWaveformPreview),
        ["Artist – Track A"]
    );
    assert_eq!(subjects(FindingKind::PpthMismatch), ["Artist – Track A"]);
    assert_eq!(
        group(&r, FindingKind::PpthMismatch).items[0]
            .detail
            .as_deref(),
        Some(format!(".EXT names {B}").as_str())
    );
    let damaged = group(&r, FindingKind::AnlzUnreadable);
    assert_eq!(damaged.severity, Severity::Warning, "a bad .2EX");
    assert_eq!(
        subjects(FindingKind::PathOutsideStick),
        ["Artist – Track D"]
    );
    assert_eq!(
        group(&r, FindingKind::AudioMissing).title,
        "1 track: audio file missing"
    );
    assert_eq!(r.summary.status, Status::Problems);
}

#[test]
fn a_bad_wav_header_and_target_dependent_audio_issues() {
    let mut rows = pdb_rows();
    let mut c = track_row(3, 1, 30, "Track C", "/Contents/Artist/Album/c.flac");
    set_analyze_path(&mut c, "");
    rows[0].1.push(c);
    let mut files = track_files();
    for (p, bytes) in &mut files {
        if p == B {
            // RIFF/WAVE with a fmt chunk cut short.
            *bytes = b"RIFF\x24\0\0\0WAVEfmt \x10\0\0\0\x01\0".to_vec();
        }
    }
    files.push((
        "/Contents/Artist/Album/c.flac".into(),
        fixture_audio("sine-click-128.flac"),
    ));
    let dir = write_stick(&with_pdb(files, full_pdb(&rows)));

    // Default: the strictest players, where FLAC is a problem.
    let r = run(dir.path(), &NoReader, None);
    assert_eq!(r.target, STRICTEST);
    assert!(r.target_text.contains("CDJ-2000NXS, the strictest players"));
    let bad = group(&r, FindingKind::AudioUnreadable);
    assert_eq!(bad.items[0].subject, "Artist – Track B");
    let flac = group(&r, FindingKind::AudioFormat);
    assert_eq!(flac.severity, Severity::Problem);
    assert_eq!(
        flac.title,
        "1 track: FLAC playback depends on the player model and firmware"
    );
    assert_eq!(
        flac.items[0].path.as_deref(),
        Some("/Contents/Artist/Album/c.flac")
    );

    let r = run(dir.path(), &NoReader, Some(TargetProfile::Cdj2000Nxs2));
    assert_eq!(
        group(&r, FindingKind::AudioFormat).severity,
        Severity::Warning
    );
    let r = run(dir.path(), &NoReader, Some(TargetProfile::Cdj3000));
    assert!(!has(&r, FindingKind::AudioFormat));
}

/// Row damage the page walk accepts but [`crate::pdb`]'s
/// strict read refuses (the Devices section shows an error for this stick).
#[test]
fn a_damaged_row_is_a_finding_and_the_rest_is_checked() {
    let mut rows = pdb_rows();
    rows[3].1.push(playlist_entry(1, 1, 99));
    let pdb = full_pdb(&rows);
    assert!(crate::pdb::read_pdb(&pdb).is_err());
    let dir = write_stick(&with_pdb(track_files(), pdb));
    let r = run(dir.path(), &NoReader, Some(TargetProfile::Cdj3000));
    let damaged = group(&r, FindingKind::RowsDamaged);
    assert_eq!(damaged.severity, Severity::Problem);
    assert_eq!(damaged.title, "1 row of export.pdb does not decode");
    assert_eq!(
        damaged.items[0].detail.as_deref(),
        Some("entry for missing playlist 99")
    );
    assert_eq!(r.tracks_checked, 2);
    assert_eq!(r.formats[0].status, "2 tracks in 1 playlist");
}

#[test]
fn missing_tables_and_an_unreadable_pdb() {
    // The testutil PDB has 3 of the 20 tables.
    let dir = write_stick(&with_pdb(track_files(), unplayed_pdb()));
    let r = run(dir.path(), &NoReader, None);
    let tables = group(&r, FindingKind::DeviceLibraryDamaged);
    assert_eq!(tables.items.len(), 17);
    assert_eq!(tables.title, "export.pdb is incomplete");

    let dir = write_stick(&with_pdb(Vec::new(), b"not a database".to_vec()));
    let r = run(dir.path(), &NoReader, None);
    assert_eq!(
        kinds(&r),
        [
            (FindingKind::DeviceLibraryUnreadable, Severity::Problem, 1),
            (FindingKind::NoOneLibrary, Severity::Warning, 0)
        ]
    );
    assert!(r.formats[0].status.starts_with("Does not open"));
}

#[test]
fn playlists_that_differ_between_the_libraries() {
    let mut rows = pdb_rows();
    rows[2].1.extend([
        playlist_node(11, 0, 1, true, "Folder"),
        playlist_node(12, 11, 0, false, "Warmup"),
        playlist_node(13, 0, 2, false, "Peak"),
    ]);
    rows[3]
        .1
        .extend([playlist_entry(1, 1, 12), playlist_entry(1, 2, 13)]);
    let dir = write_stick(&with_onelibrary(with_pdb(track_files(), full_pdb(&rows))));
    let mut one = mirror();
    one.playlists[0].track_ids = vec![2, 1];
    one.playlists.push(OneLibraryPlaylist {
        id: 13,
        parent_id: 0,
        sequence_no: 2,
        name: "Peak".into(),
        is_folder: false,
        track_ids: vec![2, 1],
    });
    one.playlists.push(OneLibraryPlaylist {
        id: 14,
        parent_id: 0,
        sequence_no: 3,
        name: "Closing".into(),
        is_folder: false,
        track_ids: vec![1],
    });
    let r = run(dir.path(), &Fake(one), Some(TargetProfile::Cdj3000));

    let items = |kind| -> Vec<(String, String)> {
        group(&r, kind)
            .items
            .iter()
            .map(|i| (i.subject.clone(), i.detail.clone().unwrap()))
            .collect()
    };
    assert_eq!(
        items(FindingKind::PlaylistInOneLibrary),
        [
            ("Folder".into(), "Only in the Device Library".into()),
            ("Closing".into(), "Only in OneLibrary".into()),
        ],
        "Folder/Warmup is covered by its folder"
    );
    assert_eq!(
        items(FindingKind::PlaylistDiffers),
        [
            ("Set".into(), "Same tracks in another order".into()),
            (
                "Peak".into(),
                "1 track in the Device Library, 2 in OneLibrary".into()
            ),
        ]
    );
    assert_eq!(r.summary.status, Status::Warnings);
}

#[test]
fn tracks_in_one_library_and_a_onelibrary_that_was_not_read() {
    let dir = write_stick(&with_onelibrary(with_pdb(
        track_files(),
        full_pdb(&pdb_rows()),
    )));
    let mut one = mirror();
    one.tracks.pop();
    one.playlists[0].track_ids = vec![1];
    let r = run(dir.path(), &Fake(one), Some(TargetProfile::Cdj3000));
    let only = group(&r, FindingKind::TrackInOneLibrary);
    assert_eq!(only.items[0].subject, "Artist – Track B");
    assert!(has(&r, FindingKind::PlaylistDiffers));

    let mut damaged = mirror();
    damaged.integrity = "*** in database main ***".into();
    let r = run(dir.path(), &Fake(damaged), Some(TargetProfile::Cdj3000));
    assert_eq!(
        kinds(&r),
        [(FindingKind::OneLibraryDamaged, Severity::Problem, 1)]
    );

    let r = run(dir.path(), &NoReader, Some(TargetProfile::Cdj3000));
    let not_read = group(&r, FindingKind::OneLibraryNotRead);
    assert_eq!(
        not_read.items[0].detail.as_deref(),
        Some("no OneLibrary key")
    );
    assert_eq!(r.formats[2].status, "Not read: no OneLibrary key");
}

#[test]
fn file_systems_per_target() {
    let dir = write_stick(&with_onelibrary(with_pdb(
        track_files(),
        full_pdb(&pdb_rows()),
    )));
    let check = |fs: FsKind, target| {
        check_stick(
            &device(dir.path(), fs),
            &CheckOptions {
                target,
                volume_serial: None,
                directory: false,
            },
            &Fake(mirror()),
            None,
        )
    };
    let r = check(FsKind::ExFat, None);
    assert_eq!(
        kinds(&r),
        [(FindingKind::FilesystemExFat, Severity::Warning, 0)]
    );
    assert!(r.filesystem.text.starts_with("exFAT · "));
    assert_eq!(
        kinds(&check(FsKind::ExFat, Some(TargetProfile::Cdj3000))),
        []
    );
    let r = check(FsKind::Other("ntfs".into()), Some(TargetProfile::Cdj3000));
    assert_eq!(
        kinds(&r),
        [(FindingKind::FilesystemUnsupported, Severity::Problem, 0)]
    );
    assert_eq!(groups(&r)[0].title, "The drive is ntfs");
}

#[test]
fn play_history_not_yet_imported() {
    let mut rows = pdb_rows();
    rows.push((
        T_HISTORY_PLAYLISTS,
        vec![history_playlist(1, "HISTORY 001")],
    ));
    rows.push((
        T_HISTORY_ENTRIES,
        vec![history_entry(1, 1, 1), history_entry(2, 1, 2)],
    ));
    let dir = write_stick(&with_onelibrary(with_pdb(track_files(), full_pdb(&rows))));
    let mut one = mirror();
    one.sessions.push(SourceSession {
        library: HistoryLibrary::OneLibrary,
        source_name: "HISTORY #1".into(),
        played_on: None,
        entries: vec![SourceEntry {
            stick_path: Some(A.into()),
            title: Some("Track A".into()),
            artist: Some("Artist".into()),
            duration_ms: Some(30_000),
        }],
    });

    let r = run(dir.path(), &Fake(one.clone()), Some(TargetProfile::Cdj3000));
    let on_stick = group(&r, FindingKind::HistoryOnStick);
    assert_eq!(on_stick.severity, Severity::Note);
    assert_eq!(on_stick.title, "2 play history sessions on the stick");
    assert_eq!(
        on_stick.items[0].detail.as_deref(),
        Some("Device Library · 2 tracks")
    );
    assert_eq!(r.summary.status, Status::Ok, "notes don't count");
    assert_eq!(r.summary.notes, 2);

    let cache = tempfile::tempdir().unwrap();
    let lib = Library::open_in_memory(cache.path()).unwrap();
    let opts = CheckOptions {
        target: Some(TargetProfile::Cdj3000),
        volume_serial: Some("ABCD-1234".into()),
        directory: false,
    };
    let dev = device(dir.path(), FsKind::Fat32);
    let r = check_stick(&dev, &opts, &Fake(one.clone()), Some(&lib));
    let pending = group(&r, FindingKind::HistoryNotImported);
    assert_eq!(pending.items.len(), 2);
    assert_eq!(
        pending.title,
        "2 play history sessions not imported into your library"
    );

    // Imported: nothing left to report.
    let mut source = crate::history::read_play_history(
        &HistorySource::Stick(dir.path().to_owned()),
        &HistoryReadOptions {
            stick_serial: opts.volume_serial.clone(),
            hist1_dates: false,
        },
    )
    .unwrap()
    .to_source_history();
    source.sessions.extend(one.sessions.clone());
    let plan = reks_core::history::scan(&source, &lib).unwrap();
    reks_core::history::import(&plan, &ImportChoices::default(), &lib).unwrap();
    let r = check_stick(&dev, &opts, &Fake(one), Some(&lib));
    assert!(!r.sections.iter().any(|s| s.section == Section::History));
}

fn reference_root() -> PathBuf {
    Path::new(FIXTURES).join("onelibrary/ref-7.2.19")
}

/// rekordbox 7.2.19's reference export with every fixture file it lists put
/// back (the fixture keeps no audio).
#[test]
fn rekordbox_reference_export() {
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut stack = vec![reference_root().join("PIONEER")];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            } else {
                let rel = p.strip_prefix(reference_root()).unwrap();
                files.push((format!("/{}", rel.display()), fs::read(&p).unwrap()));
            }
        }
    }
    let pdb = crate::pdb::read_stick(&reference_root()).unwrap();
    for t in &pdb.tracks {
        let name = t.file_path.rsplit('/').next().unwrap();
        files.push((t.file_path.clone(), fixture_audio(name)));
    }
    let refs: Vec<(&str, &[u8])> = files
        .iter()
        .map(|(p, b)| (p.trim_start_matches('/'), b.as_slice()))
        .collect();
    let dir = stick(&refs);

    let r = run(dir.path(), &NoReader, Some(TargetProfile::Cdj3000));
    assert_eq!(r.tracks_checked, 7);
    assert_eq!(r.formats[0].status, "7 tracks in 1 playlist");
    assert!(r.formats[2].present);
    // Without the key the OneLibrary is not read; the files are all sound.
    assert!(has(&r, FindingKind::OneLibraryNotRead));
    let files_problems: Vec<_> = r
        .sections
        .iter()
        .filter(|s| s.section == Section::Files)
        .flat_map(|s| &s.groups)
        .map(|g| (g.kind, g.severity))
        .collect();
    assert_eq!(files_problems, [], "{:#?}", kinds(&r));
}

/// A stick baken writes from the fixture audio, as a reks export does
/// (generated analysis).
#[test]
fn a_stick_baken_wrote_has_no_problems() {
    let tmp = tempfile::tempdir().unwrap();
    let music = tmp.path().join("music");
    let root = tmp.path().join("stick");
    let anlz_roots = tmp.path().join("no-rekordbox");
    for d in [&music, &root, &anlz_roots] {
        fs::create_dir_all(d).unwrap();
    }
    for file in [
        "sine-click-128.mp3",
        "sine-click-128.flac",
        "sine-click-128.m4a",
    ] {
        fs::write(music.join(file), fixture_audio(file)).unwrap();
    }
    let xml = fs::read_to_string(Path::new(FIXTURES).join("rekordbox/collection-small.xml"))
        .unwrap()
        .replace(
            "file://localhost/Users/reks/Music/reks%20fixtures/",
            &format!("file://localhost{}/", music.display()),
        );
    let xml_path = tmp.path().join("collection.xml");
    fs::write(&xml_path, xml).unwrap();
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

    let r = run(&root, &NoReader, Some(TargetProfile::Cdj3000));
    assert!(r.tracks_checked >= 3);
    assert_eq!(r.summary.problems, 0, "{:#?}", kinds(&r));
    assert!(has(&r, FindingKind::NoOneLibrary));
}
