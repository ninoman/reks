//! Matching, fingerprint and import on synthetic sticks (phase-2.5.md task 6).

use std::path::Path;

use super::*;
use crate::manifest::{ManifestItemKind, ManifestRow};

const SERIAL: &str = "1234-ABCD";

fn lib() -> Library {
    Library::open_in_memory(Path::new("cache-unused")).unwrap()
}

fn track(lib: &Library, path: &str, artist: &str, title: &str, duration_ms: i64) -> TrackId {
    let c = lib.conn().unwrap();
    c.execute(
        "INSERT INTO track (path, file_size, file_mtime, kind, title, artist, duration_ms,
                            date_added, created_at, updated_at)
         VALUES (?1, 1, '2026-01-01T00:00:00Z', 'mp3', ?2, ?3, ?4,
                 '2026-01-01', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        params![path, title, artist, duration_ms],
    )
    .unwrap();
    c.last_insert_rowid()
}

fn entry(path: &str, artist: &str, title: &str, secs: i64) -> SourceEntry {
    SourceEntry {
        stick_path: Some(path.into()),
        title: Some(title.into()),
        artist: Some(artist.into()),
        duration_ms: Some(secs * 1000),
    }
}

fn session(name: &str, entries: Vec<SourceEntry>) -> SourceSession {
    SourceSession {
        source_name: name.into(),
        played_on: None,
        entries,
    }
}

fn stick(serial: Option<&str>, sessions: Vec<SourceSession>) -> SourceHistory {
    SourceHistory {
        volume_serial: serial.map(str::to_owned),
        sessions,
    }
}

