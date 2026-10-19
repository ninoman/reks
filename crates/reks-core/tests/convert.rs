//! Converter plan/apply on a temp library (phase 2.6 task 5): Replace keeps
//! cues, grid, playlists and play history; Add as new copies them; a re-scan
//! adds nothing; name collisions; cancel leaves nothing behind; skips.

// Test helpers outside `#[test]` fns are not covered by clippy's allow-unwrap-in-tests.
#![allow(clippy::unwrap_used)]

use std::cell::RefCell;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use reks_core::convert::{
    self, ConvertEvent, ConvertOptions, ConvertSpec, Depth, Destination, LibraryMode, OriginalFate,
    Originals, Preset, Rate, SkipReason, Target, Warning, TEMP_SUFFIX,
};
use reks_core::{scan, CancelToken, Library, TrackId};
use rusqlite::Connection;

struct Env {
    dir: tempfile::TempDir,
    lib: Library,
}

impl Env {
    /// A library with `music/sine.<ext>` for each of `exts`, scanned in.
    fn with(exts: &[&str]) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let lib =
            Library::open(&dir.path().join("reks.sqlite"), &dir.path().join("cache")).unwrap();
        let env = Env { dir, lib };
        std::fs::create_dir_all(env.root()).unwrap();
        for ext in exts {
            let src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../fixtures/audio")
                .join(format!("sine-click-128.{ext}"));
            std::fs::copy(src, env.root().join(format!("sine.{ext}"))).unwrap();
        }
        let report = env.scan();
        assert_eq!(report.added, exts.len(), "{report:?}");
        env
    }

    fn root(&self) -> PathBuf {
        self.dir.path().join("music")
    }

    fn scan(&self) -> reks_core::ScanReport {
        scan(&[self.root()], &self.lib, &CancelToken::new(), |_| {}).unwrap()
    }

    fn db(&self) -> Connection {
        Connection::open(self.dir.path().join("reks.sqlite")).unwrap()
    }

    fn id(&self, file_name: &str) -> TrackId {
        self.db()
            .query_row(
                "SELECT id FROM track WHERE path LIKE '%' || ?1",
                [file_name],
                |r| r.get(0),
            )
            .unwrap()
    }

    fn count(&self, sql: &str) -> i64 {
        self.db().query_row(sql, [], |r| r.get(0)).unwrap()
    }

    /// Cues, a two-anchor grid, a playlist entry, rating and play history.
    fn decorate(&self, id: TrackId) {
        let c = self.db();
        c.execute_batch(&format!(
            "INSERT INTO cue (track_id, slot, kind, start_ms, name, color_rgb, source)
               VALUES ({id}, 0, 0, 1406.25, 'Drop', 16711680, 'rekordbox');
             INSERT INTO cue (track_id, slot, kind, start_ms, end_ms, source)
               VALUES ({id}, NULL, 4, 1875.0, 3750.0, 'manual');
             INSERT INTO beatgrid (track_id, position_ms, bpm_x100, beat_in_bar, source)
               VALUES ({id}, 1.45, 12800, 1, 'rekordbox'), ({id}, 15000.0, 12800, 3, 'rekordbox');
             INSERT INTO playlist (id, name, sort_order, created_at, updated_at)
               VALUES (1, 'Set', 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
             INSERT INTO playlist_track (playlist_id, track_id, position) VALUES (1, {id}, 0);
             UPDATE track SET rating = 4, play_count = 7, last_played = '2026-10-01',
               rb_location = 'file://localhost/old/sine.wav' WHERE id = {id};"
        ))
        .unwrap();
    }

    /// Everything attached to a track, for before/after comparisons.
    fn attached(&self, id: TrackId) -> Vec<String> {
        let c = self.db();
        let mut out = Vec::new();
        for sql in [
            "SELECT slot, kind, start_ms, end_ms, name, color_rgb, source FROM cue
             WHERE track_id = ?1 ORDER BY id",
            "SELECT position_ms, bpm_x100, meter_num, meter_den, beat_in_bar, source, NULL
             FROM beatgrid WHERE track_id = ?1 ORDER BY id",
        ] {
            let mut stmt = c.prepare(sql).unwrap();
            let rows = stmt
                .query_map([id], |r| {
                    let cols: Vec<rusqlite::types::Value> =
                        (0..7).map(|i| r.get(i)).collect::<Result<_, _>>()?;
                    Ok(format!("{cols:?}"))
                })
                .unwrap();
            out.extend(rows.map(Result::unwrap));
        }
        out
    }

    /// Every row of the tables a conversion may touch.
    fn snapshot(&self) -> String {
        let c = self.db();
        let mut out = String::new();
        for table in [
            "track",
            "cue",
            "beatgrid",
            "playlist_track",
            "superseded_file",
        ] {
            let mut stmt = c.prepare(&format!("SELECT * FROM {table}")).unwrap();
            let n = stmt.column_count();
            let rows = stmt
                .query_map([], |r| {
                    let cols: Vec<rusqlite::types::Value> =
                        (0..n).map(|i| r.get(i)).collect::<Result<_, _>>()?;
                    Ok(format!("{cols:?}"))
                })
                .unwrap();
            for row in rows {
                out.push_str(&format!("{table}: {}\n", row.unwrap()));
            }
        }
        out
    }

    /// The track row as text.
    fn row(&self, id: TrackId) -> String {
        let c = self.db();
        let mut stmt = c.prepare("SELECT * FROM track WHERE id = ?1").unwrap();
        let n = stmt.column_count();
        stmt.query_row([id], |r| {
            let cols: Vec<rusqlite::types::Value> =
                (0..n).map(|i| r.get(i)).collect::<Result<_, _>>()?;
            Ok(format!("{cols:?}"))
        })
        .unwrap()
    }

    fn files(&self) -> Vec<String> {
        let mut names: Vec<String> = walk(&self.root())
            .iter()
            .map(|p| p.strip_prefix(self.root()).unwrap().display().to_string())
            .collect();
        names.sort();
        names
    }
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else {
            out.push(p);
        }
    }
    out
}

