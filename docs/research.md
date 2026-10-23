# Research findings

Research date: 2026-10-18. This file is the synthesised, decision-relevant
summary. The full per-topic reports with every source are in
`docs/research/raw-*.md`; read those before re-litigating a finding. Items
that could not be verified against a current primary source are marked
**UNVERIFIED**. Nothing here has been tested by us on hardware yet.

Contents
1. Reusable projects for rekordbox formats
2. OneLibrary (`exportLibrary.db`)
3. Audio analysis options and Pioneer ANLZ encodings
4. Known USB export pitfalls
5. Engine DJ library format
6. Tauri 2 specifics
7. rekordbox XML (`collection.xml`)
8. Consolidated list of unverified items
9. DJ mixer reference and real-time audio crates (Phase 5)
10. Play history and playlist dividers (Phases 2 and 2.5)
11. File conversion (Phase 2.6)
12. rekordbox pain points and reks gaps

---

## 1. Reusable projects for rekordbox formats

Headline: as of October 2026 there are **three independent, maintained Rust
writers** for `export.pdb` + ANLZ. In January 2025 a Mixxx developer still
said writing was infeasible; that is no longer true.

| Project | Lang | Licence | GPLv3 OK | Reads | Writes | Activity | As Rust dep? |
|---|---|---|---|---|---|---|---|
| baken / `baken-export` (M-Igashi) | Rust | MIT | yes | `collection.xml`, rekordbox local ANLZ cache, MySettings, audio | `export.pdb`, ANLZ DAT/EXT/2EX (copied or generated), `exportLibrary.db` (opt-in), settings, artwork | v4.6.1 2026-10-12, pushed 2026-10-17 | **yes** (crates.io) |
| rbxport (chrisle) | Rust/Tauri | GPL-2.0-or-later | yes | rekordbox `master.db`, USB sticks | `export.pdb`, `exportExt.pdb`, ANLZ, `exportLibrary.db`, `.sync`, artwork, settings | v1.9.0 2026-10-16; 109 open issues | git path deps only (`rbl-pdb`, `rbl-anlz`, `rbl-onelibrary`; not on crates.io, verified) |
| rbox (dylanljones) | Rust | GPL-3.0-only (≥0.1.6) | yes | `master.db`, `exportLibrary.db`, XML, ANLZ, settings | OneLibrary, ANLZ (modify/save), XML, settings; **no `export.pdb`** | 0.1.8 2026-10-11; GitHub repo 404 | yes, but not for PDB |
| rekordcrate (Holzhaus) | Rust | MPL-2.0 | yes | `export.pdb`, `exportExt.pdb` (main), ANLZ, settings | ANLZ (`BinWrite` in 0.3.0); PDB writer only on `main`/open PR #259 | 0.3.0 2025-01-23; pushed 2026-10-05 | yes for **reading/validation** |
| crate-digger (Deep Symmetry) | Java + Kaitai | EPL-2.0 with MPL-2.0/LGPL-3.0 secondary | yes (secondary licence; explicitly extended to generated parsers, Sept 2026) | PDB, exportExt, ANLZ | nothing | v0.2.1 2025-07; docs through Dec 2025 | spec of record (`.ksy`) |
| pyrekordbox (dylanljones) | Python | MIT | yes | `master.db`, XML, ANLZ, MySettings, OneLibrary | XML, MySettings, OneLibrary (0.5.0), ANLZ modify; **no `export.pdb`** | 0.5.0 2026-10-14 | reference only |
| rex (kimtore) | Go | **none** | no | Mixxx DB | partial `export.pdb` (no grids/cues/waveforms) | pushed 2025-08 | unusable (unlicensed) |
| Mixxx | C++ | GPL-2.0-or-later | yes | PDB + ANLZ via Kaitai C++ | nothing | active | reference (analyzers, importer) |
| dj-usb-tkit (haivala) | Rust/Tauri | AGPL-3.0-or-later | combinable, but AGPL would govern | rekordbox/Mixxx libs | own PDB writer, Engine (opt) | pushed 2026-10-16 | not a library |
| export-studio, djbooth | Rust/Electron, TS/Rust | GPL-2.0 (vendors rbxport) / none | — | — | vendored rbxport / unlicensed | 2026-10 | no |

