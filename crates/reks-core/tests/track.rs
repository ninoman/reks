//! `Library::track` and `Library::update_track`: full detail, cues and grid
//! summary, validation, FTS after an edit, unknown ids and `updated_at`.

// Test helpers outside `#[test]` fns are not covered by clippy's allow-unwrap-in-tests.
#![allow(clippy::unwrap_used)]

use reks_core::DataSource;
use reks_core::{Error, Library, TrackPatch, TrackQuery};
use rusqlite::Connection;

/// (slot, kind, start_ms, end_ms, name)
type CueSeed = (Option<i64>, i64, f64, Option<f64>, Option<&'static str>);

const OLD: &str = "2026-01-01T00:00:00.000Z";

struct Env {
    dir: tempfile::TempDir,
    lib: Library,
}

impl Env {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let lib =
            Library::open(&dir.path().join("reks.sqlite"), &dir.path().join("cache")).unwrap();
        Env { dir, lib }
    }

    /// A second connection to the library file for seeding (WAL allows it).
    fn db(&self) -> Connection {
        Connection::open(self.dir.path().join("reks.sqlite")).unwrap()
    }

    /// A track titled `title` by `artist`, with fixed old timestamps.
    fn track(&self, title: &str, artist: &str) -> i64 {
        let c = self.db();
        c.execute(
            "INSERT INTO track (path, file_size, file_mtime, kind, title, artist, comment, key,
                                key_source, year, rating, color, duration_ms, date_added,
                                created_at, updated_at)
             VALUES (?1, 1234, ?3, 'mp3', ?1, ?2, 'warm-up', 'Am', 'rekordbox', 2001, 3, 'red',
                     300000, '2026-01-01', ?3, ?3)",
            rusqlite::params![title, artist, OLD],
        )
        .unwrap();
        c.last_insert_rowid()
    }

    fn search(&self, text: &str) -> Vec<i64> {
        let q = TrackQuery {
            text: Some(text.to_owned()),
            ..TrackQuery::default()
        };
        self.lib
            .tracks(&q)
            .unwrap()
            .rows
            .iter()
            .map(|r| r.id)
            .collect()
    }
}

fn patch(json: &str) -> TrackPatch {
    serde_json::from_str(json).unwrap()
}

#[test]
fn track_returns_full_fields_cues_and_grid() {
    let env = Env::new();
    let id = env.track("Midnight Groove", "Ana");
    let c = env.db();
    for (pos, bpm) in [(1500.0, 12800), (120.5, 12800), (60000.0, 13000)] {
        c.execute(
            "INSERT INTO beatgrid (track_id, position_ms, bpm_x100, source)
             VALUES (?1, ?2, ?3, 'rekordbox')",
            rusqlite::params![id, pos, bpm],
        )
        .unwrap();
    }
    // (slot, kind, start, end, name): hot C, memory, loop, hot A, memory.
    let cues: [CueSeed; 5] = [
        (Some(2), 0, 9000.0, None, Some("drop")),
        (None, 0, 5000.0, None, None),
        (None, 4, 1000.0, Some(8500.25), Some("intro loop")),
        (Some(0), 0, 120.5, None, None),
        (None, 0, 200.0, None, None),
    ];
    for (slot, kind, start, end, name) in cues {
        c.execute(
            "INSERT INTO cue (track_id, slot, kind, start_ms, end_ms, name, color_rgb, source)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 16711680, 'rekordbox')",
            rusqlite::params![id, slot, kind, start, end, name],
        )
        .unwrap();
    }

    let d = env.lib.track(id).unwrap();
    assert_eq!(d.track.row.id, id);
    assert_eq!(d.track.row.title.as_deref(), Some("Midnight Groove"));
    assert_eq!(d.track.path, "Midnight Groove");
    assert_eq!(d.track.file_size, 1234);
    assert_eq!(d.track.key_source, Some(DataSource::Rekordbox));
    assert_eq!(d.track.updated_at, OLD);
    assert!(!d.track.row.needs_analysis);
    assert_eq!(d.track.artwork, None);

    let order: Vec<(Option<u8>, f64)> = d.cues.iter().map(|c| (c.slot, c.start_ms)).collect();
    assert_eq!(
        order,
        [
            (Some(0), 120.5),
            (Some(2), 9000.0),
            (None, 200.0),
            (None, 1000.0),
            (None, 5000.0)
        ]
    );
    let lp = &d.cues[3];
    assert_eq!((lp.kind, lp.end_ms), (4, Some(8500.25)));
    assert_eq!(lp.name.as_deref(), Some("intro loop"));
    assert_eq!(lp.color_rgb, Some(0xFF0000));

    let g = d.beatgrid.unwrap();
    assert_eq!(g.markers, 3);
    assert_eq!(g.first_beat_ms, 120.5);
    assert_eq!((g.bpm_min_x100, g.bpm_max_x100), (12800, 13000));
}

