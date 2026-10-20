//! `reks` command-line interface: thin wrappers over the reks crates for
//! headless testing and fixtures.
//!
//! ```text
//! reks                          print versions
//! reks synth-xml <N> <out.xml>  write a synthetic rekordbox XML with N tracks
//! reks devices                  list audio output devices
//! reks play <file> [seconds]    play a file on the default output (engine smoke test)
//! reks import-xml <library.sqlite> <collection.xml> [--remap FROM=TO]... [--dry-run]
//!                               import a rekordbox XML (or, with --dry-run, preview it)
//! reks rb plan <stick> --library <library.sqlite> --playlist <id|Folder/Name>... [...]
//!                               dry run of a rekordbox USB export (flags: see `rb.rs`)
//! reks rb export <stick> <plan flags> [--yes] [--data-dir <dir>]
//!                               dry run, then with --yes write and validate the stick
//! reks rb validate <stick> <plan flags>
//!                               read the stick back against a fresh plan
//! reks rb history <stick> [--json]
//!                               list the play history players left on the stick
//! reks history scan <stick|backup> --library <library.sqlite> [--hist1-dates] [--json]
//!                               dry run of importing a stick's play history
//! reks history import <stick|backup> <scan flags> [--practice|--include <session>]... [--yes]
//!                               dry run, then with --yes import the sessions
//! reks adopt scan <stick> --library <library.sqlite> [--json]
//!                               dry run of adopting a stick reks did not write
//! reks convert plan --library <library.sqlite> (--track <id> | --playlist <id|path>)... [...]
//!                               dry run of converting tracks (flags: see `convert.rs`)
//! reks convert apply <plan flags> [--yes]
//!                               dry run, then with --yes convert and update the library
//! ```

mod adopt;
mod convert;
mod history;
mod rb;

use std::io::BufWriter;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context};
use reks_core::rbxml::{
    self, Collection, Extra, ImportOptions, PrefixRemap, Tempo, XmlNode, XmlTrack, TRACK_COLOURS,
};
use reks_core::{Key, Library};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [] => {
            println!(
                "reks-cli {} (reks-core {})",
                env!("CARGO_PKG_VERSION"),
                reks_core::VERSION
            );
            Ok(())
        }
        [cmd, n, out] if cmd == "synth-xml" => {
            let n: u32 = n.parse().context("track count")?;
            synth_xml(n, Path::new(out))
        }
        [cmd, file, out] if cmd == "columns" => columns(Path::new(file), Path::new(out)),
        [cmd] if cmd == "devices" => {
            for name in reks_mix::output_devices()? {
                println!("{name}");
            }
            Ok(())
        }
        [cmd, file] if cmd == "play" => play(Path::new(file), 10),
        [cmd, file, secs] if cmd == "play" => {
            play(Path::new(file), secs.parse().context("seconds")?)
        }
        [cmd, db, xml, rest @ ..] if cmd == "import-xml" => {
            import_xml(Path::new(db), Path::new(xml), rest)
        }
        [cmd, rest @ ..] if cmd == "rb" => rb::run(rest),
        [cmd, rest @ ..] if cmd == "convert" => convert::run(rest),
        [cmd, rest @ ..] if cmd == "history" => history::run(rest),
        [cmd, rest @ ..] if cmd == "adopt" => adopt::run(rest),
        _ => bail!(
            "usage: reks [synth-xml <tracks> <out.xml> | devices | play <file> [seconds] \
             | columns <audio> <out.bin> \
             | import-xml <library.sqlite> <collection.xml> [--remap FROM=TO]... [--dry-run] \
             | rb plan|export|validate <stick> ... | rb history <stick> [--json] \
             | history scan|import <stick|backup> --library <library.sqlite> ... \
             | adopt scan <stick> --library <library.sqlite> [--json] \
             | convert plan|apply --library <library.sqlite> ...]"
        ),
    }
}

/// Writes the waveform columns of `file` (4 bytes per 1/150 s: low, mid,
/// high, mono), the payload the grid editor draws.
fn columns(file: &Path, out: &Path) -> anyhow::Result<()> {
    let cols = reks_analysis::columns::file_columns(file)?;
    let bytes = cols.to_interleaved().context("inconsistent columns")?;
    std::fs::write(out, &bytes).with_context(|| format!("writing {}", out.display()))?;
    println!("{} columns -> {}", cols.len(), out.display());
    Ok(())
}

