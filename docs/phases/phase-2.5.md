# Phase 2.5 — Stick round trip (playlist dividers, play history, adopt)

Status: started 2026-10-20 ahead of the Phase 2 hardware campaign with the
maintainer's approval (software only; anything that needs a player stays
UNVERIFIED); tasks 1–2 done. Depends on: Phase 2 done, including its hardware campaign,
history safeguard and `export_manifest` (tasks 12–14). Runs before Phase 3.
Read `docs/decisions.md` D7, D22, D23, D27 and `docs/research.md` §10 first.

## Goal

1. **Dividers.** A user inserts named dividers anywhere in a reks playlist. On a
   CDJ each one shows as a row with the label as its title, in the right place.
   Loading one by accident plays a few seconds of silence and affects nothing else.
2. **Play history (H1).** reks imports the history playlists that players wrote
   to a stick's `export.pdb` (or to a Phase 2 backup of it) as sessions, without
   changing the stick. Play count and last played are derived from a frozen
   rekordbox baseline plus included sessions, so they can always be explained
   and recomputed. A session can be marked practice and excluded; undoing
   that restores the exact previous counts.

## Scope

- Library: divider entries in playlists (D22); sessions, entries, baseline and
  derived counts (D23).
- Export: dividers become synthetic planned tracks backed by a generated silent
  WAV, behind the `export.dividers` flag.
- `reks-core::history`: read a stick or a backup folder, match entries to library
  tracks, dry-run import plan, import.
- Export guard: the Phase 2 safeguard gains "Import now". An export stops when
  the stick holds sessions that are not imported, with Import now (default) /
  Back up and continue / Continue and discard (explicit confirm).
- UI: dividers in the playlist view; Histories tree, session view, import
  dialog, "Played in" on the track detail panel, play columns and filters.
- One hardware session on CDJ-2000NXS2 and CDJ-3000 covering both features.

## Non-goals

- Hiding dividers from All Tracks, search or history on the player. No PDB flag
  for that is known; they appear there.
- Dividers in rekordbox XML export (omitted); detecting hand-made silent
  "divider tracks" in an imported `collection.xml` (they stay normal tracks);
  colours or icons for dividers.
- Counting reks preview or Phase 6 mixer playback as plays (never).
- Writing history or play counts to sticks (player-owned data).
- Manual sessions; rekordbox's own past histories (not in XML, `master.db` is
  out of scope; workaround: export them as playlists in rekordbox first);
  automatic practice detection; Pro DJ Link capture; ratings, charts, sharing.
- OneLibrary history (H2, Phase 4 task 11) and Engine history (H3, Phase 5
  task 11).

## Data model (`reks-core`, migration [0005](../../crates/reks-core/migrations/0005_dividers_history.sql))

```sql
-- Dividers (D22). Table rebuild: track_id becomes nullable.
--   playlist_track + item_kind TEXT NOT NULL DEFAULT 'track'   -- 'track' | 'divider'
--                  + label     TEXT
--   CHECK ((item_kind = 'track'   AND track_id IS NOT NULL AND label IS NULL) OR
--          (item_kind = 'divider' AND track_id IS NULL     AND label IS NOT NULL))

CREATE TABLE play_session (
  id              INTEGER PRIMARY KEY,
  source          TEXT NOT NULL,          -- 'device_library' (H2/H3 add 'onelibrary', 'engine')
  fingerprint     TEXT NOT NULL UNIQUE,   -- see Matching
  volume_serial   TEXT,
  source_name     TEXT NOT NULL,          -- as on the stick, e.g. 'HISTORY 001'
  name            TEXT NOT NULL,          -- user-editable, defaults to source_name
  played_on       TEXT,                   -- YYYY-MM-DD; NULL until HIST1 is decoded
  venue TEXT, notes TEXT,
  excluded        INTEGER NOT NULL DEFAULT 0,
  excluded_reason TEXT,                   -- 'user' | 'pre_baseline'
  imported_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE TABLE play_session_entry (
  session_id INTEGER NOT NULL REFERENCES play_session(id) ON DELETE CASCADE,
  position   INTEGER NOT NULL,
  item_kind  TEXT NOT NULL DEFAULT 'track',   -- 'divider' entries never count
  track_id   INTEGER REFERENCES track(id) ON DELETE SET NULL,
  raw_title TEXT, raw_artist TEXT, raw_path TEXT, raw_duration_ms INTEGER,
  PRIMARY KEY (session_id, position)
);
CREATE INDEX play_session_entry_track_idx ON play_session_entry(track_id);

ALTER TABLE track ADD COLUMN baseline_play_count  INTEGER NOT NULL DEFAULT 0;
ALTER TABLE track ADD COLUMN baseline_last_played TEXT;
-- Data step: baseline_* := play_count / last_played; setting
-- 'history.baseline_date' := the migration date (UTC; Phase 1 stores no import
-- date, and a later cutoff only excludes more sessions by default). From then on
-- track.play_count / last_played are a cache written only by recompute_play_stats,
-- so existing indexes and sorting keep working.
```

