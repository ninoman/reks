# Phase 3 — Own analysis engine (BPM, beatgrid, key, waveforms)

Status: not started. Depends on: Phase 1 (library, decode) and Phase 2
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
- UI: waveform display (from our data), grid editor basics (set downbeat,
  nudge, tap BPM, double/halve), key display in the user's notation.

## Non-goals

- Phrase analysis (PSSI). Vocal detection. Loudness/gain.
- Dynamic-grid *editing* beyond anchor nudge.
- Matching rekordbox byte-for-byte; the target is "indistinguishable on the
  player" and "same BPM/downbeat in the large majority of cases".
- Changing Phase 2's writer path (still baken for PDB); only ANLZ sourcing changes.

## Data structures and interfaces

```rust
pub struct AnalysisInput { pub pcm: Pcm /* f32 interleaved */, pub sample_rate: u32, pub channels: u8,
                           pub decoder_delay_samples: u32 /* from Symphonia, see D4 */ }

pub struct BeatAnalysis {
    pub beats_s: Vec<f64>,          // raw beat times from the tracker
    pub downbeats_s: Vec<f64>,
    pub grid: Grid,
    pub confidence: f32,
}
pub enum Grid {
    Constant { first_beat_s: f64, bpm: f64, first_beat_in_bar: u8 },        // bpm snapped per BeatUtils rules
    Dynamic  { anchors: Vec<Anchor> },                                        // Anchor { time_s, bpm, beat_in_bar }
}
pub struct KeyAnalysis { pub key: Key, pub confidence: f32, pub chroma: [f32; 12] }

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

pub mod grid {   // Mixxx BeatUtils port (GPL-2.0-or-later origin, attributed)
    pub fn const_regions(beats: &[f64], max_phase_err_s: f64 /*0.025*/, min_beats: usize /*16*/) -> Vec<Region>;
    pub fn make_const_bpm(regions: &[Region], downbeats: &[f64]) -> Option<(f64 /*bpm*/, f64 /*first beat*/, u8)>;
    pub fn round_bpm_within_range(bpm: f64, region_len_beats: usize, tol_s: f64) -> f64;   // integer → 0.5 → 0.1 → 0.01
}

// reks-anlz
pub struct AnlzSet { pub dat: Vec<u8>, pub ext: Vec<u8>, pub ex2: Option<Vec<u8>> }
pub fn build(track: &TrackDetail, analysis: &Analysis, contents_path: &str /* /Contents/... */, target: AnlzTarget) -> Result<AnlzSet>;
pub fn parse(bytes: &[u8]) -> Result<rekordcrate::anlz::ANLZ>;
```

Encoding rules (from research §3; big-endian)
- PPTH first after PMAI; UTF-16BE path with NUL.
- PQTZ: one 8-byte entry per beat for the whole track: `beat_number` 1–4,
  `tempo` = round(bpm×100), `time` = round(ms). Constant grid → identical
  tempo in every entry. Beats before the first anchor are extrapolated
  backwards to 0 only if rekordbox does so (check reference; UNVERIFIED).
- PCOB type 0 (memory) and type 1 (hot) in `.DAT`; PCO2 same in `.EXT`
  with `color_id`, comment, loop numerator/denominator (quantised loops only).
- PWAV 400 columns / PWV2 100 columns: resample the mono column stream;
  height 5-bit (PWV2 4-bit); whiteness 3-bit from high-band ratio (initial
  heuristic; calibrate).
- PWV3: mono byte per column at 150/s. PWV5: `R:3 G:3 B:3 H:5` per column
  from band energies (initial map: R = low, G = mid, B = high scaled to 0–7).
- PWV4: 1200 columns × 6 bytes: bytes 3–5 = low/mid/high thirds, byte 2 =
  energy < 10 kHz, bytes 0–1 = whiteness placeholder (copy behaviour from a
  reference until understood).
- PWV6 (1200) / PWV7 (150/s): 3 bytes per column in order mid, high, low
  after per-band gain/gamma calibration; PWVC: three u16, UNVERIFIED
  semantics. Not a constant: the two reference tracks hold (91, 138, 396)
  and (145, 249, 500); we write the first until the meaning is known.
