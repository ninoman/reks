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
- Risks: none significant.

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
- Decision: the Phase 1 player is the first slice of the Phase 6 engine (D21):
  crate `reks-mix` on a raw `cpal` 0.18 stream, `rtrb` 0.4 queues for control,
  state and retired sources, an offline `render()` and an allocation guard
  test. One deck, play/pause/seek/volume. The output opens at 44.1 kHz when the
  device offers it (most DJ music is 44.1 kHz, so those tracks skip the
  resampler; on macOS this sets the device clock, as rekordbox does), else at
  the device's default rate (2026-10-19). Tracks are decoded whole to stereo
  `i16` in memory by `reks-audio` and resampled to the output rate at load with `rubato` 5 (FFT, off the audio thread). Sync, tempo, EQ,
  filters and cue routing stay in Phase 6.
- Licence: cpal Apache-2.0; rtrb MIT/Apache-2.0; rubato MIT/Apache-2.0.
- Rationale: Phase 6 replaces any rodio player, so building on rodio first is
  throwaway work; the engine's real-time rules get tested in daily use before
  the mixer depends on them. Cost: we own resampling, device loss and the
  no-allocation rule that rodio would hide.
- Rejected: rodio (removed by Phase 6 anyway; no per-sample control); kira
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
- Grid positions (2026-10-20): every `beatgrid.position_ms` is written as
  whole ms (XML import, analysis, grid editor), so the Rust types are `i64`.
  The column stays `REAL` (no migration; whole numbers are exact in it).
  Rationale: every consumer is whole ms (PQTZ `u32` ms; XML `Inizio` has 3
  decimals), and rounding one anchor costs ≤ 0.5 ms without accumulating;
  the sample-accurate grid stays in the analysis cache.

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
  Agreement with the C++ library is **UNVERIFIED** until
  `.github/workflows/keyfinder-parity.yml` runs.

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

- Licence: rusqlite `bundled-sqlcipher` (SQLCipher BSD-style) or own page
  cipher via RustCrypto (MIT/Apache); decide in Phase 4 task 2.
- Rationale and policy: see `docs/phases/phase-4.md`. The key is user- or
  environment-supplied at runtime, mirroring libdjinterop and Mixxx, because
  the legal position is untested and the format is in flux (R§2). Feature off
  by default until a hardware success is logged.
- Rejected: enabling `baken-export`'s `onelibrary` option (embeds the key,
  untested on players); rbox (GPL-3.0-only is compatible but the repo is
  unreachable and docs.rs fails).
- Risks: hardware rejection, cipher parameters, spec churn, legal.

## D15. Engine DJ (Phase 5): own Rust writer targeting schema 3.0.2 (and 2.21.0), no FFI

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

## D21. Mix-prep mixer (Phase 6): 2–4 synced decks through a Xone:92-style mixer on our own cpal engine

- Date: 2026-10-19 (maintainer's call). Plan: [`phases/phase-6.md`](phases/phase-6.md).
- Decision: a preparation tool, not a performance app. While building a playlist,
  load 2–4 tracks on decks, beat-sync them from their grids, and try a transition
  or a techno layer through a model of the Allen & Heath Xone:92 music section:
  4-band asymmetric EQ with HI/LO kill, two state-variable filters with LFOs,
  X/Y crossfader with curve, headphone cue (R§9.1). New crate `reks-mix` owns a
  raw `cpal` stream with `rtrb` queues for control and state. Tempo is varispeed
  through `rubato` (already in the tree via `beat-this`, D10). The engine's
  first slice ships in Phase 1 as the preview player (D5), so one engine owns
  the audio device from the start.
- Order: after Phase 5. It depends only on Phase 1 (imported grids, decode)
  and Phase 3 (waveform columns, grid editor); moving it earlier needs a new
  decision.
- Licence: cpal Apache-2.0; rtrb MIT/Apache-2.0; rubato MIT/Apache-2.0 (R§9.2).
  EQ, filter, LFO and fader laws are our own code from published designs;
  Mixxx (GPL-2.0-or-later) may be ported with attribution, as in D10.
- Rationale: auditioning a transition while preparing a set is library work
  (rekordbox ships it in the same app), it runs on the grids we already import,
  and it adds no file formats. Copying one well-documented mixer gives a fixed
  spec to test against instead of a mixer designed by us.
- Key lock: not in the first cut; pitch follows tempo. Candidates (R§9.2):
  `timestretch` (pure Rust, MIT, real-time engine, young), `signalsmith-stretch`
  (MIT, C++ built with `cc`), `rubberband` (GPL-2.0-or-later, crate weeks old),
  `soundtouch` (SPDX `LGPL-2.1`, needs a `deny.toml` note). Choose by listening test.
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
  (H3): Phase 5 task 11.
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
- Decision: tokens CSS-first in `src/styles/tokens.css` (`@theme`, light and dark
  via `prefers-color-scheme`); Tailwind CSS v4 (`@tailwindcss/vite`) utilities;
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