/// Imports (or previews, with `--dry-run`) `xml` into the library at `db`,
/// creating it if needed; derived files go to `cache/` next to it.
fn import_xml(db: &Path, xml: &Path, flags: &[String]) -> anyhow::Result<()> {
    let mut opts = ImportOptions::default();
    let mut dry_run = false;
    let mut flags = flags.iter();
    while let Some(flag) = flags.next() {
        match flag.as_str() {
            "--dry-run" => dry_run = true,
            "--remap" => {
                let arg = flags.next().context("--remap needs FROM=TO")?;
                let (from, to) = arg.split_once('=').context("--remap needs FROM=TO")?;
                opts.remap.push(PrefixRemap {
                    from: PathBuf::from(from),
                    to: PathBuf::from(to),
                });
            }
            other => bail!("unknown flag {other}"),
        }
    }
    let cache = db.parent().unwrap_or(Path::new(".")).join("cache");
    let lib = Library::open(db, &cache)?;
    let started = std::time::Instant::now();
    let collection = rbxml::parse_file(xml)?;
    println!("parsed in {} ms", started.elapsed().as_millis());
    if dry_run {
        let started = std::time::Instant::now();
        let p = rbxml::preview(&collection, &lib, &opts)?;
        println!("{p:#?}");
        println!("previewed in {} ms", started.elapsed().as_millis());
        return Ok(());
    }
    let report = rbxml::import(&collection, &lib, opts, |_| {})?;
    for u in &report.unmatched {
        println!("unmatched {}: {:?}", u.track_id, u.reason);
    }
    println!(
        "{} tracks: {} added, {} matched by path, {} by location, {} by fallback, {} missing, \
         {} unmatched, {} warnings, {} ms",
        report.total,
        report.added,
        report.matched_by_path,
        report.matched_by_location,
        report.matched_by_fallback,
        report.missing,
        report.unmatched.len(),
        report.warnings.len(),
        report.elapsed_ms
    );
    if let Some(root) = &report.missing_root {
        println!("missing files are under {root}");
    }
    Ok(())
}

/// Plays `file` for `secs` seconds through the preview player, printing the
/// position and the device's xrun count.
fn play(file: &Path, secs: u64) -> anyhow::Result<()> {
    let mut player = reks_mix::Player::new(None);
    let rate = player.sample_rate()?;
    let decoded = reks_audio::decode_stereo(file, Some(rate))?;
    println!(
        "{} Hz -> {rate} Hz, {} ms",
        decoded.source_rate,
        decoded.pcm.duration_ms()
    );
    player.load(decoded.pcm.samples, rate)?;
    player.play()?;
    for _ in 0..secs * 4 {
        std::thread::sleep(std::time::Duration::from_millis(250));
        let st = player.state();
        println!(
            "{:>7} ms playing={} device={:?}",
            st.position_ms, st.playing, st.device
        );
        if !st.playing && st.position_ms > 0 {
            break;
        }
    }
    player.pause()?;
    std::thread::sleep(std::time::Duration::from_millis(50));
    Ok(())
}

/// Writes a rekordbox XML with `n` varied synthetic tracks (genres, keys,
/// BPM, colours, ratings; two in three with a beatgrid) and a few
/// playlists. The `Location`s point at files that don't exist, so an import
/// stores them as missing: enough to exercise the track table at scale.
fn synth_xml(n: u32, out: &Path) -> anyhow::Result<()> {
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
    let keys: Vec<Key> = Key::all().collect();
    let pick = |list: &[&'static str], i: u32| list[i as usize % list.len()].to_owned();

    let tracks: Vec<XmlTrack> = (1..=n)
        .map(|id| {
            let bpm = 70.0 + f64::from((id * 7919) % 11_000) / 100.0;
            XmlTrack {
                track_id: id,
                name: Some(format!(
                    "{} {} {id}",
                    pick(&WORDS, id),
                    pick(&WORDS, id / 12 + 5)
                )),
                artist: Some(format!("Artist {}", (id * 31) % 700)),
                album: Some(format!("Album {}", id % 900)),
                genre: Some(pick(&GENRES, id / 3)),
                kind: Some("MP3 File".into()),
                size: Some(8_000_000),
                total_time: Some(180 + (id * 13) % 300),
                year: Some(1990 + id % 36),
                average_bpm: Some(bpm),
                date_added: Some(format!("2025-{:02}-{:02}", 1 + id % 12, 1 + id % 28)),
                bit_rate: Some(320),
                sample_rate: Some(44_100),
                play_count: Some(id % 17),
                rating: Some(((id % 6) * 51) as u8),
                location: Some(format!(
                    "file://localhost/reks-synthetic/{:03}/track-{id}.mp3",
                    id % 100
                )),
                tonality: Some(keys[id as usize % keys.len()].canonical().into()),
                colour: (id % 3 == 0).then(|| TRACK_COLOURS[id as usize % 8].1),
                tempos: if id % 3 == 0 {
                    Vec::new()
                } else {
                    vec![Tempo {
                        inizio: 0.05,
                        bpm,
                        metro: Some("4/4".into()),
                        battito: Some(1),
                        extra: Extra::new(),
                    }]
                },
                ..XmlTrack::default()
            }
        })
        .collect();

    let ids = |step: u32| {
        (1..=n)
            .step_by(step as usize)
            .map(|i| i.to_string())
            .collect()
    };
    let collection = Collection {
        tracks,
        playlists: XmlNode::folder(
            "ROOT",
            vec![
                XmlNode::folder(
                    "Synthetic",
                    vec![
                        XmlNode::playlist("Every 10th", 0, ids(10)),
                        XmlNode::playlist("Every 100th", 0, ids(100)),
                    ],
                ),
                XmlNode::playlist("Every 7th", 0, ids(7)),
            ],
        ),
        ..Collection::default()
    };
    let f = std::fs::File::create(out).with_context(|| format!("creating {}", out.display()))?;
    rbxml::write(&collection, BufWriter::new(f))?;
    println!("wrote {n} tracks to {}", out.display());
    Ok(())
}
