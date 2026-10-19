# Phase 1 — Scaffold + library

Status: in progress (tasks 1–10 done 2026-10-19). Depends on: nothing. Read `CLAUDE.md` and `docs/decisions.md` first.

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
    pub fn tracks(&self, q: &TrackQuery) -> Result<TrackPage>;          // { rows, total }; paged, sorted, filtered
    pub fn facets(&self) -> Result<Facets>;                             // genres, keys, colours, playlists for filters
    pub fn track(&self, id: TrackId) -> Result<TrackDetail>;            // track, cues, grid summary
    pub fn update_track(&self, id: TrackId, patch: &TrackPatch) -> Result<TrackDetail>;
    pub fn artwork(&self, id: ArtworkId) -> Result<Vec<u8>>;            // image bytes
    pub fn playlists(&self) -> Result<PlaylistTree>;
    pub fn playlist_mutate(&self, op: PlaylistOp) -> Result<PlaylistOutcome>;
}

pub struct TrackQuery { pub text: Option<String>, pub filters: Vec<Filter>,
                        pub sort: Sort, pub offset: u32, pub limit: u32 }
pub enum Filter { Genre { genre }, KeyIn { keys: Vec<Key> }, BpmRange { min, max } /* bpm×100 */,
                  Color { color }, RatingMin { rating: u8 }, Playlist { id: PlaylistId },
                  Missing { missing: bool }, NeedsAnalysis { needs: bool } }   // serde tag "type"
pub struct Sort { pub column: SortColumn, pub descending: bool }

pub mod scan {   // folder import
    pub fn scan(roots: &[PathBuf], lib: &Library, cancel: &CancelToken,
                progress: impl Fn(&ScanEvent)) -> Result<ScanReport>;
    // ScanEvent::{Found, Progress{done,total}, Added, Updated, Moved{id,from,to}, Missing, Error{path,message}}
    // (struct variants, serialised with a "type" tag for the Tauri channel)
}

pub mod rbxml {  // rekordbox XML
    pub struct Collection { pub version: String, pub product: Product, pub tracks: Vec<XmlTrack>,
                            pub playlists: XmlNode /* ROOT folder */, pub extra: Extra }
    pub fn parse(reader: impl BufRead) -> Result<Collection>;          // quick-xml, streaming
    pub fn write(c: &Collection, w: impl Write) -> Result<()>;
    pub fn import(c: &Collection, lib: &Library, opts: ImportOptions, progress: impl Fn(ImportEvent))
        -> Result<ImportReport>;
    pub fn export(lib: &Library, selection: Selection) -> Result<Collection>;
    pub fn location_to_path(url: &str) -> Result<PathBuf>;              // file://localhost/... → path, percent-decoded, NFC
    pub fn path_to_location(path: &Path) -> Result<String>;             // inverse, for export
}
```

`XmlTrack` carries every attribute in the rekordbox XML spec (TrackID, Name,
Artist, Composer, Album, Grouping, Genre, Kind, Size, TotalTime, DiscNumber,
TrackNumber, Year, AverageBpm, DateModified, DateAdded, BitRate, SampleRate,
Comments, PlayCount, LastPlayed, Rating, Location, Remixer, Tonality, Label,
Mix, Colour) plus `Vec<Tempo{inizio,bpm,metro,battito}>` and
`Vec<PositionMark{name,type,start,end,num,red,green,blue}>`. Unknown attributes
are preserved in a `extra: BTreeMap<String,String>` so round-trips don't lose data.

Notes (task 6, [`rbxml`](../../crates/reks-core/src/rbxml/))
- Model: text attributes and dates are `Option<String>` kept raw (missing ≠
  `""`); numbers are typed. Positions and BPM stay `f64` seconds/BPM as in the
  XML; conversion to ms happens at import (task 7). `PositionMark.rgb` is set
  only when Red, Green and Blue are all present. Playlist `NODE`s are a
  `Folder`/`Playlist { key_type, keys }` tree; a missing `KeyType` reads as 0.
- Lossless: unknown attributes on every element go to `extra`. A known
  optional value that doesn't parse (`Year=""`, `Colour="red"`, `NaN`) is kept
  raw in `extra` under its own name. Missing or invalid *required* values
  (`TrackID`, `TEMPO` Inizio/Bpm, `POSITION_MARK` Type/Start/Num, `NODE` Type/Name)
  are errors. Unknown elements are skipped with a warning. `Entries`/`Count`
  are checked (warning on mismatch) and recomputed on write.
- Writer: the attribute order and layout of the spec list and the fixture
  (which it reproduces byte for byte), then `extra` sorted by name. Seconds are
  written with 3 decimals and BPM with 2, falling back to the shortest exact
  form when that would round, so `parse(write(c)) == c`. Tab/CR/LF in values
  become `&#9;`/`&#13;`/`&#10;`; other C0 controls are dropped (illegal in XML 1.0).
