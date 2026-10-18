# Research 5 — Engine DJ library format and open-source writers

Date of research: 2026-10-18. Every factual claim cites a URL (numbered, see Sources). Items I could not confirm are marked **UNVERIFIED**.

## 0. Current state of Engine DJ (October 2026)

- enginedj.com/downloads lists Engine **Desktop v5.1.0 (2026-09-01)** and Engine **OS v5.0.4** as the latest releases. 5.0.0 shipped 2026-05-13 (RGB waveforms, Import Assistant for Serato/iTunes/rekordbox/Traktor); 4.3.4 (2025-11-18) was the last 4.x desktop release listed. [1]
- Engine 4.5.x/4.6.x were Rane System One-only firmware builds (Serato DJ Pro support needed "a new Engine DJ 4.6 build exclusive to the System One"). [2][3] libdjinterop issue #186 also records that OS 4.3.4 was the latest version supporting the SC5000 at that time, and a commenter states 5.1.0 is specific to the PRIME 4 G2 (**UNVERIFIED** beyond that comment). [3]
- Engine OS hardware list (17 devices): PRIME 4 G2, Rane SYSTEM ONE, PRIME GO+, PRIME 4+, Mixstream Pro GO / Pro+ / Pro, SC LIVE 2/4, SC6000/SC6000M, LC6000, PRIME 4, PRIME 2, PRIME GO, SC5000/SC5000M. The page does not state per-device OS versions. [4] DJ TechTools reports 5.0 brings RGB waveforms to "every Engine DJ device going back to the original SC5000". [5]

## 1. Format

### 1.1 Folder layout and database files

- Convention: the database directory's last path component is `Engine Library`; the v2/v3 library lives at `Engine Library/Database2/m.db`, and the API is given the `Engine Library` directory, not `Database2`. [6][7][8]
- Engine Prime 1.x (schema 1.x) used `Engine Library/m.db` (metadata) + `p.db` (performance data) plus `sm.db`/`sp.db` mirrors; each with a `-journal` file. [9]
- Engine's official third-party-tool guidance (titled for v3.0, still the only such doc) says: all `.db` files are in `Database2`; do **not** change the schema of any of them or Engine "will refuse to load the database"; Engine may later drop any data not conforming to its schema; never run your tool while Engine DJ is open (corruption); put extension data in your own DB in a *subfolder* of `Database2` and `ATTACH` Engine's DB read-only (`mode=ro`); bind tracks by `(originDatabaseUuid, originId)` and playlists by the `PlaylistPath` view, not by SQLite rowid. [10]
- `hm.db` ("history main"), `sm.db`, `stm.db`: community reports place `hm.db` at `Engine Library/Database2/hm.db`, holding history (imported from performance volumes when Engine opens), with `sm.db`/`stm.db` holding streaming data; playlist data lives only in `m.db`. [11][12] libdjinterop's engine_library headers only name `m.db` and say "one or more SQLite databases" may exist; they never mention `hm.db`. [7][8] Whether a player *requires* `hm.db` to accept a USB stick: **UNVERIFIED** (no source says so; Mixxx/libdjinterop/rb2engine/PenBridge all write only `m.db` and report working exports, see §2).

### 1.2 Schema versions (Information table) → Engine versions

`Information` columns (v2): `id` (always 1), `uuid`, `schemaVersionMajor`, `schemaVersionMinor`, `schemaVersionPatch`, `currentPlayedIndiciator` (sic), `lastRekordBoxLibraryImportReadCounter`. [13]

Mapping from libdjinterop `engine_schema.hpp` (master, Oct 2026) [14]:

| Schema | Engine product |
|---|---|
| 1.6.0 … 1.18.0 (11 values; 1.18.0 has separate Desktop/OS variants) | Engine OS 1.0.0 – 1.6.2 / Engine Prime 1.1.1 – 1.6.1 |
| 2.18.0 | Engine DJ Desktop/OS 2.0.x – 2.1.x |
| 2.20.1 | 2.2.x – 2.3.x |
| 2.20.2 | 2.4.0 |
| 2.20.3 | 3.0.0 – 3.0.1 |
| 2.21.0 | 3.1.0 – 3.4.0 |
| 2.21.1 | 4.0.0 |
| 2.21.2 | 4.0.1 |
| 3.0.0 | 4.1.0 – 4.2.1 |
| 3.0.1 | 4.3.x |
| 3.0.2 | 4.5.0 / 5.0.x (`latest_schema`) |

