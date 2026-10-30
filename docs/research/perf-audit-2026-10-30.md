# Performance audit: library import and USB export (2026-10-30)

Scope: rekordbox XML import, folder scan, export plan / dry run / apply /
validate / check a stick, plus a lighter pass over analysis throughput, track
queries, IPC payloads and the track list. Analysis only: no product code was
changed. The generator and benchmark harness lived in a scratch folder and
are not in the repo; [§10](#10-reproducing) describes them well enough to
rebuild.

## 1. Verdict

- **No P0.** Up to 30k tracks, nothing corrupted or half-wrote a stick, hung,
  crashed or ran out of memory. Peak RSS was 435 MB, for a 30k export plan.
- **One P1 cluster, in `apply::sync_written`.** The flush after writing takes
  44–67 % of every export run except the decode-bound `generate` run with
  5-minute tracks (19 %). At 3k it is 95 s of a 159 s export, and 84 s of an
  unchanged 126 s re-sync. Its write-open also leaves a `._*` AppleDouble
  file next to every ANLZ file and next to `export.pdb` on macOS.
- **One P2: select-all is quadratic.** It fetches the whole track table with
  OFFSET paging: 57 ms at 3k, 11.8 s at 30k.
- **Import is fine at 3k.** It is linear, about 12 s for 3k full-size tracks
  (extrapolated from 1k 5-minute tracks), and 0.17 s when nothing changed.

Priority scale used, as given for this audit:

- **P0:** stage risk. A corrupt or half-written stick, a hang, a crash, an
  OOM, or a job that fails at 30k tracks or fewer.
- **P1:** clearly slow for the maintainer's ~3k library: over 30 s, or a UI
  freeze over 200 ms, with measured evidence.
- **P2:** fine at 3k, but grows faster than linearly towards 10k–30k.
- **P3:** minor, low gain, or cost not measured.

## 2. Method

### Machine and build

- Apple M4 Pro (14 cores), 48 GB RAM, macOS 26, internal NVMe SSD.
- `cargo build --release` with `lto = "thin"`, `codegen-units = 16` and
  `debug = "line-tables-only"`, so the profiles have symbols. The shipped
  profile is fat LTO with `codegen-units = 1`. The measured paths are
  I/O-bound, so the difference does not matter here.
- Toolchain from `rust-toolchain.toml` (1.99.1), with the workspace
  `Cargo.lock`.

### Synthetic libraries (generated audio only)

A Python generator wrote N audio files and a matching `collection.xml`:

| Property | Value |
|---|---|
| Formats | MP3 85 % (LAME CBR 320 kbps with Info header, ID3v2.4 tags), FLAC 10 % (Vorbis comments), WAV 5 % (no tags) |
| Audio | ffmpeg sine + beeps (MP3, WAV) or noise (FLAC). 30 s per track for the scaling sets (≈1.2 MB per MP3), 5 min for one calibration set (≈12 MB per MP3) |
| Tags | title, artist, album, genre, BPM, key, year, track number, comment |
| Artwork | 60 % of files embed a 500×500 JPEG (≈70 KB); 300 distinct images, one per album |
| Names | 5 % of artists and 3 % of titles are non-ASCII (Cyrillic, Han, accents) |
| Missing | 2 % of XML tracks point at files that do not exist |
| Grids | 80 % one `TEMPO`; 15 % dynamic grids with 20–400 `TEMPO` rows; 5 % none |
| Cues | 0–6 hot cues in random slots A–H, 0–3 memory cues, 15 % of those loops |
| My Tags | 20 % of comments end with a `/* A / B */` block |
| Playlists | `max(200, N/15)` playlists in 10 top folders, each with 0–3 sub-folders. Sizes are log-normal, with about 3 entries per track; popular tracks appear in many lists |

| Set | Tracks | Playlists | Entries | Audio |
|---|---|---|---|---|
| 1k | 1,000 | 200 | 3,042 | 1.35 GB |
| 3k | 3,000 | 200 | 8,927 | 4.06 GB |
| 10k | 10,000 | 666 | 29,734 | 13.5 GB |
| 30k | 30,000 | 2,000 | 89,154 | 40.5 GB |
| 1k, 5-min | 1,000 | 200 | 3,042 | 12.5 GB |

### Sticks

- **What.** Every export went to a mounted FAT32 sparse disk image, made
  with `hdiutil create -fs "MS-DOS FAT32" -type SPARSE` and attached without
  `-nobrowse` so that `device::list_devices` reports it.
- **What the planner saw.** `fs = msdos` and `StickCheck::Mounted`.
- **Who wrote.** macOS's own FAT driver (FSKit on macOS 26) did every write,
  as it would for a real stick.
- **Why not a folder.** A plain APFS folder doesn't work: the planner
  blocks it with `UnsupportedFilesystem`.

### Harness

A small Rust binary linked against `reks-core`, `reks-export-rb`,
`reks-audio` and `reks-analysis`. It called the same public functions as the
CLI and the Tauri commands, with timers between the calls:

- **import:** `Library::open` → `rbxml::parse_file` → `rbxml::preview` →
  `rbxml::import`. The split between the files stage and the DB stage comes
  from the last `ImportEvent::Progress { stage: Files }`.
- **scan:** `reks_core::scan` twice: once on an empty library, once as an
  unchanged rescan.
- **export:** the CLI's `build_plan` sequence (`RbCache::scan`,
  `CacheLocator::for_selection`, `plan::load`, `plan::plan_sources`,
  `plan::guard_history`), then `DryRun::build`, `plan_track_labels`,
  `BakenWriter::prepare` and `apply`.
  - Each apply stage is timed from `ApplyEvent::Stage`.
  - Options: `skip_unsupported`, target generic, every top-level folder
    selected.
