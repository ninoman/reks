# Research 1 — Open-source projects for reading/writing rekordbox export formats

Surveyed 2026-10-18. Every factual claim cites a URL (see Sources). Items I could not confirm are marked **UNVERIFIED**.
Target context: a GPLv3 Rust/Tauri desktop app that must write `export.pdb` + ANLZ (Device Library) and `exportLibrary.db` (OneLibrary).

## 0. Executive summary

| Project | Lang | Licence (SPDX) | GPLv3-compat | Reads | WRITES | Last activity | Rust dep? |
|---|---|---|---|---|---|---|---|
| **baken / baken-export** (M-Igashi) | Rust | MIT | Yes | rekordbox `collection.xml`, rekordbox ANLZ cache, MySettings | **export.pdb, ANLZ (copy or generate), exportLibrary.db (`--onelibrary`), settings, artwork** | pushed 2026-10-17; v4.6.1 2026-10-12 | **Yes** (crates.io `baken-export`) |
| **rbxport** (chrisle) | Rust/Tauri 2/React | GPL-2.0-or-later | Yes (GPLv2+ → GPLv3 OK) | rekordbox master.db (SQLCipher), USB sticks | **export.pdb, exportExt.pdb, ANLZ DAT/EXT/2EX, exportLibrary.db, sync records, artwork, settings** | pushed 2026-10-18; v1.9.0 2026-10-16 | Workspace crates `rbl-pdb`, `rbl-anlz`, `rbl-onelibrary` (git only, not on crates.io — UNVERIFIED whether published) |
| **rbox** (dylanljones) | Rust (+py/npm bindings) | GPL-3.0-only (≥0.1.6) | Yes | master.db, exportLibrary.db, XML, ANLZ, settings | exportLibrary.db (claimed), ANLZ (modify/save), XML, settings, master.db — **no export.pdb** | 0.1.8 2026-10-11 | Yes (crates.io), but GitHub repo returns 404 |
| **rekordcrate** (Holzhaus) | Rust | MPL-2.0 | Yes | export.pdb, exportExt.pdb (main), ANLZ, settings, XML | ANLZ (BinWrite); PDB write-side in progress on `main`, PR #259 "create exports from scratch" OPEN; released 0.3.0 has no PDB writer | pushed 2026-10-05; last release 0.3.0 2025-01-23 | Yes (crates.io), but writer only via git |
| **crate-digger** (Deep Symmetry) | Java + Kaitai `.ksy` | EPL-2.0 w/ secondary MPL-2.0 or LGPL-3.0 (generated parsers too) | Yes via secondary licence | export.pdb, exportExt.pdb, ANLZ (over NFS from players too) | nothing | pushed 2026-10-07; v0.2.1 2025-07-21 | Reference only (`.ksy` specs) |
| **pyrekordbox** (dylanljones) | Python | MIT | Yes | master.db, XML, ANLZ, MySettings, exportLibrary.db | XML, MySettings, master.db (limited), exportLibrary.db (0.5.0), ANLZ modify/save — **no export.pdb** | v0.5.0 2026-10-14 | Reference only |
| **rex** (kimtore) | Go | **none** (no LICENSE file) | No (unlicensed = all rights reserved) | Mixxx `mixxxdb.sqlite3` | export.pdb (partial; no waveforms/grids/hot cues) | pushed 2025-08-13 | Reference only, and legally unusable as code |
| **dj-usb-tkit** (haivala) | Rust/Tauri 2 | AGPL-3.0-or-later | Yes (GPLv3 ↔ AGPLv3 combination allowed via GPLv3 §13) but AGPL would then apply to combined work | rekordbox/Mixxx libraries, existing USB PDB | "PDB/eDB" USB databases, Engine DJ (opt.) | pushed 2026-10-16 | Not a library; own in-house code |
| **export-studio** (claw1200) | Rust+Electron | GPL-2.0 (API says GPL-2.0; UPSTREAM.md says GPL-2.0-or-later via vendored rbxport) | Likely yes if "or later" | rekordbox master.db | DeviceLibrary + OneLibrary USB (vendors rbxport crates) | pushed 2026-10-18 | Not a library |
| **Mixxx** | C++ | GPL-2.0-or-later | Yes | export.pdb + ANLZ via Kaitai-generated C++ | nothing (import only) | pushed 2026-10-17 | Reference only |
| **djl-analysis docs** (Deep Symmetry) | AsciiDoc (Antora in crate-digger `doc/`) | repo EPL-2.0 (+MPL/LGPL secondary); docs text licence not separately stated | n/a | — | — | content updated through Dec 2025 (row-count footnote) | Reference |
| baken-export's upstream "rbox"/"REX" confusion | — | — | — | — | — | — | — |

