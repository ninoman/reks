//! The OneLibrary export end to end on a temp-dir stick (Phase 4 task 8): a
//! stick with rekordbox's OneLibrary files, planned with the option on and
//! the real [`OneLibraryWriter`], written by baken and the writer through
//! `apply`, then read back with the key and validated against `export.pdb`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use reks_core::rbxml::{self, ImportOptions, PrefixRemap};
use reks_core::{CancelToken, Library, OneLibraryKey};
use reks_export_onelibrary::{validate, Error, OneLibraryReader, OneLibraryWriter};
use reks_export_rb::apply::{
    apply, ApplyEnv, ApplyEvent, ApplyStage, ExportStatus, OneLibraryEnv, StickCheck,
};
use reks_export_rb::device::{DeviceInfo, FsKind};
use reks_export_rb::onelibrary::{ExistingLibrary, DB_PATH};
use reks_export_rb::plan::{
    self, guard_history, plan_onelibrary, ExportOptions, ExportPlan, MissingAnalysis, NoAnlz,
    Selection,
};
use reks_export_rb::writer::BakenWriter;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/audio");
const XML_PREFIX: &str = "/Users/reks/Music/reks onelibrary";
const SERIAL: &str = "4993-1CFB";
/// A made-up key: the writer takes any passphrase.
const KEY: &str = "reks-test-key";

/// (fixture, file name, title)
const TRACKS: [(&str, &str, &str); 2] = [
    ("sine-click-128.mp3", "one.mp3", "One"),
    ("sine-click-128.flac", "two.flac", "Two"),
];

struct World {
    tmp: tempfile::TempDir,
    lib: Library,
    stick: PathBuf,
}

impl World {
    /// [`TRACKS`] in playlist `Set`; a stick holding what rekordbox 7 leaves
    /// in `PIONEER/rekordbox` besides `export.pdb`.
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let music = tmp.path().join("music");
        let stick = tmp.path().join("stick");
        let rb = stick.join("PIONEER/rekordbox");
        fs::create_dir_all(&music).unwrap();
        fs::create_dir_all(&rb).unwrap();
        for (fixture, name, _) in TRACKS {
            fs::copy(Path::new(FIXTURES).join(fixture), music.join(name)).unwrap();
        }
        for name in ["exportLibrary.db", "exportLibrary.db-wal", "exportExt.pdb"] {
            fs::write(rb.join(name), format!("rekordbox's {name}")).unwrap();
        }
        let lib =
            Library::open(&tmp.path().join("reks.sqlite"), &tmp.path().join("cache")).unwrap();
        let c = rbxml::parse(Cursor::new(collection_xml())).unwrap();
        let opts = ImportOptions {
            remap: vec![PrefixRemap {
                from: XML_PREFIX.into(),
                to: music,
            }],
            ..ImportOptions::default()
        };
        rbxml::import(&c, &lib, opts, |_| {}).unwrap();
        World { tmp, lib, stick }
    }

    fn sel(&self) -> Selection {
        let tree = self.lib.playlists().unwrap();
        Selection {
            playlists: vec![tree.nodes.iter().find(|n| n.name == "Set").unwrap().id],
        }
    }

    fn dev(&self) -> DeviceInfo {
        DeviceInfo {
            mount_point: self.stick.clone(),
            fs: FsKind::Fat32,
            fs_name: "msdos".into(),
            removable: true,
            total_bytes: 32 << 30,
            free_bytes: 16 << 30,
            label: "REKS".into(),
            os_name: "disk9s1".into(),
        }
    }

    /// The steps `dryrun::plan_at` runs, with a known serial (a temp dir has
    /// none) and the OneLibrary option on.
    fn plan(&self, key: &OneLibraryKey) -> ExportPlan {
        let opts = ExportOptions {
            missing_analysis: MissingAnalysis::Generate,
            onelibrary: true,
            ..ExportOptions::default()
        };
        let sources = plan::load(&self.lib, &self.sel()).unwrap();
        let mut p = plan::plan_sources(
            sources,
            &self.dev(),
            Some(SERIAL.into()),
            &opts,
            &NoAnlz,
            &NoAnlz,
        )
        .unwrap();
        guard_history(&mut p, &self.lib).unwrap();
        plan_onelibrary(&mut p, &OneLibraryWriter::default(), Some(key)).unwrap();
        assert!(!p.is_blocked(), "{:?}", p.errors);
        p
    }
}