fn run(env: &Env, plan: &convert::ConvertPlan) -> convert::ConvertReport {
    convert::apply(&env.lib, plan, &|_| {}, &CancelToken::new()).unwrap()
}

fn spec(target: Target) -> ConvertSpec {
    ConvertSpec {
        target,
        depth: Depth::Keep,
        rate: Rate::Keep,
    }
}

#[test]
fn replace_keeps_everything_attached_and_rescan_adds_nothing() {
    let env = Env::with(&["wav"]);
    let id = env.id("sine.wav");
    env.decorate(id);
    let before = env.attached(id);
    assert_eq!(before.len(), 4);
    // Fill the overview cache, which Replace must drop.
    env.lib.overview_waveform(id).unwrap();
    let overview_dir = env.dir.path().join("cache/waveform/overview");
    assert_eq!(std::fs::read_dir(&overview_dir).unwrap().count(), 1);
    let original = std::fs::read(env.root().join("sine.wav")).unwrap();

    let plan = convert::plan(
        &env.lib,
        &[id],
        &Preset::OlderPlayers.spec(),
        &Preset::OlderPlayers.options(),
    )
    .unwrap();
    assert_eq!(plan.items.len(), 1, "{plan:?}");
    let item = &plan.items[0];
    assert_eq!(item.from.kind, "wav");
    assert_eq!(
        (item.to.kind, item.to.sample_rate, item.to.bit_depth),
        ("aiff", 44_100, 16)
    );
    assert_eq!(plan.bytes_before, original.len() as u64);
    assert_eq!(plan.bytes_after_est, 2_646_000 + 64);
    assert!(plan
        .warnings
        .contains(&Warning::RekordboxAnalysisDropped { track_id: id }));

    let events = RefCell::new(Vec::new());
    let report = convert::apply(
        &env.lib,
        &plan,
        &|e| events.borrow_mut().push(e),
        &CancelToken::new(),
    )
    .unwrap();
    assert!(report.failed.is_empty(), "{report:?}");
    assert_eq!(report.converted.len(), 1);
    let done = &report.converted[0];
    assert_eq!(done.new_track_id, id);
    assert_eq!(done.original, OriginalFate::Kept);
    assert!(done.dest.ends_with("sine.aiff"));
    let events = events.into_inner();
    assert!(matches!(
        events.first(),
        Some(ConvertEvent::Started {
            index: 0,
            total: 1,
            ..
        })
    ));
    assert!(events
        .iter()
        .any(|e| matches!(e, ConvertEvent::Progress { .. })));
    assert!(matches!(
        events.last(),
        Some(ConvertEvent::Converted { .. })
    ));

    // The track now points at the AIFF; everything attached is unchanged.
    let (path, kind, size, rate, depth, rating, plays, last, rb): (
        String,
        String,
        i64,
        i64,
        i64,
        i64,
        i64,
        String,
        String,
    ) = env
        .db()
        .query_row(
            "SELECT path, kind, file_size, sample_rate, bit_depth, rating, play_count,
                    last_played, rb_location FROM track WHERE id = ?1",
            [id],
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
                ))
            },
        )
        .unwrap();
    assert_eq!(path, done.dest);
    assert_eq!(kind, "aiff");
    assert_eq!(size as u64, done.bytes_after);
    assert_eq!((rate, depth), (44_100, 16));
    assert_eq!((rating, plays, last.as_str()), (4, 7, "2026-10-01"));
    assert_eq!(rb, "file://localhost/old/sine.wav");
    assert_eq!(env.attached(id), before);
    assert_eq!(
        env.count(&format!(
            "SELECT count(*) FROM playlist_track WHERE playlist_id = 1 AND track_id = {id}"
        )),
        1
    );
    assert_eq!(env.count("SELECT count(*) FROM superseded_file"), 1);
    assert_eq!(std::fs::read_dir(&overview_dir).unwrap().count(), 0);

    // Original untouched, no temp file, same samples.
    assert_eq!(
        std::fs::read(env.root().join("sine.wav")).unwrap(),
        original
    );
    assert_eq!(env.files(), ["sine.aiff", "sine.wav"]);
    assert_eq!(
        reks_audio::pcm_hash(&env.root().join("sine.aiff")).unwrap(),
        reks_audio::pcm_hash(&env.root().join("sine.wav")).unwrap()
    );

    // The folder holds both files; a re-scan adds nothing back.
    let rescan = env.scan();
    assert_eq!(
        (rescan.found, rescan.added, rescan.missing),
        (2, 0, 0),
        "{rescan:?}"
    );
    assert_eq!(env.count("SELECT count(*) FROM track"), 1);

    // The track plays from the new file.
    env.lib.preview_audio(id, 48_000).unwrap();
}

