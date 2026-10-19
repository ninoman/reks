# Phase 2 — Legacy USB export (`export.pdb` + ANLZ)

Status: in progress (tasks 2–8 done; 1 done except the Windows/Linux stick check; 12, 13 backend done; planner wired to rbcache and history, first `reks rb` commands; 2026-10-19). Depends on: Phase 1 (library, XML writer, playlists,
`rb_track_id`/`rb_location`). Read `docs/decisions.md` D7, D8, D16, D23 and
`docs/research.md` §1, §4, §10 first.

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
- History safeguard (D23): before writing, find the play history players left
  on the stick, back it up and warn; after writing, record an export manifest
  that Phase 2.5 uses to import that history.
- Export UI: pick device, pick playlists, options, dry-run view, progress,
  result report, eject.
- Hardware test campaign on the maintainer's players and `docs/hw-test-log.md`.

## Non-goals

- Our own analysis (Phase 3). Tracks without rekordbox analysis are either
  skipped (default) or get baken's generated ANLZ (option, labelled
  approximate) until Phase 3 lands.
- OneLibrary (Phase 4), Engine DJ (Phase 5).
- Writing `exportExt.pdb` My Tags, history, hot-cue banks, phrase (PSSI).
- Importing play history and playlist dividers (Phase 2.5). This phase only
  makes sure no history is lost.
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
    pub history: StickHistory,               // what players left on the stick (D23)
    pub backups: Vec<FileOp>,                // stick DB files copied to app_data_dir before any write; .backup_plan() feeds run_backups
    pub warnings: Vec<Warning>,              // e.g. FLAC on NXS target, path > 255, exFAT
    pub errors: Vec<Problem>,                // blockers: missing file, > 4 GiB on FAT32, WAV_EXTENSIBLE
    pub bytes_to_write: u64, pub bytes_free_after: i64,
}
pub struct PlannedTrack { pub track_id: TrackId, pub src: PathBuf, pub dst_rel: String /* /Contents/... */,
                          pub kind: Kind, pub probe: Probe, pub has_grid: bool, pub cue_count: u8,
                          pub anlz_source: AnlzSource }
pub enum TargetProfile { Cdj2000Nxs, Cdj2000Nxs2, Cdj3000, XdjXz, Generic }   // affects fs + codec warnings
pub enum AnlzSource { RekordboxCache(PathBuf), Generated, None }
pub struct StickHistory { pub volume_serial: Option<String>,             // FAT/exFAT serial via the OS (R§8 item 14)
                          pub pdb_sessions: u32, pub pdb_entries: u32,  // export.pdb history rows (rekordcrate)
                          pub pdb_unreadable: bool,                     // treated as "has history"
                          pub onelibrary_db: bool }                     // exportLibrary.db present (baken removes it)
pub enum HistoryPolicy { BackUp /* default */, Discard /* only after an explicit confirm */ }   // Phase 2.5 adds Import

pub fn plan(lib: &Library, sel: &Selection, dev: &DeviceInfo, opts: &ExportOptions, anlz: &dyn AnlzLocator)
    -> Result<ExportPlan, PlanError>;   // done: crates/reks-export-rb/src/plan/
// ExportOptions { target, keep_stale, missing_analysis, artwork, skip_unsupported, history: HistoryPolicy,
//                 discard_history_confirmed }
pub struct CacheLocator;   // AnlzLocator over rbcache: CacheLocator::for_selection(lib, sel, RbCache::scan(roots)?)
pub fn apply(plan: &ExportPlan, writer: &dyn RekordboxWriter, progress: impl Fn(ExportEvent), cancel: &CancelToken)
    -> Result<ExportReport>;

pub trait RekordboxWriter {   // done: crates/reks-export-rb/src/writer/
    /// Writes PDB + ANLZ + audio for the plan into `device_root`; the module docs say what
    /// each kind of failure leaves on the stick.
    fn write(&self, plan: &ExportPlan, device_root: &Path, progress: &(dyn Fn(ExportEvent) + Sync),
             cancel: &CancelToken) -> Result<WriterReport, WriterError>;
}
pub struct BakenWriter;   // BakenWriter::prepare(lib, sel, work_dir) writes the temp collection.xml

pub fn validate(device_root: &Path, plan: &ExportPlan) -> Result<ValidationReport>;  // rekordcrate read-back

