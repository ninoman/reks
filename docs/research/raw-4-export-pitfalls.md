# Research 4: Pitfalls and requirements for a hand-written rekordbox USB export (export.pdb + ANLZ)

Researched 2026-10-18. Every claim carries a URL; items I could not confirm are marked **UNVERIFIED**.
Official sources (AlphaTheta/Pioneer manuals, rekordbox manual/FAQ/notices) are preferred; reverse-engineering
sources (Deep Symmetry djl-analysis, crate-digger, rekordcrate, pyrekordbox) and third-party writers (Lexicon,
rekordbox_converter, DJUSB/crate-digger issue #46, rex) are flagged as such.

## 0. The big-picture landscape change you must design around (2025-2026)

- rekordbox now writes **two** libraries to a stick: the legacy **Device Library** (`PIONEER/rekordbox/export.pdb`,
  DeviceSQL) and **OneLibrary** (formerly "Device Library Plus", an SQLCipher-encrypted SQLite file `exportLibrary.db`).
  Players read one or the other, not both. Official split (AlphaTheta notice):
  OneLibrary: CDJ-3000X, OPUS-QUAD, OMNIS-DUO, XDJ-AZ. Device Library: CDJ-3000, XDJ-XZ, XDJ-RX3, XDJ-RR,
  CDJ-2000NXS2 and older, XDJ-1000MK2, XDJ-700, CDJ-900NXS2.
  https://alphatheta.com/en/information/important-notice-for-customers-using-usb-devices-with-our-dj-equipment/
- The rekordbox FAQ adds CDJ-1500X and XDJ-AN to the OneLibrary-only group and says the CDJ-3000X "can browse tracks and
  playlists on a USB storage device only if OneLibrary" exists. https://rekordbox.com/en/support/faq/trouble-shooting-7/
- `exportLibrary.db` is a SQLCipher-encrypted SQLite DB; third-party readers (Now Playing's `onelibrary-connect`) open it
  "using the known decryption key" (key source undocumented). https://nowplayingapp.com/help/reference/integrations/onelibrary/
- Consequence for this project: an `export.pdb`-only writer covers every CDJ-2000/NXS/NXS2, CDJ-3000, XDJ-1000MK2,
  XDJ-RX2/RX3, XDJ-XZ. It will show **nothing** on Opus Quad, Omnis Duo, XDJ-AZ, CDJ-3000X, CDJ-1500X, XDJ-AN unless you
  also write OneLibrary. Lexicon writes both to the same stick. https://www.lexicondj.com/manual/sync-to-pioneer-usb
- CDJ-3000 firmware 3.30 (Oct 21 2025) tried to prioritise OneLibrary and was withdrawn (Jan 7 2026) after tracks/playlists
  vanished; CDJ-3000 is back on Device Library and "no OneLibrary preparation is needed".
  https://www.pioneerdj.com/en-gb/news/2026/cdj-3000-firmware-ver330-important-notice
- rekordbox 7.2.12 (Mar 17 2026) was pulled for a USB-export bug: affected tracks show no waveform in the "Preview"
  column and "may not load properly" on hardware; fix is delete + re-export. Evidence that players are strict about
  analysis data. https://rekordbox.com/en/2026/03/important-notice-regarding-rekordbox-v7212/

## 1. Filesystem and media

### 1.1 File systems per player (official where possible)

| Player | FAT16/FAT32 | HFS+ | exFAT | Source |
|---|---|---|---|---|
| CDJ-2000 (orig.), CDJ-2000NXS | yes | yes | no | retailer spec sheets only (FAT, FAT32, HFS+) https://www.greentoe.com/product/Pioneer_CDJ-2000NXS ; UNVERIFIED against official manual |
| CDJ-2000NXS2 | yes | yes | no (not listed) | Pioneer press release lists "FAT, FAT32, HFS+" https://www.pioneerdj.com/it/news/2017/cdj-2000nxs2-w-and-djm-900nxs2-w-limited-edition-white-set-up-with-distinctive-plaques/ ; exFAT absent from the official exFAT list below |
| XDJ-RR | FAT16, FAT32 | yes | no ("NTFS is not supported") | official manual, p.6 (see extracted text) https://cdn.teufelaudio.com/products/PIONEER/XDJ-RR/XDJ-RR_manual_EN |
| XDJ-1000MK2, XDJ-700, CDJ-900NXS | yes | yes | no (not in official exFAT list) | https://rekordbox.com/en/support/faq/v7/ (FAQ q700010) |
| XDJ-RX2 | yes | yes | no (not in list) | same FAQ; manual not fetched (403) |
| CDJ-3000 | yes | yes | yes (fw 1.20+) | https://rekordbox.com/en/2022/03/rekordbox-v662-release-information/ |
| XDJ-XZ | yes | yes | yes (fw 1.23+) | same |
| XDJ-RX3 | yes | yes | yes (fw 1.11+) | same |
| CDJ-3000X, CDJ-1500X, OPUS-QUAD, OMNIS-DUO, XDJ-AZ, XDJ-AN | yes | yes | yes | rekordbox FAQ q700010 (as of July 2026) https://rekordbox.com/en/support/faq/v7/ |

- The official OneLibrary export guide (rekordbox 7.2.11+) states: "Windows: FAT32, exFAT; Mac: FAT32, exFAT, HFS+ ...
  exFAT is now supported only: CDJ-3000X, CDJ-3000, XDJ-AZ, OPUS-QUAD, OMNIS-DUO, XDJ-XZ and XDJ-RX3."
  https://cdn.rekordbox.com/files/20260317132052/OneLibrary-Compatible-USB-Device-Export-.pdf
- The rekordbox 7.2.18 manual ("Supported media (file system)", p.262): rekordbox writes only FAT16/FAT32/exFAT/HFS+;
  "File systems such as NTFS and HFS are not supported ... Use FAT32 for Windows and a FAT32 or HFS+ formatted device for Mac."
  https://cdn.rekordbox.com/files/20260807093645/rekordbox7.2.18_manual_EN.pdf
- rekordbox FAQ: "Your device may be formatted in a file system that is not supported by the DJ equipment. If the file
  system is exFAT, only these DJ equipment are available." https://rekordbox.com/en/support/faq/trouble-shooting-7/
- **Recommendation**: default to FAT32 for universal compatibility; offer exFAT only with a per-player warning.

### 1.2 Partitions, cluster size, limits

- Partition table: official manual says "If multiple partitions are set on the USB device, only the first partition can be
  used. (When rekordbox library information is present, the partition containing the rekordbox library information has
  priority.)" (XDJ-RR manual). MBR vs GPT is **not stated officially**; a third-party tester claims "GPT can stop the CDJ
  from mounting the drive" and recommends MBR. https://djban.com.br/en/rekordbox-usb-tester/ (UNVERIFIED by AlphaTheta).
  Engine DJ users report similar GPT trouble. https://community.enginedj.com/t/usb-sticks-are-not-recognized-on-prime-2/34022
  Practical: write to a single MBR primary partition; the rex project reports `mkfs.fat -F 32 -S 512` sticks work on Pioneer
  hardware. https://github.com/kimtore/rex
- Cluster size: no official guidance found (UNVERIFIED). Let the OS default.
- FAT32 limits: max file size 2^32-1 bytes (just under 4 GB); Windows GUI format caps at 32 GB (archived MS doc:
  https://learn.microsoft.com/en-us/previous-versions/windows/it-pro/windows-2000-server/cc938432(v=technet.10) ;
  2026 reports that only the CLI `format` was raised to 2 TB: https://www.storagenewsletter.com/2026/06/19/microsoft-breaks-past-fat32s-32gb-barrier-extending-support-up-to-2tb/ ).
  Long WAV/AIFF > 4 GB must be rejected or transcoded on FAT32.
- Folder depth / counts (official, XDJ-RR manual, same family firmware as XDJ-RX2/RX3 era): "Folder levels: Up to 8 levels
  (files in folders beyond the 8th level cannot be played)"; "folders beyond the 10,000th folder within a single folder
  cannot be displayed"; "files beyond the 10,000th file within a single folder cannot be displayed"; "It takes time to read
  a connected USB device if ... many folders and files" and "Do not store files or folders other than music files".
  https://cdn.teufelaudio.com/products/PIONEER/XDJ-RR/XDJ-RR_manual_EN  (CDJ-3000/NXS2 manuals not fetched; assume same).
- Path length: Lexicon states "CDJs have a 255-character limit on the full file path" and warns on longer paths.
  https://www.lexicondj.com/manual/all  (third-party; UNVERIFIED officially). Keep `/Contents/...` paths < 255 chars.
- Characters: the third-party tester says avoid `: * ? < > | ;` https://djban.com.br/en/rekordbox-usb-tester/ . FAT32 itself
  forbids `\ / : * ? " < > |`. Sanitise to be safe.
- Unicode normalisation (NFC vs NFD on macOS), case sensitivity, 8.3 short names: **UNVERIFIED** for CDJs. No Pioneer
  source found. Engine DJ users report accented titles failing to play on other hardware
  (https://community.enginedj.com/t/accents-in-song-titles-causes-the-red-tracks-issue/43139). Safe policy: write
  NFC-composed filenames, store exactly the same byte string in `file_path`/PPTH, and avoid case-only collisions (FAT32 is
  case-insensitive). 8.3 entries are generated by the host FS driver; nothing to do.
- Stick size: a venue-side guide recommends 64 GB or below, FAT32. https://support.pirate.com/hc/en-gb/articles/4420241649553

## 2. Folder layout rekordbox writes

- `PIONEER/rekordbox/export.pdb` (Device Library) and `PIONEER/rekordbox/exportExt.pdb` (tags/tag-tracks, newer players).
  https://djl-analysis.deepsymmetry.org/rekordbox-export-analysis/exports.html
- `PIONEER/USBANLZ/P0xx/0000xxxx/ANLZ0000.DAT|.EXT|.2EX`, e.g. `PIONEER/USBANLZ/P016/0000875E/ANLZ0000.DAT`
  https://github.com/Holzhaus/rekordcrate ; a Windows log shows `...PIONEER\USBANLZ\P00B\0000D5A7\ANLZ0000.DAT`
  https://community.pioneerdj.com/hc/en-us/community/posts/22980199519513-Rekordbox-doesn-t-export-some-tracks-to-USB
  (local rekordbox 6 uses a different UUID-style layout: `USBANLZ/0e8/0d4cc-06a6-.../ANLZ0000.DAT`
  https://pyrekordbox.readthedocs.io/en/stable/tutorial/anlz.html ).
- ANLZ folder naming scheme (third-party reverse engineering, from rekordbox_converter `core/src/anlz.rs`): path is
  `/PIONEER/USBANLZ/P{p:03X}/{hash:08X}/ANLZ0000.DAT`, where the hash iterates the audio path's UTF-16 code units with
  `h = (h*0x5BC9 + c)*0x93B5 + c` (wrapping u32), `hash = h % 200003`, and `p` is 7 scattered bits of `hash`
  (shifts 0,1,4,4,5,8,10 masked to 0x01..0x40). Comment claims it "Matches the algorithm used by rekordbox / CDJ
  `CreateAnlzFileFolderPath`" and that "Players ignore the track row's `analyze_path` and hash the audio path themselves."
  https://raw.githubusercontent.com/ModeAxe/rekordbox_converter/main/core/src/anlz.rs  **UNVERIFIED** independently; the
  "players ignore analyze_path" claim conflicts with djl-analysis (which says the path is read from string slot 14).
  Safest: compute the folder with this hash AND write the same string into `analyze_path`.
- Audio: rekordbox copies files into a `Contents` folder at the USB root (Lexicon also "copies music files into the
  Contents folder"). https://www.lexicondj.com/manual/sync-to-pioneer-usb . The exact sub-layout is not officially
  documented; forum users describe artist-sorted subfolders
  (https://forum.djtechtools.com/t/change-the-file-structure-on-my-rekordbox-stick/72172). `/Contents/<Artist>/<Album>/<file>`
  is the commonly observed scheme but **UNVERIFIED** here; players only follow `file_path`, so any layout under
  `/Contents/` that respects depth/char limits works.
- Artwork: `artwork` table rows hold a DeviceSQL `path`; originals are 80x80, newer rekordbox also writes 240x240 with
  `_m` before the extension (`/a/b/foo_m.jpg`). https://djl-analysis.deepsymmetry.org/rekordbox-export-analysis/exports.html
  (folder is `PIONEER/Artwork/...` by convention; exact naming UNVERIFIED).
- Settings files live in `PIONEER/`: `DEVSETTING.DAT`, `DJMMYSETTING.DAT`, `MYSETTING.DAT`, `MYSETTING2.DAT`
  (little-endian, brand strings "PIONEER DJ"/"PioneerDJ"/"PIONEER"). https://pyrekordbox.readthedocs.io/en/latest/formats/mysetting.html
  Lexicon recommends initialising the stick in rekordbox first so these settings exist; "If you want to change user/CDJ
  settings, you need to do it in Rekordbox." https://www.lexicondj.com/manual/sync-to-pioneer-usb
- OneLibrary file: `exportLibrary.db` on the stick (location relative to PIONEER/ not documented in sources fetched).
  https://nowplayingapp.com/help/reference/integrations/onelibrary/
- PPTH: `len_header 0x10`, `len_path` at 0x0c, then the path as **UTF-16BE with trailing NUL** starting at 0x10.
  https://djl-analysis.deepsymmetry.org/rekordbox-export-analysis/anlz.html ; crate-digger ksy declares `encoding: utf-16be`.
  https://raw.githubusercontent.com/Deep-Symmetry/crate-digger/main/src/main/kaitai/rekordbox_anlz.ksy
  Path form is the same USB-root-relative, forward-slash form as `file_path` (e.g. `/Contents/...`) — form inferred from
  the Rockbox port's note that export.pdb references `/Contents/...` paths and the player prefixes the volume root
  (https://git.lenord.me/len0rd/rockbox/commit/b897b400af3469e6bbfb70ae83cb504d754428bb). Treat exact prefix as
  something to copy from a reference export.
- Page size: Henry Betts' notes: "The file consists of fixed size blocks of 4096 bytes. The 4 byte integer at 0x04 is
  always 4096". https://github.com/henrybetts/Rekordbox-Decoding . djl-analysis names the field `len_page`.

## 3. PDB details that break players

Primary spec: https://djl-analysis.deepsymmetry.org/rekordbox-export-analysis/exports.html and
https://raw.githubusercontent.com/Deep-Symmetry/crate-digger/main/src/main/kaitai/rekordbox_pdb.ksy .
Hardware-verified writer findings: crate-digger issue #46 (DJUSB author, "verified against a CDJ with reference rekordbox
PDB exports", model unnamed, not independently confirmed) https://github.com/Deep-Symmetry/crate-digger/issues/46 .
rekordcrate's writer branch got an export "read correctly by a CDJ-350" by round-tripping a real DB and by a
"pdb-from-scratch" MP3 exporter. https://github.com/Holzhaus/rekordcrate/pull/220

### 3.1 File header (page 0)
- `00` zeros; `04` `len_page` (4096); `08` `num_tables`; `0c` `next_unused_page` ("points past the end of the file");
  `10` unknown — issue #46: **must be 5** ("verified against a reference rekordbox export", other values "may cause
  rejection on some firmware versions"); `14` `sequence` (incremented on each edit); `18` zeros; `1c` table pointers
  (`type, empty_candidate, first_page, last_page`). (djl-analysis + ksy)
- **All 20 table types (0..19) must have directory entries, even when empty**: "Missing tables cause the CDJ to ignore
  the entire file." (issue #46). Types: 0 tracks, 1 genres, 2 artists, 3 albums, 4 labels, 5 keys, 6 colors,
  7 playlist_tree, 8 playlist_entries, 9/10 unknown, 11 history_playlists, 12 history_entries, 13 artwork,
  14/15 unknown, 16 columns, 17/18 unknown, 19 history (ksy enum). Each table = an index page (first_page) plus data pages.

### 3.2 Page header
- `00` zeros; `04` `page_index`; `08` `type`; `0c` `next_page`; `10` `sequence`; `14` zeros;
  `18-1a` a **24-bit packed field**: low 13 bits `num_row_offsets` (offsets ever allocated), high 11 bits `num_rows`
  (valid rows). Issue #46: "Treating these as separate fields causes the CDJ to silently skip the page." djl-analysis
  credits Robin McCorkell (Dec 2025) for the bit packing. `num_rows_small`/`num_rows_large` are older names for this
  pair (ksy no longer has them).
- `1b` `page_flags`: data pages `0x24`/`0x34` (bit 4 = page has deleted rows), index pages `0x64`; data page iff
  `flags & 0x40 == 0`. `1c` `free_size`, `1e` `used_size`. Then `transaction_row_count` (`u2`), `transaction_row_index`
  (`u2`, both `0x1fff` if the last transaction failed), `u6` (`0000`), `u7` (0, "1 for history pages"); heap starts at 0x28.
- Issue #46 (data pages): "bytes 0x19-0x1A must be zero ... non-zero values get the page rejected" — this overlaps the
  packed field, so it only holds while `num_rows` fits in the low byte; the issue does not resolve this. Keep
  `num_rows` small per page (rekordbox does) and you satisfy both readings.
- Index page (per issue #46, flags `0x64`): `free_size = 0`, `u5 = 0x1FFF`, `nrl = 0x1FFF`, `u6 = 0x03EC` (1004);
  "the CDJ ignores the table entirely if these values are wrong." djl-analysis adds: magic `0x03ec` at `04`, `next_offset`
  at `06`, `0x03ffffff` at `16`, `num_entries`, `first_empty` (`0x1fff` if none), empty entry `0x1ffffff8`.

### 3.3 Row index, groups, presence flags
- Row offsets grow backwards from page end in groups of 16; each group = 16 x u2 offsets + u2 `row_present_flags`
  + u2 `transaction_row_flags`. Low bit = first row of the group. "Rows whose presence bit is clear must be ignored and
  may not be well-formed." Offsets are relative to the end of the 0x28-byte page header. (djl-analysis, ksy)
- Rekordbox over-provisions heap space after rows ("extra space after rows, large after Track rows"), per Robin McCorkell;
  not required but harmless. https://github.com/Holzhaus/rekordcrate/issues/173
- Allocation: switch to a new page when heap and index would meet; rekordcrate's writer uses a `heap_bytes_required`
  size-query approach. https://github.com/Holzhaus/rekordcrate/pull/220

### 3.4 DeviceSQL strings
- First byte `length_and_kind`: odd value = short ASCII, length = `value >> 1` including the flag byte, max ~126 bytes;
  `0x40` = long ASCII (u2 length incl. 4 header bytes, 1 pad byte, then text); `0x90` = long UTF-16LE (same header).
  No terminators. (ksy; djl-analysis)
- History note: crate-digger issue #14 (2020) flipped to "UTF-16BE", later corrected back to UTF-16LE after @evilfred
  found the leading `00`/flag semantics; current ksy and djl-analysis say **UTF-16LE**.
  https://github.com/Deep-Symmetry/crate-digger/issues/14 ; https://djl-analysis.deepsymmetry.org/rekordbox-export-analysis/exports.html
- ISRC strings are special: flag `0x90`, then `03`, ASCII, NUL (not UTF-16). Write empty ISRC unless you replicate this.
- Cyrillic/accented names are where endianness bugs surface; test with non-Latin titles.

### 3.5 Row subtypes / unknown fields that must be right (issue #46 + djl-analysis)
- Track row subtype `0x0024` (bit 4 => 16-bit offsets), artist `0x0060` (far variant `0x0064`), album `0x0080`
  (`0x0084` far); "Wrong subtypes make rows invisible to the CDJ."
- Track fixed fields: `u3 = 19048?`, `u4 = 30967?` ("always"), `u5 = 29` ("seems to always hold"), `file_type`
  (`0x01` MP3, `0x04` M4A, `0x05` FLAC, `0x0b` WAV, `0x0c` AIFF), `tempo` = BPM*100, `duration` seconds, `sample_rate`,
  `bitrate`, `sample_depth`, `file_size`, then the "3" marker (`u7`) and **21 x u2 string offsets** from the row start:
  0 isrc, 1 lyricist, 2 unknown numeric ("increments when re-exported"), 3, 4, 5 message, 6 publish_track_info
  ("ON"/empty), 7 autoload_hotcues ("ON"/empty), 8, 9, 10 date_added (YYYY-MM-DD), 11 release_date, 12 mix_name, 13,
  14 analyze_path, 15 analyze_date (YYYY-MM-DD), 16 comment, 17 title, 18, 19 filename, 20 file_path.
  Timestamps are **date strings**, not integers.
- Relationship: `file_path` is the USB-root-relative audio path (`/Contents/...`); `analyze_path` is the USB-root-relative
  ANLZ `.DAT` path (`/PIONEER/USBANLZ/P0xx/0000xxxx/ANLZ0000.DAT`); the `.EXT`/`.2EX` share the base name.
  (djl-analysis anlz.html; rekordcrate README)
- Foreign keys: `artist_id`, `album_id`, `genre_id`, `key_id`, `color_id`, `label_id`, `artwork_id`, `composer_id`,
  `original_artist_id`, `remixer_id` reference rows in those tables (0 = none). Color IDs are fixed
  (1 pink ... 8 purple) and the colors table should carry those rows. Key rows hold `id` twice.
- Playlist tree rows: `parent_id`, `sort_order`, `id`, `raw_is_folder`, `name`; entries: `entry_index`, `track_id`,
  `playlist_id`. History tables need valid empty structure (`u7 = 1` on history pages).
- IDs: no official rule; observed exports use dense incrementing IDs. rekordcrate's successful CDJ-350 export preserved
  rekordbox's row order. Keep IDs stable across re-exports so playlists/history keep pointing at the right track.
- `columns` (type 16) table contents are undocumented ("details unconfirmed"); copy from a reference export.
- `exportExt.pdb`: types 3 tags, 4 tag_tracks; tag row subtype `0x0680/0x0684`. Optional for playback; copy an empty
  reference if you want parity. (djl-analysis)

## 4. ANLZ pitfalls

Spec: https://djl-analysis.deepsymmetry.org/rekordbox-export-analysis/anlz.html and crate-digger `rekordbox_anlz.ksy`.
- All sections are big-endian (opposite of PDB). File = `PMAI` header (`len_header` 0x1c, `len_file`) + tags, each
  `fourcc, len_header, len_tag`.
- **PPTH must be the first tag after PMAI** and hold the UTF-16BE NUL-terminated audio path; "Moving PPTH elsewhere
  means the CDJ does not associate the analysis with the track." https://github.com/Deep-Symmetry/crate-digger/issues/46
- Minimal `.DAT` for playback-with-grid: PPTH, PVBR (seek index; structure largely unknown — copy a reference or emit
  the fixed 400-entry zero body UNVERIFIED), PQTZ (beat grid), PWAV (400-byte preview, nexus/2000 strip), PWV2 (100-byte
  tiny preview, CDJ-900), PCOB x2 (memory cues `type 0`, hot cues `type 1`).
- `.EXT` (NXS2 and later): PWV3 (mono detail, 150 entries/s), PWV4 (color preview, 7200 bytes = 1200 cols x 6), PWV5
  (color detail, u16 BE per half-frame: r3 g3 b3 height5), PCO2 x2 (extended cues: colour, comment, quantised loop
  numerator/denominator, RGB), PSSI (song structure; rekordbox 6+ XOR-masks the body — you may omit PSSI).
- `.2EX` (CDJ-3000): PWV6 (3-band preview, 3600 bytes), PWV7 (3-band detail). The ksy comments place PWV6/PWV7 in `.2EX`
  while the prose puts PWV7 in `.EXT`; players fall back to PWV5 if absent. Without `.EXT`/`.2EX` the track still loads
  but CDJ-3000/NXS2 show blue mono or no detail waveform. A third-party tester summarises "Blank waveforms: USBANLZ missing".
  https://djban.com.br/en/rekordbox-usb-tester/
- PQTZ entries are 8 bytes: `beat_number` (1-4), `tempo` = BPM x 100 (u2), `time` in ms at 100% pitch (u4); header
  `unknown` usually `00800000`. Without a grid the player enters "Emergency loop"/no quantise behaviour.
- PCOB/PCPT cue entry 0x38 bytes: `hot_cue` (0 = memory, 1 = A ...), `status` (4 = active loop), `type` (1 point,
  2 loop), `time`/`loop_time` ms, `order_first/order_last`. PCO2/PCP2 is the NXS2 variant (variable length; parsers must
  tolerate truncated entries) adding `color_id` (colors table row), `loop_numerator/denominator`, UTF-16BE comment,
  `color_code` and RGB. Beat Link "prefers PCO2 and falls back to PCOB" — write both for cross-player cue support.
- rekordbox "does not allow 2 memory cues on the same position" (Lexicon) — dedupe. https://www.lexicondj.com/manual/all
- MP3 decoder delay: Mixxx found rekordbox cue times offset ~26/52 ms vs libmad-vs-FFmpeg length, depending on
  Xing/LAME headers. Measure cue `time` with the same decoder convention rekordbox uses (compare against a reference
  export) or cues will land late/early on the player. https://github.com/mixxxdj/mixxx/pull/2119
- Lexicon generates its own CDJ waveforms ("first sync takes longer") — proof a third party can produce PWAV/PWV* that
  players accept. https://www.lexicondj.com/manual/sync-to-pioneer-usb

## 5. Stale files and files not in the DB

- rekordbox manual: delete a track or playlist from the device with the Delete key in [Devices]; preference
  "[Delete Tracks] When a playlist is deleted from the USB storage device ... tracks in the playlist are deleted. Tracks can
  be deleted only if they are not contained on another playlist." (rekordbox 7.2.18 manual, EXPORT mode p.108 and
  Preferences > Device). https://cdn.rekordbox.com/files/20260807093645/rekordbox7.2.18_manual_EN.pdf  Whether the
  `Contents` audio and USBANLZ folders are physically removed is not stated (UNVERIFIED); Lexicon exposes an explicit
  "Clean up unused files" option that "deletes music files from the USB that aren't in the current sync", implying stale
  files are otherwise left behind. https://www.lexicondj.com/manual/sync-to-pioneer-usb
- Players never delete: "music files and playlists are not deleted from the USB when connected to a player."
  https://alphatheta.com/en/information/important-notice-for-customers-using-usb-devices-with-our-dj-equipment/
- Files not in the DB: players read the exported list, not the filesystem; a file dropped into `Contents` by hand "won't be
  found" in rekordbox browse, but appears under the player's FOLDER browse if placed elsewhere, with no waveform/BPM/hot
  cues until the player analyses it. https://forum.djtechtools.com/t/rekordbox-question/41995 . Official manual: in folder
  browse "tracks for which ID3 tags ... have never been loaded are registered using the file name as the track name".
  https://cdn.teufelaudio.com/products/PIONEER/XDJ-RR/XDJ-RR_manual_EN
- Extra junk slows mounting: "It takes time to read a connected USB device if files other than music files are stored"
  (same manual). Orphaned ANLZ folders are harmless to the player but should be garbage-collected.

## 6. Firmware quirks, error codes, and rekordbox's reaction to foreign sticks

- Official error table (XDJ-RR manual, same codes on CDJs): `E-8302 CANNOT PLAY TRACK` = file cannot be read / corrupt;
  `E-8304`/`E-8305 UNSUPPORTED FILE FORMAT` = format not supported. https://cdn.teufelaudio.com/products/PIONEER/XDJ-RR/XDJ-RR_manual_EN
  The third-party tester's mapping ("E-8305 = PIONEER folder not found", "E-8306 export.pdb error") is **not** the official
  meaning. https://djban.com.br/en/rekordbox-usb-tester/
- E-8305 in practice: WAVs with `WAVE_FORMAT_EXTENSIBLE`/non-PCM `wFormatTag` — Pioneer support: "rekordbox ignores it, but
  the CDJs don't"; fix = set wFormatTag to 1 or re-encode. https://forum.audacityteam.org/t/change-format/64479
  Lexicon auto-fixes exactly these: WAV_EXTENSIBLE headers, 32-bit float WAV (-> 24-bit PCM), rates > 48 kHz or < 32 kHz,
  VBR MP3 without Xing header, > 2 channels, AIFF-C. https://www.lexicondj.com/manual/sync-to-pioneer-usb  Your exporter
  should run the same header checks (and refuse/transcode) before copying.
- "USB stick not recognised": exFAT on a non-exFAT player; GPT partition map (unofficial); multiple partitions (only the
  first is used, official). See section 1.
- Works on one player, not another: FLAC/ALAC only on NXS2-class and later (section 8); `.2EX` only used by CDJ-3000;
  OneLibrary-only players show nothing from `export.pdb` (section 0); VirtualDJ's export reportedly produced E-8305 on an
  XDJ-AZ for tracks that played on an XDJ-RX3. https://virtualdj.com/forums/269331/VirtualDJ_Technical_Support/CDJ_export_issue.html
- "Library inconsistent" dialogs: the exact string "The library on this device is inconsistent" was **not found** in any
  official source (UNVERIFIED). What rekordbox 7.2.11+ does show is a caution on first launch, and a second caution when
  "OneLibrary and Device Library on your USB drive are out of matching"; the fix is Step 1 compare playlist/track counts,
  Step 2 right-click OneLibrary > "Convert from Device Library" (overwrites OneLibrary).
  https://cdn.rekordbox.com/files/20260317132052/OneLibrary-Compatible-USB-Device-Export-.pdf  So a stick that has
  `export.pdb` but no `exportLibrary.db` (or a mismatched one) will trigger rekordbox's mismatch flow, not a hard error.
- Can rekordbox sync a stick written by another app? Lexicon's manual does not claim rekordbox round-trips its sticks; it
  says "Use at your own risk" and "Always carry a USB drive made by Rekordbox as your backup". It does recommend letting
  rekordbox initialise the stick first (settings files) and warns rekordbox's Sync Manager "can misbehave when syncing to
  USB". https://www.lexicondj.com/manual/sync-to-pioneer-usb ; https://www.lexicondj.com/manual/all  (UNVERIFIED beyond that).
- rekordcrate maintainers: acceptance by "Rekordbox and CDJ ... can't be guaranteed", risk of data loss when writing.
  https://github.com/Holzhaus/rekordcrate/issues/108
- Marker files: `PIONEER/DEVSETTING.DAT`, `MYSETTING*.DAT`, `DJMMYSETTING.DAT` are settings, not integrity markers
  (pyrekordbox). No evidence found of a device-ID/".rekordbox" marker that rekordbox checks (UNVERIFIED).

## 7. Performance and limits

- Per-folder limits 10,000 files / 10,000 folders, 8 folder levels (official; section 1.2). Keep `Contents/` shallow and
  shard large libraries across sub-folders.
- Playlist limit: forum consensus is 1,000 tracks per exported playlist (rekordbox-side), e.g.
  https://forum.djtechtools.com/t/newbie-question-exporting-rekordbox-collection-so-that-pioneer-cdj2000-sees-it/79068 and
  https://virtualdj.com/forums/206246/General_Discussion/Rekordbox_Limitations..html ; "10,000 tracks" collection cap
  reported for old rekordbox 3 and contradicted by users at 10,066+ (UNVERIFIED, likely obsolete). No official
  CDJ-2000 per-stick track limit found.
- Mount time scales with folder/file count and non-music files (official manual quote in section 5).
- Stick advice: FAT32, 64 GB or under for universal compatibility; Windows GUI cannot format > 32 GB FAT32 (section 1.2).
- DB size: one 4096-byte page per table minimum (20 tables + header => ~86 KB floor); PWV5/PWV7 detail waveforms are the
  bulk of per-track storage (150 entries/s x 2-3 bytes).

## 8. Audio format support per player

- Official per-format table (XDJ-RR manual; representative of 2000NXS-era decoders): MP3 (MPEG-1 L3, 16-bit, 32-320 kbps,
  32/44.1/48 kHz, CBR/VBR — VBR seeks slower, CBR recommended); AAC-LC in .m4a/.aac/.mp4 (16-bit, 16-320 kbps,
  32/44.1/48 kHz; DRM .m4p not playable); WAV and AIFF 16/24-bit at 44.1/48 kHz uncompressed PCM.
  https://cdn.teufelaudio.com/products/PIONEER/XDJ-RR/XDJ-RR_manual_EN
- CDJ-2000NXS: "plays your MP3, AAC, WAV and AIFF files" (no FLAC/ALAC). https://www.pioneerdj.com/en/product/player/cdj-2000nxs/black/specifications/
- CDJ-2000NXS2: FLAC + ALAC added with 96 kHz/24-bit card. https://www.pioneerdj.com/en/product/player/cdj-2000nxs2/black/specifications/
  Retailer sheet: WAV/AIFF/FLAC/ALAC at 44.1/48/88.2/96 kHz, 16/24-bit; MP3 32/44.1/48 kHz.
  https://www.musicstore.de/en_DE/EUR/Pioneer-DJ-CDJ-2000NXS2-W/art-DJE0006617-000 (UNVERIFIED official).
- XDJ-1000MK2: FLAC/ALAC "up to 48kHz/24-bit". https://www.pioneerdj.com/en/product/player/xdj-1000mk2/black/specifications/
- XDJ-XZ gained FLAC via firmware. https://www.pioneerdj.com/en/product/all-in-one-system/xdj-xz/black/specifications/
- CDJ-3000: all of the above; internal processing 96 kHz/32-bit float. https://www.pioneerdj.com/en/product/player/cdj-3000/black/specifications/
- Lexicon: "FLAC works only on CDJ-3000, CDJ-TOUR1, and CDJ-2000NXS2" (their compatibility warning list); FLAC/ALAC are
  never converted. https://www.lexicondj.com/manual/sync-to-pioneer-usb
- Metadata: title/artist/BPM/key/cues come from the exported DB, not file tags (rekordbox keeps analysis in its DB; tags
  are only written back manually). https://www.digitaldjtips.com/dj-software-secrets/  WAV/AIFF/FLAC therefore display
  whatever you put in the track row; "Rekordbox does not support album art on WAV files". https://www.lexicondj.com/manual/all
- Per-track `file_type` codes in the track row must match (MP3 1, M4A 4, FLAC 5, WAV 0x0b, AIFF 0x0c); set `bitrate`,
  `sample_rate`, `sample_depth`, `file_size`, `duration` from the actual file.

## 9. Practical checklist for the writer

1. FAT32, single MBR partition, `PIONEER/` at root; `Contents/` <= 8 levels, < 10,000 entries per folder, path < 255.
2. Transcode/reject: WAV_EXTENSIBLE, float WAV, > 48 kHz on non-NXS2, > 2 ch, AIFF-C, VBR MP3 without Xing, > 4 GB files.
3. `export.pdb`: 4096-byte pages, header byte 0x10 = 5, all 20 table directory entries, index page magic values, packed
   24-bit row counts, correct row subtypes, UTF-16LE long strings, date strings, 21 track string slots.
4. ANLZ: `.DAT` with PPTH first (UTF-16BE), PQTZ (BPM*100, ms), PWAV, PWV2, PCOB x2; `.EXT` with PWV3/4/5 + PCO2 x2;
   optional `.2EX` PWV6/7 for CDJ-3000. Folder = hash of audio path (section 2) and `analyze_path` pointing at it.
5. Expect rekordbox 7.2.11+ to raise its OneLibrary/Device Library mismatch caution on your stick; document it.
6. Test matrix: a Device-Library player (NXS2 or CDJ-3000) and, if you add OneLibrary later, an Opus Quad/XDJ-AZ.

## Sources

- https://alphatheta.com/en/information/important-notice-for-customers-using-usb-devices-with-our-dj-equipment/
- https://rekordbox.com/en/support/faq/trouble-shooting-7/
- https://rekordbox.com/en/support/faq/v7/
- https://rekordbox.com/support/faq/onelibrary-7/
- https://rekordbox.com/en/support/faq/devicelibraryplus-6/
- https://rekordbox.com/en/2022/03/rekordbox-v662-release-information/
- https://rekordbox.com/en/2026/03/important-notice-regarding-rekordbox-v7212/
- https://cdn.rekordbox.com/files/20260317132052/OneLibrary-Compatible-USB-Device-Export-.pdf
- https://cdn.rekordbox.com/files/20260807093645/rekordbox7.2.18_manual_EN.pdf
- https://www.pioneerdj.com/en-gb/news/2026/cdj-3000-firmware-ver330-important-notice
- https://cdn.teufelaudio.com/products/PIONEER/XDJ-RR/XDJ-RR_manual_EN
- https://www.pioneerdj.com/en/product/player/cdj-3000/black/specifications/
- https://www.pioneerdj.com/en/product/player/cdj-2000nxs2/black/specifications/
- https://www.pioneerdj.com/en/product/player/cdj-2000nxs/black/specifications/
- https://www.pioneerdj.com/en/product/player/xdj-1000mk2/black/specifications/
- https://www.pioneerdj.com/en/product/all-in-one-system/xdj-xz/black/specifications/
- https://www.pioneerdj.com/it/news/2017/cdj-2000nxs2-w-and-djm-900nxs2-w-limited-edition-white-set-up-with-distinctive-plaques/
- https://djl-analysis.deepsymmetry.org/rekordbox-export-analysis/exports.html
- https://djl-analysis.deepsymmetry.org/rekordbox-export-analysis/anlz.html
- https://raw.githubusercontent.com/Deep-Symmetry/crate-digger/main/src/main/kaitai/rekordbox_pdb.ksy
- https://raw.githubusercontent.com/Deep-Symmetry/crate-digger/main/src/main/kaitai/rekordbox_anlz.ksy
- https://github.com/Deep-Symmetry/crate-digger/issues/46
- https://github.com/Deep-Symmetry/crate-digger/issues/45
- https://github.com/Deep-Symmetry/crate-digger/issues/14
- https://github.com/Holzhaus/rekordcrate
- https://github.com/Holzhaus/rekordcrate/issues/108
- https://github.com/Holzhaus/rekordcrate/issues/173
- https://github.com/Holzhaus/rekordcrate/pull/220
- https://github.com/henrybetts/Rekordbox-Decoding
- https://github.com/kimtore/rex
- https://raw.githubusercontent.com/ModeAxe/rekordbox_converter/main/core/src/anlz.rs
- https://github.com/ModeAxe/rekordbox_converter
- https://github.com/mixxxdj/mixxx/pull/2119
- https://pyrekordbox.readthedocs.io/en/stable/tutorial/anlz.html
- https://pyrekordbox.readthedocs.io/en/latest/formats/mysetting.html
- https://nowplayingapp.com/help/reference/integrations/onelibrary/
- https://www.lexicondj.com/manual/sync-to-pioneer-usb
- https://www.lexicondj.com/manual/all
- https://djban.com.br/en/rekordbox-usb-tester/
- https://forum.audacityteam.org/t/change-format/64479
- https://virtualdj.com/forums/269331/VirtualDJ_Technical_Support/CDJ_export_issue.html
- https://forum.djtechtools.com/t/rekordbox-question/41995
- https://forum.djtechtools.com/t/change-the-file-structure-on-my-rekordbox-stick/72172
- https://forum.djtechtools.com/t/newbie-question-exporting-rekordbox-collection-so-that-pioneer-cdj2000-sees-it/79068
- https://virtualdj.com/forums/206246/General_Discussion/Rekordbox_Limitations..html
- https://community.pioneerdj.com/hc/en-us/community/posts/22980199519513-Rekordbox-doesn-t-export-some-tracks-to-USB
- https://community.enginedj.com/t/usb-sticks-are-not-recognized-on-prime-2/34022
- https://community.enginedj.com/t/accents-in-song-titles-causes-the-red-tracks-issue/43139
- https://support.pirate.com/hc/en-gb/articles/4420241649553
- https://learn.microsoft.com/en-us/previous-versions/windows/it-pro/windows-2000-server/cc938432(v=technet.10)
- https://www.storagenewsletter.com/2026/06/19/microsoft-breaks-past-fat32s-32gb-barrier-extending-support-up-to-2tb/
- https://www.digitaldjtips.com/dj-software-secrets/
- https://www.greentoe.com/product/Pioneer_CDJ-2000NXS
- https://www.musicstore.de/en_DE/EUR/Pioneer-DJ-CDJ-2000NXS2-W/art-DJE0006617-000
- https://git.lenord.me/len0rd/rockbox/commit/b897b400af3469e6bbfb70ae83cb504d754428bb