Count rules (D23)
```
play_count(t)  = baseline_play_count(t) + count(track entries of t in included sessions)
last_played(t) = max(baseline_last_played(t), max(played_on) of included sessions containing t)
```
- Included means `excluded = 0`. Sessions with `played_on` on or before the
  baseline date import excluded (`pre_baseline`, "may already be in
  rekordbox's count"); unknown dates are preselected as excluded in the dry run.
- The baseline is frozen. A later rekordbox XML re-import sets `baseline_*`
  only for tracks it creates, never for existing ones, because rekordbox may
  have counted the same stick histories. This replaces Phase 1's "XML values
  win" rule for `PlayCount` and `LastPlayed`; the import then recomputes.

Matching (stick entry → library track)
1. `export_manifest` by (volume serial, stick path): exact for reks sticks;
   `item_kind = 'divider'` marks divider entries.
2. Otherwise (rekordbox-made stick, unknown serial): stick path suffix against
   library paths, then (artist, title, duration ±1 s). Only a unique match counts.
3. Otherwise unmatched: raw fields kept, never counted, linkable later.

Fingerprint: SHA-256 (`sha2`, already in the tree via baken) of source, volume
serial, source name and the ordered entry stick paths. Same fingerprint: skip.
Same serial and name with the stored entries as a strict prefix: extend that
session (UNVERIFIED whether players append across mounts, HIST3).

## Interfaces

```rust
// reks-core
pub enum PlaylistOp { /* existing ops */ InsertDivider { playlist_id, position: Option<u32>, label }, RenameDivider { playlist_id, position, label } }
// move/removeEntries/reorderEntries work on divider positions unchanged; dividers are never duplicates

pub mod history {
    pub enum HistorySource { Stick(PathBuf), Backup(PathBuf) }   // Backup = a Phase 2 stick-backups/<serial>/<ts>/ folder
    // The PDB reader is reks_export_rb::history::read_play_history (task 5);
    // PlayHistory::to_source_history() gives scan its input.
    pub fn scan(history: &SourceHistory, lib: &Library) -> Result<HistoryImportPlan>;  // read-only
    pub fn import(plan: &HistoryImportPlan, choices: &ImportChoices, lib: &Library) -> Result<ImportReport>;
}
impl Library {
    pub fn sessions(&self, q: &SessionQuery) -> Result<Vec<SessionRow>>;
    pub fn session(&self, id: SessionId) -> Result<SessionDetail>;
    pub fn session_update(&self, id: SessionId, patch: SessionPatch) -> Result<()>;   // name, venue, notes, excluded
    pub fn session_delete(&self, id: SessionId) -> Result<()>;
    pub fn session_link_entry(&self, id: SessionId, position: u32, track: TrackId) -> Result<()>;  // "Locate…"
    pub fn track_sessions(&self, id: TrackId) -> Result<Vec<SessionRef>>;
    pub fn recompute_play_stats(&self, ids: TrackSet) -> Result<()>;                 // All | Some(Vec)
}
// Query filters: NeverPlayed, PlayCountRange { min, max }, PlayedSince { date }
```

Divider rules
- Labels: trimmed, non-empty, at most 64 characters (display limit UNVERIFIED),
  same text rules as playlist names. The UI suggests ASCII (`---- PEAK ----`);
  other characters are allowed but flagged until the hardware session shows
  how they render.
- Playlist view: when the first `Playlist` filter names a playlist, the rows
  are its track entries and `TrackPage.dividers` lists every divider (entry
  position, label) on every page, so the UI can place them between rows
  without paging them. Dividers show only in ascending playlist order with no
  other filter or search; sorted or filtered views get none. Track counts
  (`entries`, `total`) exclude dividers.
- rekordbox XML: export omits dividers; a re-import that reorders a
  playlist's tracks keeps each divider at its entry position (clamped to the end).
- Export: each divider becomes a `PlannedTrack` (`kind = Divider`) whose source
  is a generated WAV in `app_cache_dir/dividers/div-<playlist_id>-<n>.wav`:
  PCM 16-bit, 44.1 kHz, stereo, `DIVIDER_SECONDS` = 4, header written by hand
  (no new dependency). Temp XML row: Name = label; Artist, Album, Genre empty;
  no Tonality, AverageBpm, TEMPO or POSITION_MARK.
- baken places the file under `/Contents/<Artist>/<Album>/` itself; we don't
  choose the folder (R§10). Stale dividers go with `prune` like any file.
- ANLZ: the planner sets `generate_analysis` when the plan has dividers. baken
  applies it only to tracks rekordbox never analysed, and the planner already
  leaves other unanalysed tracks out unless the user opted in. If baken can't
  analyse a silent file with no TEMPO, dividers wait for `reks-anlz` (Phase 3).
  Shipping dividers without ANLZ is not an option.
- Each divider gets its own file until hardware shows two PDB rows can share
  one (DIV3). The dry run lists dividers as their own group ("3 dividers, 2.1 MB").
  `export_manifest` rows carry `item_kind = 'divider'`.

## Tasks

1. [x] **Migration**: `playlist_track` rebuild, `play_session`,
   `play_session_entry`, baseline columns, data step, baseline date. Tests:
   existing entries preserved; CHECK rejects invalid rows; baseline copied;
   re-run is a no-op. Done: migration 0005, tests in
   [db.rs](../../crates/reks-core/src/db.rs). The baseline date is stored as
   setting `history.baseline_date` (JSON string, `YYYY-MM-DD`).
2. [x] **Divider ops and query**: `InsertDivider`, `RenameDivider`, label rules,
   entry rows in the playlist query, counts without dividers, XML writer omits
   them. Tests per op; dividers absent from search and track counts. Done in
   [playlist.rs](../../crates/reks-core/src/playlist.rs) (`Entry`,
   `MAX_DIVIDER_LABEL_CHARS`, `Error::NotADivider`) and
   [query.rs](../../crates/reks-core/src/query.rs) (`DividerRow`); tests in
   [tests/dividers.rs](../../crates/reks-core/tests/dividers.rs).
3. **Divider UI**: "Insert divider" in the playlist context menu, inline
   rename, drag like an entry, full-width row; hidden when sorted or filtered.
4. **Divider export**: WAV generator (round-trip through Symphonia probe and
   `audiocheck`), planner expansion, temp XML rows, `generate_analysis`,
   manifest kind, dry-run group, `export.dividers` flag (off with an UNVERIFIED
   badge until task 11 passes). Temp-dir test: 30 tracks and 3 dividers →
   33 playlist entries in order (rekordcrate), 3 WAVs with ANLZ, zero validator
   findings; a second identical export copies nothing; removing a divider and
   re-exporting deletes exactly its WAV and ANLZ.
5. [x] **History reader**: `HistoryPlaylist` / `HistoryEntry` rows via rekordcrate;
   resolve each entry to its PDB track row (stick path, title, artist,
   duration); dates from table 19 when decodable (HIST1), else NULL; works on a
   stick and on a backup folder. Golden JSON test on the played-stick fixture
   from Phase 2 (two or more history playlists from different dates).
   Done in `reks_export_rb::history::read`
   ([read.rs](../../crates/reks-export-rb/src/history/read.rs)):
   `read_play_history(&HistorySource, &HistoryReadOptions) -> PlayHistory`.
   It lives in reks-export-rb, not reks-core, because rekordcrate is here and
   reks-core can't depend on this crate; task 6's `reks-core::history::scan`
   takes the `PlayHistory` from the caller. A backup is read only with its
   `manifest.json` (serial from there) and when `export.pdb` hashes as recorded.
   A missing track or artist row leaves the entry's fields `None` (unmatched
   later). Dates: a heuristic over table 19 (**UNVERIFIED**, no source; see the
   module docs), behind `HistoryReadOptions::hist1_dates`, off by default, so
   `played_on` is NULL until the fixture confirms it. Synthetic tests cover two
   dated sessions, a missing track row, undecodable dates and a backup folder.
   The golden JSON test (`played_stick_fixture_golden_json`) is `#[ignore]`
   until `fixtures/rekordbox/played-<ver>/` (with `history.json`) exists from
   the hardware session.
