//! The OneLibrary export end to end on a temp-dir stick (Phase 4 task 8): a
//! stick with rekordbox's OneLibrary files, planned with the option on and
//! the real [`OneLibraryWriter`], written by baken and the writer through
//! `apply`, then read back with the key and validated against `export.pdb`.
//! Then a player's history in that database (Phase 4 task 11): the export
//! guard stops on it, Import now takes it into the library before the backup
//! and the write, and the backup imports as already imported.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use reks_core::history::{self as library_history, HistoryLibrary, MatchKind, SessionAction};
use reks_core::rbxml::{self, ImportOptions, PrefixRemap};
use reks_core::{CancelToken, Library, OneLibraryKey};
use reks_export_onelibrary::history::read_play_history;
use reks_export_onelibrary::{
    cipher, validate, CipherProfile, Error, OneLibraryReader, OneLibraryWriter,
};
use reks_export_rb::apply::{
    apply, ApplyEnv, ApplyEvent, ApplyStage, ExportReport, ExportStatus, OneLibraryEnv, StickCheck,
};
use reks_export_rb::device::{DeviceInfo, FsKind};
use reks_export_rb::history::{HistoryPolicy, HistorySource};
use reks_export_rb::onelibrary::{ExistingLibrary, DB_PATH};
use reks_export_rb::plan::{
    self, guard_history, plan_onelibrary, ExportOptions, ExportPlan, MissingAnalysis, NoAnlz,
    Problem, Selection,
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
    /// none) and the OneLibrary option on; not blocked.
    fn plan(&self, key: &OneLibraryKey) -> ExportPlan {
        let p = self.plan_with(key, HistoryPolicy::BackUp, false);
        assert!(!p.is_blocked(), "{:?}", p.errors);
        p
    }

    /// [`Self::plan`] with a history choice, blocked or not.
    fn plan_with(&self, key: &OneLibraryKey, history: HistoryPolicy, chosen: bool) -> ExportPlan {
        let opts = ExportOptions {
            missing_analysis: MissingAnalysis::Generate,
            onelibrary: true,
            history,
            history_chosen: chosen,
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
        plan_onelibrary(&mut p, &OneLibraryWriter::default(), Some(key), &self.lib).unwrap();
        p
    }

    /// `apply` with the real writers; `Success` expected.
    fn apply(&self, p: &ExportPlan, key: &OneLibraryKey) -> (ExportReport, Vec<ApplyStage>) {
        let writer =
            BakenWriter::prepare(&self.lib, &self.sel(), p, &self.tmp.path().join("work")).unwrap();
        let data = self.tmp.path().join("data");
        let onelibrary = OneLibraryWriter::default();
        let env = ApplyEnv {
            library: &self.lib,
            app_data_dir: &data,
            stick: StickCheck::Directory,
            onelibrary: Some(OneLibraryEnv {
                export: &onelibrary,
                key,
            }),
        };
        let stages = Mutex::new(Vec::new());
        let progress = |e: ApplyEvent| {
            if let ApplyEvent::Stage { stage } = e {
                stages.lock().unwrap().push(stage);
            }
        };
        let report = apply(p, &writer, &env, &progress, &CancelToken::new()).unwrap();
        assert_eq!(
            report.status,
            ExportStatus::Success,
            "{:?} {:?}",
            report.findings(),
            report.errors
        );
        (report, stages.into_inner().unwrap())
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

    let (report, stages) = w.apply(&p, &key);
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

/// A OneLibrary player's session on a reks stick: the export guard stops on
/// it, Import now imports it in play order (exact manifest matches), the
/// backup keeps the database first, and importing the backup adds nothing.
#[test]
fn history_in_exportlibrary_is_imported_before_it_is_replaced() {
    let w = World::new();
    let key = OneLibraryKey::new(KEY).unwrap();
    w.apply(&w.plan(&key), &key);
    let db = w.stick.join(DB_PATH.trim_start_matches('/'));
    let rows = OneLibraryReader::open(&db, KEY).unwrap().library().unwrap();
    let id = |title: &str| {
        rows.contents
            .iter()
            .find(|c| c.title == title)
            .unwrap()
            .content_id
    };
    // A player plays Two, One, Two. Columns as in the reference DDL.
    let conn = cipher::open_read_write(&db, KEY, CipherProfile::Sqlcipher4).unwrap();
    conn.execute_batch(&format!(
        "INSERT INTO history VALUES (1, 0, 'HISTORY 001', 0, 0);
         INSERT INTO history_content VALUES (1, {two}, 1), (1, {one}, 2), (1, {two}, 3);",
        one = id("One"),
        two = id("Two"),
    ))
    .unwrap();
    drop(conn);

    let blocked = w.plan_with(&key, HistoryPolicy::BackUp, false);
    assert_eq!(
        blocked.errors,
        [Problem::OneLibraryHistoryNotImported { sessions: 1 }]
    );
    assert_eq!(blocked.guard.onelibrary_sessions[0].entries, 3);

    let p = w.plan_with(&key, HistoryPolicy::Import, true);
    assert!(!p.is_blocked(), "{:?}", p.errors);
    let (report, stages) = w.apply(&p, &key);
    let at = |s| stages.iter().position(|x| *x == s).unwrap();
    assert!(at(ApplyStage::ImportHistory) < at(ApplyStage::Backup));
    assert!(at(ApplyStage::Backup) < at(ApplyStage::Write));
    let imported = report.history_import.as_ref().unwrap();
    assert_eq!((imported.created.len(), imported.entries), (1, 3));
    assert_eq!(imported.unmatched, 0);
    let session = w.lib.session(imported.created[0]).unwrap();
    assert_eq!(session.session.library, HistoryLibrary::OneLibrary);
    assert_eq!(session.session.source_name, "HISTORY 001");
    let titles: Vec<&str> = session
        .entries
        .iter()
        .map(|e| e.title.as_deref().unwrap())
        .collect();
    assert_eq!(titles, ["Two", "One", "Two"]);

    // The new database has no history; the backup kept the old one, and
    // importing it adds nothing.
    let after = OneLibraryReader::open(&db, KEY).unwrap();
    assert!(after.play_history().unwrap().is_empty());
    let backup = HistorySource::Backup(report.backup_dir.clone().unwrap());
    let backed_up = read_play_history(&backup, KEY, None).unwrap();
    assert_eq!(backed_up.volume_serial.as_deref(), Some(SERIAL));
    let plan = library_history::scan(&backed_up, &w.lib).unwrap();
    assert_eq!(plan.sessions.len(), 1);
    assert!(matches!(plan.sessions[0].action, SessionAction::Skip(_)));
    // Fresh matching of the backup's entries: exact, through the manifest.
    let matched =
        library_history::match_entries(Some(SERIAL), &backed_up.sessions[0].entries, &w.lib)
            .unwrap();
    assert!(matched
        .iter()
        .all(|m| m.matched_by == Some(MatchKind::Manifest)));

    // And the next export needs no choice.
    assert!(!w.plan(&key).guard.needs_choice());
}
