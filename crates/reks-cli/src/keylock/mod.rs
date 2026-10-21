//! `reks keylock-render`: renders an excerpt of one track through every key
//! lock engine this build has, at several tempo ratios, for a listening
//! test (phase 5 task 10). Also measures each engine's cost and latency.
//!
//! ```text
//! reks keylock-render <audio> --out-dir <dir> [--ratios 0.94,1.0,1.06]
//!     [--from <s>] [--seconds <s>] [--engines a,b] [--block <frames>] [--blind]
//! ```
//!
//! Writes `<stem>.<engine>.<ratio>.wav` (24-bit, the file's own rate), or
//! with `--blind` `<stem>.<ratio>.<letter>.wav` with the key only in
//! `keylock-report.json`. Every render is aligned to the input by the
//! engine's reported latency, so the files can be switched between in sync.

mod alloc;
mod engines;
mod probe;
#[cfg(feature = "keylock-rubberband")]
mod rubberband;
#[cfg(feature = "keylock-signalsmith")]
mod signalsmith;
mod stretcher;
#[cfg(feature = "keylock-timestretch")]
mod timestretch;
mod varispeed;
mod wav;

use std::path::PathBuf;

use anyhow::{bail, Context};
use serde::Serialize;

use engines::{Engine, MISSING};
use stretcher::{render, Feed};

const USAGE: &str = "usage: reks keylock-render <audio> --out-dir <dir> [--ratios 0.94,1.0,1.06] \
     [--from <s>] [--seconds <s>] [--engines a,b] [--block <frames>] [--blind]";

#[derive(Debug)]
struct Args {
    input: PathBuf,
    out_dir: PathBuf,
    ratios: Vec<f64>,
    from_s: f64,
    seconds: f64,
    engines: Vec<Engine>,
    block: usize,
    blind: bool,
}

fn parse(args: &[String]) -> anyhow::Result<Args> {
    let mut input = None;
    let mut out_dir = None;
    let mut ratios = vec![0.94, 1.0, 1.06];
    let mut from_s = 60.0;
    let mut seconds = 90.0;
    let mut engines = Engine::all();
    let mut block = 256;
    let mut blind = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || it.next().context(USAGE);
        match a.as_str() {
            "--out-dir" => out_dir = Some(PathBuf::from(value()?)),
            "--ratios" => {
                ratios = value()?
                    .split(',')
                    .map(|r| r.trim().parse::<f64>().context("--ratios"))
                    .collect::<anyhow::Result<_>>()?;
            }
            "--from" => from_s = value()?.parse().context("--from")?,
            "--seconds" => seconds = value()?.parse().context("--seconds")?,
            "--block" => block = value()?.parse().context("--block")?,
            "--engines" => {
                engines = value()?
                    .split(',')
                    .map(|n| {
                        Engine::parse(n.trim()).with_context(|| {
                            let have: Vec<_> = Engine::all().iter().map(|e| e.name()).collect();
                            format!("unknown or not built engine {n}; this build has {have:?}")
                        })
                    })
                    .collect::<anyhow::Result<_>>()?;
            }
            "--blind" => blind = true,
            flag if flag.starts_with("--") => bail!("unknown flag {flag}\n{USAGE}"),
            path if input.is_none() => input = Some(PathBuf::from(path)),
            _ => bail!("{USAGE}"),
        }
    }
    if ratios.iter().any(|r| !(0.5..=2.0).contains(r)) {
        bail!("ratios must be within 0.5..=2.0");
    }
    if !(64..=8192).contains(&block) {
        bail!("--block must be 64..=8192 frames");
    }
    let (Some(input), Some(out_dir)) = (input, out_dir) else {
        bail!("{USAGE}")
    };
    Ok(Args {
        input,
        out_dir,
        ratios,
        from_s,
        seconds,
        engines,
        block,
        blind,
    })
}