- `.2EX` written only for `AnlzTarget::Cdj3000`-capable exports (harmless elsewhere).

Analysis cache: `app_cache_dir/analysis/<track_id>/{analysis.json, columns.bin}`;
invalidated when `content_hash` or analyser version changes. The library DB
stores the grid, key, confidence and `analysis_version` on the track row
(migration 0002).

## Tasks

1. **Decode + conditioning**: `AnalysisInput` from `reks-audio::decode_f32`;
   mono mix; resample to 22.05 kHz for the beat model (`rubato`, which
   `beat-this` already pulls in) and keep 44.1 kHz for key/waveform; capture
   Symphonia's reported delay/padding; tests on fixtures.
2. **Beat/downbeat tracking**: integrate `beat-this` (rten backend, small
   model bundled as a Tauri resource; full model optional download); wrap in
   `spawn_blocking`; test: 128 BPM click fixture → beats within 10 ms, 4/4
   downbeats correct.
3. **Grid builder**: port Mixxx `BeatUtils::retrieveConstRegions` /
   `makeConstBpm` / `roundBpmWithinRange` with attribution; choose the
   downbeat nearest the start of the longest constant region as beat 1;
   fall back to `Dynamic` when no region covers ≥ 80% of beats; tests:
   synthetic beat lists with jitter, tempo change at 2:00, half/double
   tempo ambiguity (prefer 70–180 BPM range by genre-agnostic octave rule).
4. **Decoder-delay calibration**: export a reference set from rekordbox
   (same MP3/AAC/FLAC files) and compare rekordbox cue/grid times with ours;
   determine the per-codec offset convention (Mixxx saw 26/52 ms on MP3);
   encode the result in `reks-audio` and record it in D4.
5. **Key detection**: port libKeyFinder's pipeline (downsample, Blackman
   windowed chromagram across octaves, tone profiles, cosine similarity)
   on `rustfft`; tests: synthetic chord fixtures for all 24 keys; CI job
   that, when `libkeyfinder` is installed, compares against the C++ output
   on fixtures (must match ≥ 95%).
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
   `WaveformCalibration`; `PLACEHOLDER_CALIBRATION` is a first guess from
   two reference tracks for task 8 to fit. Section headers match rekordcrate's
   rekordbox 6 reference files byte for byte; every section parses with
   rekordcrate and writes back byte-identical.
8. **Calibration against rekordbox**: for 50 rekordbox-analysed tracks, load
   rekordbox's `.EXT/.2EX` from the local cache, fit per-band gain/gamma and
   the whiteness heuristic to minimise column error; store constants in
   `reks-analysis::calib`; produce side-by-side PNGs in `docs/analysis-parity/`.
9. **ANLZ builder** (`reks-anlz`): assemble `.DAT/.EXT/.2EX` with
   rekordcrate types; validator; golden test: build ANLZ for a reference
   track and compare section lists, lengths and PQTZ/PCOB contents with
   rekordbox's files (waveform bytes compared by error metric, not equality).
10. **Phase 2 integration**: `AnlzSource::Reks` in the planner; prefer reks
    analysis when present, rekordbox cache otherwise (user setting);
    `reks-cli analyse <path>` and `reks-cli anlz build`.
11. **Background queue**: analyse on import/scan with N parallel workers
    (rayon, N = cores − 1), progress channel, pause/cancel, re-analyse
    selection; store results; "needs analysis" filter goes away.
12. **Parity measurement**: run the whole imported library through reks;
    report BPM agreement (|Δ| ≤ 0.05), downbeat agreement (first beat within
    30 ms modulo bar), key agreement (exact, relative, neighbour), with
    per-genre breakdown; publish `docs/analysis-parity.md` with the method.
    Then tune and re-measure.
13. **Grid editor UI**: waveform view from our columns (zoomable, 150/s),
    beat markers, set-downbeat-here, nudge ±1 ms/±10 ms, tap tempo,
    double/halve, lock grid; edits write `beatgrid` rows with `source =
    manual` and are preserved on re-analysis.
14. **Hardware check**: export 20 reks-analysed tracks (no rekordbox data)
    and run the Phase 2 checklist on NXS2 and CDJ-3000; log.

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