pub mod fsname {   // FAT32-safe naming, shared with Phase 4/5
    pub fn sanitize_component(s: &str, max_len: usize) -> String;   // NFC, strip \ / : * ? " < > | ; control chars, trim dots/spaces
    pub fn contents_path(artist: &str, album: &str, file: &str) -> String; // /Contents/<Artist>/<Album>/<file>, depth ≤ 3
    pub fn dedupe(existing: &mut HashSet<String>, path: String) -> String; // case-insensitive, -1/-2 suffixes
}
pub mod audiocheck {   // done: crates/reks-export-rb/src/audiocheck.rs
    pub enum Issue { WavExtensible, FloatWav, WavFormat(u16), BitDepth(u8), SampleRateUnsupported(u32),
                     Channels(u8), AiffC, VbrNoXing, Over4GiB, Codec(Kind) }
    pub enum Severity { Warning, Error }                       // Issue::severity(target)
    pub fn check(path: &Path, probe: &Probe, target: TargetProfile) -> Result<Vec<Issue>, Error>;  // Err = unreadable/malformed header
}
pub mod rbcache {   // done: crates/reks-export-rb/src/rbcache.rs
    pub fn default_roots() -> Vec<PathBuf>;                      // baken's: macOS ~/Library/Pioneer/rekordbox/share/PIONEER/USBANLZ (verified), Windows %APPDATA%\... (UNVERIFIED), library drives
    pub struct AnlzQuery { pub path: String, pub total_time_s: u32, pub first_tempo: Option<(f64, f64)> } // From<&baken Track>
    pub struct RbCache;                                          // RbCache::scan(roots) once, then .locate(&AnlzQuery) -> Lookup
    pub enum Lookup { Found(AnlzSet), Missing(Missing) }         // Missing::{NoRoots, NotAnalysed, Mismatch}
    pub struct AnlzSet { pub dat: PathBuf, pub ext: Option<PathBuf>, pub two_ex: Option<PathBuf>, pub beats: u32 }
}
pub mod rbxml {     // in reks-core; done: crates/reks-core/src/rbxml/export.rs
    pub struct Selection { pub playlists: Vec<PlaylistId> }      // playlists or folders (= every playlist under it)
    pub fn export(lib: &Library, sel: &Selection) -> Result<XmlExport>;          // { collection, playlists: Vec<(id, "Folder/Name")> }
    pub fn export_file(lib: &Library, sel: &Selection, path: &Path) -> Result<XmlExport>;
    impl XmlExport { pub fn baken_playlists(&self) -> Vec<String>; }            // Options.playlists
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

History safeguard (D23)
- baken rebuilds `export.pdb` on every run and removes an existing
  `exportLibrary.db` unless OneLibrary is on (R§10). Players write their
  history into both, so a plain re-export erases it.
- The planner reads the stick's history rows (rekordcrate `HistoryPlaylist` /
  `HistoryEntry`, PDB tables 11/12) and notes `exportLibrary.db`. If either
  holds or may hold history, the dry run says so and plans `backups`.
- `apply` copies `PIONEER/rekordbox/*.pdb` and `exportLibrary.db` to
  `app_data_dir/stick-backups/<volume serial>/<UTC timestamp>/`, checks the
  copies by hash, and only then writes. Backups go in the data dir, not the
  cache dir (D19): they can be the only copy. A failed backup aborts before
  any stick write. `Discard` skips the backup only after an explicit confirm.
- After a successful validation, `export_manifest` gets one row per file on
  the stick, built from the read-back, replacing that serial's previous rows.
  Phase 2.5 matches history entries through it, so PDB track IDs don't need to
  stay stable across exports.

```sql
CREATE TABLE export_manifest (              -- latest export to each stick
  volume_serial TEXT NOT NULL,
  stick_path    TEXT NOT NULL,              -- '/Contents/...' as in the PDB track row
  item_kind     TEXT NOT NULL DEFAULT 'track',   -- 'track' | 'divider' (Phase 2.5)
  track_id      INTEGER REFERENCES track(id) ON DELETE SET NULL,
  exported_at   TEXT NOT NULL,
  PRIMARY KEY (volume_serial, stick_path)
);
CREATE INDEX export_manifest_track_idx ON export_manifest(track_id);  -- for ON DELETE SET NULL
```

## Tasks

