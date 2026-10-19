//! rekordbox XML: parse the fixture, write→parse round trips (fixture and a
//! generated 3,000-track collection), lossless extras, and error handling.

// Test helpers outside `#[test]` fns are not covered by clippy's allow-unwrap-in-tests.
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use reks_core::rbxml::{
    self, location_to_path, Collection, Extra, NodeKind, PositionMark, Product, Tempo, XmlNode,
    XmlTrack,
};

fn fixture_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/rekordbox/collection-small.xml")
}

fn parse_str(s: &str) -> reks_core::Result<Collection> {
    rbxml::parse(s.as_bytes())
}

fn write_string(c: &Collection) -> String {
    let mut out = Vec::new();
    rbxml::write(c, &mut out).unwrap();
    String::from_utf8(out).unwrap()
}

/// (name, type, start, end, num, rgb)
type Mark<'a> = (
    Option<&'a str>,
    u8,
    f64,
    Option<f64>,
    i32,
    Option<(u8, u8, u8)>,
);

fn mark(m: &PositionMark) -> Mark<'_> {
    (m.name.as_deref(), m.kind, m.start, m.end, m.num, m.rgb)
}

#[test]
fn parses_small_fixture() {
    let file = std::fs::File::open(fixture_path()).unwrap();
    let c = rbxml::parse(std::io::BufReader::new(file)).unwrap();

    assert_eq!(c.version, "1.0.0");
    assert_eq!(c.product.name.as_deref(), Some("rekordbox"));
    assert_eq!(c.product.version.as_deref(), Some("7.0.0"));
    assert_eq!(c.product.company.as_deref(), Some("AlphaTheta"));
    assert_eq!(c.tracks.len(), 3);

    let t = &c.tracks[0];
    assert_eq!(t.track_id, 1);
    assert_eq!(t.name.as_deref(), Some("Sine Click 128 (MP3)"));
    assert_eq!(t.artist.as_deref(), Some("reks fixtures"));
    assert_eq!(t.composer.as_deref(), Some(""));
    assert_eq!(t.album.as_deref(), Some("reks fixtures"));
    assert_eq!(t.genre.as_deref(), Some("Techno"));
    assert_eq!(t.kind.as_deref(), Some("MP3 File"));
    assert_eq!(t.size, Some(240_605));
    assert_eq!(t.total_time, Some(30));
    assert_eq!(t.disc_number, Some(0));
    assert_eq!(t.track_number, Some(1));
    assert_eq!(t.year, Some(2026));
    assert_eq!(t.average_bpm, Some(128.0));
    assert_eq!(t.date_added.as_deref(), Some("2026-10-19"));
    assert_eq!(t.date_modified, None);
    assert_eq!(t.bit_rate, Some(64));
    assert_eq!(t.sample_rate, Some(44_100));
    assert_eq!(t.comments.as_deref(), Some("Generated test signal"));
    assert_eq!(t.play_count, Some(3));
    assert_eq!(t.last_played, None);
    assert_eq!(t.rating, Some(255));
    assert_eq!(
        t.location.as_deref(),
        Some("file://localhost/Users/reks/Music/reks%20fixtures/sine-click-128.mp3")
    );
    assert_eq!(t.tonality.as_deref(), Some("Am"));
    assert_eq!(t.mix.as_deref(), Some(""));
    assert_eq!(t.colour, Some(0xFF007F));
    assert!(t.extra.is_empty());

    // Grid: one 128 BPM anchor at 0 on every track.
    for t in &c.tracks {
        assert_eq!(
            t.tempos,
            vec![Tempo {
                inizio: 0.0,
                bpm: 128.0,
                metro: Some("4/4".into()),
                battito: Some(1),
                extra: Extra::new(),
            }]
        );
    }

    // Track 1: two memory cues, hot cues A and B, loop on hot cue C.
    let marks: Vec<_> = t.position_marks.iter().map(mark).collect();
    assert_eq!(
        marks,
        vec![
            (Some(""), 0, 0.0, None, -1, None),
            (Some(""), 0, 15.0, None, -1, None),
            (Some("Intro"), 0, 0.0, None, 0, Some((40, 226, 20))),
            (Some("Break"), 0, 7.5, None, 1, Some((255, 140, 0))),
            (Some("Loop"), 4, 15.0, Some(16.875), 2, Some((255, 140, 0))),
        ]
    );

    let t2 = &c.tracks[1];
    assert_eq!((t2.rating, t2.colour), (Some(153), Some(0x00FF00)));
    assert_eq!(
        t2.position_marks.iter().map(mark).collect::<Vec<_>>(),
        vec![
            (Some(""), 0, 3.75, None, -1, None),
            (Some("Drop"), 0, 7.5, None, 0, Some((230, 40, 40))),
        ]
    );

    let t3 = &c.tracks[2];
    assert_eq!((t3.rating, t3.colour), (Some(0), None));
    assert_eq!(t3.genre.as_deref(), Some("House"));
    assert_eq!(t3.position_marks.len(), 3);

    // Playlists: ROOT → "reks fixtures" → [All fixtures (1,2,3), Lossless (2)].
    let keys = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert_eq!(
        c.playlists,
        XmlNode::folder(
            "ROOT",
            vec![XmlNode::folder(
                "reks fixtures",
                vec![
                    XmlNode::playlist("All fixtures", 0, keys(&["1", "2", "3"])),
                    XmlNode::playlist("Lossless", 0, keys(&["2"])),
                ],
            )],
        )
    );

    assert_eq!(
        location_to_path(t.location.as_deref().unwrap()).unwrap(),
        PathBuf::from("/Users/reks/Music/reks fixtures/sine-click-128.mp3")
    );
}

