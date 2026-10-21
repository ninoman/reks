//! `Library::tracks` and `Library::facets` on a seeded library: every filter,
//! every sort column, paging, full-text search and a 10,000-track run.

// Test helpers outside `#[test]` fns are not covered by clippy's allow-unwrap-in-tests.
#![allow(clippy::unwrap_used)]

use std::time::Instant;

use reks_core::{Filter, Key, Library, Sort, SortColumn, TrackQuery};
use rusqlite::Connection;

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

    fn ids(&self, q: &TrackQuery) -> Vec<i64> {
        self.lib
            .tracks(q)
            .unwrap()
            .rows
            .iter()
            .map(|r| r.id)
            .collect()
    }

    /// Ids matching `filters` / `text`, in id order.
    fn matching(&self, text: Option<&str>, filters: Vec<Filter>) -> Vec<i64> {
        let q = TrackQuery {
            text: text.map(str::to_owned),
            filters,
            limit: 1000,
            ..TrackQuery::default()
        };
        let page = self.lib.tracks(&q).unwrap();
        let mut ids: Vec<i64> = page.rows.iter().map(|r| r.id).collect();
        assert_eq!(page.total, ids.len() as u64, "total for {q:?}");
        ids.sort_unstable();
        ids
    }

    fn sorted(&self, column: SortColumn, descending: bool, filters: Vec<Filter>) -> Vec<i64> {
        self.ids(&TrackQuery {
            filters,
            sort: Sort { column, descending },
            limit: 1000,
            ..TrackQuery::default()
        })
    }
}

#[derive(Default)]
struct T {
    title: Option<&'static str>,
    artist: Option<&'static str>,
    album: Option<&'static str>,
    genre: Option<&'static str>,
    label: Option<&'static str>,
    comment: Option<&'static str>,
    key: Option<&'static str>,
    bpm_x100: Option<i64>,
    rating: i64,
    color: Option<&'static str>,
    duration_ms: Option<i64>,
    year: Option<i64>,
    kind: &'static str,
    bit_rate_kbps: Option<i64>,
    date_added: &'static str,
    play_count: i64,
    missing: bool,
    grid: bool,
}

fn insert(c: &Connection, n: usize, t: &T) -> i64 {
    c.execute(
        "INSERT INTO track (path, file_size, file_mtime, kind, title, artist, album, genre,
                            label, comment, key, bpm_x100, rating, color, duration_ms, year,
                            bit_rate_kbps, date_added, play_count, missing, created_at, updated_at)
         VALUES (?1, 1, '2026-01-01T00:00:00Z', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                 ?13, ?14, ?15, ?16, ?17, ?18, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        rusqlite::params![
            format!("/music/{n}.{}", t.kind),
            t.kind,
            t.title,
            t.artist,
            t.album,
            t.genre,
            t.label,
            t.comment,
            t.key,
            t.bpm_x100,
            t.rating,
            t.color,
            t.duration_ms,
            t.year,
            t.bit_rate_kbps,
            t.date_added,
            t.play_count,
            t.missing,
        ],
    )
    .unwrap();
    let id = c.last_insert_rowid();
    if t.grid {
        c.execute(
            "INSERT INTO beatgrid (track_id, position_ms, bpm_x100, source)
             VALUES (?1, 0.0, 12800, 'rekordbox')",
            [id],
        )
        .unwrap();
    }
    id
}

fn playlist(c: &Connection, id: i64, parent: Option<i64>, folder: bool, name: &str) {
    c.execute(
        "INSERT INTO playlist (id, parent_id, is_folder, name, sort_order, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        rusqlite::params![id, parent, folder, name],
    )
    .unwrap();
}

fn add_to(c: &Connection, playlist: i64, tracks: &[i64]) {
    for (pos, t) in tracks.iter().enumerate() {
        c.execute(
            "INSERT INTO playlist_track (playlist_id, track_id, position) VALUES (?1, ?2, ?3)",
            rusqlite::params![playlist, t, pos as i64],
        )
        .unwrap();
    }
}

