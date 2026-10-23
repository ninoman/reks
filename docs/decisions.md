# Architecture decisions

Each entry: decision, licence, rationale, rejected alternatives, open risks.
Status is `accepted` unless marked `proposed`. Evidence lives in
`docs/research.md` (sections referenced as R§n). Change a decision only by
editing this file in the same PR as the code change.

Date of decisions: 2026-10-18.

## D1. Application framework: Tauri 2 (Rust core + React/TypeScript/Vite UI)

- Licence: Tauri MIT/Apache-2.0; React MIT; Vite MIT.
- Rationale: fixed by the project brief. Rust owns all state, I/O, audio and
  export logic; the WebView is a thin UI. Current stable 2.12.x, MSRV 1.90
  (R§6). Tauri 3 is alpha; do not adopt until stable.
- Rejected: Electron (size, two runtimes, no Rust-first story); egui/iced
  (immature table/virtualisation UX for a 10k-row library).
- Risks: WebView differences per OS (mitigated by keeping media and files out
  of the WebView); IPC serialisation cost (mitigated by `ipc::Response` raw
  bytes and `Channel` for progress, R§6).

## D2. Business logic lives in independent crates, not in the Tauri shell

- Rationale: every crate (`reks-core`, `reks-audio`, `reks-analysis`,
  `reks-anlz`, `reks-export-*`) builds and tests headlessly; `reks-cli` wraps
  them for CI fixtures and hardware-test iteration without the GUI.
- Rejected: logic in `src-tauri` commands (untestable without the app).

## D3. Library database: SQLite via `rusqlite` (bundled) + `rusqlite_migration`

- Licence: rusqlite MIT; rusqlite_migration Apache-2.0; SQLite public domain.
- Rationale: backend-owned DB, synchronous API, bundled SQLite gives one
  version on all OSes; migrations via `user_version` (R§6). WAL mode, one
  writer connection behind a mutex, queries on `spawn_blocking`. FTS5 for search.
- Rejected: `tauri-plugin-sql` (sqlx-based, JS-facing, no Rust query API);
  `sqlx` (async overhead without benefit here); `diesel` (heavier, used by
  rbox/rekordcrate's OneLibrary branches but not needed for our own schema).
- Risks: none significant. Since Phase 4 the bundled SQLite is SQLCipher's
  (D14), one version on all OSes still.

## D4. Audio decoding: Symphonia 0.6

- Licence: MPL-2.0 (GPL-compatible via its secondary-licence clause).
- Rationale: pure Rust, excellent MP3/FLAC/WAV/AIFF/ALAC/Vorbis with gapless,
  AAC-LC good; same decoder for preview, analysis and export probes (R§6).
  Features enabled: `mp3`, `aac`, `alac`, `isomp4`, `aiff`, `flac`, `wav`,
  plus `pcm` (the PCM codec inside WAV/AIFF); default features off.
- Rejected: FFmpeg (`ffmpeg-next`): heavy multi-platform bundling,
  inconsistent crate licence metadata; WebView `<audio>`: codec set differs
  per OS and webkit2gtk needs host GStreamer plugins.
- Risks: no HE-AAC (rare in DJ libraries; show "unsupported codec"); MP3
  decoder-delay convention must be calibrated against rekordbox cue positions
  (R§3, R§4); measured below.
- Finding 2026-10-20 (phase 3 task 1): gapless decode trims the delay Symphonia
  reports (LAME MP3: 1105 frames), but Symphonia 0.6 reports none for ffmpeg
  AAC in MP4 (priming in the edit list), so such files decode 1024 frames late.
  Input to phase 3 task 4 (resolved in the next entry).
- Timing convention (2026-10-20, phase 3 task 4): **reks positions are
  rekordbox positions.** The decoders (`decode_f32`, `decode_stereo`) put
  sample 0 where rekordbox does, by prepending silence
  ([`reks_audio::timing`](../crates/reks-audio/src/timing.rs)); analysis,
  waveforms, preview and export inherit it, and cues/grids imported from
  rekordbox already use it. Rules, from the maintainer's rekordbox 6 library
  (method and numbers: [analysis-parity.md](analysis-parity.md), "Timing
  offset per codec"): an MP3 whose Info frame carries LAME's own tag (encoder
  `LAME…`, valid tag CRC) is delay + one frame later (1105 + 1152 samples,
  51.2 ms); an MP3 with another Info tag (ffmpeg's `Lavf`/`Lavc`, or a LAME
  tag with a bad CRC, n = 2) is the trimmed delay later (25.1 ms); MP3
  without an Info tag, FLAC, WAV, AIFF and ALAC as decoded. Symphonia's MP3
  decode equals ffmpeg's gapless decode sample for sample on every library
  MP3, so the offsets are rekordbox's convention, not a decoder error (Mixxx's
  26/52 ms, R§3). AAC in MP4: as decoded, **UNVERIFIED** — Symphonia applies
  neither the edit list nor `iTunSMPB`, and the one AAC track measured
  (iTunes, 2112 priming frames) sits near our decode, not near true gapless.
  `ANALYSER_VERSION` 2. Whether a CDJ's own MP3 decoder agrees with
  rekordbox's timeline is a hardware check (phase 3 checklist).
  Rejected: shifting only at export (preview and the grid editor would
  disagree with imported cues); decoding with gapless off (gives the delay
  but not LAME files' extra Info frame, which Symphonia never decodes).

## D5. Preview playback: our own `cpal` engine (`reks-mix`), decoding with Symphonia

- Date: 2026-10-19 (maintainer's call; replaces rodio 0.23 from 2026-10-18).
- Decision: the Phase 1 player is the first slice of the Phase 5 engine (D21):
  crate `reks-mix` on a raw `cpal` 0.18 stream, `rtrb` 0.4 queues for control,
  state and retired sources, an offline `render()` and an allocation guard
  test. One deck, play/pause/seek/volume (2026-10-21: the app's player is
  now deck 2 of the mixer, D30). The output opens at 44.1 kHz when the
  device offers it (most DJ music is 44.1 kHz, so those tracks skip the
  resampler; on macOS this sets the device clock, as rekordbox does), else at
  the device's default rate (2026-10-19). Tracks are decoded whole to stereo
  `i16` in memory by `reks-audio` and resampled to the output rate at load with `rubato` 5 (FFT, off the audio thread). Sync, tempo, EQ,
  filters and cue routing stay in Phase 5.
- Licence: cpal Apache-2.0; rtrb MIT/Apache-2.0; rubato MIT/Apache-2.0.
- Rationale: Phase 5 replaces any rodio player, so building on rodio first is
  throwaway work; the engine's real-time rules get tested in daily use before
  the mixer depends on them. Cost: we own resampling, device loss and the
  no-allocation rule that rodio would hide.
- Rejected: rodio (removed by Phase 5 anyway; no per-sample control); kira
  (game-oriented); web-audio-api crate (niche); WebView audio (see D4).
- Risks: Linux needs ALSA/Pulse/PipeWire client libs (standard); whole-track
  decode costs about 10 MB per minute at 44.1 kHz, so tracks over 60 minutes
  are refused for preview; a device-rate change needs a reload.

## D6. Tags and artwork: `lofty`

- Licence: MIT/Apache-2.0. Rationale: one crate for ID3v2/MP4/Vorbis/RIFF/
  AIFF tags and embedded pictures; actively maintained (0.25.x, 2026-09).
- Rejected: `id3` + `mp4ameta` + `metaflac` (three crates, inconsistent APIs).

## D7. rekordbox legacy USB export writer: `baken-export` behind our adapter

- Licence: MIT (deps verified: baken-core MIT, mp3rgain MIT, symphonia
  MPL-2.0, rusqlite MIT, image MIT/Apache, RustCrypto crates MIT/Apache,
  jpeg-encoder MIT/Apache AND IJG, see D18). Pinned `=4.6.1` (2026-10-19).
- Decision: Phase 2 writes `export.pdb` + ANLZ by (1) generating a rekordbox
  `collection.xml` from our library into a temp dir (we need the XML writer
  anyway, D11), (2) calling `baken_export::plan(&Options)` then
  `export()`, with `anlz_roots` pointing at rekordbox's local analysis cache
  for imported tracks and `prune` for stale-file cleanup. Our
  `reks-export-rb` crate wraps this behind a `RekordboxWriter` trait and
  adds our own dry-run presentation, FAT32 validation and read-back
  validation (D8).
- Rationale: it is the only open writer with dated real-player confirmations
  (CDJ-3000 3.22, CDJ-2000NXS2 1.85/1.87, CDJ-2000NXS 1.44), it already
  handles the known PDB breakers (string alignment, hot cues D–H, key names),
  copies or generates ANLZ, prunes, and is on crates.io under MIT (R§1). We
  get to a stick that plays in weeks instead of months and can test our
  library/import pipeline on real hardware early.
- Rejected:
  - Own PDB/ANLZ writer first: months of hardware-driven debugging that others
    have already done; revisit after Phase 3 if baken becomes a blocker.
  - rbxport `rbl-*` crates (GPL-2.0-or-later): licence fine, best written
    documentation, but git-path-only dependencies, 109 open issues, hardware
    validation mostly simulated, reported NXS/900NXS lockups. Keep as a
    documentation source (exportExt.pdb, `.sync`, DEVSETTING).
  - rekordcrate writer (MPL-2.0): still an open PR, no ANLZ generation.
  - rex, djbooth: no licence; never copy from them.
- Paths (2026-10-19, task 7): baken lays out `/Contents` itself (Unicode,
  48-character cut); our planner uses ASCII paths (D24). `BakenWriter`
  rewrites baken's public `Plan` between `plan()` and `export()` (tracks,
  `usb_path`, ANLZ slot and source), so the stick follows our plan; no fork.
  `baken-core` (same pin) is a direct dependency for `CancelToken`/`Progress`.
- Risks (open):
  - The `Plan` rewrite relies on baken's public fields and on `export` using
    `usb_path`/`anlz_dir`/`anlz` as it does in 4.6.1; the writer integration
    test is the check on every bump.
  - Input is an XML **file path**; we round-trip through XML (lossless for
    everything rekordbox XML carries; waveforms come from `anlz_roots` or
    generation, not XML).
  - Single maintainer, young crate (first published 2026-09-18), 47% docs.
    Mitigation: pin exact version, adapter trait, keep a fork-ready vendoring
    plan; contribute fixes upstream.
  - `baken-export` contains an obfuscated OneLibrary key in its `onelibrary`
    module. We never set `Options::onelibrary`. See D14.
  - ANLZ generated by baken for non-rekordbox tracks is "close approximation"
    for colour/3-band waveforms; Phase 3 replaces generation with ours.
- reks ANLZ (2026-10-20, phase 3 task 10; the default ordering is proposed,
  for the maintainer to confirm): tracks reks analysed get ANLZ built by
  `reks-anlz` in the writer's scratch folder, handed to baken as cache entries
  (`Entry.dat`), so baken rewrites `PPTH` and splices the XML cues as it does
  for rekordbox's files; no fork. When rekordbox and reks both have a track,
  the setting `export.anlz_preference` decides; default rekordbox first,
  because rekordbox's files are the ones proven on players (phase 3 risk
  mitigation). The phase doc's "prefer reks" is the `reksFirst` value;
  `rekordboxOnly` restores Phase 2 behaviour. Consequences: tracks only reks
  analysed now export with reks ANLZ by default instead of being skipped or
  generated (UNVERIFIED until phase 3 task 14). A track whose grid the user
  owns (locked or edited, `Library::grid_is_user_owned`) takes reks's
  analysis under `rekordboxFirst` too when reks has one, since rekordbox's
  cached `PQTZ` is copied as it is and the edit would be lost;
  `rekordboxOnly` still never writes reks's.
  A track mapped from rekordbox (D10, "rekordbox mapping") counts as reks
  analysis here: its reks-built set holds rekordbox's waveform bytes with
  the library's grid.
  - Phase 2's ANLZ for imported tracks depends on rekordbox's local cache
    existing on the machine; otherwise fall back to generation.