#[test]
fn fixture_round_trips_and_writes_identically() {
    let src = std::fs::read_to_string(fixture_path()).unwrap();
    let c = parse_str(&src).unwrap();
    let out = write_string(&c);
    assert_eq!(parse_str(&out).unwrap(), c);
    // The fixture uses rekordbox's attribute order and layout, so our output
    // reproduces it byte for byte.
    assert_eq!(out, src);
    assert_eq!(write_string(&parse_str(&out).unwrap()), out);
}

/// Deterministic xorshift so the generated collection is stable.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }
}

const NAMES: &[&str] = &[
    "Plain Title",
    "Beyonce\u{301} (NFD)",
    "Beyonc\u{e9} (NFC)",
    "日本語のトラック",
    "Rock & Roll <Live> \"Quoted\" 'Single'",
    "Line\nbreak\tand tab",
    "Ümlaut — em dash … ellipsis",
    "100% Pure #1 + more",
    "",
];

fn opt(rng: &mut Rng, s: String) -> Option<String> {
    (!rng.chance(10)).then_some(s)
}

fn generated(n: u32) -> Collection {
    let mut rng = Rng(0x5eed_1234_abcd_ef01);
    let mut tracks = Vec::with_capacity(n as usize);
    for id in 1..=n {
        let name = NAMES[rng.below(NAMES.len() as u64) as usize];
        let bpm = 70.0 + rng.below(11_000) as f64 / 100.0;
        let beat = 60.0 / bpm;
        let anchors = 1 + rng.below(3) as usize;
        let tempos = (0..anchors)
            .map(|i| Tempo {
                // ms-precision seconds as rekordbox writes them; one anchor in 7
                // has sub-ms precision to exercise the writer's fallback.
                inizio: if rng.chance(15) {
                    i as f64 * 30.0 + 0.025_125
                } else {
                    (i as u64 * 30_000 + rng.below(1_000)) as f64 / 1000.0
                },
                bpm: if i == 0 {
                    bpm
                } else {
                    bpm + rng.below(300) as f64 / 1000.0
                },
                metro: Some(if rng.chance(5) { "3/4" } else { "4/4" }.into()),
                battito: Some(1 + rng.below(4) as u8),
                extra: Extra::new(),
            })
            .collect();
        let mut marks = Vec::new();
        for _ in 0..rng.below(3) {
            marks.push(PositionMark {
                name: Some(String::new()),
                kind: 0,
                start: rng.below(300_000) as f64 / 1000.0,
                end: None,
                num: -1,
                rgb: None,
                extra: Extra::new(),
            });
        }
        let hot = rng.below(9) as i32; // 0..=8 hot cues
        for slot in 0..hot {
            let start = (slot as f64 * 16.0 * beat * 1000.0).round() / 1000.0;
            let is_loop = rng.chance(20);
            marks.push(PositionMark {
                name: Some(NAMES[rng.below(NAMES.len() as u64) as usize].into()),
                kind: if is_loop { 4 } else { 0 },
                start,
                end: is_loop.then(|| ((start + 4.0 * beat) * 1000.0).round() / 1000.0),
                num: slot,
                rgb: rng.chance(80).then(|| {
                    (
                        rng.below(256) as u8,
                        rng.below(256) as u8,
                        rng.below(256) as u8,
                    )
                }),
                extra: Extra::new(),
            });
        }
        let mut extra = Extra::new();
        if rng.chance(5) {
            extra.insert("FutureAttr".into(), format!("v{id} & <x>"));
        }
        if rng.chance(3) {
            // Unparseable known numeric attribute, kept raw.
            extra.insert("Year".into(), String::new());
        }
        let year = (!extra.contains_key("Year")).then(|| 1990 + rng.below(37) as u32);
        tracks.push(XmlTrack {
            track_id: id,
            name: Some(format!("{name} {id}")),
            artist: opt(&mut rng, format!("Artist {}", id % 97)),
            composer: Some(String::new()),
            album: opt(&mut rng, format!("Album {}", id % 211)),
            grouping: Some(String::new()),
            genre: opt(
                &mut rng,
                ["Techno", "House", "Drum & Bass"][id as usize % 3].into(),
            ),
            kind: Some("MP3 File".into()),
            size: Some(3_000_000 + rng.below(20_000_000)),
            total_time: Some(120 + rng.below(480) as u32),
            disc_number: Some(0),
            track_number: Some(id % 20),
            year,
            average_bpm: Some(bpm),
            date_modified: opt(&mut rng, "2025-01-02".into()),
            date_added: Some("2026-10-19".into()),
            bit_rate: Some(320),
            sample_rate: Some(44_100),
            comments: opt(&mut rng, format!("Comment for {id}\nsecond line")),
            play_count: Some(rng.below(50) as u32),
            last_played: opt(&mut rng, "2026-09-30".into()),
            rating: Some([0, 51, 102, 153, 204, 255][rng.below(6) as usize]),
            location: Some(format!(
                "file://localhost/Users/dj/Music/{}/track%20{id}.mp3",
                ["House", "Techno%20%26%20Co", "Beyonce%CC%81"][id as usize % 3]
            )),
            remixer: Some(String::new()),
            tonality: Some(["Am", "8A", "F#m", "Db"][id as usize % 4].into()),
            label: Some(String::new()),
            mix: Some(String::new()),
            colour: rng.chance(40).then(|| rng.below(0x100_0000) as u32),
            tempos,
            position_marks: marks,
            extra,
        });
    }

    let mut folders = Vec::new();
    for f in 0..10u32 {
        let mut children = Vec::new();
        for p in 0..10u32 {
            let len = rng.below(200);
            let by_location = (f + p) % 4 == 0;
            let keys = (0..len)
                .map(|_| {
                    let t = &tracks[rng.below(u64::from(n)) as usize];
                    if by_location {
                        t.location.clone().unwrap()
                    } else {
                        t.track_id.to_string()
                    }
                })
                .collect();
            children.push(XmlNode::playlist(
                format!("Playlist {f}.{p} & \"set\""),
                u8::from(by_location),
                keys,
            ));
        }
        let mut nested = XmlNode::folder(
            format!("Sub {f}"),
            vec![XmlNode::playlist("Empty", 0, vec![])],
        );
        nested.extra.insert("Unknown".into(), "kept".into());
        children.push(nested);
        children.push(XmlNode::folder("Empty folder", vec![]));
        folders.push(XmlNode::folder(format!("Folder {f}"), children));
    }

    let mut product_extra = Extra::new();
    product_extra.insert("Build".into(), "42".into());
    let mut extra = Extra::new();
    extra.insert("xmlns:x".into(), "urn:example".into());
    Collection {
        version: "1.0.0".into(),
        product: Product {
            name: Some("reks".into()),
            version: Some("0.1.0".into()),
            company: Some("reks contributors".into()),
            extra: product_extra,
        },
        tracks,
        playlists: XmlNode::folder("ROOT", folders),
        extra,
    }
}

