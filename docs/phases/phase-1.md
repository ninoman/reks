# Phase 1 — Scaffold + library

Status: in progress (tasks 1–3 done 2026-10-19). Depends on: nothing. Read `CLAUDE.md` and `docs/decisions.md` first.

## Goal

A working desktop app that owns a music library: scan folders, read tags and
artwork, import an existing rekordbox `collection.xml` (tracks, beatgrids,
cues, loops, keys, playlists), browse it in a fast table with search and
filters, manage playlists, and preview-play tracks. By the end of this phase
the maintainer's ~3,000-track rekordbox collection is fully represented in the
reks library with nothing lost that Phase 2 needs for export.

## Scope

- Tauri 2 workspace scaffold, CI, licence gate (`cargo deny`), fixtures generator.
- SQLite library schema + migrations (`reks-core`).
- Folder import: recursive scan, tag reading (ID3v2/MP4/Vorbis/RIFF/AIFF),
  embedded artwork extraction, duration/sample-rate/bit-rate probe via Symphonia.
- rekordbox XML import (`collection.xml`): tracks matched to files, metadata,
  `TEMPO` beatgrid, `POSITION_MARK` hot/memory cues and loops, key, colour,
  rating, comments, play count, date added, playlists and folders.
- rekordbox XML export (needed by Phase 2's writer path and as a user feature).
- Track table UI (virtualised), search, filters (genre, key, BPM range, colour,
  rating, playlist, "missing file", "needs analysis"), sorting, multi-select,
  inline edit of core fields.
- Playlists and folders: create, rename, reorder, drag tracks in, remove.
- Preview playback from Rust: play/pause/seek/volume, position events, output
  device selection, with a simple overview waveform drawn from decoded PCM.

## Non-goals (this phase)

- No USB export, no PDB/ANLZ writing (Phase 2).
- No BPM/key/waveform analysis of our own (Phase 3). We only store what the
  XML gives us. Tracks without XML analysis are flagged "needs analysis".
- No reading of rekordbox `master.db` (encrypted; out of scope entirely).
- No tag *writing* to audio files. The library DB is the source of truth.
- No streaming services, no DVS, no mixing features.
- No smart playlists / "My Tags" (schema leaves room; UI later).

## Data structures and interfaces

### SQLite schema (migration 0001)

```sql
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE track (
  id            INTEGER PRIMARY KEY,
  path          TEXT NOT NULL UNIQUE,        -- absolute, NFC, as on disk
  file_size     INTEGER NOT NULL,
  file_mtime    TEXT NOT NULL,               -- ISO-8601 UTC
  content_hash  TEXT,                        -- blake3 of first+last 1 MiB + size; for move detection
  missing       INTEGER NOT NULL DEFAULT 0,  -- file not found at last check
  kind          TEXT NOT NULL,               -- mp3|aac|alac|flac|wav|aiff|ogg
  duration_ms   INTEGER,
  sample_rate   INTEGER,
  bit_depth     INTEGER,
  bit_rate_kbps INTEGER,
  channels      INTEGER,
  title         TEXT, artist TEXT, album TEXT, album_artist TEXT, genre TEXT,
  composer TEXT, remixer TEXT, label TEXT, mix TEXT, grouping TEXT,
  track_no INTEGER, disc_no INTEGER, year INTEGER,
  comment       TEXT,
  rating        INTEGER NOT NULL DEFAULT 0,  -- 0..5
  color         TEXT,                        -- rekordbox colour name or NULL
  key           TEXT,                        -- canonical, e.g. "Am", "F#m", "Db"; see keys.rs
  key_source    TEXT,                        -- tag|rekordbox|analysis|manual
  bpm_x100      INTEGER,                     -- average/display BPM * 100
  bpm_source    TEXT,
  play_count    INTEGER NOT NULL DEFAULT 0,
  date_added    TEXT NOT NULL,
  last_played   TEXT,
  artwork_id    INTEGER REFERENCES artwork(id) ON DELETE SET NULL,
  rb_track_id   INTEGER,                     -- TrackID from the imported XML, if any
  rb_location   TEXT,                        -- original Location URL from XML
  created_at    TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE INDEX track_artist_idx ON track(artist);
CREATE INDEX track_bpm_idx ON track(bpm_x100);
CREATE INDEX track_key_idx ON track(key);

CREATE TABLE artwork (
  id      INTEGER PRIMARY KEY,
  sha256  TEXT NOT NULL UNIQUE,
  mime    TEXT NOT NULL,
  width INTEGER, height INTEGER,
  blob_path TEXT NOT NULL                    -- file under app_cache_dir/artwork/<sha>.jpg
);

-- Beatgrid: one row per tempo anchor (rekordbox TEMPO element). A constant
-- grid has one row. position_ms is the first beat at/after the anchor.
CREATE TABLE beatgrid (
  id        INTEGER PRIMARY KEY,
  track_id  INTEGER NOT NULL REFERENCES track(id) ON DELETE CASCADE,
  position_ms REAL NOT NULL,                 -- float ms (XML Inizio is seconds with 3 decimals)
  bpm_x100  INTEGER NOT NULL,
  meter_num INTEGER NOT NULL DEFAULT 4,
  meter_den INTEGER NOT NULL DEFAULT 4,
  beat_in_bar INTEGER NOT NULL DEFAULT 1,    -- rekordbox Battito 1..4
  source    TEXT NOT NULL                    -- rekordbox|analysis|manual
);
CREATE INDEX beatgrid_track_idx ON beatgrid(track_id, position_ms);

-- Cues: hot cues (slot 0..7 = A..H), memory cues (slot NULL), loops (end_ms set).
CREATE TABLE cue (
  id        INTEGER PRIMARY KEY,
  track_id  INTEGER NOT NULL REFERENCES track(id) ON DELETE CASCADE,
  slot      INTEGER,                         -- NULL = memory cue
  kind      INTEGER NOT NULL,                -- 0 cue,1 fade-in,2 fade-out,3 load,4 loop (rekordbox Type)
  start_ms  REAL NOT NULL,
  end_ms    REAL,                            -- loops only
  name      TEXT,
  color_rgb INTEGER,                         -- 0xRRGGBB or NULL
  source    TEXT NOT NULL
);
CREATE INDEX cue_track_idx ON cue(track_id);

CREATE TABLE playlist (
  id        INTEGER PRIMARY KEY,
  parent_id INTEGER REFERENCES playlist(id) ON DELETE CASCADE, -- NULL = root
  is_folder INTEGER NOT NULL DEFAULT 0,
  name      TEXT NOT NULL,
  sort_order INTEGER NOT NULL,
  created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE TABLE playlist_track (
  playlist_id INTEGER NOT NULL REFERENCES playlist(id) ON DELETE CASCADE,
  track_id    INTEGER NOT NULL REFERENCES track(id) ON DELETE CASCADE,
  position    INTEGER NOT NULL,
  PRIMARY KEY (playlist_id, position)
);
CREATE INDEX playlist_track_track_idx ON playlist_track(track_id);

CREATE TABLE library_root (                 -- watched folders
  id INTEGER PRIMARY KEY, path TEXT NOT NULL UNIQUE, last_scan TEXT
);

CREATE TABLE setting (key TEXT PRIMARY KEY, value TEXT NOT NULL);

CREATE VIRTUAL TABLE track_fts USING fts5(
  title, artist, album, genre, comment, label, remixer, content='track', content_rowid='id'
);
-- plus the three standard FTS5 sync triggers on track.
```

Design notes
- Positions are milliseconds as `REAL` because rekordbox XML gives seconds with
  3 decimals and ANLZ stores ms integers; we keep sub-ms precision from analysis
  (Phase 3) and round only on export.
- `key` is canonical (`keys.rs`: parse Camelot `8A`, Open Key `1m`, classical
  `Am`/`A minor`/`F#m`/`Gb`; expose conversions). One spelling per key, following
  the Camelot wheel labels (`Abm`, `Dbm`, `F#m`, `Db`, `F#`; table in
  [`keys.rs`](../../crates/reks-core/src/keys.rs)). rekordbox `Tonality` arrives
  as whatever the user's rekordbox preference emitted; we normalise. The exact
  strings rekordbox emits are UNVERIFIED until task 7 checks a real export.
- The two PRAGMAs above are per-connection settings, so `Library::open` sets
  them (plus `synchronous=NORMAL`, `busy_timeout`); migration 0001
  ([`0001_init.sql`](../../crates/reks-core/migrations/0001_init.sql)) holds the
  rest. Its FTS update trigger fires only when an indexed column changes.
- Waveform/analysis caches are *not* in the library DB; they go in
  `app_cache_dir/analysis/<track_id>/...` (Phase 3) so the DB stays small and
  portable.

### Core Rust interfaces (`reks-core`)

```rust
pub struct Library { conn: Mutex<rusqlite::Connection>, cache_dir: PathBuf }

impl Library {
    pub fn open(db_path: &Path, cache_dir: &Path) -> Result<Self>;      // runs migrations
    pub fn tracks(&self, q: &TrackQuery) -> Result<Vec<TrackRow>>;      // paged, sorted, filtered
    pub fn track(&self, id: TrackId) -> Result<TrackDetail>;            // with grid, cues, artwork
    pub fn update_track(&self, id: TrackId, patch: TrackPatch) -> Result<()>;
    pub fn playlists(&self) -> Result<PlaylistTree>;
    pub fn playlist_mutate(&self, op: PlaylistOp) -> Result<()>;
}

pub struct TrackQuery { pub text: Option<String>, pub filters: Vec<Filter>,
                        pub sort: Sort, pub offset: u32, pub limit: u32 }
pub enum Filter { Genre(String), KeyIn(Vec<Key>), BpmRange(u32,u32), Color(String),
                  RatingMin(u8), Playlist(PlaylistId), Missing(bool), NeedsAnalysis(bool) }

pub mod scan {   // folder import
    pub fn scan(roots: &[PathBuf], lib: &Library, progress: impl Fn(ScanEvent)) -> Result<ScanReport>;
    // ScanEvent::{Found(path), Added(id), Updated(id), Moved{from,to}, Missing(id), Error{path,msg}}
}

pub mod rbxml {  // rekordbox XML
    pub struct Collection { pub product: Product, pub tracks: Vec<XmlTrack>, pub playlists: XmlNode }
    pub fn parse(reader: impl BufRead) -> Result<Collection>;          // quick-xml, streaming
    pub fn write(c: &Collection, w: impl Write) -> Result<()>;
    pub fn import(c: &Collection, lib: &Library, opts: ImportOptions, progress: impl Fn(ImportEvent))
        -> Result<ImportReport>;
    pub fn export(lib: &Library, selection: Selection) -> Result<Collection>;
    pub fn location_to_path(url: &str) -> Result<PathBuf>;              // file://localhost/... → path, percent-decoded, NFC
}
```

`XmlTrack` carries every attribute in the rekordbox XML spec (TrackID, Name,
Artist, Composer, Album, Grouping, Genre, Kind, Size, TotalTime, DiscNumber,
TrackNumber, Year, AverageBpm, DateModified, DateAdded, BitRate, SampleRate,
Comments, PlayCount, LastPlayed, Rating, Location, Remixer, Tonality, Label,
Mix, Colour) plus `Vec<Tempo{inizio,bpm,metro,battito}>` and
`Vec<PositionMark{name,type,start,end,num,red,green,blue}>`. Unknown attributes
are preserved in a `extra: BTreeMap<String,String>` so round-trips don't lose data.

### Audio (`reks-audio`)

```rust
pub struct Probe { pub kind: Kind, pub duration_ms: u64, pub sample_rate: u32,
                   pub channels: u8, pub bit_depth: Option<u8>, pub bit_rate_kbps: Option<u32> }
pub fn probe(path: &Path) -> Result<Probe>;                 // header only, no full decode
pub fn decode_f32(path: &Path) -> Result<Pcm>;              // interleaved f32 + sample_rate (Phase 3 uses this too)
pub fn tags(path: &Path) -> Result<Tags>;                   // lofty crate
pub fn embedded_artwork(path: &Path) -> Result<Option<Picture>>;

pub struct Player { /* owns the output thread */ }
impl Player {
    pub fn new(device: Option<String>) -> Result<Self>;
    pub fn load(&self, path: &Path) -> Result<()>;           // decodes ahead on a worker
    pub fn play(&self); pub fn pause(&self); pub fn seek_ms(&self, ms: u64); pub fn set_volume(&self, v: f32);
    pub fn position_ms(&self) -> u64;
    pub fn devices() -> Vec<String>;
}
```

### Tauri commands (`src-tauri`)

`library_query`, `library_track`, `library_update_track`, `playlists_get`,
`playlists_mutate`, `scan_start` (progress via `Channel<ScanEvent>`),
`rbxml_import_start` (progress via Channel), `rbxml_export`,
`player_load/play/pause/seek/volume/devices`, `player_state` (polled or event
every 100 ms), `artwork_get(id) -> Response(bytes)`, `overview_waveform(id) ->
Response(bytes)`.

## Tasks (each sized for one coding session)

1. [x] **Scaffold**: `cargo new` workspace with the crate layout from CLAUDE.md;
   `npm create tauri-app` (React-TS-Vite template) wired to `src-tauri`;
   `rust-toolchain.toml`; `deny.toml` with the licence allowlist; GitHub
   Actions: fmt, clippy, test, `cargo deny`, Vite build on macOS/Windows/Linux.
   Done when `cargo tauri dev` opens an empty window and CI is green.
2. [x] **Fixtures generator**: `fixtures/gen` (Rust bin) producing a 30-second
   sine+click WAV at 128 BPM and the same as MP3/FLAC/AIFF/M4A (encode with
   `hound` for WAV; for compressed formats, check in tiny pre-generated files
   produced once with ffmpeg, documented in `fixtures/README.md`); a minimal
   `fixtures/rekordbox/collection-small.xml` referencing them (3 tracks, 1
   folder, 2 playlists, grids, hot cues, a loop, memory cues). See
   [`fixtures/README.md`](../../fixtures/README.md).
3. [x] **Schema + migrations**: migration 0001 above, `Library::open`, FTS triggers,
   `keys.rs` with parsing/normalisation and tests for every notation.
4. [ ] **Audio probe + tags + artwork**: `probe`, `tags` (`lofty`), artwork
   extraction and de-dup into cache, unit tests on the fixtures.
5. [ ] **Folder scan**: walk roots (`walkdir`), extension filter, NFC normalise
   paths, insert/update by path, detect moves by `content_hash`, mark missing,
   progress events, cancellation token. Tests on a temp dir.
6. [ ] **rekordbox XML parser/writer**: streaming parse with `quick-xml` into
   `Collection`; writer that reproduces rekordbox's element order and the
   `file://localhost/` Location form (percent-encoding rules from the XML
   spec); round-trip test on `collection-small.xml` and on a 3,000-track
   sample (generated).
7. [ ] **rekordbox XML import**: map `XmlTrack` → `track`, `TEMPO` → `beatgrid`,
   `POSITION_MARK` → `cue` (Num −1 memory, 0..7 hot, Type 4 loop with End),
   playlists tree, options: match by Location path, fall back to
   (artist,title,size); report unmatched tracks; idempotent re-import updates
   rather than duplicates.
8. [ ] **Track table UI**: virtualised table (`@tanstack/react-virtual` +
   `@tanstack/react-table`), column chooser, sorting, 50 ms debounced search
   calling `library_query`, filters sidebar, multi-select, keyboard navigation.
9. [ ] **Track detail + inline edit**: side panel with editable fields, cue and
   grid listing (read-only), artwork display via `artwork_get`.
10. [ ] **Playlists UI**: tree, create/rename/delete folder/playlist, drag tracks,
    reorder, context menu; backed by `playlists_mutate` ops.
11. [ ] **Player**: `reks-audio::Player` on rodio (Symphonia decoders), output
    device list, seek; Tauri commands; transport bar with position slider and
    a simple overview waveform (min/max per 1,000 samples, computed on first
    load, cached as `.f32` in cache dir, sent as bytes).
12. [ ] **Import wizard**: choose `collection.xml`, show counts, run import with
    progress, show report (matched, unmatched, missing files) and a "map
    folder" option when the XML paths point at a moved music folder (prefix
    replacement).