## D8. Export validation: parse what we wrote with `rekordcrate` 0.3.0; spec of record is crate-digger `.ksy` + djl-analysis

- Licence: rekordcrate MPL-2.0; crate-digger `.ksy` EPL-2.0 OR MPL-2.0 OR
  LGPL-3.0 with generated parsers covered (R§1).
- Rationale: an independent reader catches writer bugs; rekordcrate is the
  best-documented Rust parser and reads PDB, exportExt and ANLZ. Byte-level
  diffs against rekordbox reference exports in `fixtures/` complement it.
- Rejected: pyrekordbox in CI (Python toolchain in a Rust CI; optional later).
- Finding (2026-10-19, Phase 2 task 8): rekordcrate 0.3.0's ANLZ parser
  models cue type only as 0 (point) and 2 (loop) and fails on type 1, the
  point cue rekordbox and baken write (crate-digger: 1 = point), so it can't
  read a real cue list. The validator reads ANLZ with our own section
  reader (`reks-anlz::read`, crate-digger layout; PPTH, PQTZ, PCOB/PCO2
  counts) and keeps rekordcrate for `export.pdb`. Row fields rekordcrate
  keeps private are read from rows re-serialised by rekordcrate.

## D9. ANLZ serialisation (Phase 3): `rekordcrate::anlz` types (`BinWrite`) inside our `reks-anlz` crate

- Rationale: rekordcrate 0.3.0 already models PQTZ, PCOB/PCO2, PPTH, PVBR,
  PWAV, PWV2–7, PWVC, PSSI with `binrw` read+write (R§3). We produce section
  contents; it serialises. PPTH first; big-endian handled by the crate.
- Rejected: hand-written writer (duplicate effort); baken's generator
  (approximate waveforms, no API for our own analysis results).
- Risks: PQT2 not modelled (write only what rekordbox-less players need;
  verify on hardware whether PQT2 is required for NXS2 features); any
  rekordcrate API break (pin version); 0.3.0 can't represent cue type 1
  (point cues, see D8), so cue lists need a newer rekordcrate or our own
  encoding.
- Finding (2026-10-20, Phase 3 task 7; status: accepted 2026-10-20 by the maintainer): rekordcrate
  0.3.0 can't serialise our waveforms. Its waveform section structs keep the
  `unknown` header words private (not constructible outside the crate), it
  has no PWV6/PWV7/PWVC types (they parse as `Unknown`), and its PWV2 and
  PWV5 bitfields disagree with djl-analysis and rekordbox's files (height in
  the high nibble; red in the low bits). The waveform sections are therefore
  written by hand in `reks-anlz::write` (big-endian, explicit offsets,
  headers checked against rekordbox reference files), and rekordcrate stays
  the independent parser: every section must parse and write back
  byte-identical. Proposed: do the same for the remaining sections in task 9
  unless a newer rekordcrate fixes this.
- Finding (2026-10-20, Phase 3 task 9; status: accepted 2026-10-20 by the
  maintainer, follows the task 7 finding): 0.3.0 is still the latest rekordcrate (crates.io, 2026-10-20) and
  it can't write the other sections either: `Cue`/`ExtendedCue`/`BeatGrid`/
  `CueList` keep fields private, `CueType` has no point cue (1), and there is
  no PQT2 type. Its parser also can't read a `PCO2` with cues from rekordbox
  or from us: it expects a non-empty comment length and no 20 padding bytes
  after the colour. So every section is hand-written in `reks-anlz::write`;
  cue layouts follow baken-export 4.6.1 (MIT), whose tests pin them against
  real rekordbox 7 exports. Independent checks: rekordcrate round-trips every
  file without point cues byte-identically (loops included); cue-bearing
  `.EXT` files round-trip through our own reader (property test), and the
  golden test matches rekordbox 6's PPTH/PVBR/PCOB/PCO2 bytes. Revisit if a
  rekordcrate release makes these types constructible.

## D10. BPM/beatgrid analysis (Phase 3): Beat This! via the `beat-this` crate + a Rust port of Mixxx `BeatUtils` constant-grid logic

- Licence: beat-this crate MIT; Beat This! code and weights MIT; `rten`
  runtime MIT/Apache-2.0; Mixxx BeatUtils GPL-2.0-or-later (port lives in
  our GPLv3 crate, attribution kept).
- Rationale: best published accuracy, gives **downbeats** (needed for bar
  phase, which rekordbox users expect), pure Rust inference, ~10 MB small
  model bundle-friendly, verified on crates.io 2026-10-02 (R§3). A Mixxx-style
  constant-region pass turns the beat list into a single BPM (snapped to
  integer/0.5/0.01 within the 25 ms phase tolerance) anchored on a detected
  downbeat, matching rekordbox's constant-grid model; dynamic grids are kept
  as a full beat list where the tempo really varies.
- Rejected: qm-dsp TempoTrackV2 via FFI (no downbeats, C++ vendoring; keep as
  optional second opinion if disagreement is high); aubio (stale bindings);
  Essentia (AGPL, heavy build); madmom/BeatNet (non-commercial model licences,
  Python); `stratum-dsp` (unproven).
- Integration (2026-10-20): `beat-this` 1.1.0 pinned, no default features;
  small model + mel front end committed under `crates/reks-analysis/models/`
  (MIT, provenance and checksums there) and bundled as Tauri resources; full
  model is a later optional download. Beats are snapped to the nearest
  transient (the model ran 5–15 ms late on generated fixtures).
- Risks: ~real-time CPU cost on long tracks (batch with rayon, show
  progress); model provenance note in Beat This! README; agreement with
  rekordbox unknown until Phase 3 measures it on the maintainer's library.
- Port (2026-10-20): `retrieveConstRegions`, `makeConstBpm`,
  `roundBpmWithinRange` and `adjustPhase` from Mixxx `src/track/beatutils.cpp`
  live in [`reks-analysis/src/grid.rs`](../crates/reks-analysis/src/grid.rs)
  with the attribution in its module doc; tests reimplement the behaviour of
  Mixxx's `beatutils_test.cpp`, no code copied. Deviations are listed there.
- Queue (2026-10-20, phase 3 task 11): every track is analysed in the
  background (cores − 1 workers), but analysis only fills in. `beatgrid`
  rows are replaced only when all are `source = 'analysis'` (manual and
  rekordbox grids stay); `bpm_x100` follows a written grid unless
  `bpm_source = 'manual'`; `key` is written only when empty or from
  analysis (manual, tag and rekordbox keys stay). reks's own result is
  always in the analysis cache, so task 10 and the parity report (task 12)
  can use it without touching the library. Rationale: unmeasured analysis
  must not silently replace grids a DJ has played with. A locked grid
  (`track.grid_locked`, task 13) stays too.
- Queue order (2026-10-21): pending tracks with a rekordbox grid are
  analysed after the rest
  ([`reks-core/src/analysis.rs`](../crates/reks-core/src/analysis.rs)), so
  scanned tracks with no grid get one first.