PenBridge independently reports: 2.21.0 readable by OS 3.1–5.x; 3.0.1 readable by OS 4.3+; 3.0.2 read by OS 5.0+ (its default). [15] rb2engine targets 3.0.1 and 3.0.2. [16] Whether 5.1.0 bumped the schema past 3.0.2: **UNVERIFIED** (no open libdjinterop issue mentions a 3.0.3 as of 2026-10-18 [17]).

Official compatibility statements: 4.1.0 converts any drive it touches to a format "only compatible with versions 4.0.1 or later" [18]; legacy 1.x databases are unsupported from 4.0 (migrate via Engine DJ 3.4.0 first) [19]; "Drives exported from Engine Desktop 2.0 will only work with Engine OS 2.0" [1]. Community: Engine 5.0 performs forward migrations 4.3.4 does not know, so rollback needs a manual backup, though one user reports a 5.0.1-exported stick worked on a 4.3.4 player. [20]

### 1.3 Tables

v2 (schema 2.x) tables exposed by libdjinterop: `Track`, `Playlist`, `PlaylistEntity`, `Information`, `AlbumArt`, `ChangeLog`. [7] v3 (schema 3.x) adds a separate `PerformanceData` table (and drops `ChangeLog` from the API). [8]

- **Track** (v2): id, playOrder, length (s), bpm (int), year, `path` (relative to the Engine Library folder), filename, bitrate, `bpmAnalyzed` (REAL), albumArtId (1 = none), file_bytes, title, artist, album, genre, comment, label, composer, remixer, `key` (0–23), rating (0 = none), albumArt, timeLastPlayed, isPlayed, fileType, isAnalyzed, dateCreated, dateAdded, isAvailable, isMetadataOfPackedTrackChanged, `isPerfomanceDataOfPackedTrackChanged` (sic, misspelled in the real DB), playedIndicator, isMetadataImported, pdbImportKey, streamingSource, uri, isBeatGridLocked, originDatabaseUuid, originTrackId, **trackData, overviewWaveFormData, beatData, quickCues, loops** (blobs on Track in v2), thirdPartySourceId, streamingFlags, explicitLyrics, activeOnLoadLoops, lastEditTime (set by trigger). [21]
- **Track** (v3): same metadata columns but the five blob columns, thirdPartySourceId and activeOnLoadLoops move to **PerformanceData** (`trackId`, trackData, overviewWaveFormData, beatData, quickCues, loops, thirdPartySourceId, activeOnLoadLoops); a trigger auto-creates a blank PerformanceData row per Track insert. New in 3.0.2: `albumArtSourceHash`. [22][23]
- **Playlist**: id, title, parentListId (0 = root), isPersisted, nextListId (sibling order is a singly linked list, 0 = end), lastEditTime, isExplicitlyExported. [24]
- **PlaylistEntity**: id, listId, trackId, databaseUuid, nextEntityId (linked-list ordering, 0 = end), membershipReference (unknown; 0 is safe). [25]
- Engine Prime 1.x used `Crate`, `CrateTrackList`, `CrateHierarchy`, `PlaylistTrackList`, `MetaData`/`MetaDataInteger`, `Historylist`, `Preparelist` tables and a `PerformanceData` table in `p.db`. [9] Crates no longer exist as a table in the v2+ API (Mixxx exports crates as playlists; see §2).
- Path convention: PenBridge maps a rekordbox file at `/Contents/Artist/Track.mp3` to `../Contents/Artist/Track.mp3` so one stick serves CDJs and Engine; the same `..`-relative convention is used in libdjinterop's example. [15]
- AlbumArt.hash: 20-byte hash, URL-safe base64 without padding (28 chars); how it is computed from image bytes is unknown. [3]

### 1.4 Blobs (performance data)

