//! Waveform calibration against rekordbox (phase 3 task 8). Fits
//! `reks_analysis::calib::CALIBRATION` and draws the comparison pictures in
//! `docs/analysis-parity/`; method and results in
//! `docs/analysis-parity/waveforms.md`.
//!
//! Needs rekordbox's local analysis cache and the audio it analysed, so it is
//! ignored by default:
//!
//! ```sh
//! REKS_RB_ANLZ=~/Library/Pioneer/rekordbox/share/PIONEER/USBANLZ \
//! REKS_CALIB_MUSIC=~/Music \
//! cargo test --release -p reks-anlz --test calibrate -- --ignored --nocapture
//! ```
//!
//! `REKS_RB_ANLZ` is only read. `REKS_CALIB_MUSIC` is searched for the files
//! the cache names (rekordbox's cache keeps only `?/<file name>` in PPTH).
//! Optional: `REKS_CALIB_TRACKS` (default 50) and `REKS_CALIB_PNG` (absolute
//! folder for the pictures; none are drawn without it). The test prints the
//! per-channel error before and after the fit and the fitted constants as
//! Rust, to paste into `calib.rs`.

#![allow(
    clippy::print_stderr,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use reks_analysis::calib::{AutoGain, Curve, LevelStat, WaveformCalibration, CALIBRATION};
use reks_analysis::waveform::{columns_from_interleaved, WaveformColumns};
use reks_analysis::{AnalysisInput, ANALYSIS_RATE};
use reks_anlz::compare::waveform_error;
use reks_anlz::read::{self, Section};
use reks_anlz::waveform::unpack_color;
use reks_anlz::write;
use reks_anlz::Waveforms;

// ---------------------------------------------------------------------------
// Track selection

/// One rekordbox-analysed track in the local cache.
#[derive(Debug, Clone)]
struct Candidate {
    anlz_dir: PathBuf,
    audio: PathBuf,
    genre: &'static str,
    /// Mean PWV3 height of rekordbox's sounding columns (loudness proxy).
    loudness: f64,
}

fn genre_bucket(genre: Option<&str>) -> &'static str {
    let g = genre.unwrap_or("").to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| g.contains(w));
    if g.trim().is_empty() {
        "untagged"
    } else if has(&["psy", "goa"]) {
        "psytrance"
    } else if has(&["trance"]) {
        "trance"
    } else if has(&["techno"]) {
        "techno"
    } else if has(&["house"]) {
        "house"
    } else if has(&["ambient", "chill", "downtempo", "lounge"]) {
        "ambient"
    } else if has(&["drum", "dnb", "bass", "dubstep", "breaks"]) {
        "bass"
    } else if has(&["electro", "dance", "edm"]) {
        "electro"
    } else {
        "other"
    }
}

/// NFC file name, the key PPTH paths are matched on.
fn name_key(name: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    name.nfc().collect()
}

/// Audio files under `music` by NFC file name; a name found twice maps to
/// `None` (ambiguous).
fn music_index(music: &Path) -> HashMap<String, Option<PathBuf>> {
    const AUDIO: [&str; 7] = ["mp3", "m4a", "aac", "flac", "wav", "aif", "aiff"];
    let mut index: HashMap<String, Option<PathBuf>> = HashMap::new();
    for e in walkdir::WalkDir::new(music).into_iter().flatten() {
        let p = e.path();
        let is_audio = p
            .extension()
            .is_some_and(|x| AUDIO.contains(&x.to_string_lossy().to_lowercase().as_str()));
        if e.file_type().is_file() && is_audio {
            index
                .entry(name_key(&e.file_name().to_string_lossy()))
                .and_modify(|v| *v = None)
                .or_insert_with(|| Some(p.to_path_buf()));
        }
    }
    index
}