6. [x] **Matching, fingerprint, import plan**: as above. Tests: manifest hit,
   divider entry, suffix fallback, metadata fallback (unique only), unmatched
   kept and linkable; fingerprint skip, extend, different serial.
   Done in `reks_core::history` ([mod.rs](../../crates/reks-core/src/history/mod.rs),
   [matcher.rs](../../crates/reks-core/src/history/matcher.rs)); tests in
   [tests.rs](../../crates/reks-core/src/history/tests.rs) and an end-to-end
   synthetic stick in `reks_export_rb::history::read`. Choices made here:
   a manifest row whose track was deleted falls through to the fallbacks;
   the suffix fallback compares whole path components (NFC, case-folded),
   needs the file name to match, picks the strictly longest suffix and drops
   a candidate whose known duration is off by more than 1 s; the metadata
   fallback needs both durations. Fingerprint fields are length-prefixed;
   extend also applies when both serials are absent and takes the longest
   stored prefix; extending replaces the session's fingerprint. A plan entry
   whose `Extend` no longer fits fails the whole import (`StaleHistoryPlan`);
   a `New` imported meanwhile is skipped. New sessions dated on or before
   the baseline date, or undated, default to excluded (`pre_baseline`); an
   explicit Practice choice stores `user`. `Library::session_link_entry`
   ("Locate…") is here; counts are not recomputed until task 7. The fixture
   test (`played_stick_fixture_imports_once`) is `#[ignore]` like task 5's.