- **query:** `Library::tracks` with various sorts, filters and searches (20
  warm runs each), plus `playlists()`, `facets()`, `track(id)` and paging
  through the whole table.
- **Measurements:** peak RSS from `/usr/bin/time -l`. CPU profiles from
  macOS `sample <pid>` (1 ms interval), aggregated into inclusive counts per
  function.

### ANLZ sources exercised

1. **`generate`** (`--missing-analysis generate`): baken decodes each file
   and builds the ANLZ set itself.
2. **rekordbox cache**, the maintainer's real path for imported tracks. The
   `generate` run's `USBANLZ` folder was copied to the SSD and passed as
   `--anlz-root`. `RbCache` indexes it, and the plan copies the cached
   `.DAT/.EXT/.2EX` sets exactly as it would copy rekordbox's own.
3. **reks-built.** One 5-minute file was analysed with `analyse_file`. That
   analysis was written into every track's cache entry under the track's
   own content hash (`cache::write`). The plan then picks
   `AnlzSource::Reks`, and `reksanlz::write_all` builds every set.

### Caveats

- Source files were mostly in the page cache, so reads were warm (see §8).
- The images sit on the SSD. Copy throughput and per-call flush costs are
  therefore those of FSKit on a sparse image, not of a USB stick.
- The real rekordbox analysis cache on this machine was only scanned,
  read-only, to time `RbCache::scan`. The maintainer's library database was
  not touched.

## 3. Results: import and scan

### XML import

| | 1k | 3k | 10k | 30k |
|---|---|---|---|---|
| `Library::open` (new DB, migrations) | 5 ms | 5 ms | 4 ms | 5 ms |
| `parse_file` | 18 ms | 56 ms | 182 ms | 541 ms |
| `preview` | 10 ms | 25 ms | 85 ms | 266 ms |
| **Import, fresh** | **2.0 s** | **6.2 s** | **20.7 s** | **61.3 s** |
| └ files stage (match, stat, hash, probe, tags, artwork) | 1.95 s | 5.96 s | 19.6 s | 57.5 s |
| └ DB stage (one transaction) | 57 ms | 263 ms | 1.11 s | 3.82 s |
| **Import again, unchanged** | **52 ms** | **172 ms** | **0.74 s** | **2.6 s** |
| └ DB stage | 41 ms | 140 ms | 634 ms | 2.26 s |
| Peak RSS (fresh / again) | 25 / 22 MB | 41 / 37 MB | 93 / 96 MB | 238 / 249 MB |
| Rows written: grid / cue | 29,949 / 4,448 | 93,798 / 13,442 | 317,321 / 44,766 | 937,109 / 135,306 |
| Playlists + folders / entries | 221 / 3,042 | 225 / 8,927 | 692 / 29,734 | 2,022 / 89,154 |
| Progress events sent | 42 | 125 | 458 | 1,322 |

- **5-minute files (1k).** The import took 4.2 s, of which the files stage
  was 3.8 s (≈3.8 ms/track). With these files the content hash reads the
  full 2 MiB, against 1.2 MB for the short ones. Extrapolated: about 12 s at
  3k and about 2 min at 30k.
- **Files stage.** Linear, about 2 ms/track for the short files.
- **DB stage.** Grows a little faster than linearly: 57, 88, 111 and
  127 µs/track. That is B-tree and FTS growth, not an algorithmic problem.

**Profile, 10k fresh import.** Share of the import thread, over 8 s inside
the files stage:

| Function | Share |
|---|---|
| `fileinfo::read_file` | 99 % |
| └ `fileinfo::content_hash` | 90 % |
| &nbsp;&nbsp;└ `read` syscall (reading 2 × 1 MiB) | 60 % |
| &nbsp;&nbsp;└ `blake3::Hasher::update` | 30 % |
| └ `reks_audio::probe` (Symphonia headers) | 5 % |
| └ lofty reads (`tags`, `embedded_artwork`, `stream_bit_rate`) | ≈4 % |
| └ `Library::store_artwork` (SHA-256, write, INSERT) | 2 % |

**Profile, 30k unchanged re-import.** Share of `rbxml::import`:

| Function | Share |
|---|---|
| `Import::write_track` | 78 % |
| `sqlite3_step`, mostly the wide `UPDATE track` | 78 % |
| `resolve` (URL decode, NFC, `exists`) | 17 % |
| `existing_rows` | 12 % |
| `replace_rows` (compare grid and cue rows) | 11 % |
| `write_grid`'s `query_row`, re-prepared per track | 6 % |

### Folder scan

| | 1k | 3k | 10k |
|---|---|---|---|
| Scan into an empty library | 2.1 s | 8.8 s | 32.6 s |
| Per file | 2.2 ms | 3.0 ms | 3.3 ms |
| Unchanged rescan | 14 ms | 72 ms | 407 ms |
| Events emitted (first pass) | 2,946 | 8,841 | 29,346 |
| Peak RSS | 17 MB | 20 MB | 29 MB |

**Profile, 10k scan:**

