# Phase 5 — Engine DJ export

Status: not started. Depends on: Phase 2 (export planner, FAT32/exFAT layout,
dry-run, stale cleanup) and Phase 3 (own beatgrid/key/waveform data). Read
`docs/decisions.md` §Engine DJ and `docs/research.md` §5 first.

## Goal

The same planned export (tracks + playlists + grids + cues + loops) can be
written as a Denon/inMusic Engine DJ library (`Engine Library/Database2/m.db`)
so one stick plays on Engine OS players (PRIME 4/4+/G2, SC6000/SC5000, SC
LIVE, PRIME GO, Mixstream) as well as on Pioneer players. Audio files are
shared: Engine paths point at the same `/Contents/...` files written in Phase 2.

## What we know (summary; sources in research.md)

- Engine DJ Desktop 5.1.0 / Engine OS 5.0.4 are current (Sept 2026). Schema
  version is stored in `Information(schemaVersionMajor/Minor/Patch)`; mapping
  (from libdjinterop): 2.21.0 = Engine 3.1–3.4; 2.21.1/2.21.2 = 4.0.x;
  3.0.0 = 4.1–4.2; 3.0.1 = 4.3.x; 3.0.2 = 4.5/5.0.x (latest known). Whether
  5.1 bumped the schema is **UNVERIFIED**.
