//! rekordbox XML import into a library: the small fixture, idempotency,
//! unmatched tracks, prefix remap, preview, cancel and a 3,000-track run.

// Test helpers outside `#[test]` fns are not covered by clippy's allow-unwrap-in-tests.
#![allow(clippy::unwrap_used)]

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::time::Instant;

use reks_core::rbxml::{
    self, import, path_to_location, preview, Collection, Extra, ImportEvent, ImportOptions,
    ImportReport, PositionMark, PrefixRemap, Tempo, UnmatchedReason, XmlNode, XmlTrack,
};
use reks_core::{scan, CancelToken, DuplicateMode, Library, PlaylistOp, PlaylistOutcome, TagOp};
use rusqlite::types::Value;

const XML_PREFIX: &str = "/Users/reks/Music/reks fixtures";

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn audio_dir() -> PathBuf {
    std::fs::canonicalize(fixtures().join("audio")).unwrap()
}

fn small_collection() -> Collection {
    let f = std::fs::File::open(fixtures().join("rekordbox/collection-small.xml")).unwrap();
    rbxml::parse(std::io::BufReader::new(f)).unwrap()
}

fn fixture_remap() -> ImportOptions {
    ImportOptions {
        remap: vec![PrefixRemap {
            from: XML_PREFIX.into(),
            to: audio_dir(),
        }],
        ..ImportOptions::default()
    }
}

struct Env {
    _dir: tempfile::TempDir,
    lib: Library,
}

impl Env {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let lib =
            Library::open(&dir.path().join("reks.sqlite"), &dir.path().join("cache")).unwrap();
        Env { _dir: dir, lib }
    }

    fn import(&self, c: &Collection, opts: ImportOptions) -> ImportReport {
        import(c, &self.lib, opts, |_| {}).unwrap()
    }

    /// A read-only connection to the library file (WAL allows it alongside
    /// the library's own connection).
    fn db(&self) -> rusqlite::Connection {
        rusqlite::Connection::open(self._dir.path().join("reks.sqlite")).unwrap()
    }

    fn count(&self, table: &str) -> i64 {
        self.db()
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }

    fn track_id(&self, rb_track_id: u32) -> i64 {
        self.db()
            .query_row(
                "SELECT id FROM track WHERE rb_track_id = ?1",
                [rb_track_id],
                |r| r.get(0),
            )
            .unwrap()
    }

    /// Every row of the import-touched tables, minus timestamps.
    fn snapshot(&self) -> Vec<Vec<Value>> {
        let db = self.db();
        let mut out = Vec::new();
        for sql in [
            "SELECT * FROM track ORDER BY id",
            "SELECT * FROM beatgrid ORDER BY id",
            "SELECT * FROM cue ORDER BY id",
            "SELECT id, parent_id, is_folder, name, sort_order FROM playlist ORDER BY id",
            "SELECT * FROM playlist_track ORDER BY playlist_id, position",
        ] {
            let mut stmt = db.prepare(sql).unwrap();
            let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
            let mut rows = stmt.query([]).unwrap();
            while let Some(r) = rows.next().unwrap() {
                let mut row = vec![Value::Text(sql.into())];
                for (i, name) in names.iter().enumerate() {
                    if name != "created_at" && name != "updated_at" {
                        row.push(r.get(i).unwrap());
                    }
                }
                out.push(row);
            }
        }
        out
    }

    fn playlist_tracks(&self, name: &str) -> Vec<u32> {
        let db = self.db();
        let mut stmt = db
            .prepare(
                "SELECT t.rb_track_id FROM playlist_track pt
                 JOIN playlist p ON p.id = pt.playlist_id JOIN track t ON t.id = pt.track_id
                 WHERE p.name = ?1 ORDER BY pt.position",
            )
            .unwrap();
        stmt.query_map([name], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }
}

#[derive(Debug, PartialEq)]
struct Cue {
    slot: Option<i64>,
    kind: i64,
    start_ms: f64,
    end_ms: Option<f64>,
    name: Option<String>,
    color_rgb: Option<i64>,
}

fn cues(env: &Env, rb_track_id: u32) -> Vec<Cue> {
    let db = env.db();
    let mut stmt = db
        .prepare(
            "SELECT slot, kind, start_ms, end_ms, name, color_rgb FROM cue
             WHERE track_id = ?1 ORDER BY id",
        )
        .unwrap();
    stmt.query_map([env.track_id(rb_track_id)], |r| {
        Ok(Cue {
            slot: r.get(0)?,
            kind: r.get(1)?,
            start_ms: r.get(2)?,
            end_ms: r.get(3)?,
            name: r.get(4)?,
            color_rgb: r.get(5)?,
        })
    })
    .unwrap()
    .collect::<Result<_, _>>()
    .unwrap()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1.0
}