1. **Device discovery**: `sysinfo::Disks` wrapper → `DeviceInfo` (mount point,
   fs kind string normalised to Fat32/ExFat/Hfs/Other, removable flag, free
   space, label); Tauri command + UI picker; macOS `Info.plist`
   `NSRemovableVolumesUsageDescription`; eject command (`diskutil` on macOS,
   IOCTL on Windows, `udisksctl`/`eject` on Linux). Tests with a temp dir
   "fake device".
   - [x] Backend: `reks_export_rb::device` — `list_devices`/`list_removable`,
     `device_for_path`, `eject` (typed `DeviceError`, refuses non-removable
     volumes), `volume_serial` for task 12 (UNVERIFIED, R§10.1). Parsing is
     pure (`device::parse`, captured `diskutil` plist in `fixtures/device/`);
     `src-tauri/Info.plist` carries `NSRemovableVolumesUsageDescription`.
     Checked live on macOS with a FAT32 disk image (ignored test
     `live_volume_discovery_serial_and_eject`); Windows and Linux paths only
     compile-checked (clippy for both targets).
   - [x] Tauri commands `devices_list` (removable only) and `device_eject`
     (takes a mount point; `device::eject_at` looks the volume up again, so
     the UI can't name a non-removable one). UI: header button "USB devices"
     opens [DevicesDialog.tsx](../../src/components/DevicesDialog.tsx) (file
     system, free space, a note for exFAT / HFS+ / unreadable file systems,
     Eject; refreshes every 2 s while open). `DeviceList` with
     `selected`/`onSelect` is the wizard's device step (task 10). Volumes
     the OS reports as fixed (some USB SSDs on Windows, R§6) are not listed.
   - [ ] Eject on Windows and Linux, and `volume_serial` on all three OSes,
     against a real stick.
2. [x] **fsname + path planning**: implement `fsname`, `/Contents` layout, depth
   and length limits (255 full path, 8 levels), case-insensitive dedupe,
   NFC; property tests. Done in `reks-core::fsname`
   ([fsname.rs](../../crates/reks-core/src/fsname.rs)): stick paths are
   ASCII-transliterated (D24); forbidden characters become `_`; reserved
   names get a `_` (`CON` → `CON_`); artist/album folders ≤ 64 characters and
   `contents_path` leaves 6 characters for a dedupe suffix; `check_path`
   validates any stick path (also non-ASCII ones rekordbox wrote) for the
   planner and validator. Folders that differ only in case are not merged
   (same folder on FAT32); the planner (task 4) seeds `dedupe` with the
   stick's existing paths via `fold_key`.
3. [x] **audiocheck**: parse WAV/AIFF headers (format tag, bit depth, float),
   MP3 Xing/Info/VBRI presence, sample rate/channels from probe; map to `Issue`
   with target-dependent severity (FLAC/ALAC warn on NXS2/XDJ-XZ/Generic,
   error on NXS1; VBR without seek header warns; 88.2/96 kHz on CDJ-3000
   warns, UNVERIFIED; the rest are errors); tests on headers crafted in code
   plus the generated fixtures.
4. [x] **Planner**: selection → `ExportPlan` with copies, anlz ops, deletes,
   warnings, errors, byte totals; stable `/Contents` paths across re-exports
   (reuse existing path if the same source file is already on the stick:
   compare size + mtime or hash); unit tests on fake device trees.
   Done in `reks_export_rb::plan` ([plan/](../../crates/reks-export-rb/src/plan/mod.rs)):
   `plan(lib, sel, dev, opts, anlz)`; `plan_sources` takes pre-loaded
   `Sources` (tests). Reuse = same size and extension, stick copy not older
   than the source (2 s FAT slack; baken doesn't keep source times), and 3 ×
   64 KiB sampled bytes equal; only ASCII paths are reused (D24). New copies
   never overwrite: `dedupe` is seeded with every path on the stick. Stale
   ANLZ is found through `anlz_dir` (path hash, R§4); with `artwork` on,
   `/PIONEER/Artwork` is the writer's and no deletes are listed there.
   Per-track blockers (missing source, audiocheck errors) block the plan
   unless `skip_unsupported`; tracks without analysis follow
   `MissingAnalysis` (default `Skip`). ANLZ comes from an `AnlzLocator`
   (`CacheLocator` for rekordbox's cache, task 5; `NoAnlz` in tests); its
   `why_missing` text goes into the `Skipped` warning. `history`/`backups`:
   task 12. `Warning`/`Problem` implement `Display` (dry-run wording for the
   CLI and the UI). For task 7: baken assigns its own `/Contents` paths (Unicode,
   48-character cut), so the writer must make it use the plan's `dst_rel`,
   or plan and stick disagree.
5. [x] **rbcache locate**: find rekordbox's local ANLZ for an imported track
   (use baken's `anlz_roots` default resolution; add an index that scans
   roots once and maps PPTH → files for our own needs); report `Missing`.
   Done in [rbcache.rs](../../crates/reks-export-rb/src/rbcache.rs): wraps
   baken's `AnlzIndex` (one scan, PPTH file name → `.DAT`s), so the dry run
   predicts exactly what baken copies. baken matches on the file name, then
   requires the first beat (±2 ms) and BPM of the XML's first `TEMPO`, and
   prefers a grid whose length agrees with `TotalTime` (±3 s); a track
   without a grid only takes an analysis without one. `Missing` says why:
   no root, never analysed, or a same-named file that doesn't match. Build
   `AnlzQuery` from the temp XML parsed with `baken_export::collection`, so
   both see the same values: `plan::CacheLocator::for_selection` does this
   (in memory, `rbxml::export` → baken `Library::parse`), with the cache
   scanned once per locator. macOS root checked on the maintainer's machine
   (2026-10-19, `USBANLZ/<3 hex>/<id>/ANLZ0000.{DAT,EXT,2EX}`, PPTH
   `?/<file name>`); Windows and library-drive roots UNVERIFIED.
6. [x] **XML bridge for baken**: `rbxml::export(lib, selection)` to a temp file
   with only the planned playlists; map our `Selection` to baken
   `Options.playlists` (`Folder/Name` paths). Test that the temp XML parses
   with our parser and baken's `collection` module.
   Done in [rbxml/export.rs](../../crates/reks-core/src/rbxml/export.rs),
   tests in [rbxml_export.rs](../../crates/reks-core/tests/rbxml_export.rs).
   The file holds the selected playlists (a folder = all playlists under
   it), their ancestor folders in library order, and each referenced track
   once; `TrackID` is our track id, `KeyType` 0. Fields are the inverse of
   the import (stars × 51, colour name → RGB, `Kind` from `track.kind` with
   ALAC as `ALAC File`, which baken reads to tell ALAC from AAC). baken picks
   a playlist by its first exact path match, so when two exported playlists
   share a path (same-named siblings, or a `/` in a name) `baken_playlists`
   returns an empty list, which baken reads as every playlist in the file:
   the same set.
7. [x] **BakenWriter**: implement `RekordboxWriter` with `baken_export::plan` +
   `export`; translate its `Plan`/`Report`/`Skipped` into our events; handle
   `Error::DeviceWrite`; set `prune`, `no_settings` (unless settings sourced),
   `artwork`, `generate_analysis` (option). Integration test on a temp dir.
   Done in [writer/](../../crates/reks-export-rb/src/writer/mod.rs), test in
   [tests/writer.rs](../../crates/reks-export-rb/tests/writer.rs). Paths: between
   baken's `plan` and `export` the writer rewrites baken's public `Plan` to
   ours: planned tracks only, in plan order, `usb_path` = `dst_rel`, ANLZ
   slot from baken's `AnlzSlots` on that path, ANLZ source = the plan's
   (`generate_analysis` always on, empty ANLZ root, planned `.DAT` handed in),
   so PDB `file_path`, PPTH and USBANLZ all carry our ASCII path; no upstream
   patch needed. A planned track missing from baken's plan or with another
   source is `PlanMismatch` before any write; `ExportWithout` is refused
   (baken writes ANLZ for every track). `prune` = `!keep_stale`, settings via
   `SettingsSource` (default `Skip` = baken `no_settings`), device name = the
   plan's label, `cancel` is passed to baken between tracks. Failure
   behaviour (read from baken 4.6.1 and tested where it can be provoked):
   cancel and stick-wide errors (`DeviceWrite`, `NotEnoughSpace`) leave the
   old `export.pdb` byte for byte; `export.pdb` is written beside the old one
   and renamed; a single failing track does **not** stop the run (new PDB
   without it, prune skipped, `WriterReport::failures`); errors after the PDB
   was replaced are `AfterDatabase`. The test reads the stick back with
   rekordcrate: 20 tables, one row per planned track, `file_path` = `dst_rel`,
   `file_type` codes, UTF-16 for non-ASCII titles/artists, PPTH = `dst_rel`.
8. [x] **Read-back validator**: parse `export.pdb` with rekordcrate; check
   track count = plan, every `file_path` exists, every `analyze_path` exists
   with PPTH matching `file_path`, PQTZ non-empty where `has_grid`, cue
   counts match, playlists tree and order match, all 20 tables present;
   produce `ValidationReport` with per-track findings. Optional byte-diff
   tool against a rekordbox reference export (`reks-cli rb diff`).
   Done in [validate.rs](../../crates/reks-export-rb/src/validate.rs):
   `validate(root, plan) -> Result<ValidationReport, ValidateError>`; every
   problem is a serialisable `Finding` (per-track ones carry `track_id`,
   `track_findings(id)`), `Err` only for I/O that stops the run. Rows match
   plan tracks on `file_path` = `dst_rel`. Beyond the list above it also
   checks that `analyze_path` is in the folder players derive from
   `file_path` (`anlz_dir`), the `.EXT`'s PPTH and `PCO2` count, extra and
   duplicate rows, and sibling order (PDB `sort_order`). Grid and cue
   checks skip tracks planned without analysis (`AnlzSource::None`).
   `report.tracks` is the read-back (stick path → PDB row → planned track);
   `manifest_rows()` feeds `Library::replace_manifest` (task 13). The PDB
   pages are walked by us (bounded, cycle-checked); row fields rekordcrate
   0.3.0 keeps private are read from the row re-serialised by rekordcrate.
   ANLZ files go through our own section reader
   ([reks-anlz `read`](../../crates/reks-anlz/src/read.rs)) because
   rekordcrate 0.3.0 rejects point cues (D8). Tests write a real stick with
   baken (fixture audio, generated analysis), validate it clean, then
   delete an ANLZ or audio file, corrupt PPTH, empty PQTZ, clear a track
   row's presence bit, and change plan order/tree/cues. Byte-diff
   (`reks-cli rb diff`) left for task 11, with the rekordbox reference export.
9. **Export orchestration**: `apply` = pre-flight (free space, device still
   mounted, no rekordbox running? warn), history backup (task 12), writer,
   fsync, validate, export manifest (task 13), report;
   cancellation leaves the stick in its previous DB state (verified in task
   7 for cancel and stick-wide errors; a single failed track still gets a new
   PDB without it, see task 7).
10. **Export UI**: wizard (device → playlists → options → dry-run table with
    warnings/errors/deletes → progress → report with "Eject"). The device's
    selection is its stick profile (task 15).
11. **`reks-cli rb export/validate/diff/history`** for headless iteration
    (`history` lists the stick's history playlists and entry counts).
    The binary is `reks`; commands live in [rb.rs](../../crates/reks-cli/src/rb.rs).
    A `<stick>` that is not a mount point is used as the stick root, without
    a volume serial (fake sticks for testing).
    - [x] `reks rb plan <stick> --library <db> --playlist <id|Folder/Name>...`
      (options as flags, `--anlz-root` overrides rekordbox's folders,
      `--discard-history` is the explicit confirmation, `--json`): prints the
      dry run, exits non-zero when blocked.
    - [x] `reks rb history <stick> [--json]`: sessions (`HISTORY 001`, ...) with
      entry counts via `history::read_sessions`, and `exportLibrary.db` presence.
    - [ ] `rb export` (task 9), `rb validate` (task 8), `rb diff`.
12. **History safeguard**: `StickHistory` in the planner; volume serial from
    the OS (`diskutil` on macOS, `GetVolumeInformationW` on Windows, `blkid`
    UUID on Linux; UNVERIFIED, R§8 item 14); `backups` with hash check at the
    start of `apply`; `HistoryPolicy`; dry-run and UI wording. Tests: fixture
    `export.pdb` with history → counts and planned backups; a failing backup
    leaves the stick untouched; `Discard` needs the explicit flag.
    - [x] Backend: `reks_export_rb::history`
      ([history.rs](../../crates/reks-export-rb/src/history.rs)).
      `read_history(root, serial)` counts table 11/12 rows with rekordcrate
      0.3.0 (own page walk, bounded and cycle-checked; damaged or unreadable
      PDB → `pdb_unreadable`), notes `exportLibrary.db`; names matched
      case-insensitively. `plan_backups(root, &history, policy, discard_confirmed)`
      lists `PIONEER/rekordbox/*.pdb` and `exportLibrary.db` (+ `-wal`/`-shm`/
      `-journal`) only when the stick may hold history; `Discard` without the
      flag is `DiscardNotConfirmed`. `run_backups(&plan, app_data_dir)` copies
      into `stick-backups/<serial|unknown-serial>/<UTC time, ':'→'-'>/` keeping
      the stick layout, fsyncs, checks BLAKE3 of a second stick read and of the
      copy, writes `manifest.json` last; any error removes the new folder and
      returns a typed `HistoryError`. Only reads from the stick. Tests on
      synthetic PDBs built in code (with/without history, garbage, truncated,
      looping page chain) and temp-dir sticks.
    - [x] Planner: `ExportPlan.history` from `read_history` (serial from
      `device::volume_serial` in `plan`, passed in to `plan_sources`);
      `ExportPlan.backups` from `plan_backups` with `ExportOptions.history` /
      `discard_history_confirmed` (stick paths, `OpReason::HistoryBackup`,
      not counted in `bytes_to_write`); `ExportPlan::backup_plan()` rebuilds
      the `BackupPlan`. `Warning::StickHistory` whenever the stick holds or
      may hold history, `Warning::NoVolumeSerial` without a serial,
      `Problem::DiscardNotConfirmed` for an unconfirmed `Discard` on such a
      stick (a stick without history needs no confirmation).
    - [ ] `apply` (task 9) calls `run_backups(&plan.backup_plan(), ..)` first
      and aborts on error.
    - [ ] Real played-stick fixture: `played_stick_fixture_counts` is `#[ignore]`d
      until `fixtures/rekordbox/played-<ver>/` exists (checklist step 7).
    - [x] Dry-run wording (`Display` for `Warning`/`Problem`) and
      `reks rb history` (task 11; `history::read_sessions` lists sessions).
    - [ ] UI wording (task 10).
    - [x] macOS keys backups by `VolumeUUID`, Windows/Linux by the FAT serial
      (R§10.1). Kept by D28: one library sees one kind of ID; a library moved
      to another OS relinks each stick once (stick-profiles §4.2).
13. **Export manifest**: migration adding `export_manifest`; rows from the
    validator's read-back after each successful export. Tests: re-export
    replaces the serial's rows; deleting a library track keeps its row with
    `track_id` NULL; no serial → no rows, export still succeeds.
    - [x] Migration `0002_export_manifest.sql` (table above plus a `track_id`
      index); `Library::replace_manifest(Option<serial>, rows)` (one
      transaction; no serial → `Ok(0)`, nothing stored; bad rows keep the old
      ones), `manifest(serial)`, `manifest_entry(serial, stick_path)` in
      [manifest.rs](../../crates/reks-core/src/manifest.rs), with the three tests.
    - [x] Rows from the validator's read-back: `ValidationReport::manifest_rows()`.
    - [ ] Call `replace_manifest` after a successful validation in `apply` (task 9).
14. **Hardware campaign**: run the checklist on CDJ-2000NXS2 and CDJ-3000
    (and any other player available); fix; log in `docs/hw-test-log.md`.
    Tasks 12–13 come first: the checklist re-exports a played stick.
15. **Stick profiles** (D27, [`../stick-profiles.md`](../stick-profiles.md) §4.1–4.2,
    §5): migration for `device`, `device_selection`, `device_synced_playlist`;
    `devices::identify` (serial, label+size prompt, no-serial confirm),
    `profile_status`, `set_selection`; Devices panel with status, "frees X MB"
    and capacity bar; Sync records `device_synced_playlist`. Tests per
    stick-profiles §8 (status property test, identity, profile → `Selection`
    equals hand-built plan). Depends on tasks 9, 12, 13.

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
- A stick holding player history is never written without a backup whose
  hashes match the stick's files, unless the user confirmed `Discard`.

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
- History safeguard: an `export.pdb` from a stick played on a CDJ (sanitised,
  metadata only) in `fixtures/rekordbox/played-<ver>/`; backup, abort and
  manifest cases from tasks 12–13.

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
7. Play 3 tracks for over a minute each; eject; on the computer run
   `reks-cli rb validate` (still valid: the player wrote into our PDB without
   breaking it) and `reks-cli rb history` (the session is listed). Keep a copy
   of this `export.pdb` as the played-stick fixture.
8. Re-export with 20 tracks removed/added: the dry run reports the history and
   plans a backup; the backup folder then holds the old PDB with matching
   hashes. Re-insert; removed tracks gone, new ones present, previously set
   player hot cue state noted.
9. Log everything in `docs/hw-test-log.md` with photos of any error screens.

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
- **Player history lost on re-export** (baken rebuilds `export.pdb`, removes
  `exportLibrary.db`). Mitigation: history safeguard (task 12) before the
  hardware campaign; Phase 2.5 imports the backups.