/// Five tracks (ids 1..=5) and a playlist tree:
/// folder 10 { playlist 11 [3, 1], folder 13 { playlist 14 [5] } }, playlist 12 [2].
fn seeded() -> Env {
    let env = Env::new();
    let c = env.db();
    let tracks = [
        T {
            title: Some("Midnight Groove"),
            artist: Some("Beyoncé"),
            album: Some("A1"),
            genre: Some("House"),
            comment: Some("warm-up"),
            key: Some("Am"),
            bpm_x100: Some(12800),
            rating: 5,
            color: Some("red"),
            duration_ms: Some(300_000),
            year: Some(2020),
            kind: "mp3",
            bit_rate_kbps: Some(320),
            date_added: "2026-01-03",
            play_count: 10,
            grid: true,
            ..T::default()
        },
        T {
            title: Some("Sunrise Dub"),
            artist: Some("Dub Collective"),
            genre: Some("Dub"),
            key: Some("Dm"),
            bpm_x100: Some(9000),
            rating: 3,
            color: Some("blue"),
            duration_ms: Some(240_000),
            year: Some(2018),
            kind: "flac",
            date_added: "2026-01-01",
            missing: true,
            ..T::default()
        },
        T {
            title: Some("Deep Night"),
            artist: Some("Artist C"),
            album: Some("B"),
            genre: Some("house"),
            key: Some("F#m"),
            bpm_x100: Some(12400),
            kind: "wav",
            bit_rate_kbps: Some(1411),
            date_added: "2026-01-02",
            play_count: 5,
            grid: true,
            ..T::default()
        },
        T {
            title: Some(""),
            album: Some("a0"),
            rating: 1,
            color: Some("pink"),
            duration_ms: Some(200_000),
            year: Some(2021),
            kind: "mp3",
            bit_rate_kbps: Some(128),
            date_added: "2026-01-04",
            grid: true,
            ..T::default()
        },
        T {
            title: Some("Café del Mar"),
            artist: Some("Energy 52"),
            genre: Some("Trance"),
            label: Some("Café Records"),
            key: Some("B"),
            bpm_x100: Some(13600),
            rating: 4,
            color: Some("green"),
            duration_ms: Some(420_000),
            year: Some(1993),
            kind: "aiff",
            date_added: "2026-01-05",
            play_count: 2,
            ..T::default()
        },
    ];
    for (n, t) in tracks.iter().enumerate() {
        assert_eq!(insert(&c, n, t), n as i64 + 1);
    }
    playlist(&c, 10, None, true, "Folder");
    playlist(&c, 11, Some(10), false, "Set A");
    playlist(&c, 12, None, false, "Set B");
    playlist(&c, 13, Some(10), true, "Sub");
    playlist(&c, 14, Some(13), false, "Set C");
    add_to(&c, 11, &[3, 1]);
    add_to(&c, 12, &[2]);
    add_to(&c, 14, &[5]);
    env
}

fn key(s: &str) -> Key {
    Key::parse(s).unwrap()
}

#[test]
fn filters() {
    let env = seeded();
    let m = |f: Filter| env.matching(None, vec![f]);
    assert_eq!(env.matching(None, vec![]), [1, 2, 3, 4, 5]);
    assert_eq!(
        m(Filter::Genre {
            genre: "HOUSE".into()
        }),
        [1, 3]
    );
    assert_eq!(
        m(Filter::KeyIn {
            keys: vec![key("8A"), key("B")]
        }),
        [1, 5]
    );
    assert_eq!(m(Filter::KeyIn { keys: vec![] }), Vec::<i64>::new());
    assert_eq!(
        m(Filter::BpmRange {
            min: 12400,
            max: 12800
        }),
        [1, 3]
    );
    assert_eq!(
        m(Filter::Color {
            color: "Red".into()
        }),
        [1]
    );
    assert_eq!(m(Filter::RatingMin { rating: 4 }), [1, 5]);
    assert_eq!(m(Filter::RatingMin { rating: 0 }), [1, 2, 3, 4, 5]);
    assert_eq!(m(Filter::Playlist { id: 11 }), [1, 3]);
    assert_eq!(m(Filter::Playlist { id: 12 }), [2]);
    assert_eq!(
        m(Filter::Playlist { id: 10 }),
        [1, 3, 5],
        "folder includes sub-folders"
    );
    assert_eq!(m(Filter::Playlist { id: 999 }), Vec::<i64>::new());
    assert_eq!(m(Filter::Missing { missing: true }), [2]);
    assert_eq!(m(Filter::Missing { missing: false }), [1, 3, 4, 5]);

    let combined = vec![
        Filter::Genre {
            genre: "house".into(),
        },
        Filter::Missing { missing: false },
        Filter::RatingMin { rating: 1 },
    ];
    assert_eq!(env.matching(None, combined), [1]);
    assert_eq!(
        env.matching(Some("night"), vec![Filter::Playlist { id: 10 }]),
        [3]
    );
}