fn collection_xml() -> String {
    let prefix = XML_PREFIX.replace(' ', "%20");
    let mut tracks = String::new();
    let mut keys = String::new();
    for (i, (_, name, title)) in TRACKS.iter().enumerate() {
        let id = i + 1;
        tracks += &format!(
            r#"<TRACK TrackID="{id}" Name="{title}" Artist="Alpha" Album="Album" Genre="Techno" Kind="" TotalTime="30" AverageBpm="128.00" Tonality="Am" Location="file://localhost{prefix}/{name}">
  <TEMPO Inizio="0.000" Bpm="128.00" Metro="4/4" Battito="1"/>
</TRACK>
"#
        );
        keys += &format!(r#"<TRACK Key="{id}"/>"#);
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<DJ_PLAYLISTS Version="1.0.0">
<PRODUCT Name="rekordbox" Version="7.0.0" Company="AlphaTheta"/>
<COLLECTION Entries="{n}">
{tracks}</COLLECTION>
<PLAYLISTS>
<NODE Type="0" Name="ROOT" Count="1">
  <NODE Name="Set" Type="1" KeyType="0" Entries="{n}">{keys}</NODE>
</NODE>
</PLAYLISTS>
</DJ_PLAYLISTS>
"#,
        n = TRACKS.len()
    )
}

/// The file names in the stick's `PIONEER/rekordbox`, sorted.
fn rekordbox_files(stick: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(stick.join("PIONEER/rekordbox"))
        .unwrap()
        .flatten()
        .filter(|e| e.path().is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn apply_writes_an_exportlibrary_that_matches_export_pdb() {
    let w = World::new();
    let key = OneLibraryKey::new(KEY).unwrap();
    let p = w.plan(&key);
    // rekordbox's database does not open with this key: the Phase 2
    // safeguard backs it up instead of the export guard asking.
    assert!(matches!(
        p.onelibrary.as_ref().unwrap().existing,
        ExistingLibrary::NotRead { .. }
    ));

    let writer = BakenWriter::prepare(&w.lib, &w.sel(), &p, &w.tmp.path().join("work")).unwrap();
    let data = w.tmp.path().join("data");
    let onelibrary = OneLibraryWriter::default();
    let env = ApplyEnv {
        library: &w.lib,
        app_data_dir: &data,
        stick: StickCheck::Directory,
        onelibrary: Some(OneLibraryEnv {
            export: &onelibrary,
            key: &key,
        }),
    };
    let stages = Mutex::new(Vec::new());
    let progress = |e: ApplyEvent| {
        if let ApplyEvent::Stage { stage } = e {
            stages.lock().unwrap().push(stage);
        }
    };
    let report = apply(&p, &writer, &env, &progress, &CancelToken::new()).unwrap();
    assert_eq!(
        report.status,
        ExportStatus::Success,
        "{:?} {:?}",
        report.findings(),
        report.errors
    );
    let stages = stages.into_inner().unwrap();
    let at = |s| stages.iter().position(|x| *x == s).unwrap();
    assert!(at(ApplyStage::Write) < at(ApplyStage::OneLibrary));
    assert!(at(ApplyStage::OneLibrary) < at(ApplyStage::Validate));

    // Our two databases and nothing of rekordbox's: no -wal or -shm SQLite
    // would replay, no exportExt.pdb, no macOS `._` files.
    assert_eq!(
        rekordbox_files(&w.stick),
        ["export.pdb", "exportLibrary.db"]
    );
    let written = &report.onelibrary.as_ref().unwrap().written;
    assert_eq!(written.tracks, TRACKS.len());
    let db = w.stick.join(DB_PATH.trim_start_matches('/'));
    assert_eq!(written.bytes, fs::metadata(&db).unwrap().len());
    // rekordbox's database went to the backup before the stick was touched.
    assert_eq!(
        fs::read(
            report
                .backup_dir
                .as_ref()
                .unwrap()
                .join("PIONEER/rekordbox/exportLibrary.db")
        )
        .unwrap(),
        b"rekordbox's exportLibrary.db"
    );

    // It opens with the key only, and agrees with export.pdb and the ANLZ.
    let rows = OneLibraryReader::open(&db, KEY).unwrap().library().unwrap();
    let mut titles: Vec<&str> = rows.contents.iter().map(|c| c.title.as_str()).collect();
    titles.sort_unstable();
    assert_eq!(titles, ["One", "Two"]);
    assert_eq!(rows.playlist_entries.len(), TRACKS.len());
    assert!(matches!(
        OneLibraryReader::open(&db, "wrong-key"),
        Err(Error::NotDecrypted { .. })
    ));
    let findings = validate::validate(&w.stick, KEY).unwrap();
    assert!(findings.is_empty(), "{findings:?}");
}