- rekordbox mapping (2026-10-21; status: accepted, maintainer decision
  2026-10-21: "map rekordbox analysis into the reks cache instead of
  re-analysing"). This replaces "every track is analysed" above for tracks
  rekordbox analysed. Before queueing, the app (and `reks rb map`) runs
  [`rbmap`](../crates/reks-export-rb/src/rbmap.rs). A pending track with a
  rekordbox grid and a key from the XML import, whose `.DAT`/`.EXT`/`.2EX`
  rekordbox's local cache has (`rbcache`, the match the export uses) with
  every waveform section and a length within 1 s of the track's, gets a
  rekordbox entry in its cache folder: copies of those three files plus
  `rekordbox.json` (`source: "rekordbox"`, content hash, length). The track
  is then marked analysed (`track.analysis_source = 'rekordbox'`, migration
  0008, no analyser version and no confidences) and leaves the pending list
  until its file changes; analyser upgrades don't re-queue it. What comes
  from where:
  - **Mapped from rekordbox**:
    - grid, BPM, key and cues: the library's `source = 'rekordbox'` rows
      from the import. They are not copied into the cache, so the cache
      can't disagree with the library.
    - all waveforms, PWAV/PWV2–PWV7/PWVC: read by our own
      [`reks-anlz::rbwave`](../crates/reks-anlz/src/rbwave.rs), because
      rekordcrate can't read them (D9). Reading and writing them back is
      byte-identical on the rekordbox 6 reference files.
  - **Still computed by reks**: the deck and grid-editor waveform columns,
    on demand from the audio (`reks-analysis::columns`, decode + band
    filters only, no beat model), as before. Everything for tracks that
    don't qualify.
  - **Absent**: beat and key confidence.

  rekordbox's waveform bytes are not converted into reks's 150/s band
  columns. Our encoders (peak-hold envelope, per-track gain, gamma curves,
  3–7-bit fields) are not invertible, so columns rebuilt from them would
  only approximate rekordbox's bytes. Keeping the bytes is exact, and
  nothing that reads columns needs them from the cache. When reks builds
  ANLZ for a mapped track (`reksFirst`, or a grid the user edited or locked,
  D7), [`reksanlz`](../crates/reks-export-rb/src/reksanlz.rs) writes
  rekordbox's waveform payloads unchanged with the library's grid and cues.
  A reks analysis entry for the same file takes precedence (Re-analyse
  still runs the full analyser and sets `analysis_source = 'reks'`).
  Measured on a copy of the maintainer's library (2026-10-21, 14-core Mac,
  analyser version 4):
  - 2,284 of 2,292 candidates (273 h of audio) mapped in 10.2 s, using
    934 MB of cache. Full analysis of the same tracks is about 11 CPU-hours
    (15.3 CPU-s per 368 s track, 24× real time per core), so roughly an hour
    of wall time on this machine.
  - Left to the analyser: 3 tracks whose rekordbox PWV3 is one column
    shorter than its PWV5/PWV7, 2 whose rekordbox waveform is ~16 s shorter
    than the file, 2 grid mismatches, 1 missing file.
  - All of 200 random mapped tracks built and validated with
    `reks anlz build --library`.

  UNVERIFIED: reks-built ANLZ carrying rekordbox's waveforms has not been
  played on a CDJ (phase 3 task 14). Its sections are rekordbox's own bytes
  in the framing already checked against rekordbox's files.
- Grid positions (2026-10-20): every `beatgrid.position_ms` is written as
  whole ms (XML import, analysis, grid editor), so the Rust types are `i64`.
  The column stays `REAL` (no migration; whole numbers are exact in it).
  Rationale: every consumer is whole ms (PQTZ `u32` ms; XML `Inizio` has 3
  decimals), and rounding one anchor costs ≤ 0.5 ms without accumulating;
  the sample-accurate grid stays in the analysis cache.
- Grid tuning (2026-10-20, phase 3 task 12a, analyser version 3): the
  constant tempo is a whole-track search (`fit_tempo`, 0.01 BPM steps
  around Mixxx's estimate, most beats within 15 ms); Mixxx's region pass
  still gives the phase and the downbeat anchor. A grid is dynamic only
  when under 30% of beats fit a constant one or a ≥ 32-beat region holds
  another tempo. Beats stay on the broadband onset; a low-band kick curve
  chooses the grid's phase among clusters of beats (`kick_phase`).
  Rationale: measured on the maintainer's library, BPM 85.1% → 96.7%,
  downbeat 70.9% → 73.3%; placing beats on the kick lowered both
  (docs/analysis-parity.md).
- Bar placement (2026-10-21, phase 3 task 12a, analyser version 4): beat 1
  is no longer the tracker downbeat nearest the steady region's start (a
  coin toss on many tracks). Each bar position of the constant grid scores
  its share of all tracker downbeats, plus 0.3 if the first grid line with
  a ≥ 10 dB low-band rise (the first kick) is on it; the old rule breaks
  ties ([`bar.rs`](../crates/reks-analysis/src/bar.rs)). Rationale:
  measured on the maintainer's library, downbeat 73.3% → 83.7% with BPM
  and grid phase unchanged; phrase cues from the kick curve (re-entry
  after a breakdown, per-position accent) added nothing, and the full
  Beat This! model stays a later option rather than a requirement.

## D11. rekordbox XML reader/writer: own implementation on `quick-xml`

- Licence: quick-xml MIT. Rationale: streaming parse of a 3,000+ track file,
  exact attribute control for round-trips and for feeding baken (D7).
- Rejected: rekordcrate's XML (open PRs), rbox (GPL-3.0-only, repo 404).

## D12. Key detection (Phase 3): Rust port of libKeyFinder's pipeline on `rustfft`, validated against the C++ library

- Licence: libKeyFinder GPL-3.0-or-later (port stays GPL); rustfft MIT/Apache.
- Rationale: Mixxx-maintained algorithm with wide DJ use; `libkeyfinder-sys`
  needs a system library via pkg-config and fails on docs.rs, which is not
  shippable in a desktop bundle (R§3). A port removes FFTW and C++ from the
  build. CI validates our output against the C++ library on fixtures where
  the system lib is available.
- Rejected: qm-dsp key (second opinion only); Essentia `edma` profiles (best
  EDM profiles in the literature but AGPL and the profile vectors are
  Essentia source); shipping `libkeyfinder-sys` (build fragility).
- Risks: port effort "weekend-scale" is unverified; key tools disagree with
  each other often, so key is advisory with a confidence value shown.
- Implemented (phase 3 task 5): `crates/reks-analysis/src/key.rs`, ported
  from mixxxdj/libkeyfinder `main` (2024-11; credit in the module docs).
  Same constants (decimate to ~4.4 kHz, 16384/4096 Blackman FFT, 72-band
  direct spectral kernel, Sha'ath profiles × octave weights, whole-track
  mean); key type is `reks_core::Key`. Additions: an uncalibrated
  `confidence` (winner's margin over runner-up) and an error on NaN input.
  Agreement with the C++ library: verified 2026-10-21, identical keys on
  the 26 synthetic fixtures and on all 2,644 decoded tracks of the
  maintainer's library (`reks parity keyfinder`, libKeyFinder `main`
  941e517 run as a separate program; docs/analysis-parity.md).
- Mode (2026-10-21, phase 3 task 12a): reks keeps libKeyFinder's
  major/minor decision (best cosine over all 24 profiles), no minor bias or
  second-stage mode classifier. Rationale: against rekordbox on the
  maintainer's library, the best rule (+0.01 on minor scores) gains 2.4
  points exact (62.2% → 64.6%; holdout 65.1% → 67.3%) only by calling 97.6%
  of tracks minor, which drops rekordbox-major tracks from 24% to 6% exact;
  a classifier trained on rekordbox's labels collapses to "always minor".
  That fits this library's 80% minor share, not music in general, and a
  parallel-mode miss is three Camelot steps off. Rejected: minor bias,
  parallel-switch threshold, segment voting, energy-weighted frames (all
  measured in docs/analysis-parity.md). Revisit if tonic work (the larger
  error, 15.6% wrong tonic) changes the chroma.

## D13. Waveforms (Phase 3): own Rust generator at 150 columns/s, calibrated against rekordbox output

- Rationale: PWAV/PWV2/PWV3/PWV4/PWV5/PWV6/PWV7 encodings are documented
  (R§3) but rekordbox's band filters and scaling are not. We compute three
  bands (start from Mixxx's Bessel-4 at 600 Hz / 4 kHz) with per-column peak,
  then fit per-band gain/gamma to rekordbox's analysis of the same tracks
  (the maintainer has ~3,000 rekordbox-analysed tracks; rekordcrate ships a
  sweep fixture). Acceptance is visual parity on the CDJ, not byte equality.
- Rejected: copying baken's generator (MIT, allowed, but approximate and not
  structured for our pipeline; borrow ideas with attribution if useful).
- Risks: PWV4 unknown bytes, PWVC semantics, 3-band non-linear scaling.
- Calibration (2026-10-20, phase 3 task 8): fitted against rekordbox 6's
  own files for 50 tracks; constants in `reks-analysis::calib`, method and
  error in [`analysis-parity/waveforms.md`](analysis-parity/waveforms.md).
  Format findings from that data, applied in `reks-anlz::waveform`:
  PWV6/PWV7 hold **low, mid, high** (djl-analysis says mid, high, low; band
  energy correlation says otherwise, 0.65–0.93 on the diagonal); PWVC is the
  per-track gain per band ×100 that rekordbox applied to PWV6/PWV7 (0.99
  correlation with the fitted gain for low and mid), so we compute and write
  ours instead of a constant; PWV5 colour is normalised to the loudest
  band per column. Crossovers stay at 600 Hz / 4 kHz: PWV4 bytes 2–5 follow
  those bands closely, while PWV7 would prefer about 250 Hz / 2 kHz (an
  estimated 20% lower PWV7 error; follow-up, needs two band sets).
  Everything here is visual parity on files, **UNVERIFIED** on a player.

## D14. OneLibrary (Phase 4): own writer, experimental flag, no key in the repository

