# Research 2: rekordbox "OneLibrary" / "Device Library Plus" (exportLibrary.db)

Researched 2026-10-18. Every factual claim carries a source URL (full list at the end). Items I could not confirm from a primary or independent source are marked **UNVERIFIED**.

## TL;DR

- OneLibrary == Device Library Plus (DLP). AlphaTheta renamed it in rekordbox 7.2.5 (2025-10-21). It is a single SQLCipher-encrypted SQLite file at `PIONEER/rekordbox/exportLibrary.db` and lives *beside* the legacy `export.pdb`; rekordbox 7 writes both on every USB export.
- The encryption key is static (same for every stick, not machine/licence bound) and has been published by several community projects. SQLCipher 4-style parameters; some reports say the exact KDF settings are non-default. rekordbox 7 is the only official writer; djay Pro and VirtualDJ are officially sanctioned third-party writers; Traktor was announced but still "coming".
- Players that read ONLY OneLibrary: CDJ-3000X, OPUS-QUAD, OMNIS-DUO, XDJ-AZ, plus CDJ-1500X and XDJ-AN (2026 models). Players that read ONLY legacy Device Library (export.pdb): CDJ-3000 (fw 3.30 added OneLibrary but was withdrawn; current 3.22 has no OneLibrary), XDJ-XZ, XDJ-RX3, XDJ-RR, XDJ-1000MK2, XDJ-700, CDJ-900NXS2, CDJ-2000NXS2 and older.
- Open-source read support exists (pyrekordbox, rbox, onelibrary-connect, libdjinterop merged 2026-10-14, rekordcrate PR open, rekordlib). Write support exists in pyrekordbox (ORM `add_*`), rbox (claims), rekordlib (claims) and baken (`--onelibrary`, untested on real players). No one has publicly documented a third-party-written exportLibrary.db loading on a real OneLibrary player.
- Main risks: shipping a static key (DMCA §1201 trafficking exposure vs. §1201(f) interoperability defence), undocumented and already-churning spec ("AlphaTheta is reviewing the specifications of both Device Library and OneLibrary"), and AlphaTheta's own guides being withdrawn twice (CDJ-3000 fw 3.30, rekordbox 7.2.12) for library-loading bugs.

## 1. What the format is