/// (item_kind, track_id, raw_path) per stored entry.
fn stored(lib: &Library, id: SessionId) -> Vec<(String, Option<TrackId>, Option<String>)> {
    let c = lib.conn().unwrap();
    let mut stmt = c
        .prepare(
            "SELECT item_kind, track_id, raw_path FROM play_session_entry
             WHERE session_id = ?1 ORDER BY position",
        )
        .unwrap();
    stmt.query_map([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}

fn session_count(lib: &Library) -> i64 {
    lib.conn()
        .unwrap()
        .query_row("SELECT COUNT(*) FROM play_session", [], |r| r.get(0))
        .unwrap()
}

fn only(plan: &HistoryImportPlan) -> &PlannedSession {
    assert_eq!(plan.sessions.len(), 1);
    &plan.sessions[0]
}

#[test]
fn manifest_hit_and_divider_entry() {
    let lib = lib();
    let a = track(&lib, "/m/A/a.mp3", "Art", "Alpha", 200_000);
    // Library metadata differs from the stick on purpose: the manifest wins.
    let b = track(&lib, "/m/B/b.mp3", "Other", "Beta", 100_000);
    lib.replace_manifest(
        Some(SERIAL),
        &[
            ManifestRow {
                stick_path: "/Contents/Art/x/a.mp3".into(),
                item_kind: ManifestItemKind::Track,
                track_id: Some(a),
            },
            ManifestRow {
                stick_path: "/Contents/Unknown/b.mp3".into(),
                item_kind: ManifestItemKind::Track,
                track_id: Some(b),
            },
            ManifestRow {
                stick_path: "/Contents/UnknownArtist/UnknownAlbum/div-1-0.wav".into(),
                item_kind: ManifestItemKind::Divider,
                track_id: None,
            },
        ],
    )
    .unwrap();
    let h = stick(
        Some(SERIAL),
        vec![session(
            "HISTORY 001",
            vec![
                entry("/Contents/Art/x/a.mp3", "Art", "Alpha", 200),
                entry(
                    "/Contents/UnknownArtist/UnknownAlbum/div-1-0.wav",
                    "",
                    "PEAK",
                    4,
                ),
                entry("/Contents/Unknown/b.mp3", "Someone", "Gamma", 300),
            ],
        )],
    );
    let plan = scan(&h, &lib).unwrap();
    let s = only(&plan);
    assert_eq!(s.action, SessionAction::New);
    let got: Vec<_> = s
        .entries
        .iter()
        .map(|e| (e.kind, e.track_id, e.matched_by))
        .collect();
    assert_eq!(
        got,
        vec![
            (EntryKind::Track, Some(a), Some(MatchKind::Manifest)),
            (EntryKind::Divider, None, Some(MatchKind::Manifest)),
            (EntryKind::Track, Some(b), Some(MatchKind::Manifest)),
        ]
    );

    let report = import(&plan, &ImportChoices::default(), &lib).unwrap();
    assert_eq!(report.entries, 3);
    assert_eq!(report.unmatched, 0);
    let rows = stored(&lib, report.created[0]);
    assert_eq!(rows[1].0, "divider");
    assert_eq!(rows[1].1, None);
}

#[test]
fn manifest_of_another_serial_is_not_used() {
    let lib = lib();
    let a = track(&lib, "/m/zzz.mp3", "X", "Y", 1000);
    lib.replace_manifest(
        Some("OTHER"),
        &[ManifestRow {
            stick_path: "/Contents/a.mp3".into(),
            item_kind: ManifestItemKind::Track,
            track_id: Some(a),
        }],
    )
    .unwrap();
    let h = stick(
        Some(SERIAL),
        vec![session(
            "HISTORY 001",
            vec![entry("/Contents/a.mp3", "", "Q", 9)],
        )],
    );
    let plan = scan(&h, &lib).unwrap();
    assert_eq!(only(&plan).entries[0].track_id, None);
}

#[test]
fn path_suffix_fallback() {
    let lib = lib();
    let a = track(
        &lib,
        "/Users/me/Music/Art/Album/01 Song.mp3",
        "Art",
        "Song",
        245_400,
    );
    // Same file name, shorter suffix: loses to `a`.
    track(&lib, "/Users/me/Other/01 Song.mp3", "Art", "Song", 245_400);
    let unique = track(&lib, "/Users/me/Music/only.flac", "U", "Only", 90_000);
    let h = stick(
        None,
        vec![session(
            "HISTORY 001",
            vec![
                entry("/Contents/Art/Album/01 song.MP3", "Art", "Song", 245),
                entry("/Contents/U/only.flac", "U", "Only", 90),
            ],
        )],
    );
    let s = only(&scan(&h, &lib).unwrap()).clone();
    assert_eq!(s.entries[0].track_id, Some(a));
    assert_eq!(s.entries[0].matched_by, Some(MatchKind::PathSuffix));
    assert_eq!(s.entries[1].track_id, Some(unique));
}

#[test]
fn path_suffix_tie_or_duration_mismatch_falls_through() {
    let lib = lib();
    track(&lib, "/a/Art/x.mp3", "A1", "T1", 100_000);
    track(&lib, "/b/Art/x.mp3", "A2", "T2", 100_000);
    let long = track(&lib, "/c/y.mp3", "A3", "T3", 300_000);
    let h = stick(
        None,
        vec![session(
            "HISTORY 001",
            vec![
                // Two tracks end in Art/x.mp3: no suffix match, no metadata match.
                entry("/Contents/Art/x.mp3", "Nobody", "Nothing", 100),
                // Name matches, duration is 200 s off.
                entry("/Contents/y.mp3", "Nobody", "Nothing", 100),
            ],
        )],
    );
    let s = only(&scan(&h, &lib).unwrap()).clone();
    assert_eq!(s.entries[0].track_id, None);
    assert_ne!(s.entries[1].track_id, Some(long));
    assert_eq!(s.entries[1].track_id, None);
}

#[test]
fn metadata_fallback_unique_only() {
    let lib = lib();
    let solo = track(&lib, "/m/solo.mp3", "Kino", "Gruppa Krovi", 287_600);
    track(&lib, "/m/d1.mp3", "Dup", "Twice", 200_000);
    track(&lib, "/m/d2.flac", "Dup", "Twice", 200_400);
    // Same artist and title, 3 s longer: not the same track.
    track(&lib, "/m/near.mp3", "Near", "Miss", 203_000);
    let h = stick(
        None,
        vec![session(
            "HISTORY 001",
            vec![
                entry(
                    "/Contents/Kino/x/Gruppa_Krovi.mp3",
                    " kino ",
                    "GRUPPA KROVI",
                    287,
                ),
                entry("/Contents/Dup/x/twice.mp3", "Dup", "Twice", 200),
                entry("/Contents/Near/x/miss.mp3", "Near", "Miss", 200),
            ],
        )],
    );
    let s = only(&scan(&h, &lib).unwrap()).clone();
    assert_eq!(s.entries[0].track_id, Some(solo));
    assert_eq!(s.entries[0].matched_by, Some(MatchKind::Metadata));
    assert_eq!(s.entries[1].track_id, None, "two candidates: no match");
    assert_eq!(s.entries[2].track_id, None, "duration outside ±1 s");
}

#[test]
fn unmatched_entries_are_kept_and_linkable() {
    let lib = lib();
    let later = track(&lib, "/m/late.mp3", "L", "Late", 1000);
    let missing_row = SourceEntry::default();
    let h = stick(
        Some(SERIAL),
        vec![session(
            "HISTORY 001",
            vec![
                entry("/Contents/u/unknown.mp3", "U", "Unknown", 123),
                missing_row,
            ],
        )],
    );
    let plan = scan(&h, &lib).unwrap();
    let report = import(&plan, &ImportChoices::default(), &lib).unwrap();
    assert_eq!((report.entries, report.unmatched), (2, 2));
    let id = report.created[0];
    {
        let c = lib.conn().unwrap();
        let raw: (Option<String>, Option<String>, Option<i64>) = c
            .query_row(
                "SELECT raw_title, raw_artist, raw_duration_ms FROM play_session_entry
                 WHERE session_id = ?1 AND position = 0",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            raw,
            (Some("Unknown".into()), Some("U".into()), Some(123_000))
        );
    }

    lib.session_link_entry(id, 0, later).unwrap();
    assert_eq!(stored(&lib, id)[0].1, Some(later));
    assert!(matches!(
        lib.session_link_entry(id, 9, later),
        Err(Error::SessionEntryNotFound { position: 9, .. })
    ));
    assert!(matches!(
        lib.session_link_entry(id, 1, 999),
        Err(Error::TrackNotFound(999))
    ));
}

#[test]
fn dividers_cannot_be_linked() {
    let lib = lib();
    let t = track(&lib, "/m/t.mp3", "A", "T", 1000);
    lib.replace_manifest(
        Some(SERIAL),
        &[ManifestRow {
            stick_path: "/Contents/d.wav".into(),
            item_kind: ManifestItemKind::Divider,
            track_id: None,
        }],
    )
    .unwrap();
    let h = stick(
        Some(SERIAL),
        vec![session("H", vec![entry("/Contents/d.wav", "", "--", 4)])],
    );
    let r = import(&scan(&h, &lib).unwrap(), &ImportChoices::default(), &lib).unwrap();
    assert!(matches!(
        lib.session_link_entry(r.created[0], 0, t),
        Err(Error::NotATrackEntry { .. })
    ));
}

#[test]
fn fingerprint_skip_on_reimport() {
    let lib = lib();
    let h = stick(
        Some(SERIAL),
        vec![
            session("HISTORY 001", vec![entry("/Contents/a.mp3", "A", "A", 1)]),
            session("HISTORY 002", vec![entry("/Contents/b.mp3", "B", "B", 1)]),
        ],
    );
    let first = import(&scan(&h, &lib).unwrap(), &ImportChoices::default(), &lib).unwrap();
    assert_eq!(first.created.len(), 2);

    let again = scan(&h, &lib).unwrap();
    for (s, id) in again.sessions.iter().zip(&first.created) {
        assert_eq!(
            s.action,
            SessionAction::Skip(SkipReason::AlreadyImported { session_id: *id })
        );
        assert!(s.entries.is_empty());
    }
    let report = import(&again, &ImportChoices::default(), &lib).unwrap();
    assert_eq!((report.created.len(), report.skipped), (0, 2));
    assert_eq!(session_count(&lib), 2);

    // A stale plan for a session imported meanwhile adds nothing either.
    let stale = HistoryImportPlan {
        sessions: vec![PlannedSession {
            action: SessionAction::New,
            ..again.sessions[0].clone()
        }],
        ..again
    };
    let report = import(&stale, &ImportChoices::default(), &lib).unwrap();
    assert_eq!((report.created.len(), report.skipped), (0, 1));
    assert_eq!(session_count(&lib), 2);
}

#[test]
fn fingerprint_extend_appends_only_the_new_entries() {
    let lib = lib();
    let a = entry("/Contents/a.mp3", "A", "A", 1);
    let b = entry("/Contents/b.mp3", "B", "B", 1);
    let c = entry("/Contents/c.mp3", "C", "C", 1);
    let short = stick(
        Some(SERIAL),
        vec![session("HISTORY 001", vec![a.clone(), b.clone()])],
    );
    let mut choices = ImportChoices::default();
    let plan = scan(&short, &lib).unwrap();
    choices
        .excluded
        .insert(only(&plan).fingerprint.clone(), false);
    let id = import(&plan, &choices, &lib).unwrap().created[0];

    let long = stick(Some(SERIAL), vec![session("HISTORY 001", vec![a, b, c])]);
    let plan = scan(&long, &lib).unwrap();
    let s = only(&plan);
    assert_eq!(
        s.action,
        SessionAction::Extend {
            session_id: id,
            existing_entries: 2
        }
    );
    assert_eq!(s.default_exclusion, None);
    assert_eq!(s.new_entries().len(), 1);
    assert_eq!(s.new_entries()[0].position, 2);

    let report = import(&plan, &ImportChoices::default(), &lib).unwrap();
    assert_eq!(report.extended, vec![id]);
    assert_eq!(report.entries, 1);
    assert_eq!(session_count(&lib), 1);
    let paths: Vec<_> = stored(&lib, id).into_iter().map(|r| r.2).collect();
    assert_eq!(
        paths,
        ["/Contents/a.mp3", "/Contents/b.mp3", "/Contents/c.mp3"].map(|p| Some(p.to_owned()))
    );
    // The extended session keeps its exclusion state and now has the long fingerprint.
    let excluded: bool = lib
        .conn()
        .unwrap()
        .query_row(
            "SELECT excluded FROM play_session WHERE id = ?1",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!excluded);
    assert!(matches!(
        only(&scan(&long, &lib).unwrap()).action,
        SessionAction::Skip(SkipReason::AlreadyImported { session_id }) if session_id == id
    ));

    // Re-applying the same extend plan is stale: nothing written.
    assert!(matches!(
        import(&plan, &ImportChoices::default(), &lib),
        Err(Error::StaleHistoryPlan(_))
    ));
    assert_eq!(stored(&lib, id).len(), 3);
}

#[test]
fn not_a_prefix_is_a_new_session() {
    let lib = lib();
    let a = entry("/Contents/a.mp3", "A", "A", 1);
    let b = entry("/Contents/b.mp3", "B", "B", 1);
    let first = stick(
        Some(SERIAL),
        vec![session("HISTORY 001", vec![a.clone(), b.clone()])],
    );
    import(
        &scan(&first, &lib).unwrap(),
        &ImportChoices::default(),
        &lib,
    )
    .unwrap();
    // The stick was re-exported and the player started a fresh HISTORY 001.
    let fresh = stick(Some(SERIAL), vec![session("HISTORY 001", vec![b, a])]);
    assert_eq!(
        only(&scan(&fresh, &lib).unwrap()).action,
        SessionAction::New
    );
}

#[test]
fn fingerprint_differs_by_serial() {
    let lib = lib();
    let s = || session("HISTORY 001", vec![entry("/Contents/a.mp3", "A", "A", 1)]);
    import(
        &scan(&stick(Some(SERIAL), vec![s()]), &lib).unwrap(),
        &ImportChoices::default(),
        &lib,
    )
    .unwrap();
    let other = scan(&stick(Some("9999-0000"), vec![s()]), &lib).unwrap();
    assert_eq!(only(&other).action, SessionAction::New);
    let no_serial = scan(&stick(None, vec![s()]), &lib).unwrap();
    assert_eq!(only(&no_serial).action, SessionAction::New);
    assert_ne!(only(&other).fingerprint, only(&no_serial).fingerprint);

    // A longer session on another stick never extends this one.
    let longer = session(
        "HISTORY 001",
        vec![
            entry("/Contents/a.mp3", "A", "A", 1),
            entry("/Contents/b.mp3", "B", "B", 1),
        ],
    );
    let other = scan(&stick(Some("9999-0000"), vec![longer]), &lib).unwrap();
    assert_eq!(only(&other).action, SessionAction::New);
}

#[test]
fn duplicate_session_in_one_source_is_skipped() {
    let lib = lib();
    let s = || session("HISTORY 001", vec![entry("/Contents/a.mp3", "A", "A", 1)]);
    let plan = scan(&stick(Some(SERIAL), vec![s(), s()]), &lib).unwrap();
    assert_eq!(plan.sessions[0].action, SessionAction::New);
    assert_eq!(
        plan.sessions[1].action,
        SessionAction::Skip(SkipReason::DuplicateInSource)
    );
    import(&plan, &ImportChoices::default(), &lib).unwrap();
    assert_eq!(session_count(&lib), 1);
}

#[test]
fn exclusion_defaults_and_choices() {
    let lib = lib();
    lib.conn()
        .unwrap()
        .execute(
            "UPDATE setting SET value = '\"2026-06-01\"' WHERE key = 'history.baseline_date'",
            [],
        )
        .unwrap();
    let dated = |name: &str, d: Option<&str>| SourceSession {
        played_on: d.map(str::to_owned),
        ..session(
            name,
            vec![entry(&format!("/Contents/{name}.mp3"), "A", "A", 1)],
        )
    };
    let h = stick(
        Some(SERIAL),
        vec![
            dated("before", Some("2026-06-01")),
            dated("after", Some("2026-06-02")),
            dated("unknown", None),
            dated("practice", Some("2026-07-01")),
        ],
    );
    let plan = scan(&h, &lib).unwrap();
    assert_eq!(plan.baseline_date.as_deref(), Some("2026-06-01"));
    let defaults: Vec<_> = plan.sessions.iter().map(|s| s.default_exclusion).collect();
    assert_eq!(
        defaults,
        vec![
            Some(DefaultExclusion::PreBaseline),
            None,
            Some(DefaultExclusion::UnknownDate),
            None
        ]
    );
    let mut choices = ImportChoices::default();
    choices
        .excluded
        .insert(plan.sessions[3].fingerprint.clone(), true);
    import(&plan, &choices, &lib).unwrap();
    let rows: Vec<(String, bool, Option<String>)> = {
        let c = lib.conn().unwrap();
        let mut stmt = c
            .prepare("SELECT source_name, excluded, excluded_reason FROM play_session ORDER BY id")
            .unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    };
    assert_eq!(
        rows,
        vec![
            ("before".into(), true, Some("pre_baseline".into())),
            ("after".into(), false, None),
            ("unknown".into(), true, Some("pre_baseline".into())),
            ("practice".into(), true, Some("user".into())),
        ]
    );
}

#[test]
fn import_is_one_transaction() {
    let lib = lib();
    let h = stick(
        Some(SERIAL),
        vec![session(
            "HISTORY 001",
            vec![entry("/Contents/a.mp3", "A", "A", 1)],
        )],
    );
    let mut plan = scan(&h, &lib).unwrap();
    // A second, bogus extend after a valid new session: the whole import fails.
    let mut bogus = plan.sessions[0].clone();
    bogus.fingerprint = "f".repeat(64);
    bogus.action = SessionAction::Extend {
        session_id: 4242,
        existing_entries: 1,
    };
    plan.sessions.push(bogus);
    assert!(matches!(
        import(&plan, &ImportChoices::default(), &lib),
        Err(Error::StaleHistoryPlan(_))
    ));
    assert_eq!(session_count(&lib), 0);
}

#[test]
fn scan_writes_nothing() {
    let lib = lib();
    let h = stick(
        Some(SERIAL),
        vec![session(
            "HISTORY 001",
            vec![entry("/Contents/a.mp3", "A", "A", 1)],
        )],
    );
    scan(&h, &lib).unwrap();
    assert_eq!(session_count(&lib), 0);
}

// ---- Derived counts (task 7) ----

/// (play_count, last_played) of a track.
fn stats(lib: &Library, id: TrackId) -> (i64, Option<String>) {
    lib.conn()
        .unwrap()
        .query_row(
            "SELECT play_count, last_played FROM track WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap()
}

fn set_baseline(lib: &Library, id: TrackId, count: i64, last: Option<&str>) {
    lib.conn()
        .unwrap()
        .execute(
            "UPDATE track SET baseline_play_count = ?2, baseline_last_played = ?3,
                              play_count = ?2, last_played = ?3 WHERE id = ?1",
            params![id, count, last],
        )
        .unwrap();
}

fn set_baseline_date(lib: &Library, date: &str) {
    lib.conn()
        .unwrap()
        .execute(
            "UPDATE setting SET value = ?1 WHERE key = 'history.baseline_date'",
            [format!("\"{date}\"")],
        )
        .unwrap();
}

fn dated(name: &str, played_on: Option<&str>, entries: Vec<SourceEntry>) -> SourceSession {
    SourceSession {
        played_on: played_on.map(str::to_owned),
        ..session(name, entries)
    }
}

#[test]
fn counts_follow_included_track_entries() {
    let lib = lib();
    set_baseline_date(&lib, "2026-06-01");
    let a = track(&lib, "/m/a.mp3", "A", "Alpha", 200_000);
    let b = track(&lib, "/m/b.mp3", "B", "Beta", 200_000);
    let div = track(&lib, "/m/never.mp3", "N", "Never", 200_000);
    set_baseline(&lib, a, 5, Some("2026-05-01"));
    set_baseline(&lib, b, 0, None);
    lib.replace_manifest(
        Some(SERIAL),
        &[ManifestRow {
            stick_path: "/Contents/d/div-1-0.wav".into(),
            item_kind: ManifestItemKind::Divider,
            track_id: None,
        }],
    )
    .unwrap();
    let ea = entry("/Contents/x/a.mp3", "A", "Alpha", 200);
    let eb = entry("/Contents/x/b.mp3", "B", "Beta", 200);
    let h = stick(
        Some(SERIAL),
        vec![
            dated(
                "HISTORY 001",
                Some("2026-07-01"),
                vec![
                    ea.clone(),
                    entry("/Contents/d/div-1-0.wav", "", "PEAK", 4),
                    ea.clone(),
                    entry("/Contents/u/unknown.mp3", "U", "U", 1),
                ],
            ),
            // Pre-baseline: imported excluded, so not counted.
            dated(
                "HISTORY 002",
                Some("2026-05-20"),
                vec![ea.clone(), eb.clone()],
            ),
            dated("HISTORY 003", Some("2026-08-01"), vec![eb.clone()]),
        ],
    );
    let plan = scan(&h, &lib).unwrap();
    let report = import(&plan, &ImportChoices::default(), &lib).unwrap();
    assert_eq!(report.created.len(), 3);
    assert_eq!(stats(&lib, a), (7, Some("2026-07-01".into())));
    assert_eq!(stats(&lib, b), (1, Some("2026-08-01".into())));
    assert_eq!(stats(&lib, div), (0, None));

    // Practice on and off restores the exact values.
    let first = report.created[0];
    let before = (stats(&lib, a), stats(&lib, b));
    lib.session_update(
        first,
        SessionPatch {
            excluded: Some(true),
            ..SessionPatch::default()
        },
    )
    .unwrap();
    assert_eq!(stats(&lib, a), (5, Some("2026-05-01".into())));
    lib.session_update(
        first,
        SessionPatch {
            excluded: Some(false),
            ..SessionPatch::default()
        },
    )
    .unwrap();
    assert_eq!((stats(&lib, a), stats(&lib, b)), before);

    // Including the pre-baseline session counts it.
    lib.session_update(
        report.created[1],
        SessionPatch {
            excluded: Some(false),
            ..SessionPatch::default()
        },
    )
    .unwrap();
    assert_eq!(stats(&lib, a), (8, Some("2026-07-01".into())));
    assert_eq!(stats(&lib, b), (2, Some("2026-08-01".into())));

    // Linking the unmatched entry counts it for its new track.
    lib.session_link_entry(first, 3, b).unwrap();
    assert_eq!(stats(&lib, b), (3, Some("2026-08-01".into())));
    // Relinking moves the play from b to div.
    lib.session_link_entry(first, 3, div).unwrap();
    assert_eq!(stats(&lib, b).0, 2);
    assert_eq!(stats(&lib, div), (1, Some("2026-07-01".into())));

    for id in report.created {
        lib.session_delete(id).unwrap();
    }
    assert_eq!(stats(&lib, a), (5, Some("2026-05-01".into())));
    assert_eq!(stats(&lib, b), (0, None));
    assert_eq!(stats(&lib, div), (0, None));
    assert_eq!(session_count(&lib), 0);
}

#[test]
fn session_update_edits_and_errors() {
    let lib = lib();
    let h = stick(
        Some(SERIAL),
        vec![session(
            "HISTORY 001",
            vec![entry("/Contents/a.mp3", "A", "A", 1)],
        )],
    );
    let plan = scan(&h, &lib).unwrap();
    let id = import(&plan, &ImportChoices::default(), &lib)
        .unwrap()
        .created[0];
    lib.session_update(
        id,
        SessionPatch {
            name: Some("  Friday  ".into()),
            venue: Some(Some("Club".into())),
            notes: Some(Some("  ".into())),
            excluded: Some(false),
        },
    )
    .unwrap();
    let row: (String, Option<String>, Option<String>, bool, Option<String>) = lib
        .conn()
        .unwrap()
        .query_row(
            "SELECT name, venue, notes, excluded, excluded_reason FROM play_session WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .unwrap();
    assert_eq!(
        row,
        ("Friday".into(), Some("Club".into()), None, false, None)
    );
    lib.session_update(
        id,
        SessionPatch {
            excluded: Some(true),
            ..SessionPatch::default()
        },
    )
    .unwrap();
    let reason: Option<String> = lib
        .conn()
        .unwrap()
        .query_row(
            "SELECT excluded_reason FROM play_session WHERE id = ?1",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(reason.as_deref(), Some("user"));
    assert!(matches!(
        lib.session_update(
            id,
            SessionPatch {
                name: Some(" ".into()),
                ..SessionPatch::default()
            }
        ),
        Err(Error::InvalidField { field: "name", .. })
    ));
    assert!(matches!(
        lib.session_update(99, SessionPatch::default()),
        Err(Error::SessionNotFound(99))
    ));
    assert!(matches!(
        lib.session_delete(99),
        Err(Error::SessionNotFound(99))
    ));
}

/// Acceptance: Practice on a 100-track session updates within 100 ms and
/// toggling back restores the exact values.
#[test]
fn practice_toggle_on_100_tracks_is_fast_and_exact() {
    let lib = lib();
    set_baseline_date(&lib, "2026-01-01");
    let mut entries = Vec::new();
    let mut ids = Vec::new();
    for i in 0..1000 {
        let id = track(
            &lib,
            &format!("/m/t{i}.mp3"),
            "A",
            &format!("T{i}"),
            200_000,
        );
        set_baseline(&lib, id, i % 7, Some("2026-01-01"));
        if i % 10 == 0 {
            entries.push(entry(
                &format!("/Contents/x/t{i}.mp3"),
                "A",
                &format!("T{i}"),
                200,
            ));
        }
        ids.push(id);
    }
    let h = stick(
        Some(SERIAL),
        vec![dated("HISTORY 001", Some("2026-02-01"), entries)],
    );
    let plan = scan(&h, &lib).unwrap();
    let sid = import(&plan, &ImportChoices::default(), &lib)
        .unwrap()
        .created[0];
    let all = |lib: &Library| ids.iter().map(|&id| stats(lib, id)).collect::<Vec<_>>();
    let before = all(&lib);
    let started = std::time::Instant::now();
    lib.session_update(
        sid,
        SessionPatch {
            excluded: Some(true),
            ..SessionPatch::default()
        },
    )
    .unwrap();
    let took = started.elapsed();
    assert!(took.as_millis() < 100, "practice toggle took {took:?}");
    assert_eq!(stats(&lib, ids[10]), (3, Some("2026-01-01".into())));
    lib.session_update(
        sid,
        SessionPatch {
            excluded: Some(false),
            ..SessionPatch::default()
        },
    )
    .unwrap();
    assert_eq!(all(&lib), before);
}

mod counts_property {
    use proptest::prelude::*;

    use super::*;

    const DATES: [&str; 5] = [
        "2025-12-01",
        "2026-03-01",
        "2026-06-01",
        "2026-07-01",
        "2026-09-15",
    ];
    const BASELINE_DATE: &str = "2026-06-01";

    #[derive(Debug, Clone)]
    struct Case {
        /// (baseline count, baseline last played) per track.
        tracks: Vec<(i64, Option<usize>)>,
        /// (played_on, entries as track index (== tracks.len(): unmatched), choice).
        sessions: Vec<(Option<usize>, Vec<usize>, Option<bool>)>,
        /// (session index, excluded).
        toggles: Vec<(usize, bool)>,
    }

    fn case() -> impl Strategy<Value = Case> {
        prop::collection::vec((0i64..5, prop::option::of(0..DATES.len())), 1..5).prop_flat_map(
            |tracks| {
                let n = tracks.len();
                let session = (
                    prop::option::of(0..DATES.len()),
                    prop::collection::vec(0..=n, 0..8),
                    prop::option::of(any::<bool>()),
                );
                (
                    Just(tracks),
                    prop::collection::vec(session, 0..6),
                    prop::collection::vec((0usize..6, any::<bool>()), 0..10),
                )
                    .prop_map(|(tracks, sessions, toggles)| Case {
                        tracks,
                        sessions,
                        toggles,
                    })
            },
        )
    }

    /// The count rules over the model.
    fn expected(c: &Case, excluded: &[bool], t: usize) -> (i64, Option<String>) {
        let (base, base_last) = c.tracks[t];
        let mut count = base;
        let mut last = base_last.map(|d| DATES[d]);
        for ((played_on, entries, _), &ex) in c.sessions.iter().zip(excluded) {
            let n = entries.iter().filter(|&&e| e == t).count() as i64;
            if ex || n == 0 {
                continue;
            }
            count += n;
            if let Some(d) = played_on.map(|d| DATES[d]) {
                last = last.max(Some(d));
            }
        }
        (count, last.map(str::to_owned))
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn count_is_baseline_plus_included_entries(c in case()) {
            let lib = lib();
            set_baseline_date(&lib, BASELINE_DATE);
            let ids: Vec<TrackId> = c.tracks.iter().enumerate().map(|(i, &(n, last))| {
                let id = track(&lib, &format!("/m/t{i}.mp3"), "A", &format!("T{i}"), 200_000);
                set_baseline(&lib, id, n, last.map(|d| DATES[d]));
                id
            }).collect();
            let sessions = c.sessions.iter().enumerate().map(|(i, (d, entries, _))| {
                let entries = entries.iter().map(|&t| if t == ids.len() {
                    entry("/Contents/x/nothing.mp3", "Nobody", "Nothing", 1)
                } else {
                    entry(&format!("/Contents/x/t{t}.mp3"), "A", &format!("T{t}"), 200)
                }).collect();
                dated(&format!("HISTORY {i:03}"), d.map(|d| DATES[d]), entries)
            }).collect();
            let plan = scan(&stick(Some(SERIAL), sessions), &lib).unwrap();
            let mut choices = ImportChoices::default();
            let mut excluded = Vec::new();
            for (p, (_, _, choice)) in plan.sessions.iter().zip(&c.sessions) {
                if let Some(ch) = choice {
                    choices.excluded.insert(p.fingerprint.clone(), *ch);
                }
                excluded.push(choice.unwrap_or(p.default_exclusion.is_some()));
            }
            let report = import(&plan, &choices, &lib).unwrap();
            prop_assert_eq!(report.created.len(), c.sessions.len());
            for (i, &id) in ids.iter().enumerate() {
                prop_assert_eq!(stats(&lib, id), expected(&c, &excluded, i));
            }

            for &(s, ex) in &c.toggles {
                let Some(&sid) = report.created.get(s) else { continue };
                lib.session_update(sid, SessionPatch { excluded: Some(ex), ..SessionPatch::default() }).unwrap();
                excluded[s] = ex;
                for (i, &id) in ids.iter().enumerate() {
                    prop_assert_eq!(stats(&lib, id), expected(&c, &excluded, i));
                }
            }
            // A full recompute agrees with the incremental ones.
            let before: Vec<_> = ids.iter().map(|&id| stats(&lib, id)).collect();
            lib.recompute_play_stats(TrackSet::All).unwrap();
            let after: Vec<_> = ids.iter().map(|&id| stats(&lib, id)).collect();
            prop_assert_eq!(before, after);

            for &sid in &report.created {
                lib.session_delete(sid).unwrap();
            }
            for (i, &id) in ids.iter().enumerate() {
                let (n, last) = c.tracks[i];
                prop_assert_eq!(stats(&lib, id), (n, last.map(|d| DATES[d].to_owned())));
            }
        }
    }
}
