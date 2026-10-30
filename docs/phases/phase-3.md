# Phase 3 — Own analysis engine (BPM, beatgrid, key, waveforms)

Status: in progress (started 2026-10-20 ahead of the Phase 2 hardware checks;
tasks 1–11, 11b and 13 done; 12 measured, tuning (12a) in progress: BPM and downbeat met; key port verified, mode kept, tonic next; grid editing with tempo sections, tasks 15–18, added and done 2026-10-24; its hardware check, item 7, UNVERIFIED). Depends on: Phase 1 (library, decode) and Phase 2
(export pipeline, hardware checklist). Read `docs/decisions.md` D9, D10, D12,
D13 and `docs/research.md` §3 first.

## Goal

reks analyses tracks itself and produces beatgrids, keys and all Pioneer
waveform encodings (PWAV, PWV2, PWV3, PWV4, PWV5, PWV6, PWV7) so that any
track, not just rekordbox-imported ones, exports with full CDJ features. The
results are measured against rekordbox's analysis of the same tracks (the
maintainer's ~3,000-track collection) and the measurement is published in
the repo before tuning begins.

## Scope

- `reks-analysis` crate: decode → mono/resample → beat/downbeat tracking
  (Beat This!) → constant-grid/dynamic-grid builder (Mixxx BeatUtils port) →
  key (libKeyFinder port) → 3-band + mono waveform columns at 150/s + previews.
- `reks-anlz` crate: ANLZ `.DAT/.EXT/.2EX` builder from reks data using
  `rekordcrate::anlz` types; PPTH/PVBR/PQTZ/PCOB/PCO2/PWAV/PWV2–7 (+ PWVC
  copied/constant); reader for validation.
- Analysis cache on disk; background analysis queue with progress and
  cancellation; "needs analysis" → "analysed" lifecycle; re-analyse command.
- Comparison harness: rekordbox XML + rekordbox ANLZ cache vs reks results,
  with a report (`docs/analysis-parity.md`).
- Phase 2 writer switched from `AnlzSource::Generated` (baken) to reks ANLZ.
- UI: waveform display (from our data), grid editor (set downbeat,
  nudge, tap BPM, double/halve; tempo sections since 2026-10-24, tasks
  15–18), key display in the user's notation.

## Non-goals