Details that matter
- **baken-export** is the only writer with public, dated, real-player
  confirmations: CDJ-3000 fw 3.22 and CDJ-2000NXS2 fw 1.85/1.87 (issue #116,
  closed 2026-09-29), CDJ-2000NXS fw 1.44 (issue #242, closed 2026-10-14).
  Fixes on the way there: string/track-count alignment to stop an NXS2 freeze
  on non-ASCII artist names (#168), key names (#189/#190), hot cues D–H
  (#205). Open oddity: `E-8307 DEVICE NO RESPONSE` on first insert on a
  CDJ-3000. Its public API (verified on docs.rs): `plan(&Options) ->
  Result<Plan>`, `export(...) -> Report`; `Options { xml: PathBuf, device:
  PathBuf, playlists: Vec<String>, anlz_roots: Vec<PathBuf>, settings_dir,
  no_settings, device_name, cdjsafe, generate_analysis, prune, onelibrary,
  artwork }`. Input is a `collection.xml` **file path**; ANLZ files are copied
  from rekordbox's local cache (`anlz_roots`, default = rekordbox's locations
  on this machine) or generated from audio with `generate_analysis`
  (scrolling waveform "pixel-matching", colour/3-band/preview "close
  approximations", no PSSI phrase data). `prune` deletes unreferenced audio
  and analysis on the stick. Dependencies (verified on crates.io): baken-core
  (MIT; depends on `mp3rgain` MIT, `symphonia` MPL-2.0), rusqlite, quick-xml,
  image, aes/cbc/hmac/pbkdf2/sha2 (own SQLCipher page encryption), rayon.
  All GPLv3-compatible. Docs coverage 47%, no examples, single maintainer who
  also sells a paid Mac App Store GUI around the same engine. FAT32+MBR
  recommended; "don't layer expressport over a stick rekordbox has written".
- **rbxport** has the most thorough format notes (`docs/reference/usb-export-db.md`:
  exportExt.pdb, `.sync` files, DEVSETTING bytes, OneLibrary open order) and
  byte-parity tests against rekordbox 7.2.11 (556/562 files identical), but
  hardware validation is mostly a firmware simulator plus one XDJ-AZ check,
  and users have reported CDJ-2000NXS/CDJ-900NXS lockups.
- **rekordcrate** `main` has merged row insertion (PR #220, Mar 2026) and an
  open PR #259 (Jul 2026) "create exports from scratch" (PDB + settings, no
  ANLZ). Hardware note from that PR: a CDJ-350 crashes in the TRACK menu if a
  track row is shorter than 221 bytes. Best-documented parser; use it to
  parse what we write. 0.3.0 limits found 2026-10-19 (Phase 2 task 7): PDB
  row fields are private (no getters), and its ANLZ `CueType` accepts only 0
  and 2, so it rejects the `PCPT` type 1 that baken writes for a point cue.
- **crate-digger** licence was clarified in Sept 2026 (issues #49/#50):
  `.ksy` files are `EPL-2.0 OR MPL-2.0 OR LGPL-3.0-only` and the maintainer
  states generated parsers carry the same election. Safe for GPL.
- Deep Symmetry's docs (`djl-analysis.deepsymmetry.org/rekordbox-export-analysis/`)
  cover PDB (header, pages, index pages, 14 row types, DeviceSQL strings,
  exportExt tags) and ANLZ (PQTZ, PCOB, PCO2, PPTH, PVBR, PWAV, PWV2–7,
  PSSI). No OneLibrary coverage anywhere in Deep Symmetry's work.
- "REX" = kimtore/rex (unlicensed Go). "Unbox", "djsync": no such projects
  found. Lexicon DJ is proprietary and hands USB writing to rekordbox via
  XML; no evidence it writes `exportLibrary.db` (**UNVERIFIED** negative).

Sources: `docs/research/raw-1-projects.md` (76 URLs). Key ones:
https://github.com/M-Igashi/baken · https://docs.rs/baken-export/latest/baken_export/ ·
https://github.com/M-Igashi/baken/issues/116 · https://github.com/M-Igashi/baken/issues/242 ·
https://github.com/chrisle/rbxport/blob/main/docs/reference/usb-export-db.md ·
https://github.com/Holzhaus/rekordcrate/pull/259 · https://github.com/Deep-Symmetry/crate-digger/issues/50 ·
https://djl-analysis.deepsymmetry.org/rekordbox-export-analysis/exports.html ·
https://github.com/dylanljones/pyrekordbox · https://github.com/kimtore/rex

---

## 2. OneLibrary (`exportLibrary.db`)

- OneLibrary = "Device Library Plus", renamed in rekordbox 7.2.5
  (2025-10-21). File: `PIONEER/rekordbox/exportLibrary.db`, a SQLite DB
  encrypted with SQLCipher (v4-style). It sits **beside** `export.pdb`,
  `exportExt.pdb` and ANLZ. rekordbox 6.8.1+ and all 7.x write both libraries
  on every export; there is no toggle. Grids, waveforms and cues are not in
  the DB (they stay in ANLZ); rekordbox exports the `cue` table empty.
- Official player split (AlphaTheta notice 2026-03-18, rekordbox FAQ):
  - OneLibrary only: CDJ-3000X, CDJ-1500X, OPUS-QUAD, OMNIS-DUO, XDJ-AZ, XDJ-AN.
  - Device Library (legacy) only: CDJ-3000 (fw 3.30 added OneLibrary and was
    withdrawn in Jan 2026; current 3.22 ignores it), XDJ-XZ, XDJ-RX3, XDJ-RR,
    XDJ-1000MK2, XDJ-700, CDJ-900NXS2, CDJ-2000NXS2 and older.
  - AlphaTheta (2026-01-07): "reviewing the specifications of both Device
    Library and OneLibrary"; a CDJ-3000X firmware with "more flexible library
    loading" was promised for March 2026 (**UNVERIFIED** whether shipped).
- Encryption: the passphrase is static (same on every stick, not licence or
  machine bound) and has been published by pyrekordbox, onelibrary-connect,
  DjManager issue #300 and others. libdjinterop and Mixxx deliberately do not
  ship it (caller-supplied). Cipher parameters conflict across sources: baken
  PR #259 says a stock SQLCipher 4 profile (AES-256-CBC, PBKDF2-HMAC-SHA512
  256k rounds, HMAC-SHA512, 16-byte salt, 80 reserved bytes/page) opens a
  rekordbox file and that a stock SQLCipher 4.14 opened baken's output with
  the passphrase alone; DjManager #300 reports the standard profiles failed
  and loaded rekordbox's own `sqlite3.dll`. Likely a passphrase-vs-raw-key
  difference. **UNVERIFIED** until tested against a real 7.2.13+ export.
  Tested 2026-10-20 (Phase 4 task 2) on rekordbox 7.2.19.0342's export of
  the fixture tracks (`fixtures/onelibrary/ref-7.2.19/`): stock SQLCipher 4
  with the passphrase opens it, `integrity_check` ok. rekordbox leaves the
  file in WAL mode with a 910 KiB `-wal` beside a 118 KiB `.db`.
  `property.dbVersion` is `"1000"`; every `content` value equals the
  `export.pdb` track row's (D14).
- Schema (22 tables, 4 indexes): `property`, `content` (~40 columns incl.
  `path`, `fileName`, `analysisDataFilePath`, `analysedBits`, `contentLink`,
  `masterDbId`, `masterContentId`, `bpmx100`, `length`, `key_id`…), lookups
  `artist/album/genre/label/key/color/image`, `playlist`, `playlist_content`,
  `cue`, `history`, `history_content`, `hotCueBankList(_cue)`, `myTag(_content)`,
  `menuItem`, `category`, `sort`, `recommendedLike`. Seed rows for
  `menuItem/category/sort/color/myTag` and the `property.dbVersion` value
  (`1000` vs `'10000'` reported) must be copied from a real export.
- Open-source status: read support in pyrekordbox, rbox, onelibrary-connect,
  libdjinterop (PR #196 merged 2026-10-14), rekordcrate (PR #269 open),
  rekordlib (Zig). Write support claimed by pyrekordbox 0.5.0 (ORM), rbox,
  rekordlib, rbxport, baken (`--onelibrary`). **No open-source project has
  published a success on a real OneLibrary player**; baken's README says so
  explicitly. Commercial sanctioned writers: djay Pro (launch partner),
  VirtualDJ (sanctioned Sept 2026). Traktor "coming".
- Risks for a non-partner app: (1) DMCA §1201 trafficking exposure vs the
  §1201(f) interoperability defence; no takedowns found but Mixxx chose not
  to ship the key; (2) closed and churning spec; (3) AlphaTheta's own stack
  broke twice (CDJ-3000 fw 3.30, rekordbox 7.2.12 withdrawn over export
  bugs); (4) no evidence of signatures beyond SQLCipher's per-page HMAC, but
  also no evidence a third-party DB loads; (5) rekordbox rewrites device
  libraries it opens and shows an "out of matching" caution when the two
  libraries differ.

Sources: `docs/research/raw-2-onelibrary.md` (45 URLs). Key ones:
https://alphatheta.com/en/information/important-notice-for-customers-using-usb-devices-with-our-dj-equipment/ ·
https://rekordbox.com/en/support/faq/onelibrary-7/ ·
https://www.pioneerdj.com/en/news/2026/cdj-3000-firmware-ver330-important-notice/ ·
https://rekordbox.com/en/2026/03/important-notice-regarding-rekordbox-v7212/ ·
https://pyrekordbox.readthedocs.io/en/latest/formats/onelibrary.html ·
https://github.com/xsco/libdjinterop/pull/196 · https://github.com/Radexito/DjManager/issues/300 ·
https://github.com/mixxxdj/proposals/issues/20 · https://www.law.cornell.edu/uscode/text/17/1201

---

## 3. Audio analysis options and Pioneer ANLZ encodings

### 3.1 Licences

GPL-2.0-or-later (qm-dsp, Mixxx), GPL-3.0(+) (libKeyFinder, aubio, bliss),
LGPL-2.1+ (SoundTouch), MPL-2.0 (Symphonia, rekordcrate), MIT/Apache
(rustfft, rten, ort, beat-this, Beat This! weights) are all usable in a GPLv3
app. AGPL-3.0 (Essentia) is combinable but the Essentia parts stay AGPL and
its pretrained models are CC BY-NC-ND (not redistributable). madmom's code is
BSD but its models are CC BY-NC-SA (not redistributable in a free app).

### 3.2 Candidates

| Candidate | What | Licence | Rust route | Maintenance | Notes |
|---|---|---|---|---|---|
| Beat This! (CPJKU, ISMIR 2024) via `beat-this` 1.1.0 crate | neural beat **and downbeat** tracker | MIT (code + weights); crate MIT | pure Rust via `rten` 0.27 (MIT/Apache), optional `ort` | crate 2026-10-02 (verified) | GTZAN beat F1 89.1 / downbeat 78.3; EDM subset HJDB 96.6% downbeat F1; small model ~10 MB; crate claims F-measure parity with Python; 4:32 track in 4.6 s on M4 Pro. Deps verified: rten, rten-tensor, rubato, anyhow; symphonia/ort optional |
| qm-dsp TempoTrackV2 (+ Mixxx `BeatUtils`) | Davies & Plumbley beat tracker; Mixxx default | GPL-2.0-or-later | vendor C++ via `cxx`/`cc`, or port (~few kLOC) | frozen since 2022, stable | beats only, no downbeats; Mixxx makes a constant grid with 25 ms phase tolerance, ≥16-beat regions, BPM snapped to integer/0.5; known to grid on snares for house |
| aubio tempo via `aubio-lib` | Davies lineage | GPL-3.0+ | stale bindings (2021) | 0.4.9 | fallback only |
| Essentia RhythmExtractor2013 / KeyExtractor (`edma` profiles) | best EDM key profiles in literature | AGPL-3.0 | `essentia-sys` 0.1.5 (MIT shim), heavy C++ build | active | not worth it for a Tauri bundle |
| libKeyFinder (Mixxx-maintained) | chromagram + Shaath profiles | GPL-3.0-or-later | `libkeyfinder-sys` 0.1.0 (cxx, needs **system** lib via pkg-config; docs.rs build fails) | last push 2024-11 | no formal accuracy numbers; community tests show tools disagree often |
| qm-dsp GetKeyMode | Bach-derived profiles; Mixxx default key | GPL-2.0+ | same vendoring as above | frozen | second opinion |
| Rust-native: `stratum-dsp` 1.0.0 (MIT/Apache) "BPM/key/beat tracking for DJ apps" | | | | 2025-12 | accuracy **UNVERIFIED**, unproven |
| `bliss-audio` 0.13 | similarity descriptors; contains a GPL-3.0 Rust port of aubio tempo code | GPL-3.0-only | | 2026-08 | not a DJ analyser; borrowable code |

No crates named `bpm-detection`, `musical-key`, `chromagram`, `keyfinder`
exist. Matching rekordbox: its analyser is proprietary; the only quantified
open comparison (Cuepoint, 60 tracks, self-reported) got 92% tempo, 70% grid
on-beat, 62% downbeat agreement. Mixxx PR #2119 found a ~26/52 ms offset
between rekordbox cue positions and other MP3 decoders tied to Xing/LAME
header handling; decoder delay must be calibrated against a reference export.

### 3.3 ANLZ file format (big-endian throughout)

- Three files per track share a base name: `ANLZ0000.DAT` (all players),
  `.EXT` (NXS2+: colour waveforms, extended cues), `.2EX` (CDJ-3000: 3-band).
  Header `PMAI`, `len_header` 0x1C, `len_file`; then sections `fourcc,
  len_header, len_tag`.

| Section | File | Entry | Rate | Content |
|---|---|---|---|---|
| PPTH | all, **must be first** | — | — | `len_path` + UTF-16BE NUL-terminated USB-root path of the audio file (`/Contents/...`) |
| PVBR | DAT | u32×400 | — | VBR seek index; `len_tag` 1620; structure largely unknown |
| PQTZ | DAT | 8 B | every beat | `beat_number` u16 (1–4), `tempo` u16 = BPM×100, `time` u32 ms. A full beat list, so dynamic grids are representable |
| PQT2 | EXT | 2 B | every beat | `len_header` 0x38; two PQTZ-style entries then count then 2-byte entries of unknown meaning (**UNVERIFIED** layout; not in crate-digger ksy) |
| PCOB | DAT, EXT | PCPT 0x38 B | — | `type` 0 memory / 1 hot; entry: `hot_cue` (0 memory, 1=A…), `status` (4 = active loop), `order_first/last`, `type` 1 cue / 2 loop, `time` ms, `loop_time` ms |
| PCO2 | EXT | PCP2 variable | — | adds `color_id`, `loop_numerator/denominator`, UTF-16BE comment, `color_code` + RGB; entries may be truncated; Beat Link prefers PCO2 |
| PWAV | DAT | 1 B ×400 | whole track | low 5 bits height, high 3 bits "whiteness" |
| PWV2 | DAT | 1 B ×100 | whole track | 4-bit height (CDJ-900) |
| PWV3 | EXT | 1 B | 150/s (half-frames at 75 fps) | as PWAV |
| PWV4 | EXT | 6 B ×1200 | whole track | 2 unknown "whiteness" bytes, energy <10 kHz, energies of bottom/mid/top thirds; colour derived at render time (Beat Link `segmentColor`) |
| PWV5 | EXT | u16 | 150/s | bits 15–13 R, 12–10 G, 9–7 B, 6–2 height |
| PWV6 | 2EX | 3 B ×1200 | whole track | mid, high, low heights; non-linear scaling undocumented |
| PWV7 | 2EX | 3 B | 150/s | same; Deep Symmetry has not matched rekordbox's scaling |
| PWVC | 2EX | 6 B | — | per-band calibration (rekordcrate name); semantics **UNVERIFIED** |
| PSSI | EXT | 24 B/phrase | — | song structure; XOR-masked since rekordbox 6 with a 19-byte pattern offset by `len_entries`; optional |

How rekordbox computes band energies (filters, crossovers, compression) is
undocumented (**UNVERIFIED**). Practical approach used by others: three band
filters + per-window peak, then fit per-band gain/gamma against a rekordbox
reference (rekordcrate ships a frequency-sweep fixture). Mixxx uses 4th-order
Bessel filters at 600 Hz / 4 kHz for its own waveform. rekordcrate 0.3.0
reads and writes all these sections from Rust.

Observed 2026-10-20 in rekordcrate 0.3.0's test data (rekordbox 6 export,
`data/complete_export/demo_tracks/PIONEER/USBANLZ`, two tracks): PWAV/PWV2
header word is 0x00010000 (docs say 0x00100000); PWV3/PWV7 0x00960000;
PWV5 0x00960305; PWV4 0; PWV6 has no such word (`len_header` 0x14); PWVC
`len_header` 0x0E with one zero u16, then three u16 that differ per track
(91/138/396, 145/249/500). Leading silence in PWV3 is 0xE0. Value ranges:
PWAV height ≤ 24, PWV3 ≤ 31, PWV4 bytes 2–5 ≤ ~120, PWV6 ≤ ~43, PWV7 ≤ 127.
Correction to the line above: rekordcrate 0.3.0 has no PWV6/PWV7/PWVC
types, can't construct its waveform structs from outside the crate (private
fields), and its PWV2/PWV5 bitfields disagree with the layout above (D9
finding).

Measured 2026-10-20 on rekordbox 6's local cache (50 tracks, phase 3 task 8,
[analysis-parity/waveforms.md](analysis-parity/waveforms.md)): PWV6 and PWV7
bytes are low, mid, high in that order, not as in the table above; PWVC holds
the per-track band gains ×100 (low, mid, high) applied to PWV6/PWV7, ranging
0.80–2.37, 1.00–3.00 and 1.01–5.00 on that set; PWV7 ≤ 127, PWV6 ≤ 115.

Sources: `docs/research/raw-3-analysis-anlz.md`. Key ones:
https://djl-analysis.deepsymmetry.org/rekordbox-export-analysis/anlz.html ·
https://raw.githubusercontent.com/Deep-Symmetry/crate-digger/main/src/main/kaitai/rekordbox_anlz.ksy ·
https://pyrekordbox.readthedocs.io/en/stable/formats/anlz.html ·
https://github.com/CPJKU/beat_this · https://arxiv.org/html/2407.21658 ·
https://docs.rs/crate/beat-this/latest · https://raw.githubusercontent.com/mixxxdj/mixxx/main/src/track/beatutils.cpp ·
https://raw.githubusercontent.com/mixxxdj/libkeyfinder/main/README.md ·
https://github.com/mixxxdj/mixxx/pull/2119 · https://essentia.upf.edu/licensing_information.html

---

## 4. Known USB export pitfalls

Filesystem and media
- FAT32 (and HFS+ on Mac) is accepted by every player. exFAT is officially
  supported only on CDJ-3000 (fw 1.20+), XDJ-XZ (1.23+), XDJ-RX3 (1.11+),
  CDJ-3000X, CDJ-1500X, OPUS-QUAD, OMNIS-DUO, XDJ-AZ, XDJ-AN. CDJ-2000NXS2
  and older: no exFAT. NTFS never. Only the first partition is used
  (official). MBR vs GPT is not stated officially; third parties report GPT
  sticks not mounting (**UNVERIFIED**). Default to FAT32 + single MBR partition.
- FAT32: files < 4 GB; Windows GUI formats FAT32 only ≤ 32 GB. Official
  limits (XDJ-RR manual, assumed same family): 8 folder levels, 10,000
  files and 10,000 folders per folder; non-music files slow mounting.
  Lexicon: 255-character full-path limit (third-party). Avoid `: * ? < > | ;`
  and FAT32-illegal characters. NFC vs NFD, case sensitivity, 8.3 names: no
  Pioneer source (**UNVERIFIED**); policy: write NFC, store the identical
  string in `file_path` and PPTH, avoid case-only collisions.
- Layout: `PIONEER/rekordbox/export.pdb` (+ `exportExt.pdb`),
  `PIONEER/USBANLZ/P0xx/0000xxxx/ANLZ0000.{DAT,EXT,2EX}`, audio under
  `/Contents/` (commonly `<Artist>/<Album>/<file>`; players follow
  `file_path`, so any layout within limits works), `PIONEER/Artwork/...`
  (80 px and 240 px `_m` variants), settings `PIONEER/{DEV,MY,MY2,DJMMY}SETTING.DAT`.
  A third-party Rust port documents the USBANLZ folder as a hash of the UTF-16
  audio path (`h=(h*0x5BC9+c)*0x93B5+c`, `% 200003`, P = 7 scattered bits) and
  claims players recompute it rather than trusting `analyze_path`
  (**UNVERIFIED**; conflicts with Deep Symmetry; do both).

PDB breakers (crate-digger issue #46 hardware-verified writer; djl-analysis; rekordcrate PR #220 CDJ-350 success)
- Page size 4096; header byte 0x10 must be 5; all 20 table directory entries
  (types 0–19) must exist even when empty; index pages need
  `free_size=0`, `0x1FFF`, `0x1FFF`, `0x03EC`.
- Page header bytes 0x18–0x1A are one packed 24-bit field (13-bit
  `num_row_offsets` | 11-bit `num_rows`); splitting it makes CDJs skip the page.
- Row subtypes: track 0x0024, artist 0x0060, album 0x0080 (far variants
  +4); wrong subtypes make rows invisible. Row-present bitmask per 16-row
  group. Long strings UTF-16LE (0x90) or ASCII (0x40); short ASCII odd length
  byte; ISRC special. Dates are strings. 21 track string slots (14
  `analyze_path`, 20 `file_path`). Track rows under 221 bytes crash a CDJ-350.
- `file_type` codes: MP3 1, M4A 4, FLAC 5, WAV 0x0b, AIFF 0x0c; set real
  bitrate/rate/depth/size/duration. Colour IDs fixed 1–8.

ANLZ
- PPTH must be the first tag after PMAI or the player will not link the
  analysis. Minimal `.DAT`: PPTH, PVBR, PQTZ, PWAV, PWV2, PCOB×2. `.EXT`:
  PWV3/4/5 + PCO2×2 (NXS2 colours/comments/quantised loops). `.2EX`: PWV6/7
  for CDJ-3000. Without a grid the player has no quantise. Write both PCOB
  and PCO2. rekordbox refuses two memory cues at the same position. MP3 cue
  offsets of ~26/52 ms tied to Xing/LAME headers must be calibrated.

Audio file gotchas (and what Lexicon auto-fixes)
- E-8305 "unsupported file format" is typically `WAVE_FORMAT_EXTENSIBLE` or
  float WAV ("rekordbox ignores it, but the CDJs don't"). Lexicon fixes:
  WAV_EXTENSIBLE, 32-bit float WAV, rates > 48 kHz or < 32 kHz, VBR MP3
  without Xing, > 2 channels, AIFF-C. Official support: MP3 32–320 kbps
  32/44.1/48 kHz; AAC-LC m4a/aac/mp4; WAV/AIFF 16/24-bit 44.1/48 kHz.
  FLAC/ALAC only on CDJ-2000NXS2 (to 96 kHz/24-bit), XDJ-1000MK2 (≤ 48 kHz),
  XDJ-XZ (firmware), CDJ-3000; not CDJ-2000NXS. Displayed metadata comes from
  the DB, not tags.

Stale files, rekordbox reactions, limits
- Players never delete. rekordbox deletes tracks from a device only via its
  Delete key / "Delete Tracks" preference; whether audio/ANLZ are physically
  removed is undocumented; Lexicon exposes a separate "Clean up unused
  files". Files not in the DB are invisible in library browse, visible in
  FOLDER browse without grid/waveform.
- The exact string "The library on this device is inconsistent" was not
  found in any official source (**UNVERIFIED**). rekordbox 7.2.11+ shows a
  caution when OneLibrary and Device Library are "out of matching", fixed by
  "Convert from Device Library". A legacy-only stick triggers that flow.
- Official error codes: E-8302 cannot read/corrupt, E-8304/E-8305 unsupported
  format. Third-party mappings (E-8305 = "PIONEER folder missing") are not official.
- Lexicon writes both libraries without rekordbox and still says "use at
  your own risk, always carry a rekordbox-made backup"; recommends
  initialising the stick in rekordbox first so settings files exist.
- Playlist limit of 1,000 tracks is forum consensus (not official); old
  10,000-track caps are unverified/obsolete. Mount time scales with file and
  folder count.

Sources: `docs/research/raw-4-export-pitfalls.md` (55 URLs). Key ones:
https://rekordbox.com/en/support/faq/v7/ · https://cdn.rekordbox.com/files/20260317132052/OneLibrary-Compatible-USB-Device-Export-.pdf ·
https://cdn.rekordbox.com/files/20260807093645/rekordbox7.2.18_manual_EN.pdf ·
https://cdn.teufelaudio.com/products/PIONEER/XDJ-RR/XDJ-RR_manual_EN ·
https://github.com/Deep-Symmetry/crate-digger/issues/46 · https://github.com/Holzhaus/rekordcrate/pull/220 ·
https://raw.githubusercontent.com/ModeAxe/rekordbox_converter/main/core/src/anlz.rs ·
https://www.lexicondj.com/manual/sync-to-pioneer-usb · https://forum.audacityteam.org/t/change-format/64479 ·
https://www.pioneerdj.com/en/product/player/cdj-2000nxs2/black/specifications/

---

## 5. Engine DJ library format

- Current: Engine DJ Desktop 5.1.0 (2026-09-01), Engine OS 5.0.4; 5.0
  (2026-05) reaches every device back to the SC5000. 4.5/4.6 were Rane
  System One-only builds.
- Layout: `Engine Library/Database2/m.db` (plain SQLite, 2.x+); 1.x used
  `m.db` + `p.db`. `hm.db` (history), `sm.db`/`stm.db` exist on desktop
  volumes; no source shows players requiring `hm.db` (**UNVERIFIED**); every
  open-source exporter writes only `m.db`.
- Schema ↔ Engine version (libdjinterop `engine_schema.hpp`): 2.18.0 =
  2.0–2.1; 2.20.1 = 2.2–2.3; 2.20.2 = 2.4; 2.20.3 = 3.0; 2.21.0 = 3.1–3.4;
  2.21.1 = 4.0.0; 2.21.2 = 4.0.1; 3.0.0 = 4.1–4.2; 3.0.1 = 4.3.x; 3.0.2 =
  4.5/5.0.x (latest known). 5.1 schema **UNVERIFIED**. Players refuse newer
  schemas ("is from a newer version of Engine"); Engine OS 4.1+ migrates any
  inserted drive to a 4.0.1+-only format (official).
- Tables: `Information` (schema version, uuid), `Track`, `Playlist`
  (`parentListId`, `nextListId` linked list), `PlaylistEntity` (`listId`,
  `trackId`, `databaseUuid`, `nextEntityId`), `AlbumArt`; in 3.x the five
  blobs move from `Track` to `PerformanceData` (trigger creates rows); 3.0.2
  adds `albumArtSourceHash`. Crates are gone (Mixxx exports them as playlists).
- Blobs: `trackData`, `beatData`, `quickCues`, `overviewWaveFormData` are
  zlib with a 4-byte big-endian length prefix (qCompress); `loops` is
  uncompressed little-endian. Overview waveform = i64 N, i64 N, f64
  samplesPerPoint, N×(low,mid,high) u8, 3-byte max; no colour. Beat grid
  markers `{offset f64, beatNumber i64, beatsToNext i32, unknown i32}`, first
  at beat −4, last at N+1. Key enum 0–23 (0 = C major, 1 = A minor, …, circle
  of fifths). `Track.path` relative to `Engine Library` (`../Contents/...`).
- Writers: libdjinterop (C++, LGPL-3.0, 0.27.1 Mar 2026, 21 schemas, refuses
  newer; tested only on SC5000/Mixstream Pro; Mixxx pins it, playlist export
  in 2.6); `endjine` (Rust, MPL-2.0, 0.0.6, experimental); rb2engine (Python,
  MIT, 3.0.1/3.0.2, no waveforms); PenBridge (Swift, MIT, writes overview
  waveforms). DeCU and Lexicon both broke on Engine 3.0.0.
- Players: analyse unanalysed tracks on load, so waveforms may be omitted but
  grids/cues must be written. FAT32 or exFAT with MBR; NTFS unsupported.
  Official third-party guidance: never change the schema; never write while
  Engine DJ is open; bind by `(originDatabaseUuid, originId)`.
- Licence: linking LGPLv3 into GPLv3 is fine (static too); a native Rust
  writer avoids FFI entirely.

Sources: `docs/research/raw-5-enginedj.md` (59 URLs). Key ones:
https://enginedj.com/downloads · https://raw.githubusercontent.com/xsco/libdjinterop/master/include/djinterop/engine/engine_schema.hpp ·
https://github.com/mixxxdj/mixxx/wiki/Engine%20library%20format ·
https://support.enginedj.com/support/solutions/articles/69000834165-engine-dj-v3-0-support-for-third-party-database-tools ·
https://support.enginedj.com/support/solutions/articles/69000861060-engine-dj-4-1-release-notes ·
https://github.com/jrgutier/rb2engine · https://github.com/athousanddetails/penbridge · https://github.com/xsco/libdjinterop

---

## 6. Tauri 2 specifics

Versions (2026-10-18): tauri 2.12.4 (MSRV Rust 1.90; 3.0.0-alpha exists),
symphonia 0.6.1 (MPL-2.0), cpal 0.18.2 (Apache-2.0), rodio 0.23.0, kira
0.12.5, rusqlite 0.40.2 (MIT), rusqlite_migration 2.6.0 (Apache-2.0),
tauri-plugin-fs 2.7.1, sysinfo 0.39.6, lofty 0.25.4 (MIT/Apache). All
GPLv3-compatible.

- **Symphonia 0.6**: MP3, FLAC, WAV, Vorbis, ALAC excellent/great with
  gapless; AAC is **LC only**, no gapless; HE-AAC only via third-party
  fdk-aac adapter. mp3/aac/alac/isomp4/aiff are opt-in features. Gapless
  moved to `AudioDecoderOptions.gapless` (default true). Seek is per-packet
  (`SeekMode::Accurate`), decoders must be reset after seeking; set
  `prebuild_seek_index` for MP3. ffmpeg-next possible but heavy to bundle.
- **Playback**: decode on `tauri::async_runtime::spawn_blocking`, feed a ring
  buffer drained by the cpal callback; rodio 0.23 does this with
  `Player`/`MixerDeviceSink` and `Source::try_seek`. WebView `<audio>` is
  engine-dependent (webkit2gtk needs host GStreamer plugins; AAC commonly
  fails), so Rust-side playback gives one codec path on all OSes.
- **Filesystem**: fs-plugin scopes apply only to JS commands; Rust `std::fs`
  is unrestricted. Dialog `open()` adds picked paths to scopes
  non-persistently. Volumes: `sysinfo::Disks` (mount point, file system
  string, `is_removable`); Windows `GetDriveTypeW`; eject via `diskutil` on
  macOS and FSCTL/IOCTL or `CM_Request_Device_Eject` on Windows. macOS needs
  `NSRemovableVolumesUsageDescription` in `src-tauri/Info.plist` (TCC
  `SystemPolicyRemovableVolumes`). GUI apps don't inherit shell PATH
  (`fix-path-env-rs`) if shelling out.
- **SQLite**: rusqlite `bundled` + rusqlite_migration (`user_version`);
  tauri-plugin-sql is sqlx-based and JS-facing, adds nothing when the backend
  owns the DB. Store at `app_data_dir()` (`app_local_data_dir()` on Windows to
  avoid roaming), caches at `app_cache_dir()`.
- **macOS signing**: Developer ID Application + notarization need the $99/yr
  program; Tauri bundler env vars `APPLE_CERTIFICATE[_PASSWORD]`,
  `APPLE_SIGNING_IDENTITY`, `APPLE_ID/APPLE_PASSWORD/APPLE_TEAM_ID` or
  `APPLE_API_KEY/ISSUER/KEY_PATH`; hardened runtime on by default; audio
  output needs no entitlement. Ad-hoc signing still requires the Privacy &
  Security approval; Sequoia removed the Control-click bypass; Tahoe reports
  of further tightening are third-party (**UNVERIFIED**).
- **Windows signing**: `bundle.windows.certificateThumbprint/digestAlgorithm/
  timestampUrl` or `signCommand`. EV no longer gives instant SmartScreen
  reputation. Azure Artifact Signing: individuals only in US/Canada, paid Azure
  subscription, pricing page shows no number (the $9.99 figure is
  **UNVERIFIED**). SignPath Foundation: free for OSS built in CI, requires
  policy text on the project page. Certum OSS SKU **UNVERIFIED**.
- **Linux/updater/CI**: AppImage/deb/rpm; only AppImage auto-updates;
  `bundleMediaFramework` only needed for HTML5 media. Updater: `tauri signer
  generate`, `TAURI_SIGNING_PRIVATE_KEY`, `createUpdaterArtifacts`,
  `latest.json` with `darwin-aarch64/darwin-x86_64/windows-x86_64/linux-x86_64`;
  tauri-action@v1 uploads it; official matrix builds the two macOS targets
  separately plus ubuntu-22.04 and windows-latest.
- **IPC**: return `tauri::ipc::Response::new(bytes)` for waveforms (ArrayBuffer);
  `Channel<Vec<u8>>` is still JSON, use `Channel<InvokeResponseBody>` with
  `Raw`; use `Channel` not `emit` for progress; non-async commands run on the
  main thread unless `#[tauri::command(async)]`.

Sources: `docs/research/raw-6-tauri.md` (63 URLs). Key ones:
https://v2.tauri.app/reference/config/ · https://v2.tauri.app/develop/calling-rust/ ·
https://v2.tauri.app/plugin/file-system/ · https://v2.tauri.app/distribute/sign/macos/ ·
https://v2.tauri.app/distribute/sign/windows/ · https://v2.tauri.app/plugin/updater/ ·
https://github.com/pdeljanov/Symphonia · https://docs.rs/rodio/latest/rodio/ · https://signpath.org/

---

## 7. rekordbox XML (`collection.xml`)

Verified against pyrekordbox's format page (which mirrors Pioneer's XML spec):
- Root `DJ_PLAYLISTS Version="1.0.0"`, `PRODUCT Name/Version/Company`,
  `COLLECTION Entries="n"` of `TRACK`, `PLAYLISTS` of nested `NODE`.
- `TRACK` attributes: TrackID, Name, Artist, Composer, Album, Grouping, Genre,
  Kind, Size, TotalTime (s, integer), DiscNumber, TrackNumber, Year,
  AverageBpm, DateModified, DateAdded (yyyy-mm-dd), BitRate, SampleRate,
  Comments, PlayCount, LastPlayed, Rating (0/51/102/153/204/255), Location
  (`file://localhost/...` URI, percent-encoded), Remixer, Tonality, Label,
  Mix, Colour (RGB; fixed palette Rose `0xFF007F`, Red `0xFF0000`, Orange
  `0xFFA500`, Lemon `0xFFFF00`, Green `0x00FF00`, Turquoise `0x25FDE9`, Blue
  `0x0000FF`, Violet `0x660099`; pyrekordbox format page, fetched 2026-10-19.
  A real rekordbox 6.8.6 export uses exactly the Red/Green/Turquoise/Blue values).
- `TEMPO` children: Inizio (s), Bpm, Metro ("4/4"), Battito (beat in bar
  1–4). Multiple per track for dynamic grids.
- `POSITION_MARK` children: Name, Type (0 cue, 1 fade-in, 2 fade-out, 3
  load, 4 loop), Start (s), End (s, loops), Num (−1 memory, 0..7 hot cue
  A..H); rekordbox 6+ also writes Red/Green/Blue (seen in the wild; not on
  the pyrekordbox page).
- `NODE`: Type 0 folder / 1 playlist, Name, Count, Entries, KeyType (0 =
  TrackID, 1 = Location), child `TRACK Key="..."`.
- rekordbox 6 removed XML *export* from the File menu; 7 exposes it under
  Preferences/Advanced ("rekordbox xml" / export) and still imports XML
  (`File > Library > Import Collection in xml format` path **UNVERIFIED** for 7.2).

- Real rekordbox 6.8.6 export (maintainer's library, 2026-10-19; read locally,
  not committed): attribute order, empty vs omitted attributes, decimals,
  `Location` encoding and streaming entries are recorded in
  `docs/phases/phase-1.md` (task 6 notes). Memory cues there are all
  `Type="4"` loops with `End` and no RGB; hot cues always carry RGB.

Sources: https://pyrekordbox.readthedocs.io/en/latest/formats/xml.html ·
https://pyrekordbox.readthedocs.io/en/latest/generated/pyrekordbox.rbxml.html ·
https://community.enginedj.com/t/rekordbox-import-hotcue-offset/46395 ·
https://rekord.cloud/blog/added-support-for-rekordbox-6

---

## 8. Consolidated unverified items

Track these; each should be resolved by a test, a reference export, or a
hardware run, and the result recorded in `docs/decisions.md` or
`docs/hw-test-log.md`.

1. ~~OneLibrary cipher parameters (stock SQLCipher 4 vs non-default) and
   `property.dbVersion` value; seed table contents.~~ Resolved 2026-10-20 for
   rekordbox 7.2.19 (§2, D14). Default `myTag` names and folder `attribute`
   still UNVERIFIED (not in the reference).
2. Whether any third-party `exportLibrary.db` loads on a real OneLibrary
   player; whether the CDJ-3000X "flexible loading" firmware shipped.
3. USBANLZ folder hash algorithm and whether players recompute it or trust
   `analyze_path`.
4. MBR vs GPT requirement; NFC/NFD and case handling on players; 255-char path limit.
5. rekordbox's physical deletion of stale audio/ANLZ; the exact "inconsistent library" wording.
6. PQT2 field layout; PWVC semantics; PWV4 unknown bytes; rekordbox's
   waveform band filters and scaling; PVBR structure.
7. MP3 cue-offset convention relative to Symphonia's decoder delay handling.
8. Engine DJ 5.1 schema version; whether `hm.db` is required; `AlbumArt.hash`
   algorithm; overview waveform point count constants.
9. Azure Artifact Signing price/eligibility; Certum OSS certificate; macOS
   Tahoe Gatekeeper behaviour; WebView2/WKWebView codec sets (moot if Rust playback).
10. `libkeyfinder` accuracy figures; `stratum-dsp` quality; Mixxx downbeat support in 2.6/2.7.
11. rekordbox 7.2 XML import menu path.
12. rekordbox XML output details: attribute order, omitted vs empty
    attributes, `Location` percent-encoding set and decimal precision were
    resolved against a real 6.8.6 export on 2026-10-19 (Phase 1 task 7). Still
    open: whether rekordbox imports `&#10;` and our stricter `Location`
    encoding, and the RGB of the pink/orange/yellow/purple track colours.
13. Mixer (§9): Xone:92 Mk2 differences from the original; its filter slope;
    how the `rubberband` crate builds (bundled or system
    library); `rubato` 5.x API for a smoothly changing ratio.
14. History and dividers (§10): PDB table 19 date layout; whether players
    append to a history across mounts and what they log as a play; volume
    serial access per OS (`diskutil`, `GetVolumeInformationW`, `blkid`) and
    whether reformatting changes it; Engine players writing `hm.db` on the
    stick; minimum divider length, visible title length, shared divider
    files and baken's handling of a silent file with empty artist/album.
    OneLibrary (added 2026-10-21, Phase 4 task 11): how players fill
    `history` / `history_content` (entry order, folders, names, dates; the
    7.2.19 reference's are empty) and whether a player logs one session in
    both libraries.

---

## 9. DJ mixer reference and real-time audio crates (Phase 5)

Research date: 2026-10-19. Supports D21.

### 9.1 Xone:92 music section

From the XONE:92 User Guide, publication AP5345 Issue 1 (original model,
2004), pages 20–25. Mk2 changes (refined filter tapers, a 3-way channel
fader curve switch) come from a retailer listing only (**UNVERIFIED**).

- Four stereo music channels (plus two mic/line channels we don't model). Per
  channel: LEVEL trim (20 dB range), 4-band EQ, FILTER assign (X / off / Y),
  XFADE assign (X / off = straight to mix / Y), CUE, 60 mm VCA fader.
- EQ is "asymmetric": every band boosts at most +6 dB.
  - HI: shelf above a 2.5 kHz turning point; fully anticlockwise it becomes a
    12 dB/oct low-pass (total kill).
  - MID 1: bell at 2 kHz, +6 / −30 dB. MID 2: bell at 350 Hz, same range.
    Q is narrow on boost and wide on cut.
  - LO: shelf below 250 Hz; fully anticlockwise it becomes a 12 dB/oct high-pass.
  - Cue is pre-EQ by default; a master POST EQ switch changes that.
- Music-channel EQ across models (checked 2026-10-22 against the guides'
  text; the mic channels' 10 kHz / 2.5 kHz / 250 Hz / 100 Hz EQ is separate):

  | Model, guide | HI shelf | HM bell | LM bell | LO shelf | Mid range |
  |---|---|---|---|---|---|
  | Xone:92, AP5345 Issue 4 | 2.5 kHz | 2 kHz | 350 Hz | 250 Hz | +6 / −30 dB |
  | Xone:92 Limited Edition, Issue 1 (2024) | 2.4 kHz | 1.8 kHz | 320 Hz | 220 Hz | +6 / −30 dB (panel legend −27) |
  | Xone:96, AP11645 Issue 1 | 3 kHz | 1.1 kHz | 350 Hz | 180 Hz | +10 / −27 dB |

  All three: HI and LO +6 dB / −∞ (12 dB/oct kill on the '92s; the '96
  guide gives no slope). Both '92 guides say the mid Q goes from "high Q
  (narrow bandwidth) on boost to a low Q wide bandwidth when cutting"; none
  gives a Q figure. The Limited Edition labels the mids HM / LM.
  Sources: AP5345 Issue 4 (copy at
  https://www.adorama.com/col/productManuals/AHXONE92FADR.pdf); Limited
  Edition guide
  https://www.allen-heath.com/content/uploads/2024/08/Xone92-Limited-Edition-User-Guide.pdf;
  Xone:96 guide (copy at
  https://www.veranstaltungstechnikmueller.de/pdf/Allen_Heath_Xone96_Bedienungsanleitung.pdf).
- Filters: two stereo state-variable VCFs, X and Y. Cutoff (−3 dB point)
  30 Hz–20 kHz. HPF, BPF and LPF can be combined (HP+LP gives a notch, all
  three an "all-pass" effect), and switching is ramped to avoid clicks.
  RESONANCE goes from MILD to WILD and is flattest at about two o'clock.
  The slope is not stated (**UNVERIFIED**).
- LFOs: one per filter with ON, DEPTH, TAP tempo (last two taps) and ×2.
  The waveform is not stated (**UNVERIFIED**).
- VCF TO XFADER lets the crossfader sweep a filter's cutoff. VCF LINK 2-1
  makes filter 2 follow filter 1's FREQ, e.g. LP on one channel and HP on the
  other for a moving crossover.
- Crossfader curve: from progressive with a 4 dB dip at centre to almost
  instant full level off the end stop.
- Headphones: a new cue cancels the previous one (several by pressing
  together), cue/mix balance, split cue.

### 9.2 Rust crates for a real-time engine (crates.io, 2026-10-19)

| Crate | Version | Licence | Last publish | Use |
|---|---|---|---|---|
| `cpal` | 0.18.2 | Apache-2.0 | 2026-08-16 | output stream, multichannel devices |
| `rtrb` | 0.4.0 | MIT OR Apache-2.0 | 2026-08-18 | wait-free SPSC ring buffer for control/state |
| `rubato` | 5.0.1 | MIT OR Apache-2.0 | 2026-10-01 | resampling for varispeed |
| `midir` | 0.11.1 | MIT | 2026-10-10 | MIDI, if controller mapping is ever added |
| `timestretch` | 0.15.0 | MIT | 2026-09-02 (first 2026-02-26) | pure Rust; real-time engine with varispeed + SOLA key lock, "optimised for EDM"; no published quality figures |
| `signalsmith-stretch` | 0.1.3 | MIT | 2025-09-18 | binds the Signalsmith Stretch C++ library, built with `cc`/`bindgen` |
| `ssstretch` | 0.1.0 | MIT | 2025-03-01 | second Signalsmith binding, via `cxx` |
| `rubberband` | 0.1.5 | GPL-2.0-or-later | 2026-09-18 (first 2026-09-09) | Rubber Band bindings; build method **UNVERIFIED** |
| `bungee-rs` | 0.1.1 | MPL-2.0 | 2026-02-21 | Bungee bindings, C++ as a git submodule |
| `soundtouch` | 0.5.4 | `LGPL-2.1` (only/or-later not stated) | 2026-03-23 | SoundTouch bindings; `deny.toml` allows only LGPL-2.1-or-later |

Sources: XONE:92 User Guide AP5345 (mirror):
https://www.dittasistema.it/wp-content/uploads/2015/09/Manuale-Allen-Heath-XONE-92.pdf ·
https://www.manualslib.com/products/Allen-And-Heath-Xone-92-3144081.html ·
Mk2 listing: https://avlgear.com/collections/allen-heath-dj-equipment/products/allen-and-heath-xone-92-mk2-analog-4-channel-dj-mixer ·
crates.io API `https://crates.io/api/v1/crates/<name>` for every crate above ·
https://docs.rs/timestretch · https://docs.rs/signalsmith-stretch ·
https://docs.rs/ssstretch · https://github.com/jefflongo/rubberband-rs

### 9.3 Key lock candidates in detail (2026-10-21)

Checked for phase 5 task 10 by reading each crate's published source
(crates.io downloads) and running all three through
`reks keylock-render` on an Apple M4 Pro, macOS 26.6.2, Apple clang 21,
Rust 1.99.1, release build. Timings and listening steps:
[`phases/phase-5.md`](phases/phase-5.md) task 10.

| | `timestretch` 0.15.0 | `signalsmith-stretch` 0.1.3 | `rubberband` 0.1.5 + `rubberband-sys` 0.1.4 |
|---|---|---|---|
| Library | own (Rob Morgan) | Signalsmith Stretch **1.3.1** vendored (upstream 1.4.0, 2026-09) | Rubber Band Library **4.0.0** vendored (single-file build) |
| Licence of library | MIT | MIT (Geraint Luff / Signalsmith Audio) | GPL-2.0-or-later (or a paid commercial licence) |
| Licence of binding | n/a (pure Rust) | MIT (Colin Marc) | GPL-2.0-or-later (Jeff Longo) |
| GPLv3-compatible | yes | yes | yes ("version 2 … or any later version"); already on the `deny.toml` allowlist |
| New deps | arc-swap, ebur128, rustfft, serde_json (all MIT/Apache) | dasp 0.11 (MIT/Apache); build: cc, bindgen 0.70 | bitflags; build: cc, bindgen 0.71 |
| Build needs | stable Rust only | C++14 compiler + libclang for bindgen | C++11 compiler + libclang; links Accelerate on macOS |
| macOS | builds | builds (Xcode CLT) | builds (Xcode CLT) |
| Windows | should build (pure Rust), **UNVERIFIED** | MSVC + LLVM install for libclang (`LIBCLANG_PATH`), **UNVERIFIED** | same, plus its `_WIN32`/`NOMINMAX` defines; **UNVERIFIED** |
| Linux | should build, **UNVERIFIED** | g++/clang + `libclang-dev`, **UNVERIFIED** | same, pthreads; **UNVERIFIED** |
| Real-time API | deck engine: lock-free controller, `process(out)` fills exactly the frames asked for | `process(in, n_in, out, n_out)`, caller paces the input | `process`/`available`/`retrieve`; works in its own hop sizes, so a deck needs an output FIFO |
| Latency (reported) | Keylock 12.7 ms; WideKeylock 0 ms output delay but buffers source lookahead | 60 ms at 44.1 kHz (`output_latency`, default preset) after priming with `seek` | R3 46.4 ms, R2 23.2 ms (`start_delay`) |
| Allocation in process | claims none; measured none in Keylock, **15–27 heap events per 60 s in WideKeylock** | none by reading the vendored header (`configure` sizes every buffer; `resize` stays within capacity); not a documented promise; C++ heap not measurable from Rust | library: RT-safe in real-time mode per its header docs ("performs no allocation, locking, or blocking"); **the Rust wrapper allocates a `Vec` on every `process` and `retrieve` call** and `expect`s on null; calling `rubberband-sys` directly avoids both |
| Maintenance | 15 releases since 2026-02-26, last 2026-09-02, repo pushed 2026-10-09; single maintainer; repo carries a `CLAUDE.md` (agent-written code); 25 stars | binding: last release 2025-09-18, repo pushed 2026-06-12, 14 stars; library: active (1.4.0, repo pushed 2026-09-25, 571 stars) | binding: first release 2026-09-09, 0 stars, single maintainer, edition 2024; library: mature (since 2007), 4.0.0 is the latest (GitHub mirror pushed 2025-03-03) |

Findings that matter for a deck:

- `timestretch`'s `SourceProducer::demand_hint` leaves out the head's
  lookahead: fed by it alone, `WideKeylock` underruns (512 frames in our tone
  test). Adding `EngineProcessor::varispeed_lookahead_frames()` fixes it.
  Its Keylock latency measured on a tone burst moves with the ratio
  (5.7 ms at 0.94, 14.7 ms at 1.06; SOLA splices shift the burst).
- Signalsmith Stretch is a phase vocoder with a fixed 60 ms delay; on a deck
  the read head must run 60 ms ahead (or the waveform lag) when key lock is on.
- Rubber Band is the reference in this field (timestretch's own CI compares
  against it), but R3 costs ~3–4× R2 and the binding is weeks old.
- Under GPL-3.0-or-later all three are fine. Rubber Band's GPL would bind a
  future relicensing of reks; MIT options would not.

Sources (all fetched 2026-10-21): crates.io API and crate downloads for
`timestretch` 0.15.0, `signalsmith-stretch` 0.1.3, `rubberband` 0.1.5,
`rubberband-sys` 0.1.4 (`https://crates.io/api/v1/crates/<name>`); their
`build.rs`, `LICENSE*`, `COPYING` and vendored headers
(`signalsmith-stretch.h` version 1.3.1, `RubberBandStretcher.h` RT-safety
notes, `RUBBERBAND_VERSION "4.0.0"`); GitHub API for repo activity:
https://github.com/robmorgan/timestretch-rs ·
https://github.com/colinmarc/signalsmith-stretch-rs ·
https://github.com/Signalsmith-Audio/signalsmith-stretch ·
https://github.com/jefflongo/rubberband-rs ·
https://github.com/breakfastquay/rubberband · https://breakfastquay.com/rubberband/

### 9.4 Deck loop controls and deck/mixer layouts (2026-10-22)

Loop conventions (rekordbox 7.2.18, Traktor Pro 4, Serato DJ Pro, Engine DJ
5.1, SC6000, CDJ-3000), software mixer layouts, DJM/Xone channel order,
measurements of reks's own layout, four layout concepts and a quick-loop
spec: [`research/deck-mixer-layout.md`](research/deck-mixer-layout.md),
sources with dates in its §3. Proposal only; no published usability guidance
on waveform or knob sizes was found.

### 9.5 DJM 3-band EQ and Sound Color FX (2026-10-23)

For the 3-band EQ layout (issue #56): EQ, FX knob, effect select and PARAMETER. Spec: [`research/djm-eq-fx.md`](research/djm-eq-fx.md).
Not checked on a real DJM.

- Channel EQ, all three bands −26 / +6 dB, with an EQ CURVE switch
  (ISOLATOR / EQ). Each band has a frequency printed next to it: DJM-900nexus
  HI 13 kHz, MID 1 kHz, LOW 70 Hz (operating instructions, spec page; copy at
  http://www.revolution.pt/pdfs/djm900n.pdf). DJM-900NXS2 HI 30 kHz, MID 1 kHz,
  LOW 20 Hz (operating instructions 2015, "Adjusting the sound"; French copy at
  http://www.la-bs.com/Objets/DJM-900NXS2_me.pdf). DJM-A9 HI 20 kHz, MID 1 kHz,
  LOW 20 Hz (instruction manual, "Channel equalizer characteristic"; copy at
  https://cdn01.4wall.com/cms/rentals/files/f6835eb526b3d2.pdf). The isolator
  range, −∞ / +6 dB, appears in retailer spec sheets only (e.g.
  https://www.scan.co.uk/products/pioneer-djm900-nexus-mk2-4ch-pro-dj-mixer-for-cdj900nxs-2000nxs-64-bit-mixing-processor-4-phono-inpu).
  No source gives corner frequencies, slopes or Q. The 70 Hz / 1 kHz / 13 kHz
  "crossover" figures on forums are disputed in the same threads
  (https://forum.djtechtools.com/showthread.php?t=45205,
  https://forum.djtechtools.com/showthread.php?t=24558): **UNVERIFIED**.
- Sound Color FX (NXS2 and A9 manuals above): SPACE, DUB ECHO, SWEEP, NOISE,
  CRUSH, FILTER. One effect is selected for CH1–CH4; each channel's COLOR
  knob sets it and one PARAMETER knob sets resonance, feedback, frequency or level.
  FILTER: counterclockwise lowers the LPF cutoff, clockwise raises the HPF
  cutoff, PARAMETER = resonance. SWEEP is gate / band-pass on the NXS2 and
  notch / band-pass on the A9. The DJM-900nexus had GATE/COMP in its place.
  The A9 adds CENTER LOCK (the knob stops at centre on a quick turn). No
  cutoff range, slope or resonance figure is published. Owners report the
  NXS-series filter resonance as very high near 2–3 o'clock, which Pioneer
  says is by design
  (https://forums.pioneerdj.com/hc/en-us/community/posts/205869885-Adjust-DJM-900-Filter-Resonance).
- Open reference for a 3-band DJ EQ: Mixxx splits at 246 Hz and 2.5 kHz by
  default, as shelves plus a band (Mixxx 2.5 manual, "Equalizers",
  https://manual.mixxx.org/2.5/en/chapters/preferences/equalizers).
- Reverb: Freeverb, Jezar at Dreampoint, 2000, released into the public
  domain; structure and tunings as published in J. O. Smith, *Physical
  Audio Signal Processing*, "Freeverb" (CCRMA, online edition,
  https://ccrma.stanford.edu/~jos/pasp/Freeverb.html, read 2026-10-23).
- Shelf design: A. Simper, "Linear Trapezoidal Integrated SVF" (Cytomic,
  2013, https://cytomic.com/technical-papers), the same paper as our SVF.

---

## 10. Play history and playlist dividers (Phases 2 and 2.5)

Research date: 2026-10-19. Supports D22 and D23. §10.2 summarises the
maintainer's play-history PRD of the same date; we did not re-check its sources.

### 10.1 What a stick holds, and what our writer does to it

- Legacy players write history into `export.pdb`: tables 11
  (`history_playlists`) and 12 (`history_entries`), plus table 19 (`history`,
  dates; layout undecoded) (§4, raw-4 §3.1). History pages need a valid empty
  structure (`u7 = 1`) (raw-4 §3.5). The prolink project reports table 19 rows
  with ascending ids and ASCII `YYYY-MM-DD`; its licence is unchecked, so it is
  a reference only.
- rekordcrate 0.3.0 has `HistoryPlaylist`, `HistoryPlaylistId` and
  `HistoryEntry` row types; nothing for table 19.
- baken-export 4.6.1 rebuilds `export.pdb` on every run (raw-1). Without its
  `onelibrary` option, "an existing OneLibrary on the stick is removed".
  `generate_analysis` covers only tracks "rekordbox never analysed, instead of
  leaving them out". Audio goes to `/Contents/<Artist>/<Album>/<file>`, names
  cut at 48 characters, collisions suffixed `-1`, `-2`; empty artist/album
  handling is not documented (**UNVERIFIED**).
- Volume serial per OS (checked 2026-10-19 on macOS 26 with a FAT32 disk
  image, see `fixtures/device/`): `diskutil info -plist` has no raw FAT serial,
  only `VolumeUUID` (a version-3 UUID, `10DE9FD0-…-3F77-…` for boot-sector
  serial `4993-1CFB`), stable across detach/attach. Windows
  (`GetVolumeInformationW`) and Linux (`/dev/disk/by-uuid`, `blkid`) report the
  serial itself, so the same stick gets a different key per OS. Not yet checked
  on Windows, Linux or a real stick (**UNVERIFIED**).
- macOS `VolumeUUID` derivation (Apple msdosfs source, `msdosfs_vfsops.c`
  `msdosfs_generate_volume_uuid` and `msdos_appex/utils.m`, commit c9f076c,
  2025-10-16; checked 2026-10-19 on macOS 26 against 64 MB `hdiutil` images):
  MD5 over namespace `B3E20F39-F292-11D6-97A4-00306543ECAC`, the 4 serial
  bytes as on disk, and for FAT the BPB total sectors as LE u32, then forced
  to version 3. FAT32 serial `49CC-1DF8`, 131070 sectors →
  `ECDD941D-AD33-38A9-BCB2-F3F0681ED3E2`, matching `diskutil`. exFAT (closed
  source; checked empirically) hashes the serial only: `6AD6-5E23` →
  `FE42BE2B-2FA4-3F54-A2B6-6A605C659CB0`. The hash can't be reversed, and
  the raw device a real stick shows up as is `root:operator 0640`
  (`hdiutil` images are owned by the attaching user). So macOS can't give the
  serial without root; see D28. Source:
  https://github.com/apple-oss-distributions/msdosfs
- OneLibrary keeps history in `history` / `history_content` (§2); Engine in
  `hm.db` (§5; whether players write it on the stick is **UNVERIFIED**).

### 10.2 Play history in other apps (from the PRD)

| App | Sessions | What counts as a play | Counts | Reset / exclude |
|---|---|---|---|---|
| rekordbox | History per stick mount; desktop groups by year/month | Performance mode: 60 s default, configurable | Imported from stick histories (export mode only) | Dedupes by name + contents; deletes from the stick by default (keep option since 7.0.6) |
| Serato | Explicit sessions with start/end; shows auditioned tracks | Deck playing with channel fader and crossfader open | Own count, no backfill from older versions | Mark played/unplayed; counting can be disabled |
| Traktor | One history per app run, archived on close | Minimum play time; on air in internal mixing | Separate Play Count, Last Played, Played-State | Reset touches only the played checkmark |
| Mixxx | Set-log playlists | Player logic | Derived from set-log rows; deleting a session updates counts | Remove from or delete a session |
| Lexicon | Imported sessions with location and rating | From the source app | From the source app | — |

Adopted: derived counts (Mixxx); sessions with metadata (Serato, Lexicon);
name + contents dedupe and keep-on-stick (rekordbox); a separate baseline
without backfill (Serato). Divider tracks: DJs make them by hand in rekordbox
with silent files renamed to a label (playlist-dividers PRD, 2026-10-19).

Sources: https://docs.rs/rekordcrate/0.3.0/rekordcrate/pdb/index.html ·
https://docs.rs/baken-export/latest/baken_export/struct.Options.html ·
https://docs.rs/baken-export/latest/baken_export/layout/index.html ·
https://github.com/usr-ein/prolink/releases/tag/v0.3.0 ·
https://pyrekordbox.readthedocs.io/en/latest/formats/devicelib_plus.html ·
PRD sources: https://rekordbox.com/en/support/faq/operation-hints-7/ ·
https://cdn.rekordbox.com/files/20251021171528/USB_export_guide_en_251007.pdf ·
https://cdm.link/rekordbox-7-0-6/ · https://preview.nowplayingapp.com/tutorials/rekordbox ·
https://support.serato.com/hc/en-us/articles/223455687 ·
https://support.serato.com/hc/en-us/articles/360001031675-How-to-use-the-Plays-column-in-Serato-DJ-Pro-2-2-0-and-above ·
https://support.native-instruments.com/support/solutions/articles/69000879487-my-history-playlist-is-empty-after-restarting-traktor ·
https://community.native-instruments.com/discussion/comment/54748/ ·
https://github.com/mixxxdj/mixxx/pull/3140 ·
https://community.enginedj.com/t/multi-source-sets-and-shared-merged-history-no-start-time-on-history-exports/58299 ·
https://www.digitaldjtips.com/lexicon-1-10-brings-serato-4-0-support-track-timeline-play-history/ ·
https://community.algoriddim.com/t/edit-play-count-on-macos-windows/23018

---

## 11. File conversion (Phase 2.6)

Research date: 2026-10-19. Supports D25.

- Need: which formats each player loads is in §4. In short, WAV/AIFF 16/24-bit
  at 44.1/48 kHz load everywhere; FLAC/ALAC and rates above 48 kHz don't on
  CDJ-2000NXS and older.
- Other tools convert to fix this. Lexicon auto-fixes the §4 list.
  rekordbox-edit `convert` writes AIFF (default), FLAC, WAV or MP3, repoints
  the rekordbox DB entry so cues and grids stay, skips lossy sources, and
  deletes originals only when nothing was lost. rekordbox itself treats a
  converted file as a new track; cues live in its DB, not in the file.
- `hound` 3.5.1 (`src/write.rs`, `new_with_spec_ex`) picks
  `WAVE_FORMAT_EXTENSIBLE` when channels > 2 or bits per sample > 16. So
  24-bit WAVs written with hound hit the E-8305 issue (§4).
- `flacenc` 0.5.1: Apache-2.0, pure Rust FLAC encoder, last publish
  2025-12-18, ~750k downloads (crates.io API, 2026-10-19).

Sources: https://rekordbox-edit.readthedocs.io/en/latest/commands/convert/ ·
https://pypi.org/project/rekordbox-playlist-converter/ ·
https://www.mixgraph.io/tools/wav-to-aiff ·
https://crates.io/api/v1/crates/flacenc · https://github.com/yotarok/flacenc-rs/ ·
https://docs.rs/hound/3.5.1/src/hound/write.rs.html

---

## 12. rekordbox pain points and reks gaps

Research date: 2026-10-24. Not tied to a decision; input for planning. Full
analysis: [`research/rekordbox-pain-points.md`](research/rekordbox-pain-points.md)
(theme table, ranked gaps, every cited source with its date); raw tables:
[`research/raw-7-rekordbox-pain-points.md`](research/raw-7-rekordbox-pain-points.md).

- Loudest 2025–26 complaint: USB export trust. CDJ-3000 firmware 3.30 blanked
  playlists and was pulled (2025-11-04); rekordbox 7.2.12 shipped an export
  players could not load and was pulled (2026-03-17); Device Library vs
  OneLibrary sticks read differently per player (AlphaTheta, 2026-03-18).
- Then subscription and paywalls (older controllers lost the free v7 unlock
  2024-07; billing outage 2025-01-25 to 2025-03), and library hygiene:
  duplicates, missing files, database loss, which paid tools (Lexicon, RCT,
  Mixed In Key) sell fixes for.
- Cheapest high-signal gaps for reks: a read-only "check a stick" report,
  target players per stick profile, automatic library backups, relocating
  missing files.
- Limits: Reddit was unreachable with our tools and the Pioneer forum
  returned 403 (snippets only); vendor pitches are flagged.

Key sources (accessed 2026-10-24): https://rekordbox.com/en/support/releasenote/
(2023-03 to 2026-09) ·
https://rekordbox.com/en/2026/03/important-notice-regarding-rekordbox-v7212/
(2026-03-17) ·
https://www.pioneerdj.com/en-gb/news/2026/cdj-3000-firmware-ver330-important-notice
(2026-01-07) ·
https://alphatheta.com/en/information/important-notice-for-customers-using-usb-devices-with-our-dj-equipment/
(2026-03-18) ·
https://www.digitaldjtips.com/rekordbox-cloud-library-sync-advice/ (2026-04-16) ·
https://www.digitaldjtips.com/owners-of-older-pioneer-dj-controllers-must-now-subscribe-to-use-rekordbox-7/
(2024-07-11) ·
https://musictech.com/features/pioneer-dj-cdj-3000-firmware-bug-jaguar-mojaxx/
(2026-01-09) ·
https://community.algoriddim.com/t/rekordbox-onelibrary-misaligned-beatgrids/40391
(2025-12-06) · https://www.trustpilot.com/review/rekordbox.com (2024-11 to
2026-08) · https://www.lexicondj.com (vendor, 2026-10)