#[test]
fn generated_3000_tracks_round_trip() {
    let c = generated(3_000);
    let xml = write_string(&c);
    let parsed = parse_str(&xml).unwrap();
    assert_eq!(parsed.tracks.len(), 3_000);
    // Compare track by track first for a readable failure.
    for (a, b) in parsed.tracks.iter().zip(&c.tracks) {
        assert_eq!(a, b, "track {}", b.track_id);
    }
    assert_eq!(parsed, c);
    assert_eq!(write_string(&parsed), xml);

    // Derived counts are written from content.
    assert!(xml.contains("<COLLECTION Entries=\"3000\">"));
    assert!(xml.contains("<NODE Type=\"0\" Name=\"ROOT\" Count=\"10\">"));
    assert!(xml.contains("<NODE Name=\"Empty\" Type=\"1\" KeyType=\"0\" Entries=\"0\"/>"));
}

#[test]
fn unknown_attributes_and_bad_values_survive() {
    let src = r#"<?xml version="1.0" encoding="UTF-8"?>
<DJ_PLAYLISTS Version="1.0.0" Extra="root">
  <PRODUCT Name="rekordbox" Version="6.8.5" Company="AlphaTheta" Note="x"/>
  <COLLECTION Entries="1">
    <TRACK TrackID="7" Name="a&amp;b &#10;c" Year="" Rating="lots" Colour="red" AverageBpm="NaN" NewAttr="&lt;v&gt;">
      <TEMPO Inizio="0.025" Bpm="120.00" Metro="4/4" Battito="x" Phase="1"/>
      <POSITION_MARK Name="" Type="0" Start="1.000" Num="0" Red="300" Green="0" Blue="0"/>
      <POSITION_MARK Type="0" Start="2.000" Num="1" Red="10"/>
      <VENDOR_THING A="1"><NESTED/></VENDOR_THING>
    </TRACK>
  </COLLECTION>
</DJ_PLAYLISTS>"#;
    let c = parse_str(src).unwrap();
    assert_eq!(c.extra.get("Extra").map(String::as_str), Some("root"));
    assert_eq!(c.product.extra.get("Note").map(String::as_str), Some("x"));
    let t = &c.tracks[0];
    assert_eq!(t.name.as_deref(), Some("a&b \nc"));
    assert_eq!(
        (t.year, t.rating, t.colour, t.average_bpm),
        (None, None, None, None)
    );
    for (k, v) in [
        ("Year", ""),
        ("Rating", "lots"),
        ("Colour", "red"),
        ("AverageBpm", "NaN"),
        ("NewAttr", "<v>"),
    ] {
        assert_eq!(t.extra.get(k).map(String::as_str), Some(v), "{k}");
    }
    assert_eq!(t.tempos[0].battito, None);
    assert_eq!(t.tempos[0].extra.len(), 2);
    let m = &t.position_marks;
    assert_eq!(m[0].rgb, None);
    assert_eq!(m[0].extra.get("Red").map(String::as_str), Some("300"));
    assert_eq!(m[1].name, None);
    assert_eq!(m[1].extra.get("Red").map(String::as_str), Some("10"));
    // No PLAYLISTS element: an empty ROOT folder.
    assert_eq!(c.playlists, XmlNode::folder("ROOT", vec![]));

    let out = write_string(&c);
    assert_eq!(parse_str(&out).unwrap(), c);
    assert!(out.contains("Name=\"a&amp;b &#10;c\""));
}