Bottom line: there are now **three independent, actively maintained Rust `export.pdb` + ANLZ + `exportLibrary.db` writers** (baken-export [MIT, on crates.io, hardware-confirmed on CDJ-3000/2000NXS2/2000NXS], rbxport `rbl-*` [GPL-2.0+, git], and rekordcrate's in-progress PR #259 [MPL-2.0]). A year ago (Jan 2025) a Mixxx dev said this was not feasible; that is no longer true.

---

## 1. baken / baken-export (M-Igashi) — "Bake'n Deck"

- Repo: https://github.com/M-Igashi/baken (GitHub API: licence `MIT`, language Rust, created 2026-01-08, pushed 2026-10-17, 58 stars, 6 forks, 5 open issues). [S1]
- Crates: `baken` (CLI, 4.6.1, 2026-10-12, MIT), `baken-core`, **`baken-export`** 4.6.1 (MIT, first published 2026-09-18, 336 downloads). Description: "writes a rekordbox USB export (export.pdb + ANLZ) from collection.xml". [S2][S3][S4]
- "baken-export" is the thing the task called "baken-export": it is a *workspace crate inside baken*, not a separate repo.
- Author: GitHub user M-Igashi (crates.io owner). [S2]
- **Reads:** rekordbox `collection.xml` (also Lexicon-written XML), rekordbox's local ANLZ cache, My Settings files, audio (FLAC/AIFF/WAV/MP3/AAC/ALAC). It explicitly never reads/decrypts `master.db`. [S2][S1]
- **Writes (via `baken expressport` / `baken_export::export`):** `PIONEER/rekordbox/export.pdb` (always, rebuilt every run); `PIONEER/USBANLZ/...ANLZ0000.{DAT,EXT,2EX}` copied from rekordbox's cache or **generated from audio** with `--generate-analysis` (scrolling waveform "pixel-matching"; colour/3-band/preview "close approximations"; phrase/PSSI not generated; grid+cues come from XML); audio under `Contents/`; MySettings + `djprofile.nxs`; `exportLibrary.db` only with `--onelibrary`; artwork with `--artwork`. [S2][S1][S5]
- **Public library API (`baken_export`):** modules `anlz`, `artwork`, `build`, `collection`, `layout`, `onelibrary`, `pdb` ("Writes export.pdb (DeviceSQL)"), `settings`, `volume`; `plan()` → `Plan` then `export()` → `Report`; `Options`, `Error::DeviceWrite`, const `ONELIBRARY_FILES`. Source tree: `src/pdb/{fixed,page,rows,string,writer}.rs`, `src/anlz/{cues,flac,generate,hash,locate,pssi,rewrite,section}`, `src/onelibrary.rs`, `src/sqlcipher.rs`. [S5][S6][S7]
- **OneLibrary implementation:** builds `exportLibrary.db` as SQLCipher 4 with rekordbox defaults, "one passphrase for every stick"; passphrase is embedded (obfuscated) in source, scheme credited to pyrekordbox; writes 22 tables/4 indexes — populates `content, artist, album, genre, label, key, color, playlist, playlist_content, image, menuItem, category, sort, property, myTag`; leaves `cue, history, history_content, hotCueBankList, hotCueBankList_cue` empty (cues go through ANLZ). Schema taken from rekordbox 7.2.18/7.2.19 fixtures. Deps are `rusqlite` + `aes/cbc/hmac/pbkdf2/sha2` (no sqlcipher C lib; it appears to implement page encryption itself — inference from dependency list, UNVERIFIED). [S8][S4]
- **Hardware status:** Issue #116 ("Player reports…", closed 2026-09-29 with 4.3.0 = out of beta): CDJ-3000 fw 3.22 and CDJ-2000NXS2 fw 1.85/1.87 confirmed (recognition, playlists, search, grid, waveform, hot cues). Fixes made along the way: string/track-count alignment to stop an NXS2 freeze on non-ASCII artist names (#168), key-name handling (#189/#190), hot cues D–H placement (#205). Open oddity: `E-8307 DEVICE NO RESPONSE` on first insertion on CDJ-3000. [S9] Issue #242 (closed 2026-10-14): CDJ-2000NXS fw 1.44 works (playlists, playback, waveforms, hot cues A–C, memory cues, loops). [S10]
- **OneLibrary is untested on real players** (README says so; open issue #139 tracks CDJ-3000X/XDJ-AZ/OPUS-QUAD/OMNIS-DUO). My Tags, artwork, hot-cue banks not included in OneLibrary. [S1]
- Caveats: FAT32+MBR recommended; "don't layer expressport over a stick rekordbox has written" (rekordbox rewrites device library); only one active loop per track. [S1]
- **Usable as a Rust dependency: YES** (`baken-export` on crates.io, MIT). Caveat: API docs coverage 46.8%, 0 examples; it is XML-driven (`collection.xml` model) so you may need to synthesise its `collection` model from your own library rather than from XML — check `Plan`/`PlanTrack` API. [S4][S5]

## 2. rbxport (chrisle) — Tauri/Rust library manager with USB export

- Repo: https://github.com/chrisle/rbxport (API: licence NOASSERTION/"Other" per GitHub, but README and `Cargo.toml` state **GPL-2.0-or-later**; created 2026-09-25; pushed 2026-10-18; 218 stars; 34 forks; 109 open issues; not a fork). Version 1.9.0 cut 2026-10-16. [S11][S12][S13][S14]
- Stack: Tauri 2 + React/TS frontend, Rust backend split into 18 `rbl-*` crates incl. **`rbl-pdb` ("Legacy DeviceSQL binary database"), `rbl-anlz` ("Parsing and authoring of analysis files"), `rbl-onelibrary` ("Device Library Plus / OneLibrary database"), `rbl-export`**, plus `rbl-prolink`, `rbl-dbserver`, `rbl-nfs` (serves as a Pro DJ Link source). [S15]
- **Writes to USB** (docs/reference/usb-export-db.md): `export.pdb`, `exportExt.pdb` (My Tags), `exportLibrary.db` (SQLCipher 4; open order `PRAGMA cipher=sqlcipher; legacy=4; key`; published with `journal_mode=DELETE`, `synchronous=EXTRA`), ANLZ DAT/EXT/2EX, `playlists3.sync`/`playlists3Plus.sync`, artwork (4 JPEGs per image), `DEVSETTING.DAT` + MySettings, plus private `PIONEER/rbxport/manifest.json`. Own writer; rekordcrate/pyrekordbox used only as cross-checks. [S16]
- **Verification:** byte-comparison against rekordbox 7.2.11 exports (556/562 files identical), CDJ-3000 fw 3.20 playback path exercised in a firmware test instance with synthesized peripherals, static analysis of CDJ-3000X 1.40 / XDJ-RX2 / XDJ-RX3 firmware, one physical XDJ-AZ check (ratings). Past user-reported failures: CDJ-2000NXS freeze, CDJ-900NXS lockup, rekordbox 6.8.6 crash (#349). Docs say structural verification "does not establish compatibility with every firmware or device." [S16]
- **Documented unknowns:** `myTagMasterDBID` derivation, whether players list My Tags from its `exportExt.pdb`, parts of PWV3/4/5 and PQT2 headers, some DEVSETTING bytes, 22-vs-26 OneLibrary table count mismatch, `contentLink` left at 0. [S16]
- **Usable as Rust dependency:** crates are in-repo (git path deps); not found on crates.io (UNVERIFIED — no crates.io lookup done for `rbl-pdb`). Licence GPL-2.0-or-later is combinable with GPLv3. export-studio already vendors them. [S14][S17]

## 3. rbox (dylanljones) — Rust library, GPL-3.0

- crates.io: `rbox` 0.1.8 (2026-10-11), created 2025-06-05, 2,383 downloads, repository field `https://github.com/dylanljones/rbox`. Licence history: 0.1.0–0.1.3 MIT, 0.1.4–0.1.5 `MIT OR Apache-2.0`, **0.1.6+ `GPL-3.0-only`**. MSRV 1.88. [S18][S19]
- **GitHub repo returns HTTP 404** (both HTML and API) as of 2026-10-18 — private, renamed, or removed. Source is still obtainable from the crates.io tarball / docs.rs source view. UNVERIFIED why. [S20][S21]
- PyPI `rbox` 0.1.8 (2026-10-11, GPLv3, author Dylan Jones); npm `rbox-js`. [S22][S23]
- Features (`Cargo.toml`): `master-db` (diesel + SQLCipher via `libsqlite3-sys` `sqlcipher` feature), `one-library` (diesel + diesel_migrations), `anlz` (binrw 0.15), `xml` (quick-xml), `settings` (binrw, crc16). Source modules: `anlz/`, `masterdb/`, `one_library/`, `xml.rs`, `settings.rs`. **No `pdb` module — it does not read or write `export.pdb`.** [S19][S24][S25]
- README claims: query/update master.db via type-safe ORM; "read and write" One Library (`exportLibrary.db`); "parse and write all three analysis files" (.DAT/.EXT/.2EX) — save preserves section order and copies untouched sections; XML read/write; MySettings parse/write. Whether it can *create* a new `exportLibrary.db` or ANLZ from scratch is not stated (UNVERIFIED). [S18][S23]
- docs.rs build failed for 0.1.7/0.1.8 (last good: 0.1.4). Windows build of vendored OpenSSL needs Perl. [S18]
- **Usable as Rust dependency:** yes for OneLibrary/ANLZ/XML/settings; GPL-3.0-only is compatible with a GPLv3 app. Not a solution for `export.pdb`. Code-provenance risk given the 404 repo.

## 4. rekordcrate (Holzhaus) — Rust, MPL-2.0

- Repo: https://github.com/Holzhaus/rekordcrate (API: MPL-2.0, Rust, created 2022-02-06, pushed 2026-10-05, 213 stars, 42 forks, 47 open issues+PRs; README page shows 18 issues / 29 PRs). Author Jan Holthuis (Holzhaus), also a Mixxx developer. [S26][S27][S28]
- crates.io: 0.3.0 (2025-01-23), prior 0.2.1 (2023-11-30), 0.2.0 (2022-10-09), 0.1.0 (2022-02-10); 20,831 downloads; all versions MPL-2.0. **Nothing released in 21 months**; the write-side work is only on `main`. [S29]
- README: "still under heavy development and might have breaking API changes"; CLI `dump-pdb`, `dump-anlz`, `dump-setting`, `--format json`. [S30]
- **Reads:** export.pdb (all documented row types incl. History, MenuItem/table 17), **exportExt.pdb** (unreleased, on main), ANLZ incl. 2EX 3-band and PWVC, settings, XML. [S31]
- **Writes:** `anlz::ANLZ` implements `BinWrite` (so ANLZ serialisation exists in 0.3.0). `pdb::Page` implements only `BinRead` in 0.3.0 docs ("not yet writable"). [S32][S33]
- Unreleased on `main` (CHANGELOG): custom `BinWrite` for `IndexPageContent`, 4-byte alignment fixes for labels/artwork/tags/Ucs2le strings, "Encapsulate database reading/writing in a lazy container", `IndexEntry` constructors, PR #220 (merged 2026-03-19, RobinMcCorkell) "Initial support for adding new rows to a page" (`insert_row`, `heap_bytes_required`), PR #250 merged 2026-06-18. [S31][S34]
- **PR #259 (acrilique, opened 2026-07-16, OPEN, awaiting Holzhaus/Swiftb0y review):** "Extending the device API to create exports from scratch" — `DeviceExportWriter::create`, `add_track` (dedups artists/albums/genres/keys/labels), playlists, tag categories; writes PDB + `*SETTING.DAT` + directory layout; **no ANLZ generation, no audio copy, append-only**. Hardware note: CDJ-350 crashes in TRACK menu if any track row < 221 bytes (cause unknown; padding set empirically). [S35] Older PR #103 (erictapen, 2024, draft) produced PDBs rekordbox could read but "far from mergeable". [S36]
- Other open PRs: XML export (#225/#226/#237/#251/#254), ANLZ path calc (#264), ext cue comment sync (#270), round-trip preservation (#271/#272). Closed bug #240: ANLZ ExtendedCue parse failure on .EXT. [S34]
- **Usable as Rust dependency:** yes (MPL-2.0 is GPLv3-compatible); but for *writing* you'd pin a git rev of `main` or a PR branch. Most principled/best-documented codebase; writer least hardware-proven.

## 5. crate-digger (Deep Symmetry) — Java + Kaitai Struct, reference

- Repo: https://github.com/Deep-Symmetry/crate-digger (API: NOASSERTION; Java; created 2018-11-06; pushed 2026-10-07; 235 stars; 27 forks; 9 open issues/PRs). [S37]
- Licence: **EPL-2.0 with secondary licences MPL-2.0 or LGPL-3.0** (README). Embeds Remote Tea (LGPL-2.0), Kaitai compiler (GPL-3.0, build-time), Kaitai Java runtime (MIT). [S38]
- **Licence clarified for your use-case, Sept 2026:** issue #49 fixed stale `meta.license: EPL-1.0` in both `.ksy` files → now `EPL-2.0 OR MPL-2.0 OR LGPL-3.0-only` (commit 4d1e726, 2026-09-11). Issue #50: maintainer (brunchboy) agreed **parsers generated from the `.ksy` specs carry the same licence + secondary-licence election** (commit 7c9d535, README updated). Filed by sebasje for a GPL-2.0-or-later project. [S39][S40][S41]
- `.ksy` files: `src/main/kaitai/rekordbox_pdb.ksy`, `src/main/kaitai/rekordbox_anlz.ksy`; `exportExt.pdb` is parsed by instantiating `rekordbox_pdb(true)`. [S38]
- Releases: 0.2.1 (2025-07-21: exportExt.pdb tags, metadata archives for Opus Quad), 0.2.0 (2024-05-04), 0.1.6 (2022-03-07). Unreleased: reworked string-offset subtypes for Album/Tag/TagTrack, corrected row-count interpretation ("wrong since the beginning"), `raw_bank` in PSSI. [S42]
- Recent commits: Dec 2025–Mar 2026 doc additions (last-transaction/sequence fields, page header, index pages, page flags) by RobinMcCorkell and acrilique — the same people driving rekordcrate's writer. [S41]
- Reads only (plus NFS fetch from players). No write support; "There are still more tables to be figured out"; index tables not used. [S38]
- **Usable from Rust:** reference only (Kaitai has no Rust serialisation target; Mixxx uses the C++ target).

## 6. Deep Symmetry "rekordbox export analysis" docs (djl-analysis.deepsymmetry.org)

- Source: Antora component in crate-digger `doc/` (`antora.yml`, `modules/ROOT/pages/{exports,anlz,crate_digger}.adoc`). [S43][S44]
- `exports.html` sections: Background; Database Exports (`/PIONEER/rekordbox/export.pdb` + `exportExt.pdb`); File Header; Table Pages (header, data pages, index pages); Table Rows (Album, Artist, Artwork, Color, Genre, History Playlist, History Entry, Key, Label, Playlist Tree, Playlist Entry, Tag, Tag Track, Track); DeviceSQL Strings (short ASCII, long, ISRC); Footnotes. exportExt.pdb contains tables `03 tags` and `04 tag_tracks` (worked out by @voidc). History table type 13 "not yet studied". Copyright 2018–2024; footnote cites Robin McCorkell's Dec 2025 row-count bit discovery. **No OneLibrary/exportLibrary.db coverage.** [S45][S46]
- `anlz.html`: PMAI header + tags PQTZ, PCOB, PCO2, PPTH, PVBR, PWAV, PWV2, PWV3, PWV4, PWV5, PWV6, PWV7, PSSI; file types .DAT/.EXT/**.2EX** (CDJ-3000 three-band). Unknowns flagged: PVBR structure, many header bytes, PSSI bank values, 3-band detail scaling. PQT2 and PWVC not covered. [S47]
- Site landing page is the DJ Link packet analysis (dysentery); export analysis linked as a sibling. [S48]

## 7. pyrekordbox (dylanljones) — Python, MIT, reference

- Repo: https://github.com/dylanljones/pyrekordbox (MIT; pushed 2026-10-14; 497 stars; 12 open issues/PRs). PyPI 0.5.0 (2026-10-14), Python ≥3.11. [S49][S50]
- Reads/writes: XML read+write; MySettings parse+write (not DEVSETTING.DAT); master.db read + limited write (field edits, add/delete only for DjmdContent/Playlist/SongPlaylist/Album/Artist/Genre/Label); ANLZ parse + modify-and-save (PWV6/PWV7/PWVC unsupported; "creating" ANLZ "planned"); **0.5.0 adds OneLibrary (Device Library Plus) read AND write**, PVB2 tag, PSSI v1, keeps unknown tags on rebuild. **No export.pdb support** (roadmap item only). Tested rekordbox 5.8.6/6.7.7/7.0.9. [S51][S52]
- Docs state the Device Library Plus key "is not license or machine dependent and all Device Libraries are encrypted with the same key". [S53]
- Rust usability: reference only (useful for OneLibrary schema and SQLCipher handling; baken credits it for the passphrase scheme).

## 8. rex (kimtore) — Go, unlicensed

- https://github.com/kimtore/rex — "rekordbox exporter for Mixxx". GitHub API `license: null`; no LICENSE file in listing → **no licence = not reusable**. Go; created 2023-03-16; pushed 2025-08-13 (one fix by acrilique); 71 stars; 2 open issues. [S54][S55]
- Reads Mixxx `mixxxdb.sqlite3`; writes `PIONEER/rekordbox/export.pdb` + copies audio; **no waveforms, beat grid, hot cues**; "Do not use files generated from this project on a live gig"; files open in rekordbox, "usable to varying degrees" on a few Pioneer devices; Denon Prime 4 shows no library; "many tests are broken". [S54]
- "REX" in the task = this project (only match found). Reference only, and only for ideas (no licence).

## 9. Mixxx — C++, GPL-2.0-or-later, reader only

- LICENSE: GPL v2 "or (at your option) any later version". [S56]
- `src/library/rekordbox/rekordboxfeature.cpp` includes `rekordbox_pdb.h`/`rekordbox_anlz.h` (Kaitai-generated C++ from `lib/rekordbox-metadata/rekordbox_pdb.ksy` + `rekordbox_anlz.ksy`), reads keys/genres/artists/albums/playlist entries/tracks/playlist tree into Mixxx's own tables, reads ANLZ beat grids and cues (memory/loops/hot cues); opens PDB read-only. History not handled. [S57][S58]
- Mixxx 2.3 blog (2020): cannot prepare a device for CDJs; "contributions welcome". HN thread (Jan 2025): Holzhaus (hlzhs) said the format wasn't reverse-engineered enough to write — superseded by 2026 developments above. [S59][S60]

## 10. Other projects found

- **dj-usb-tkit (haivala)** — https://github.com/haivala/dj-usb-tkit — AGPL-3.0-or-later; Rust+Tauri 2; created 2026-06-30; pushed 2026-10-16; 42 stars. Reads rekordbox/Mixxx libraries; writes Pioneer "PDB/eDB" USB databases, repairs sticks, optional Engine DJ library. Deps: `rusqlite` with `bundled-sqlcipher-vendored-openssl`, no rekordcrate/rbox/binrw — i.e. its own writer. No hardware list published. AGPL would govern a combined work. [S61][S62][S63]
- **export-studio (claw1200)** — https://github.com/claw1200/export-studio — GitHub says GPL-2.0; Rust+Electron; created 2026-10-06; alpha. Writes "DeviceLibrary and OneLibrary" to USB by **vendoring rbxport 1.9.0 crates** (`rbl-pdb`, `rbl-anlz`, `rbl-onelibrary`, …) under GPL-2.0-or-later plus its own `rb-usb`/`rb-anlz`. [S64][S17][S65]
- **djbooth (gakonst)** — https://github.com/gakonst/djbooth — TypeScript + Rust; **no licence detected** (API `license: null`); claims to emit Device Library and OneLibrary artefacts incl. DAT/EXT/2EX with "physical-player validation". Not reusable without a licence. [S66][S67]
- **rekordbox-parser (npm)** — JS wrapper around crate-digger's Kaitai specs; read-only. [S68]
- **rb2engine (PyPI, MIT)** — reads export.pdb + ANLZ, writes Engine DJ library; converter, not a PDB writer. [S69]
- **Now Playing `onelibrary-connect`** — read-only OneLibrary adapter, opens exportLibrary.db "using the known decryption key"; no licence/repo given (UNVERIFIED open source). [S70]
- **mixxx-utils (FrankwaP)** — Mixxx → rekordbox XML exporter (the "mixxx2rekordbox" used by baken testers; exact repo for that name UNVERIFIED). [S71]
- **dysentery** — Clojure, EPL-1.0, Pro DJ Link packet analysis only; not about exports. [S72]
- **"Unbox"/"Unboxed"**: no matching open-source project found (UNVERIFIED that one exists). **"djsync"**: no repo found; closest is LibTools4DJs. **Lexicon**: proprietary; Lexicon's documented workflow is to export rekordbox XML and let rekordbox write the stick (baken consumes Lexicon XML); no evidence it writes export.pdb itself (UNVERIFIED). **crate-digger write support**: none exists. [S73][S74][S75]

## 11. OneLibrary / exportLibrary.db — format facts gathered

- AlphaTheta notice (2026-03-18): OneLibrary (formerly "Device Library Plus") is used by CDJ-3000X, OPUS-QUAD, OMNIS-DUO, XDJ-AZ; Device Library by CDJ-3000, XDJ-XZ, XDJ-RX3, XDJ-RR, XDJ-1000MK2, XDJ-700, CDJ-900NXS2, CDJ-2000NXS2 and older; rekordbox ≥7.2.11 writes both on every export. [S76]
- Encrypted SQLite via SQLCipher 4, same key on every stick (pyrekordbox, baken); open sequence `PRAGMA cipher=sqlcipher; legacy=4; key` (rbxport). Table list (baken): content, artist, album, genre, label, key, color, playlist, playlist_content, image, menuItem, category, sort, property, myTag, cue, history, history_content, hotCueBankList, hotCueBankList_cue, (+recommendedLike, myTag_content). [S53][S8][S16]
- Open-source writers: baken-export (`onelibrary.rs`, untested on players), rbxport `rbl-onelibrary` (one physical XDJ-AZ check), rbox `one_library` (GPL-3.0), pyrekordbox 0.5.0 (Python). No Deep Symmetry documentation exists for it.

## 12. Recommendation for a GPLv3 Rust/Tauri app

1. **Primary candidate: `baken-export` (MIT, crates.io).** Only writer with public, dated, real-player confirmations (CDJ-3000, CDJ-2000NXS2, CDJ-2000NXS) and an in-crate waveform generator. Risks: XML-centric input model, thin docs, OneLibrary untested on hardware, single maintainer, very young (first publish 2026-09-18).
2. **Second: rbxport `rbl-pdb`/`rbl-anlz`/`rbl-onelibrary` (GPL-2.0-or-later, git).** Most thorough format documentation (incl. exportExt.pdb, sync files, DEVSETTING) and byte-parity against rekordbox 7.2.11, but hardware validation is mostly firmware-simulation; has 109 open issues and a history of player lockups.
3. **Third: rekordcrate `main` + PR #259 (MPL-2.0).** Cleanest code and the de-facto Rust reference parser; use for *reading/validating* what you write (round-trip tests) even if you write with something else.
4. Use crate-digger `.ksy` + djl-analysis as the spec of record (EPL-2.0/MPL-2.0/LGPL-3.0, generated code explicitly covered); use pyrekordbox for OneLibrary schema cross-checks.
5. Avoid: rex (no licence), djbooth (no licence), rbox for PDB (has none; repo 404).

## Sources

- [S1] https://github.com/M-Igashi/baken
- [S2] https://docs.rs/crate/baken/latest
- [S3] https://api.github.com/repos/M-Igashi/baken
- [S4] https://crates.io/api/v1/crates/baken-export ; https://docs.rs/crate/baken-export/latest
- [S5] https://docs.rs/baken-export/latest/baken_export/
- [S6] https://docs.rs/crate/baken-export/latest/source/src/
- [S7] https://docs.rs/crate/baken-export/latest/source/src/pdb/mod.rs ; https://docs.rs/crate/baken-export/latest/source/src/anlz/mod.rs
- [S8] https://github.com/M-Igashi/baken/blob/main/crates/baken-export/src/onelibrary.rs
- [S9] https://github.com/M-Igashi/baken/issues/116
- [S10] https://github.com/M-Igashi/baken/issues/242
- [S11] https://api.github.com/repos/chrisle/rbxport
- [S12] https://github.com/chrisle/rbxport
- [S13] https://github.com/chrisle/rbxport/commits/main
- [S14] https://github.com/chrisle/rbxport/blob/main/Cargo.toml
- [S15] https://github.com/chrisle/rbxport/tree/main/crates
- [S16] https://github.com/chrisle/rbxport/blob/main/docs/reference/usb-export-db.md
- [S17] https://github.com/claw1200/export-studio/blob/main/native/UPSTREAM.md
- [S18] https://docs.rs/crate/rbox/latest
- [S19] https://crates.io/api/v1/crates/rbox
- [S20] https://github.com/dylanljones/rbox (404 on 2026-10-18)
- [S21] https://api.github.com/repos/dylanljones/rbox (404 on 2026-10-18)
- [S22] https://pypi.org/project/rbox/
- [S23] https://cdn.jsdelivr.net/npm/rbox-js@0.1.7/README.md
- [S24] https://docs.rs/crate/rbox/0.1.8/source/Cargo.toml
- [S25] https://docs.rs/crate/rbox/0.1.8/source/src/ ; https://docs.rs/rbox/0.1.4/rbox/
- [S26] https://api.github.com/repos/Holzhaus/rekordcrate
- [S27] https://github.com/Holzhaus/rekordcrate
- [S28] https://github.com/Holzhaus/rekordcrate/commits/main
- [S29] https://crates.io/api/v1/crates/rekordcrate
- [S30] https://github.com/Holzhaus/rekordcrate/blob/main/README.md
- [S31] https://github.com/Holzhaus/rekordcrate/blob/main/CHANGELOG.md
- [S32] https://docs.rs/rekordcrate/latest/rekordcrate/pdb/struct.Page.html ; https://docs.rs/rekordcrate/latest/rekordcrate/pdb/index.html
- [S33] https://docs.rs/rekordcrate/latest/rekordcrate/anlz/struct.ANLZ.html
- [S34] https://github.com/Holzhaus/rekordcrate/issues?q=write+OR+serialize+OR+export+OR+BinWrite ; https://github.com/Holzhaus/rekordcrate/pull/220
- [S35] https://github.com/Holzhaus/rekordcrate/pull/259
- [S36] https://github.com/Holzhaus/rekordcrate/pull/103
- [S37] https://api.github.com/repos/Deep-Symmetry/crate-digger
- [S38] https://github.com/Deep-Symmetry/crate-digger
- [S39] https://github.com/Deep-Symmetry/crate-digger/issues/49
- [S40] https://github.com/Deep-Symmetry/crate-digger/issues/50
- [S41] https://github.com/Deep-Symmetry/crate-digger/commits/main
- [S42] https://github.com/Deep-Symmetry/crate-digger/blob/main/CHANGELOG.md
- [S43] https://github.com/Deep-Symmetry/crate-digger/tree/main/doc
- [S44] https://github.com/Deep-Symmetry/crate-digger/tree/main/doc/modules/ROOT/pages
- [S45] https://djl-analysis.deepsymmetry.org/rekordbox-export-analysis/exports.html
- [S46] https://djl-analysis.deepsymmetry.org/rekordbox-export-analysis/
- [S47] https://djl-analysis.deepsymmetry.org/rekordbox-export-analysis/anlz.html
- [S48] https://djl-analysis.deepsymmetry.org/
- [S49] https://api.github.com/repos/dylanljones/pyrekordbox
- [S50] https://pypi.org/pypi/pyrekordbox/json
- [S51] https://github.com/dylanljones/pyrekordbox
- [S52] https://github.com/dylanljones/pyrekordbox/releases
- [S53] https://pyrekordbox.readthedocs.io/en/latest/formats/devicelib_plus.html (via search snippet; page fetch returned 404 on 2026-10-18)
- [S54] https://github.com/kimtore/rex
- [S55] https://api.github.com/repos/kimtore/rex ; https://github.com/kimtore/rex/commits/github
- [S56] https://github.com/mixxxdj/mixxx/blob/main/LICENSE ; https://api.github.com/repos/mixxxdj/mixxx
- [S57] https://github.com/mixxxdj/mixxx/blob/main/src/library/rekordbox/rekordboxfeature.cpp
- [S58] https://github.com/mixxxdj/mixxx/tree/main/lib/rekordbox-metadata
- [S59] https://mixxx.org/news/2020-07-20-new-in-2-3-rekordbox-support
- [S60] https://news.ycombinator.com/item?id=42770170
- [S61] https://github.com/haivala/dj-usb-tkit
- [S62] https://api.github.com/repos/haivala/dj-usb-tkit
- [S63] https://github.com/haivala/dj-usb-tkit/blob/main/backend/Cargo.toml
- [S64] https://github.com/claw1200/export-studio ; https://api.github.com/repos/claw1200/export-studio
- [S65] https://github.com/claw1200/export-studio/tree/main/native/crates
- [S66] https://github.com/gakonst/djbooth
- [S67] https://api.github.com/repos/gakonst/djbooth
- [S68] https://npmjs.com/package/rekordbox-parser
- [S69] https://pypi.org/project/rb2engine/
- [S70] https://nowplayingapp.com/help/reference/integrations/onelibrary/
- [S71] https://github.com/FrankwaP/mixxx-utils
- [S72] https://github.com/Deep-Symmetry/dysentery
- [S73] https://github.com/M-Igashi/baken/blob/main/docs/dj-software-compatibility.md
- [S74] https://www.nuget.org/packages/libtools4djs
- [S75] https://github.com/topics/rekordbox?o=desc&s=updated
- [S76] https://alphatheta.com/en/information/important-notice-for-customers-using-usb-devices-with-our-dj-equipment/