Compression: libdjinterop `zlib_compress` writes a 4-byte **big-endian** uncompressed-length header followed by a zlib stream (`Z_DEFAULT_COMPRESSION`), i.e. Qt `qCompress` layout; the Mixxx wiki documents the same. [26][9] trackData, beatData, quickCues and overviewWaveFormData are compressed; **loops is not compressed** ("Note that the loops blob is not compressed"). [27][28]

- **trackData**: sampleRate (double), samples (int64), key (int32), averageLoudnessLow/Mid/High (doubles; used by players to scale waveforms; 0 = no rescale; units unknown — open issue #103). [29][17]
- **beatData**: sampleRate (double), samples (double), isBeatgridSet (uint8), then default and adjusted beat grids, each a list of markers {sampleOffset double, beatNumber int64, numberOfBeats int32 (beats until next marker, 0 = last), unknown int32 (0 ok)}. First marker customarily at beat −4, last at N+1. [30] Mixxx wiki: doubles/ints in beatData are little-endian; BPM = sampleRate·60·(lastBeat−firstBeat)/(lastOffset−firstOffset). [9]
- **quickCues**: up to 8 cues {label (len byte + chars), sampleOffset double (−1 = unset), colour ARGB bytes}, then adjustedMainCue (double), isMainCueAdjusted (byte), defaultMainCue (double). Big-endian doubles (confirmed by an independent implementer). [31][32] Default pad colours cue1 `EAC532` … cue8 `158EE2`. [9]
- **loops**: uncompressed; int64 LE count, then per loop {label len uint8 + chars, start double LE, end double LE, isStartSet uint8, isEndSet uint8, A,R,G,B}; no padding. [28]
- **overviewWaveFormData** (uncompressed layout, all big-endian): int64 N, int64 N (duplicate), double samplesPerWaveformPoint (may be fractional), then N×3 uint8 (low, mid, high), then 3 uint8 maximum point. 27 + 3N bytes. No colour field; the UI colours bands blue/green/white. [33][34] Recommended N / samples-per-point come from `calculate_overview_waveform_extents(sample_count, sample_rate)`; the exact constants live in libdjinterop's util code which I could not fetch — **UNVERIFIED** (commonly cited ~1024 points). Engine v1 let apps write a high-resolution waveform; v2+ only stores the overview and the player derives the scrolling waveform from audio. [6][15]
- Positions are sample offsets; divide by sampleRate (usually 44100) for seconds. [9]

### 1.5 Key enum and BPM

- `key` 0–23: 0 C major, 1 A minor, 2 G major, 3 E minor, 4 D major, 5 B minor, 6 A major, 7 F♯ minor, 8 E major, 9 D♭ minor, 10 B major, 11 A♭ minor, 12 F♯ major, 13 E♭ minor, 14 D♭ major, 15 B♭ minor, 16 A♭ major, 17 F minor, 18 E♭ major, 19 C minor, 20 B♭ major, 21 G minor, 22 F major, 23 D minor (i.e. alternating major/minor around the circle of fifths; 0 = Camelot 8B). [35][9]
- BPM: `Track.bpm` integer (from tags) and `Track.bpmAnalyzed` REAL; the authoritative tempo is the beat grid in beatData. [21][9] Rating stored 0–100 in steps of 20 (1.x observation). [9]

## 2. Open-source writers / readers

| Project | Lang / licence | Schemas | Write | Last activity | Notes |
|---|---|---|---|---|---|
| **libdjinterop** (xsco) | C++17, LGPL-3.0 [36] | 1.6.0–3.0.2 (21 schemas) [14] | Yes: tracks, beat grids, hot cues, loops, overview waveforms, playlists/crates; no album art or history [36] | tag 0.27.1 (2026-03-05); 0.27.0 (2025-12-01) [37] | README claims OS 1.0.3–4.3.3 / Desktop 1.0.1–4.3.0 tested on SC5000 and Mixstream Pro only. Open issues: future-versioned DB loading (#99, open since 2023 — a newer schema throws "unsupported"), 4.5/4.6 Rane exclusivity (#186), ANLZ/OneLibrary/rekordcrate work (#208/#177/#194). [17][38][3] |
| **Mixxx** Engine export | C++, GPL-2+ | via libdjinterop, pinned **0.27.1** (ExternalProject static build; `ENGINEPRIME` option default ON) [39] | Yes (crates since 2.4; playlists in 2.6) | 2.5.1 (2025-04-27) "libdjinterop 0.24.3 – support for Engine 4.1/4.2"; 2.6.0 (unreleased) "Update to libdjinterop 0.27.0", "Engine DJ: add support for playlists export" [40] | Shipped in 2.4.0 (2024-02-16) as "export crates, playlists and the library to Engine DJ OS devices" [41]; community thread notes Prime 4 "listed but untested", no streaming tracks. [42] Not documented in the 2.5 manual library chapter. [43] |
| **endjine** (uklotzde) | Rust, MPL-2.0 | not stated | sqlx-based; "all modifying tasks are considered experimental" | 0.0.6 on crates.io (2025-06-01); 135 commits [44][45] | Only Engine-related crate on crates.io; searches for "djinterop"/"enginelibrary" return nothing. [46][47] |
| **rb2engine** (jrgutier) | Python 3.11+, MIT | 3.0.1, 3.0.2 (refuses unknown) | Own writer (DDL captured from Engine); beatgrids, 8 hot cues, loops, playlists, album art; **no waveforms** (player generates) | 31 commits, © 2026 | Round-trip tested encode(decode(blob)) == blob for beatData/quickCues/loops/trackData. [16] |
| **PenBridge** (athousanddetails) | Swift/macOS, MIT | 2.21.0, 3.0.1, 3.0.2 | Own writer from libdjinterop reference DBs; writes overview waveform, cues, grids, art | © 2026 | Builds DB on internal disk, `PRAGMA integrity_check`, then copies. [15] |
| **djtools** (nateranda) | Go, MIT | n/a | Engine **import only**; export planned | v0.1.0 2025-04-26 [48] | |
| **DeCU / DJCU** (ATGR) | closed, paid macOS | v6.8 needs Engine DJ ≥ 4.1 | yes | — | Commercial reference point; broke with Engine 3.0.0 along with Lexicon. [49][50] |
| **Lexicon** | closed | — | yes | — | Engine 3.0.0 regression: SC6000 said "Database … is from a newer version of Engine and is not compatible"; no staff fix in thread. [50] |

Python bindings for libdjinterop: none found on PyPI (**UNVERIFIED** that none exist privately). [51] Deep Symmetry (crate-digger/dysentery) covers Pioneer PDB/ANLZ only; PenBridge credits it for the rekordbox side. [15][52] No Kaitai `.ksy` for Engine blobs found; crate-digger's Kaitai specs are Pioneer-only. [52]

## 3. Player behaviour

- **Migration on insert**: official — Engine OS 4.1.0 "the collection will be updated in a way that will only be compatible with versions 4.0.1 or later" when a USB/SD is used, and large collections should be converted on the desktop. [18] Engine DJ 2.0 migrated Engine Prime drives automatically on first connect, keeping the old collection. [53] Legacy 1.x databases are *not* migrated by 4.0+. [19]
- **Analysis on the player**: SC6000 guide — "when you load a track to play, SC6000 PRIME will automatically analyze it (if it has not already been analyzed)"; Prime 4 FAQ — the unit is "ready to automatically analyze any tracks on a connected drive" not in its database. [54][55] So a writer may omit waveforms (rb2engine does) but must supply beat grids/cues itself to be preserved.
- **File systems**: Prime 4 / Prime 4 G2 FAQs — drives must be FAT32 or exFAT with an **MBR** partition table; no capacity limit; exFAT recommended ≥ 1 TB and for internal SATA drives. NTFS is not listed anywhere official; community reports say NTFS drives are rejected. [55][56][57]
- **Player OS by device**: not published per device on enginedj.com; the hardware page lists 17 Engine OS devices without versions. [4] SC5000 last supported at OS 4.3.4 per libdjinterop maintainer (**UNVERIFIED** officially). [3]
- "Newer version" error text seen on SC6000: "Database of … is from a newer version of Engine and is not compatible with this version." [50]

## 4. Risks and mitigations

1. **Schema churn**: 21 schema versions since 2017; 4.1 (3.0.0) moved blobs out of Track into PerformanceData, 3.0.2 added albumArtSourceHash. [14][22][23] Mitigation: pin a target schema per export (PenBridge/rb2engine pattern), store DDL per schema, refuse unknown versions, expose a "target device/OS" picker, and ship a compatibility note as libdjinterop #186 recommends. [3][15][16]
2. **libdjinterop lag**: 4.1 (Oct 2024) was covered in Mixxx only by 2.5.1 (Apr 2025); 5.0 (May 2026) corresponds to 3.0.2, already supported in 0.27.x; 5.1 unknown. libdjinterop refuses newer schemas outright (#99). [40][14][38]
3. **Encryption**: Engine databases are plain SQLite — Engine's own guidance tells tools to `ATTACH` them read-only, and libdjinterop needs only SQLite3 + zlib for Engine; only AlphaTheta OneLibrary is SQLCipher-encrypted. [10][36]
4. **Multiple DB files**: history (`hm.db`) and streaming DBs exist on desktop volumes; players appear to accept sticks with only `m.db` (Mixxx/rb2engine/PenBridge). Treat as **UNVERIFIED** until tested on real hardware.
5. **Corruption**: never write while Engine DJ is open; write to a temp DB, run `PRAGMA integrity_check`, then copy (PenBridge). [10][15]
6. **Licence**: libdjinterop is LGPL-3.0. GNU's matrix: using (linking) an LGPLv3 library from a GPLv3-or-later program is "OK", and copying LGPLv3 code in yields a GPLv3 combination. Static linking is fine for a GPLv3 app; the LGPL's relink/object-code obligations only matter for non-GPL apps, and you must convey libdjinterop's source if you distribute it. [58][59] A Rust port (own writer, like rb2engine) avoids the C++ FFI entirely and is the route taken by every non-Mixxx tool found.

## Sources

1. https://enginedj.com/downloads
2. https://www.digitaldjtips.com/rane-system-one-now-seamlessly-supports-serato-dj-pro/
3. https://github.com/xsco/libdjinterop/issues/186
4. https://enginedj.com/hardware
5. https://djtechtools.com/2026/05/13/engine-dj-5-0-on-bard-stems-on-the-system-1-rgb-waveforms-for-everyone/
6. https://raw.githubusercontent.com/xsco/libdjinterop/master/include/djinterop/engine/engine.hpp
7. https://raw.githubusercontent.com/xsco/libdjinterop/master/include/djinterop/engine/v2/engine_library.hpp
8. https://raw.githubusercontent.com/xsco/libdjinterop/master/include/djinterop/engine/v3/engine_library.hpp
9. https://github.com/mixxxdj/mixxx/wiki/Engine%20library%20format
10. https://support.enginedj.com/support/solutions/articles/69000834165-engine-dj-v3-0-support-for-third-party-database-tools
11. https://community.enginedj.com/t/multi-source-sets-and-shared-merged-history-no-start-time-on-history-exports/58299
12. https://community.enginedj.com/t/engine-dj-5-0-0-sync-manager-does-not-create-playlist-folder-structure-on-usb-sd-drives-works-on-ssd/70166/14
13. https://raw.githubusercontent.com/xsco/libdjinterop/master/include/djinterop/engine/v2/information_table.hpp
14. https://raw.githubusercontent.com/xsco/libdjinterop/master/include/djinterop/engine/engine_schema.hpp
15. https://github.com/athousanddetails/penbridge
16. https://github.com/jrgutier/rb2engine
17. https://github.com/xsco/libdjinterop/issues
18. https://support.enginedj.com/support/solutions/articles/69000861060-engine-dj-4-1-release-notes
19. https://support.enginedj.com/en/support/solutions/articles/69000856026-engine-dj-unsupported-databases-for-engine-4-0
20. https://community.enginedj.com/t/engine-dj-5-0-library-compatibility/68994
21. https://raw.githubusercontent.com/xsco/libdjinterop/master/include/djinterop/engine/v2/track_table.hpp
22. https://raw.githubusercontent.com/xsco/libdjinterop/master/include/djinterop/engine/v3/track_table.hpp
23. https://raw.githubusercontent.com/xsco/libdjinterop/master/include/djinterop/engine/v3/performance_data_table.hpp
24. https://raw.githubusercontent.com/xsco/libdjinterop/master/include/djinterop/engine/v2/playlist_table.hpp
25. https://raw.githubusercontent.com/xsco/libdjinterop/master/include/djinterop/engine/v2/playlist_entity_table.hpp
26. https://raw.githubusercontent.com/xsco/libdjinterop/master/src/djinterop/engine/encode_decode_utils.cpp
27. https://raw.githubusercontent.com/xsco/libdjinterop/master/src/djinterop/engine/v2/overview_waveform_data_blob.cpp
28. https://raw.githubusercontent.com/xsco/libdjinterop/master/src/djinterop/engine/v2/loops_blob.cpp
29. https://raw.githubusercontent.com/xsco/libdjinterop/master/include/djinterop/engine/v2/track_data_blob.hpp
30. https://raw.githubusercontent.com/xsco/libdjinterop/master/include/djinterop/engine/v2/beat_data_blob.hpp
31. https://raw.githubusercontent.com/xsco/libdjinterop/master/include/djinterop/engine/v2/quick_cues_blob.hpp
32. https://community.enginedj.com/t/loops-in-database/66328
33. https://raw.githubusercontent.com/xsco/libdjinterop/master/include/djinterop/engine/v2/overview_waveform_data_blob.hpp
34. https://cdn.inmusicbrands.com/engine/51/Engine%20DJ%20-%20User%20Guide%20-%20v5.1.0.pdf
35. https://raw.githubusercontent.com/xsco/libdjinterop/master/include/djinterop/musical_key.hpp
36. https://github.com/xsco/libdjinterop
37. https://github.com/xsco/libdjinterop/tags
38. https://github.com/xsco/libdjinterop/issues/99
39. https://raw.githubusercontent.com/mixxxdj/mixxx/main/CMakeLists.txt
40. https://raw.githubusercontent.com/mixxxdj/mixxx/main/CHANGELOG.md
41. https://mixxx.org/news/2024-02-16-mixxx-2-4-0-features/
42. https://community.enginedj.com/t/mixxx-now-supports-exporting-to-engine/54743
43. https://manual.mixxx.org/2.5/en/chapters/library.html
44. https://docs.rs/crate/endjine/latest
45. https://github.com/uklotzde/endjine
46. https://crates.io/api/v1/crates?q=engine%20dj&per_page=20
47. https://crates.io/api/v1/crates?q=djinterop&per_page=20
48. https://pkg.go.dev/github.com/nateranda/djtools
49. https://atgr-production-team.sellfy.store/p/y06n/
50. https://community.enginedj.com/t/engine-dj-v3-0-0-database-issues-when-using-third-party-tools-lexicon-de-cu/47248
51. https://pypi.org/ (searches for djinterop / pydjinterop / python-djinterop returned no package)
52. https://github.com/athousanddetails/penbridge (credits Deep-Symmetry/crate-digger for Pioneer PDB/ANLZ Kaitai specs)
53. https://enginedj.com/news/articles/engine-dj-v2-0-faq
54. https://cdn.inmusicbrands.com/denondj/EngineOS/16/Engine%20v1.6%20User%20Guides/SC6000%20PRIME%20-%20User%20Guide%20-%20v1.3.pdf
55. https://support.enginedj.com/support/solutions/articles/69000809366-denon-dj-prime-4-frequently-asked-questions
56. https://support.enginedj.com/support/solutions/articles/69000882239-prime-4-g2-frequently-asked-questions
57. https://community.enginedj.com/t/usb-stick-is-not-recognized-on-prime4-usb-stick-wird-nicht-erkannt-an-prime-4/19821
58. https://www.gnu.org/licenses/gpl-faq.html#AllCompatibility
59. https://www.gnu.org/licenses/gpl-faq.html#LGPLStaticVsDynamic
