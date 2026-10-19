//! `reks` command-line interface: thin wrappers over the reks crates for
//! headless testing and fixtures.
//!
//! ```text
//! reks                          print versions
//! reks synth-xml <N> <out.xml>  write a synthetic rekordbox XML with N tracks
//! reks devices                  list audio output devices
//! reks play <file> [seconds]    play a file on the default output (engine smoke test)
//! ```

use std::io::BufWriter;
use std::path::Path;

use anyhow::{bail, Context};
use reks_core::rbxml::{self, Collection, Extra, Tempo, XmlNode, XmlTrack, TRACK_COLOURS};
use reks_core::Key;

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
        _ => bail!("usage: reks [synth-xml <tracks> <out.xml> | devices | play <file> [seconds]]"),
    }
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