/// Cache entries (`ANLZ0000.DAT/.EXT/.2EX`) whose PPTH names exactly one
/// audio file under `music`. The full path lives only in rekordbox's
/// encrypted `master.db`, which we do not read, so a file name that occurs
/// twice in the cache is skipped.
fn candidates(cache: &Path, music: &Path) -> Vec<Candidate> {
    let index = music_index(music);
    let mut by_name: HashMap<String, Vec<(PathBuf, f64)>> = HashMap::new();
    for e in walkdir::WalkDir::new(cache).into_iter().flatten() {
        let p = e.path();
        if e.file_name() != "ANLZ0000.EXT"
            || !p.with_extension("2EX").is_file()
            || !p.with_extension("DAT").is_file()
        {
            continue;
        }
        let Ok(ext) = std::fs::read(p) else { continue };
        let Ok(sections) = read::sections(&ext) else {
            continue;
        };
        let (Some(ppth), Some(pwv3)) = (
            sections.iter().find(|s| s.tag == write::PPTH),
            sections.iter().find(|s| s.tag == write::PWV3),
        ) else {
            continue;
        };
        let Ok(path) = read::path(ppth) else { continue };
        let heights: Vec<f64> = pwv3
            .payload()
            .iter()
            .map(|b| f64::from(b & 0x1F))
            .filter(|&h| h > 0.0)
            .collect();
        if heights.len() < 150 * 30 {
            continue; // under 30 s of sound
        }
        by_name
            .entry(name_key(path.rsplit('/').next().unwrap_or(&path)))
            .or_default()
            .push((
                p.parent().unwrap().to_path_buf(),
                heights.iter().sum::<f64>() / heights.len() as f64,
            ));
    }
    let mut out = Vec::new();
    for (name, entries) in by_name {
        let ([(anlz_dir, loudness)], Some(Some(audio))) = (entries.as_slice(), index.get(&name))
        else {
            continue;
        };
        let genre = reks_audio::tags::tags(audio).ok().and_then(|t| t.genre);
        out.push(Candidate {
            anlz_dir: anlz_dir.clone(),
            audio: audio.clone(),
            genre: genre_bucket(genre.as_deref()),
            loudness: *loudness,
        });
    }
    out.sort_by(|a, b| a.audio.cmp(&b.audio));
    out
}

/// `n` tracks spread over genre buckets (at least 3 each where possible,
/// the rest by share) and, within each bucket, evenly over loudness.
fn select(all: &[Candidate], n: usize) -> Vec<Candidate> {
    let mut buckets: BTreeMap<&str, Vec<&Candidate>> = BTreeMap::new();
    for c in all {
        buckets.entry(c.genre).or_default().push(c);
    }
    for v in buckets.values_mut() {
        v.sort_by(|a, b| a.loudness.total_cmp(&b.loudness));
    }
    let total = all.len().max(1);
    let mut quota: BTreeMap<&str, usize> = buckets
        .iter()
        .map(|(k, v)| (*k, v.len().min(3.max(v.len() * n / total))))
        .collect();
    while quota.values().sum::<usize>() > n {
        let k = *quota.iter().max_by_key(|(_, q)| **q).unwrap().0;
        *quota.get_mut(k).unwrap() -= 1;
    }
    while quota.values().sum::<usize>() < n {
        let Some(k) = quota
            .iter()
            .filter(|(k, q)| **q < buckets[*k].len())
            .max_by_key(|(k, q)| buckets[*k].len() / (**q + 1))
            .map(|(k, _)| *k)
        else {
            break;
        };
        *quota.get_mut(k).unwrap() += 1;
    }
    let mut out = Vec::new();
    for (k, v) in &buckets {
        let q = quota[k];
        out.extend((0..q).map(|i| v[((2 * i + 1) * v.len()) / (2 * q)].clone()));
    }
    out
}

// ---------------------------------------------------------------------------
// Reference data

/// The rekordbox waveform sections compared.
const TAGS: [[u8; 4]; 7] = [
    write::PWAV,
    write::PWV2,
    write::PWV3,
    write::PWV4,
    write::PWV5,
    write::PWV6,
    write::PWV7,
];

/// Detail sections (150/s), shifted by the track's lag before comparing.
const DETAIL: [[u8; 4]; 3] = [write::PWV3, write::PWV5, write::PWV7];

struct Track {
    candidate: Candidate,
    columns: WaveformColumns,
    /// rekordbox's columns run this many columns behind ours (decoder delay,
    /// phase 3 task 4); its detail sections are compared without them.
    lag: usize,
    /// One single-section ANLZ file per tag in [`TAGS`], aligned.
    theirs: Vec<Vec<u8>>,
    pwvc: [u16; 3],
}

fn section_of<'a>(sections: &'a [Section<'a>], tag: [u8; 4]) -> &'a Section<'a> {
    sections.iter().find(|s| s.tag == tag).unwrap()
}

