# Phase 2 — Legacy USB export (`export.pdb` + ANLZ)

Status: not started. Depends on: Phase 1 (library, XML writer, playlists,
`rb_track_id`/`rb_location`). Read `docs/decisions.md` D7, D8, D16 and
`docs/research.md` §1, §4 first.

## Goal

From a reks playlist selection, write a USB stick that a CDJ-2000NXS2 and a
CDJ-3000 (and older Device Library players) browse and play with correct
titles, playlists, beatgrids, hot/memory cues, loops, key and waveforms,
using the analysis rekordbox already produced for imported tracks. The
export is planned first (dry run), FAT32-safe, cleans up stale files, and is
validated by reading the stick back with an independent parser.

## Scope

- `reks-export-rb` crate: `ExportPlan` model, planner, FAT32/path
  validation, audio header checks, `RekordboxWriter` trait with the
  `baken-export` implementation, stale-file reconciliation, read-back
  validator (rekordcrate), reports.
- Locating rekordbox's local ANLZ cache for imported tracks.
- Export UI: pick device, pick playlists, options, dry-run view, progress,
  result report, eject.
- Hardware test campaign on the maintainer's players and `docs/hw-test-log.md`.

## Non-goals

- Our own analysis (Phase 3). Tracks without rekordbox analysis are either
  skipped (default) or get baken's generated ANLZ (option, labelled
  approximate) until Phase 3 lands.
- OneLibrary (Phase 4), Engine DJ (Phase 5).
- Writing `exportExt.pdb` My Tags, history, hot-cue banks, phrase (PSSI).
- Formatting sticks; transcoding audio (we refuse unsupported files and
  explain; "CDJ-safe transcode" is a later feature).
- Settings files (`MYSETTING*.DAT`, `DEVSETTING.DAT`): copy if present on the
  stick or in rekordbox's folder, never synthesise.

## Data structures and interfaces

```rust
/// Pure description of what an export will do. Serialisable, shown to the user.
pub struct ExportPlan {
    pub device: DeviceInfo,                  // mount point, fs kind, free bytes, label
    pub target: TargetProfile,               // which player family drives the checks
    pub tracks: Vec<PlannedTrack>,
    pub playlists: Vec<PlannedPlaylist>,     // tree with ordered track refs
    pub copies: Vec<FileOp>,                 // audio to copy (src, dst, bytes, reason)
    pub anlz: Vec<AnlzOp>,                   // per track: CopyFromCache{src}, Generate, Missing
    pub deletes: Vec<FileOp>,                // stale audio/ANLZ/artwork to remove (prune)
    pub warnings: Vec<Warning>,              // e.g. FLAC on NXS target, path > 255, exFAT
    pub errors: Vec<Problem>,                // blockers: missing file, > 4 GiB on FAT32, WAV_EXTENSIBLE
    pub bytes_to_write: u64, pub bytes_free_after: i64,
}
pub struct PlannedTrack { pub track_id: TrackId, pub src: PathBuf, pub dst_rel: String /* /Contents/... */,
                          pub kind: Kind, pub probe: Probe, pub has_grid: bool, pub cue_count: u8,
                          pub anlz_source: AnlzSource }
pub enum TargetProfile { Cdj2000Nxs, Cdj2000Nxs2, Cdj3000, XdjXz, Generic }   // affects fs + codec warnings
pub enum AnlzSource { RekordboxCache(PathBuf), Generated, None }

pub fn plan(lib: &Library, sel: &Selection, dev: &DeviceInfo, opts: &ExportOptions) -> Result<ExportPlan>;
pub fn apply(plan: &ExportPlan, writer: &dyn RekordboxWriter, progress: impl Fn(ExportEvent), cancel: &CancelToken)
    -> Result<ExportReport>;

pub trait RekordboxWriter {
    /// Writes PDB + ANLZ + audio for the plan into `device_root`. Must be atomic per file and leave
    /// any existing export.pdb untouched on failure.
    fn write(&self, plan: &ExportPlan, device_root: &Path, progress: &dyn Fn(ExportEvent)) -> Result<WriterReport>;
}
pub struct BakenWriter { /* builds temp collection.xml, calls baken_export::plan/export */ }

pub fn validate(device_root: &Path, plan: &ExportPlan) -> Result<ValidationReport>;  // rekordcrate read-back

pub mod fsname {   // FAT32-safe naming, shared with Phase 4/5
    pub fn sanitize_component(s: &str, max_len: usize) -> String;   // NFC, strip \ / : * ? " < > | ; control chars, trim dots/spaces
    pub fn contents_path(artist: &str, album: &str, file: &str) -> String; // /Contents/<Artist>/<Album>/<file>, depth ≤ 3
    pub fn dedupe(existing: &mut HashSet<String>, path: String) -> String; // case-insensitive, -1/-2 suffixes
}
pub mod audiocheck {
    pub enum Issue { WavExtensible, FloatWav, SampleRateUnsupported(u32), Channels(u8), AiffC, VbrNoXing, Over4GiB, Codec(Kind) }
    pub fn check(path: &Path, probe: &Probe, target: TargetProfile) -> Vec<Issue>;
}
pub mod rbcache {   // rekordbox's local analysis files for imported tracks
    pub fn default_roots() -> Vec<PathBuf>;                      // macOS: ~/Library/Pioneer/rekordbox/share/PIONEER/USBANLZ, Windows: %APPDATA%\Pioneer\rekordbox\share\PIONEER\USBANLZ (verify)
    pub fn locate(track: &TrackRow, roots: &[PathBuf]) -> Option<AnlzSet>; // delegate to baken's locate, or own index of PPTH → files
}
```