13. [ ] **Settings**: library roots, cache dir, audio device, rekordbox key
    notation preference; stored in `setting`.
14. [ ] **Polish + release pipeline**: app icon, window state persistence, unsigned
    dev builds via `tauri-action` on tags (signing is Phase 2+ concern; see
    decisions.md).

## Acceptance criteria

- Import of the maintainer's real `collection.xml` (~3,000 tracks) completes in
  under 60 s on a laptop, with 100% of tracks whose files exist matched, and
  the report lists every unmatched track with a reason.
- For 20 randomly chosen tracks, grid anchors, hot cues (slot, position,
  colour, name), memory cues, loops, key, BPM, rating, colour, comment and
  playlist membership in reks equal what rekordbox shows (positions within
  1 ms).
- XML export → re-import into a fresh library is lossless for all fields
  listed above; the exported XML opens in rekordbox 7's "Import Collection"
  without errors (manual check).
- Table scrolls at 60 fps with 10,000 synthetic tracks; search results appear
  within 100 ms.
- Preview playback works for MP3, AAC (M4A), ALAC, FLAC, WAV, AIFF fixtures;
  seek lands within 50 ms of the requested position.
- `cargo deny check licenses` passes; CI green on all three OSes.

## Tests to write

- `keys.rs`: table-driven tests for Camelot/Open Key/classical/flat/sharp
  spellings and rekordbox's exact `Tonality` strings.