| Function | Share |
|---|---|
| `content_hash` | 76 % |
| `read_file_hashed` (probe, tags, artwork) | 13 % |
| `sqlite3_step` (one autocommit per file) | 8 % |
| `open` | 6 % |
| `fsync` (WAL checkpoints) | 0.6 % |

## 4. Results: export

### Plan (dry run)

These runs had no rekordbox cache hits, so every track was also checked with
`ReksCache::has_analysis`.

| | 1k | 3k | 10k | 30k |
|---|---|---|---|---|
| Tracks planned | 919 | 2,747 | 9,110 | 27,275 |
| `plan::load` | 63 ms | 170 ms | 656 ms | 2.52 s |
| `CacheLocator::for_selection` (XML built in memory, parsed by baken) | 51 ms | 190 ms | 691 ms | 2.15 s |
| `plan_sources` | 1.86 s | 5.52 s | 17.9 s | 56.6 s |
| **Plan total** | **2.1 s** | **6.0 s** | **19.4 s** | **61.5 s** |
| `DryRun::build` / JSON sent to the UI | — | 20 ms / 0.97 MB | 78 ms / 3.3 MB | 215 ms / 9.8 MB |
| `plan_track_labels` (export start) | 35 ms | 118 ms | 390 ms | 1.15 s |
| Peak RSS | 34 MB | 68 MB | 151 MB | 435 MB |

- **With rekordbox cache hits (3k):** plan 0.92 s, of which `plan_sources`
  0.37 s. Cache hits skip both re-reads of the audio.
- **With 5-minute files (1k):** plan 3.9 s.
- **Onto a stick that already holds the files (3k):** 4.4–5.0 s, because
  `stick::Matcher` compares samples with the stick's copies.
- **`RbCache::scan` on the maintainer's real rekordbox cache** (3,117
  analysed tracks, 9,361 files): 0.15–0.38 s.

**Profile, 3k plan.** Share of `plan_sources`:

| Function | Share |
|---|---|
| `audiocheck::check` (reads 1 MiB per MP3) | 53 % |
| `ReksCache::has_analysis` → `file_content_hash` (reads 2 MiB per track) | 45 % |

### Apply

Times in seconds per stage. "Sync" is `sync_written`.

| Run | Tracks / bytes | Prep | Write | **Sync** | Validate | Total |
|---|---|---|---|---|---|---|
| 3k, rekordbox cache | 2,647 / 3.6 GB | 0.08 | 61.2 | **95.1** | 1.3 | 158.7 |
| 3k, rekordbox cache, unchanged re-sync | 0 copied, 7,941 ANLZ unchanged | 0.08 | 35.9 | **84.4** | 0.2 | 125.6 |
| 3k, `generate` | 2,747 / 3.7 GB | 0.15 | 135.4 | **116.3** | 2.1 | 265.0 |
| 3k, `generate`, unchanged re-sync | 0 copied, all regenerated | 0.09 | 46.7 | **76.9** | 1.6 | 136.3 |
| 10k, `generate` | 9,110 / 13.5 GB | 0.28 | 333.4 | **357.9** | 4.4 | 717.2 |
| 1k 5-min, `generate` | 919 / 11.5 GB | 0.23 | 168.6 | **40.5** | 0.5 | 214.1 |
| 1k 5-min, reks-built | 919 / 11.5 GB | 6.37 | 29.2 | **34.1** | 0.7 | 73.6 |
| 1k 5-min, reks-built, unchanged re-sync | 0 copied | 5.25 | 4.2 | **26.3** | 0.1 | 40.9 |

"Total" includes planning. Peak RSS: 104 MB at 3k, 268 MB at 10k.

**Profiles:**

- **Sync stage** (3k, unchanged re-sync): `fcntl` (`F_FULLFSYNC`) 60 %,
  `open` 40 %.
- **Write stage of an unchanged re-sync** (3k, rekordbox cache). Share of
  `baken_export::export`:
  - `remove_apple_double` 76 %, of which `unlink` 57 %.
  - `write_track` 18 % (`write_if_changed` reads back each ANLZ file).
  - `prune_tree` 5 %.
- **Write stage of `generate` with 5-minute files:**
  - baken's single `prepare` thread is always busy in
    `anlz::generate::waveform::measure`; MP3 decoding is 59 % of it.
  - The writer thread mostly waits on that thread's channel, and the rayon
    workers sit idle.
  - Cost: 183 ms/track.
- **Writer prep for reks-built sets.** Share of `reksanlz::build`:
  `Waveforms::encode` 43 %, `mp3_audio_frames` (reads the whole file) 23 %,
  the second `file_content_hash` 16 %. Cache file reads and `write_set`
  make up the rest.

### Validate and check a stick

- **`validate`** (the read-back inside apply): 1.3–2.1 s at 3k, 4.4 s at
  10k. Right after writing, its reads are served from the OS cache.
- **`reks rb check`:** 19.3 s on the 3k image the first time, 2.8 s once
  cached; 21.7 s on the 10k image. It probes every audio file and reads
  1 MiB of each MP3 from the stick.

### Flush microbenchmark (same FAT32 image)

500 files of 62 KiB each were written, then flushed one by one:

| Per file | FAT32 image | APFS |
|---|---|---|
| open/close only | 0.04 ms | 0.02 ms |
| `fsync()` | 0.18 ms | 0.02 ms |
| `fcntl(F_FULLFSYNC)` (= Rust `File::sync_all` on Apple) | 4.92 ms | 0.97 ms |
| `F_FULLFSYNC` on an already clean file | 2.98 ms | 0.34 ms |