**File path.** `PIONEER/rekordbox/exportLibrary.db` on the USB/SD root (DjManager issue: `{usbRoot}/PIONEER/rekordbox/exportLibrary.db`; 0xdevalias gist; DropDex2 imports from `PIONEER/rekordbox/exportLibrary.db`). A libdjinterop reviewer questioned the path, and the author pointed to a Mixxx issue that "resolved the discrepancy" (PR #196) — treat the exact casing as something to confirm against a real stick.

**Container.** A SQLite database encrypted with SQLCipher (rekordbox's own description via Lexicon: "encrypted SQLite database"; pyrekordbox docs: "SQLite database encrypted with SQLCipher"; libdjinterop: "single SQLite database encrypted with SQLCipher 4"). onelibrary-connect opens it with `PRAGMA cipher = sqlcipher; PRAGMA legacy = 4` via better-sqlite3-multiple-ciphers (gist).

**Encryption parameters (community findings, partly conflicting):**
- baken PR #259 reimplements the cipher without SQLCipher: AES-256-CBC, random IV per page, HMAC-SHA512 over ciphertext+IV+page number, PBKDF2-HMAC-SHA512 with 256,000 rounds, HMAC key derived with 2 further rounds over salt XOR 0x3a, 16-byte salt in place of the SQLite header, 80 reserved bytes per page (this is SQLCipher 4's default profile). The PR reports that a stock SQLCipher 4.14.0 opened a 583-track DB written by baken "with the passphrase alone", and that rekordbox's own exportLibrary.db passed the HMAC check.
- DjManager issue #300 reports the opposite: "SQLCipher 3 SHA1/64k" and "SQLCipher 4 SHA512/256k" both failed to decrypt a rekordbox 7.2.11 export, so the author loaded rekordbox's own bundled `sqlite3.dll` via ctypes/FFI. The key there is described as "64 ASCII bytes, passed directly to `sqlite3_key(db, key, 64)`".
- pyrekordbox sets no SQLCipher pragmas at all and relies on `sqlcipher3` defaults (source read of `pyrekordbox/onelibrary/database.py`).
- **UNVERIFIED which is right**; the likely explanation is passphrase-vs-raw-key handling, but I found no source stating that. A Rust implementation should test against a real rekordbox 7.2.13+ export before committing to a cipher config.

**Key.** pyrekordbox docs: the key "does not appear to depend on the license or machine" and "all Device Libraries use the same key". pyrekordbox ships it as an obfuscated constant decoded at runtime and validates user-supplied keys by prefix; onelibrary-connect ships the same obfuscated blob (base85 → XOR → zlib) and calls it "the built-in SQLCipher key used by Pioneer DJ devices"; the DjManager issue prints the key in full; the gist says the exportLibrary.db key is different from the `master.db` key. So: **yes, publicly known**. libdjinterop deliberately does NOT ship it (passphrase must be supplied by caller / `ONELIBRARY_PASSPHRASE` env var). rekordcrate PR #269 uses "Rekordbox's default database key". I have not reproduced the key in this document.

**Accompanying files.**
- `PIONEER/rekordbox/export.pdb` — legacy DeviceSQL database, still written (Deep Symmetry; gist).
- `PIONEER/rekordbox/exportExt.pdb` — small companion DeviceSQL database holding tag / tag-track tables for newer players (Deep Symmetry exports page). It is part of the legacy export, not of OneLibrary.
- ANLZ `.DAT` / `.EXT` / `.2EX` under `PIONEER/USBANLZ/` — OneLibrary does not duplicate analysis: "beat grids, waveforms, hot cues and loops are not in the database at all, since they live in the ANLZ files" and "the `cue` table is exported empty" (libdjinterop PR #196 / README). `content.analysisDataFilePath` must match the ANLZ path (DjManager).
- Artwork: referenced by `image.path`; onelibrary-connect returns "only references, not the image files" (Now Playing docs). Folder name for OneLibrary artwork **UNVERIFIED** (legacy artwork folder documented by Deep Symmetry; `_m` 240px variants).
- `exportLibrary.db-journal`: **UNVERIFIED** — no source mentions it; it would be a normal SQLite rollback journal if present. baken says it removes "rekordbox's leftover OneLibrary files" when it rewrites the DB, which hints that rekordbox leaves auxiliary files, but names none.

**Coexistence with export.pdb.** Yes by design: "Two libraries will coexist within the USB storage device: OneLibrary (a new library) and Device Library (a traditional library). The type of library used depends on the DJ equipment model" (USB Export Guide PDF, Oct 2025). The two libraries are managed separately; playlists/histories created on one kind of player are not shown on the other (same PDF, FAQ).

## 2. rekordbox terminology and history

- rekordbox 6.6.11 (2023-03-07): "OPUS-QUAD compatibility added"; an opt-in Preferences toggle `[DJ System] > [Device] > [Library] > "Device Library Plus (OPUS-QUAD ONLY)"` converted the USB library.
- rekordbox 6.8.1 (2023-12-19): Device Library Plus is always exported alongside Device Library — "as of version 6.8.1 this is no longer optional" (Digital DJ Tips; Lexicon blog says the same). Some third-party pages say 6.8.2 — **UNVERIFIED** which minor version; official notes I could read say 6.8.1 "improvements to ... Device Library Plus display".
- rekordbox 7.2.4 (2025-10-07): "Fixed a bug where playlists exported to Device Library Plus could appear in the wrong order."
- rekordbox 7.2.5 (2025-10-21): "Renamed Device Library Plus to OneLibrary." Same day: CDJ-3000 fw 3.30 and the OneLibrary launch announcement.
- rekordbox 7.2.11 (2026-03-03): the version AlphaTheta's March-2026 notice tells users to be on; "both OneLibrary and Device Library are generated automatically" on export.
- rekordbox 7.2.12 (2026-03-17): withdrawn — "tracks exported to a USB device using rekordbox ver. 7.2.12 may not load correctly on AlphaTheta and Pioneer DJ hardware"; 7.2.13 (2026-03-31) "Fixed the issue with the USB export function that occurred in rekordbox ver. 7.2.12." Newest listed on the release-notes page as of today: 7.2.19.
- Official FAQ language (rekordbox.com OneLibrary FAQ): "OneLibrary (formerly Device Library Plus) ... a newer library format that rekordbox creates on USB or SD storage"; players support "either the traditional Device Library or OneLibrary"; conversion via right-click `OneLibrary` > `Convert from Device Library`; conversion is one-way and overwrites an existing OneLibrary; "After a conversion ... newly exported tracks and playlists are automatically added to both the Device Library and the OneLibrary". LINK EXPORT is unchanged.
- alphatheta.com/onelibrary FAQ: Device Library Plus and OneLibrary "are essentially the same ... fully compatible"; "AlphaTheta plans to gradually rename it to OneLibrary"; OneLibrary "is supported in rekordbox ver. 7"; "full compatibility between different applications is not guaranteed".
- The rekordbox UI shows two nodes under a USB device in the Devices tree: `Device Library` and `OneLibrary`, each with `All Tracks`, playlists and histories (OneLibrary Compatible USB Device Export Guide PDF, March 2026).

## 3. Which players require which format

Primary source: AlphaTheta "Important Notice for Customers Using USB Devices" (2026-03-18), cross-checked with the two rekordbox PDFs.

| Player | Reads | Notes |
|---|---|---|
| CDJ-3000X | OneLibrary only | "can only browse tracks and playlists from a USB if OneLibrary ... has been exported" (FAQ). AlphaTheta "plans firmware updates for the CDJ-3000X that will support more flexible library loading" (notice). No CDJ-3000X firmware release notes found — **UNVERIFIED** whether that update has shipped. |
| OPUS-QUAD | OneLibrary only | First DLP device (rekordbox 6.6.11 notes; Lexicon/DDJT). |
| OMNIS-DUO | OneLibrary only | AlphaTheta notice; alphatheta.com/onelibrary. |
| XDJ-AZ | OneLibrary only | AlphaTheta notice. |
| CDJ-1500X, XDJ-AN | OneLibrary only | Listed on alphatheta.com/onelibrary and rekordbox FAQ; rekordbox 7.2.16 (2026-07-09) added compatibility. |
| CDJ-3000 | Device Library only (current) | fw 3.30 (2025-10-21) added OneLibrary: "If Device Library and OneLibrary are both present ... OneLibrary will load by default" — withdrawn; fw 3.22 (2026-01-15): "This update does not include OneLibrary support." Pioneer DJ notice 2026-01-07: "CDJ-3000 has reverted to the Device Library format"; AlphaTheta "is reviewing the specifications of both Device Library and OneLibrary". |
| XDJ-XZ, XDJ-RX3, XDJ-RR, XDJ-1000MK2, XDJ-700, CDJ-900NXS2, CDJ-2000NXS2 and older | Device Library only | AlphaTheta notice. (XDJ-AX also listed as legacy in the March-2026 guide.) |
| DDJ-FLX10 | n/a | Controller, no standalone USB browsing; not mentioned in any OneLibrary source. **UNVERIFIED** that it is irrelevant beyond that. |

Firmware versions that added OneLibrary support on the OneLibrary-only units: none needed; they shipped with it. Opus Quad firmware history: **UNVERIFIED** (no changelog found).

Practical consequence: to be usable on every current player a stick needs BOTH `export.pdb` (+ `exportExt.pdb`, ANLZ) and `exportLibrary.db`. AlphaTheta's own guidance to users is exactly that (notice step 2; guide "Exporting to a brand new USB Drive with Both OneLibrary and Device Library").

## 4. Open-source and third-party implementations (as of 2026-10-18)

| Project | Lang / licence | Read | Write | Key handling | Notes |
|---|---|---|---|---|---|
| pyrekordbox (dylanljones) | Python / MIT | yes (`pyrekordbox.onelibrary.OneLibrary`) | ORM `add_content`, `add_playlist`, `add_my_tag`, `commit`, `update_content_count` etc. | obfuscated constant in repo; prefix check on user keys | No pragmas set; relies on sqlcipher3. Docs list the full schema (below). |
| rbox (dylanljones) | Rust (+ Python, npm) / GPL-3.0 | yes | "Read and write ... One Library" claimed; no write example | not described | Repo was private per rekordcrate #172 (Jul 2026); docs.rs build failed for 0.1.8. |
| onelibrary-connect (chrisle) | TypeScript / MIT | yes, read-only | no | built-in obfuscated key | Used by Now Playing app. |
| libdjinterop (xsco) | C++ / LGPL-3.0 | yes — PR #196 merged 2026-10-14 | no ("everything that would change a database throws unsupported_operation"); PR #204 (empty-DB writer) closed | passphrase NOT shipped; caller supplies | Needs `-DEXPERIMENTAL_ENABLE_SQLCIPHER=ON`. Follow-ups #211/#212 on reference schema. |
| rekordcrate (Holzhaus) | Rust / MPL-2.0 | PR #269 (opened 2026-08-29, still open) read-only, Diesel schema at `src/device_library_plus/schema.sql`, bundled SQLCipher | out of scope | "Rekordbox's default database key"; arbitrary keys excluded | Issue #172 open since 2025-09. Collaborator asked about attribution/licence provenance of the models. |
| rekordlib (acrilique) | Zig / MPL-2.0 | yes | claims "read, write, and create ... OneLibrary (exportLibrary.db)" | not described | `-Donelibrary=system-sqlcipher|vendored-sqlcipher|off`. Ported from rekordcrate. |
| baken (M-Igashi) | Rust CLI / MIT | — | yes, `expressport --onelibrary` | not described publicly | Writes export.pdb by default; OneLibrary omits My Tags, artwork, hot cue banks; "No OneLibrary player has read the `--onelibrary` output yet". Own crypto, no SQLCipher dep since 4.6.0. |
| DjManager (Radexito) | — | — | issue #300 (open) plans FFI to rekordbox's sqlite3.dll | printed in issue | Documents schema and seed strategy. |
| Mixxx | C++ / GPL-2.0 | issue #15556 open (2025-10-31) | proposals#20 (rev. 2026-10-14) puts OneLibrary in scope; "whether Mixxx should ship code using the fixed SQLCipher passphrase is an open maintainer question" (build flag / DLP-only / user-supplied) | — | Original proposal excluded OneLibrary citing DMCA §1201. |
| crate-digger / Deep Symmetry | Java / docs | no issues mention OneLibrary (0 results) | — | — | djl-analysis documents export.pdb and exportExt.pdb only. |
| Lexicon DJ | commercial | — | no evidence Lexicon writes exportLibrary.db; its 2023 blog says it could not open the file "because they lacked the key"; 2024–2026 changelogs mention only "Rekordbox exporter" fixes (XML-based) | — | **UNVERIFIED** negative. |
| djay Pro (Algoriddim) | commercial, official partner | yes | yes (desktop; iOS read-only) | — | OneLibrary launch partner 2025-10-21. |
| VirtualDJ (Atomix) | commercial, official since 2026-09 | yes | yes | — | Had written rekordbox-readable sticks for ~5 years "with nobody's permission" before being sanctioned. |
| Traktor Pro 4 / Play | announced | — | "coming in the next few months" still, a year later | — | **UNVERIFIED** shipped. |
| Serato, Engine DJ | — | no announcement found | — | — | Mixxx commenter claims the spec was shared with Serato — **UNVERIFIED**. |
| fourfour "pioneer-usb-writer" | mentioned in gist | — | — | — | Could not locate the repo — **UNVERIFIED**. |

**Schema** (pyrekordbox "Rekordbox OneLibrary Format" page; matches DjManager and libdjinterop, 22 tables / 4 indexes per libdjinterop PR #204):
- `property` (deviceName, dbVersion, numberOfContents, createdDate, backGroundColorType, myTagMasterDBID) — `dbVersion` reported as `1000` by libdjinterop and `'10000'` by DjManager: **conflicting, UNVERIFIED**; check a real export. libdjinterop `verify()` requires the `key` table plus exactly one `property` row with non-empty `dbVersion`.
- `content` (content_id, title, titleForSearch, subtitle, bpmx100, length, trackNo, discNo, artist_id_artist/_remixer/_originalArtist/_composer/_lyricist, album_id, genre_id, label_id, key_id, color_id, image_id, djComment, rating, releaseYear, releaseDate, dateCreated, dateAdded, path, fileName, fileSize, fileType, bitrate, bitDepth, samplingRate, isrc, djPlayCount, isHotCueAutoLoadOn, isKuvoDeliverStatusOn, kuvoDeliveryComment, masterDbId, masterContentId, analysisDataFilePath, analysedBits, contentLink, hasModified, cueUpdateCount, analysisDataUpdateCount, informationUpdateCount). DjManager: `analysedBits = 41` for fully analysed tracks; `contentLink` constant `0x000C0700` (meaning unknown); `masterDbId` = source library id.
- Lookups: `artist`, `album` (isComplation [sic], nameForSearch), `genre`, `label`, `key`, `color`, `image` (path).
- `playlist` (playlist_id, sequenceNo, name, image_id, attribute, playlist_id_parent), `playlist_content` (playlist_id, content_id, sequenceNo).
- `cue` (kind, colorTableIndex, cueComment, isActiveLoop, beatLoop num/den, inUsec/outUsec, in/out150FramePerSec, MPEG frame/abs/decoding-start/file-offset/sample fields) — exported empty by rekordbox per libdjinterop; `kind` values unknown (DjManager).
- `history`, `history_content`, `hotCueBankList`, `hotCueBankList_cue`, `myTag`, `myTag_content`, `menuItem` (kind, name), `category`, `sort`, `recommendedLike`.
- Key notation is stored "as written (which may be Camelot)" (libdjinterop).
- Seed rows for `menuItem`/`category`/`sort`/`color`/default `myTag` must be copied from a native rekordbox export (DjManager); none are published.

## 5. Risks for a non-partner (GPLv3) writer

1. **Key distribution / anti-circumvention.** The key is a static secret extracted from rekordbox binaries (Frida hook on `sqlite3_key`, per DjManager). 17 U.S.C. §1201(a)(2) bars trafficking in circumvention tools; §1201(f)(2)-(3) permits developing and sharing means "solely for the purpose of enabling interoperability of an independently created computer program with other programs". That is the defence every project implicitly relies on, but no source shows AlphaTheta's position, and I found no takedown against pyrekordbox or others (pyrekordbox did remove its `download_key` CLI in v0.4.4 with no stated reason). Mixxx and libdjinterop both chose NOT to ship the key; libdjinterop requires the caller to provide it. Mitigation options the community uses: user-supplied passphrase, build-time feature flag, or reading the key from a locally installed rekordbox.
2. **Spec is closed and explicitly in flux.** "The specification is not public and is not managed by a consortium" (Mixxx #15556). AlphaTheta: "reviewing the specifications of both Device Library and OneLibrary ... the fix will take more time" (2026-01-07). CDJ-3000X "more flexible library loading" firmware is planned (2026-03-18). Expect the on-disk contract to change.
3. **AlphaTheta's own stack has broken twice.** CDJ-3000 fw 3.30 withdrawn (tracks/playlists not shown when libraries from older rekordbox were present); rekordbox 7.2.12 withdrawn (exported tracks "may not load correctly"). A third-party writer will be blamed first when something fails on stage.
4. **Players rejecting non-rekordbox DBs.** No evidence either way of signatures/checksums beyond SQLCipher's per-page HMAC (which any correct writer satisfies). libdjinterop's `verify()` only checks structure. But: no published confirmation that a third-party-written exportLibrary.db loads on a CDJ-3000X/OPUS-QUAD/XDJ-AZ. baken: "No OneLibrary player has read the --onelibrary output yet." djay Pro / VirtualDJ writes do load — but they are sanctioned partners and may have the official spec. **UNVERIFIED** hardware acceptance for community writers.
5. **rekordbox rewrites what it opens.** baken warns not to verify a stick in rekordbox "because rekordbox can rewrite a device library it opens"; the FAQ says converting overwrites an existing OneLibrary. A user who plugs your stick into rekordbox may get playlists duplicated or clobbered.
6. **Licence compatibility.** SQLCipher is BSD-style and fine with GPLv3; rbox is GPL-3.0 (compatible), rekordcrate/rekordlib are MPL-2.0 (compatible with GPLv3 under MPL §3.3), pyrekordbox/onelibrary-connect MIT. rekordcrate maintainers raised attribution/provenance questions about AI-generated models; keep derivation history clean.
7. **Unknown fields.** `contentLink`, `cue.kind`, `analysedBits` semantics, `dbVersion` value, seed tables — all reverse-engineered, some conflicting.

## 6. Does rekordbox 7 still write export.pdb? Does CDJ-3000 prefer exportLibrary.db?

- Yes: rekordbox 7 (6.8.1 onward) writes both; "In this version [7.2.11], both OneLibrary and Device Library are generated automatically, so the USB works on either type of hardware" (AlphaTheta notice). There is no longer a toggle to disable either (DDJT).
- CDJ-3000 preference: only firmware 3.30 did so ("OneLibrary will load by default" when both exist), and that is exactly what caused missing playlists when the two libraries were out of sync, so AlphaTheta withdrew it. Current CDJ-3000 (3.22) ignores exportLibrary.db entirely. Digital DJ Tips described 3.30's fallback failing "when both databases exist on the same USB". So today the only players that look at exportLibrary.db are the OneLibrary-only units, which ignore export.pdb.

## Recommendation for the app

- Ship legacy export (`export.pdb` + `exportExt.pdb` + ANLZ) as the baseline; it covers every player except CDJ-3000X/1500X, OPUS-QUAD, OMNIS-DUO, XDJ-AZ/AN.
- Treat OneLibrary as an optional, feature-flagged writer: SQLCipher 4 profile (verify against a 7.2.13+ export), schema per pyrekordbox/rekordcrate `schema.sql`, seed tables copied from a real export, `cue` left empty, key supplied by the user or read from an installed rekordbox rather than embedded — mirroring libdjinterop's and Mixxx's stance.
- Budget for hardware testing on an actual OneLibrary player before advertising support; nobody in the open-source world has published a success.

## Sources

- https://rekordbox.com/en/support/faq/onelibrary-7/
- https://rekordbox.com/en/support/faq/devicelibraryplus-6/
- https://rekordbox.com/en/2025/10/dj-brands-unite-to-launch-onelibrary/
- https://rekordbox.com/en/2025/10/rekordbox-v725-release-information/
- https://rekordbox.com/en/2023/03/rekordbox-v6611-release-information/
- https://rekordbox.com/en/2026/03/important-notice-regarding-rekordbox-v7212/
- https://rekordbox.com/en/support/releasenote/
- https://cdn.rekordbox.com/files/20251021171528/USB_export_guide_en_251007.pdf
- https://cdn.rekordbox.com/files/20260317132052/OneLibrary-Compatible-USB-Device-Export-.pdf
- https://alphatheta.com/en/onelibrary/
- https://alphatheta.com/en/information/important-notice-for-customers-using-usb-devices-with-our-dj-equipment/
- https://www.pioneerdj.com/en/news/2026/cdj-3000-firmware-ver330-important-notice/
- https://downloads.support.alphatheta.com/firmwares/dj-players/CDJ-3000/CDJ-3000-Firmware-Change-History-Ver330-en.pdf
- https://downloads.support.alphatheta.com/firmwares/dj-players/CDJ-3000/CDJ-3000-Firmware-Change-History-Ver322-en.pdf
- https://www.digitaldjtips.com/onelibrary/
- https://www.digitaldjtips.com/rekordbox-device-library-plus/
- https://www.digitaldjtips.com/alphatheta-pulls-cdj-3000-firmware-after-playlist-issues/
- https://www.digitaldjtips.com/virtualdj-now-officially-supports-alphathetas-onelibrary/
- https://cdm.link/onelibrary-dj-interoperability/
- https://www.lexicondj.com/blog/everything-you-need-to-know-about-device-library-plus-and-more
- https://www.lexicondj.com/dj-software-updates
- https://reallychrism.substack.com/p/why-im-not-cheering-onelibrary
- https://community.algoriddim.com/t/onelibrary-usb-setup-for-cdj-3000/39780
- https://gist.github.com/0xdevalias/b803476793b56f7c45e6361799168eb0
- https://github.com/dylanljones/pyrekordbox
- https://raw.githubusercontent.com/dylanljones/pyrekordbox/master/pyrekordbox/onelibrary/database.py
- https://pyrekordbox.readthedocs.io/en/latest/formats/onelibrary.html
- https://docs.rs/crate/rbox/0.1.8
- https://github.com/chrisle/onelibrary-connect
- https://nowplayingapp.com/help/reference/integrations/onelibrary/
- https://github.com/xsco/libdjinterop
- https://github.com/xsco/libdjinterop/issues/177
- https://github.com/xsco/libdjinterop/pull/196
- https://github.com/xsco/libdjinterop/pull/204
- https://github.com/Holzhaus/rekordcrate/issues/172
- https://github.com/Holzhaus/rekordcrate/pull/269
- https://github.com/acrilique/rekordlib
- https://github.com/M-Igashi/baken
- https://github.com/M-Igashi/baken/pull/259
- https://github.com/Radexito/DjManager/issues/300
- https://github.com/mixxxdj/mixxx/issues/15556
- https://github.com/mixxxdj/proposals/issues/20
- https://github.com/Deep-Symmetry/crate-digger/issues?q=exportLibrary+OR+OneLibrary+OR+%22Device+Library+Plus%22
- https://djl-analysis.deepsymmetry.org/rekordbox-export-analysis/exports.html
- https://www.law.cornell.edu/uscode/text/17/1201