7. **Derived counts**: `recompute_play_stats`, pre-baseline rule, frozen
   baseline on XML re-import. Property test: count = baseline + included
   track entries for random session sets and toggles. Deleting all sessions
   returns every track to baseline.
8. **Export guard**: "Import now" / "Back up and continue" / "Continue and
   discard" when `scan` finds sessions not yet imported; the planner refuses
   to apply without a choice. Tests on the fixture stick.
9. **History UI**: Histories tree (year → month → session); session view in
   play order with unmatched entries greyed and "Locate…", dividers hidden by
   default; header with rename, venue, notes, Practice toggle, delete
   (confirm); import dry-run dialog with a Practice checkbox per session;
   "Played in" on the track detail panel (practice sessions marked); Play
   count and Last played columns and filters.
10. **`reks-cli history scan/import`** for headless runs.
11. **Hardware session**: checklist below on CDJ-2000NXS2 and CDJ-3000; log in
    `docs/hw-test-log.md`; turn `export.dividers` on by default only after both pass.
12. **Adopt a foreign stick** (D27, [`../stick-profiles.md`](../stick-profiles.md) §4.3):
    `adopt::scan` / `adopt::apply`, read-only on the stick; history import
    (tasks 5–6) first; Link / Create under `From stick/<name>/` / Skip per
    playlist; UI dialog. First check open item S4 (rekordcrate on a
    rekordbox 7 fixture). Tests per stick-profiles §8 (golden `AdoptPlan`,
    stick bytes unchanged, Link only on exact ordered match).

