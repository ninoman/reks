//! `reks parity keyfinder`: our key port against the C++ libKeyFinder on the
//! parity set (phase 3 task 12a, D12).
//!
//! ```text
//! reks parity keyfinder --driver <keyfinder-driver> [--xml ..] [--out ..] [--jobs N]
//!                       [--every N] [--limit N] [--only flac,..] [--remap FROM=TO]...
//! ```
//!
//! Each track is decoded as `run` does ([`AnalysisInput::from_file`]); the
//! same mono signal goes to [`detect_key`] and, on stdin, to the driver built
//! from `fixtures/keyfinder/driver.cpp`. libKeyFinder runs as a separate
//! program only: nothing of it is linked into reks. Results go to
//! `<out>/keyfinder.jsonl` (resumable, keyed like `results.jsonl`); the
//! report printed at the end compares both with rekordbox's `Tonality` on the
//! scored tracks (one `TEMPO` anchor).

use std::collections::HashMap;
use std::fmt::Write as _;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use anyhow::{bail, Context};
use reks_analysis::key::{detect_key, from_keyfinder_index};
use reks_analysis::{queue::default_workers, AnalysisInput, ANALYSER_VERSION, ANALYSIS_RATE};
use reks_core::rbxml;
use reks_core::Key;
use serde::{Deserialize, Serialize};

use super::{class_table, file_stamp, pct, Flags, KeyClass, RbGrid};

const KEYFINDER: &str = "keyfinder.jsonl";

/// One track, one line of `keyfinder.jsonl`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct KeyRecord {
    location: String,
    size: u64,
    mtime: i64,
    analyser: u32,
    /// Canonical key names; `None` for silence.
    reks: Option<String>,
    cpp: Option<String>,
    error: Option<String>,
}

pub(super) fn run(flags: &Flags) -> anyhow::Result<()> {
    let driver = flags
        .driver
        .clone()
        .context("give --driver <path to the built fixtures/keyfinder driver>")?;
    let xml = flags.xml()?;
    let out = flags.out()?;
    std::fs::create_dir_all(&out).with_context(|| format!("creating {}", out.display()))?;
    let collection = rbxml::parse_file(&xml)?;
    let done = load(&out)?;

    let mut todo: Vec<(String, PathBuf, u64, i64)> = Vec::new();
    for (i, t) in collection.tracks.iter().enumerate() {
        if i % flags.every != 0 {
            continue;
        }
        let (Some(loc), Some(path)) = (t.location.clone(), flags.path_of(t)) else {
            continue;
        };
        if let Some(only) = &flags.only {
            if !only.contains(&super::extension(&path)) {
                continue;
            }
        }
        let Some((size, mtime)) = file_stamp(&path) else {
            continue;
        };
        let fresh = done.get(&loc).is_some_and(|r| {
            r.size == size && r.mtime == mtime && r.analyser == ANALYSER_VERSION
        });
        if !fresh {
            todo.push((loc, path, size, mtime));
        }
    }
    if let Some(limit) = flags.limit {
        todo.truncate(limit);
    }
    let jobs = flags.jobs.unwrap_or_else(default_workers).max(1);
    eprintln!("{} to compare on {jobs} workers", todo.len());

    let writer = Mutex::new(
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(out.join(KEYFINDER))
            .context("opening keyfinder.jsonl")?,
    );
    let next = AtomicUsize::new(0);
    let finished = AtomicUsize::new(0);
    std::thread::scope(|s| -> anyhow::Result<()> {
        let handles: Vec<_> = (0..jobs)
            .map(|_| {
                s.spawn(|| -> anyhow::Result<()> {
                    while let Some((loc, path, size, mtime)) =
                        todo.get(next.fetch_add(1, Ordering::Relaxed))
                    {
                        let rec = compare_one(&driver, loc, path, *size, *mtime);
                        let line = serde_json::to_string(&rec)?;
                        writeln!(
                            writer
                                .lock()
                                .map_err(|_| anyhow::anyhow!("writer poisoned"))?,
                            "{line}"
                        )?;
                        let n = finished.fetch_add(1, Ordering::Relaxed) + 1;
                        if n.is_multiple_of(100) || n == todo.len() {
                            eprintln!("{n}/{} done", todo.len());
                        }
                    }
                    Ok(())
                })
            })
            .collect();
        for h in handles {
            h.join().map_err(|_| anyhow::anyhow!("worker panicked"))??;
        }
        Ok(())
    })?;

    let md = report(&collection.tracks, &load(&out)?);
    match &flags.md {
        Some(path) => {
            std::fs::write(path, &md).with_context(|| format!("writing {}", path.display()))?
        }
        None => print!("{md}"),
    }
    Ok(())
}