/// One engine at one ratio.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Row {
    engine: &'static str,
    ratio: f64,
    file: String,
    /// CPU time per second of output audio, one deck.
    cpu_ms_per_s: f64,
    /// Slowest block against the block's playback time (1.0 = a dropout).
    worst_block_load: f64,
    worst_block_ms: f64,
    /// Rust heap allocation/free events inside process calls (C++ heaps
    /// are not visible).
    alloc_events: u64,
    reported_latency_ms: f64,
    measured_latency_ms: f64,
    underrun_frames: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    input: String,
    sample_rate: u32,
    from_s: f64,
    seconds: f64,
    block_frames: usize,
    rows: Vec<Row>,
}

pub fn run(args: &[String]) -> anyhow::Result<()> {
    let args = parse(args)?;
    for (engines, feature) in MISSING {
        println!("not built: {engines} (cargo feature {feature})");
    }
    let decoded = reks_audio::decode_stereo(&args.input, None)
        .with_context(|| format!("decoding {}", args.input.display()))?;
    let rate = decoded.pcm.sample_rate;
    let total = decoded.pcm.frames();
    let start = ((args.from_s * f64::from(rate)) as usize).min(total);
    let end = (start + (args.seconds * f64::from(rate)) as usize).min(total);
    if end <= start {
        bail!(
            "the excerpt is empty: the track is {} s long",
            total / rate as usize
        );
    }
    let input: Vec<f32> = decoded.pcm.samples[start * 2..end * 2]
        .iter()
        .map(|&s| f32::from(s) / 32_768.0)
        .collect();
    let in_frames = end - start;
    std::fs::create_dir_all(&args.out_dir)
        .with_context(|| format!("creating {}", args.out_dir.display()))?;
    let stem = args
        .input
        .file_stem()
        .map_or_else(|| "track".to_owned(), |s| s.to_string_lossy().into_owned());
    let letters = blind_letters(args.engines.len());
    let ms = |frames: f64| frames * 1000.0 / f64::from(rate);
    let block_ms = ms(args.block as f64);

    let mut rows = Vec::new();
    for &ratio in &args.ratios {
        for (i, &engine) in args.engines.iter().enumerate() {
            let frames = (in_frames as f64 / ratio).floor() as usize;
            let mut feed = Feed::new(&input);
            let mut st = engine.build(rate, ratio, args.block, &mut feed)?;
            let r = render(st.as_mut(), &mut feed, frames, args.block);
            let underrun_frames = st.underruns();
            let name = if args.blind {
                format!("{stem}.{ratio:.2}.{}.wav", letters[i])
            } else {
                format!("{stem}.{}.{ratio:.2}.wav", engine.name())
            };
            let path = args.out_dir.join(&name);
            wav::write_24(&path, rate, r.aligned(frames))?;

            let burst = probe::burst(rate);
            let mut feed = Feed::new(&burst);
            let mut st = engine.build(rate, ratio, args.block, &mut feed)?;
            let burst_frames = (burst.len() / 2) as f64 / ratio;
            let b = render(st.as_mut(), &mut feed, burst_frames as usize, args.block);
            let measured = probe::measured_latency(&b.samples, rate, ratio);

            let out_s = frames as f64 / f64::from(rate);
            let row = Row {
                engine: engine.name(),
                ratio,
                file: name,
                cpu_ms_per_s: r.cpu.as_secs_f64() * 1000.0 / out_s,
                worst_block_ms: r.worst_block.as_secs_f64() * 1000.0,
                worst_block_load: r.worst_block.as_secs_f64() * 1000.0 / block_ms,
                alloc_events: r.alloc_events,
                reported_latency_ms: ms(r.reported_latency as f64),
                measured_latency_ms: ms(measured),
                underrun_frames,
            };
            if !args.blind {
                print_row(&row);
            }
            rows.push(row);
        }
    }
    let report = Report {
        input: args.input.display().to_string(),
        sample_rate: rate,
        from_s: start as f64 / f64::from(rate),
        seconds: in_frames as f64 / f64::from(rate),
        block_frames: args.block,
        rows,
    };
    let json = args.out_dir.join("keylock-report.json");
    std::fs::write(&json, serde_json::to_string_pretty(&report)?)
        .with_context(|| format!("writing {}", json.display()))?;
    println!(
        "{} renders of {:.0} s at {rate} Hz in {}; report {}",
        report.rows.len(),
        report.seconds,
        args.out_dir.display(),
        json.display()
    );
    if args.blind {
        println!("blind: the engine behind each letter is in the report; listen first");
    }
    Ok(())
}