- Licence: rusqlite `bundled-sqlcipher` (SQLCipher 4.14.0, BSD-3-Clause,
  bundled in `libsqlite3-sys` 0.38). Crypto: CommonCrypto on Apple targets;
  elsewhere `bundled-sqlcipher-vendored-openssl` builds OpenSSL 3 from source
  (`openssl-src`, MIT/Apache-2.0; OpenSSL 3 is Apache-2.0, GPLv3-compatible),
  because Windows has no system libcrypto and a vendored one avoids a
  distro-specific `libcrypto.so` in the Linux bundles. Chosen 2026-10-20
  (Phase 4 task 2) over an own page cipher: SQLCipher is the format's own
  implementation.
- SQLCipher next to the library DB: Cargo unifies features, so any build that
  includes `reks-export-onelibrary` (the app, `cargo test --workspace`) links
  SQLCipher's SQLite (3.51.3) instead of the plain bundled one (3.53.2) for
  every crate, `reks-core` included. A connection without `PRAGMA key` is a
  plain, unencrypted SQLite database, so the library file format is
  unchanged. Checked 2026-10-20: all `reks-core` tests (migrations, WAL, FTS5)
  pass on the SQLCipher build. Consequence: the app's SQLite version is
  SQLCipher's, which trails upstream by a release or two; do not use SQLite
  features newer than SQLCipher's bundled version in `reks-core`.
- Rationale and policy: see `docs/phases/phase-4.md`. Originally the key was
  user- or environment-supplied at runtime only, mirroring libdjinterop and
  Mixxx, because the legal position is untested and the format is in flux
  (R§2). Feature off by default until a hardware success is logged.
- Amended 2026-10-21 (maintainer decision): release builds have the key built
  in, because asking users to find it was too unfriendly. It is still never in
  the repository: CI injects it from a secret at compile time and the binary
  holds it masked (`reks_core::secret::builtin`). Why the risk is acceptable:
  the key is public and shipped by pyrekordbox, baken and onelibrary-connect
  with no takedowns found (R§2); the only use is writing the user's own
  library to their own stick for players to read, which is the case DMCA
  §1201(f) exempts; AlphaTheta now sanctions third-party writers. Keeping it
  out of git keeps the repository, the likeliest takedown target, free of
  it. Users never enter a key (the Settings key field is gone);
  `REKS_ONELIBRARY_KEY` overrides the built-in one for CI and builds from
  source, and if rekordbox changes the passphrase, a new release ships it.
  Rejected: committing it obfuscated (as baken does; puts the repository at
  risk) and extracting it from a local rekordbox (does not help users
  without rekordbox).
- Rejected: enabling `baken-export`'s `onelibrary` option (embeds the key,
  untested on players); rbox (GPL-3.0-only is compatible but the repo is
  unreachable and docs.rs fails).
- Risks: hardware rejection, cipher parameters (resolved for 7.2.19), spec churn, legal.
- Planner seam (2026-10-20, Phase 4 task 8): the trait `OneLibraryExport`
  lives in `reks-export-rb` (the writer crate needs `ExportPlan`, so the
  dependency can only go writer → reks-export-rb), and the app injects the
  implementation. The key is a `reks_core::OneLibraryKey` (`zeroize`,
  Apache-2.0 OR MIT): never serialised, never logged. (Until 2026-10-21 it
  could also be pasted in Settings and stored in the library database; an
  old `onelibrary.key` row is now ignored.)
- Apply seam (2026-10-20): the trait also has `write(plan, key,
  device_root)`, which `apply` calls after baken has written `export.pdb`
  and removed rekordbox's OneLibrary files, the order baken uses for its own
  OneLibrary file. The key reaches `apply` in `ApplyEnv` (`OneLibraryEnv`),
  not in the serialisable plan. The writer writes the file in place
  atomically itself (task 6), like `RekordboxWriter`; `apply` stays the only
  caller, so every stick write still happens inside `apply()`. The writer
  (tasks 4–6) implements this shape as is; the app has no stub any more.
- Cipher profile (2026-10-20, task 2): rekordbox 7.2.19's `exportLibrary.db`
  opens with stock SQLCipher 4 (AES-256-CBC, PBKDF2-HMAC-SHA512 256 000
  rounds, HMAC-SHA512, 4096-byte pages), the key used as a passphrase
  (`PRAGMA key`, `cipher_compatibility = 4`). The DjManager #300 report that
  standard profiles fail does not hold for 7.2.19. The key is not all hex,
  so a raw-key reading is ruled out.