#[test]
fn row_fields() {
    let env = seeded();
    let page = env
        .lib
        .tracks(&TrackQuery {
            text: Some("midnight".into()),
            ..TrackQuery::default()
        })
        .unwrap();
    let r = &page.rows[0];
    assert_eq!(r.title.as_deref(), Some("Midnight Groove"));
    assert_eq!(r.key.as_deref(), Some("Am"));
    assert_eq!(r.bpm_x100, Some(12800));
    assert_eq!(r.color.as_deref(), Some("red"));
    assert!(!r.missing);
    assert!(!r.needs_analysis);
    let r2 = &env.lib.tracks(&TrackQuery::default()).unwrap().rows;
    let dub = r2.iter().find(|r| r.id == 2).unwrap();
    assert!(dub.missing && dub.needs_analysis);
}

#[test]
fn sort_orders() {
    use SortColumn::*;
    let env = seeded();
    // (column, ascending, descending); empty values last both ways, ties by id.
    let cases: [(SortColumn, [i64; 5], [i64; 5]); 15] = [
        (Title, [5, 3, 1, 2, 4], [2, 1, 3, 5, 4]),
        (Artist, [3, 1, 2, 5, 4], [5, 2, 1, 3, 4]),
        (Album, [4, 1, 3, 2, 5], [3, 1, 4, 2, 5]),
        (Genre, [2, 1, 3, 5, 4], [5, 1, 3, 2, 4]),
        (Label, [5, 1, 2, 3, 4], [5, 1, 2, 3, 4]),
        (Bpm, [2, 3, 1, 5, 4], [5, 1, 3, 2, 4]),
        (Key, [5, 2, 1, 3, 4], [3, 1, 2, 5, 4]),
        (Rating, [3, 4, 2, 5, 1], [1, 5, 2, 4, 3]),
        (Color, [4, 1, 5, 2, 3], [2, 5, 1, 4, 3]),
        (Duration, [4, 2, 1, 5, 3], [5, 1, 2, 4, 3]),
        (Year, [5, 2, 1, 4, 3], [4, 1, 2, 5, 3]),
        (DateAdded, [2, 3, 1, 4, 5], [5, 4, 1, 3, 2]),
        (PlayCount, [2, 4, 5, 3, 1], [1, 3, 5, 2, 4]),
        (Kind, [5, 2, 1, 4, 3], [3, 1, 4, 2, 5]),
        (BitRate, [4, 1, 3, 2, 5], [3, 1, 4, 2, 5]),
    ];
    for (col, asc, desc) in cases {
        assert_eq!(env.sorted(col, false, vec![]), asc, "{col:?} asc");
        assert_eq!(env.sorted(col, true, vec![]), desc, "{col:?} desc");
    }

    let in_set_a = vec![Filter::Playlist { id: 11 }];
    assert_eq!(
        env.sorted(PlaylistPosition, false, in_set_a.clone()),
        [3, 1]
    );
    assert_eq!(env.sorted(PlaylistPosition, true, in_set_a), [1, 3]);
    assert_eq!(env.sorted(PlaylistPosition, false, vec![]), [1, 2, 3, 4, 5]);
}

#[test]
fn paging_and_total() {
    let env = seeded();
    let page = |offset, limit| {
        env.lib
            .tracks(&TrackQuery {
                offset,
                limit,
                ..TrackQuery::default()
            })
            .unwrap()
    };
    let all = env.sorted(SortColumn::Title, false, vec![]);
    let mut joined = Vec::new();
    for offset in [0, 2, 4] {
        let p = page(offset, 2);
        assert_eq!(p.total, 5);
        joined.extend(p.rows.iter().map(|r| r.id));
    }
    assert_eq!(joined, all);

    let past_end = page(10, 2);
    assert!(past_end.rows.is_empty());
    assert_eq!(past_end.total, 5);
    assert_eq!(page(0, 0).rows.len(), 1, "limit 0 is clamped to 1");

    let filtered = env
        .lib
        .tracks(&TrackQuery {
            filters: vec![Filter::Missing { missing: false }],
            limit: 1,
            ..TrackQuery::default()
        })
        .unwrap();
    assert_eq!((filtered.rows.len(), filtered.total), (1, 4));
}