- Engine OS 4.1+ migrates any inserted drive forward to a 4.0.1+-only format;
  players refuse databases from a *newer* schema ("is from a newer version of
  Engine"). So: target the **oldest schema the user's players accept**, and
  let the player migrate up.
- Tables (v3 schema): `Information`, `Track`, `PerformanceData` (blobs:
  `trackData`, `overviewWaveFormData`, `beatData`, `quickCues`, `loops`),
  `Playlist` (linked list via `nextListId`, `parentListId`), `PlaylistEntity`
  (linked list via `nextEntityId`), `AlbumArt`, plus views/triggers captured
  from a real DB. 3.0.2 adds `Track.albumArtSourceHash`.
- Blob encoding: `trackData`, `beatData`, `quickCues`, `overviewWaveFormData`
  are zlib with a 4-byte big-endian uncompressed-length prefix (Qt
  `qCompress`); `loops` is uncompressed little-endian. Beat grid markers:
  `{sampleOffset f64, beatNumber i64, beatsToNext i32, unknown i32}`; first
  marker conventionally at beat −4 and last at N+1. Cues: 8 slots, label,
  sample offset (−1 unset), ARGB colour; big-endian doubles. Overview
  waveform: N, N, samplesPerPoint, N×(low,mid,high) u8, 3-byte max.
- Key enum 0–23 (0 = C major, 1 = A minor, alternating around the circle of
  fifths). `Track.path` is relative to the `Engine Library` folder (e.g.
  `../Contents/Artist/Album/file.mp3`).
- Players analyse unanalysed tracks on load, so the overview waveform can be
  omitted, but grids/cues/loops must be written or they will be lost.
- Official third-party guidance: never change the schema, never write while
  Engine DJ is open, bind by `(originDatabaseUuid, originId)`.
- Engine OS drives: FAT32 or exFAT with MBR; NTFS unsupported.
- Open-source references: libdjinterop (C++, LGPL-3.0, 0.27.1, 21 schemas,
  write support, used by Mixxx), rb2engine (Python, MIT, 3.0.1/3.0.2, own
  writer, no waveforms), PenBridge (Swift, MIT, own writer with waveforms),
  endjine (Rust, MPL-2.0, experimental). Decision: **own Rust writer**, no FFI.

## Scope

- `reks-export-engine` crate: DDL + triggers per target schema (3.0.2 and
  2.21.0 initially), blob codecs with round-trip tests, writer from the
  export plan, reader for validation.
- Export planner integration: target "Engine DJ (schema X)" with a device
  picker that maps player/OS version → schema.
- Shared-stick layout: `Engine Library/` next to `PIONEER/` and `Contents/`.

## Non-goals

- Importing Engine libraries into reks (maybe later).
- Writing history (`hm.db`). We read it (task 11) and never delete it.
- Streaming DBs, Smartlists, crates (gone in 2.x+; folders become playlist
  hierarchy).
- Album art hashing exactly like Engine (`AlbumArt.hash` algorithm unknown);
  we write art with our own hash and accept that Engine may re-hash.
- Engine OS 1.x schemas (`m.db` + `p.db`).

## Data structures and interfaces

```rust
pub enum EngineSchema { V2_21_0, V3_0_2 }           // extend as verified
pub struct EngineTarget { pub schema: EngineSchema, pub write_overview_waveform: bool }

pub struct EngineWriter<'a> { plan: &'a ExportPlan, target: EngineTarget }
impl EngineWriter<'_> {
    pub fn write(&self, usb_root: &Path, progress: impl Fn(Progress)) -> Result<WriteReport>;
}

pub mod blobs {
    pub struct TrackData { sample_rate: f64, samples: i64, key: i32, loud_low: f64, loud_mid: f64, loud_high: f64 }
    pub struct BeatGridMarker { sample_offset: f64, beat_number: i64, beats_to_next: i32, unknown: i32 }
    pub struct BeatData { sample_rate: f64, samples: f64, is_set: bool, default_grid: Vec<BeatGridMarker>, adjusted_grid: Vec<BeatGridMarker> }
    pub struct QuickCue { label: String, sample_offset: f64 /* -1 unset */, argb: [u8;4] }
    pub struct QuickCues { cues: [QuickCue; 8], adjusted_main_cue: f64, is_main_cue_adjusted: bool, default_main_cue: f64 }
    pub struct Loop { label: String, start: f64, end: f64, start_set: bool, end_set: bool, argb: [u8;4] }
    pub struct OverviewWaveform { samples_per_point: f64, points: Vec<[u8;3]>, max: [u8;3] }
    pub fn qcompress(bytes: &[u8]) -> Vec<u8>; pub fn quncompress(bytes: &[u8]) -> Result<Vec<u8>>;
    // encode/decode for each struct, with byte-exact round-trip tests against libdjinterop fixtures
}

pub fn engine_key(key: &reks_core::Key) -> i32;   // 0..23 mapping, tested against libdjinterop musical_key.hpp
```

Mapping from the export plan
- `Track`: `path` = `../` + plan-relative audio path with `/`; `filename`,
  `length` (s), `bpm` (int), `bpmAnalyzed` (f64 from grid), `key`, `rating`
  (0–100 in steps of 20), metadata strings, `fileType`, `fileBytes`,
  `isAnalyzed=1` when we write beatData, `isBeatGridLocked=1`,
  `originDatabaseUuid` = our library UUID, `originTrackId` = our track id,
  `dateAdded`/`dateCreated` as Engine expects (verify format from reference).
- `PerformanceData`: `trackData` (sample rate, samples, key, loudness 0.0 =
  no rescale), `beatData` (both grids identical from our grid; markers from
  reks beatgrid anchors converted to sample offsets), `quickCues` (hot cues
  A–H → 1–8; colour from reks cue colour or Engine default pad colours),
  `loops` (reks loops; up to 8), `overviewWaveFormData` from Phase 3's
  3-band data downsampled to N points (optional).
- `Playlist`/`PlaylistEntity`: folders and playlists as a tree with linked-list
  ordering; `isPersisted=1`, `isExplicitlyExported=1`; `databaseUuid` = ours.
- `Information`: `uuid` (new per stick), schema version of the target.

## Tasks

1. **Reference capture**: install Engine DJ Desktop (free) and export a 5-track
   playlist to a stick at the current version and, if available, at 3.4
   (schema 2.21.0). Store `m.db` (metadata only) and `.schema` dumps in
   `fixtures/engine/ref-<schema>/`.
2. **DDL module**: embed the captured DDL (tables, indexes, views, triggers)
   per schema; `create_empty(schema)` test with schema diff = empty.
3. **Blob codecs**: implement `qcompress`/`quncompress` (`flate2`), all five
   blob structs; byte-exact round-trip tests against blobs extracted from the
   reference DB and against libdjinterop's documented examples.
4. **Track + PerformanceData writer**: from plan; unit tests comparing to the
   reference rows (ignoring ids/uuids/timestamps).
5. **Playlist writer**: linked-list ordering; tests for empty, nested, and
   reordered cases; verify with the `PlaylistPath`-style view if present.
6. **Album art**: write `AlbumArt` rows and files per reference; UNVERIFIED hash
   algorithm → use sha1-based 28-char URL-safe base64 and test whether Engine
   accepts/rewrites it (hardware task).
7. **Writer orchestration**: build DB in temp, `PRAGMA integrity_check`,
   fsync, copy to `Engine Library/Database2/m.db`; never write while an
   Engine DJ process is running (check process list; warn).
8. **Reader/validator**: open our DB, decode all blobs, cross-check track count
   and file existence; CLI `reks-cli engine verify <stick>`.
9. **Planner + UI**: Engine target with schema picker and "also write Pioneer"
   combined mode; dry-run lists the DB and any stale `Engine Library` files
   (never `hm.db` or other DBs we don't write, task 11).
10. **Hardware test**: checklist below on an Engine OS player (borrow a PRIME
    GO or SC LIVE; also test Engine DJ Desktop reading the stick).
11. **History (H3, D23)**: read `Engine Library/Database2/hm.db` into Phase 2.5
    sessions with `source = 'engine'`; the writer and stale cleanup never
    delete or overwrite `hm.db`; the Phase 2.5 export guard covers it.
    Location, schema and whether players write it on the stick come from a
    reference stick played on an Engine player (UNVERIFIED, R§5). Test: the
    reference `hm.db` imports; an export leaves its hash unchanged.

## Acceptance criteria

- `m.db` written by reks opens in Engine DJ Desktop (current) without a
  migration error, and tracks/playlists/grids/cues/loops match the plan.
- Byte-exact blob round-trips for every blob in the reference DB.
- On a real Engine OS player: stick recognised, playlists in order, tracks load
  with our beatgrid and cues, player-side analysis is not triggered for tracks
  we analysed (`isAnalyzed=1`), or if it is, our grid/cues survive.
- Combined stick works on both a CDJ and an Engine player with one copy of
  each audio file.

## Tests

- Codec round-trips (reference blobs, synthetic edge cases: 0 cues, 8 cues,
  non-ASCII labels, loops without end).
- Key mapping table test (24 keys both directions, Camelot cross-check).
- Schema diff tests per target.
- Linked-list integrity test: every `nextListId`/`nextEntityId` chain is
  acyclic and covers all rows exactly once.
- Path tests: `../Contents/...` relative form, `/` separators, NFC.

## Manual Engine test checklist

1. Fresh exFAT (MBR) stick; export combined Pioneer + Engine.
2. Insert into Engine OS player: library detected, no "newer version" error;
   note whether the player migrates and how long it takes.
3. Browse playlists (order, nesting), search by artist/title.
4. Load 3 tracks: BPM/key display, grid alignment at bar 1 and at 3 minutes,
   hot cues 1–8 with colours, loops, overview waveform present.
5. Insert into Engine DJ Desktop: library reads, no repair prompt.
6. Insert into a CDJ: legacy library still works (shared audio files).
7. Play 2 tracks on the Engine player; on the computer note whether `hm.db`
   exists on the stick and what `reks-cli history scan` reads; re-export and
   confirm `hm.db` is unchanged.
8. Record Engine OS version, player, and results in `docs/hw-test-log.md`.

## Risks

- **Schema churn** (21 versions since 2017; 4.1 moved blobs to a new table).
  Mitigation: schema-tagged DDL fixtures; refuse unknown targets; add a new
  schema only after capturing a reference.
- **Player migrates the stick** into a newer-only format, breaking older
  players the user also owns. Mitigation: document; offer separate sticks.
- **Unknown fields** (`membershipReference`, `albumArt.hash`, loudness units).
  Mitigation: copy reference values; keep notes in decisions.md.
- **Waveform mismatch**: our overview waveform scaling vs Engine's. Mitigation:
  make it optional; players regenerate.
- **Writing while Engine DJ is open** corrupts; we only write to removable
  media, but warn if Engine DJ is running.