- `rbxml`: parse the small fixture; parse a generated 3,000-track file;
  write→parse equality; Location URL decoding for spaces, `#`, `%`, unicode
  NFD input (macOS) → NFC path; Windows drive-letter form `file://localhost/C:/`.
- Import: idempotency (import twice → same row count); unmatched handling;
  prefix remap.
- Scan: add/modify/move/delete scenarios in a temp dir.
- Audio: probe values for each fixture format; decode length equals probe
  duration ± 1 frame; player seek accuracy.
- UI: Vitest + React Testing Library for the table's query building and the
  playlist tree reducer; one Playwright-free smoke test via `tauri-driver` is
  optional (document if skipped).

## Manual checklist

- Import real collection; spot-check 20 tracks against rekordbox side by side.
- Export XML, import into rekordbox 7 as a test playlist; confirm cues/grids.
- Play a track on each OS; switch output device while playing.

## Risks

- **XML is lossy**: rekordbox XML has no waveforms, no phrase data, and only
  the TEMPO anchors of a dynamic grid; Phase 2 must source waveforms elsewhere
  (rekordbox's local ANLZ cache or generation). Mitigation: store `rb_track_id`
  and `rb_location` now so Phase 2 can locate rekordbox's ANLZ files.
- **Location URL edge cases** (percent-encoding differences between rekordbox
  versions, NFD on macOS). Mitigation: tests above; "map folder" fallback.
- **AAC/ALAC decoding** needs Symphonia's non-default features; HE-AAC is not
  supported natively (rare in DJ libraries). Mitigation: show "unsupported
  codec" rather than failing silently; log kind.
- **Large libraries in the WebView**: never send the whole table; page from
  SQLite.
- **Scope creep** into analysis: resist; "needs analysis" flag only.