/// Lag (0..=16 columns) that best lines rekordbox's PWV3 heights up with our
/// mono peaks, by correlation.
fn lag(ours: &[u8], theirs: &[u8]) -> usize {
    let n = ours.len().min(theirs.len()).saturating_sub(16);
    let a: Vec<f64> = ours[..n].iter().map(|&v| f64::from(v)).collect();
    let corr = |l: usize| {
        let b: Vec<f64> = theirs[l..l + n]
            .iter()
            .map(|&v| f64::from(v & 0x1F))
            .collect();
        let (ma, mb) = (mean(&a), mean(&b));
        let (mut sab, mut saa, mut sbb) = (0.0, 0.0, 0.0);
        for (x, y) in a.iter().zip(&b) {
            sab += (x - ma) * (y - mb);
            saa += (x - ma) * (x - ma);
            sbb += (y - mb) * (y - mb);
        }
        sab / (saa * sbb).sqrt().max(1e-12)
    };
    (0..=16)
        .max_by(|&x, &y| corr(x).total_cmp(&corr(y)))
        .unwrap()
}

fn load_track(c: Candidate) -> Track {
    let input = AnalysisInput::from_file(&c.audio).unwrap();
    let columns = columns_from_interleaved(&input.mono, ANALYSIS_RATE, 1).unwrap();
    let files: Vec<Vec<u8>> = ["DAT", "EXT", "2EX"]
        .iter()
        .map(|e| std::fs::read(c.anlz_dir.join(format!("ANLZ0000.{e}"))).unwrap())
        .collect();
    let sections: Vec<Section<'_>> = files
        .iter()
        .flat_map(|f| read::sections(f).unwrap())
        .collect();
    let lag = lag(&columns.mono, section_of(&sections, write::PWV3).payload());
    let theirs = TAGS
        .iter()
        .map(|&tag| {
            let s = section_of(&sections, tag);
            if !DETAIL.contains(&tag) {
                return write::file(&[s.bytes.to_vec()]).unwrap();
            }
            let p = s.payload();
            let section = match tag {
                write::PWV3 => write::pwv3(&p[lag..]),
                write::PWV5 => write::pwv5(
                    &p.as_chunks::<2>().0[lag..]
                        .iter()
                        .map(|&e| u16::from_be_bytes(e))
                        .collect::<Vec<_>>(),
                ),
                _ => write::pwv7(&p.as_chunks::<3>().0[lag..]),
            };
            write::file(&[section.unwrap()]).unwrap()
        })
        .collect();
    let p = section_of(&sections, write::PWVC).payload();
    let pwvc: [u16; 3] = std::array::from_fn(|i| u16::from_be_bytes([p[2 * i], p[2 * i + 1]]));
    Track {
        candidate: c,
        columns,
        lag,
        theirs,
        pwvc,
    }
}