The proposed alternative was then run on the 7,942 ANLZ and PDB files of the
3k stick: a read-only `open` and `fsync` per file, then one `F_FULLFSYNC`. It
took 0.75 s for the files plus 21 ms for the final flush, and created no new
`._` files.

## 5. Results: queries, analysis, UI

### Track queries

`Library::tracks`, warm, mean of 20 runs:

| Query | 3k | 30k |
|---|---|---|
| All, sort by title, 100 rows | 1.23 ms | 16.2 ms |
| All, sort by artist / BPM / date added, 100 rows | 1.1–1.2 ms | 8.1–9.3 ms |
| All, sort by key, 100 rows | 1.90 ms | 15.4 ms |
| All, sort by title, 1000 rows | 4.87 ms | 15.7 ms |
| Search "deep" / "mid", 100 rows | 1.32 / 0.82 ms | 12.4 / 10.5 ms |
| Genre filter / BPM range | 1.31 / 0.23 ms | 14.7 / 0.41 ms |
| Biggest playlist, in playlist order | 1.20 ms (385 rows) | 3.18 ms (676 rows) |
| `playlists()` / `facets()` | 0.55 / 1.74 ms | 5.9 / 19.9 ms |
| `track(id)` detail | 0.07 ms | 0.14 ms |
| JSON size of a 100-row page | 44 KB | 44 KB |

Paging through the whole table (what select-all triggers, see P2-1):