#[test]
fn full_text_search() {
    let env = seeded();
    let s = |text: &str| env.matching(Some(text), vec![]);
    let none: Vec<i64> = Vec::new();
    assert_eq!(s("groove"), [1]);
    assert_eq!(s("midn"), [1], "prefix");
    assert_eq!(s("night"), [3], "prefix of a word, not a substring");
    assert_eq!(s("deep NIGHT"), [3], "every word must match");
    assert_eq!(s("deep groove"), none);
    assert_eq!(s("house"), [1, 3], "genre is indexed");
    assert_eq!(s("collective"), [2], "artist");
    assert_eq!(s("records"), [5], "label");
    // Diacritics both ways (FTS5 unicode61, remove_diacritics).
    assert_eq!(s("beyonce"), [1]);
    assert_eq!(s("Beyoncé"), [1]);
    assert_eq!(s("CAFÉ"), [5]);
    assert_eq!(s("cafe del"), [5]);
    // Punctuation and FTS syntax are plain text.
    assert_eq!(s("warm-up"), [1]);
    assert_eq!(s("groove -dub"), none);
    assert_eq!(s("NEAR(groove"), none);
    assert_eq!(s("\"groove"), [1]);
    assert_eq!(s("groove OR dub"), none);
    assert_eq!(s("*"), [1, 2, 3, 4, 5], "no words = no text filter");
    assert_eq!(s("   "), [1, 2, 3, 4, 5]);
}

#[test]
fn facets() {
    let env = seeded();
    let f = env.lib.facets().unwrap();
    assert_eq!(f.genres.len(), 3);
    assert_eq!(f.genres[0], "Dub");
    assert!(f.genres[1].eq_ignore_ascii_case("house"));
    assert_eq!(f.genres[2], "Trance");
    assert_eq!(f.keys.len(), 24);
    assert_eq!(f.keys[0].camelot, "1A");
    assert_eq!(f.keys[0].key, key("Abm"));
    assert_eq!(f.colours[0], "pink");
    let tree: Vec<(i64, u32)> = f.playlists.iter().map(|p| (p.id, p.depth)).collect();
    assert_eq!(tree, [(10, 0), (11, 1), (13, 1), (14, 2), (12, 0)]);
}

/// The JSON `src/lib/ipc.ts` sends and expects. Changing these shapes means
/// changing the TS types too.
#[test]
fn serde_shapes() {
    let json = r#"{
        "text": "groove",
        "filters": [
            {"type": "genre", "genre": "House"},
            {"type": "keyIn", "keys": ["Am", "8A", "F#m"]},
            {"type": "bpmRange", "min": 12000, "max": 13000},
            {"type": "color", "color": "red"},
            {"type": "ratingMin", "rating": 3},
            {"type": "playlist", "id": 11},
            {"type": "missing", "missing": false},
            {"type": "neverPlayed"},
            {"type": "playCountRange", "min": 1, "max": 5},
            {"type": "playedSince", "date": "2026-01-01"},
            {"type": "tagAny", "ids": [3, 4]}
        ],
        "sort": {"column": "dateAdded", "descending": true},
        "offset": 200,
        "limit": 200
    }"#;
    let q: TrackQuery = serde_json::from_str(json).unwrap();
    assert_eq!(q.filters.len(), 11);
    assert_eq!(q.filters[10], Filter::TagAny { ids: vec![3, 4] });
    assert_eq!(q.filters[7], Filter::NeverPlayed);
    assert_eq!(
        q.filters[1],
        Filter::KeyIn {
            keys: vec![key("Am"), key("Am"), key("F#m")]
        }
    );
    assert_eq!(q.sort.column, SortColumn::DateAdded);
    assert_eq!((q.offset, q.limit), (200, 200));

    let minimal: TrackQuery = serde_json::from_str("{}").unwrap();
    assert_eq!(minimal, TrackQuery::default());

    let env = seeded();
    let page = env.lib.tracks(&TrackQuery::default()).unwrap();
    let v = serde_json::to_value(&page).unwrap();
    assert_eq!(v["total"], 5);
    let mut keys: Vec<&str> = v["rows"][0]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "album",
            "artist",
            "bitRateKbps",
            "bpmX100",
            "color",
            "comment",
            "dateAdded",
            "durationMs",
            "entryPosition",
            "genre",
            "id",
            "key",
            "kind",
            "label",
            "lastPlayed",
            "missing",
            "needsAnalysis",
            "playCount",
            "rating",
            "remixer",
            "tags",
            "title",
            "year"
        ]
    );
    let f = serde_json::to_value(env.lib.facets().unwrap()).unwrap();
    assert_eq!(f["keys"][0]["key"], "Abm");
    assert_eq!(f["playlists"][1]["parentId"], 10);
    assert_eq!(f["playlists"][1]["isFolder"], false);
}