#[test]
fn playlist_without_key_type_and_empty_nodes() {
    let src = r#"<DJ_PLAYLISTS Version="1.0.0"><PRODUCT/><COLLECTION Entries="0"/>
<PLAYLISTS><NODE Type="0" Name="ROOT" Count="5">
<NODE Type="1" Name="No key type" Entries="9"><TRACK Key="1"/><TRACK Key="2"></TRACK></NODE>
<NODE Type="0" Name="F"/>
</NODE></PLAYLISTS></DJ_PLAYLISTS>"#;
    let c = parse_str(src).unwrap();
    let NodeKind::Folder(children) = &c.playlists.kind else {
        panic!("root is not a folder");
    };
    assert_eq!(
        children,
        &vec![
            XmlNode::playlist("No key type", 0, vec!["1".into(), "2".into()]),
            XmlNode::folder("F", vec![]),
        ]
    );
}

#[test]
fn errors_are_reported_not_panics() {
    let cases = [
        ("", "no root"),
        ("<OTHER/>", "wrong root"),
        ("<DJ_PLAYLISTS><COLLECTION>", "truncated"),
        (
            "<DJ_PLAYLISTS><COLLECTION></DJ_PLAYLISTS>",
            "mismatched end tag",
        ),
        (
            r#"<DJ_PLAYLISTS><COLLECTION><TRACK Name="x"/></COLLECTION></DJ_PLAYLISTS>"#,
            "missing TrackID",
        ),
        (
            r#"<DJ_PLAYLISTS><COLLECTION><TRACK TrackID="-1"/></COLLECTION></DJ_PLAYLISTS>"#,
            "bad TrackID",
        ),
        (
            r#"<DJ_PLAYLISTS><COLLECTION><TRACK TrackID="1"><TEMPO Inizio="0" Bpm="fast"/></TRACK></COLLECTION></DJ_PLAYLISTS>"#,
            "bad Bpm",
        ),
        (
            r#"<DJ_PLAYLISTS><COLLECTION><TRACK TrackID="1"><POSITION_MARK Type="0" Start="1"/></TRACK></COLLECTION></DJ_PLAYLISTS>"#,
            "missing Num",
        ),
        (
            r#"<DJ_PLAYLISTS><PLAYLISTS><NODE Name="ROOT"/></PLAYLISTS></DJ_PLAYLISTS>"#,
            "NODE without Type",
        ),
        (
            r#"<DJ_PLAYLISTS><PLAYLISTS><NODE Type="2" Name="x"/></PLAYLISTS></DJ_PLAYLISTS>"#,
            "NODE Type 2",
        ),
        (
            r#"<DJ_PLAYLISTS><COLLECTION><TRACK TrackID="1" Name="a" Name="b"/></COLLECTION></DJ_PLAYLISTS>"#,
            "duplicate attribute",
        ),
        (
            r#"<DJ_PLAYLISTS><COLLECTION><TRACK TrackID="1" Name="&bogus;"/></COLLECTION></DJ_PLAYLISTS>"#,
            "unknown entity",
        ),
    ];
    for (src, what) in cases {
        let err = parse_str(src).err();
        assert!(err.is_some(), "{what}: expected an error");
    }
    // Invalid UTF-8 bytes in the document.
    assert!(rbxml::parse(&b"<DJ_PLAYLISTS Version=\"\xff\"/>"[..]).is_err());
}

#[test]
fn minimal_documents() {
    let c = parse_str(r#"<?xml version="1.0"?><DJ_PLAYLISTS Version="1.0.0"/>"#).unwrap();
    assert_eq!(c, Collection::default());
    let c =
        parse_str("\u{feff}<DJ_PLAYLISTS Version=\"1.0.0\"><!-- c --><COLLECTION/></DJ_PLAYLISTS>")
            .unwrap();
    assert!(c.tracks.is_empty());
    let out = write_string(&Collection::default());
    assert_eq!(parse_str(&out).unwrap(), Collection::default());
}
