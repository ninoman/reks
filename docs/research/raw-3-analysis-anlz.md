# Research 3: Audio analysis options (BPM/beatgrid, key, waveform) and rekordbox ANLZ encodings

Date of research: 2026-10-18. Target: GPLv3 desktop DJ library app, Rust core (Tauri 2, Symphonia). Every factual claim cites a URL; items marked UNVERIFIED could not be confirmed from a current source.

---

## TASK A — GPLv3-compatible analysis candidates

### A.1 Licence compatibility cheat-sheet

| Licence | GPLv3 app can link/embed? | Note |
|---|---|---|
| GPL-2.0-or-later (qm-dsp, Mixxx, BeatRoot) | Yes | "or later" allows use under GPLv3 terms; GPLv2-only would NOT be ([gnu.org license list](https://www.gnu.org/licenses/license-list.html)) |
| GPL-3.0(-or-later) (libKeyFinder, aubio, bliss) | Yes | same licence |
| AGPL-3.0 (Essentia) | Yes, with caveat | GNU says AGPL is "technically not compatible with GPLv3 in a strict sense" but modules under both can be combined in one program (GPLv3 §13); the Essentia parts stay AGPL ([gnu.org](https://www.gnu.org/licenses/license-list.html)) |
| LGPL-2.1+ (SoundTouch) | Yes | compatible with GPLv2 and GPLv3 ([gnu.org](https://www.gnu.org/licenses/license-list.html)) |
| MPL-2.0 (Symphonia, rekordcrate) | Yes | §3.3 secondary-licence clause; combined work becomes GPL ([gnu.org](https://www.gnu.org/licenses/license-list.html)) |
| MIT / Apache-2.0 (rustfft, ort, rten, beat-this, Beat This! weights) | Yes | permissive |
| CC BY-NC-* (madmom models, Essentia pretrained models) | No for redistribution in a free app | GNU lists CC NonCommercial as non-free ([gnu.org](https://www.gnu.org/licenses/license-list.html)); madmom models are CC BY-NC-SA 4.0 ([madmom LICENSE](https://github.com/CPJKU/madmom)) |

### A.2 Mixxx analyzers (reference implementation for a GPL DJ app)

- **Mixxx licence:** GPL-2.0-or-later ("either version 2 of the License, or (at your option) any later version") — [mixxx LICENSE](https://raw.githubusercontent.com/mixxxdj/mixxx/main/LICENSE). Compatible with a GPLv3 app.
- **Beat plugins:** `AnalyzerBeats::availablePlugins()` registers Queen Mary first (= default) then SoundTouch — [analyzerbeats.cpp](https://raw.githubusercontent.com/mixxxdj/mixxx/main/src/analyzer/analyzerbeats.cpp). The SoundTouch plugin only yields a BPM (`supportsBeatTracking()` false path → `getBpm()`), the QM plugin yields beat positions.
- **QM beat pipeline (analyzerqueenmarybeats.cpp):** qm-dsp `DetectionFunction` with `DF_COMPLEXSD` (complex spectral difference), `dbRise=3`, no adaptive whitening; step `kStepSecs = 0.01161 s` (~86 Hz, 512 frames at 44.1 kHz); window = nextPowerOfTwo(sampleRate / 50 Hz); first 2 detection-function values skipped; then `TempoTrackV2::calculateBeatPeriod` + `calculateBeats`; each beat is placed at `beat*stepSize + stepSize/2` — [analyzerqueenmarybeats.cpp](https://raw.githubusercontent.com/mixxxdj/mixxx/main/src/analyzer/plugins/analyzerqueenmarybeats.cpp).
- **Constant vs non-constant grid:** the plugin returns a raw beat list (a "BeatMap"). If the user preference "Assume constant tempo" is on (default `m_bPreferencesFixedTempo(true)`), Mixxx converts it to a constant grid via `BeatUtils` — [analyzerbeats.cpp](https://raw.githubusercontent.com/mixxxdj/mixxx/main/src/analyzer/analyzerbeats.cpp), [manual 2.7 Beat Detection](https://manual.mixxx.org/2.7/en/chapters/preferences/beat_detection) ("If enabled, Mixxx assumes that the distances between the beats are constant").
- **BeatUtils algorithm ([beatutils.cpp](https://raw.githubusercontent.com/mixxxdj/mixxx/main/src/track/beatutils.cpp)):**
  - `retrieveConstRegions`: walks the coarse beats, "ironing" them with a mean beat length; a region is constant while per-beat phase error ≤ `kMaxSecsPhaseError = 0.025 s`, cumulative error ≤ `kMaxSecsPhaseErrorSum = 0.1 s`, at most `kMaxOutliersCount = 1` outlier, and ≥ `kMinRegionBeatCount = 16` beats. Comment in source: a pure 120.000 BPM track yields many 120.185 BPM beats and a few 117.454 BPM beats because the detector's step is quantised.
  - `makeConstBpm`: takes the longest constant region, extends it to earlier/later regions whose tempo and phase are within the rounding range, then `roundBpmWithinRange` snaps first to an integer BPM, then (if < 85 BPM) to 0.5 BPM, etc., provided the snapped value stays inside the phase-error tolerance of the region length. The first beat of the start region becomes the grid anchor.
  - Mixxx's manual warns a 0.02 BPM error causes visible drift on long tracks and that the usual failure is a correct BPM with a wrongly placed first beat — [manual](https://manual.mixxx.org/2.7/en/chapters/preferences/beat_detection).
- **Downbeats:** the QM plugin only produces beats; Mixxx's beat grid has no bar/downbeat information from the analyzer. A 2020 GSoC "meter detection" project was described as experimental — [mixxx.org/news/tag/analyzer](https://mixxx.org/news/tag/analyzer). Whether any downbeat code has since landed in 2.6/2.7: UNVERIFIED (no release note found).
- **Key plugins:** `AnalyzerKey::availablePlugins()` pushes Queen Mary first ("First one below is the default") and KeyFinder only `#if defined __KEYFINDER__` — [analyzerkey.cpp](https://raw.githubusercontent.com/mixxxdj/mixxx/main/src/analyzer/analyzerkey.cpp). QM key uses `GetKeyMode` with concert A = 440 Hz — [analyzerqueenmarykey.cpp](https://raw.githubusercontent.com/mixxxdj/mixxx/main/src/analyzer/plugins/analyzerqueenmarykey.cpp). KeyFinder plugin streams audio through `progressiveChromagram`, then `finalChromagram` and `keyOfChromagram`, reporting a single key for the whole track (plugin id `keyfinder:2`) — [analyzerkeyfinder.cpp](https://raw.githubusercontent.com/mixxxdj/mixxx/main/src/analyzer/plugins/analyzerkeyfinder.cpp).
- **Waveform (for reference):** Mixxx splits audio with 4th-order Bessel low/band/high filters at 600 Hz and 4 kHz and stores per-stride max |sample| per band — [analyzerwaveform.cpp](https://raw.githubusercontent.com/mixxxdj/mixxx/main/src/analyzer/analyzerwaveform.cpp).
- **rekordbox offset caveat:** when Mixxx imported rekordbox cues, a ~50 ms discrepancy was observed between rekordbox positions and Mixxx's MP3 decoding path (suspected MP3 encoder-delay/Xing-LAME metadata handling); never conclusively resolved — [mixxx PR #2119](https://github.com/mixxxdj/mixxx/pull/2119). Any "match rekordbox" effort must pin down decoder delay handling in Symphonia.

### A.3 qm-dsp / qm-vamp-plugins

- Repo: https://github.com/c4dm/qm-dsp. Licence: GPL-2.0-or-later per README ("either version 2 of the License, or (at your option) any later version") — [README](https://raw.githubusercontent.com/c4dm/qm-dsp/master/README.md), [COPYING = GPLv2 text](https://raw.githubusercontent.com/c4dm/qm-dsp/master/COPYING). Third-party bits: `ext/kissfft` (Mark Borgerding, BSD) and `maths/Polyfit.h` (MPL) — [Ardour's qm-dsp README](https://git.ardour.org/ardour/ardour/src/branch/master/libs/qm-dsp/README.txt). SPDX: `GPL-2.0-or-later`.
- Language: C++. Maintenance: last push 2022-05-11 (GitHub API, 70 stars). qm-vamp-plugins: GPL-2.0, last push 2021-06-02 (GitHub API). Effectively frozen but stable; Mixxx and Ardour vendor it.
- Algorithms: TempoTrackV2 = Davies & Plumbley context-dependent beat tracker: detection function split into 6 s frames with 1.5 s hop, autocorrelation → perceptually weighted comb filterbank → Viterbi tempo path; beats via two-state model + Ellis-style dynamic programming. Downbeats (qm "Bar and Beat Tracker") come from a second pass using beat-synchronous spectral difference (Jensen–Shannon) at 2.8 kHz, Davies & Plumbley 2006 — [qm-vamp-plugins docs](https://vamp-plugins.org/plugin-doc/qm-vamp-plugins.html). Header note: fixed window sizes may bias towards double/half tempo at high sample rates — [TempoTrackV2.h](https://code.soundsoftware.ac.uk/hg/qm-dsp/file/tip/dsp/tempotracking/TempoTrackV2.h).
- Key detector: block-wise chromagram correlated with major/minor profiles "drawn from analysis of Book I of the Well Tempered Klavier by J S Bach" (Noland & Sandler 2007) — [qm-vamp-plugins docs](https://vamp-plugins.org/plugin-doc/qm-vamp-plugins.html).
- Rust bindings: none found on crates.io (searches for "qm-dsp", "tempo", "beat tracking" returned nothing relevant — [crates.io API](https://crates.io/api/v1/crates?q=beat%20tracking)). Integration: vendor the ~dozen needed C++ files and expose via `cxx`/`cc`, or port TempoTrackV2 + DetectionFunction (a few thousand lines, FFT via kissfft → rustfft) to Rust. Effort: medium (FFI) / medium-high (port).
- Accuracy: no MIREX result table naming qm/TempoTrackV2 was retrievable — UNVERIFIED. Closest published comparison: on the Ballroom set, Krebs/Böck/Widmer 2013 report F-measures of 0.855–0.903 for their model vs Degara 0.790, Ircambeat 0.764, Klapuri 0.728 — [cp.jku.at ISMIR2013 page](https://www.cp.jku.at/people/krebs/ISMIR2013.html). Community: a 2023 r/Beatmatch test on hard constant-tempo tracks reported that only Mixxx got all tracks right vs VirtualDJ/Serato/rekordbox; other commenters note Mixxx often grids on snares instead of kicks for house, and psytrance is weak — [reddit mirror](https://lr.us.psf.lt/r/Beatmatch/comments/12rorli/mixxx_is_the_best_bpm_analyser) (anecdotal, small sample).

### A.4 libKeyFinder

- Repo: https://github.com/mixxxdj/libkeyfinder. Licence: GPL-3.0-or-later ("GNU General Public License version 3 or later") — [README](https://raw.githubusercontent.com/mixxxdj/libkeyfinder/main/README.md). SPDX `GPL-3.0-or-later`. C++11; needs FFTW3 (GPL) and CMake; Catch2 for tests. Written by Ibrahim Shaath (2011 MSc thesis), maintained by the Mixxx team since 2020, shipped in Mixxx ≥ 2.3 — same README.
- Maintenance: last push 2024-11-25, 185 stars (GitHub API). Tags end at v2.2.6 on GitHub while distros package 2.2.8 ([Arch](https://www.archlinux.org/packages/extra/x86_64/libkeyfinder)) — version provenance UNVERIFIED.
- Rust binding: `libkeyfinder-sys` 0.1.0, published 2025-11-07, licence `GPL-3.0-or-later`, repo https://github.com/evanpurkhiser/libkeyfinder-sys (last push 2026-09-17) — [crates.io API](https://crates.io/api/v1/crates/libkeyfinder-sys). It is a `cxx` bridge that locates a **system** libkeyfinder ≥ 2.2 via pkg-config (no vendoring) — [build.rs](https://raw.githubusercontent.com/evanpurkhiser/libkeyfinder-sys/main/build.rs); docs.rs build failed — [docs.rs](https://docs.rs/libkeyfinder-sys). API: `AudioData` (frame rate, channels, samples) + `KeyFinder::key_of_audio` — [repo README](https://github.com/evanpurkhiser/libkeyfinder-sys). No safe higher-level crate ("keyfinder" does not exist on crates.io).
- Integration effort: medium. For a shippable Tauri app you would vendor libkeyfinder + FFTW (or swap FFTW for a thin FFT shim) and build with `cc`/CMake; or port the algorithm (chromagram + Shaath key profiles, ~small codebase) to Rust on rustfft — plausible weekend-scale port, UNVERIFIED size estimate.
- Accuracy: no formal evaluation published by the project (README has none). Community datasets: the "KeyFinder v2 dataset" comparisons on r/DJs (2020/2021) put Mixed In Key around 92.8% with KeyFinder lower; exact per-tool numbers not retrievable here — [search result snippet](https://reddit.birdcat.cafe/r/DJs/comments/rn2wu5/key_detection_comparison_2021), UNVERIFIED. A 2026 DubSpot article claims a 200-track ear-keyed test with MIK beating "Beatport, Rekordbox, and KeyFinder" (178 vs 121) — [blog.dubspot.com](https://blog.dubspot.com/tag/key-detection), per-tool numbers UNVERIFIED.

### A.5 Essentia

- Repo: https://github.com/MTG/essentia, licence AGPL-3.0 (GitHub API; "Affero GPLv3" for non-commercial, commercial licence from UPF/MTG; pretrained models CC BY-NC-ND 4.0) — [licensing page](https://essentia.upf.edu/licensing_information.html). Active: last push 2026-10-14, 3775 stars (GitHub API). C++ with Python bindings; heavy deps (FFTW/KissFFT, FFmpeg, TagLib, Chromaprint…).
- GPLv3 compatibility: usable in a GPLv3 program (see A.1) but the Essentia portions remain AGPL; also, the CC BY-NC-ND models cannot be bundled.
- `RhythmExtractor2013`: `multifeature` (BeatTrackerMultiFeature, with confidence) or `degara`; outputs bpm, ticks, confidence, estimates, bpmIntervals; **requires 44.1 kHz input**; tempo range 40–208 default — [reference](https://essentia.upf.edu/reference/std_RhythmExtractor2013.html).
- `KeyExtractor`: HPCP → Key; `profileType` default `bgate`; options include `edma`/`edmm` (profiles learned from an EDM corpus — [std_Key](https://essentia.upf.edu/reference/std_Key.html), Faraldo et al. 2016 "Key estimation in electronic dance music" [UPF](https://repositori.upf.edu/items/7ed52987-edb2-48a6-90e6-674a34a54ed0/full)); frameSize/hopSize 4096, tuning 440 Hz, `averageDetuningCorrection` — [reference](https://essentia.upf.edu/reference/std_KeyExtractor.html). Evaluation sets: GiantSteps key/tempo datasets (ISMIR 2015, CC BY 4.0) — [UPF](https://repositori.upf.edu/items/4e5f12d8-9dc0-43c5-a5e6-337c1e7184c2/full).
- Rust: `essentia-sys` 0.1.5 (2025-07-03, MIT, FFI via `cxx`, 0% documented) + `essentia-rs` workspace (last push 2025-07-08, 9 stars) — [crates.io](https://crates.io/api/v1/crates/essentia-sys), [docs.rs](https://docs.rs/crate/essentia-sys/latest). Integration effort: high (build Essentia C++ per platform for Tauri bundles).

### A.6 aubio and aubio-rs

- Repo: https://github.com/aubio/aubio, GPL-3.0-or-later ([GitHub](https://github.com/aubio/aubio)). C. Last tag 0.4.9; last commit 2026-04-10; 147 open issues (GitHub API/tags). No 0.5 release found.
- Tempo: `src/tempo/beattracking.c` is "Copyright (C) 2005-2009 Matthew Davies and Paul Brossier" with a "context dependant model" — i.e. the same Davies lineage as qm-dsp — [beattracking.c](https://raw.githubusercontent.com/aubio/aubio/master/src/tempo/beattracking.c).
- Crates: `aubio-rs` 0.2.0 (2021-04-30, GPL-3.0), `aubio` 0.2.1 (2021-05-22, GPL-3.0), `aubio-sys` 0.2.1, `aubio-lib` 0.1.3 (bundled C lib, 2020-04-04) — [crates.io](https://crates.io/api/v1/crates/aubio-rs); repo last push 2023-01-21 (GitHub API). Stale but functional; `aubio-lib` builds the C library from source so no system dep. Effort: low-medium.
- Accuracy: no published figures for aubio's tempo on DJ material found — UNVERIFIED.

### A.7 bliss-rs / bliss-audio

- Repo: https://github.com/Polochon-street/bliss-rs; crate `bliss-audio` 0.13.0 (2026-08-18), licence `GPL-3.0-only` — [crates.io](https://crates.io/api/v1/crates/bliss-audio), [Cargo.toml](https://raw.githubusercontent.com/Polochon-street/bliss-rs/master/Cargo.toml). Pure Rust analysis (`analysis = ["dep:rustfft"]`), decoders: ffmpeg (default) or `symphonia` feature.
- What it computes: a fixed descriptor vector for playlist similarity — modules `aubio` (internal Rust port of aubio tempo/onset code), `chroma` (librosa-style chroma summarised to a few "harmony" dims), `temporal`, `timbral`, loudness/zcr; weights in `lib.rs` note "tempo: beat tracker unreliable on onset-poor music" — [lib.rs](https://raw.githubusercontent.com/Polochon-street/bliss-rs/master/src/lib.rs), [GitHub](https://github.com/Polochon-street/bliss-rs). It does NOT output a beat grid or a named key; not a DJ analyzer, but its `aubio` module is a useful GPL-3.0 Rust port to borrow from.

### A.8 Neural beat/downbeat trackers

- **madmom** (https://github.com/CPJKU/madmom): code BSD, **models CC BY-NC-SA 4.0** (commercial use requires contacting G. Widmer); Python/Cython, latest tag v0.16.1, last push 2026-03-20; no ONNX export — [GitHub](https://github.com/CPJKU/madmom). Not embeddable in Rust and the model licence blocks redistribution in a free app.
- **BeatNet** (https://github.com/mjhydri/BeatNet): CC-BY-4.0, Python/PyTorch, depends on madmom for DBN inference, no ONNX; last push 2026-04-13 — [GitHub](https://github.com/mjhydri/BeatNet).
- **Beat This!** (https://github.com/CPJKU/beat_this, ISMIR 2024, JKU Linz): "code and the published model weights are released under the MIT license"; models ~78 MB (full) / ~8.1 MB (small); no DBN; trained on 16 datasets, GTZAN held out — [GitHub](https://github.com/CPJKU/beat_this). Paper: GTZAN beat F1 89.1 / downbeat F1 78.3 (main), small model 88.8 / 77.2; ~20 M params (small ~2 M); input mono 22.05 kHz, 128-bin mel 30 Hz–10 kHz, hop 441 → 50 fps — [arXiv 2407.21658](https://arxiv.org/html/2407.21658). Caveat: trained on some copyrighted/limited-CC audio (README note) and weaker continuity metrics than DBN systems.
- **beat-this crate** (https://github.com/danigb/beat-this-rs): `beat-this` 1.1.0 (2026-10-02), MIT — [crates.io](https://crates.io/api/v1/crates/beat-this). Default backend `rten` (pure Rust, MIT/Apache-2.0, 0.27.0 — [crates.io](https://crates.io/api/v1/crates/rten)); optional `ort` backend. Ships `mel_spectrogram.onnx` + `beat_this_small.onnx` (~10 MB); full FP32 (~83 MB) from GitHub Releases; models converted from official checkpoints via `scripts/ckpt2onnx.py`. Claims F-measure 1.0 vs Python reference (full), ≥0.99 (small); only "minimal" post-processing; 4:32 MP3 analysed in 4.6 s on M4 Pro — [docs.rs](https://docs.rs/crate/beat-this/latest). Streaming API in 1.1.0. Effort: low (cargo dep + model file).
- ONNX runtimes: `ort` 2.0.0-rc.14 (2026-10-12, MIT OR Apache-2.0, wraps ONNX Runtime 1.31; needs libonnxruntime per platform) — [crates.io](https://crates.io/api/v1/crates/ort); `tract-onnx` 0.23.9 (MIT/Apache, pure Rust) — [crates.io](https://crates.io/api/v1/crates/tract-onnx); `rten` as above.
- ONNX exports of Beat This! also exist on Hugging Face (ashudesai/songbird-models; musetric/beat-this-onnx for WebGPU) — [HF](https://huggingface.co/musetric/beat-this-onnx).

### A.9 Rust-native crates (crates.io, queried 2026-10-18)

| Crate | Version / updated | Licence | Relevance |
|---|---|---|---|
| rustfft | 6.4.1 / 2025-09-18 | MIT OR Apache-2.0 | FFT backbone ([crates.io](https://crates.io/api/v1/crates/rustfft)) |
| realfft | 3.5.0 / 2025-06-12 | MIT | real FFT ([crates.io](https://crates.io/api/v1/crates/realfft)) |
| symphonia | 0.6.1 / 2026-08-13 | MPL-2.0 | decoding ([crates.io](https://crates.io/api/v1/crates/symphonia)) |
| mel_spec | 0.5.0 / 2026-10-02 | MIT | librosa/whisper-aligned mel ([crates.io](https://crates.io/api/v1/crates/mel-spec)) |
| pitch-detection | 0.3.0 / 2022-06-30 | MIT/Apache-2.0 | monophonic pitch only ([crates.io](https://crates.io/api/v1/crates/pitch-detection)) |
| stratum-dsp | 1.0.0 / 2025-12-19 | MIT OR Apache-2.0 | "BPM detection, key detection, and beat tracking" for DJ apps; accuracy UNVERIFIED ([crates.io](https://crates.io/api/v1/crates/stratum-dsp)) |
| chord_detector | 0.1.0 / 2025-07-25 | GPL-3.0 | streaming 12-bin chromagram ([crates.io](https://crates.io/api/v1/crates/chord_detector)) |
| oximedia-mir | 0.2.1 / 2026-08-12 | Apache-2.0 | generic MIR, unproven ([crates.io](https://crates.io/api/v1/crates/oximedia-mir)) |
| soundtouch | 0.5.4 / 2026-03-23 | LGPL-2.1 | wraps SoundTouch incl. BPMDetect ([crates.io](https://crates.io/api/v1/crates/soundtouch)) |
| rekordcrate | 0.3.0 / 2025-01-23 | MPL-2.0 | parses/writes ANLZ + PDB (Task B) ([crates.io](https://crates.io/api/v1/crates/rekordcrate)) |

Crates named `bpm-detection`, `musical-key`, `chromagram`, `keyfinder`, `rust-dsp`, `madmom` do not exist on crates.io (API returned not-found).

### A.10 Others

- **SoundTouch BPMDetect:** LGPL-2.1-or-later (Debian copyright) — [Debian](https://metadata.ftp-master.debian.org/changelogs/main/s/soundtouch/stable_copyright); BPM only (no beat positions); Mixxx's secondary plugin.
- **BeatRoot** (Simon Dixon): Vamp plugin README states GPL-2.0-or-later — [beatroot-vamp README](https://code.soundsoftware.ac.uk/hg/beatroot-vamp/file/tip/README); Java original, old; no Rust binding.
- **LibXtract:** now zlib licence (LICENSE "Copyright (c) 2006-2026 Jamie Bullock"), last push 2026-10-02 — [LICENSE](https://raw.githubusercontent.com/jamiebullock/LibXtract/master/LICENSE); feature extraction only, no beat tracker.

### A.11 Accuracy on DJ music and matching rekordbox

- rekordbox's analyzer is proprietary; no open implementation reproduces it. The only quantified open comparison found: "Cuepoint" (Python, no ML) vs rekordbox on 60 afro-house/gqom/amapiano tracks — tempo agreement 55/60 (92 %), grid on-beat 42/60 (70 %), downbeat correct 26/42 (62 %); using low band 20–160 Hz for beat phase improved 10→17/24 — [LinkedIn post](https://uk.linkedin.com/in/chuksjn) (self-reported, UNVERIFIED).
- rekordbox stores tempo as BPM×100 and beat times in ms (see Task B), so its grids are effectively two-decimal BPM; most electronic releases analyse to integer or .5 BPM.
- Downbeat phase is the hard part: a stock beat tracker (qm, aubio) gives no bar position; Beat This! gives downbeats with F1≈0.78 on GTZAN (mixed genres; EDM subsets like HJDB reach 96.6 % downbeat F1 — [arXiv](https://arxiv.org/html/2407.21658)).

### A.12 Recommendation

**(1) BPM + beatgrid (constant-tempo DJ music):**
1. **Beat This! via `beat-this` (rten backend)** — MIT, pure Rust, gives beats AND downbeats, best published accuracy, ~10 MB small model is bundle-friendly. Post-process with a Mixxx-style constant-region/ironing pass (port `BeatUtils::retrieveConstRegions`/`makeConstBpm`, GPL-2.0+) to produce a single BPM (snapped to integer/0.5/0.1 within the 25 ms phase tolerance) and anchor the grid on a detected downbeat. This matches rekordbox's constant-grid model and gives bar phase. Risks: model size, ~1× real-time-ish CPU cost on long tracks (12 s for 13:48), tempo range extremes, training-data provenance note.
2. **qm-dsp TempoTrackV2 (FFI or Rust port) + BeatUtils** — exactly what Mixxx ships, well-understood, light; no downbeats; snare-vs-kick phase errors on house reported.
3. **aubio tempo via `aubio-lib`** — fallback/secondary opinion; stale bindings.
4. Essentia RhythmExtractor2013 — accurate but AGPL + heavy native build; not worth it for a Tauri bundle.

**(2) Key:**
1. **libKeyFinder** (GPL-3.0+, Mixxx-maintained) — use `libkeyfinder-sys` for prototyping, but plan to vendor-build or port to Rust (rustfft) to avoid the system pkg-config/FFTW dependency in a desktop bundle. Expose as Camelot/Open Key alongside classical names.
2. **qm-dsp GetKeyMode** (Mixxx default; Bach-derived profiles) — fine as a second opinion; same FFI cost as the beat tracker, so cheap if qm-dsp is already vendored.
3. **Essentia KeyExtractor with `edma`/`bgate` profiles** — profiles were learned on EDM and are the most "rekordbox-like" for electronic music in the literature, but AGPL + build cost; alternative: reimplement HPCP + `edma` profile correlation in Rust (the profiles are plain 12-value vectors in Essentia's source, AGPL — copying them means AGPL for that module).
Expect disagreement with rekordbox on a sizeable fraction of tracks regardless: community tests show major DJ tools disagree with each other on key often (DubSpot/r/DJs, UNVERIFIED numbers); treat key as advisory and show confidence.

**(3) Waveform:** implement natively in Rust: 3 band-pass branches (Mixxx uses Bessel-4 at 600 Hz / 4 kHz) + per-column max-abs, at 150 columns/s to mirror rekordbox PWV5/PWV7 resolution (Task B), plus a 400- and 1200-column preview.

---

## TASK B — rekordbox ANLZ files (Deep Symmetry + crate-digger + pyrekordbox + rekordcrate)

Primary sources: Deep Symmetry "Analysis Files" page — https://djl-analysis.deepsymmetry.org/rekordbox-export-analysis/anlz.html (the `/djl-analysis/anlz.html` URL in the brief returns 404); crate-digger Kaitai spec — https://raw.githubusercontent.com/Deep-Symmetry/crate-digger/main/src/main/kaitai/rekordbox_anlz.ksy; pyrekordbox — https://pyrekordbox.readthedocs.io/en/stable/formats/anlz.html; rekordcrate — https://raw.githubusercontent.com/Holzhaus/rekordcrate/main/src/anlz.rs. All numbers big-endian.

### B.1 Files and header (PMAI)

- Three files share a base name (e.g. `ANLZ0000.DAT`, `.EXT`, `.2EX`); `.EXT` was added for colour/detail waveforms too large for old players, `.2EX` by the CDJ-3000 for 3-band waveforms. Path of the .DAT is referenced from the track row in export.pdb (string index 14) — [Deep Symmetry](https://djl-analysis.deepsymmetry.org/rekordbox-export-analysis/anlz.html).
- Header: bytes 00–03 `PMAI`; 04–07 `len_header` (u4, usually 0x1C); 08–0B `len_file` (u4); remaining header bytes unknown. Then tagged sections until EOF — [Deep Symmetry](https://djl-analysis.deepsymmetry.org/rekordbox-export-analysis/anlz.html), [ksy](https://raw.githubusercontent.com/Deep-Symmetry/crate-digger/main/src/main/kaitai/rekordbox_anlz.ksy).
- Section envelope: `fourcc` (4), `len_header` (u4), `len_tag` (u4, whole section incl. header; add to section start to reach next tag). Body conventionally modelled from byte 0x0C — same sources.

### B.2 Section table

| fourcc | File | len_header | Entry size | Entries/s | Content |
|---|---|---|---|---|---|
| PPTH | all | 0x10 | — | — | `len_path` u4 @0x0C, then UTF-16BE path with NUL ([DS](https://djl-analysis.deepsymmetry.org/rekordbox-export-analysis/anlz.html)) |
| PVBR | .DAT | 0x10 | u4 ×400 | — | VBR seek index: u4 unknown then 400 × u4; `len_tag` always 1620 ([ksy](https://raw.githubusercontent.com/Deep-Symmetry/crate-digger/main/src/main/kaitai/rekordbox_anlz.ksy), [pyrekordbox](https://pyrekordbox.readthedocs.io/en/stable/formats/anlz.html)) |
| PQTZ | .DAT | 0x18 | 8 B | per beat | beat grid (B.3) |
| PQT2 | .EXT | 0x38 | 2 B | per beat | extended beat grid (B.3) |
| PCOB | .DAT, .EXT | 0x18 | 0x38 (PCPT) | — | cue list (B.4) |
| PCO2 | .EXT | 0x14 | variable (PCP2) | — | nxs2 cue list with colour/comment (B.4) |
| PWAV | .DAT | 0x14 | 1 B | 400 columns total | mono preview (B.5) |
| PWV2 | .DAT | 0x14 | 1 B | 100 columns total | tiny preview, CDJ-900 (B.5) |
| PWV3 | .EXT | 0x18 | 1 B | 150 | mono detail (B.5) |
| PWV4 | .EXT | 0x18 | 6 B | 1200 columns total | colour preview (B.6) |
| PWV5 | .EXT | 0x18 | 2 B | 150 | colour detail (B.6) |
| PWV6 | .2EX | 0x14 | 3 B | 1200 columns total | 3-band preview (B.7) |
| PWV7 | .2EX | 0x18 | 3 B | 150 | 3-band detail (B.7) |
| PWVC | .2EX | 0x0E | 6 B total | — | 3-band calibration, `len_tag` always 20 (B.7) |
| PSSI | .EXT | 0x20 | 24 B | per phrase | song structure, XOR-masked (B.8) |

Sources for file placement: ksy comments ("PWV3 seen in .EXT", "PWV6/PWV7 in .2EX", "PSSI in .EXT") — [ksy](https://raw.githubusercontent.com/Deep-Symmetry/crate-digger/main/src/main/kaitai/rekordbox_anlz.ksy); rekordcrate `ContentKind` docs — [anlz.rs](https://raw.githubusercontent.com/Holzhaus/rekordcrate/main/src/anlz.rs); pyrekordbox table — [pyrekordbox](https://pyrekordbox.readthedocs.io/en/stable/formats/anlz.html). `PCPT`/`PCP2` are not top-level sections; they are the per-entry magic inside PCOB/PCO2.

### B.3 Beat grid — PQTZ and PQT2

- **PQTZ** ("Pioneer Quantization"): body `unknown1` u4, `unknown2` u4 (always 0x00800000), `len_beats` u4 @0x14, then entries. Entry (8 B): `beat_number` u2 (1–4 = position in bar), `tempo` u2 (BPM × 100), `time` u4 (ms at 100 % pitch) — [DS](https://djl-analysis.deepsymmetry.org/rekordbox-export-analysis/anlz.html), [ksy](https://raw.githubusercontent.com/Deep-Symmetry/crate-digger/main/src/main/kaitai/rekordbox_anlz.ksy). Implication: the grid is a list of every beat (not a BPM+offset), so rekordbox's "dynamic" grids are representable; constant grids simply repeat the same tempo. Resolution is 1 ms and 0.01 BPM.
- **PQT2** (.EXT, len_header 0x38): first 4 bytes after `len_tag` are zero; `unknown1` seems always 0x01000002; then **two** PQTZ-style 8-byte beat entries; then `entry_count` (almost always equal to the PQTZ count); three unknown values; then 2-byte entries whose meaning is unknown ("first byte seems to always have a value between 0 and 3, so maybe this is an index for a beat?") — [pyrekordbox](https://pyrekordbox.readthedocs.io/en/stable/formats/anlz.html). The crate-digger ksy defines no PQT2 type (its enum stops at PWV7/PSSI) — [ksy](https://raw.githubusercontent.com/Deep-Symmetry/crate-digger/main/src/main/kaitai/rekordbox_anlz.ksy). Field-level layout: UNVERIFIED beyond pyrekordbox's notes.

### B.4 Cues — PCOB/PCPT and PCO2/PCP2

- **PCOB**: `type` u4 @0x0C (0 = memory points, 1 = hot cues), `unk` u2 @0x10, `len_cues` u2 @0x12, `memory_count` u4 @0x14, entries from 0x18. Each **PCPT** entry is 0x38 bytes: magic `PCPT`, `len_header` 0x1C, `len_entry` 0x38, `hot_cue` u4 @0x0C (0 = memory, else A=1, B=2…), `status` u4 @0x10 (0 normal, 4 active loop), u4 always 0x00100000, `order_first` u2 @0x18 (0xFFFF for first), `order_last` u2 @0x1A (0xFFFF for last), `type` u1 @0x1C (1 = cue, 2 = loop), 3 bytes = 0x0003E8 (1000), `time` u4 @0x20 (ms), `loop_time` u4 @0x24 (ms, loop end), rest unknown — [DS](https://djl-analysis.deepsymmetry.org/rekordbox-export-analysis/anlz.html).
- **PCO2** (nxs2+): `type` u4 @0x0C, `len_cues` u2 @0x10, 2 unknown bytes, entries from 0x14. Each **PCP2** entry is variable-length: magic `PCP2`, `len_header` 0x10, `len_entry`, `hot_cue` u4 @0x0C, `type` u1 @0x10, 3 bytes 0x0003E8, `time` u4 @0x14, `loop_time` u4 @0x18, `color_id` u1 @0x1C (memory-point/loop colour table row, 0 = none), 7 bytes (01 00…), `loop_numerator` u2 @0x24, `loop_denominator` u2 @0x26 (quantised loop size as beat fraction, powers of two; 0/0 for non-quantised), `len_comment` u4 @0x28, `comment` UTF-16BE+NUL @0x2C, then 4 bytes: `color_code` (0 = default green; 01–3E index into rekordbox's 4×4 palettes, RGB table in Beat Link `CueList.findRecordboxColor`), `color_red`, `color_green`, `color_blue` (believed to be the player's RGB LED colour, "similar, but not identical" to the displayed colour). Entries may be truncated before comment/colour; parsers must tolerate it. Beat Link prefers PCO2 and falls back to PCOB — [DS](https://djl-analysis.deepsymmetry.org/rekordbox-export-analysis/anlz.html).

### B.5 Monochrome waveforms — PWAV, PWV2, PWV3

- **PWAV**: `len_preview` u4 @0x0C (400), u4 always 0x00100000, data @0x14 = 400 bytes, one byte per column: low 5 bits = height (0–31), high 3 bits = "whiteness" (higher → less saturated/whiter blue) — [DS](https://djl-analysis.deepsymmetry.org/rekordbox-export-analysis/anlz.html).
- **PWV2**: same structure, 100 bytes, only the low 4 bits used (height 0–15) — [DS](https://djl-analysis.deepsymmetry.org/rekordbox-export-analysis/anlz.html).
- **PWV3**: `len_entry_bytes` u4 (=1) @0x0C, `len_entries` u4 @0x10, u4 unknown (0x00960000) @0x14, entries @0x18; one entry per half-frame, 75 frames/s → **150 entries per second**; byte layout as PWAV — [DS](https://djl-analysis.deepsymmetry.org/rekordbox-export-analysis/anlz.html).

### B.6 Colour waveforms — PWV4, PWV5

- **PWV4** (colour preview, nxs2 + rekordbox): `len_entry_bytes`=6, `len_entries`=1200, u4 unknown, data 7200 bytes @0x18. Per-column 6 bytes (Beat Link/rekordcrate interpretation): bytes 0–1 unknown ("somehow encodes the whiteness"), byte 2 = energy in the bottom half of the spectrum (<10 kHz), byte 3 = energy in bottom third, byte 4 = middle third, byte 5 = top third of the frequency range — [rekordcrate anlz.rs `WaveformColorPreviewColumn`](https://raw.githubusercontent.com/Holzhaus/rekordcrate/main/src/anlz.rs), [DS track_metadata page](https://djl-analysis.deepsymmetry.org/djl-analysis/track_metadata.html). The displayed RGB colour is **derived at render time** from bytes 3–5 (algorithm by @jan2000 in Beat Link's `segmentColor`/`segmentHeight`); the blue preview is derived from the colour stripe plus the two unknown bytes — same DS page. Exact formula not reproduced in prose: UNVERIFIED here (read Beat Link `WaveformPreview.java`). rekordcrate's test fixture uses a frequency sweep to assert that the band bytes track low→mid→high content — [anlz.rs tests](https://raw.githubusercontent.com/Holzhaus/rekordcrate/main/src/anlz.rs).
- **PWV5** (colour detail): `len_entry_bytes`=2, `len_entries`, u4 unknown (maybe 0x00960305), entries @0x18, 150 per second. Entry = u16 BE: bits 15–13 red, 12–10 green, 9–7 blue, 6–2 height (5 bits), bits 1–0 unused — [DS](https://djl-analysis.deepsymmetry.org/rekordbox-export-analysis/anlz.html). So RGB is stored explicitly (3 bits each); rekordbox computes it from band energies at analysis time — the mapping is not documented (UNVERIFIED), but community descriptions of rekordbox RGB mode are red = lows, green/yellow = mids, blue = highs — [Reverb Reverends newsletter](https://reallychrism.substack.com/p/the-secret-language-of-waveforms) (informal).

### B.7 3-band waveforms — PWV6, PWV7, PWVC (CDJ-3000, rekordbox ≥ 6)

- rekordbox 6.0.0 (April 2020) "Added 3Band waveform" — [rekordbox.com](https://rekordbox.com/en/2020/04/rekordbox-ver-600-has-been-released); the CDJ-3000 launched (Sept 2020) with the 3Band display — [selector.news](https://selector.news/2020/09/11/pioneer-dj-cdj-3000/).
- **PWV6**: `len_entry_bytes`=3, `len_entries`=1200, data 3600 bytes @0x14 (no unknown u4). Each entry = three one-byte heights in the order **mid, high, low**; drawn stacked, lows dark blue, mids amber, highs white; "some scaling involved" — [DS](https://djl-analysis.deepsymmetry.org/rekordbox-export-analysis/anlz.html).
- **PWV7**: `len_entry_bytes`=3, `len_entries`, u4 unknown (0x00960000), entries @0x18, 150 per second; same mid/high/low byte order; scaling "seems to be non-linear" and Deep Symmetry has "not yet found an approach that perfectly matches" rekordbox; bands drawn on one axis, low+mid overlap in brown, white highs drawn last — [DS](https://djl-analysis.deepsymmetry.org/rekordbox-export-analysis/anlz.html). (DS text says .EXT for PWV7 in one sentence but the ksy and rekordcrate place it in .2EX.)
- **PWVC**: `len_header` 0x0E, `len_tag` always 20 → 6 bytes of payload, plausibly three u16 — [pyrekordbox](https://pyrekordbox.readthedocs.io/en/stable/formats/anlz.html); rekordcrate names it `Waveform3BandCalibration`, "Per-band gain calibration for the 3-band player waveform" — [anlz.rs](https://raw.githubusercontent.com/Holzhaus/rekordcrate/main/src/anlz.rs). Semantics otherwise UNVERIFIED.
- How rekordbox computes the band energies (filter types, crossover frequencies, averaging per 1/150 s, compression curve) is not documented anywhere found — UNVERIFIED. Practical approach: 3 band filters + per-window peak, then fit a per-band gain/γ curve to a rekordbox-analysed reference track (rekordcrate ships a sweep fixture `data/anlz/sweep/ANLZ0000.*` useful for this).

### B.8 Song structure — PSSI (masked)

- Body: `len_entry_bytes` u4 (=0x18), `len_entries` u2 @0x10, `mood` u2 @0x12 (1 high, 2 mid, 3 low), 6 unknown bytes, `end_beat` u2 @0x1A, 2 unknown, `bank` u1 @0x1E (0 default/Cool … 8 Club 2), 1 unknown, entries from 0x20. Entry (24 B): `index` u2, `beat` u2 (start beat), `kind` u2, `k1` u1 @0x07, `k2` u1 @0x09, `b` u1 @0x0B, `beat2` u2 @0x0C, `beat3` @0x0E, `beat4` @0x10, `k3` u1 @0x13, `fill` u1 @0x15, `beat_fill` u2 @0x16 — [DS](https://djl-analysis.deepsymmetry.org/rekordbox-export-analysis/anlz.html).
- **Obfuscation:** since rekordbox 6 the export is XOR-masked: every byte after `len_entries` is XORed with the 19-byte base pattern `CB E1 EE FA E5 EE AD EE E9 D2 E9 EB E1 E9 F3 E8 E9 F4 E1` with `len_entries` added to each byte (mod 256), repeating. Detection heuristic in the ksy: `raw_mood > 20` ⇒ masked — [ksy](https://raw.githubusercontent.com/Deep-Symmetry/crate-digger/main/src/main/kaitai/rekordbox_anlz.ksy), [DS](https://djl-analysis.deepsymmetry.org/rekordbox-export-analysis/anlz.html). Phrase labels per mood and the high-mood k1/k2/k3 variants are tabulated on the DS page.

### B.9 Device Library Plus / OneLibrary

- "Device Library Plus" was renamed **OneLibrary**; it is a SQLCipher-encrypted SQLite `exportLibrary.db` with a subset of master.db tables (`content`, `cue`, `playlist`, …); the `content` table keeps an `analysisDataFilePath` column — [pyrekordbox OneLibrary](https://pyrekordbox.readthedocs.io/en/latest/formats/onelibrary.html), [rbox README](https://cdn.jsdelivr.net/npm/rbox-js@0.1.7/README.md). Supported by CDJ-3000X, OPUS-QUAD, OMNIS-DUO, XDJ-AZ; legacy Device Library (export.pdb) covers CDJ-3000 and older; rekordbox 7.2.11+ writes **both** formats on export — [AlphaTheta notice](https://alphatheta.com/en/information/important-notice-for-customers-using-usb-devices-with-our-dj-equipment/).
- No source documents any change to the ANLZ tag formats for OneLibrary; rbox claims to parse/write all three ANLZ types for current exports — [rbox README](https://cdn.jsdelivr.net/npm/rbox-js@0.1.7/README.md). Conclusion: ANLZ .DAT/.EXT/.2EX appear unchanged; only the database changed — UNVERIFIED beyond absence of evidence.

### B.10 Rust tooling for ANLZ

- `rekordcrate` 0.3.0 (MPL-2.0, binrw-based, reads and writes PQTZ/PCOB/PCO2/PWAV…PWV7/PWVC/PSSI, repo pushed 2026-10-05) — [crates.io](https://crates.io/api/v1/crates/rekordcrate), [anlz.rs](https://raw.githubusercontent.com/Holzhaus/rekordcrate/main/src/anlz.rs). MPL-2.0 is GPLv3-compatible, so it can be used directly for import/export.

---

## Sources

- Deep Symmetry, Analysis Files — https://djl-analysis.deepsymmetry.org/rekordbox-export-analysis/anlz.html
- Deep Symmetry, Track Metadata / waveform requests — https://djl-analysis.deepsymmetry.org/djl-analysis/track_metadata.html
- crate-digger rekordbox_anlz.ksy — https://raw.githubusercontent.com/Deep-Symmetry/crate-digger/main/src/main/kaitai/rekordbox_anlz.ksy
- pyrekordbox ANLZ format — https://pyrekordbox.readthedocs.io/en/stable/formats/anlz.html ; OneLibrary — https://pyrekordbox.readthedocs.io/en/latest/formats/onelibrary.html
- rekordcrate anlz.rs — https://raw.githubusercontent.com/Holzhaus/rekordcrate/main/src/anlz.rs ; crates.io — https://crates.io/api/v1/crates/rekordcrate
- rbox README — https://cdn.jsdelivr.net/npm/rbox-js@0.1.7/README.md
- AlphaTheta OneLibrary notice — https://alphatheta.com/en/information/important-notice-for-customers-using-usb-devices-with-our-dj-equipment/
- rekordbox 6.0.0 release — https://rekordbox.com/en/2020/04/rekordbox-ver-600-has-been-released ; CDJ-3000 launch — https://selector.news/2020/09/11/pioneer-dj-cdj-3000/
- Mixxx sources: LICENSE — https://raw.githubusercontent.com/mixxxdj/mixxx/main/LICENSE ; analyzerbeats.cpp — https://raw.githubusercontent.com/mixxxdj/mixxx/main/src/analyzer/analyzerbeats.cpp ; analyzerqueenmarybeats.cpp — https://raw.githubusercontent.com/mixxxdj/mixxx/main/src/analyzer/plugins/analyzerqueenmarybeats.cpp ; beatutils.cpp — https://raw.githubusercontent.com/mixxxdj/mixxx/main/src/track/beatutils.cpp ; analyzerkey.cpp — https://raw.githubusercontent.com/mixxxdj/mixxx/main/src/analyzer/analyzerkey.cpp ; analyzerkeyfinder.cpp — https://raw.githubusercontent.com/mixxxdj/mixxx/main/src/analyzer/plugins/analyzerkeyfinder.cpp ; analyzerqueenmarykey.cpp — https://raw.githubusercontent.com/mixxxdj/mixxx/main/src/analyzer/plugins/analyzerqueenmarykey.cpp ; analyzerwaveform.cpp — https://raw.githubusercontent.com/mixxxdj/mixxx/main/src/analyzer/analyzerwaveform.cpp
- Mixxx manual 2.7 Beat Detection — https://manual.mixxx.org/2.7/en/chapters/preferences/beat_detection ; Mixxx PR #2119 — https://github.com/mixxxdj/mixxx/pull/2119 ; Mixxx analyzer news — https://mixxx.org/news/tag/analyzer
- qm-dsp README/COPYING — https://raw.githubusercontent.com/c4dm/qm-dsp/master/README.md , https://raw.githubusercontent.com/c4dm/qm-dsp/master/COPYING ; TempoTrackV2.h — https://code.soundsoftware.ac.uk/hg/qm-dsp/file/tip/dsp/tempotracking/TempoTrackV2.h ; Ardour qm-dsp README — https://git.ardour.org/ardour/ardour/src/branch/master/libs/qm-dsp/README.txt ; qm-vamp-plugins docs — https://vamp-plugins.org/plugin-doc/qm-vamp-plugins.html
- libkeyfinder README — https://raw.githubusercontent.com/mixxxdj/libkeyfinder/main/README.md ; libkeyfinder-sys — https://crates.io/api/v1/crates/libkeyfinder-sys , https://raw.githubusercontent.com/evanpurkhiser/libkeyfinder-sys/main/build.rs , https://github.com/evanpurkhiser/libkeyfinder-sys , https://docs.rs/libkeyfinder-sys ; Arch package — https://www.archlinux.org/packages/extra/x86_64/libkeyfinder
- Essentia licensing — https://essentia.upf.edu/licensing_information.html ; RhythmExtractor2013 — https://essentia.upf.edu/reference/std_RhythmExtractor2013.html ; KeyExtractor — https://essentia.upf.edu/reference/std_KeyExtractor.html ; Key — https://essentia.upf.edu/reference/std_Key.html ; Faraldo 2016 — https://repositori.upf.edu/items/7ed52987-edb2-48a6-90e6-674a34a54ed0/full ; GiantSteps datasets — https://repositori.upf.edu/items/4e5f12d8-9dc0-43c5-a5e6-337c1e7184c2/full ; essentia-sys — https://crates.io/api/v1/crates/essentia-sys , https://docs.rs/crate/essentia-sys/latest
- aubio — https://github.com/aubio/aubio , https://raw.githubusercontent.com/aubio/aubio/master/src/tempo/beattracking.c ; aubio-rs — https://crates.io/api/v1/crates/aubio-rs , https://raw.githubusercontent.com/katyo/aubio-rs/master/README.md
- bliss-rs — https://github.com/Polochon-street/bliss-rs , https://raw.githubusercontent.com/Polochon-street/bliss-rs/master/Cargo.toml , https://raw.githubusercontent.com/Polochon-street/bliss-rs/master/src/lib.rs , https://crates.io/api/v1/crates/bliss-audio
- madmom — https://github.com/CPJKU/madmom ; BeatNet — https://github.com/mjhydri/BeatNet ; Beat This! — https://github.com/CPJKU/beat_this , https://arxiv.org/html/2407.21658 ; beat-this crate — https://docs.rs/crate/beat-this/latest , https://crates.io/api/v1/crates/beat-this , https://raw.githubusercontent.com/danigb/beat-this-rs/main/LICENSE ; HF ONNX — https://huggingface.co/musetric/beat-this-onnx
- ort — https://crates.io/api/v1/crates/ort ; rten — https://crates.io/api/v1/crates/rten ; tract-onnx — https://crates.io/api/v1/crates/tract-onnx
- Rust crates: rustfft https://crates.io/api/v1/crates/rustfft ; realfft https://crates.io/api/v1/crates/realfft ; symphonia https://crates.io/api/v1/crates/symphonia ; mel-spec https://crates.io/api/v1/crates/mel-spec ; pitch-detection https://crates.io/api/v1/crates/pitch-detection ; stratum-dsp https://crates.io/api/v1/crates/stratum-dsp ; chord_detector https://crates.io/api/v1/crates/chord_detector ; oximedia-mir https://crates.io/api/v1/crates/oximedia-mir ; soundtouch https://crates.io/api/v1/crates/soundtouch
- SoundTouch licence (Debian) — https://metadata.ftp-master.debian.org/changelogs/main/s/soundtouch/stable_copyright ; BeatRoot Vamp README — https://code.soundsoftware.ac.uk/hg/beatroot-vamp/file/tip/README ; LibXtract LICENSE — https://raw.githubusercontent.com/jamiebullock/LibXtract/master/LICENSE
- GNU licence list — https://www.gnu.org/licenses/license-list.html
- Evaluations/community: Krebs ISMIR 2013 — https://www.cp.jku.at/people/krebs/ISMIR2013.html ; r/Beatmatch 2023 — https://lr.us.psf.lt/r/Beatmatch/comments/12rorli/mixxx_is_the_best_bpm_analyser ; r/DJs key comparison 2021 — https://reddit.birdcat.cafe/r/DJs/comments/rn2wu5/key_detection_comparison_2021 ; DubSpot key tag — https://blog.dubspot.com/tag/key-detection ; Cuepoint vs rekordbox — https://uk.linkedin.com/in/chuksjn ; waveform colour explainer — https://reallychrism.substack.com/p/the-secret-language-of-waveforms
- GitHub API repo metadata (pushed_at/licence) for c4dm/qm-dsp, c4dm/qm-vamp-plugins, aubio/aubio, mixxxdj/libkeyfinder, Polochon-street/bliss-rs, CPJKU/beat_this, CPJKU/madmom, mjhydri/BeatNet, MTG/essentia, danigb/beat-this-rs, evanpurkhiser/libkeyfinder-sys, lagmoellertim/essentia-rs, katyo/aubio-rs, Holzhaus/rekordcrate, Deep-Symmetry/crate-digger, jamiebullock/LibXtract, pykeio/ort — https://api.github.com/repos/<owner>/<repo> (queried 2026-10-18)