- `Location`: `file://localhost/` (and `file:///`) only; percent-decoded to
  UTF-8, then NFC. `#`, `?`, `+` and stray `%` are literal. `/C:/...` becomes
  `C:/...`. `path_to_location` encodes everything except `A–Z a–z 0–9 - . _ ~ /`.
- Checked against a real rekordbox 6.8.6 export (3,062 tracks, 2026-10-19;
  not committed): the `TRACK` attribute order is the spec order our writer
  uses; `Composer`/`Grouping`/`Remixer`/`Label`/`Mix`/`Comments`/`Tonality`
  etc. are written empty, while `Colour`, `DateModified` and `LastPlayed` are
  omitted when unset. Inizio/Start/End always have 3 decimals, Bpm/AverageBpm 2.
  `Tonality` is classical (`Am`, `F#m`, `Abm`, `Db`, …), all parsed by `keys.rs`.
  `Location` leaves `( ) , + ? ! ~ # - . _ :` unencoded and encodes space,
  `& ' [ ]` and all non-ASCII (UTF-8). rekordbox itself writes `&apos;`.
  Streaming entries use `file://localhostsoundcloud:…` with `Kind="Unknown Format"`.
  The root `NODE` is written `Type` before `Name`, other folders `Name` first.
- Still UNVERIFIED: whether rekordbox *imports* `&#10;` in attribute values,
  and the import of our `Location` encoding (stricter than rekordbox's).

Notes (task 7, [`rbxml/import.rs`](../../crates/reks-core/src/rbxml/import.rs))
- `import(c, lib, ImportOptions { remap }, progress)`: files are read (hash,
  probe, tags, artwork; shared with the scan in `fileinfo.rs`) before the DB is
  locked; all track, grid, cue and playlist writes are one transaction.
- Location → `location_to_path` → first matching `PrefixRemap` (component-wise,
  `to` made absolute) → native separators (so Windows `C:/…` matches the
  scan's `C:\…`) → NFC path (NFD if only that exists). Match order: path,
  then `rb_location` from an earlier import (relinks after a remap), then
  (artist, title, size), case-folded, only if exactly one row fits. A matched
  row moves to the XML path only if its own file is gone and the XML's exists.
  Two XML tracks on one row: the second is reported `DuplicateOf`.
- Not stored, reported: no `Location`, non-file URLs (streaming), undecodable
  Locations. Stored and reported: file not found (`missing = 1`, `file_size` =
  XML `Size` or 0, `file_mtime` = `''`, `kind` from extension or XML `Kind`)
  and files that exist but don't probe (XML `TotalTime`/`SampleRate`/`BitRate`).
- Fields: XML values win; an absent or blank attribute never clears a value;
  tags fill only what is still NULL. Rating `round(r/51)`; `Tonality` → `keys.rs`
  (`key_source = rekordbox`, unparsable → warning, key kept); `AverageBpm` → `bpm_x100`
  (0 = none); 0 for track/disc number and year = none; dates must be `yyyy-mm-dd`.
- `TEMPO` → `beatgrid` (seconds × 1000, `Metro` n/d, bad `Metro` → 4/4 + warning),
  `POSITION_MARK` → `cue` (`Num` −1 → slot NULL, 0..7 → slot, else skipped;
  `End` → `end_ms`; RGB → `color_rgb`). Only `source = 'rekordbox'` rows are
  replaced, and only when they differ, so re-imports keep ids.
- Playlists: matched by name and kind within their parent (n-th same-named
  sibling ↔ n-th existing); new ones appended. Contents are the XML entries in
  order, then any tracks added in reks that the XML lacks. Playlists not in the
  XML are untouched. KeyType 0 via TrackID, 1 via Location.
- Colour: `track.color` stores the lowercase rekordbox names in PDB id order
  (`pink red orange yellow green aqua blue purple`; table in
  [`colour.rs`](../../crates/reks-core/src/rbxml/colour.rs)). Exact RGB match only;
  other values → NULL + warning. Pink/orange/yellow/purple RGB are UNVERIFIED
  against rekordbox output (R§7).
- Timings (release build, maintainer's Mac): real 3,062-entry collection (2,646 files
  present) 8.1 s, re-import 0.08 s; generated 3,000 tracks 2.2 s. 20 sampled
  tracks matched the XML exactly on key, rating, BPM, colour, cues and grid.
  Debug builds are ~15× slower (the 3,000-track test takes ~35 s there).

Notes (task 8, [`query.rs`](../../crates/reks-core/src/query.rs), [`src/`](../../src/))
- `Library::tracks(&TrackQuery) -> TrackPage { rows, total }`. `limit` is clamped to
  1..=1,000 (`MAX_PAGE`); the UI fetches 200-row pages as they scroll into view.
  All filters AND together; user values are always bound parameters.
- Text: each whitespace-separated word with a letter or digit becomes a quoted FTS5
  prefix term (`"deep"* "nig"*`), so every word must prefix-match a word of title,
  artist, album, genre, comment, label or remixer, and FTS syntax in the input is plain
  text. Diacritics fold both ways via the 0001 `unicode61` tokenizer (`beyonce` ↔ `Beyoncé`).
- Filters: genre and colour compare case-insensitively; `KeyIn` uses canonical keys;
  `BpmRange` is bpm×100, inclusive; `Playlist` on a folder means any playlist below it.
  When the first `Playlist` filter names a playlist (not a folder), the result has one
  row per entry, so a track in it twice is two rows (`TrackRow.entryPosition`, null
  elsewhere) and `total` counts entries (task 10).
- **Needs analysis** = the track has no `beatgrid` row (any source). Key and BPM alone
  don't count: a grid is what export needs, and what rekordbox XML carries for analysed
  tracks. `TrackRow.needsAnalysis` exposes it.
- Sort: empty values (NULL, and `''` for text) last in both directions, text
  case-insensitive, ties by id (stable paging). Key sorts in Camelot order (1A, 1B, 2A …),
  colour in rekordbox order, `playlistPosition` by position in the filtered playlist.
- No migration 0002: on 10,000 generated tracks every query in the test takes
  0.5–8 ms in release (2–23 ms debug; slowest is the last page by title), well inside
  the 100 ms budget, so no extra index paid for itself.
- `library_facets` sends genres, the 24 keys with Camelot labels, the colour names and the
  playlist tree, so key and colour knowledge stays in Rust.
- App: `AppState` opens `reks.sqlite` in `app_data_dir` (`app_local_data_dir` on
  Windows, D19); commands run on `spawn_blocking`. `scan_start` / `rbxml_import_start`
  stream progress over a `Channel`; the toolbar uses them until the wizard (task 12).
  "Import rekordbox XML…" is one step (pick the file, it imports). If files are not
  found, it names the folder they were expected in (`ImportReport.missing_root`: the
  deepest common folder of the not-found paths, `None` when only a filesystem root is
  shared) and offers "Locate folder…", which re-imports with that prefix remapped.
- UI: Zustand store (`src/store/library.ts`) builds the `TrackQuery`; a generation
  counter drops stale pages. Selection is a pure reducer over row indices of the current
  result (click, cmd/ctrl toggle, shift range, arrows/Page/Home/End with shift-extend,
  cmd/ctrl+A, Esc) and is cleared when the query changes. Column visibility is not
  persisted yet (task 13). Vitest covers query building, the selection reducer and the
  search debounce; a `tauri-driver` smoke test is skipped (optional).
- 10k manual check: `reks synth-xml 10000 synth.xml` writes a synthetic collection (files
  missing) to import in the app. The fixture XML imports with its files missing until
  "Locate folder…" points it at `fixtures/audio`.

Notes (task 9, [`track.rs`](../../crates/reks-core/src/track.rs), [`TrackDetailPanel`](../../src/components/TrackDetailPanel.tsx))
- `Library::track(id) -> TrackDetail { track, cues, beatgrid }`. `Track` serialises as the
  `TrackRow` fields (flattened) plus every other column and `artwork { id, mime, width,
  height }`, so the UI can drop it into the table cache. Cues come hot cues by slot first,
  then memory cues and loops by start. `beatgrid` is `{ markers, firstBeatMs, bpmMinX100,
  bpmMaxX100 }`, or `null` when there is no grid. An unknown id is `Error::TrackNotFound`.
- Editable: title, artist, album, album_artist, genre, composer, remixer, label, mix,
  grouping, comment, track_no, disc_no, year, rating, color, key. BPM, grid and cues are
  read-only (grid editing is a later phase). Edits change the DB only, never the file tags.
- `TrackPatch`: an absent field is unchanged, `null` clears it. Unknown fields are refused.
  Every field is validated before anything is written, and a failure is
  `Error::InvalidField { field, reason }` with the row left unchanged. Text is trimmed,
  empty means NULL, and the limit is 1,024 chars (our own guard). Control characters are
  refused (comment allows LF and tab), since they can't round-trip through rekordbox XML.
  track_no and disc_no take 1–999, year 1–9999 and rating 0–5. Colour is a
  `TRACK_COLOURS` name in any case, stored lowercase. Key accepts any `keys.rs` notation,
  stored canonical with `key_source = 'manual'`; clearing it clears `key_source`.
- An edit sets `updated_at` (UTC ISO-8601) in the same transaction. An empty patch writes
  nothing. The 0001 `track_fts_au` trigger re-indexes FTS when a searched column changes.
- `artwork_get(artwork_id)` returns the cached image as raw bytes (`ipc::Response`). The UI
  shows it through a Blob URL with `artwork.mime`, and shows a placeholder when there is
  no artwork or the cache file is gone.
- UI: the panel follows the primary selected row (`primaryIndex`: the focused row if it is
  selected, else the lowest selected one), so with several rows selected it shows one track.
  Bulk edit is not built yet. Save sends only the changed fields; Revert or Esc resets.
  Switching tracks drops unsaved edits, and a rejected save shows the core error inline.
  After a save, `replaceRow` swaps the row into the loaded pages without re-querying, so
  selection and scroll stay put. A row that no longer matches the filter or sort stays until
  the next refresh. Facets are reloaded so a new genre shows up in the filter.

Notes (task 10, [`playlist.rs`](../../crates/reks-core/src/playlist.rs), [`PlaylistTree`](../../src/components/PlaylistTree.tsx))
- `Library::playlists() -> PlaylistTree { nodes }`: the depth-first `PlaylistNode` list that
  facets already use, now with `position` (index among siblings) and `entries`.
- `Library::playlist_mutate(PlaylistOp) -> PlaylistOutcome` (serde tag `type`): `createPlaylist`,
  `createFolder`, `rename`, `delete` (a folder takes its subtree and their entries), `move`,
  `addTracks`, `removeEntries`, `reorderEntries`. `position` / `before` are indices in the list as
  it is now (`0..=len`, `len` = end); reks-core adjusts for the moved items, which keep their
  relative order. A reorder moves the given entries as one block.
- Every op is validated before writing and runs in one transaction. Errors: `PlaylistNotFound`,
  `TrackNotFound`, `NotAPlaylist` (entries of a folder), `NotAFolder` (a playlist as parent),
  `MoveCycle` (into itself or a descendant), `IndexOutOfRange`, and `InvalidField { field: "name" }`
  (trimmed; empty or control characters refused, the task 9 text rules).
- Entry positions and sibling `sort_order`s stay dense (`0..n`). Entry edits delete and re-insert
  from the first changed position, so the `(playlist_id, position)` key never collides.
  Removing an entry removes that position only, not other copies of the track.
- `updated_at` (UTC ISO-8601) is set on each node whose name, parent, `sort_order` or entries
  change; a no-op writes nothing.
- **Duplicates.** A playlist may hold a track more than once, but only when the user says so.
  `addTracks.duplicates` is `ask` (default), `allow` or `skip`. A requested track is a duplicate
  if the playlist has it or the same request named it earlier. `ask`: any duplicate → nothing
  written, outcome `duplicates { trackIds, duplicateCount, requested }` (data, not an error);
  none → added. `allow` adds all; `skip` adds only the rest. The UI always sends `ask` and on
  `duplicates` shows "N of M tracks are already in <playlist>" with Add anyway (`allow`),
  Skip duplicates (`skip`) and Cancel (nothing sent); Skip is left out when all M are
  duplicates. A remembered choice is task 13.
- XML re-import: matching (task 7) is unchanged and works on edited trees, since sort orders stay
  dense. XML entry order wins; entries added in reks follow it, counted per track, so a second
  copy added in reks survives. A renamed or moved playlist no longer matches its XML node, so a
  re-import creates that one anew.
- UI layout follows rekordbox: the left sidebar holds only the tree (Collection, then a
  collapsible Playlists root with a `+` button; disclosure triangles, folder and playlist icons).
  Filters sit in a bar between the toolbar and the table (they narrow the whole list); the
  toolbar's Filters button shows or hides it and badges how many filters are set (muted when
  off). A control that filters is outlined; keys are picked from a Camelot popover; the bar
  wraps on narrow windows. The on/off switch next to that button applies the filters or sets
  them aside (values kept, the playlist still applies) and works with the bar hidden. Track
  info is shown only by the toolbar's info button (selecting a row never opens it): it slides
  over the table's right edge (Web Animations: 240 ms ease-out in, 160 ms fast-start out,
  reversing mid-slide from where it is; none under reduced motion), so the table and toolbars
  keep their width, and shows the primary selected track. Scan and import
  live in an app header across the whole window (the home for future library-wide actions),
  with their status line under it; the list toolbar holds only list controls. Both start as bar shown, filters on, info
  hidden; saving that is task 13.