#[test]
fn imports_small_fixture() {
    let env = Env::new();
    let events = RefCell::new(Vec::new());
    let report = import(&small_collection(), &env.lib, fixture_remap(), |e| {
        events.borrow_mut().push(e.clone())
    })
    .unwrap();

    assert_eq!(report.total, 3);
    assert_eq!(report.added, 3);
    assert_eq!(report.missing, 0);
    assert!(report.unmatched.is_empty(), "{:?}", report.unmatched);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert_eq!(report.beatgrid_rows, 3);
    assert_eq!(report.cue_rows, 10);
    assert_eq!(report.playlists_created, 3);
    assert_eq!(report.playlist_entries, 4);
    assert!(matches!(
        events.borrow().last(),
        Some(ImportEvent::Progress {
            done: 3,
            total: 3,
            ..
        })
    ));
    assert_eq!(env.count("track"), 3);

    // Track fields.
    let db = env.db();
    type Fields = (
        String,
        i64,
        String,
        i64,
        Option<String>,
        String,
        String,
        i64,
        i64,
        String,
        i64,
        String,
    );
    let row = |rb: u32| -> Fields {
        db.query_row(
            "SELECT path, missing, kind, rating, color, key, key_source, bpm_x100, play_count,
                    date_added, duration_ms, rb_location
             FROM track WHERE rb_track_id = ?1",
            [rb],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                    r.get(8)?,
                    r.get(9)?,
                    r.get(10)?,
                    r.get(11)?,
                ))
            },
        )
        .unwrap()
    };
    let t1 = row(1);
    assert_eq!(
        t1.0,
        audio_dir().join("sine-click-128.mp3").to_str().unwrap()
    );
    assert_eq!((t1.1, t1.2.as_str(), t1.3), (0, "mp3", 5));
    assert_eq!(t1.4.as_deref(), Some("pink"));
    assert_eq!((t1.5.as_str(), t1.6.as_str()), ("Am", "rekordbox"));
    assert_eq!((t1.7, t1.8, t1.9.as_str()), (12800, 3, "2026-10-19"));
    assert!((29_900..30_200).contains(&t1.10), "duration {}", t1.10);
    assert_eq!(
        t1.11,
        "file://localhost/Users/reks/Music/reks%20fixtures/sine-click-128.mp3"
    );
    let t2 = row(2);
    assert_eq!(
        (t2.2.as_str(), t2.3, t2.4.as_deref()),
        ("flac", 3, Some("green"))
    );
    let t3 = row(3);
    assert_eq!((t3.2.as_str(), t3.3, t3.4.as_deref()), ("aac", 0, None));
    let (title, artist, comment, genre): (String, String, String, String) = db
        .query_row(
            "SELECT title, artist, comment, genre FROM track WHERE rb_track_id = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        (
            title.as_str(),
            artist.as_str(),
            comment.as_str(),
            genre.as_str()
        ),
        (
            "Sine Click 128 (MP3)",
            "reks fixtures",
            "Generated test signal",
            "Techno"
        )
    );
    // Empty XML attributes stay NULL.
    let composer: Option<String> = db
        .query_row(
            "SELECT composer FROM track WHERE rb_track_id = 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(composer, None);

    // Grid: one anchor per track.
    let grid: (f64, i64, i64, i64, i64, String) = db
        .query_row(
            "SELECT position_ms, bpm_x100, meter_num, meter_den, beat_in_bar, source
             FROM beatgrid WHERE track_id = ?1",
            [env.track_id(1)],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )
        .unwrap();
    assert!(close(grid.0, 0.0));
    assert_eq!((grid.1, grid.2, grid.3, grid.4), (12800, 4, 4, 1));
    assert_eq!(grid.5, "rekordbox");

    // Cues of track 1: two memory cues, hot cues A/B, loop on C.
    let c = cues(&env, 1);
    assert_eq!(c.len(), 5);
    assert_eq!(
        (c[0].slot, c[0].kind, c[0].name.as_deref()),
        (None, 0, None)
    );
    assert!(close(c[0].start_ms, 0.0));
    assert_eq!(c[1].slot, None);
    assert!(close(c[1].start_ms, 15_000.0));
    assert_eq!(
        (c[2].slot, c[2].name.as_deref(), c[2].color_rgb),
        (Some(0), Some("Intro"), Some(0x28E214))
    );
    assert_eq!(
        (c[3].slot, c[3].name.as_deref(), c[3].color_rgb),
        (Some(1), Some("Break"), Some(0xFF8C00))
    );
    assert!(close(c[3].start_ms, 7_500.0));
    let lp = &c[4];
    assert_eq!(
        (lp.slot, lp.kind, lp.name.as_deref()),
        (Some(2), 4, Some("Loop"))
    );
    assert!(close(lp.start_ms, 15_000.0));
    assert!(close(lp.end_ms.unwrap(), 16_875.0));
    assert_eq!(cues(&env, 2).len(), 2);
    assert_eq!(cues(&env, 3).len(), 3);
    assert!(cues(&env, 2).iter().all(|c| c.end_ms.is_none()));

    // Playlist tree.
    let tree: Vec<(String, Option<String>, i64)> = {
        let mut stmt = db
            .prepare(
                "SELECT p.name, parent.name, p.is_folder FROM playlist p
                 LEFT JOIN playlist parent ON parent.id = p.parent_id ORDER BY p.id",
            )
            .unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    assert_eq!(
        tree,
        vec![
            ("reks fixtures".into(), None, 1),
            ("All fixtures".into(), Some("reks fixtures".into()), 0),
            ("Lossless".into(), Some("reks fixtures".into()), 0),
        ]
    );
    assert_eq!(env.playlist_tracks("All fixtures"), vec![1, 2, 3]);
    assert_eq!(env.playlist_tracks("Lossless"), vec![2]);
}

#[test]
fn second_import_changes_nothing_and_keeps_user_data() {
    let env = Env::new();
    let c = small_collection();
    env.import(&c, fixture_remap());
    let first = env.snapshot();

    let again = env.import(&c, fixture_remap());
    assert_eq!(again.matched_by_path, 3);
    assert_eq!(again.added, 0);
    assert_eq!(again.playlists_created, 0);
    assert_eq!(again.playlists_matched, 3);
    assert_eq!(env.snapshot(), first);

    // User data added in reks survives a re-import.
    let t2 = env.track_id(2);
    let db = env.db();
    db.execute(
        "INSERT INTO cue (track_id, slot, kind, start_ms, source) VALUES (?1, 5, 0, 1234.0, 'manual')",
        [t2],
    )
    .unwrap();
    db.execute(
        "INSERT INTO playlist_track (playlist_id, track_id, position)
         SELECT id, ?1, 1 FROM playlist WHERE name = 'Lossless'",
        [env.track_id(1)],
    )
    .unwrap();
    db.execute("UPDATE track SET album_artist = 'Mine' WHERE id = ?1", [t2])
        .unwrap();
    let with_user = env.snapshot();
    env.import(&c, fixture_remap());
    assert_eq!(env.snapshot(), with_user);
    assert_eq!(env.playlist_tracks("Lossless"), vec![2, 1]);
    assert_eq!(cues(&env, 2).len(), 3);
}

/// D23: `PlayCount` / `LastPlayed` set the baseline of new tracks only; a
/// re-import never moves an existing track's baseline, and the cached play
/// count stays baseline + included sessions.
#[test]
fn reimport_keeps_the_play_baseline_frozen() {
    let env = Env::new();
    let mut c = small_collection();
    c.tracks[0].last_played = Some("2026-09-01".into());
    env.import(&c, fixture_remap());
    let t1 = env.track_id(1);
    let stats = |id: i64| -> (i64, i64, Option<String>, Option<String>) {
        env.db()
            .query_row(
                "SELECT play_count, baseline_play_count, last_played, baseline_last_played
                 FROM track WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap()
    };
    let d = |s: &str| Some(s.to_owned());
    assert_eq!(stats(t1), (3, 3, d("2026-09-01"), d("2026-09-01")));

    // One included session with t1 played twice.
    let db = env.db();
    db.execute(
        "INSERT INTO play_session (id, source, fingerprint, source_name, name, played_on,
                                   imported_at, updated_at)
         VALUES (1, 'device_library', 'f', 'HISTORY 001', 'HISTORY 001', '2026-10-10', 'x', 'x')",
        [],
    )
    .unwrap();
    db.execute(
        "INSERT INTO play_session_entry (session_id, position, track_id)
         VALUES (1, 0, ?1), (1, 1, ?1)",
        [t1],
    )
    .unwrap();
    drop(db);

    // rekordbox has counted the same stick meanwhile; a new track arrives too.
    c.tracks[0].play_count = Some(9);
    c.tracks[0].last_played = Some("2026-10-10".into());
    let dir = tempfile::tempdir().unwrap();
    let new_path = dir.path().join("new.mp3");
    link(&audio_dir().join("sine-click-128.mp3"), &new_path);
    c.tracks.push(XmlTrack {
        play_count: Some(4),
        last_played: Some("2026-08-08".into()),
        ..xml_track(50, &new_path)
    });
    env.import(&c, fixture_remap());
    assert_eq!(stats(t1), (5, 3, d("2026-10-10"), d("2026-09-01")));
    let t50 = env.track_id(50);
    assert_eq!(stats(t50), (4, 4, d("2026-08-08"), d("2026-08-08")));
}

#[test]
fn reimport_after_playlist_edits_still_matches() {
    let env = Env::new();
    let c = small_collection();
    env.import(&c, fixture_remap());
    let id = |name: &str| -> i64 {
        env.db()
            .query_row("SELECT id FROM playlist WHERE name = ?1", [name], |r| {
                r.get(0)
            })
            .unwrap()
    };
    let (folder, all, lossless) = (id("reks fixtures"), id("All fixtures"), id("Lossless"));
    let (t1, t2, t3) = (env.track_id(1), env.track_id(2), env.track_id(3));
    let mutate = |op| env.lib.playlist_mutate(op).unwrap();

    // Edits in reks: reorder, insert, a second copy, a sibling move, a new playlist.
    mutate(PlaylistOp::ReorderEntries {
        playlist_id: all,
        positions: vec![2],
        before: 0,
    });
    mutate(PlaylistOp::AddTracks {
        playlist_id: lossless,
        track_ids: vec![t1, t2],
        before: Some(0),
        duplicates: DuplicateMode::Allow,
    });
    mutate(PlaylistOp::Move {
        id: lossless,
        parent_id: Some(folder),
        position: 0,
    });
    let PlaylistOutcome::Done {
        created_id: Some(mine),
        ..
    } = mutate(PlaylistOp::CreatePlaylist {
        parent_id: Some(folder),
        name: "Mine".into(),
        position: Some(1),
    })
    else {
        panic!("no id");
    };
    mutate(PlaylistOp::AddTracks {
        playlist_id: mine,
        track_ids: vec![t3],
        before: None,
        duplicates: DuplicateMode::Ask,
    });
    assert_eq!(env.playlist_tracks("Lossless"), vec![1, 2, 2]);

    let again = env.import(&c, fixture_remap());
    assert_eq!(again.playlists_created, 0);
    assert_eq!(again.playlists_matched, 3);
    assert_eq!(id("Lossless"), lossless);
    // XML order wins; what reks added (including the second copy) follows it.
    assert_eq!(env.playlist_tracks("All fixtures"), vec![1, 2, 3]);
    assert_eq!(env.playlist_tracks("Lossless"), vec![2, 1, 2]);
    assert_eq!(env.playlist_tracks("Mine"), vec![3]);
    let order: Vec<i64> = env
        .lib
        .playlists()
        .unwrap()
        .nodes
        .iter()
        .map(|n| n.id)
        .collect();
    assert_eq!(order, vec![folder, lossless, mine, all]);
}

#[test]
fn comment_tags_become_my_tags() {
    let env = Env::new();
    // An existing tag of the same name is reused, whatever its category.
    let genre = env.lib.tags().unwrap().categories[0].id;
    let deep = env
        .lib
        .tag_mutate(TagOp::CreateTag {
            category_id: genre,
            name: "Deep".into(),
            position: None,
        })
        .unwrap()
        .created_id
        .unwrap();
    let mut c = small_collection();
    c.tracks[0].comments = Some("ok /* deep / Peak */".into());
    c.tracks[1].comments = Some(" /* Peak */".into());
    let with_tags = || ImportOptions {
        comment_tags: true,
        ..fixture_remap()
    };
    let preview = rbxml::preview(&c, &env.lib, &with_tags()).unwrap();
    assert_eq!(
        (
            preview.comment_tags.tracks,
            preview.comment_tags.names,
            preview.comment_tags.new_names
        ),
        (2, 2, 1)
    );
    let report = env.import(&c, with_tags());
    let tags = report.comment_tags.unwrap();
    assert_eq!((tags.tracks, tags.created, tags.assigned), (2, 1, 3));

    let tree = env.lib.tags().unwrap();
    let imported = tree.categories.last().unwrap();
    assert_eq!(imported.name, rbxml::IMPORTED_TAG_CATEGORY);
    assert_eq!(imported.tags.len(), 1);
    assert_eq!(
        (imported.tags[0].name.as_str(), imported.tags[0].track_count),
        ("Peak", 2)
    );
    let peak = imported.tags[0].id;
    assert_eq!(tags.tag_ids, vec![deep, peak]);
    let first = env.lib.track(env.track_id(c.tracks[0].track_id)).unwrap();
    assert_eq!(first.track.row.tags, vec![deep, peak]);
    // The comment is stored as it is.
    assert_eq!(
        first.track.row.comment.as_deref(),
        Some("ok /* deep / Peak */")
    );

    // A second import adds nothing.
    let again = env.import(&c, with_tags()).comment_tags.unwrap();
    assert_eq!((again.created, again.assigned), (0, 0));
}

#[test]
fn comment_tags_are_skipped_when_not_asked_for() {
    let env = Env::new();
    let mut c = small_collection();
    c.tracks[0].comments = Some("ok /* Deep / Peak */".into());
    let report = env.import(&c, fixture_remap());
    assert_eq!(report.comment_tags, None);
    let tree = env.lib.tags().unwrap();
    assert!(tree.categories.iter().all(|cat| cat.tags.is_empty()));
    // The comment is still stored, so the tags can be read later.
    let id = env.track_id(c.tracks[0].track_id);
    assert_eq!(
        env.lib.track(id).unwrap().track.row.comment.as_deref(),
        Some("ok /* Deep / Peak */")
    );
    let later = env.lib.comment_tags_import().unwrap();
    assert_eq!((later.tracks, later.created, later.assigned), (1, 2, 2));
}

#[test]
fn unmatched_tracks_are_reported() {
    let env = Env::new();
    let mut c = small_collection();
    let mut gone = c.tracks[0].clone();
    gone.track_id = 10;
    gone.location = Some("file://localhost/nowhere/reks/missing%20track.mp3".into());
    let mut stream = c.tracks[0].clone();
    stream.track_id = 11;
    stream.location = Some("file://localhostsoundcloud:tracks:123".into());
    let mut no_loc = c.tracks[0].clone();
    no_loc.track_id = 12;
    no_loc.location = None;
    let mut bad = c.tracks[0].clone();
    bad.track_id = 13;
    bad.location = Some("file://localhost/bad%FF.mp3".into());
    let mut dup = c.tracks[1].clone();
    dup.track_id = 14;
    c.tracks.extend([gone, stream, no_loc, bad, dup]);
    c.playlists = XmlNode::folder(
        "ROOT",
        vec![XmlNode::playlist(
            "Mixed",
            0,
            ["10", "11", "1", "99"].map(String::from).to_vec(),
        )],
    );

    let report = env.import(&c, fixture_remap());
    let reasons: Vec<(u32, &UnmatchedReason)> = report
        .unmatched
        .iter()
        .map(|u| (u.track_id, &u.reason))
        .collect();
    assert_eq!(reasons.len(), 5, "{reasons:?}");
    assert!(matches!(
        reasons[0],
        (10, UnmatchedReason::FileNotFound { path })
            if Path::new(path) == Path::new("/nowhere/reks/missing track.mp3")
    ));
    assert_eq!(reasons[1], (11, &UnmatchedReason::NotAFile));
    assert_eq!(reasons[2], (12, &UnmatchedReason::NoLocation));
    assert!(matches!(
        reasons[3],
        (13, UnmatchedReason::InvalidLocation { .. })
    ));
    assert_eq!(
        reasons[4],
        (14, &UnmatchedReason::DuplicateOf { track_id: 2 })
    );
    assert_eq!((report.added, report.missing), (4, 1));

    // The missing file is stored with its cues, flagged missing.
    let (missing, size, mtime, kind): (i64, i64, String, String) = env
        .db()
        .query_row(
            "SELECT missing, file_size, file_mtime, kind FROM track WHERE rb_track_id = 10",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        (missing, size, mtime.as_str(), kind.as_str()),
        (1, 240_605, "", "mp3")
    );
    assert_eq!(cues(&env, 10).len(), 5);
    assert_eq!(env.count("track"), 4);
    // Entries for tracks without a row are skipped.
    assert_eq!(env.playlist_tracks("Mixed"), vec![10, 1]);
    assert_eq!(report.playlist_entries_skipped, 2);
}

#[test]
fn falls_back_to_artist_title_size() {
    let env = Env::new();
    // The FLAC is already in the library from a folder scan (tags: title
    // "Sine Click 128", artist "reks fixtures").
    let music = env._dir.path().join("music");
    std::fs::create_dir_all(&music).unwrap();
    let flac = music.join("scanned.flac");
    std::fs::copy(audio_dir().join("sine-click-128.flac"), &flac).unwrap();
    scan(
        std::slice::from_ref(&music),
        &env.lib,
        &CancelToken::new(),
        |_| {},
    )
    .unwrap();
    let scanned_id: i64 = env
        .db()
        .query_row("SELECT id FROM track", [], |r| r.get(0))
        .unwrap();

    let mut c = small_collection();
    c.tracks.truncate(2);
    c.tracks.remove(0);
    let t = &mut c.tracks[0];
    t.name = Some("SINE CLICK 128".into());
    t.location = Some("file://localhost/old/drive/sine-click-128.flac".into());
    c.playlists = XmlNode::folder("ROOT", Vec::new());

    let report = env.import(&c, ImportOptions::default());
    assert_eq!(report.matched_by_fallback, 1);
    assert_eq!(report.added, 0);
    assert!(report.unmatched.is_empty(), "{:?}", report.unmatched);
    assert_eq!(env.count("track"), 1);
    assert_eq!(env.track_id(2), scanned_id);
    let (path, missing, rating): (String, i64, i64) = env
        .db()
        .query_row("SELECT path, missing, rating FROM track", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .unwrap();
    assert_eq!(path, flac.to_str().unwrap());
    assert_eq!((missing, rating), (0, 3));
    assert_eq!(cues(&env, 2).len(), 2);
}

#[test]
fn prefix_remap_relinks_tracks_stored_as_missing() {
    let env = Env::new();
    let c = small_collection();
    let before = env.import(&c, ImportOptions::default());
    assert_eq!((before.added, before.missing), (3, 3));
    assert_eq!(before.unmatched.len(), 3);
    // The suggested remap source is the XML's music folder.
    assert_eq!(
        before.missing_root.as_deref().map(Path::new),
        Some(Path::new(XML_PREFIX))
    );
    let missing: i64 = env
        .db()
        .query_row("SELECT sum(missing) FROM track", [], |r| r.get(0))
        .unwrap();
    assert_eq!(missing, 3);
    let ids: Vec<i64> = (1..=3).map(|i| env.track_id(i)).collect();

    let after = env.import(&c, fixture_remap());
    assert_eq!(after.matched_by_location, 3);
    assert_eq!((after.added, after.missing), (0, 0));
    assert!(after.unmatched.is_empty());
    assert_eq!(after.missing_root, None);
    assert_eq!(env.count("track"), 3);
    assert_eq!((1..=3).map(|i| env.track_id(i)).collect::<Vec<_>>(), ids);
    let (path, missing, kind, size): (String, i64, String, i64) = env
        .db()
        .query_row(
            "SELECT path, missing, kind, file_size FROM track WHERE rb_track_id = 2",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        path,
        audio_dir().join("sine-click-128.flac").to_str().unwrap()
    );
    assert_eq!((missing, kind.as_str(), size), (0, "flac", 480_894));
    assert_eq!(env.count("cue"), 10);
    assert_eq!(env.count("beatgrid"), 3);
    assert_eq!(env.count("playlist"), 3);
}

/// 3,000 tracks over hard links to the fixture files, 1 in 20 missing.
/// Logs the time; the acceptance target is 60 s for a real collection.
#[test]
fn preview_counts_without_writing() {
    let env = Env::new();
    let c = small_collection();
    let before = env.snapshot();

    let p = preview(&c, &env.lib, &ImportOptions::default()).unwrap();
    assert_eq!(p.total, 3);
    assert_eq!((p.folders, p.playlists, p.playlist_entries), (1, 2, 4));
    assert_eq!((p.with_grid, p.with_cues), (3, 3));
    assert_eq!((p.files_found, p.files_not_found, p.in_library), (0, 3, 0));
    assert_eq!(
        p.missing_root.as_deref().map(Path::new),
        Some(Path::new(XML_PREFIX))
    );
    assert_eq!(p.not_found_sample.len(), 3);
    assert_eq!(env.snapshot(), before);
    assert_eq!(env.count("track"), 0);

    // The remap finds the files; nothing is missing, so no root to suggest.
    let p = preview(&c, &env.lib, &fixture_remap()).unwrap();
    assert_eq!((p.files_found, p.files_not_found), (3, 0));
    assert_eq!(p.missing_root, None);
    assert!(p.not_found_sample.is_empty());

    // After an import the same tracks are in the library.
    let report = env.import(&c, fixture_remap());
    let p = preview(&c, &env.lib, &fixture_remap()).unwrap();
    let again = env.import(&c, fixture_remap());
    assert_eq!(report.added, 3);
    assert_eq!(
        p.in_library,
        again.matched_by_path + again.matched_by_location + again.matched_by_fallback
    );
    assert_eq!(p.in_library, 3);
}

#[test]
fn preview_counts_entries_that_are_not_stored() {
    let env = Env::new();
    let mut c = small_collection();
    let mut stream = c.tracks[0].clone();
    stream.location = Some("file://localhostsoundcloud:tracks:123".into());
    let mut no_loc = c.tracks[0].clone();
    no_loc.location = None;
    let mut bad = c.tracks[0].clone();
    bad.location = Some("file://localhost/bad%FF.mp3".into());
    c.tracks.extend([stream, no_loc, bad]);

    let p = preview(&c, &env.lib, &fixture_remap()).unwrap();
    assert_eq!(p.total, 6);
    assert_eq!((p.not_a_file, p.no_location, p.invalid_location), (1, 1, 1));
    assert_eq!((p.files_found, p.files_not_found), (3, 0));
}

#[test]
fn cancelled_import_writes_nothing() {
    let env = Env::new();
    let cancel = CancelToken::new();
    cancel.cancel();
    let report = env.import(
        &small_collection(),
        ImportOptions {
            cancel: Some(cancel),
            ..fixture_remap()
        },
    );
    assert!(report.cancelled);
    assert_eq!(report.added, 0);
    assert_eq!(env.count("track"), 0);
    assert_eq!(env.count("playlist"), 0);

    // Cancelling while files are read stops before the database stage.
    let cancel = CancelToken::new();
    let opts = ImportOptions {
        cancel: Some(cancel.clone()),
        ..fixture_remap()
    };
    let report = import(&small_collection(), &env.lib, opts, |e| {
        if matches!(e, ImportEvent::Progress { .. }) {
            cancel.cancel();
        }
    })
    .unwrap();
    assert!(report.cancelled);
    assert_eq!(env.count("track"), 0);
}

#[test]
fn imports_3000_tracks() {
    let env = Env::new();
    let music = env._dir.path().join("music");
    std::fs::create_dir_all(&music).unwrap();
    let exts = ["mp3", "flac", "m4a", "wav", "aiff"];
    let originals: Vec<PathBuf> = exts
        .iter()
        .map(|ext| {
            let dst = music.join(format!("orig.{ext}"));
            std::fs::copy(audio_dir().join(format!("sine-click-128.{ext}")), &dst).unwrap();
            dst
        })
        .collect();

    let n = 3000u32;
    let mut tracks = Vec::new();
    let mut keys = Vec::new();
    for id in 1..=n {
        let ext = exts[id as usize % exts.len()];
        let path = music.join(format!("track {id:04}.{ext}"));
        if id % 20 != 0 {
            link(&originals[id as usize % exts.len()], &path);
        }
        tracks.push(xml_track(id, &path));
        keys.push(id.to_string());
    }
    let c = Collection {
        tracks,
        playlists: XmlNode::folder(
            "ROOT",
            vec![XmlNode::folder(
                "Folder",
                vec![XmlNode::playlist("Everything", 0, keys)],
            )],
        ),
        ..Collection::default()
    };

    let t = Instant::now();
    let report = env.import(&c, ImportOptions::default());
    let first = t.elapsed();
    let t = Instant::now();
    let again = env.import(&c, ImportOptions::default());
    let second = t.elapsed();
    eprintln!("3,000-track import: first {first:?}, re-import {second:?}");

    assert_eq!(report.added, n as usize);
    assert_eq!(report.missing, 150);
    assert_eq!(report.unmatched.len(), 150);
    assert_eq!(report.cue_rows, 3000 * 3);
    assert_eq!(again.matched_by_path, n as usize);
    assert_eq!(env.count("track"), i64::from(n));
    assert_eq!(env.count("cue"), 3000 * 3);
    assert_eq!(env.count("playlist_track"), i64::from(n));
}

fn link(src: &Path, dst: &Path) {
    if std::fs::hard_link(src, dst).is_err() {
        std::fs::copy(src, dst).unwrap();
    }
}

fn xml_track(id: u32, path: &Path) -> XmlTrack {
    XmlTrack {
        track_id: id,
        name: Some(format!("Track {id}")),
        artist: Some(format!("Artist {}", id % 97)),
        size: Some(1000),
        average_bpm: Some(120.0 + f64::from(id % 20)),
        rating: Some(((id % 6) * 51) as u8),
        tonality: Some(["Am", "F#m", "Db", "Ebm"][id as usize % 4].into()),
        colour: id.is_multiple_of(3).then_some(0x25FDE9),
        date_added: Some("2026-10-19".into()),
        location: Some(path_to_location(path).unwrap()),
        tempos: vec![Tempo {
            inizio: 0.025,
            bpm: 128.0,
            metro: Some("4/4".into()),
            battito: Some(1),
            extra: Extra::new(),
        }],
        position_marks: (0..3)
            .map(|i| PositionMark {
                name: Some(format!("Cue {i}")),
                kind: if i == 2 { 4 } else { 0 },
                start: f64::from(i) * 7.5,
                end: (i == 2).then_some(16.875),
                num: i - 1,
                rgb: Some((40, 226, 20)),
                extra: Extra::new(),
            })
            .collect(),
        ..XmlTrack::default()
    }
}

/// An XML playlist never merges into a smart playlist of the same name: it
/// gets a static sibling, and the smart list keeps no entries (D29).
#[test]
fn reimport_never_writes_into_a_smart_playlist() {
    let env = Env::new();
    let c = small_collection();
    env.import(&c, fixture_remap());
    let id = |name: &str| -> i64 {
        env.db()
            .query_row(
                "SELECT id FROM playlist WHERE name = ?1 AND smart_rule IS NULL",
                [name],
                |r| r.get(0),
            )
            .unwrap()
    };
    let (folder, lossless) = (id("reks fixtures"), id("Lossless"));
    env.lib
        .playlist_mutate(PlaylistOp::Delete { id: lossless })
        .unwrap();
    let PlaylistOutcome::Done {
        created_id: Some(smart),
        ..
    } = env
        .lib
        .playlist_mutate(PlaylistOp::CreateSmart {
            parent_id: Some(folder),
            name: "Lossless".into(),
            rule: reks_core::RuleNode::Group {
                r#match: reks_core::Match::Any,
                not: false,
                rules: vec![],
            },
            sort: None,
            position: None,
        })
        .unwrap()
    else {
        panic!("no id");
    };
    env.import(&c, fixture_remap());
    let entries: i64 = env
        .db()
        .query_row(
            "SELECT count(*) FROM playlist_track WHERE playlist_id = ?1",
            [smart],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(entries, 0);
    assert_ne!(id("Lossless"), smart);
    assert_eq!(env.playlist_tracks("Lossless").len(), 1);
}

/// Files are read on a pool; matching, duplicates, the report and row order
/// must still follow the XML.
#[test]
fn parallel_reads_keep_xml_order_and_duplicates() {
    let env = Env::new();
    let music = env._dir.path().join("music");
    std::fs::create_dir_all(&music).unwrap();
    let exts = ["mp3", "flac", "m4a", "wav", "aiff"];
    let mut tracks = Vec::new();
    let mut expected = Vec::new();
    for id in 1..=300u32 {
        let ext = exts[id as usize % exts.len()];
        let mut path = music.join(format!("t{id:03}.{ext}"));
        let want = match id % 10 {
            // Same file as the track before: a duplicate of it.
            0 => {
                path = music.join(format!("t{:03}.{}", id - 1, exts[(id - 1) as usize % 5]));
                Some(UnmatchedReason::DuplicateOf { track_id: id - 1 })
            }
            3 => {
                std::fs::write(&path, b"not audio").unwrap();
                None
            }
            7 => None,
            _ => {
                link(&audio_dir().join(format!("sine-click-128.{ext}")), &path);
                None
            }
        };
        tracks.push(xml_track(id, &path));
        expected.push((id, ext, want));
    }
    let c = Collection {
        tracks,
        playlists: XmlNode::folder("ROOT", Vec::new()),
        ..Collection::default()
    };
    let report = env.import(&c, ImportOptions::default());

    let ids: Vec<u32> = report.unmatched.iter().map(|u| u.track_id).collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(ids, sorted, "unmatched in XML order");
    for u in &report.unmatched {
        let reason = match u.track_id % 10 {
            0 => "dup",
            3 => "unreadable",
            7 => "missing",
            _ => panic!("unexpected {u:?}"),
        };
        let ok = match (&u.reason, reason) {
            (UnmatchedReason::DuplicateOf { track_id }, "dup") => *track_id == u.track_id - 1,
            (UnmatchedReason::Unreadable { .. }, "unreadable") => true,
            (UnmatchedReason::FileNotFound { .. }, "missing") => true,
            _ => false,
        };
        assert!(ok, "{u:?}");
    }
    assert_eq!(report.unmatched.len(), 30 * 3);
    assert_eq!(report.added, 270);

    // Rows are created in XML order and each got its own file's fields.
    let rows: Vec<(i64, u32, String, i64)> = {
        let db = env.db();
        let mut stmt = db
            .prepare("SELECT id, rb_track_id, kind, missing FROM track ORDER BY id")
            .unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    let rb: Vec<u32> = rows.iter().map(|r| r.1).collect();
    let want_rb: Vec<u32> = expected
        .iter()
        .filter(|(_, _, w)| w.is_none())
        .map(|e| e.0)
        .collect();
    assert_eq!(rb, want_rb);
    for (_, id, kind, missing) in &rows {
        let ext = expected[*id as usize - 1].1;
        let want_kind = if ext == "m4a" { "aac" } else { ext };
        assert_eq!(
            (kind.as_str(), *missing),
            (want_kind, i64::from(id % 10 == 7)),
            "{id}"
        );
    }
}

/// An unchanged re-import skips the track UPDATE, so the FTS trigger and
/// `updated_at` stay untouched; a changed field still writes.
#[test]
fn unchanged_reimport_writes_no_track_rows() {
    let env = Env::new();
    let mut c = small_collection();
    env.import(&c, fixture_remap());
    env.db()
        .execute_batch(
            "CREATE TABLE updates (id INTEGER);
             -- The FTS trigger's columns. (The play-count cache is
             -- recomputed on every import; it is not indexed.)
             CREATE TRIGGER count_updates
             AFTER UPDATE OF title, artist, album, genre, comment, label, remixer, updated_at
             ON track
             BEGIN INSERT INTO updates VALUES (new.id); END;",
        )
        .unwrap();
    env.import(&c, fixture_remap());
    assert_eq!(env.count("updates"), 0);

    c.tracks[0].genre = Some("Changed".into());
    env.import(&c, fixture_remap());
    assert_eq!(env.count("updates"), 1);
}