- Writer source (2026-10-20, tasks 4–6): rows are built from the stick's
  `export.pdb` after baken wrote it, not from the plan or the library. On
  the reference every `content` value equals the PDB track row's (ids,
  `masterDbId`, `masterContentId`, `contentLink`, `analysedBits` included),
  so mirroring the PDB makes the two libraries agree by construction and
  needs no library access in `write`. `reks-export-rb::pdbread` is the typed
  PDB reader. `myTag` gets the four default columns, not the reference's
  (its user's) tags; `myTagMasterDBID` 0. The file is written in WAL mode
  and checkpointed into one file (rekordbox leaves a `-wal`); UNVERIFIED on
  players.

## D15. Engine DJ (Phase 6): own Rust writer targeting schema 3.0.2 (and 2.21.0), no FFI

- Licence: rusqlite MIT, flate2 MIT/Apache. libdjinterop (LGPL-3.0) used as
  documentation only.
- Rationale: plain SQLite + documented blobs; every non-Mixxx tool (rb2engine,
  PenBridge) wrote its own; avoids a C++ build and libdjinterop's lag (R§5).
- Rejected: libdjinterop via cxx (LGPL fine, build burden, refuses newer
  schemas); `endjine` (experimental, sqlx).
- Risks: schema churn; `hm.db` requirement unverified; album-art hash unknown.

## D16. Removable media: Rust `std::fs` only; FAT32 + MBR default; exFAT behind a per-player warning

- Rationale: fs-plugin scopes apply only to JS; exports are backend-only
  (R§6). FAT32 is the only filesystem every player accepts; exFAT only on
  CDJ-3000/XDJ-XZ/RX3 and newer (R§4). Volume discovery via `sysinfo::Disks`;
  eject via `diskutil` (macOS) / Windows IOCTL; `NSRemovableVolumesUsageDescription`
  in `Info.plist`. Every write goes through plan → dry-run → apply (CLAUDE.md).
- Rejected: `tauri-plugin-fs` for exports (scope friction, no benefit);
  formatting sticks ourselves (out of scope; show instructions).
- Risks: GPT vs MBR unverified officially; NFC/NFD on players unverified.

## D17. Distribution and signing (outside app stores)

- macOS: Apple Developer Program ($99/yr), Developer ID Application
  certificate, notarization through Tauri's bundler env vars; hardened
  runtime default; no extra entitlements for audio output (R§6). Unsigned
  builds only for developers.
- Windows: apply to SignPath Foundation (free for OSS, CI-built binaries,
  policy text on the project page); fallback Azure Artifact Signing (pricing
  and eligibility unverified; individuals only in US/Canada). Wire via
  `bundle.windows.signCommand`.
- Linux: AppImage (auto-updatable) + deb; build on Ubuntu 22.04.
- Updates: `tauri-plugin-updater` with a minisign keypair, `latest.json` on
  GitHub Releases, `tauri-action@v1` CI matrix (macOS arm64 + x86_64 built
  separately, Windows, Ubuntu 22.04).
- Rejected: universal macOS binary (updater key mismatch, not in Tauri's
  official matrix); EV certificate (no instant SmartScreen reputation since 2024).
- Risks: signing costs are recurring; donation funding must cover them.

## D18. Licence compliance gate: `cargo deny` (Rust) and `license-checker` (npm) in CI

- Allowlist: MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Zlib, MPL-2.0,
  LGPL-2.1-or-later, LGPL-3.0, GPL-2.0-or-later, GPL-3.0, GPL-3.0-or-later,
  CC0-1.0, BSL-1.0, Unicode-DFS-2016, Unicode-3.0, 0BSD, OpenSSL (only if
  pulled transitively; prefer rustls). Deny: GPL-2.0-only, EPL-1.0, CDDL,
  AGPL-3.0 (requires a recorded decision), SSPL, BUSL, unlicensed.
- Rationale: CLAUDE.md licence rules must be machine-enforced.
- 2026-10-19: `IJG` (Independent JPEG Group License) added for `jpeg-encoder`
  0.7.1 (`(MIT OR Apache-2.0) AND IJG`), pulled in by `baken-export` (D7) for
  artwork. FSF lists it as free and GPL-compatible
  (https://www.gnu.org/licenses/license-list.html#ijg). It asks binary
  distributions to say in their documentation that the software is based in
  part on the work of the Independent JPEG Group: release notices must carry
  that line.
- 2026-10-19: `cargo deny check advisories` ignores four advisories, each with
  a reason in `deny.toml`. RUSTSEC-2026-0194/0195 (`quick-xml` 0.37.5 denial of
  service on crafted XML) come only through `rekordcrate` 0.3.0's XML module;
  reks uses `rekordcrate` for PDB/ANLZ read-back and parses rekordbox XML with
  its own code, so the vulnerable path never runs. RUSTSEC-2024-0436 (`paste`,
  via `lofty`) and RUSTSEC-2024-0370 (`proc-macro-error`, via Tauri's Linux GTK
  crates) are unmaintained build-time macro crates with no replacement release.
  Remove each ignore when `rekordcrate`, `lofty` or Tauri moves off it; a new
  advisory still fails the check.
- 2026-10-23: `cargo deny` checks only the four release targets (`[graph]
  targets` in `deny.toml`). Dependencies for other targets never reach a
  binary. The case that prompted it: `rustls-platform-verifier` (D34) pulls
  `webpki-root-certs` (CDLA-Permissive-2.0) on wasm32 only. Add a target
  there when the release matrix gains one.

## D19. Naming and identifiers

- Project name `reks` (placeholder; rename before 1.0). Crate prefix
  `reks-`. Bundle identifier `org.reks.app` (placeholder). Library DB file
  `reks.sqlite` in `app_data_dir` (`app_local_data_dir` on Windows); caches
  in `app_cache_dir`.

## D20. UI lint and format: Biome

- Licence: `@biomejs/biome` MIT OR Apache-2.0 (dev dependency only).
- Date: 2026-10-19 (maintainer's call). Replaces ESLint (typescript-eslint strict, React
  hooks/refresh plugins) and Prettier.
- Rationale: one fast tool, one config ([`biome.jsonc`](../biome.jsonc)), no plugin stack.
  The recommended rules plus the React domain cover hook dependencies and hook placement.
  Formatting matches the old Prettier settings (100 columns, double quotes, trailing commas).
- `react-refresh/only-export-components` maps to Biome's `useComponentExportOnlyModules`.
  Lost: typescript-eslint's `strict` set, which Biome only partly covers. `tsc -b` in
  `npm run build` still type-checks strictly.
- `TrackTable.tsx` turns off three a11y rules that don't fit its virtualised ARIA grid
  of divs; the reason is in `biome.jsonc`.

## D21. Mix-prep mixer (Phase 5): 2–4 synced decks through a Xone:92-style mixer on our own cpal engine

- Date: 2026-10-19 (maintainer's call). Plan: [`phases/phase-5.md`](phases/phase-5.md).
- Decision: a preparation tool, not a performance app. While building a playlist,
  load 2–4 tracks on decks, beat-sync them from their grids, and try a transition
  or a techno layer through a model of the Allen & Heath Xone:92 music section:
  4-band asymmetric EQ with HI/LO kill, two state-variable filters (no LFOs),
  X/Y crossfader with curve, headphone cue (R§9.1). New crate `reks-mix` owns a
  raw `cpal` stream with `rtrb` queues for control and state. Tempo is varispeed
  through `rubato` (already in the tree via `beat-this`, D10). The engine's
  first slice ships in Phase 1 as the preview player (D5), so one engine owns
  the audio device from the start. Since D30 the library's player is a deck
  of this engine.
- Order: Phase 5, after OneLibrary (Phase 4) and before Engine DJ export (now
  Phase 6). It depends only on Phase 1 (imported grids, decode) and Phase 3
  (waveform columns, grid editor). Changed 2026-10-20 (maintainer's call): it
  was Phase 6, after Engine DJ.
- Filter interaction: clarified 2026-10-21 (maintainer's call; the model is
  unchanged). FREQ stays the Xone:92 law (LP closes to the left, HP closes to
  the right). In the UI, a click on HP / BP / LP picks that mode alone and
  Shift+click combines modes (a unit always keeps one); a click on a channel
  CUE cues that channel alone (or clears it) and Shift+click adds or removes
  it. Channels 1–2 default to filter X (left), 3–4 to Y (right).
- Sync master, beat jump and beat loops: added 2026-10-22 (maintainer's
  review, phase 5 task 13). The sync leader becomes a CDJ-style master
  role that passes to a playing synced deck at its matched tempo when the
  master stops, ends or is unloaded (was: recomputed every block, so a
  follower fell back to its own tempo). Beat jump and beat loops are deck
  controls DJs use to audition a transition, so they count as preparation,
  not the live-performance scope rejected below; loop rolls, slip mode and
  saved loops still need a decision.
- EQ: the Xone:96 channel EQ since 2026-10-22 (maintainer's call; the rest
  of the model stays the Xone:92's). HI 3 kHz and LO 180 Hz shelves, +6 dB /
  kill; HI MID 1.1 kHz and LO MID 350 Hz bells, +10 / −27 dB (R§9.1). The mid
  cut is a notch (Q 2), not the Xone:92 guide's "low Q", which at full cut
  sounded like a kill. Was: the Xone:92's 2.5 kHz / 2 kHz / 350 Hz / 250 Hz,
  mids +6 / −30 dB.
- Manual loops, reloop and an always-visible loop block (status:
  **accepted** 2026-10-23, maintainer's call; proposed 2026-10-22). Add
  loop IN / OUT (also without a grid), RELOOP/EXIT with the last loop
  kept, ½ / 2× and a loop size readout on every deck, so a loop can be set
  or changed in any pad mode. Engine: phase 5 task 20 (2026-10-23). Rationale: same as beat loops, auditioning a transition or a
  layer; every reference product keeps these on the deck (rekordbox,
  Traktor, Engine, CDJ-3000). Still out: slip, loop roll, emergency loop.
  Spec and sources: [`research/deck-mixer-layout.md`](research/deck-mixer-layout.md) §5;
  phase 5 tasks 20–21.
- Simple and advanced mixer views (maintainer's request, 2026-10-23; spec
  **accepted**, maintainer's call, 2026-10-23): a Settings → Mixer group, also opened from a gear on the
  mixer, chooses Simple (no filter assign, crossfader or crossfader assign;
  filters optionally hidden) or Advanced (the whole model). Hidden controls
  hold values that change nothing (default filter assigns, crossfader
  assigns THRU, hidden filters off), and the user's positions come back in
  Advanced. UI only; the DSP model above is unchanged. Spec:
  [`research/deck-mixer-layout.md`](research/deck-mixer-layout.md) §6;
  phase 5 tasks 24–25.
- 3-band EQ layout (issue #56; status: **accepted** 2026-10-23, maintainer's
  go-ahead to implement; phase 5 task 27): an EQ
  setting, 4-band · 3-band, beside Mixer view Simple · Advanced (which
  applies to both). 3-band has a 3-band EQ (−26 / +6 dB) and, per channel,
  an FX knob with a mixer-wide effect select (FILTER, ECHO, REVERB) and
  PARAMETER knob, as on DJM mixers, in place of the X/Y filters. Echo and reverb join the model on the maintainer's call
  (2026-10-23) as transition tools for auditioning exits; tempo-synced
  BEAT FX stay out. New blocks only; the Xone model above is unchanged.
  Spec: [`research/djm-eq-fx.md`](research/djm-eq-fx.md).
- Filter LFOs: dropped 2026-10-20 (maintainer's call). The Xone:92 has one
  per filter (R§9.1); reks models the filters without them. Adding them back
  needs a new decision.
- Licence: cpal Apache-2.0; rtrb MIT/Apache-2.0; rubato MIT/Apache-2.0 (R§9.2).
  EQ, filter and fader laws are our own code from published designs;
  Mixxx (GPL-2.0-or-later) may be ported with attribution, as in D10.
- Rationale: auditioning a transition while preparing a set is library work
  (rekordbox ships it in the same app), it runs on the grids we already import,
  and it adds no file formats. Copying one well-documented mixer gives a fixed
  spec to test against instead of a mixer designed by us.
- Key lock: not in the first cut; pitch follows tempo. Candidates (R§9.2):
  `timestretch` (pure Rust, MIT, real-time engine, young), `signalsmith-stretch`
  (MIT, C++ built with `cc`), `rubberband` (GPL-2.0-or-later, crate weeks old),
  `soundtouch` (SPDX `LGPL-2.1`, needs a `deny.toml` note). Choose by listening test.
  Status 2026-10-21: licences and cost checked (R§9.3), `reks keylock-render`
  ready; the choice is pending the maintainer's listening (phase 5 task 10).
- Master tempo (status: **proposed** 2026-10-23, pending task 10's
  listening): the deck's **M** turns on a key lock head built on
  `timestretch` 0.15's real-time `Keylock` profile (two-band SOLA). Why this
  one first: already in the tree and licence-checked (MIT, `cargo deny`
  passes), pure Rust (no C++ toolchain or libclang on any platform), lowest
  latency (≈ 12.7 ms) and no allocation measured in the callback (R§9.3).
  It sits behind one module, [`keylock.rs`](../crates/reks-mix/src/keylock.rs),
  so another engine is a local swap if listening prefers it. A jump primes
  the engine synchronously (reset, preroll, output dropped until the target
  leaves it), so the deck needs no latency compensation; leaving key lock
  bakes the crossfade's fade-out from the engine, since its output is not the
  source at the head. Limits: used where the step is 0.25–2 and the source
  is at the engine's rate (the app resamples at load, D5), else varispeed;
  the profile fades to varispeed beyond ±35% (WIDE). Each jump with M on
  costs one priming run on the audio thread, so tiny loops cost more CPU.
- Rejected: live-performance scope (MIDI mapping, recording, stage reliability;
  a second product, needs its own decision); embedding Mixxx (C++/Qt
  application, not a library); WebView audio (D4); rodio for the mixer (no
  per-sample control or 4-channel cue routing).
- Risks: real-time glitches (no allocation, locks or I/O in the callback;
  offline render tests; soak test); sync is only as good as the grid; the model
  follows the original user guide (AP5345 Issue 1), Mk2 differences
  UNVERIFIED; headphone cue needs a 4-output interface, otherwise split cue;
  Windows ASIO needs Steinberg's SDK, so WASAPI only. "Xone" and "Allen &
  Heath" are trademarks: docs name them as the reference, UI strings do not.
- Varispeed (2026-10-21, phase 5 task 2): `rubato` 5 `Async` sinc resampler
  per deck (128 taps, Blackman-Harris, linear sub-sample interpolation,
  32-frame output chunks), built once per deck slot when the graph is built,
  so loading allocates nothing on the audio thread. At exactly unit step
  (speed 1.0, source at the engine rate) the deck copies frames instead, so
  untouched playback stays bit-exact. The filter is designed for ratio 1, so a
  source far from the engine rate played fast aliases a little: the app keeps
  resampling to the engine rate at load (D5) and `DeckSource` takes any rate
  only as a fallback.

## D22. Playlist dividers: playlist entries in the library, silent WAV tracks on the stick

- Date: 2026-10-19 (maintainer's call). Plan: [`phases/phase-2.5.md`](phases/phase-2.5.md),
  a new phase after Phase 2 and before Phase 3 (a half step, so later phases keep
  their numbers).
- Decision: a divider is a `playlist_track` entry with `item_kind = 'divider'` and
  a label, never a `track` row, so search, counts, filters and XML export stay
  clean. On export it becomes a real track: a silent PCM WAV we generate (no
  copied audio) with the label as title, analysed by baken's `generate_analysis`,
  which only touches tracks rekordbox never analysed (R§10). One file per divider
  until hardware shows rows can share one. Behind `export.dividers` until the
  Phase 2.5 hardware checklist passes on CDJ-2000NXS2 and CDJ-3000.
- Rationale: DJs already do this by hand with silent files in rekordbox. A PDB
  playlist entry must point at a track row, and a row whose file is missing or
  invalid is the kind of thing that breaks a stick; a loadable silent file is
  the safe form.
- Rejected: PDB rows without a file (unsafe, untested); dividers as library
  tracks (pollute search, counts and XML); a reks-owned folder on the stick
  (baken lays out `/Contents/<Artist>/<Album>/` itself, R§10).
- Risks: dividers show in All Tracks, search and player history; label length
  and glyphs per player; baken's analysis of a silent file with no TEMPO.

## D23. Play history: imported sessions, derived play counts, and no export ever loses player history

- Date: 2026-10-19 (maintainer's call). Safeguard: Phase 2 tasks 12–13. Import
  from legacy sticks (H1): Phase 2.5. OneLibrary (H2): Phase 4 task 11. Engine
  (H3): Phase 6 task 11.
- Decision:
  - Play count and last played are derived: a frozen baseline copied from
    rekordbox's `collection.xml` values, plus the entries of included sessions.
    Sessions are imported read-only from sticks or from Phase 2 backups. A
    session can be marked practice (excluded) and back. Every session
    imports counted, whatever its date; the user marks practice afterwards
    (maintainer's call, 2026-10-20; undated and pre-baseline sessions used
    to import excluded). reks preview and mixer playback never count.
  - Every writer checks what history the stick holds before writing. Phase 2
    backs up `PIONEER/rekordbox/*.pdb` and `exportLibrary.db` to
    `app_data_dir/stick-backups/` by default (data dir, not cache dir: it can
    be the only copy) and discards only on explicit confirm. From Phase 2.5 an
    export stops on sessions not yet imported.
  - Entries match library tracks through `export_manifest`, keyed by (volume
    serial, stick path) and filled from the validator's read-back, so PDB
    track IDs need not stay stable; baken doesn't promise that.
  - H2 (2026-10-21): a stick's two libraries are imported side by side.
    Each session records its library (`play_session.source`, part of the
    fingerprint), so sessions in `exportLibrary.db` never skip or extend
    `export.pdb`'s; OneLibrary content paths are the PDB's, so the same
    manifest matches them. Import now imports both before the backup.
- Rationale: baken rebuilds `export.pdb` on every run and removes
  `exportLibrary.db` unless OneLibrary is on (R§10), so without this every
  re-export erases the history the players wrote. Derived counts (as in Mixxx)
  are explainable and recomputable; a stored counter can't undo a practice
  session.
- Rejected: an incremented stored counter; importing rekordbox's own histories
  (not in XML; `master.db` is out of scope); writing history back to sticks
  (player-owned data); backups in `app_cache_dir` (D19; the OS may clear it).
- Risks: history format unknowns (date table, appends across mounts, what a
  player logs as a play) and volume serial access per OS; see R§8 item 14.
  OneLibrary: how players fill `history`, and whether one that reads both
  libraries logs the same plays in both (they would count twice until one
  session is marked practice).

## D24. Stick paths: ASCII transliteration, metadata stays Unicode

- Date: 2026-10-19 (Phase 2 task 2). Code: `reks-core::fsname`.
- Decision: every path reks writes under `/Contents` is ASCII. Names are NFC
  normalised, then transliterated with `deunicode` (BSD-3-Clause): `Кино` →
  `Kino`, `Sigur Rós` → `Sigur Ros`, kana → romaji, Han → Mandarin readings
  (`坂本龍一` → `Ban Ben Long Yi`), emoji → words (`🔥` → `fire`). Titles,
  artists and albums in `export.pdb`/OneLibrary/Engine keep their Unicode, so
  browse and search are unaffected; only the file name is folded. Collisions
  that folding creates are resolved by `fsname::dedupe` (`-1`, `-2`).
- Rationale: CLAUDE.md requires ASCII-safe paths; NFC/NFD and case handling
  on players have no Pioneer source (R§4, R§8 item 4) and a non-ASCII artist
  froze an NXS2 in baken before #168 (R§1). ASCII makes the PPTH/`file_path`
  string identical however the OS or player normalises, and makes character
  and byte limits the same.
- Rejected: writing NFC Unicode paths (what rekordbox does; unverified on
  older players); `_` for every non-ASCII character (unreadable folders,
  more collisions); hashing names.
- Risks: Han readings are Mandarin, odd for Japanese users browsing by
  folder (players browse by metadata, so rarely seen). Revisit if a hardware
  test shows Unicode paths are safe on all target players.

## D25. File converter: lossless sources to WAV / AIFF / FLAC, own PCM writers, `flacenc`

- Date: 2026-10-19 (maintainer's request). Plan: [`phases/phase-2.6.md`](phases/phase-2.6.md),
  after Phase 2, independent of Phase 2.5.
- Decision: convert selected tracks to WAV, AIFF or FLAC with a few settings
  (bit depth, sample rate, destination, replace or add) and two presets, Older
  players and Save space. Sources must be lossless (FLAC, ALAC, WAV, AIFF). The
  converted file takes over the library track; the original is kept unless the
  user moves it to the Trash. Plan → dry run → apply, as for exports.
- Licence: `flacenc` 0.5 Apache-2.0 (pure Rust, R§11); `trash` for the optional
  Trash move (licence to check in task 5); rubato, lofty, Symphonia already in the tree.
- Rationale: FLAC/ALAC, rates above 48 kHz, float and `WAVE_FORMAT_EXTENSIBLE`
  WAVs don't load on CDJ-2000NXS and older (R§4); Lexicon and rekordbox-edit
  fix this by converting (R§11). Cues and grids live in our DB in ms, so a
  lossless conversion keeps them valid.
- Own WAV and AIFF writers: `hound` writes `WAVE_FORMAT_EXTENSIBLE` for anything
  above 16 bits (hound 3.5.1 `write.rs`), the E-8305 breaker (R§4).
- Rejected: lossy sources (no quality gain, and MP3 decoder-delay handling would
  shift cues, D4); lossy targets (generation loss, and all players already
  play MP3/AAC); FFmpeg (D4); `libflac-sys` (C build for no gain); converting
  during export (a transcoder in the export path; needs its own decision).
- Risks: rekordbox ANLZ reuse after Replace (CONV1), ID3 chunks in WAV (CONV2),
  mono on players (CONV3), all **UNVERIFIED** until the Phase 2.6 hardware session.
- 2026-10-19 (tasks 1–3): `flacenc` 0.5.1 with default features off (no worker
  threads, no serde). Its new crates (`crc`, `crc-catalog`, `heapless`,
  `hash32`, `md-5`, `seq-macro`, build-time `built`) are MIT or Apache-2.0, so
  the `cargo deny` allowlist is unchanged. The writer streams frame by frame
  with `encode_fixed_size_frame`, because flacenc's one-call API keeps the whole
  encoded stream in memory. Dither: TPDF only when going to 16 bits. Going to
  24 bits (32-bit and float sources, resampled audio) rounds, since that error
  is about 144 dB below full scale. The dither generator is seeded, so the same
  input always gives the same bytes.
- 2026-10-19 (task 4): tag copy uses `lofty` 0.25, already in the tree (MIT OR
  Apache-2.0); `convert()` adds no new crate beyond `blake3` (already used by
  `reks-core`) for the PCM hash. `cargo deny check licenses` passes unchanged.
  Verification is a full read-back with a hash compare on every conversion,
  not only on the sample-exact ones: the writers are lossless, so what was
  written must decode to the same samples even after resampling or dither.
- 2026-10-20 (task 5): Trash moves use `trash` 5.2 (MIT); its only crate new to
  the tree is `urlencoding` (MIT), the rest (`chrono`, `objc2`, `windows`, …)
  was already there. `cargo deny check licenses` passes with the allowlist
  unchanged. Trash is only offered with Replace.

## D26. UI styling: Tailwind v4 + cva + Radix

- Date: 2026-10-19 (maintainer's call). Design reference: an analysis of Cursor's
  marketing site (DESIGN.md, not in the repo), adapted to desktop density:
  pale canvas, dark ink, hairline depth, one scarce accent, 8px control
  radius, system fonts. How it is set up: [`ui.md`](ui.md).
- Palette history: the reference's orange fails AA for small text on its warm
  cream canvas (3.4:1). The next accent, violet (2026-10-19), passed AA but
  looked muddy on cream, because the hues are nearly opposite. Since
  2026-10-20 (maintainer's call): an indigo accent (`--indigo-*`, OKLCH hue
  272) on neutrals tinted towards it (`--mist-*`, hue 285). Every text and
  button pair passes AA in both themes, including `fg-muted` on the light
  canvas (4.6:1; the warm one was 3.9:1).
- Themes (2026-10-22, maintainer's call): the light/dark pair that followed
  the system became five themes picked in Settings (`display.theme`):
  Graphite (Claude's warm dark neutrals, ivory accent), Paper (light,
  rose), Midnight (navy, blue), Dusk (plum, lavender), Booth (black, light grey;
  for dark rooms; the default since 2026-10-23). Cool accents on the warm neutrals were tried and dropped.
  The deck follows the theme too (Paper's is light). Palettes, contrast
  checks: [`ui.md`](ui.md#ui-design-system).
- Decision: tokens CSS-first in `src/styles/tokens.css` (`@theme`, one block
  per theme under `:root[data-theme]`); Tailwind CSS v4 (`@tailwindcss/vite`) utilities;
  `class-variance-authority` for variants and `clsx` + `tailwind-merge` as `cn()`;
  Radix Primitives for behaviour (Dialog, DropdownMenu, ContextMenu, Popover,
  Tooltip, Switch, Slider, ToggleGroup, ScrollArea, Slot) in `src/components/ui/`.
  Selects stay native (styled), so keyboard and OS behaviour are unchanged.
- Licences: tailwindcss, @tailwindcss/vite, cva (Apache-2.0), clsx, tailwind-merge,
  all @radix-ui packages MIT. Build-only tree (dev deps, not in
  `licenses:check --production`): checked once in full on 2026-10-19; adds only
  MIT and lightningcss (MPL-2.0). No fonts bundled (Inter / JetBrains Mono are
  OFL-1.1, not on the allowlist).
- Rationale: one file changes the look; one control size scale keeps buttons,
  inputs and selects the same height; Radix brings focus management, keyboard
  handling and menus we would otherwise hand-roll per component.
- Rejected: plain CSS (what we had: 1,000 lines, no shared sizes, ad-hoc colours);
  CSS Modules (scoping, but no variant system or shared scale, so the same
  drift); React Aria (stronger a11y and i18n, but larger, hooks-first API and more
  code per component than this app needs).
- Tailwind's preflight is replaced by our own reset in `src/styles/base.css`
  (no pseudo-element selectors in the universal rule; measured faster table
  scrolling). Track-table scroll cost unchanged by the redesign in the
  production build (numbers in [`ui.md`](ui.md#reset)).
- Risks: Tailwind's native oxide/lightningcss binaries per platform (CI builds on
  all three); Radix menus portal to `<body>`, so tests query `document`; the JS
  bundle grew from 353 kB to 506 kB (Radix), over Vite's 500 kB warning.

## D27. Stick profiles and adopting foreign sticks

- Date: 2026-10-19. Status: proposed. Spec: [`stick-profiles.md`](stick-profiles.md).
  Profiles: Phase 2 task 15 (replaces task 10's "last selection per device
  label"). Adopt: Phase 2.5 task 12.
- Decision: each stick is a `device` keyed by FAT/exFAT volume serial (as D23),
  with a saved, live selection of playlists/folders. Sync runs the unchanged
  Phase 2 pipeline (plan → dry run → D23 backup → write → validate → manifest)
  and records `device_synced_playlist`; per-playlist status
  (`InSync | Changed | New | WillRemove`) and "frees X MB" are computed in Rust.
  A stick reks did not write can be adopted: read-only scan with rekordcrate,
  history import first, tracks matched with the D23 matcher, each stick playlist
  linked, created under `From stick/<name>/`, or skipped.
- Rationale: rekordbox makes it hard to see and prune what is on a stick. baken
  rebuilds `export.pdb` every run (D7), so a saved selection plus a dry run is
  the safe way to add and drop playlists.
- Rejected: editing the stick DB in place; snapshot entries; rule-based or
  auto-sync on mount (v1); writing any reks file to the stick.
- Risks: volume serial per OS and after reformat (R§8 item 14); player limits
  on playlist count/depth (UNVERIFIED); whether baken keeps `/Contents` paths
  so unchanged tracks are not re-copied; rekordcrate reading a rekordbox 7 stick
  (S4: reads rekordcrate's own real exports cleanly, 2026-10-20; rekordbox 7
  UNVERIFIED until a reference fixture exists).

## D28. Stick identity: the OS's own volume ID, no file on the stick

- Date: 2026-10-19 (maintainer's call). Applies to D23 (backups, `export_manifest`)
  and D27 (`device.volume_serial`, [`stick-profiles.md`](stick-profiles.md) §4.2).
- Decision: a stick's key is whatever the OS gives without privileges:
  `VolumeUUID` on macOS, the FAT/exFAT serial on Windows and Linux
  (`device::volume_serial`). One column, no aliases. The library DB lives on one
  computer, so one library only ever sees one kind of ID. Moving a library to
  another OS (or dual-booting onto one DB) makes each stick unknown once; the
  §4.2 label + size prompt relinks it and replaces the stored ID, as for a
  reformat.
- Finding (R§10.1): macOS can't read the serial without root. `VolumeUUID` is
  a one-way MD5 of the serial (plus BPB total sectors on FAT); the raw device
  is `root:operator 0640`.
- Rejected: a reks stick-ID file (`/.reks/stick-id`): an extra root folder is
  untested on players and D27 rules out reks files on the stick; per-device ID
  aliases (only needed when one DB spans OSes, which the prompt covers);
  bridging exFAT IDs across OSes (same reason); brute-forcing the UUID;
  `authopen` or a privileged helper (admin password on every export).
- Risks: a user answering "New stick" to the relink prompt gets a second device
  (a later "merge devices" action fixes it); Windows/Linux serial reads
  UNVERIFIED on a real stick (Phase 2 task 1).

## D29. My Tags, rule-tree filters and smart playlists

- Date: 2026-10-21. Status: accepted (maintainer's plan). Tasks:
  [`phases/phase-1.md`](phases/phase-1.md) "My Tags, advanced filtering and
  smart playlists".
- Decision:
  - **My Tags** are two levels, like rekordbox and OneLibrary `myTag`:
    categories, and tags below a category (`tag` table, migration 0009). Only
    tags are assigned to tracks (`track_tag`). Names are unique per parent,
    case-insensitively. New libraries get rekordbox's categories (Genre,
    Components, Situation).
  - **XML import** reads tags from Comments: rekordbox's "Add My Tag to
    Comments" ends Comments with `/* Tag A / Tag B */`, after any text
    already there (`tag::parse_comment_tags`; names split on ` / ` only, so
    `Deep/minimal House` stays one tag). Checked 2026-10-21 against the
    maintainer's rekordbox 6.8.6 `collection.xml`: 2,562 of 3,062 tracks carry
    a block, 25 of them after other text, none with text after it. Comments
    carry no category, so a name that exists in any category is reused and a
    new one goes under "Imported". The comment itself is kept as it is.
  - **Filters** become a rule tree (`reks-core::rule::RuleNode`,
    `TrackQuery::rule`, ANDed with the filter bar): nested all/any groups,
    and every rule or group can be negated. An empty "all" group matches
    every track, an empty "any" group none. NOT is compiled as
    `NOT coalesce((cond), 0)`, so "genre is not X" keeps tracks with no
    genre and "not in playlist X" keeps tracks in no playlist. A tree is
    limited to 8 levels and 200 nodes. Only a top-level playlist filter
    makes the rows a playlist's entries (and shows dividers); one inside a
    rule never does.
  - **Smart playlists** store a rule tree (`playlist.smart_rule`, JSON
    `RuleNode`) and a sort (`smart_sort`, JSON `Sort`; default date added,
    newest first; never playlist position), migration 0010. Ops:
    `CreateSmart`, `SetRule`; the entry ops (add, remove, reorder, dividers)
    refuse a smart list (`Error::SmartPlaylist`). A playlist filter on a
    smart list means its rule; on a folder, the entries below it or any
    smart list's rule below it. A smart list that would include itself
    (directly, through other smart lists, or through a folder holding it)
    is refused when saving or moving (`Error::SmartPlaylistCycle`); one
    found at query time matches nothing and is logged. Its view has no
    entry positions and no dividers. Every reader of a playlist's contents
    goes through `query::playlist_track_ids`, so the XML export, stick size,
    PDB and OneLibrary exports all see its current tracks in its sort.
  - **At export a smart playlist is a plain playlist** of the tracks it
    matches at that moment; nothing smart is ever written to a stick. The
    plan flags it (`PlannedPlaylist::smart`, `Warning::SmartPlaylistFixed`)
    so the dry run says so. Adopt never links a stick playlist to a smart
    list, and an XML import never merges into one (a same-named static
    sibling is created).
- Rejected: free-form multi-level tags (OneLibrary and rekordbox have two
  levels); writing tags into audio files (Phase 1 non-goal); a smart-playlist
  format on the stick (players have none).
- Risks: the Comments format is checked against one rekordbox version
  (6.8.6) only; how OneLibrary `myTag` rows
  map at export is not designed yet (Phase 4).

## D30. The library's player is a deck of the mix engine

- Date: 2026-10-21 (maintainer's call). Changes D5's one-deck preview player.
- Decision: there is one audio engine in the app, the Phase 5 mixer
  (`reks-mix` `Engine`, src-tauri `mix.rs`). The library plays on its deck 2
  (`PLAYER_DECK`, [`src/store/player.ts`](../src/store/player.ts)), one of the
  two decks the Mix panel shows by default. Double-click, Enter, Space and the
  history view play on it (with 2 or 4 decks shown, double-click and Enter
  load onto a spare deck instead, never onto one that may be on air; added
  2026-10-23, [`ui.md`](ui.md#mix-panel-phase-5)); it opens the output when it is off, with the saved
  output settings (`audio.*`, Settings → Playback). With the Mix
  panel off, the window shows that deck alone in the deck
  design (scrolling and overview waveform, cue, hot cues, nudge, quantise,
  tempo fader; no sync controls) under a header row with the volume
  (`MixControl::Volume`, `ui.state.volume`), the output device and the Mix
  toggle. Changing the device reopens the output and puts each loaded deck
  back at its position, tempo and play state. The deck waveform style
  defaults to RGB (was 3-band).
- Layout: changed 2026-10-22 (maintainer's call). The deck section (header
  row, then the player deck or the Mix panel) sits above the library under
  the app header, as in rekordbox and Traktor (was: along the bottom). One
  switch, 1 deck / 2 decks / 4 decks, replaces the Mix toggle and the Mix
  panel's own deck-count switch; the panel's status line and controls moved
  into the header row, with Start / Stop output. The Mix panel's start form
  is gone: the decks and mixer show with the output off, and sample rate,
  buffer and headphone cue are saved settings (`audio.sample_rate`,
  `audio.buffer_frames`, `audio.cue`) in Settings → Playback beside the
  device (they were per session). The player deck alone is laid out flat
  (CUE / PLAY and pads beside the waveforms), 162 px tall instead of 248. See
  [`ui.md`](ui.md#player-d30).
- Removed: the `player_*` commands, `overview_waveform` and the app's
  `reks_mix::Player` (the type stays for `reks-cli play`). The device list
  is `audio_devices`.
- Rationale: the player should look and work like a deck (cue, hot cues,
  tempo) without a second engine that has to grow the same features;
  one engine also means one owner of the audio device.
- Rejected: extending the preview `Player` with cue, hot cues and tempo
  (a second copy of the deck features and a second device owner).
- Risks: playing a track now goes through the mixer (EQ flat, master at the
  mix store's 0.8), so it is a little quieter than the old player at the
  same volume; a failed or lost output shows in the player's header row.

## D31. CI budget: Linux on PRs, all OSes at release

- Date: 2026-10-22. Changes how D17's and D18's CI runs are triggered.
- Context: the repo is private, so Actions minutes are billed against the free
  2,000/month, with macOS counted at 10x and Windows at 2x. CI ran the 3-OS
  matrix on every PR push and every push to `main` (about 195 billed minutes
  per run), and the quota ran out in four days (about 70 runs).
- Decision: PRs run CI on Linux only, skip drafts and docs-only changes. The
  licence gate runs on PRs only when a manifest, lockfile or `deny.toml`
  changes. Pushes to `main` run nothing. A `vX.Y.Z` tag runs the version check,
  then CI on all three OSes and the licence gate, then the release builds.
  `npm run release -- X.Y.Z` bumps every version and tags
  ([`docs/releasing.md`](releasing.md)). Every job has a `timeout-minutes` cap.
- Rejected: CI on push to `main` (doubles runs, since most work arrives through
  PRs); 3-OS on PRs (macOS alone is 10x).
- Risks: a macOS- or Windows-only break shows up at release time, not at
  merge time (run `ci.yml` by hand with `all_os` for platform code such as
  audio devices or paths). Without runs on `main`, a PR cannot restore a Rust
  cache from `main` and its first run builds from cold. Making the repo public
  removes the minute limit for standard runners and would make this decision
  unnecessary.

## D32. Browsing a plugged-in stick and playing its tracks off it

- Date: 2026-10-22 (maintainer's request; outside the phase plan). Extends D30.
- Decision: the sidebar has a **Devices** section above the Histories: every
  drive the OS reports as removable, each with the playlists (folders kept)
  and the play-history sessions its `export.pdb` holds
  ([`reks_export_rb::browse`](../crates/reks-export-rb/src/browse.rs)). A
  list opens in the main area; its tracks play on the player deck **straight
  off the stick** (`mix_load_stick`), not through a library match, with the
  grid (`PQTZ`, one anchor per tempo run) and hot and memory cues (`PCO2` in
  the `.EXT`, else `PCOB`) of the stick's own ANLZ files, so a stick made by
  rekordbox or by reks plays as a player would show it. A track with no or
  unreadable ANLZ plays without a grid or cues. The deck's waveform is read
  from the stick too ([`deckwave`](../crates/reks-anlz/src/deckwave.rs)):
  `PWV7` bands and `PWV3` height turned back into deck columns by inverting
  our calibrated encoders (detail curves, `PWVC` gains, height curve), so it
  draws at once. Without a `.2EX` (rekordbox before 6) it is computed from
  the audio, as for a library track.
- Read-only: nothing writes to the stick; a stick track's cues can't be
  edited (`mix_cue_edit` refuses them) and it is never queued for analysis.
  Paths with `..` in a stick's PDB are refused. OneLibrary-only sticks
  (no `export.pdb`) show "No rekordbox library".
- Rejected: always computing the waveform from the audio (a second full
  decode, seconds per track; the maintainer chose speed over exactness);
  playing the matched library copy instead (a friend's stick would mostly not play, and
  the grid and cues shown would not be the stick's).
- Risks: the read-back waveform is approximate: bands come back as the
  encoder's peak-hold envelope (slower decays), and rekordbox's scaling is
  only our fitted model. On rekordbox 7.2.19's click-track export the mean
  error against the waveform computed from the audio is 9% / 5% / 1% / 0.3%
  of full scale (low / mid / high / mono), the low band smeared between
  clicks. Reading `export.pdb` is strict ([`pdbread`](../crates/reks-export-rb/src/pdbread.rs)):
  one damaged row and the drive shows an error instead of its lists. Checked
  against rekordbox 7.2.19's reference export (`fixtures/onelibrary/ref-7.2.19`)
  and synthetic PDBs; a stick written by a player's history logging is
  **UNVERIFIED** beyond the history tests.

## D33. Progress tracking: GitHub Project next to the phase docs

- Date: 2026-10-23 (maintainer's request).
- Decision: the phase docs stay the plan of record (scope, design, task
  checklists). The [reks roadmap](https://github.com/users/ninoman/projects/1)
  Project tracks only what is open: one issue per open task, a milestone per
  phase, `area:*` labels, and Priority, Status and Hardware fields. Each open
  checklist item links its issue; the PR that ticks the box closes the issue
  (`Closes #N`, see the PR template). Finished work is not back-filled as
  issues. `needs-hw-test` issues close only with a logged result in
  `docs/hw-test-log.md`.
- Why: the docs carry the design and are what coding sessions read; the
  board gives a cross-phase view of what is left without copying the plan.

## D34. Bug reports and feedback from inside the app

- Date: 2026-10-23 (maintainer's request; outside the phase plan).
- Decision: a **Report a problem or idea** dialog (header button, a
  "Report" link on error messages and error toasts, the crash notice) sends a bug, feature
  request or feedback to a small relay we host
  ([`services/feedback-relay`](../services/feedback-relay)), a Cloudflare
  Worker. The relay adds it as a **draft item** to the maintainer's
  **private** GitHub Projects board, the reks roadmap (D33), with the board's
  Kind field set from the kind and Status set to "Needs triage". Triage
  happens on that board, next to the rest of the work; a report worth
  working on is turned into an issue with the board's "Convert to issue".
  No repo holds untriaged reports, and the board being private keeps
  testers' text, logs and emails private.
- Context for bugs: diagnostics are on by default for bugs and off
  otherwise. They hold the version, OS and architecture, schema version,
  track and playlist counts, the tail of the newest log file and the newest
  crash file younger than a week. The home dir becomes `~` and each library
  folder `<library-root-N>` ([`reks_core::diagnostics`](../crates/reks-core/src/diagnostics.rs)).
  "Show what's sent" previews the exact body.
- **Manual only.** The app writes a daily rolling log (5 days kept) and a
  `crash-<utc>.txt` per panic to the app log dir
  ([`logging.rs`](../src-tauri/src/logging.rs)). Every command error is
  logged when it is created (`CmdError::new`), and uncaught UI errors go to
  the same log. Nothing leaves the machine until the user presses Send. There is
  no telemetry and no automatic crash upload.
- The relay URL is built in from `REKS_FEEDBACK_URL` (a repository
  variable in the release workflow). A build without it can only save the
  report to a file. Sending uses `reqwest` with rustls on `ring` and the OS
  certificate store, since aws-lc and webpki-roots bring licences outside
  `deny.toml`.
- Rejected: pre-filled GitHub issues (some beta testers have no GitHub
  account; logs do not fit in a URL); Sentry (telemetry in a privacy-minded
  app, and a second place to triage); issues in the public repo (testers'
  text, logs and file names would be public); issues in a separate private
  repo (a second place to look, when the board already exists).
- Limits of draft items: no comments or notifications, and commits cannot
  close them; that ends once one is converted to an issue. The board must
  stay private for as long as it receives reports.
- Risks: the relay URL is public in an open-source binary. Defences are a
  per-IP rate limit (3 a minute), size caps and a version header; add
  Turnstile if spam shows up.

## Open risks (cross-cutting)

1. **Hardware acceptance is the only real test.** Every writer decision above
   is provisional until `docs/hw-test-log.md` records a pass on the
   maintainer's players. Budget time for borrowing/renting an NXS2, a
   CDJ-3000 and later a OneLibrary-only unit.
2. **Dependency on a young single-maintainer writer** (D7). Keep the adapter
   boundary clean so a switch to rbxport crates or an own writer is a
   contained change.
3. **OneLibrary legal and technical uncertainty** (D14).
4. **Analysis parity with rekordbox** is unmeasured; users compare side by
   side. Phase 3 defines the measurement before tuning.
5. **Format churn on both vendors** (AlphaTheta "reviewing specifications";
   Engine schema bumps). Version-tag fixtures and refuse unknown targets.
6. **Funding for signing** and for test hardware.