- Phrase analysis (PSSI). Vocal detection. Loudness/gain.
- ~~Dynamic-grid *editing* beyond anchor nudge.~~ Changed 2026-10-24
  (maintainer's call, D10 "Grid editing with tempo sections"): full grid
  editing with tempo sections, tasks 15–18.
- Matching rekordbox byte-for-byte; the target is "indistinguishable on the
  player" and "same BPM/downbeat in the large majority of cases".
- Changing Phase 2's writer path (still baken for PDB); only ANLZ sourcing changes.

## Data structures and interfaces

```rust
// Built (tasks 1–2); see crates/reks-analysis/src/{input,beats}.rs.
pub struct AnalysisInput { pub mono: Vec<f32> /* 44.1 kHz */, pub beat_mono: Vec<f32> /* 22.05 kHz */,
                           pub source_rate: u32, pub source_channels: u16,
                           pub encoder_delay: Option<u32>, pub encoder_padding: Option<u32> /* source frames, already trimmed, see D4 */,
                           pub timeline: reks_audio::Timeline /* leading silence that puts sample 0 on rekordbox's timeline (task 4) */ }
pub struct BeatList { pub sample_rate: u32 /* 44_100 */, pub beats: Vec<Beat>, pub confidence: f32 }
pub struct Beat { pub sample: i64, pub downbeat: bool }   // BeatList::beats_s()/downbeats_s() give f64 seconds

pub struct BeatAnalysis {
    pub beats_s: Vec<f64>,          // raw beat times from the tracker
    pub downbeats_s: Vec<f64>,
    pub grid: Grid,
    pub confidence: f32,
}
pub enum Grid {                    // reks_analysis::grid; positions are i64 samples
    Constant { bpm: f64, first_beat: i64, first_beat_in_bar: u8 },          // bpm snapped per BeatUtils rules
    Dynamic  { beats: Vec<GridBeat> },                                        // GridBeat { position, beat_in_bar }
}
pub struct KeyAnalysis { pub key: Option<Key> /* None = silence */, pub confidence: f32, pub similarity: f32, pub chroma: [f32; 12] }

pub struct WaveformColumns {       // 150 columns per second
    pub low: Vec<u8>, pub mid: Vec<u8>, pub high: Vec<u8>,                   // 0..=255 per-band peak, pre-calibration
    pub mono: Vec<u8>,                                                       // 0..=31 height
}
pub struct Waveforms {             // encoded Pioneer forms
    pub pwav: [u8; 400], pub pwv2: [u8; 100],
    pub pwv3: Vec<u8>, pub pwv4: [[u8; 6]; 1200], pub pwv5: Vec<u16>,
    pub pwv6: [[u8; 3]; 1200], pub pwv7: Vec<[u8; 3]>,
}

pub fn analyse(input: &AnalysisInput, opts: &AnalysisOptions, progress: impl Fn(f32)) -> Result<Analysis>;
pub struct Analysis { pub beats: BeatAnalysis, pub key: KeyAnalysis, pub waves: Waveforms, pub duration_ms: u64 }

pub mod grid {   // Mixxx BeatUtils port (GPL-2.0-or-later origin, attributed); see crates/reks-analysis/src/grid.rs
    pub struct BeatList { pub sample_rate: u32, pub beats: Vec<TrackedBeat> }  // TrackedBeat { position: i64, downbeat: bool }
    pub fn build_grid(input: &BeatList, opts: &GridOptions) -> Result<Grid, GridError>;
    pub fn retrieve_const_regions(beats: &[i64], sample_rate: u32) -> Vec<ConstRegion>;   // 25 ms phase tolerance
    pub fn make_const_bpm(regions: &[ConstRegion], sample_rate: u32) -> Option<ConstTempo>;  // merges regions ≥ 16 beats
    pub fn round_bpm_within_range(min: f64, center: f64, max: f64) -> f64;   // integer → 0.5 → 0.1 → 0.01
}

// reks-anlz (built, task 9; see crates/reks-anlz/src/build.rs)
pub struct AnlzInput<'a> { pub contents_path: &'a str /* /Contents/... */, pub duration_ms: u64,
                           pub beats: &'a [PqtzBeat] /* grid::pqtz_beats(&Grid, rate, duration_ms) */,
                           pub cues: &'a [reks_core::track::Cue], pub waveforms: &'a Waveforms,
                           pub mp3_audio_frames: Option<u32> /* PVBR */ }
pub enum AnlzTarget { Standard /* .DAT + .EXT */, Cdj3000 /* + .2EX */ }
pub struct AnlzSet { pub dat: Vec<u8>, pub ext: Vec<u8>, pub ex2: Option<Vec<u8>> }
pub fn build(input: &AnlzInput, target: AnlzTarget) -> Result<AnlzSet, BuildError>;   // validates its output
pub fn validate_set(dat: &[u8], ext: &[u8], ex2: Option<&[u8]>, expect: Expect) -> Vec<Issue>;
// read: sections/path/pqtz_beats/cue_list (own reader; rekordcrate stays a test-only parser, D9)
```

Encoding rules (from research §3; big-endian)
- PPTH first after PMAI in all three files; UTF-16BE path with NUL.
- Section order (rekordbox's): `.DAT` PPTH, PVBR, PQTZ, PWAV, PWV2, PCOB hot,
  PCOB memory; `.EXT` PPTH, PWV3, PCOB hot, PCOB memory, PCO2 hot, PCO2
  memory, PQT2, PWV5, PWV4 (rekordbox adds PSSI); `.2EX` PPTH, PWV7, PWV6, PWVC.
- PVBR: 400 zero entries; for MP3 the trailer is audio frames × 1152 (as in
  both reference tracks), else zero. No real VBR seek table (UNVERIFIED).
- PQTZ: one 8-byte entry per beat from the grid's first beat while the beat
  is before the end of the track: `beat_number` 1–4, `tempo` = round(bpm×100),
  `time` = round(ms). Constant grid → identical tempo in every entry; dynamic
  grid → tempo from the gap to the next beat. Both reference grids start
  within one beat of 0; whether rekordbox ever emits beats before the first
  detected one is UNVERIFIED. rekordbox's own times are not exactly linear
  in its tempo (±1 ms on ~3% of beats), so the golden test allows 1 ms.
- PCOB/PCO2 (layouts from baken-export, pinned against rekordbox 7 exports):
  `.DAT` PCOB hot holds hot cues A–C, `.EXT` PCOB hot D–H, `.DAT` PCOB memory
  all memory cues/loops, `.EXT` PCOB memory stays empty; PCO2 hot and memory
  list every cue with comment, hot cue colour (16-colour palette) and loop
  numerator/denominator (whole-beat loops only). Hot cues in descending slot,
  memory in descending time (rekordbox's order in every export seen). No
  active-loop flag yet (the library has none).
- PQT2: the empty 56-byte form rekordbox writes without an extended grid;
  whether NXS2 features need a full PQT2 is UNVERIFIED.
- Waveform scaling is the task 8 fit (`reks-analysis::calib`, model and
  error in [analysis-parity/waveforms.md](../analysis-parity/waveforms.md)).
- PWAV 400 columns / PWV2 100 columns: peak mono over each column range
  through a height curve; height 5-bit (PWV2 4-bit); whiteness 3-bit from
  the weighted mid+high share of the band peaks.
- PWV3: height and whiteness per column at 150/s. PWV5: `R:3 G:3 B:3 H:5`
  per column, colour = per-band curves normalised so the loudest band is 7.
- PWV4: 1200 columns × 6 bytes: byte 0 = height (0–127), byte 1 = 255 −
  byte 0 (rekordbox's pair sums to ≈ 257), bytes 2–5 = low, low, mid, high
  band peaks through their own curves (byte 2 is documented as "energy
  < 10 kHz"; it tracks our < 600 Hz band at r = 0.99).
- PWV6 (1200) / PWV7 (150/s): 3 bytes per column in order **low, mid,
  high** (rekordbox's files; djl-analysis says mid, high, low), each band
  × a per-track gain; PWV7 adds a peak-hold release. PWVC: those three
  gains ×100, computed per track (rekordbox's values are per track too).
- `.2EX` written only for `AnlzTarget::Cdj3000`-capable exports (harmless elsewhere).

Analysis cache: `<cache_dir>/analysis/<track_id>/{analysis.json, bands.bin}`
([cache.rs](../../crates/reks-analysis/src/cache.rs): raw beats, grid, key,
confidences as JSON; waveform columns as raw bytes; the grid editor's
`columns.bin` sits next to it, to be unified). Valid only for the same
`ANALYSER_VERSION` and file content hash. The library DB keeps the grid in
`beatgrid` (`source = 'analysis'`), key/BPM on the track row, and
`analysis_version`, `analysis_hash`, `analysed_at`, `analysis_error`,
`beat_confidence`, `key_confidence` on the track row (migration 0007).
Analysis never overwrites manual or rekordbox grids, or manual, tag or
rekordbox keys (D10, "Queue"). A track rekordbox analysed has a rekordbox entry instead
(`rekordbox.json` + rekordbox's `rekordbox.{DAT,EXT,2EX}` in the same folder,
`track.analysis_source = 'rekordbox'`; D10, "rekordbox mapping").

## Tasks

1. [x] **Decode + conditioning**: `AnalysisInput` from `reks-audio::decode_f32`;
   mono mix; resample to 22.05 kHz for the beat model (`rubato`, which
   `beat-this` already pulls in) and keep 44.1 kHz for key/waveform; capture
   Symphonia's reported delay/padding; tests on fixtures. Done:
   [`decode_f32`](../../crates/reks-audio/src/float.rs) (interleaved or
   mono-mixed `f32`, delay/padding from the container) and
   [input.rs](../../crates/reks-analysis/src/input.rs) (`AnalysisInput`;
   both rates resampled from the source through `StreamResampler`, so sample
   0 is the same instant; all positions in the crate are samples at
   `ANALYSIS_RATE` = 44.1 kHz). Finding for task 4: MP3 (LAME) reports a
   1105-frame delay and gapless decode trims it, but Symphonia 0.6 reports no
   delay for ffmpeg's AAC (priming sits in the MP4 edit list, not applied), so
   that `.m4a` decodes 1024 frames (23 ms) late; pinned by a test.
2. [x] **Beat/downbeat tracking**: integrate `beat-this` (rten backend, small
   model bundled as a Tauri resource; full model optional download); wrap in
   `spawn_blocking`; test: 128 BPM click fixture → beats within 10 ms, 4/4
   downbeats correct. Done: [beats.rs](../../crates/reks-analysis/src/beats.rs)
   (`BeatTracker::load`/`track`, blocking, `Send`; one tracker per worker).
   Model files and provenance in
   [models/](../../crates/reks-analysis/models/README.md), bundled via
   `src-tauri/tauri.conf.json`; full-model download is a TODO in
   `ModelPaths::small`. The model's peaks sat 5–15 ms after the transient on
   both fixtures, so each beat is refined (activation parabola, then snapped to
   the sharpest high-frequency energy rise within ±30 ms; initial constants,
   to be checked in task 12). Results: click fixture (WAV, MP3) max 0.2 ms; a
   generated 4/4 drum pattern
   ([bars.rs](../../fixtures/gen/src/bars.rs)) max 2.3 ms with every downbeat
   right; silence gives no beats. The plain click fixture has no accents, so
   its downbeats are not tested. Tests in
   [tests/beats.rs](../../crates/reks-analysis/tests/beats.rs).
3. [x] **Grid builder**: port Mixxx `BeatUtils::retrieveConstRegions` /
   `makeConstBpm` / `roundBpmWithinRange` with attribution; choose the
   downbeat nearest the start of the longest constant region as beat 1;
   fall back to `Dynamic` when no region covers ≥ 80% of beats; tests:
   synthetic beat lists with jitter, tempo change at 2:00, half/double
   tempo ambiguity (prefer 70–180 BPM range by genre-agnostic octave rule).
   Done: [`grid.rs`](../../crates/reks-analysis/src/grid.rs). Coverage is the
   share of all beats within 25 ms of the constant grid; the octave rule
   doubles below 70 and halves above 180 BPM (dynamic grids by their median
   tempo), so 80–90 and 160–180 BPM stay as tracked. Every BPM rounding rung
   is range-checked (Mixxx's unchecked 1/2 and 2/3 guesses and 1/12 rung are
   dropped). The constant grid's `first_beat` is extrapolated back to the
   earliest beat ≥ 0; the PQTZ writer decides whether to emit beats before
   the first tracked one.
4. [x] **Decoder-delay calibration**: export a reference set from rekordbox
   (same MP3/AAC/FLAC files) and compare rekordbox cue/grid times with ours;
   determine the per-codec offset convention (Mixxx saw 26/52 ms on MP3);
   encode the result in `reks-audio` and record it in D4.
   Done: measured on the maintainer's library instead of re-encoded copies
   (rekordbox's grid vs our raw beats, per-track median, grouped by codec and
   MP3 header; the lossless tracks give the tracker's own bias, +1.8 ms):
   LAME-tagged MP3 +51.2 ms (delay + Info frame), ffmpeg-tagged MP3 +25.1 ms
   (delay), untagged MP3 and lossless 0; Symphonia matches ffmpeg's gapless
   decode exactly, so this is rekordbox's convention. Encoded in
   [timing.rs](../../crates/reks-audio/src/timing.rs) (Info/LAME tag reader,
   rules); `decode_f32` and `decode_stereo` prepend the lead, so all reks
   positions are rekordbox positions (D4). AAC stays as decoded,
   **UNVERIFIED** (one track). New fixture `sine-click-128-lame.mp3` (LAME's
   own tag). Numbers: [analysis-parity.md](../analysis-parity.md).
5. [x] **Key detection**: port libKeyFinder's pipeline (downsample, Blackman
   windowed chromagram across octaves, tone profiles, cosine similarity)
   on `rustfft`; tests: synthetic chord fixtures for all 24 keys; CI job
   that, when `libkeyfinder` is installed, compares against the C++ output
   on fixtures (must match ≥ 95%).
   Done: `reks_analysis::key::detect_key(&[f32], sample_rate)` →
   `KeyAnalysis { key: Option<reks_core::Key> /* None = silence */,
   confidence, similarity, chroma }`; any rate ≥ ~4.35 kHz. Notation and
   spelling come from `reks_core::Key`. All 24 synthetic progressions
   detected; noise/silence safe. C++ parity: ignored test + manual workflow
   `keyfinder-parity.yml` with `fixtures/keyfinder/driver.cpp`; run locally
   2026-10-21 (task 12a): 26/26 fixtures and 2,644/2,644 library tracks
   identical to the C++ library. `confidence` is our own, uncalibrated.
6. [x] **Waveform columns**: 3 band filters (start: 4th-order Bessel at 600 Hz
   and 4 kHz as in Mixxx) + per-column peak at 150 columns/s; mono height;
   streaming over PCM; tests on sine sweeps (band bytes track frequency).
   Done: [waveform.rs](../../crates/reks-analysis/src/waveform.rs).
   `WaveformAnalyzer` takes interleaved f32 chunks of any size (mono mix),
   `WaveformColumns` has ⌈frames × 150 / rate⌉ columns (partial last column
   counts). Filters are designed from the Bessel prototype (bilinear,
   pre-warped); mid = HP 600 Hz ∘ LP 4 kHz, not Mixxx's band-pass transform.
   Tests: sines per band, exponential sweep, odd durations, chunked = one-shot.
7. [x] **Pioneer encoders**: PWAV/PWV2/PWV3/PWV4/PWV5/PWV6/PWV7 from columns;
   unit tests on bit packing; round-trip through rekordcrate parse.
   Done: [waveform.rs](../../crates/reks-anlz/src/waveform.rs) (`Waveforms`
   lives in `reks-anlz`, plus `pwvc`; `Waveforms::{dat,ext,ex2}_sections`)
   and [write.rs](../../crates/reks-anlz/src/write.rs) (hand-written
   big-endian section framing, see the D9 finding). Scales come from
   `WaveformCalibration` (first-guess placeholder, replaced by task 8's fit). Section headers match rekordcrate's
   rekordbox 6 reference files byte for byte; every section parses with
   rekordcrate and writes back byte-identical.
8. [x] **Calibration against rekordbox**: for 50 rekordbox-analysed tracks, load
   rekordbox's `.EXT/.2EX` from the local cache, fit per-band gain/gamma and
   the whiteness heuristic to minimise column error; store constants in
   `reks-analysis::calib`; produce side-by-side PNGs in `docs/analysis-parity/`.
   Done: [`calib.rs`](../../crates/reks-analysis/src/calib.rs) (model and
   `CALIBRATION`, the `Default`), encoders reworked in
   [waveform.rs](../../crates/reks-anlz/src/waveform.rs), fit and pictures in
   the ignored [tests/calibrate.rs](../../crates/reks-anlz/tests/calibrate.rs)
   (`REKS_RB_ANLZ`, `REKS_CALIB_MUSIC`). Mean compare.rs error over all
   sections 0.223 → 0.097; method, per-channel table and pictures in
   [analysis-parity/waveforms.md](../analysis-parity/waveforms.md). Findings
   (D13): PWV6/PWV7 are low, mid, high; PWVC is the per-track band gain
   ×100, now computed; MP3 columns lag ours by 4–8 (task 4). Crossovers
   unchanged (follow-up: a 250 Hz / 2 kHz set for PWV6/PWV7).
9. [x] **ANLZ builder** (`reks-anlz`): assemble `.DAT/.EXT/.2EX` with
   rekordcrate types; validator; golden test: build ANLZ for a reference
   track and compare section lists, lengths and PQTZ/PCOB contents with
   rekordbox's files (waveform bytes compared by error metric, not equality).
   Done: [build.rs](../../crates/reks-anlz/src/build.rs) (`build`,
   `AnlzInput`, `AnlzTarget`; `.2EX` only for `Cdj3000`),
   [grid.rs](../../crates/reks-anlz/src/grid.rs) (`pqtz_beats`),
   [cue.rs](../../crates/reks-anlz/src/cue.rs),
   [validate.rs](../../crates/reks-anlz/src/validate.rs) (run on every
   build), [compare.rs](../../crates/reks-anlz/src/compare.rs) (layout +
   waveform error metric for tasks 8/12), section writers in
   [write.rs](../../crates/reks-anlz/src/write.rs). Not rekordcrate types:
   they can't be constructed (D9 task 9 finding, proposed); rekordcrate
   round-trips every file without point cues byte-identically, our reader
   round-trips the rest (property test). Golden reference:
   [`fixtures/rekordbox/ref-rb6-demo`](../../fixtures/rekordbox/ref-rb6-demo/README.md)
   (rekordbox 6, no audio): same section lists and lengths, identical
   PPTH/PVBR/PCOB/PCO2 bytes, PQTZ beat numbers and tempo exact and times
   within 1 ms (360/368, 248/257 exact). The audio-driven comparison
   (`tests/golden.rs`, ignored) needs a reference set with audio in
   `REKS_ANLZ_REF`. Task 10 maps `TargetProfile` to `AnlzTarget` and gets
   `mp3_audio_frames` from the probe.
10. [x] **Phase 2 integration**: `AnlzSource::Reks` in the planner; prefer reks
    analysis when present, rekordbox cache otherwise (user setting);
    `reks-cli analyse <path>` and `reks-cli anlz build`.
    Done: [reksanlz.rs](../../crates/reks-export-rb/src/reksanlz.rs).
    The planner asks a `ReksLocator` (`ReksCache`: a current analysis cache
    entry, `cache::has_valid`) beside rekordbox's; the content hash comes
    from the track row while path, size and mtime match it
    (`analysis::current_content_hash`, the folder scan's test and blind
    spot), so planning and the writer read only files changed since the scan; `ExportOptions::anlz_preference`
    = the app setting `export.anlz_preference` (`AnlzPreference`:
    `rekordboxFirst` default, `reksFirst`, `rekordboxOnly`; picked and saved
    in the export wizard's options step, `--anlz` in `reks rb`); tracks with
    neither still follow `missing_analysis`. A user-owned grid (locked or
    edited; `SourceTrack::grid_user_owned`, one `Library::user_owned_grids`
    query per export) puts reks first under `rekordboxFirst` too, so the edit
    reaches the stick. `BakenWriter::prepare` builds
    each set into its scratch folder before any write (a stale entry refuses
    the export) and hands baken the `.DAT` as a cache entry, so baken rewrites
    `PPTH` and splices the XML cues as for rekordbox's files. A set holds the
    cached waveforms, the **library's** grid
    (`reks_anlz::grid::pqtz_beats_from_anchors`, so grid edits reach the
    stick) and cues, and `PVBR` from `reks_audio::mp3_audio_frames` (new `mp3`
    module beside the probe, which has no frame count; MPEG-1 Layer III
    only, else a zero trailer). Every `TargetProfile` maps to `AnlzTarget::Cdj3000`: the profile
    names the oldest player, so any stick may meet a CDJ-3000, and rekordbox
    and baken write `.2EX` on every export. The read-back validator runs
    `reks_anlz::validate_set` on every reks-built set on the stick
    (`Finding::AnlzInvalid`). Tests:
    [tests/reks_anlz.rs](../../crates/reks-export-rb/tests/reks_anlz.rs)
    (precedence, edited/locked grid, changed file, stale cache, temp-stick
    export validated and parsed with rekordcrate where it can, D8). CLI: `reks analyse <audio>`,
    `reks anlz build <audio> <out-dir>` (fresh analysis, no cues) or
    `--library <db> --track <id> <out-dir>` (what an export writes). No
    `PSSI`/`PVB2` (FLAC seek table), as in baken's generated files; all of it
    UNVERIFIED on players until task 14.
11. [x] **Background queue**: analyse on import/scan with N parallel workers
    (rayon, N = cores − 1), progress channel, pause/cancel, re-analyse
    selection; store results; "needs analysis" filter goes away.
    Done: pipeline glue in
    [pipeline.rs](../../crates/reks-analysis/src/pipeline.rs) (`analyse_file`
    → `TrackAnalysis`; dynamic grids become one `beatgrid` anchor per tempo
    change, 2 ms tolerance), cache in
    [cache.rs](../../crates/reks-analysis/src/cache.rs), queue in
    [queue.rs](../../crates/reks-analysis/src/queue.rs) (`AnalysisQueue`:
    dedicated rayon pool, one `BeatTracker` per worker, events through a
    sink), library side in
    [analysis.rs](../../crates/reks-core/src/analysis.rs) (pending list,
    precedence rules). The app queues pending tracks at start and after
    every scan, XML import and conversion; the list toolbar shows
    "Analysing n/N" with pause/resume/cancel, and the track menu has
    Re-analyse (ignores the cache). Cancel drops queued tracks and discards
    running ones (decode/inference can't be interrupted). Unreadable files
    are retried next time; undecodable ones are recorded in
    `analysis_error` and wait for a file or analyser change. The filter is
    gone; the "no grid" badge stays for tracks analysis could not grid.
    Tests: [tests/queue.rs](../../crates/reks-analysis/tests/queue.rs)
    (completion, pause, cancel, cache reuse and invalidation, manual data,
    failures). Throughput on real tracks is unmeasured (task 12).
    Order (2026-10-21): pending tracks with a rekordbox grid go last.
11b. [x] **rekordbox mapping** (2026-10-21, maintainer decision, D10): before
    queueing, tracks rekordbox analysed are mapped into the cache instead of
    analysed ([`rbmap.rs`](../../crates/reks-export-rb/src/rbmap.rs), app and
    `reks rb map --library <db>`). rekordbox's waveforms are read by
    [`rbwave.rs`](../../crates/reks-anlz/src/rbwave.rs) and written back
    unchanged by the reks ANLZ builder. Migration 0008 adds
    `track.analysis_source` (`reks` / `rekordbox`). On the maintainer's
    library, 2,284 tracks were mapped in 10 s instead of about 11 CPU-hours
    of analysis. Tests:
    [tests/rbmap.rs](../../crates/reks-export-rb/tests/rbmap.rs) (XML
    `TEMPO` → rekordbox grid rows, mapping, pending list, built ANLZ with
    rekordbox waveforms before and after a grid edit) and the round trip in
    `rbwave.rs`. On players: UNVERIFIED (task 14).
12. [x] **Parity measurement**: run the whole imported library through reks;
    report BPM agreement (|Δ| ≤ 0.05), downbeat agreement (first beat within
    30 ms modulo bar), key agreement (exact, relative, neighbour), with
    per-genre breakdown; publish `docs/analysis-parity.md` with the method.
    Then tune and re-measure.
    Done (measurement): `reks parity run|report`
    ([parity.rs](../../crates/reks-cli/src/parity.rs); XML and output dir
    from flags or `REKS_PARITY_XML`/`REKS_PARITY_OUT`, resumable, per-track
    data stays outside the repo). Baseline on the 1,762 one-anchor tracks:
    BPM 85.1%, downbeat 70.9%, key exact 62.3%, exact/relative 63.8%, all
    below the bar; throughput 25× real time per core (met). Multi-anchor
    grids (hand edits) are reported, not scored. Method, numbers and tuning
    plan: [analysis-parity.md](../analysis-parity.md).
12a. **Tuning** (plan in [analysis-parity.md](../analysis-parity.md#tuning-plan),
    priority BPM → downbeat → key). Re-measure after each step.
    - [x] Fixed grid first: whole-track tempo fit (`grid::fit_tempo`),
      `min_coverage` 0.3, dynamic only on a tempo change
      (`grid::tempo_change`).
    - [x] Kick: low-band onset curve ([onset.rs](../../crates/reks-analysis/src/onset.rs));
      the kick chooses the grid's phase among clusters of beats
      (`grid::kick_phase`); kick-placed beats measured worse, not used.
      `reks parity run` stores every beat placement and the kick curve.
      Version 3 on the 1,762 one-anchor tracks: BPM 85.1% → 96.7% (met),
      downbeat 70.9% → 73.3% (below).
    - [x] Bar placement ([bar.rs](../../crates/reks-analysis/src/bar.rs)):
      each bar position scores its share of the tracker's downbeats plus
      0.3 for the first kick; phrase cues (kick re-entry, accent) measured,
      no gain. Version 4: downbeat 73.3% → 83.7% (met), BPM and grid phase
      unchanged.
    - [x] libKeyFinder C++ check, then the mode decision (key): our port
      equals the C++ library on all 2,644 decoded tracks
      (`reks parity keyfinder`); mode stays libKeyFinder's, since every
      mode rule that scores higher does so by calling nearly everything
      minor (D12). Key unchanged: 62.3% exact, 63.8% exact/relative.
    - [ ] Key tonic: wrong-tonic misses (15.6%; semitone up in minor the
      largest group) before the key bar can be met ([#22](https://github.com/ninoman/reks/issues/22)).
13. [x] **Grid editor UI**: waveform view from our columns (zoomable, 150/s),
    beat markers, set-downbeat-here, nudge ±1 ms/±10 ms, tap tempo,
    double/halve, lock grid; edits write `beatgrid` rows with `source =
    manual` and are preserved on re-analysis.
    Done: maths and storage in
    [beatgrid/](../../crates/reks-core/src/beatgrid/mod.rs) (`GridEdit`,
    `tap_tempo` = least-squares slope of the newest tap run, `beat_marks`;
    positions whole ms, tempo `bpm_x100`). Set-downbeat re-phases the bar
    without moving beats; double/halve keep downbeats (halve keeps every
    other one); a constant grid's tempo becomes the track's BPM
    (`bpm_source = manual`). Only nudge and lock apply to dynamic grids.
    Lock = `track.grid_locked` (migration 0006). Re-analysis (task 11)
    keeps the grid and BPM when the track is locked or has any non-analysis
    row (`reks_core::analysis::store`; same rule as
    `Library::grid_is_user_owned`). Columns: [columns.rs](../../crates/reks-analysis/src/columns.rs)
    computes them on demand and caches `analysis/<track_id>/columns.bin`
    (keyed by path/size/mtime + format version); `reks-cli columns <audio>
    <out.bin>` writes the same payload. UI: a grid editor dialog, opened
    from the track info panel's Beatgrid section (replaced 2026-10-29 by
    grid mode on the deck, phase 5 task 32); commands in
    [grid.rs](../../src-tauri/src/grid.rs). Checked in the browser preview
    on the `sine-click-128` fixture's real columns (mock backend).
14. **Hardware check**: export 20 reks-analysed tracks (no rekordbox data)
    and run the Phase 2 checklist on NXS2 and CDJ-3000; log ([#23](https://github.com/ninoman/reks/issues/23)).

Grid editing with tempo sections (added 2026-10-24, maintainer's call; D10
"Grid editing with tempo sections" has the model and the rules):

15. [x] **Section model and edits** (`reks-core::beatgrid`): one beat rule
    (`grid_beats`: a section's beats stop half a beat before the next
    marker; the first section runs back to 0); `validate_anchors` refuses
    degenerate grids on every edit; new `GridEdit`s, each a pure function
    over anchors: `addMarker`, `moveMarker`, `deleteMarker`,
    `setSectionBpm`, `stretch` (beat k of a section lands on a time; the
    tempo rounds to 0.01 BPM, so it lands within ~1 ms per 64 beats),
    `setDownbeat` (the section at the time), `double`/`halve` (a section or
    all), `nudge` (whole grid), `reset` (a section or all, to the grid from
    before the first edit: `beatgrid_base`, migration 0013; else reks's
    cached analysis), `setAnchors` (the editor's undo/redo). Edits store
    `manual` rows (user-owned, analysis keeps them); XML re-import skips
    user-owned grids. Tests: unit tests per edit, property tests (valid
    grids give strictly increasing beats; XML round trip of anchors).
    Done: [`beatgrid/`](../../crates/reks-core/src/beatgrid/mod.rs)
    (`rule.rs` beat rule and validation, `edit.rs` edits, `tap.rs`),
    migration 0013 (`track.grid_base`); tests in the module,
    [tests/beatgrid.rs](../../crates/reks-core/tests/beatgrid.rs) and
    [tests/rbxml_grid.rs](../../crates/reks-core/tests/rbxml_grid.rs).
16. [x] **Export**: `PQTZ` from `grid_beats`; `anchors_from_pqtz` for stick
    reads (beat list back within 1 ms, anchors exact for canonical grids); `AnlzSource::ReksGrid` so a user-owned grid reaches
    the stick under `rekordboxFirst` without reks analysis; validator
    checks `PQTZ` order (exists) — test it on a stick. Tests: property
    round trip through `PQTZ`, our reader and rekordcrate; XML writes every
    section. Done: [`reks-anlz::grid`](../../crates/reks-anlz/src/grid.rs)
    (`pqtz_beats_from_anchors`, `anchors_from_pqtz`), `AnlzSource::ReksGrid`
    in the planner and [reksanlz.rs](../../crates/reks-export-rb/src/reksanlz.rs);
    tests in [tests/sections.rs](../../crates/reks-anlz/tests/sections.rs),
    `tests/build.rs`, `tests/reks_anlz.rs` and `tests/rbmap.rs`.
17. [x] **Decks** (`reks-mix`): `BeatMap` from the beat list; sync, beat
    jump, beat loops and quantise on a two-tempo fixture (140 → 174 BPM
    click track, `fixtures/gen`). Done: `BeatMap` takes `GridBeat`s
    ([beatmap.rs](../../crates/reks-mix/src/beatmap.rs)) from
    `reks_core::deck::deck_beats`; fixture
    [tempo.rs](../../fixtures/gen/src/tempo.rs); render tests in
    [tests/sections.rs](../../crates/reks-mix/tests/sections.rs) (beat jumps
    and a beat loop over the change land within a frame of the clicks; a
    synced 128 BPM deck stays within 0.08 ms through it).
18. [x] **Editor UI**: section markers on a zoomable waveform, select a
    section, drag a marker (move) or Alt-drag a beat (stretch), add/delete
    marker, typed or tapped BPM per section, set downbeat, ½/×2, reset
    section/all, shift, snap to transients (`waveform_transients`),
    keyboard shortcuts, undo/redo. Tests: component tests for each
    operation, history reducer. Done in a dialog, since replaced by grid
    mode on the deck (phase 5 task 32); transients in
    [`reks-analysis::transients`](../../crates/reks-analysis/src/transients.rs)
    (`waveform_transients`). Checked in the browser preview (mock backend);
    layout now in [ui.md](../ui.md#deck-grid-mode-phase-5-task-32-d10).

## Acceptance criteria

- On the maintainer's library (≥ 2,500 rekordbox-analysed tracks), reks
  agrees with rekordbox on BPM (|Δ| ≤ 0.05) for ≥ 90% of tracks and on
  downbeat phase for ≥ 80%; key exact match ≥ 70% and exact-or-relative
  ≥ 85%. Numbers are reported per genre; failures are inspectable in the UI.
  (If the measured baseline is lower, the phase ends with the measurement
  published and a tuning plan, not with a silently lowered bar.)
- Analysis throughput ≥ 5× real time per core on the small model.
- Exported reks-analysed tracks show grids, keys and colour/3-band waveforms
  on NXS2 and CDJ-3000 that a DJ cannot distinguish from rekordbox's at
  normal zoom (manual checklist), and quantised hot cues land on the beat.
- Every ANLZ we write parses with rekordcrate and passes the Phase 2 validator.

## Tests to write

- Click/sine fixtures at 90/128/174 BPM with known downbeats and keys; tempo
  change fixture; silence/noise fixture (must not crash, low confidence).
- BeatUtils port: table-driven tests transcribed from Mixxx's
  `beatutils_test.cpp` behaviour (reimplemented, attributed).
- Key: 24 synthetic chord progressions; enharmonic spelling; Camelot output.
- Encoders: bit-exact packing tests; PWV5 field extraction; column counts for
  odd durations; `len_tag` arithmetic.
- ANLZ golden comparisons against `fixtures/rekordbox/ref-<ver>/USBANLZ`.
- Property test: build → parse → build is byte-identical.

## Manual CDJ test checklist (delta from Phase 2)

1. Load a reks-analysed track next to the same track analysed by rekordbox
   on the other deck (rekordbox stick): waveform colours and heights look
   the same at 3 zoom levels; beatgrid lines coincide; BPM display equal.
2. Quantise on: hot cue placed on a kick snaps correctly; beat jump 1/4/8
   bars lands on bars.
3. 3-band waveform on CDJ-3000 shows lows/mids/highs in the expected bands
   (compare on a sweep track).
4. Key display matches the user's notation preference; master tempo pitch
   change does not alter displayed key.
5. Long track (12 min): grid still aligned at the end.
6. Timing (task 4): one LAME-encoded and one ffmpeg-encoded MP3, reks-analysed:
   grid lines sit on the kicks at the start and the end, as with the
   rekordbox-analysed copy (the CDJ's MP3 decoder must agree with rekordbox's
   timeline; UNVERIFIED until run).
7. Multi-tempo grid (tasks 15–18): export the 140 → 174 BPM fixture and a
   real tempo-change track with an edited grid (several sections, one
   section re-phased) on NXS2 and CDJ-3000: grid lines follow every
   section, the BPM display changes at each marker, the beat counter and
   bar phase stay right across markers, beat jump and quantised loops land
   on beats in both sections, and sync to another deck holds through the
   change. Also whether the player needs a full `PQT2` for any of it.
   UNVERIFIED until run.

## Risks

- **Parity with rekordbox may be lower than hoped**, especially downbeats and
  keys. Mitigation: measure first (task 12), expose confidence, make the grid
  editor good, keep rekordbox-cache data as the preferred source when present.
- **Model inference cost and size** on older laptops. Mitigation: small
  model default, batch queue, cancel.
- **Waveform calibration is a fit, not a spec**; some tracks will look off.
  Mitigation: calibration set spans genres and loudness; UI shows our
  waveform so users see what the player will show.
- **PQT2/PWVC unknowns**: NXS2 may expect PQT2 for some features. Mitigation:
  copy structure from reference files; hardware checklist.
- **GPL code provenance**: BeatUtils and libKeyFinder ports must be clean
  reimplementations with attribution headers; no copying from unlicensed repos.
