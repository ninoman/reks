# Phase 2.5 — Stick round trip (playlist dividers, play history, adopt)

Status: started 2026-10-20 ahead of the Phase 2 hardware campaign with the
maintainer's approval (software only; anything that needs a player stays
UNVERIFIED); tasks 1–10 and 12 done; task 11 (hardware) waits for players. Depends on: Phase 2 done, including its hardware campaign,
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
- Counting reks preview or Phase 5 mixer playback as plays (never).
- Writing history or play counts to sticks (player-owned data).
- Manual sessions; rekordbox's own past histories (not in XML, `master.db` is
  out of scope; workaround: export them as playlists in rekordbox first);
  automatic practice detection; Pro DJ Link capture; ratings, charts, sharing.
- OneLibrary history (H2, Phase 4 task 11) and Engine history (H3, Phase 6
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
  source          TEXT NOT NULL,          -- 'device_library' | 'onelibrary' (H2, Phase 4 task 11); H3 adds 'engine'
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
- Included means `excluded = 0`. Every session imports included, whatever
  its date; the user marks practice afterwards (maintainer's call,
  2026-10-20). There is no default exclusion: undated and pre-baseline
  sessions used to import excluded (`pre_baseline`, a reason no code writes
  now). A session that rekordbox may already have counted is the user's to
  mark.
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
- The planner gives it the stick path any track with no artist or album gets,
  `/Contents/Unknown Artist/Unknown Album/div-<playlist_id>-<n>.wav` (our
  planner decides paths, not baken's layout). Stale dividers go with `prune`
  like any file.
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
3. [x] **Divider UI**: "Insert divider" in the playlist context menu, inline
   rename, drag like an entry, full-width row; hidden when sorted or filtered.
   Done: the track table's context menu (playlist views) inserts above the
   first selected row, or at the end, and renames in place ("Rename divider",
   double-click or Enter); rows select, drag, reorder and delete like entries
   ([TrackTable.tsx](../../src/components/TrackTable.tsx),
   [DividerLabel.tsx](../../src/components/DividerLabel.tsx)). Table rows map
   to tracks and dividers in `itemAt` ([library.ts](../../src/store/library.ts)),
   assuming dense entry positions. The ops go through the existing
   `playlists_mutate`; `TrackPage.dividerView` says when dividers show (and
   Insert is enabled), and `DividerRow.ascii` flags labels with other
   characters with an UNVERIFIED badge (DIV2). The `#` column is the entry
   position, so it counts dividers. Edits of the shown playlist keep the
   table's scroll position.
4. [x] **Divider export**: WAV generator (round-trip through Symphonia probe and
   `audiocheck`), planner expansion, temp XML rows, `generate_analysis`,
   manifest kind, dry-run group, `export.dividers` flag (off with an UNVERIFIED
   badge until task 11 passes). Temp-dir test: 30 tracks and 3 dividers →
   33 playlist entries in order (rekordcrate), 3 WAVs with ANLZ, zero validator
   findings; a second identical export copies nothing; removing a divider and
   re-exporting deletes exactly its WAV and ANLZ.
   Done in [`divider.rs`](../../crates/reks-export-rb/src/divider.rs)
   (`wav_bytes`, `ensure_wav`, `expand`); the flag is
   `ExportOptions::dividers` (UI: "Playlist dividers" switch with an
   UNVERIFIED badge; CLI: `reks rb plan|export --dividers`). `plan()` expands
   dividers into `PlannedTrack`s with `divider: Some(DividerInfo)` and synthetic
   track ids counting down from `u32::MAX` (refused if they would meet a
   library id); their ANLZ is always `Generated`, whatever
   `missing_analysis` says. `BakenWriter::prepare` now takes the plan and
   writes the dividers' rows into the temp XML
   (`rbxml::export_with_dividers`). The validator marks divider rows
   (`StickTrack::divider`), so the manifest stores them as `divider` with no
   track, and stick profiles leave them out of synced entries. The dry run
   shows a "Dividers" fact ("3 dividers, 2.1 MB: …") and "12 tracks,
   3 dividers" per playlist, and labels them `Divider "<label>"`. A file is
   written only when missing or different, so its time stays and the stick
   copy is reused. `n` counts the playlist's dividers in entry order, so
   removing one frees the playlist's last file (the test removes the middle
   divider and checks exactly that WAV and its ANLZ go). Software result for
   DIV3: baken 4.6.1 generates `.DAT`/`.EXT` for the silent file with no
   `TEMPO`, and the validator reads them back (PPTH right, no cues); what
   players do with them is still open. Tests: `divider::tests`,
   `dryrun::tests::dividers_are_their_own_group`,
   [tests/dividers.rs](../../crates/reks-export-rb/tests/dividers.rs).
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
   a `New` imported meanwhile is skipped. New sessions import counted (the
   undated and pre-baseline default exclusions were dropped on 2026-10-20);
   an explicit Practice choice stores `user`. `Library::session_link_entry`
   ("Locate…") is here; counts are not recomputed until task 7. The fixture
   test (`played_stick_fixture_imports_once`) is `#[ignore]` like task 5's.
7. [x] **Derived counts**: `recompute_play_stats`, frozen
   baseline on XML re-import. Property test: count = baseline + included
   track entries for random session sets and toggles. Deleting all sessions
   returns every track to baseline.
   Done in [stats.rs](../../crates/reks-core/src/history/stats.rs):
   `Library::recompute_play_stats(TrackSet)`, plus `session_update`
   (`SessionPatch`; Practice stores reason `user`, clearing it clears the
   reason) and `session_delete`, which task 9 builds on. History import,
   `session_link_entry` (old and new track), the Practice toggle, session
   delete and the rekordbox XML import each recompute the tracks they touch
   in their own transaction. XML import: `PlayCount` / `LastPlayed` fill `baseline_*` only
   for tracks it creates (an existing row, scanned or imported, keeps its
   baseline). Tests in [tests.rs](../../crates/reks-core/src/history/tests.rs)
   (property test, 100-track toggle under 100 ms) and
   `reimport_keeps_the_play_baseline_frozen` in
   [rbxml_import.rs](../../crates/reks-core/tests/rbxml_import.rs).
8. [x] **Export guard**: "Import now" / "Back up and continue" / "Continue and
   discard" when `scan` finds sessions not yet imported; the planner refuses
   to apply without a choice. Tests on the fixture stick.
   Done in [guard.rs](../../crates/reks-export-rb/src/plan/guard.rs):
   `plan::plan` runs `guard_history` (callers of `plan_sources` call it
   themselves). It reads the stick's sessions (HIST1 dates off: fingerprints
   don't use them) and runs `history::scan`; New and Extend sessions fill
   `ExportPlan::guard` and, while `ExportOptions::history_chosen` is false,
   add `Problem::HistoryNotImported`, so `apply` refuses the plan. The
   choices are `HistoryPolicy::Import` (new), `BackUp` and `Discard` (still
   needs `discard_history_confirmed`). Import now is the new apply stage
   `ImportHistory` before the backup: `history::import` of the dry run's
   scan (`ExportPlan::history_import`, not serialised) with default choices,
   so they all count (Practice is set later in History); it backs up too. An import error
   aborts before any stick write; a session imported meanwhile is skipped.
   Only a stick whose `export.pdb` counts sessions is scanned: an unreadable
   one keeps the Phase 2 backup; a count that the full reader then fails on
   still needs a choice, without Import now (`HistoryImportUnavailable`).
   Dry run: `HistoryView::guard` (title, one line per session, chosen,
   `can_import`); report: "Play history imported and backed up". UI: the
   dry-run step shows the three choices (Import now primary; Continue and
   discard asks for its own confirmation), each plans again; going back
   withdraws the choice. CLI: `reks rb plan|export --import-history |
   --backup-history | --discard-history` (one of them). Tests:
   `plan::guard::tests` (synthetic played stick: blocks, each choice end to
   end, the backup scans as two new sessions, imported stick needs no
   choice, stale import skipped, plan without its scan refused); the
   fixture-stick test `played_stick_fixture_guard` is `#[ignore]`.
9. [x] **History UI**: Histories tree (year → month → session); session view in
   play order with unmatched entries greyed and "Locate…", dividers hidden by
   default; header with rename, venue, notes, Practice toggle, delete
   (confirm); import dry-run dialog with a Practice checkbox per session;
   "Played in" on the track detail panel (practice sessions marked); Play
   count and Last played columns and filters.
   Done: reks-core reads sessions back in
   [sessions.rs](../../crates/reks-core/src/history/sessions.rs)
   (`Library::sessions(&SessionQuery)`, newest first with undated last;
   `session(id)` with entries in play order, library fields for matched
   entries and the stick's `raw_*` for all; `track_sessions(id)`), and
   `query.rs` gains `Filter::{NeverPlayed, PlayCountRange, PlayedSince}`,
   `SortColumn::LastPlayed` and `TrackRow::last_played` (moved from `Track`).
   Commands in [src-tauri/src/history.rs](../../src-tauri/src/history.rs):
   `history_scan` reads a mounted stick (serial from the OS) or a folder (a
   backup when it has `manifest.json`, else a stick copy with no serial), with
   HIST1 dates off, and keeps the plan under an id that `history_import` takes,
   like the export wizard's dry run. UI: [HistoryTree.tsx](../../src/components/HistoryTree.tsx)
   under the playlists ("+" opens [HistoryImportDialog.tsx](../../src/components/HistoryImportDialog.tsx));
   an open session replaces the toolbar and table with
   [SessionView.tsx](../../src/components/SessionView.tsx) (fields save on
   blur or Enter; [LocateDialog.tsx](../../src/components/LocateDialog.tsx)
   searches the library from the stick's artist and title);
   [PlayedIn.tsx](../../src/components/PlayedIn.tsx) on the track panel;
   "Plays" and "Last played" columns (hidden by default) and filters (play
   count range; never played or played in the last 30/90/365 days). Every edit
   that can move counts reloads the table and "Played in"
   ([store/history.ts](../../src/store/history.ts)). Fixed on the way: the
   plan's `SessionAction` JSON repeated the `kind` key for skips and used
   snake_case fields; a skip is now `{"kind": "skip", "reason":
   "alreadyImported", "sessionId": 3}` (also in `reks history scan --json`).
   The mock backend has sessions and a stick to import
   ([mockHistory.ts](../../src/dev/mockHistory.ts)).
10. [x] **`reks-cli history scan/import`** for headless runs.
    Done in [history.rs](../../crates/reks-cli/src/history.rs):
    `reks history scan <stick|backup> --library <db> [--hist1-dates] [--json]`
    prints the dry run (per session: action, fingerprint prefix, entries by
    match kind, unmatched, dividers, counted or practice);
    `reks history import <same> [--practice|--include <session>]... [--yes]`
    applies the choices and imports only with `--yes`. A folder with
    `manifest.json` is read as a backup. `<session>` is the stick name or
    a fingerprint prefix (8+ hex digits) and must name one new session.
    Tests: a fake stick imported once (stick bytes unchanged, second run all
    skips), selector errors, and a backup whose import makes the stick's
    sessions skips.
11. **Hardware session**: checklist below on CDJ-2000NXS2 and CDJ-3000; log in
    `docs/hw-test-log.md`; turn `export.dividers` on by default only after both pass.
12. [x] **Adopt a foreign stick** (D27, [`../stick-profiles.md`](../stick-profiles.md) §4.3):
    `adopt::scan` / `adopt::apply`, read-only on the stick; history import
    (tasks 5–6) first; Link / Create under `From stick/<name>/` / Skip per
    playlist; UI dialog. First check open item S4 (rekordcrate on a
    rekordbox 7 fixture). Tests per stick-profiles §8 (golden `AdoptPlan`,
    stick bytes unchanged, Link only on exact ordered match).
    Done in [`reks_export_rb::adopt`](../../crates/reks-export-rb/src/adopt/mod.rs)
    (`scan(root, serial, lib)`, `apply`) and
    [`reks_core::adopt`](../../crates/reks-core/src/adopt.rs)
    (`Library::adopt_playlists`, one transaction; `playlist_track_ids`);
    stick tracks go through the history matcher, now public as
    `history::match_entries`, and the stick's artist and duration come from
    `history::read_stick_tracks`. S4: no rekordbox 7 fixture exists, so
    rekordcrate was checked on its own real exports (no findings; details in
    stick-profiles §9); rekordbox 7 stays UNVERIFIED. Link is offered only
    when every stick track matched and a same-named library playlist holds
    exactly those tracks in order; apply refuses any other Link, an unknown
    stick playlist and a profile with another volume ID. Created playlists
    keep the stick's folders under `From stick/<profile name>/` and are
    ticked in the profile with the linked ones. App: `adopt_scan` /
    `adopt_apply` ([adopt.rs](../../src-tauri/src/adopt.rs)),
    [AdoptDialog.tsx](../../src/components/AdoptDialog.tsx), opened by
    "Adopt…" on the Foreign prompt (saves the profile first) or "Adopt
    playlists…" on a plugged-in stick's status. CLI: `reks adopt scan`
    ([adopt.rs](../../crates/reks-cli/src/adopt.rs)). Tests: `adopt::tests`
    (golden plan on a synthetic rekordbox-layout stick, stick snapshot equal
    after scan and apply, Link refusals, history first and skipped on rescan)
    plus `reference_stick_fixture_golden_plan`, `#[ignore]` until
    `fixtures/rekordbox/ref-<ver>/` exists; `reks_core::adopt::tests`;
    [AdoptDialog.test.tsx](../../src/components/AdoptDialog.test.tsx) and
    the Foreign case in DevicesDialog.test.tsx.

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
   Run `reks history scan`: note what was logged (HIST2), whether the second
   mount appended or made a new session (HIST3), and that the divider is marked
   as a divider and not counted.
8. Import into reks, then re-export: the guard finds nothing un-imported and
   the export proceeds.
9. Record player, firmware, date and results in `docs/hw-test-log.md`.

## Open items (resolve by fixture or hardware, then log)

- DIV1 Minimum safe divider length: try 1 s against 4 s.
- DIV2 Visible title length per player.
- DIV3 Whether two PDB rows can share one file; how players treat baken's
  generated analysis of a silent file with no TEMPO (software: it generates
  and reads back cleanly, task 4). The folder for empty artist/album is ours:
  `Unknown Artist/Unknown Album`.
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
- S4 (stick-profiles §9), software part: rekordcrate reads its own real
  exports' playlist trees cleanly; a rekordbox 7 reference stick is still
  needed (`fixtures/rekordbox/ref-<ver>/`).

## Risks

- **Dividers look like tracks everywhere but the playlist view** (All Tracks,
  search, player history). Mitigation: accept and document; empty artist;
  hardware notes.
- **A divider the player can't load** would break a set. Mitigation: a real,
  valid WAV through `audiocheck` and the validator; flag off until both players pass.
- **Wrong matches inflate counts.** Mitigation: exact manifest matches for reks
  sticks; fallbacks need a unique match; unmatched entries never count.
- **Double counting with rekordbox.** Mitigation: frozen baseline; the user
  marks a session practice when rekordbox already counted it (no automatic
  default since 2026-10-20).
- **History format unknowns** (HIST1, HIST3). Mitigation: dates optional; prefix
  rule for appended sessions; fixtures from the maintainer's played sticks.