#[test]
fn add_as_new_copies_metadata_cues_and_grid() {
    let env = Env::with(&["flac"]);
    let id = env.id("sine.flac");
    env.decorate(id);
    let (row_before, attached_before) = (env.row(id), env.attached(id));
    let opts = ConvertOptions {
        library: LibraryMode::AddAsNew,
        ..ConvertOptions::default()
    };
    let plan = convert::plan(&env.lib, &[id], &spec(Target::Wav), &opts).unwrap();
    assert!(plan.warnings.is_empty(), "{:?}", plan.warnings);
    let report = run(&env, &plan);
    assert_eq!(report.converted.len(), 1, "{report:?}");
    let new_id = report.converted[0].new_track_id;
    assert_ne!(new_id, id);

    // The original row and everything attached to it is unchanged.
    assert_eq!(env.row(id), row_before);
    assert_eq!(env.attached(id), attached_before);
    assert_eq!(env.attached(new_id), attached_before);
    let (title, rating, plays, kind, rb): (String, i64, i64, String, Option<String>) = env
        .db()
        .query_row(
            "SELECT title, rating, play_count, kind, rb_location FROM track WHERE id = ?1",
            [new_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .unwrap();
    assert_eq!(title, "Sine Click 128");
    assert_eq!((rating, plays, kind.as_str(), rb), (4, 0, "wav", None));
    assert_eq!(env.count("SELECT count(*) FROM superseded_file"), 0);
    assert_eq!(env.count("SELECT count(*) FROM playlist_track"), 1);

    let rescan = env.scan();
    assert_eq!((rescan.added, rescan.unchanged), (0, 2), "{rescan:?}");
}

#[test]
fn taken_names_get_a_number_and_are_never_overwritten() {
    let env = Env::with(&["flac"]);
    let id = env.id("sine.flac");
    // Not in the library, but on disk: the natural name and its first alternative.
    std::fs::write(env.root().join("sine.aiff"), b"someone else's file").unwrap();
    std::fs::write(
        env.root().join(format!("sine (2).aiff{TEMP_SUFFIX}")),
        b"left over",
    )
    .unwrap();

    let plan = convert::plan(&env.lib, &[id], &spec(Target::Aiff), &Default::default()).unwrap();
    let dest = &plan.items[0].dest;
    assert!(dest.ends_with("sine (3).aiff"), "{dest}");
    assert!(
        matches!(&plan.warnings[..], [Warning::Renamed { wanted, .. }] if wanted.ends_with("sine.aiff"))
    );

    // Taken between plan and apply: apply moves on to the next free name.
    std::fs::write(env.root().join("sine (3).aiff"), b"raced").unwrap();
    let report = run(&env, &plan);
    assert!(
        report.converted[0].dest.ends_with("sine (4).aiff"),
        "{report:?}"
    );
    assert_eq!(
        std::fs::read(env.root().join("sine.aiff")).unwrap(),
        b"someone else's file"
    );
    assert_eq!(
        std::fs::read(env.root().join("sine (3).aiff")).unwrap(),
        b"raced"
    );
}

#[test]
fn chosen_folder_is_created() {
    let env = Env::with(&["aiff"]);
    let id = env.id("sine.aiff");
    let out = env.root().join("converted/flac");
    let opts = ConvertOptions {
        destination: Destination::Folder(out.clone()),
        ..ConvertOptions::default()
    };
    let plan = convert::plan(&env.lib, &[id], &Preset::SaveSpace.spec(), &opts).unwrap();
    let report = run(&env, &plan);
    assert_eq!(report.converted.len(), 1, "{report:?}");
    assert!(out.join("sine.flac").is_file());
    assert!(report.bytes_after < report.bytes_before);
}

#[test]
fn cancel_leaves_no_temp_file_and_no_db_change() {
    let env = Env::with(&["wav"]);
    let id = env.id("sine.wav");
    env.decorate(id);
    let before = env.snapshot();
    let files = env.files();

    let plan = convert::plan(&env.lib, &[id], &spec(Target::Flac), &Default::default()).unwrap();
    let cancel = CancelToken::new();
    let report = convert::apply(
        &env.lib,
        &plan,
        &|e| {
            if matches!(e, ConvertEvent::Progress { .. }) {
                cancel.cancel();
            }
        },
        &cancel,
    )
    .unwrap();
    assert!(report.cancelled, "{report:?}");
    assert!(report.converted.is_empty() && report.failed.is_empty());
    assert_eq!(env.files(), files);
    assert_eq!(env.snapshot(), before);

    // Cancelled before the start: nothing at all.
    let report = convert::apply(&env.lib, &plan, &|_| {}, &cancel).unwrap();
    assert!(report.cancelled && report.converted.is_empty());
    assert_eq!(env.files(), files);
}

#[test]
fn a_file_changed_since_the_plan_fails_alone() {
    let env = Env::with(&["wav", "aiff"]);
    let (wav, aiff) = (env.id("sine.wav"), env.id("sine.aiff"));
    let plan = convert::plan(
        &env.lib,
        &[wav, aiff],
        &spec(Target::Flac),
        &Default::default(),
    )
    .unwrap();
    assert_eq!(plan.items.len(), 2);
    std::fs::OpenOptions::new()
        .append(true)
        .open(env.root().join("sine.wav"))
        .unwrap()
        .set_len(1000)
        .unwrap();
    let before = env
        .db()
        .query_row("SELECT path FROM track WHERE id = ?1", [wav], |r| {
            r.get::<_, String>(0)
        });
    let report = run(&env, &plan);
    assert_eq!(report.failed.len(), 1, "{report:?}");
    assert_eq!(report.failed[0].track_id, wav);
    assert_eq!(report.converted.len(), 1);
    assert_eq!(report.converted[0].track_id, aiff);
    let after = env
        .db()
        .query_row("SELECT path FROM track WHERE id = ?1", [wav], |r| {
            r.get::<_, String>(0)
        });
    assert_eq!(before.unwrap(), after.unwrap());
}

#[test]
fn plan_skips_what_cannot_or_need_not_convert() {
    let env = Env::with(&["mp3", "aiff", "flac"]);
    let (mp3, aiff, flac) = (env.id("sine.mp3"), env.id("sine.aiff"), env.id("sine.flac"));
    let plan = convert::plan(
        &env.lib,
        &[mp3, aiff, flac, flac, 999],
        &Preset::OlderPlayers.spec(),
        &Preset::OlderPlayers.options(),
    )
    .unwrap();
    assert_eq!(
        plan.skipped,
        vec![
            (mp3, SkipReason::Lossy),
            (aiff, SkipReason::AlreadyTarget),
            (flac, SkipReason::Duplicate),
            (999, SkipReason::NotInLibrary),
        ]
    );
    assert_eq!(plan.items.len(), 1);
    assert_eq!(plan.items[0].track_id, flac);

    std::fs::remove_file(env.root().join("sine.flac")).unwrap();
    let plan = convert::plan(&env.lib, &[flac], &spec(Target::Wav), &Default::default()).unwrap();
    assert_eq!(plan.skipped, vec![(flac, SkipReason::FileMissing)]);
}

#[test]
fn trash_needs_replace() {
    let env = Env::with(&["wav"]);
    let opts = ConvertOptions {
        library: LibraryMode::AddAsNew,
        originals: Originals::Trash,
        ..ConvertOptions::default()
    };
    let err = convert::plan(&env.lib, &[env.id("sine.wav")], &spec(Target::Flac), &opts);
    assert!(matches!(
        err,
        Err(reks_core::Error::InvalidField {
            field: "originals",
            ..
        })
    ));
}

/// Writes a generated hi-res fixture to `music/<name>` and scans it in.
fn add_generated(
    env: &Env,
    name: &str,
    write: fn(BufWriter<File>) -> Result<(), hound::Error>,
) -> TrackId {
    let path = env.root().join(name);
    write(BufWriter::new(File::create(&path).unwrap())).unwrap();
    env.scan();
    env.id(name)
}

#[test]
fn older_players_caps_the_rate_and_fixes_wav_variants() {
    let env = Env::with(&["wav"]);
    let hires = add_generated(&env, "hires.wav", reks_fixtures_gen::write_wav_hires);
    let float = add_generated(&env, "float.wav", reks_fixtures_gen::write_wav_float);
    let plain = env.id("sine.wav");

    let plan = convert::plan(
        &env.lib,
        &[hires],
        &Preset::OlderPlayers.spec(),
        &Preset::OlderPlayers.options(),
    )
    .unwrap();
    let item = &plan.items[0];
    assert_eq!((item.from.sample_rate, item.from.bit_depth), (96_000, 24));
    assert_eq!(
        (item.to.kind, item.to.sample_rate, item.to.bit_depth),
        ("aiff", 48_000, 24)
    );
    assert_eq!(item.spec.rate, Rate::Hz48000);

    // WAV → WAV at the same rate and depth: only the plain one is skipped
    // (the hi-res one is WAVE_FORMAT_EXTENSIBLE, the other is float).
    let plan = convert::plan(
        &env.lib,
        &[hires, float, plain],
        &spec(Target::Wav),
        &Default::default(),
    )
    .unwrap();
    assert_eq!(plan.skipped, vec![(plain, SkipReason::AlreadyTarget)]);
    let ids: Vec<TrackId> = plan.items.iter().map(|i| i.track_id).collect();
    assert_eq!(ids, [hires, float]);

    // 16 bits warns about the lost precision.
    let plan = convert::plan(
        &env.lib,
        &[hires],
        &ConvertSpec {
            target: Target::Aiff,
            depth: Depth::Bits16,
            rate: Rate::Hz44100,
        },
        &Default::default(),
    )
    .unwrap();
    assert!(plan.warnings.contains(&Warning::ReducedDepth {
        track_id: hires,
        from: 24
    }));
}