- UI: the tree is a Zustand store over a pure reducer (Playlists root open, expanded folders,
  selection, rename). Selecting a playlist sets the `playlist` filter and sorts by
  `playlistPosition`; a folder shows everything below it; Collection clears it. Clear (filters)
  keeps the selected playlist. Context menu and keys: F2 / Enter rename in place (Enter commits,
  Esc cancels, blur commits), Delete / Backspace asks, then deletes. The table drags its
  selection onto a playlist; inside a playlist sorted by `playlistPosition` ascending it drags
  to reorder (top / bottom half of the target row); Delete or the menu removes the selected
  entries. Tree nodes drag before / after a node or into a folder (top / bottom quarter, middle).
  A `#` column shows the entry position. After every edit the tree, facets and table reload.
- HTML5 drag and drop needs `dragDropEnabled: false` on the window (`tauri.conf.json`); file drops
  from the OS are not accepted until something needs them.
- Not checked yet in the running app (`cargo tauri dev`); covered by reks-core and Vitest tests.

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

Notes (task 4, [`reks-audio`](../../crates/reks-audio/src/))
- `probe` takes kind, rate, channels, bit depth and duration from Symphonia.
  Duration excludes encoder delay/padding when the file records it (MP3 LAME
  header). AAC priming is not trimmed (no AAC gapless in Symphonia), so M4A
  reads ~23 ms long. Bit rate: exact for WAV/AIFF, lofty's stream bit rate
  otherwise (tags and artwork not counted).