Device layout written (via baken, verified by us)
```
/PIONEER/rekordbox/export.pdb
/PIONEER/USBANLZ/P0xx/0000xxxx/ANLZ0000.DAT [.EXT] [.2EX]
/Contents/<Artist>/<Album>/<file>
/PIONEER/Artwork/...            (optional)
/PIONEER/*SETTING.DAT            (only if sourced)
```

Stale-file policy
- "Stale" = any file under `/Contents`, `/PIONEER/USBANLZ`, `/PIONEER/Artwork`
  not referenced by the new plan. Default: delete (baken `prune`), listed in
  the dry run. Option: keep. Files outside those folders are never touched.
- A stick previously written by rekordbox is detected (`exportLibrary.db`,
  `exportExt.pdb`, `.sync` files present); warn that rekordbox will show its
  "out of matching" caution afterwards and that reks rewrites the Device
  Library completely.

## Tasks

1. **Device discovery**: `sysinfo::Disks` wrapper → `DeviceInfo` (mount point,
   fs kind string normalised to Fat32/ExFat/Hfs/Other, removable flag, free
   space, label); Tauri command + UI picker; macOS `Info.plist`
   `NSRemovableVolumesUsageDescription`; eject command (`diskutil` on macOS,
   IOCTL on Windows, `udisksctl`/`eject` on Linux). Tests with a temp dir
   "fake device".
2. **fsname + path planning**: implement `fsname`, `/Contents` layout, depth
   and length limits (255 full path, 8 levels), case-insensitive dedupe,
   NFC; property tests.
3. **audiocheck**: parse WAV/AIFF headers (format tag, bit depth, float),
   MP3 Xing/Info presence, sample rate/channels from probe; map to `Issue`
   with target-dependent severity (FLAC/ALAC warn on NXS, error on NXS1);
   tests on crafted headers in fixtures.
4. **Planner**: selection → `ExportPlan` with copies, anlz ops, deletes,
   warnings, errors, byte totals; stable `/Contents` paths across re-exports
   (reuse existing path if the same source file is already on the stick:
   compare size + mtime or hash); unit tests on fake device trees.
5. **rbcache locate**: find rekordbox's local ANLZ for an imported track
   (use baken's `anlz_roots` default resolution; add an index that scans
   roots once and maps PPTH → files for our own needs); report `Missing`.
6. **XML bridge for baken**: `rbxml::export(lib, selection)` to a temp file
   with only the planned playlists; map our `Selection` to baken
   `Options.playlists` (`Folder/Name` paths). Test that the temp XML parses
   with our parser and baken's `collection` module.
7. **BakenWriter**: implement `RekordboxWriter` with `baken_export::plan` +
   `export`; translate its `Plan`/`Report`/`Skipped` into our events; handle
   `Error::DeviceWrite`; set `prune`, `no_settings` (unless settings sourced),
   `artwork`, `generate_analysis` (option). Integration test on a temp dir.
8. **Read-back validator**: parse `export.pdb` with rekordcrate; check
   track count = plan, every `file_path` exists, every `analyze_path` exists
   with PPTH matching `file_path`, PQTZ non-empty where `has_grid`, cue
   counts match, playlists tree and order match, all 20 tables present;
   produce `ValidationReport` with per-track findings. Optional byte-diff
   tool against a rekordbox reference export (`reks-cli rb diff`).
9. **Export orchestration**: `apply` = pre-flight (free space, device still
   mounted, no rekordbox running? warn), writer, fsync, validate, report;
   cancellation leaves the stick in its previous DB state (baken keeps the
   old `export.pdb` on failure; verify).
10. **Export UI**: wizard (device → playlists → options → dry-run table with
    warnings/errors/deletes → progress → report with "Eject"). Persist last
    selection per device label.