#[test]
fn track_without_grid_or_cues() {
    let env = Env::new();
    let id = env.track("Bare", "Bo");
    let d = env.lib.track(id).unwrap();
    assert!(d.cues.is_empty());
    assert_eq!(d.beatgrid, None);
    assert!(d.track.row.needs_analysis);
}

#[test]
fn track_with_artwork() {
    let env = Env::new();
    let id = env.track("Art", "Al");
    // JPEG header up to a 3×2 SOF0 frame.
    let jpeg = [
        0xFF, 0xD8, 0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x02, 0x00, 0x03, 0x01, 0x01, 0x11, 0x00,
        0xFF, 0xD9,
    ];
    let art = env.lib.store_artwork(&jpeg).unwrap().unwrap();
    env.db()
        .execute("UPDATE track SET artwork_id = ?1 WHERE id = ?2", [art, id])
        .unwrap();
    let a = env.lib.track(id).unwrap().track.artwork.unwrap();
    assert_eq!((a.id, a.mime.as_str()), (art, "image/jpeg"));
    assert_eq!((a.width, a.height), (Some(3), Some(2)));
    assert_eq!(env.lib.artwork(a.id).unwrap(), jpeg);
}

#[test]
fn valid_edit_persists_and_leaves_other_fields() {
    let env = Env::new();
    let id = env.track("Old Title", "Ana");
    let d = env
        .lib
        .update_track(
            id,
            &patch(
                r#"{"title": "  New Title ", "albumArtist": "Various", "trackNo": 4,
                    "year": null, "rating": 5, "color": "Blue", "comment": "line 1\nline 2",
                    "mix": ""}"#,
            ),
        )
        .unwrap();
    let t = &d.track;
    assert_eq!(t.row.title.as_deref(), Some("New Title"));
    assert_eq!(t.album_artist.as_deref(), Some("Various"));
    assert_eq!(t.track_no, Some(4));
    assert_eq!(t.row.year, None);
    assert_eq!(t.row.rating, 5);
    assert_eq!(t.row.color.as_deref(), Some("blue"));
    assert_eq!(t.row.comment.as_deref(), Some("line 1\nline 2"));
    assert_eq!(t.mix, None);
    // Untouched.
    assert_eq!(t.row.artist.as_deref(), Some("Ana"));
    assert_eq!(t.row.key.as_deref(), Some("Am"));
    assert_eq!(t.key_source, Some(DataSource::Rekordbox));
    // What came back is what is stored.
    assert_eq!(env.lib.track(id).unwrap(), d);
}