- `tags` merges the primary tag with the file's other tags; `key` is the raw
  initial-key string for `keys.rs` to normalise. No `mix` field yet: which
  tag frame rekordbox reads "Mix" from is UNVERIFIED.
- Artwork: `Library::store_artwork` (`reks-core`) sniffs the bytes, de-dups by
  SHA-256 into `<cache_dir>/artwork/<sha256>.<ext>`, and stores `blob_path`
  relative to the cache dir. A deleted cache file is rewritten on the next store.

Scan rules (task 5, [`scan.rs`](../../crates/reks-core/src/scan.rs))
- Picks up `mp3 m4a aac flac wav wave aif aiff`; skips dot-files and
  dot-folders (AppleDouble `._*`, `.Trashes`). Symlinks are not followed.
- Paths are stored NFC when the NFC spelling opens the same file (macOS);
  otherwise as on disk.
- Same path, same size + mtime → unchanged. Changed file → technical fields
  refreshed; tag fields only filled where the DB value is empty (the DB is the
  source of truth). New path with the `content_hash` + size of a row whose file
  is gone → move (row, cues and playlists kept). Otherwise insert.
- Missing: set only after a complete walk. Never on cancel, for an absent root
  (unplugged drive), or under a folder that could not be read. Rows are never
  deleted by a scan.