fn load(out: &Path) -> anyhow::Result<HashMap<String, KeyRecord>> {
    let mut map = HashMap::new();
    let Ok(f) = File::open(out.join(KEYFINDER)) else {
        return Ok(map);
    };
    for line in BufReader::new(f).lines() {
        // A line cut short by an interrupted run is redone.
        if let Ok(r) = serde_json::from_str::<KeyRecord>(&line?) {
            map.insert(r.location.clone(), r);
        }
    }
    Ok(map)
}

fn compare_one(driver: &Path, loc: &str, path: &Path, size: u64, mtime: i64) -> KeyRecord {
    let mut rec = KeyRecord {
        location: loc.to_owned(),
        size,
        mtime,
        analyser: ANALYSER_VERSION,
        reks: None,
        cpp: None,
        error: None,
    };
    let keys = AnalysisInput::from_file(path)
        .map_err(anyhow::Error::from)
        .and_then(|input| {
            let ours = detect_key(&input.mono, ANALYSIS_RATE)?.key;
            Ok((ours, cpp_key(driver, &input.mono)?))
        });
    match keys {
        Ok((ours, theirs)) => {
            rec.reks = ours.map(|k| k.canonical().to_owned());
            rec.cpp = theirs.map(|k| k.canonical().to_owned());
        }
        Err(e) => rec.error = Some(format!("{e:#}")),
    }
    rec
}

/// Runs the driver on mono PCM at [`ANALYSIS_RATE`]; `None` is silence.
fn cpp_key(driver: &Path, mono: &[f32]) -> anyhow::Result<Option<Key>> {
    let mut child = Command::new(driver)
        .arg(ANALYSIS_RATE.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("starting {}", driver.display()))?;
    let bytes: Vec<u8> = mono.iter().flat_map(|s| s.to_le_bytes()).collect();
    child
        .stdin
        .take()
        .context("driver stdin")?
        .write_all(&bytes)?;
    let out = child.wait_with_output()?;
    if !out.status.success() {
        bail!("driver: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    let index: u8 = String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse()
        .context("driver output")?;
    Ok(from_keyfinder_index(index))
}

/// Aggregates over the scored tracks: reks against C++, and both against
/// rekordbox.
fn report(tracks: &[rbxml::XmlTrack], results: &HashMap<String, KeyRecord>) -> String {
    let parse = |k: &Option<String>| k.as_deref().and_then(|k| Key::parse(k).ok());
    let mut rows = Vec::new();
    let mut failed = 0;
    for t in tracks {
        let Some(rec) = t.location.as_deref().and_then(|l| results.get(l)) else {
            continue;
        };
        if rec.analyser != ANALYSER_VERSION {
            continue;
        }
        if rec.error.is_some() {
            failed += 1;
            continue;
        }
        let scored = RbGrid::from_track(t).is_some_and(|g| g.anchors.len() == 1);
        let rb = t.tonality.as_deref().and_then(|k| Key::parse(k).ok());
        if let (true, Some(rb)) = (scored, rb) {
            rows.push((parse(&rec.reks), parse(&rec.cpp), rb));
        }
    }
    let n = rows.len();
    let same = rows.iter().filter(|(ours, cpp, _)| ours == cpp).count();
    let mut md = String::new();
    let _ = writeln!(
        md,
        "Analyser version {ANALYSER_VERSION}; {n} scored tracks with a rekordbox key \
         ({failed} failed to decode).\n\nreks = C++ libKeyFinder: {same} of {n} ({}).\n",
        pct(same, n)
    );
    type Row = (Option<Key>, Option<Key>, Key);
    let sides: [(&str, fn(&Row) -> Option<Key>); 2] =
        [("reks", |r| r.0), ("C++ libKeyFinder", |r| r.1)];
    for (who, key) in sides {
        class_table(
            &mut md,
            &format!("{who} vs rekordbox"),
            rows.iter().map(|r| KeyClass::of(key(r), r.2)),
            KeyClass::label,
        );
    }
    md
}