/// 10,000 generated tracks: logs query times (no timing assertion; run with
/// `--release --nocapture` for meaningful numbers).
#[test]
fn ten_thousand_tracks() {
    const N: usize = 10_000;
    const GENRES: [&str; 8] = [
        "House",
        "Techno",
        "Trance",
        "Dub",
        "Disco",
        "Drum & Bass",
        "Ambient",
        "Electro",
    ];
    const WORDS: [&str; 12] = [
        "Midnight", "Groove", "Sunrise", "Deep", "Night", "Café", "Echo", "Pulse", "Velvet",
        "Orbit", "Signal", "Drift",
    ];
    let env = Env::new();
    let keys: Vec<&str> = Key::all().map(Key::canonical).collect();
    let colours = [
        "pink", "red", "orange", "yellow", "green", "aqua", "blue", "purple",
    ];
    let mut c = env.db();
    let tx = c.transaction().unwrap();
    for i in 0..N {
        tx.execute(
            "INSERT INTO track (path, file_size, file_mtime, kind, title, artist, genre, key,
                                bpm_x100, rating, color, duration_ms, date_added, missing,
                                created_at, updated_at)
             VALUES (?1, 1, '', 'mp3', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, '', '')",
            rusqlite::params![
                format!("/music/{i}.mp3"),
                format!("{} {} {i}", WORDS[i % 12], WORDS[(i / 12) % 12]),
                format!("Artist {}", i % 700),
                GENRES[i % 8],
                keys[i % 24],
                9000 + (i % 5000) as i64,
                (i % 6) as i64,
                (i % 3 == 0).then(|| colours[i % 8]),
                180_000 + (i % 240) as i64 * 1000,
                format!("2026-{:02}-{:02}", 1 + i % 12, 1 + i % 28),
                i % 50 == 0,
            ],
        )
        .unwrap();
        if i % 3 != 0 {
            tx.execute(
                "INSERT INTO beatgrid (track_id, position_ms, bpm_x100, source)
                 VALUES (?1, 0.0, 12800, 'rekordbox')",
                [tx.last_insert_rowid()],
            )
            .unwrap();
        }
    }
    tx.commit().unwrap();

    let time = |label: &str, q: TrackQuery| {
        let t = Instant::now();
        let page = env.lib.tracks(&q).unwrap();
        eprintln!(
            "10k {label}: {:?} ({} rows, total {})",
            t.elapsed(),
            page.rows.len(),
            page.total
        );
        page
    };
    let page = TrackQuery {
        limit: 200,
        ..TrackQuery::default()
    };
    let all = time("first page by title", page.clone());
    assert_eq!((all.rows.len(), all.total), (200, N as u64));
    time(
        "last page by title",
        TrackQuery {
            offset: 9_800,
            ..page.clone()
        },
    );
    let s = time(
        "search 'midn gro'",
        TrackQuery {
            text: Some("midn gro".into()),
            ..page.clone()
        },
    );
    assert!(s.total > 0);
    time(
        "search 'cafe' by bpm desc",
        TrackQuery {
            text: Some("cafe".into()),
            sort: Sort {
                column: SortColumn::Bpm,
                descending: true,
            },
            ..page.clone()
        },
    );
    time(
        "genre + present, by key",
        TrackQuery {
            filters: vec![
                Filter::Genre {
                    genre: "techno".into(),
                },
                Filter::Missing { missing: false },
            ],
            sort: Sort {
                column: SortColumn::Key,
                descending: false,
            },
            ..page.clone()
        },
    );
    time(
        "bpm range + colour, by date added desc",
        TrackQuery {
            filters: vec![
                Filter::BpmRange {
                    min: 12000,
                    max: 13000,
                },
                Filter::Color {
                    color: "red".into(),
                },
            ],
            sort: Sort {
                column: SortColumn::DateAdded,
                descending: true,
            },
            ..page.clone()
        },
    );
    let big = time(
        "limit 5000 (clamped)",
        TrackQuery {
            limit: 5_000,
            ..page
        },
    );
    assert_eq!(big.rows.len(), reks_core::query::MAX_PAGE as usize);
    let t = Instant::now();
    let f = env.lib.facets().unwrap();
    eprintln!("10k facets: {:?} ({} genres)", t.elapsed(), f.genres.len());
}