### Tauri commands (`src-tauri`)

`library_query`, `library_facets`, `library_track`, `library_update_track`, `playlists_get`,
`playlists_mutate`, `scan_start` (progress via `Channel<ScanEvent>`), `scan_cancel`,
`rbxml_import_start` (progress via Channel), `rbxml_export`,
`player_load/play/pause/seek/volume/devices`, `player_state` (polled or event
every 100 ms), `artwork_get(artwork_id) -> Response(bytes)`, `overview_waveform(id) ->
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
4. [x] **Audio probe + tags + artwork**: `probe`, `tags` (`lofty`), artwork
   extraction and de-dup into cache, unit tests on the fixtures.
5. [x] **Folder scan**: walk roots (`walkdir`), extension filter, NFC normalise
   paths, insert/update by path, detect moves by `content_hash`, mark missing,
   progress events, cancellation token. Tests on a temp dir.
6. [x] **rekordbox XML parser/writer**: streaming parse with `quick-xml` into
   `Collection`; writer that reproduces rekordbox's element order and the
   `file://localhost/` Location form (percent-encoding rules from the XML
   spec); round-trip test on `collection-small.xml` and on a 3,000-track
   sample (generated).
7. [x] **rekordbox XML import**: map `XmlTrack` → `track`, `TEMPO` → `beatgrid`,
   `POSITION_MARK` → `cue` (Num −1 memory, 0..7 hot, Type 4 loop with End),
   playlists tree, options: match by Location path, fall back to
   (artist,title,size); report unmatched tracks; idempotent re-import updates
   rather than duplicates.
8. [x] **Track table UI**: virtualised table (`@tanstack/react-virtual` +
   `@tanstack/react-table`), column chooser, sorting, 50 ms debounced search
   calling `library_query`, filters sidebar, multi-select, keyboard navigation.
9. [x] **Track detail + inline edit**: side panel with editable fields, cue and
   grid listing (read-only), artwork display via `artwork_get`.
10. [x] **Playlists UI**: tree, create/rename/delete folder/playlist, drag tracks,
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