| | 3k | 10k | 30k |
|---|---|---|---|
| Pages of 200 rows (the UI's page size) | 57 ms | 0.64 s | **11.8 s** |
| The last page alone | 3.6 ms | 16.8 ms | 144 ms |
| Pages of 1000 rows | 20 ms | 157 ms | 1.89 s |
| Total JSON | 1.3 MB | 4.5 MB | 13.4 MB |

### Analysis

- `analyse_file` takes 2.8–3.1 s per 5-minute MP3 on one worker. Loading
  the model takes 12 ms.
- The queue runs cores − 1 workers (`queue.rs:3`).
- So 3k tracks is roughly 11–12 min of background work on this machine,
  assuming the workers scale linearly (not measured).

### Tauri shell and UI (code review, not profiled)

- **Threading.** Every long command runs on `spawn_blocking`, through the
  `blocking` helper in `commands.rs:55`. No long job runs on the main thread.
- **Locking.** The library is an `Arc<Library>` around one
  `Mutex<Connection>`. Import holds that mutex for its whole DB stage
  (`import.rs:299–335`); the scan takes it per statement.
- **Track table.** Virtualised (`TrackTable.tsx:132`, `@tanstack/react-virtual`)
  and loaded lazily in pages of 200 rows.
- **Binary data.** Artwork thumbnails go through a 600-entry LRU
  (`useArtworkUrl.ts`). Waveforms come back as `ArrayBuffer`
  (`tauri::ipc::Response`).
- **Progress events.**
  - Import throttles in the core: one event every 50 tracks while reading
    files, every 250 while writing.
  - The scan forwards every event (`commands.rs:226`), up to three per file.
    The UI calls `set` for each progress event (`src/store/scan.ts:27`).
  - Export sends one event per track. The view it clones carries only
    apply's pre-flight warnings, which are few.
- **Dry run.** It sends every copy and delete row (`dryrun.rs:567`). The UI
  renders all of them inside a collapsed `<details>`, without virtualising
  the list (`ExportWizard.tsx:861`).

## 6. Findings

### P1-1 · The export flush forces a full drive-cache flush per file

**Where.** `crates/reks-export-rb/src/apply/mod.rs:634` (`sync_written`),
with the per-file call at `:672–676` and the per-directory call at `:685`.

**What happens.**

```rust
let synced = fs::OpenOptions::new()
    .write(true)
    .open(f)
    .and_then(|file| file.sync_all());
```

On Apple targets `File::sync_all` is `fcntl(fd, F_FULLFSYNC)`: `fsync`, then
a request that the drive write out its whole cache. Once everything is
written, the function makes that call for each of these:

- every file under `PIONEER/rekordbox`, and under `PIONEER/Artwork` when
  artwork is on;
- the `.DAT`, `.EXT` and `.2EX` of every planned track, including sets
  baken left unchanged, because the list is built from the plan rather than
  from what was written;
- every audio file copied in this run;
- every one of these directories, as a best effort.

At 3k that is about 8k files and about 2.7k directories, each paying for a
flush of the whole device.

**Evidence.**

- The sync stage takes a large share of every run:

  | Run | Sync | Total |
  |---|---|---|
  | 3k, rekordbox cache | 95.1 s | 158.7 s |
  | 3k, unchanged re-sync | 84.4 s | 125.6 s |
  | 3k, `generate` | 116.3 s | 265.0 s |
  | 10k, `generate` | 357.9 s | 717.2 s |
  | 1k 5-min, reks-built | 34.1 s | 73.6 s |
  | 1k 5-min, reks-built, unchanged | 26.3 s | 40.9 s |

- In the stage's profile, `fcntl` is 60 % and `open` 40 %. The `open` cost
  is the write-open; see P1-2.
- In the microbenchmark, `F_FULLFSYNC` costs 4.9 ms per file against
  0.18 ms for a plain `fsync`. A read-only `fsync` of the same 7,942 files
  plus one full flush took 0.77 s.

**Cause.** The intent is right: everything must be on the media before
validation and the manifest. But doing it as one device-wide flush per file
costs about 25 times more than needed on FAT32 here.

**Fix (sketch).**

```rust
// Unix: flush each file's data to the device, then the device cache once.
for f in &files {
    let file = fs::File::open(f)?;            // read-only, see P1-2
    if unsafe { libc::fsync(file.as_raw_fd()) } != 0 { /* collect error */ }
}
#[cfg(target_vendor = "apple")]
full_fsync(&root.join("PIONEER/rekordbox/export.pdb"))?; // fcntl(F_FULLFSYNC), once
// Linux: `syncfs` on any fd of the volume is the one-call equivalent.
// Windows: keep the current write-open + FlushFileBuffers per file.
```

- `libc` is already a workspace dependency.
- Keep the existing error collection.
- The final full flush must run after every per-file `fsync` has
  succeeded, and before `validate` and the manifest.

**Expected gain.**

- On the image, the sync stage drops from about 95 s to about 1–5 s at 3k.
- An export would go from 159 s to about 65 s, and an unchanged re-sync from
  126 s to about 15 s once P1-2 is fixed too.
- On real sticks the ratio should hold, because a device cache flush is the
  slowest thing a USB stick does. Real-stick numbers are UNVERIFIED.

**Correctness risk.** Low. Apple's `fcntl(2)` documents `F_FULLFSYNC` as
flushing all of the drive's buffered data, not just one file's. One call
after the per-file `fsync`s therefore gives the same guarantee as today. The
change becomes unsafe only if the final flush is dropped, moved before the
loop, or skipped after an error.

**Effort.** S: one function, with a `#[cfg]` branch per OS. The existing
apply tests cover the behaviour. To verify, re-run the 3k export: the sync
stage should take under 5 s.

### P1-2 · The flush's write-open leaves `._*` files on the stick

**Where.** `apply/mod.rs:674` (`.write(true)`).

**What happens.** macOS adds the `com.apple.provenance` extended attribute
when a provenance-tracked process opens a file for writing, even if it
writes nothing. FAT cannot store extended attributes, so the macOS FAT
driver writes them into a `._<name>` AppleDouble file next to the original.

**Evidence.**

- After each export, the 3k stick held 7,597–8,209 `._*` files:
  `._ANLZ0000.DAT/.EXT/.2EX` in every `USBANLZ` folder, plus
  `PIONEER/rekordbox/._export.pdb`. Their timestamps match the sync stage,
  not the write stage. The 1k stick with 5-minute tracks held 2,749.
- Reproduced in isolation on the image:
  1. Creating a file also creates a `._` file.
  2. After `xattr -c`, a read-only open creates nothing.
  3. An open for writing followed by `F_FULLFSYNC` brings the `._` file
     back, carrying `com.apple.provenance`.
- On the next export, baken's `remove_apple_double` walks the stick and
  deletes them all. In the unchanged re-sync that is 76 % of the write
  stage's samples, with `unlink` alone at 57 %. `sync_written` then creates
  them again.

**Cause.** The write-open is only needed for Windows `FlushFileBuffers`, but
the code uses it on every OS.

**Fix.** The same change as P1-1: open read-only on Unix. Ship both in one
PR.

**Expected gain.** About 27 s per re-sync at 3k (the deletes), less work in
the sync stage itself, and no sidecar files left on the stick.

**Risk.** None from the fix. Two things are UNVERIFIED:

- Whether the shipped app is provenance-tracked. It is distributed outside
  the App Store and assessed by Gatekeeper on first launch, so it probably
  is. This session's processes were.
- Whether any player is bothered by `._export.pdb` or `._ANLZ*` files.

**Effort.** Included in P1-1.

### P2-1 · Select-all loads the whole table in quadratic time

**Where.**

- `src/store/selectedTracks.ts:39–42`:
  `ensureRange(Math.min(...selected), Math.max(...selected))`.
- `src/store/library.ts:231–240` fires `fetchPage` for every page not yet
  loaded.
- `crates/reks-core/src/query.rs:263`: `ORDER BY {order} LIMIT ? OFFSET ?`.

**What happens.**

1. With two or more rows selected, `TrackDetailPanel` mounts
   `BulkEditPanel` (`TrackDetailPanel.tsx:68`), which uses
   `useSelectedTracks`.
2. After Cmd-A, `ensureRange` spans the whole list, so every 200-row page is
   requested at once.
3. Each page is a separate `library_query`. Each one sorts the full table
   (no index serves the sort), skips `offset` rows, and waits its turn on
   the one connection mutex.

**Evidence.**

- Paging through the whole table in 200-row pages takes 57 ms at 3k,
  0.64 s at 10k and 11.8 s at 30k.
- At 30k the last page alone takes 144 ms.
- The `useMemo` at `selectedTracks.ts:25` also walks the whole selection
  every time a page arrives: about 150 × 30k `itemAt` calls at 30k. Not
  measured.

**Fix.** Either of:

- A command that returns, in one query, the rows bulk edit needs for the
  selection.
- A cache in Rust of the sorted id list for each query generation, with
  pages sliced from it. This also makes scrolling deep into the list cost
  one page.

**Expected gain.** 11.8 s becomes one sorted pass, roughly 0.2 s at 30k
(estimate).

**Risk.** None for sticks. With the id-list cache, the cache must be cleared
on every library change.

**Effort.** M.

### P3 findings

#### P3-1 · Import and scan read files one at a time, and the hash dominates

- **Where:** `rbxml/import.rs:463` (`plan_tracks`) and `:600`
  (`fileinfo::read_file`); `fileinfo.rs:154` (`content_hash`, which reads
  1 MiB from each end of the file); `scan.rs:410`.
- **Evidence:**
  - `content_hash` is 90 % of the import samples and 76 % of the scan
    samples.
  - With 5-minute files the cost is 3.8 ms per track: about 12 s at 3k and
    about 2 min at 30k. It grows linearly.
- **Fix:** run stat, hash, probe, tags and artwork on a bounded pool of 4–8
  threads, collecting results in input order.
  - Keep the DB stage serial and in one transaction.
  - `store_artwork` already takes the mutex only briefly, so it can run from
    the pool.
- **Gain:** likely 4–6× on an SSD, not measured. An external HDD may gain
  nothing, because parallel reads can thrash its seeks (UNVERIFIED).
- **Risk:** low. Matching must still follow XML order, because the
  bookkeeping of claimed rows depends on it. Read in parallel, match
  serially.
- **Effort:** M.

#### P3-2 · Every export re-reads every source file before writing

This is the largest cost left at 3k once P1 is fixed.

- **Where:**
  - `audiocheck.rs:438–441`: `mp3_vbr_without_header` reads 1 MiB first, and
    only then looks for the Xing, Info or VBRI header in the first frame.
  - `reksanlz.rs:64`: `has_analysis` re-hashes the file (2 MiB), although
    `track.content_hash` is stored.
  - `reksanlz.rs:177`: `build` hashes the file a second time.
  - `reksanlz.rs:211`, through `reks-audio/src/mp3.rs:26`:
    `mp3_audio_frames` reads the whole file into memory to count frames for
    `PVBR`.
  - `check/tracks.rs:84`: check a stick runs the same audiocheck on the
    stick's copy.
- **Evidence:**
  - Planning without cache hits takes 6.0 s at 3k and 61.5 s at 30k.
    Audiocheck is 53 % of `plan_sources`, `has_analysis` 45 %.
  - Writer prep for reks-built sets takes 6.4 s per 1k 5-minute tracks,
    about 19 s at 3k (extrapolated). It runs serially, before the first byte
    is written.
  - Check a stick takes 19.3 s for 3k the first time.
- **Fix:**
  - Read the ID3 header and the first frame, and scan the 1 MiB only when
    there is no seek header. The result is the same.
  - Hash each file once per export, and pass the hash from the plan to the
    writer.
  - Optionally, trust the stored hash when size and mtime match the row.
    The scan already accepts this blind spot; say so in the code.
  - Store the MP3 frame count and the encoded waveforms in the analysis
    cache entry.
- **Gain:**
  - Plan: from about 2 ms to under 0.3 ms per track.
  - Check a stick: from about 1 MiB to a few KB read per MP3 track.
  - Writer prep: from about 7 ms to about 3 ms per track.
- **Risk:** low. The audiocheck change keeps the same logic on less input.
  Reusing the stored hash is the only change in behaviour.
- **Effort:** S for audiocheck and the single hash; M for caching frame
  counts and waveforms.

#### P3-3 · The import DB stage holds the library mutex and rewrites unchanged rows

- **Where:** `import.rs:299–335` (one transaction); `update_track` (the
  wide `UPDATE` with `coalesce`); `:800` (`write_grid` calls
  `query_row(&format!(…))`).
- **Evidence:**
  - The DB stage takes 263 ms at 3k and 3.8 s at 30k for a fresh import, and
    2.3–2.6 s at 30k when nothing changed. Every UI query waits on the mutex
    meanwhile.
  - `sqlite3_step` is 78 % of the stage. `UPDATE track` assigns `title`,
    `artist` and the other indexed columns every time. That fires the FTS
    trigger `track_fts_au` (`AFTER UPDATE OF title, artist, …`), which
    deletes and re-inserts the FTS row for every track, changed or not. This
    share was not isolated.
  - Re-preparing the statement in `write_grid` accounts for about 6 %.
- **Fix:** compare with the existing row and skip the `UPDATE` when nothing
  changed. Use `prepare_cached` for the user-owned grid check. Keep the
  single transaction: an atomic import is worth more than shorter locks.
- **Gain:** an unchanged re-import at 30k from about 2.3 s to under 1 s
  (estimate).
- **Risk:** low.
- **Effort:** S.

#### P3-4 · Plan load runs a query per playlist and per track

- **Where:**
  - `plan/source.rs:119` calls `lib.tracks` once per playlist. Each call runs
    `PlaylistIndex::load`, which reads every playlist row (`query.rs:238`).
  - `plan/source.rs:161` calls `lib.track(id)` once per track, loading its
    cues and grid too.
- **Evidence:** 63 ms at 1k, 170 ms at 3k, 656 ms at 10k and 2.52 s at 30k.
  The step from 10k to 30k is 3.8×, slightly worse than linear.
- **Fix:** one query for the entries of all selected playlists, one for the
  track fields the plan needs.
- **Gain:** about 2 s at 30k; small at 3k.
- **Risk:** low.
- **Effort:** S.

#### P3-5 · `generate` ANLZ runs on one thread and is redone every export

- **Where:** baken-export 4.6.1, `export::prepare` →
  `anlz::generate::waveform::measure`.
- **Evidence:**
  - 183 ms per 5-minute track on one busy thread, while the writer waits.
  - An unchanged re-sync regenerated all 2,747 sets.
  - baken also walks the whole stick each run (`remove_apple_double`,
    `prune_tree`).
- **Fix:** raise upstream: parallel prepare, and reuse of unchanged sets. Or
  route these tracks through reks-built ANLZ (Phase 3). `generate` is
  opt-in; the default is to skip.
- **Effort:** upstream.

#### P3-6 · The scan sends up to three IPC messages per file

- **Where:** `scan.rs:157` (a `Progress` event per file) and `:287` (a
  `Found` event per file). `commands.rs:226` forwards every event, and
  `src/store/scan.ts:27` calls `set` for each progress event.
- **Evidence:** a 3k scan sends 8,841 events. The UI cost was not measured.
- **Fix:** throttle `Progress` to about 10 Hz in the core, as import does,
  and stop forwarding `Found`.
- **Effort:** S.

#### P3-7 · Dry-run file lists are uncapped and not virtualised

- **Where:** `dryrun.rs:567` (`files: ops`); `ExportWizard.tsx:861`.
- **Evidence:** the JSON payload is 0.97 MB at 3k, 3.3 MB at 10k and 9.8 MB
  at 30k. Every row is rendered, even while the `<details>` is collapsed.
  The UI cost was not measured.
- **Fix:** send counts plus the first N rows with a "show all" request, or
  virtualise the list.
- **Effort:** S.

#### P3-8 · XML preview re-parses the file on every remap edit

- **Where:** `src-tauri/src/commands.rs:257`. The UI calls preview on every
  remap change (`importWizard.ts:107–116`), and import parses the file once
  more.
- **Evidence:** parse plus preview take 80 ms at 3k and 0.8 s at 30k, per
  edit.
- **Fix:** keep the parsed `Collection` in app state between preview and
  import.
- **Effort:** S.

#### P3-9 · Each file is parsed by lofty three times

- **Where:** `fileinfo.rs:64–68`. `reks_audio::tags` and `embedded_artwork`
  each call `lofty::read_from_path`, and `stream_bit_rate` (`probe.rs:189`)
  opens the file with lofty a third time.
- **Evidence:** about 4 % of the import samples.
- **Fix:** read the file once and take the tags, picture and bitrate from the
  same `TaggedFile`.
- **Effort:** S.

## 7. Checked and fine

- **XML:**
  - `quick-xml` streams into one `Collection`; no DOM is built (D11).
  - Parsing grows linearly, from 18 ms at 1k to 541 ms at 30k.
  - The whole collection in memory peaks at 238 MB at 30k.
- **Import writes:**
  - One transaction, with `prepare_cached` for the per-track statements.
  - The matcher uses hash maps, so there are no O(n²) lookups.
  - An unchanged re-import skips file reads by size and mtime: 0.17 s at 3k.
- **SQLite:**
  - WAL with `synchronous=NORMAL` (`db.rs:63–69`).
  - Indexes cover the hot lookups: `track.path` is unique, and `cue`,
    `beatgrid` and `playlist_track` are indexed by track.
  - The scan's per-file autocommit is 8 % of its samples, which is
    acceptable.
- **Unchanged rescan:** 72 ms at 3k, 407 ms at 10k.
- **Queries:**
  - First page: 1–2 ms at 3k, 8–16 ms at 30k.
  - Search: 1 ms at 3k, 12 ms at 30k.
  - Playlist view: 1–3 ms. `facets`: 20 ms at 30k.
  - The deepest single page at 30k takes 144 ms, under the 200 ms freeze
    bar.
- **Threading:** no long work on the main thread. The track table is
  virtualised and paged, thumbnails are cached, and waveforms come back as
  binary.
- **rekordbox cache index:** 0.15–0.38 s for the real 3,117-track cache.
  With cache hits, the 3k plan takes 0.9 s.
- **Writer prep without reks-built sets** (temp XML and baken plan):
  80–280 ms. The PDB write is not a visible cost anywhere.
- **Validation read-back:** 1.3–2.1 s at 3k, 4.4 s at 10k.
- **Export progress:** one event per track, carrying only the few pre-flight
  warnings.
- **OneLibrary store:** one transaction, with statements prepared once per
  table (`store.rs:23`). Validation uses `BTreeSet` lookups.
- **Memory:** under 450 MB in every run; 268 MB for a 10k export.
- **Loops checked and harmless at these sizes:**
  - `validate.rs:493`: a playlist lookup per entry list. O(P²), about 4M
    comparisons at 2k playlists.
  - `diff.rs:670`: matching findings. O(F²).
  - `import.rs:936`: matching playlists by name among siblings.
    O(siblings²).

## 8. Unverified and not measured

- **Real USB sticks and players.** Every stick number comes from a FAT32
  image on the SSD. Real sticks write and flush at different speeds.
  - The P1-1 ratio should hold, but its absolute size is UNVERIFIED.
  - Re-run the 3k export on a real stick before and after the fix, and log
    both runs in `hw-test-log.md`.
- **Cold cache and external drives.** Most source reads hit the page cache;
  dropping it needs `purge`, which needs admin rights. Import, scan and plan
  are all bound by reads, so a library on an external HDD (a seek plus 2 MiB
  per track) could push a 3k import or plan past 30 s.
- **30k full export.** Not run, for lack of disk space and time. Write and
  sync are per-file loops and grew linearly from 3k to 10k.
- **UI render cost.** Not profiled in the WebView: per-file scan events,
  dry-run lists, the select-all memo.
- **Windows and Linux flushing.** Not measured.
  - Windows needs the write-open for `FlushFileBuffers`.
  - On most Linux file systems each `fsync` already flushes the device
    cache, so one `syncfs` would be the equivalent of the P1-1 fix.
- **AppleDouble files from the shipped app.** Whether a release build is
  provenance-tracked (P1-2) is unverified.

## 9. Incidental findings (not performance)

Seen while benchmarking. Recorded for triage, not part of the priority list.

1. **`CueCount` findings on tracks with hot cues D–H.**
   - **What was seen.** Validation reported `CueCount` for 2,177 of 2,747
     tracks (3k `generate`) and for 2,090 of 2,647 (rekordbox-cache path).
     That matches the number of tracks with a hot cue in slots D–H (2,115,
     not counting FLAC).
   - **Examples.** Track 2320 has six hot cues (B, D, E, F, G, H); its
     `.DAT` holds one. Track 1031 has two memory cues and hot cue H; its
     `.DAT` holds two.
   - **Reading.** The counts fit a `.DAT` that holds memory cues and hot
     cues A–C only. That would match older players, which read only A–C from
     the `.DAT` and the extended cues from the `.EXT`. But `validate.rs`
     compares the `.DAT` count with all of the track's cues (`t.cue_count`).
   - **Consequence.** Such exports end as `Problems` and record no manifest.
   - **Limit.** Only baken-generated sets were checked, not rekordbox's own.
     Check against crate-digger before deciding whether the writer or the
     validator is wrong.
2. **Read-back is served from the OS cache.** Validation runs right after
   the flush, so it reads what the file system has in memory, not what
   reached the media. That catches writer bugs. It would not catch a stick
   that silently drops writes, such as a fake-capacity stick. Noted because
   "stick-safe" is the project's first rule.

## 10. Reproducing

1. **Generate the libraries.** Write a script, in any language, that
   produces the data described in §2.
   - Base audio comes from ffmpeg:
     - MP3: `sine=f=…:d=30:beep_factor=4`, `libmp3lame -b:a 320k -write_xing 1`.
     - FLAC: `anoisesrc`.
     - WAV: `pcm_s16le`.
     - Covers: `testsrc2` with noise, saved as JPEG.
   - Each file is a fresh tag block followed by the base audio:
     - MP3: an ID3v2.4 header with
       `TIT2/TPE1/TALB/TCON/TBPM/TKEY/TDRC/TRCK/COMM/APIC`.
     - FLAC: STREAMINFO, then `VORBIS_COMMENT`, `PICTURE` and padding.
   - `collection.xml` uses the attributes that `rbxml/parse.rs` reads.
     `Entries` and `Count` must be exact, because the parser checks them.
2. **Make a stick.**
   `hdiutil create -size 8g -fs "MS-DOS FAT32" -volname RKP3K -type SPARSE img/rkp3k`,
   then `hdiutil attach img/rkp3k.sparseimage`. Don't pass `-nobrowse`, or
   the device list won't show the volume.
3. **Import.** `reks import-xml <db> <collection.xml>` prints the parse and
   import times. Run it twice to measure the unchanged case.
4. **Export.**
   `reks rb export /Volumes/RKP3K --library <db> --playlist "Folder 0" … --missing-analysis generate --skip-unsupported --yes`.
   - For the rekordbox-cache path, copy `/Volumes/<stick>/PIONEER/USBANLZ`
     to the SSD. Pass `--anlz-root <copy>/USBANLZ` (the folder whose
     children are the `Pxxx` directories) to a second export onto a fresh
     image.
   - Per-stage times are easiest to get from a small harness that wraps
     apply's progress callback, as described in §2.
5. **Profile.** Run `sample <pid> 10 -f out.txt` during the stage you want,
   then sum the inclusive counts per function.
6. **Flush microbenchmark.** Write N files, then time `fcntl(fd,
   F_FULLFSYNC)` against `os.fsync` per file. Python exposes
   `fcntl.F_FULLFSYNC`.

## 11. Suggested fix PRs

Ordered by gain for the maintainer's 3k library:

1. **P1-1 and P1-2.** In `sync_written`: open read-only, `fsync` each file,
   then flush the device once; leave Windows as it is. Re-run the 3k
   export: sync should take under 5 s and leave no `._*` files. Log a run on
   a real stick.
2. **P3-2: audiocheck early exit, one hash per export.** P3 on the scale
   because it is linear, but it is the largest remaining cost at 3k, and it
   also speeds up check a stick.
3. **P2-1: bulk-edit rows in one command,** or a cached sorted id list.
4. **P3-1: bounded parallel file reading** for import and scan.
5. **P3-3 and P3-4:** skip unchanged `UPDATE`s, use `prepare_cached`, and
   fix the plan loader's queries.
6. **P3-6 to P3-9** when convenient. **P3-5** goes upstream to baken.
7. **Separately, not performance:** triage the `CueCount` mismatch (§9,
   item 1).
