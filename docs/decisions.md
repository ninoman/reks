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
  (R§3, R§4). Record the measured offset in D13.

## D5. Preview playback: `rodio` 0.23 (over cpal), decoding with Symphonia

- Licence: rodio MIT/Apache-2.0; cpal Apache-2.0.
- Rationale: one codec path on all OSes; rodio provides the output thread,
  mixing and `try_seek`; device selection via cpal (R§6). Enough for Phase 1
  preview; swap to raw cpal + own ring buffer if sample-accurate cueing is
  ever needed.
- Rejected: kira (game-oriented, overkill); web-audio-api crate (niche);
  WebView audio (see D4).
- Risks: Linux needs ALSA/Pulse/PipeWire client libs (standard).

## D6. Tags and artwork: `lofty`

- Licence: MIT/Apache-2.0. Rationale: one crate for ID3v2/MP4/Vorbis/RIFF/
  AIFF tags and embedded pictures; actively maintained (0.25.x, 2026-09).
- Rejected: `id3` + `mp4ameta` + `metaflac` (three crates, inconsistent APIs).

## D7. rekordbox legacy USB export writer: `baken-export` behind our adapter

- Licence: MIT (deps verified: baken-core MIT, mp3rgain MIT, symphonia
  MPL-2.0, rusqlite MIT, image MIT/Apache, RustCrypto crates MIT/Apache).
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
- Risks (open):
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
  - Phase 2's ANLZ for imported tracks depends on rekordbox's local cache
    existing on the machine; otherwise fall back to generation.

## D8. Export validation: parse what we wrote with `rekordcrate` 0.3.0; spec of record is crate-digger `.ksy` + djl-analysis

- Licence: rekordcrate MPL-2.0; crate-digger `.ksy` EPL-2.0 OR MPL-2.0 OR
  LGPL-3.0 with generated parsers covered (R§1).
- Rationale: an independent reader catches writer bugs; rekordcrate is the
  best-documented Rust parser and reads PDB, exportExt and ANLZ. Byte-level
  diffs against rekordbox reference exports in `fixtures/` complement it.
- Rejected: pyrekordbox in CI (Python toolchain in a Rust CI; optional later).

## D9. ANLZ serialisation (Phase 3): `rekordcrate::anlz` types (`BinWrite`) inside our `reks-anlz` crate

- Rationale: rekordcrate 0.3.0 already models PQTZ, PCOB/PCO2, PPTH, PVBR,
  PWAV, PWV2–7, PWVC, PSSI with `binrw` read+write (R§3). We produce section
  contents; it serialises. PPTH first; big-endian handled by the crate.
- Rejected: hand-written writer (duplicate effort); baken's generator
  (approximate waveforms, no API for our own analysis results).
- Risks: PQT2 not modelled (write only what rekordbox-less players need;
  verify on hardware whether PQT2 is required for NXS2 features); any
  rekordcrate API break (pin version).

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
- Risks: ~real-time CPU cost on long tracks (batch with rayon, show
  progress); model provenance note in Beat This! README; agreement with
  rekordbox unknown until Phase 3 measures it on the maintainer's library.

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

## D19. Naming and identifiers

- Project name `reks` (placeholder; rename before 1.0). Crate prefix
  `reks-`. Bundle identifier `org.reks.app` (placeholder). Library DB file
  `reks.sqlite` in `app_data_dir` (`app_local_data_dir` on Windows); caches
  in `app_cache_dir`.

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