/// `f` over `items` on all cores, results in order.
fn parallel<T: Send, R: Send>(items: Vec<T>, f: impl Fn(T) -> R + Sync) -> Vec<R> {
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let mut slots: Vec<Option<T>> = items.into_iter().map(Some).collect();
    let chunk = slots.len().div_ceil(threads).max(1);
    std::thread::scope(|s| {
        let handles: Vec<_> = slots
            .chunks_mut(chunk)
            .map(|part| {
                let f = &f;
                s.spawn(move || {
                    part.iter_mut()
                        .map(|t| f(t.take().unwrap()))
                        .collect::<Vec<R>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect()
    })
}

// ---------------------------------------------------------------------------
// Error and fit

fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len().max(1) as f64
}

/// Our section for `tag`, as a single-section file.
fn our_file(w: &Waveforms, tag: [u8; 4]) -> Vec<u8> {
    let s = match tag {
        write::PWAV => write::pwav(&w.pwav),
        write::PWV2 => write::pwv2(&w.pwv2),
        write::PWV3 => write::pwv3(&w.pwv3),
        write::PWV4 => write::pwv4(&w.pwv4),
        write::PWV5 => write::pwv5(&w.pwv5),
        write::PWV6 => write::pwv6(&w.pwv6),
        _ => write::pwv7(&w.pwv7),
    };
    write::file(&[s.unwrap()]).unwrap()
}

/// compare.rs error per channel of `tag` for one track.
fn track_error(t: &Track, w: &Waveforms, tag: [u8; 4]) -> Vec<f64> {
    let ours = our_file(w, tag);
    let i = TAGS.iter().position(|&x| x == tag).unwrap();
    let (a, b) = (
        read::sections(&ours).unwrap(),
        read::sections(&t.theirs[i]).unwrap(),
    );
    waveform_error(&a[0], &b[0]).unwrap().error
}

/// Mean per-channel error of each tag in `tags` over the set.
fn errors(
    tracks: &[Track],
    cal: &WaveformCalibration,
    tags: &[[u8; 4]],
) -> BTreeMap<[u8; 4], Vec<f64>> {
    let per: Vec<Vec<Vec<f64>>> = parallel(tracks.iter().collect(), |t| {
        let w = Waveforms::encode(&t.columns, cal).unwrap();
        tags.iter().map(|&tag| track_error(t, &w, tag)).collect()
    });
    tags.iter()
        .enumerate()
        .map(|(k, &tag)| {
            let channels = (0..per[0][k].len())
                .map(|c| mean(&per.iter().map(|p| p[k][c]).collect::<Vec<_>>()))
                .collect();
            (tag, channels)
        })
        .collect()
}

/// One fitted number: where it lives in the calibration, its range, and the
/// section whose mean error chooses it.
struct Knob {
    name: &'static str,
    get: fn(&mut WaveformCalibration) -> &mut f32,
    range: (f32, f32),
    /// Search multiplicatively (gains, gammas) or additively (releases).
    log: bool,
    tag: [u8; 4],
}

macro_rules! knob {
    ($tag:expr, $lo:expr, $hi:expr, $log:expr, $($field:tt)+) => {
        Knob {
            name: stringify!($($field)+),
            get: |c: &mut WaveformCalibration| &mut c.$($field)+,
            range: ($lo, $hi),
            log: $log,
            tag: $tag,
        }
    };
}

/// Every fitted number, in fitting order. Fixed by construction, not fitted:
/// the low colour gain and mid whiteness weight (only ratios matter), the
/// PWV7 curve gains (folded into the track gain, so PWVC keeps rekordbox's
/// meaning), the auto-gain statistic and clamps (from rekordbox's PWVC, see
/// the doc) and PWV4's complement (saturates at 255 either way).
fn knobs() -> Vec<Knob> {
    use write::{PWAV, PWV2, PWV3, PWV4, PWV5, PWV6, PWV7};
    vec![
        knob!(PWV3, 0.5, 2.0, true, height.gain),
        knob!(PWV3, 0.3, 4.0, true, height.gamma),
        knob!(PWV3, 0.05, 20.0, true, whiteness_weights[0]),
        knob!(PWV3, 0.05, 20.0, true, whiteness_weights[2]),
        knob!(PWV3, 0.3, 8.0, true, whiteness.gain),
        knob!(PWV3, 0.2, 5.0, true, whiteness.gamma),
        knob!(PWV5, 0.1, 20.0, true, color[1].gain),
        knob!(PWV5, 0.1, 20.0, true, color[2].gain),
        knob!(PWV5, 0.2, 5.0, true, color[0].gamma),
        knob!(PWV5, 0.2, 5.0, true, color[1].gamma),
        knob!(PWV5, 0.2, 5.0, true, color[2].gamma),
        knob!(PWAV, 0.3, 3.0, true, preview_height.gain),
        knob!(PWAV, 0.3, 5.0, true, preview_height.gamma),
        knob!(PWV2, 0.3, 3.0, true, tiny_height.gain),
        knob!(PWV2, 0.3, 5.0, true, tiny_height.gamma),
        knob!(PWV4, 0.3, 3.0, true, pwv4_height.gain),
        knob!(PWV4, 0.3, 5.0, true, pwv4_height.gamma),
        knob!(PWV4, 0.2, 5.0, true, pwv4_bands[0].gain),
        knob!(PWV4, 0.2, 5.0, true, pwv4_bands[0].gamma),
        knob!(PWV4, 0.2, 5.0, true, pwv4_bands[1].gain),
        knob!(PWV4, 0.2, 5.0, true, pwv4_bands[1].gamma),
        knob!(PWV4, 0.2, 5.0, true, pwv4_bands[2].gain),
        knob!(PWV4, 0.2, 5.0, true, pwv4_bands[2].gamma),
        knob!(PWV4, 0.2, 5.0, true, pwv4_bands[3].gain),
        knob!(PWV4, 0.2, 5.0, true, pwv4_bands[3].gamma),
        knob!(PWV7, 0.01, 1.0, true, auto_gain[0].target),
        knob!(PWV7, 0.01, 1.0, true, auto_gain[1].target),
        knob!(PWV7, 0.01, 1.0, true, auto_gain[2].target),
        knob!(PWV7, 0.0, 0.99, false, detail_release[0]),
        knob!(PWV7, 0.0, 0.99, false, detail_release[1]),
        knob!(PWV7, 0.0, 0.99, false, detail_release[2]),
        knob!(PWV7, 0.2, 5.0, true, detail[0].gamma),
        knob!(PWV7, 0.2, 5.0, true, detail[1].gamma),
        knob!(PWV7, 0.2, 5.0, true, detail[2].gamma),
        knob!(PWV6, 0.1, 10.0, true, overview[0].gain),
        knob!(PWV6, 0.2, 5.0, true, overview[0].gamma),
        knob!(PWV6, 0.1, 10.0, true, overview[1].gain),
        knob!(PWV6, 0.2, 5.0, true, overview[1].gamma),
        knob!(PWV6, 0.1, 10.0, true, overview[2].gain),
        knob!(PWV6, 0.2, 5.0, true, overview[2].gamma),
    ]
}

/// Coordinate descent: each knob in turn takes the best of 17 values around
/// its current one, the span shrinking every round.
fn fit(tracks: &[Track], start: WaveformCalibration) -> WaveformCalibration {
    let mut cal = start;
    for (round, span) in [4.0f32, 1.6, 1.2, 1.06].into_iter().enumerate() {
        for k in knobs() {
            let cur = *(k.get)(&mut cal);
            let mut best = (mean(&errors(tracks, &cal, &[k.tag])[&k.tag]), cur);
            for i in -8..=8 {
                let t = i as f32 / 8.0;
                let v = if k.log {
                    cur.max(1e-3) * span.powf(t)
                } else {
                    cur + t * (span - 1.0) * 0.25
                }
                .clamp(k.range.0, k.range.1);
                let mut c = cal;
                *(k.get)(&mut c) = v;
                let e = mean(&errors(tracks, &c, &[k.tag])[&k.tag]);
                if e < best.0 - 1e-9 {
                    best = (e, v);
                }
            }
            *(k.get)(&mut cal) = best.1;
            eprintln!(
                "round {round} {:<28} {:>8.4} → {} error {:.4}",
                k.name,
                best.1,
                String::from_utf8_lossy(&k.tag),
                best.0
            );
        }
    }
    cal
}

// ---------------------------------------------------------------------------
// Output

fn curve(c: &Curve) -> String {
    format!("c({:.3}, {:.3})", c.gain, c.gamma)
}

fn auto_gain(a: &AutoGain) -> String {
    let stat = match a.stat {
        LevelStat::Mean => "LevelStat::Mean".to_string(),
        LevelStat::Quantile(q) => format!("LevelStat::Quantile({q})"),
    };
    format!(
        "AutoGain {{ stat: {stat}, target: {:.4}, min: {:?}, max: {:?} }}",
        a.target, a.min, a.max
    )
}

fn rust(cal: &WaveformCalibration) -> String {
    let curves = |v: &[Curve]| v.iter().map(curve).collect::<Vec<_>>().join(", ");
    let floats = |v: &[f32]| {
        v.iter()
            .map(|x| format!("{x:.3}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!(
        "pub const CALIBRATION: WaveformCalibration = WaveformCalibration {{\n    \
         height: {},\n    preview_height: {},\n    tiny_height: {},\n    \
         whiteness_weights: [{}],\n    whiteness: {},\n    silent_whiteness: {},\n    \
         color: [{}],\n    pwv4_height: {},\n    pwv4_complement: {},\n    \
         pwv4_bands: [{}],\n    auto_gain: [\n        {},\n    ],\n    \
         detail_release: [{}],\n    detail: [{}],\n    overview: [{}],\n}};",
        curve(&cal.height),
        curve(&cal.preview_height),
        curve(&cal.tiny_height),
        floats(&cal.whiteness_weights),
        curve(&cal.whiteness),
        cal.silent_whiteness,
        curves(&cal.color),
        curve(&cal.pwv4_height),
        cal.pwv4_complement,
        curves(&cal.pwv4_bands),
        cal.auto_gain
            .iter()
            .map(auto_gain)
            .collect::<Vec<_>>()
            .join(",\n        "),
        floats(&cal.detail_release),
        curves(&cal.detail),
        curves(&cal.overview),
    )
}

fn report(label: &str, e: &BTreeMap<[u8; 4], Vec<f64>>) {
    let mut all = Vec::new();
    for (tag, v) in e {
        all.push(mean(v));
        let channels: Vec<String> = v.iter().map(|x| format!("{x:.3}")).collect();
        eprintln!(
            "{label} {}: mean {:.3} ({})",
            String::from_utf8_lossy(tag),
            mean(v),
            channels.join(" ")
        );
    }
    eprintln!("{label} all sections: {:.3}", mean(&all));
}

/// Mean |ln(ours / rekordbox)| of the PWVC gains, per band.
fn pwvc_error(tracks: &[Track], cal: &WaveformCalibration) -> [f64; 3] {
    let per: Vec<[f64; 3]> = parallel(tracks.iter().collect(), |t| {
        let ours = Waveforms::encode(&t.columns, cal).unwrap().pwvc;
        std::array::from_fn(|b| (f64::from(ours[b]) / f64::from(t.pwvc[b])).ln().abs())
    });
    std::array::from_fn(|b| mean(&per.iter().map(|p| p[b]).collect::<Vec<_>>()))
}

// Pictures. Colours roughly as players draw them: 3-band low dark blue, mid
// amber, high white; PWV5 from its 3-bit RGB.
const BG: [u8; 3] = [18, 18, 22];
const LOW: [u8; 3] = [32, 83, 217];
const MID: [u8; 3] = [242, 170, 60];
const HIGH: [u8; 3] = [255, 255, 255];
const SEP: [u8; 3] = [90, 90, 100];

struct Canvas {
    w: usize,
    h: usize,
    px: Vec<u8>,
}

impl Canvas {
    fn new(w: usize, h: usize) -> Self {
        Self {
            w,
            h,
            px: BG.repeat(w * h),
        }
    }

    fn put(&mut self, x: usize, y: usize, c: [u8; 3]) {
        if x < self.w && y < self.h {
            let i = 3 * (y * self.w + x);
            self.px[i..i + 3].copy_from_slice(&c);
        }
    }

    /// Vertical bar centred on `mid`, `half` pixels each way.
    fn bar(&mut self, x: usize, mid: usize, half: usize, c: [u8; 3]) {
        for y in mid.saturating_sub(half)..=mid + half {
            self.put(x, y, c);
        }
    }

    /// Bar of `len` pixels up from the bottom of row `top..top + h`.
    fn up(&mut self, x: usize, top: usize, h: usize, len: usize, c: [u8; 3]) {
        for y in (top + h).saturating_sub(len.min(h))..top + h {
            self.put(x, y, c);
        }
    }

    fn line(&mut self, y: usize) {
        for x in 0..self.w {
            self.put(x, y, SEP);
        }
    }

    fn save(&self, path: &Path) {
        let f = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
        let mut enc = png::Encoder::new(f, self.w as u32, self.h as u32);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::High);
        enc.write_header()
            .unwrap()
            .write_image_data(&self.px)
            .unwrap();
    }
}

/// Six rows, rekordbox above reks in each pair: PWV6 (whole track, bands
/// stacked), PWV7 (1200 columns = 8 s from `start`, bands overlaid low →
/// mid → high), PWV5 (same 8 s).
fn picture(t: &Track, w: &Waveforms, start: usize, path: &Path) {
    const W: usize = 1200;
    const ROW: usize = 72;
    let mut cv = Canvas::new(W, 6 * (ROW + 1));
    let theirs = |tag| {
        let i = TAGS.iter().position(|&x| x == tag).unwrap();
        read::sections(&t.theirs[i]).unwrap()[0].payload().to_vec()
    };
    let ours6: Vec<u8> = w.pwv6.iter().flatten().copied().collect();
    let ours7: Vec<u8> = w.pwv7.iter().flatten().copied().collect();
    let ours5: Vec<u8> = w.pwv5.iter().flat_map(|v| v.to_be_bytes()).collect();
    let scale = |v: u8| usize::from(v) * (ROW - 4) / 127;
    for (row, data) in [theirs(write::PWV6), ours6].iter().enumerate() {
        let top = row * (ROW + 1);
        for (x, e) in data.as_chunks::<3>().0.iter().take(W).enumerate() {
            let (l, m, h) = (scale(e[0]) / 2, scale(e[1]) / 2, scale(e[2]) / 2);
            cv.up(x, top, ROW, l + m + h, HIGH);
            cv.up(x, top, ROW, l + m, MID);
            cv.up(x, top, ROW, l, LOW);
        }
        cv.line(top + ROW);
    }
    for (row, data) in [theirs(write::PWV7), ours7].iter().enumerate() {
        let top = (2 + row) * (ROW + 1);
        for (x, e) in data
            .as_chunks::<3>()
            .0
            .iter()
            .skip(start)
            .take(W)
            .enumerate()
        {
            for (v, c) in [(e[0], LOW), (e[1], MID), (e[2], HIGH)] {
                cv.bar(x, top + ROW / 2, scale(v) / 2, c);
            }
        }
        cv.line(top + ROW);
    }
    for (row, data) in [theirs(write::PWV5), ours5].iter().enumerate() {
        let top = (4 + row) * (ROW + 1);
        for (x, e) in data
            .as_chunks::<2>()
            .0
            .iter()
            .skip(start)
            .take(W)
            .enumerate()
        {
            let c = unpack_color(u16::from_be_bytes([e[0], e[1]]));
            let rgb = [c.red, c.green, c.blue].map(|v| (u16::from(v) * 255 / 7) as u8);
            cv.bar(
                x,
                top + ROW / 2,
                usize::from(c.height) * (ROW - 4) / 62,
                rgb,
            );
        }
        cv.line(top + ROW);
    }
    cv.save(path);
}

#[test]
#[ignore = "needs rekordbox's local analysis cache (REKS_RB_ANLZ) and the audio (REKS_CALIB_MUSIC)"]
fn fit_waveform_calibration() {
    let env = |k: &str| {
        PathBuf::from(std::env::var_os(k).unwrap_or_else(|| panic!("set {k} (see module docs)")))
    };
    let (cache, music) = (env("REKS_RB_ANLZ"), env("REKS_CALIB_MUSIC"));
    let n: usize = std::env::var("REKS_CALIB_TRACKS").map_or(50, |v| v.parse().unwrap());
    let all = candidates(&cache, &music);
    let picked = select(&all, n);
    eprintln!(
        "{} matched cache entries, {} picked",
        all.len(),
        picked.len()
    );
    let tracks = parallel(picked, load_track);
    for (i, t) in tracks.iter().enumerate() {
        let c = &t.candidate;
        eprintln!(
            "track {i:02}: {:<9} loudness {:5.2} lag {:2} {}",
            c.genre,
            c.loudness,
            t.lag,
            c.audio.extension().unwrap().to_string_lossy()
        );
    }

    report("current", &errors(&tracks, &CALIBRATION, &TAGS));
    let fitted = fit(&tracks, CALIBRATION);
    report("fitted", &errors(&tracks, &fitted, &TAGS));
    let pe = pwvc_error(&tracks, &fitted);
    eprintln!(
        "PWVC mean |ln(ours/rekordbox)|: low {:.3} mid {:.3} high {:.3}",
        pe[0], pe[1], pe[2]
    );
    eprintln!("{}", rust(&fitted));

    if let Some(dir) = std::env::var_os("REKS_CALIB_PNG").map(PathBuf::from) {
        std::fs::create_dir_all(&dir).unwrap();
        // One picture per genre bucket: its loudest track, from 60 s in.
        let mut seen = BTreeSet::new();
        for t in tracks.iter().rev() {
            if seen.insert(t.candidate.genre) {
                let w = Waveforms::encode(&t.columns, &fitted).unwrap();
                let path = dir.join(format!("{}.png", t.candidate.genre));
                picture(t, &w, 60 * 150, &path);
                eprintln!("wrote {}", path.display());
            }
        }
    }
}