## Acceptance criteria

- Dividers: insert, rename, move and delete work and survive a restart;
  dividers are absent from search and counts; task 4's temp-dir test passes;
  the hardware checklist passes on NXS2 and CDJ-3000 and is logged.
- History: the fixture stick imports as one session per history playlist with
  entries in player order; on a reks-exported stick 100% of entries match
  (dividers recognised as dividers).
- Importing the same stick or backup again adds 0 sessions, and the stick's
  files are byte-identical before and after (hash check).
- Toggling Practice on a 100-track session updates play count and last played
  within 100 ms; toggling back restores the exact previous values.
- Baselines survive the migration and every recompute.
- An export onto a stick with sessions not yet imported never proceeds without
  an explicit choice; "Back up and continue" produces a copy that `scan` reads.

## Hardware checklist (one session per player)

1. Export a playlist of 30 tracks and 3 dividers: one ASCII label, one with
   box-drawing characters, one with non-ASCII letters.
2. Playlist view: labels in the right positions; note any `?` and the visible
   title length (DIV2).
3. Load a divider: no error, silence, waveform blank or flat; note quantise
   and sync behaviour with no grid, and where auto cue lands.
4. Load a normal track right after a divider: normal.
5. All Tracks, Artist and search: note where dividers appear and sort.
6. Mount time about the same as the same stick without dividers.
7. Play 5 tracks (one for under 30 s, one for over 2 min), load one divider,
   and load one track without playing it. Eject, re-insert, play 2 more, eject.
   Run `reks-cli history scan`: note what was logged (HIST2), whether the second
   mount appended or made a new session (HIST3), and that the divider is marked
   as a divider and not counted.
8. Import into reks, then re-export: the guard finds nothing un-imported and
   the export proceeds.
9. Record player, firmware, date and results in `docs/hw-test-log.md`.

## Open items (resolve by fixture or hardware, then log)

- DIV1 Minimum safe divider length: try 1 s against 4 s.
- DIV2 Visible title length per player.
- DIV3 Whether two PDB rows can share one file; baken's folder and file name for
  empty artist/album; baken's generated analysis of a silent file with no TEMPO.
- DIV4 Whether the divider's artist should stay empty or carry the playlist name
  (changes how it looks in All Tracks and search).
- HIST1 History dates (PDB table 19): rows reported with ascending id and ASCII
  `YYYY-MM-DD`, layout undecoded. Task 5's heuristic (one date per row, one row
  per playlist, paired by ascending id) is off until the played-stick fixture
  confirms or replaces it.
- HIST2 What a player logs: loaded but not played? minimum play time?
- HIST3 One history playlist per mount, or appended across mounts?
- HIST4 Volume serial via the OS: tracked as Phase 2 task 12.
- Answered: rekordcrate 0.3.0 reads history playlists and entries (R§10).

## Risks

- **Dividers look like tracks everywhere but the playlist view** (All Tracks,
  search, player history). Mitigation: accept and document; empty artist;
  hardware notes.
- **A divider the player can't load** would break a set. Mitigation: a real,
  valid WAV through `audiocheck` and the validator; flag off until both players pass.
- **Wrong matches inflate counts.** Mitigation: exact manifest matches for reks
  sticks; fallbacks need a unique match; unmatched entries never count.
- **Double counting with rekordbox.** Mitigation: frozen baseline; sessions
  before the baseline date import excluded.
- **History format unknowns** (HIST1, HIST3). Mitigation: dates optional; prefix
  rule for appended sessions; fixtures from the maintainer's played sticks.