11. **`reks-cli rb export/validate/diff`** for headless iteration.
12. **Hardware campaign**: run the checklist on CDJ-2000NXS2 and CDJ-3000
    (and any other player available); fix; log in `docs/hw-test-log.md`.

## Acceptance criteria

- Export of a 200-track, 10-playlist selection from the imported library to
  a 32 GB FAT32 stick completes with zero validator findings; dry run lists
  every copy/delete beforehand and the byte totals match within 1%.
- On CDJ-2000NXS2 and CDJ-3000: stick recognised; playlists in order;
  search by title/artist (including non-ASCII) works; 10 sampled tracks show
  correct title/artist/key/BPM, beatgrid aligned at bar 1 and after 3 min,
  hot cues A–H with colours, memory cues, loops; colour waveform (NXS2) and
  3-band waveform (3000) displayed from rekordbox-sourced ANLZ.
- Re-exporting after removing 20 tracks and adding 20 deletes exactly the
  stale files and copies exactly the new ones (idempotent otherwise: second
  identical export copies nothing).
- Unsupported files (WAV_EXTENSIBLE, float WAV, > 4 GiB) are blocked with a
  clear message, never half-exported.
- Writing never happens outside `apply`; a dry run leaves the stick's
  modification times unchanged (test).

## Tests to write

- `fsname`: property tests (output is FAT32-legal, NFC, ≤ limits,
  deterministic); unicode, emoji, trailing dots, reserved names (`CON`).
- `audiocheck`: fixture headers for each `Issue`.
- Planner: fake device trees (empty, previous reks export, rekordbox-written
  stick) → expected copies/deletes/warnings; stable paths across re-plans.
- Writer integration: temp-dir export then rekordcrate parse; every table
  present; `file_type` codes; UTF-16 strings for non-ASCII; row count equals plan.
- Validator: inject faults (delete an ANLZ, corrupt PPTH, drop a track row)
  and assert each is reported.
- Golden: compare our stick's `export.pdb` row contents (not bytes) with a
  rekordbox export of the same 5 tracks in `fixtures/rekordbox/ref-<ver>/`.

## Manual CDJ test checklist

Prepare: 32 GB stick, FAT32, single MBR partition, empty. Note player model
and firmware. Export the "HW test" playlist set (MP3 CBR, MP3 VBR, AAC,
FLAC, WAV 16-bit, AIFF, a non-ASCII artist, a 12-minute track, a track with
dynamic grid, a track with 8 hot cues + 3 memory cues + 1 loop).

1. Insert: recognised within 30 s; no E-83xx; LINK not required.
2. Browse: PLAYLIST tree order/names; ARTIST, ALBUM, GENRE, KEY, BPM menus;
   SEARCH for a non-ASCII artist; TRACK count equals plan.
3. Load each test track: title/artist/album/key/BPM/time shown; artwork
   (if enabled); waveform overview and scrolling waveform; beatgrid markers
   align with kicks at start and at 3:00 (quantise on, hit a cue: lands on
   beat); hot cues A–H names/colours; memory cues reachable with CUE/LOOP
   CALL; loop plays and stays in time; dynamic-grid track stays aligned
   across the tempo change.
4. Play through a full 12-minute track (no stutter; VBR MP3 seek works).
5. FLAC/AIFF on NXS2/3000 play; on an NXS (if available) FLAC is absent from
   the library as planned.
6. Set a hot cue on the player; eject; re-insert in the computer; run
   `reks-cli rb validate`: still valid; rekordbox 7 opens the stick (expect
   the OneLibrary mismatch caution; record wording).
7. Re-export with 20 tracks removed/added; re-insert; removed tracks gone,
   new ones present, previously set player hot cue state noted.
8. Log everything in `docs/hw-test-log.md` with photos of any error screens.

## Risks

- **Player rejects the stick with no diagnostics.** Mitigation: start with
  baken's known-good path (settings from a rekordbox-initialised stick,
  FAT32/MBR), diff against a reference export, change one variable at a time.
- **rekordbox ANLZ cache not found** for imported tracks (different machine,
  rekordbox uninstalled). Mitigation: `Missing` is explicit in the dry run;
  option to generate (approximate) until Phase 3.
- **baken API or behaviour changes** (young crate). Mitigation: pin, adapter
  trait, integration test that runs on every dependency bump.
- **Stale cleanup deletes something the user wanted** (files they copied by
  hand into `/Contents`). Mitigation: dry run lists deletes; "keep stale"
  option; never touch outside the three folders.
- **Non-ASCII and long paths** are where lockups were reported (NXS2 freeze
  on non-ASCII artist names fixed in baken #168). Mitigation: fixtures with
  Cyrillic/Japanese/emoji; hardware checklist step 2.
- **rekordbox mismatch caution** confuses users. Mitigation: explain in the
  report and docs; Phase 4 resolves it.