#[test]
fn key_edit_is_canonical_and_manual() {
    let env = Env::new();
    let id = env.track("K", "K");
    let t = env
        .lib
        .update_track(id, &patch(r#"{"key": "8B"}"#))
        .unwrap()
        .track;
    assert_eq!(t.row.key.as_deref(), Some("C"));
    assert_eq!(t.key_source, Some(DataSource::Manual));
    let t = env
        .lib
        .update_track(id, &patch(r#"{"key": null}"#))
        .unwrap()
        .track;
    assert_eq!((t.row.key, t.key_source), (None, None));
}

#[test]
fn invalid_values_are_rejected_and_nothing_is_written() {
    let env = Env::new();
    let id = env.track("Keep", "Me");
    let before = env.lib.track(id).unwrap();
    let cases = [
        (r#"{"title": "ok", "rating": 6}"#, "rating"),
        (r#"{"title": "ok", "year": 0}"#, "year"),
        (r#"{"year": 10000}"#, "year"),
        (r#"{"trackNo": -1}"#, "track_no"),
        (r#"{"discNo": 1000}"#, "disc_no"),
        (r#"{"key": "H#"}"#, "key"),
        (r#"{"color": "magenta"}"#, "color"),
        (r#"{"artist": "a", "title": "two\nlines"}"#, "title"),
        (r#"{"genre": "bell\u0007"}"#, "genre"),
    ];
    for (json, field) in cases {
        match env.lib.update_track(id, &patch(json)) {
            Err(Error::InvalidField { field: f, .. }) => assert_eq!(f, field, "{json}"),
            other => panic!("{json}: expected InvalidField, got {other:?}"),
        }
    }
    let long = format!(r#"{{"title": "{}"}}"#, "x".repeat(1025));
    assert!(matches!(
        env.lib.update_track(id, &patch(&long)),
        Err(Error::InvalidField { field: "title", .. })
    ));
    assert_eq!(env.lib.track(id).unwrap(), before);
}

#[test]
fn fts_reflects_the_edit() {
    let env = Env::new();
    let id = env.track("Sunrise Dub", "Ana");
    let other = env.track("Sunset Dub", "Bo");
    assert_eq!(env.search("sunrise"), [id]);
    env.lib
        .update_track(
            id,
            &patch(r#"{"title": "Moonlight Dub", "label": "Nocturne"}"#),
        )
        .unwrap();
    assert!(env.search("sunrise").is_empty());
    assert_eq!(env.search("moonl"), [id]);
    assert_eq!(env.search("nocturne"), [id]);
    let mut dub = env.search("dub");
    dub.sort_unstable();
    assert_eq!(dub, [id, other]);
}

#[test]
fn unknown_id_is_an_error() {
    let env = Env::new();
    assert!(matches!(env.lib.track(99), Err(Error::TrackNotFound(99))));
    assert!(matches!(
        env.lib.update_track(99, &patch(r#"{"title": "x"}"#)),
        Err(Error::TrackNotFound(99))
    ));
    assert!(matches!(
        env.lib.update_track(99, &TrackPatch::default()),
        Err(Error::TrackNotFound(99))
    ));
}

#[test]
fn updated_at_changes_on_edit_only() {
    let env = Env::new();
    let id = env.track("T", "A");
    let same = env.lib.update_track(id, &TrackPatch::default()).unwrap();
    assert_eq!(same.track.updated_at, OLD);

    let t = env
        .lib
        .update_track(id, &patch(r#"{"rating": 1}"#))
        .unwrap()
        .track;
    assert_ne!(t.updated_at, OLD);
    assert_eq!(t.created_at, OLD);
    // YYYY-MM-DDTHH:MM:SS.mmmZ
    let u = t.updated_at.as_bytes();
    assert_eq!(u.len(), 24);
    assert_eq!((u[4], u[10], u[19], u[23]), (b'-', b'T', b'.', b'Z'));
    assert!(t.updated_at.as_str() > OLD);
}

#[test]
fn serde_shape() {
    let env = Env::new();
    let id = env.track("Shape", "S");
    let v = serde_json::to_value(env.lib.track(id).unwrap()).unwrap();
    let t = v["track"].as_object().unwrap();
    // Flattened table-row fields plus the detail fields.
    for k in [
        "id",
        "title",
        "bpmX100",
        "needsAnalysis",
        "path",
        "albumArtist",
        "trackNo",
        "keySource",
        "updatedAt",
        "artwork",
    ] {
        assert!(t.contains_key(k), "missing {k}");
    }
    assert_eq!(v["cues"], serde_json::json!([]));
    assert_eq!(v["beatgrid"], serde_json::Value::Null);
}