fn print_row(r: &Row) {
    println!(
        "{:<20} {:>5.2}  cpu {:>6.2} ms/s  worst block {:>5.2} ms ({:>4.0}% of period)  \
         allocs {:>5}  latency {:>6.1} ms reported, {:>6.1} ms measured{}",
        r.engine,
        r.ratio,
        r.cpu_ms_per_s,
        r.worst_block_ms,
        r.worst_block_load * 100.0,
        r.alloc_events,
        r.reported_latency_ms,
        r.measured_latency_ms,
        if r.underrun_frames > 0 {
            format!("  UNDERRUN {} frames", r.underrun_frames)
        } else {
            String::new()
        }
    );
}

/// Letters A, B, … in an order that changes from run to run, so a blind
/// listener cannot learn which letter is which engine.
fn blind_letters(n: usize) -> Vec<char> {
    let mut letters: Vec<char> = ('A'..='Z').take(n).collect();
    let mut seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |d| d.as_nanos() as u64)
        | 1;
    for i in (1..letters.len()).rev() {
        // xorshift64: plenty for shuffling a handful of letters.
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        letters.swap(i, (seed % (i as u64 + 1)) as usize);
    }
    letters
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn parses_defaults_and_flags() {
        let a = parse(&args("t.flac --out-dir o")).unwrap();
        assert_eq!(a.ratios, [0.94, 1.0, 1.06]);
        assert_eq!(a.engines, Engine::all());
        let a = parse(&args(
            "t.flac --out-dir o --ratios 0.92,1.08 --engines varispeed",
        ))
        .unwrap();
        assert_eq!(a.ratios, [0.92, 1.08]);
        assert_eq!(a.engines, [Engine::Varispeed]);
        assert!(parse(&args("t.flac --out-dir o --engines nope")).is_err());
        assert!(parse(&args("t.flac --out-dir o --ratios 3")).is_err());
        assert!(parse(&args("t.flac")).is_err());
    }

    #[test]
    fn blind_letters_are_a_permutation() {
        let mut l = blind_letters(6);
        l.sort_unstable();
        assert_eq!(l, ['A', 'B', 'C', 'D', 'E', 'F']);
    }

    /// Every engine in this build keeps a tone's pitch (or, for
    /// varispeed, moves it by the ratio), stays aligned and does not
    /// underrun. Checks the plumbing, not the sound.
    #[test]
    fn every_engine_renders_a_tone_in_place() {
        let rate = 44_100;
        let tone: Vec<f32> = (0..rate * 4)
            .flat_map(|i| {
                let x = 0.5 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / rate as f32).sin();
                [x, x]
            })
            .collect();
        for engine in Engine::all() {
            for ratio in [0.94, 1.06] {
                let mut feed = Feed::new(&tone);
                let mut st = engine.build(rate as u32, ratio, 256, &mut feed).unwrap();
                let frames = (rate as f64 * 3.0 / ratio) as usize;
                let r = render(st.as_mut(), &mut feed, frames, 256);
                assert_eq!(st.underruns(), 0, "{}", engine.name());
                let out = r.aligned(frames);
                // Count zero crossings over the middle second of the left channel.
                let mid: Vec<f32> = out
                    .iter()
                    .step_by(2)
                    .skip(rate)
                    .take(rate)
                    .copied()
                    .collect();
                let crossings = mid
                    .windows(2)
                    .filter(|w| (w[0] < 0.0) != (w[1] < 0.0))
                    .count();
                let hz = crossings as f64 / 2.0;
                let want = if engine == Engine::Varispeed {
                    440.0 * ratio
                } else {
                    440.0
                };
                assert!(
                    (hz - want).abs() < 6.0,
                    "{} at {ratio}: {hz} Hz",
                    engine.name()
                );
            }
        }
    }
}
