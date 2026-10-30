# Phase 4 — OneLibrary export (`exportLibrary.db`)

Status: in progress (started 2026-10-20 ahead of the Phase 3 close, with the
maintainer's approval: Phase 3's ANLZ builder and export integration are in
main; its accuracy tuning (12a) and hardware checks continue in parallel; no
players available, so player behaviour stays UNVERIFIED). Tasks 1–8 and 10
done (2026-10-20), 11 (history import) 2026-10-21; 9 (hardware) open. Depends on:
Phase 2 (legacy export, ANLZ layout, planner) and Phase 3 (own analysis, so
every exported track has ANLZ files). Read `docs/decisions.md` §OneLibrary
and `docs/research.md` §2 first.

## Goal

A stick written by reks loads on OneLibrary-only players (CDJ-3000X, CDJ-1500X,
OPUS-QUAD, OMNIS-DUO, XDJ-AZ, XDJ-AN) in addition to the legacy players, by
writing `PIONEER/rekordbox/exportLibrary.db` alongside the Phase 2 output. The
two libraries on the stick must describe the same tracks, playlists and ANLZ
files, mirroring what rekordbox 7.2.11+ does on every export.

## What we know (summary; sources in research.md)

- OneLibrary = "Device Library Plus", renamed in rekordbox 7.2.5. Official
  guidance: rekordbox writes **both** libraries; a stick needs both to work on
  every current player.
- The file is a SQLite database encrypted with SQLCipher. The passphrase is
  static across all sticks and has been published by several community
  projects. Verified 2026-10-20 on rekordbox 7.2.19's export (task 2): stock
  SQLCipher 4 with the key as a passphrase opens it (decisions.md D14).
  rekordbox leaves it in WAL mode with most rows in `exportLibrary.db-wal`.
- Schema: 22 tables (`property`, `content`, `artist`, `album`, `genre`,
  `label`, `key`, `color`, `image`, `playlist`, `playlist_content`, `cue`,
  `history`, `history_content`, `hotCueBankList`, `hotCueBankList_cue`,
  `myTag`, `myTag_content`, `menuItem`, `category`, `sort`, `recommendedLike`)
  plus 4 indexes. rekordbox exports `cue` empty; grids, waveforms and cues
  stay in ANLZ. `content.analysisDataFilePath` must match the ANLZ path.
- Seed rows (`menuItem`, `category`, `sort`) come from the reference;
  `property.dbVersion` is the string `"1000"` on 7.2.19. `color` holds the
  same 8 rows as `export.pdb`'s colour table.
- **No open-source writer has a published success on a real OneLibrary player**
  as of 2026-10-18 (baken: "No OneLibrary player has read the --onelibrary
  output yet"). AlphaTheta is "reviewing the specifications" and has withdrawn
  its own firmware (CDJ-3000 3.30) and rekordbox (7.2.12) over library bugs.

## Scope

- `reks-export-onelibrary` crate: schema DDL per observed rekordbox version,
  seed data, writer from the Phase 2 export plan, SQLCipher encryption, a
  reader for validation, and a "compare to reference export" tool.
- Integration into the export planner: step "also write OneLibrary", on by
  default since 2026-10-30 (D14 amendment) and labelled UNVERIFIED.
- Key policy implementation (below).
- Hardware test campaign on at least one OneLibrary-only player.

## Non-goals

- Replacing the legacy export: never. Both are always written when enabled.
- `history`, `hotCueBankList*`, `recommendedLike`, `myTag*` content: written
  as empty tables with seed rows only. Player history already on the stick is
  imported or backed up first (task 11, D23).
- Conversion of an existing rekordbox-written stick: we rewrite from our plan.
- Reading `master.db`.

## Key policy (decision; see decisions.md)

- The passphrase is **not committed to this repository**. Release builds
  have it built in (D14, amended 2026-10-21): the release workflow passes the
  `REKS_ONELIBRARY_KEY` repository secret to the compiler as
  `REKS_BUILD_ONELIBRARY_KEY`, and
  [`reks_core::secret::builtin`](../../crates/reks-core/src/secret/builtin.rs)
  stores it masked, so it is not a plain string in the binary. A build from
  source without that variable has no built-in key.
- The writer takes the key at runtime from `REKS_ONELIBRARY_KEY` (CI,
  testing, builds from source), else the built-in key. Users never enter a
  key: there is no key field in Settings (removed 2026-10-21).
- The release workflow refuses to build without the secret, checks that the
  built-in key opens the reference `exportLibrary.db`
  (`builtin_key_opens_the_reference`), and checks that no app binary contains
  the plain key.
- The export wizard marks the option experimental and unsupported by
  AlphaTheta, and says when a build has no key.
- Note: the Phase 2 dependency `baken-export` contains its own obfuscated copy
  of the key in its `onelibrary` module. We do not call that module and we do
  not enable `Options::onelibrary`. Revisit if the maintainers object.

## Data structures and interfaces

As built (tasks 2–7, 10); code in
[`reks-export-onelibrary`](../../crates/reks-export-onelibrary/src/lib.rs):

- `OneLibraryWriter { version: RbVersion }` implements the planner's
  `OneLibraryExport` (task 8): `plan` estimates the file, lists the auxiliary
  files and reads the existing database's history sessions with the key;
  `write(plan, key, device_root)` builds and replaces the file (task 6).
- `build::from_pdb(&PdbLibrary, created_date) -> OneLibrary`: pure mapping
  from the `export.pdb` baken just wrote (read by
  [`reks_export_rb::pdb`](../../crates/reks-export-rb/src/pdb/library.rs))
  to typed rows (`rows.rs`); `store` inserts and reads them.
- `schema::RbVersion` (`V7_2_19`): DDL and seed SQL in
  `crates/reks-export-onelibrary/data/<version>/`, `create_empty`.
- `OneLibraryReader::open(path, key)`: opens a temp copy of the database and
  its `-wal`/`-shm` (never the stick's file); `integrity_check`, `library`,
  `dump`, `history_counts`.
- `validate::validate(device_root, key)`, and `diff::diff(ours, theirs,
  DiffOptions)` over `dump::Dump` (the fixtures' `dump.json` format).
- `history` (task 11): `OneLibraryReader::play_history() ->
  Vec<SourceSession>`, `read_play_history(&HistorySource, key, serial)`
  (stick or Phase 2 backup) and `read_both` (`export.pdb` and
  `exportLibrary.db` for one import, with a `OneLibraryStatus`).

Mapping (verified against the reference, where rekordbox's `content` values
equal its `export.pdb` track rows): ids are the PDB row ids; `path`,
`fileName`, `analysisDataFilePath` (`.../ANLZ0000.DAT`) are the PDB strings;
`masterDbId` = track row 0x18, `masterContentId` = 0x14, `contentLink` = the
row bitmask (0xC0700), `analysedBits` = 0x56 (41); `isHotCueAutoLoadOn` /
`isKuvoDeliverStatusOn` are the PDB's `"ON"` strings; unset ids are NULL
except `artist_id_lyricist` and `color_id` (0); dates are `YYYY-MM-DD`
(`dateCreated` = `dateAdded`); `titleForSearch`/`nameForSearch` NULL.
Playlists keep the PDB tree: `sequenceNo` = sort order, `attribute` 0 for a
playlist, 1 for a folder (UNVERIFIED: the reference has no folder),
`playlist_content.sequenceNo` 1-based. `property`: `deviceName` empty,
`dbVersion` `"1000"`, `createdDate` the export date, `myTagMasterDBID` 0.
`myTag`: the four default columns (Genre, Components, Situation, Untitled
Column, from baken's reference); the maintainer's reference holds their
renamed columns (and held their tags, removed from the fixture). Albums, genres, labels and artwork are
empty in the reference, so their rows are UNVERIFIED (mapped like baken).

## Tasks

1. [x] **Reference capture**: on the maintainer's machine, export a 5-track test
   playlist from rekordbox 7.2.x to a fresh stick; copy `exportLibrary.db`,
   `export.pdb`, `exportExt.pdb`, ANLZ into `fixtures/onelibrary/ref-<ver>/`
   (audio excluded). Record rekordbox version and date. Done 2026-10-20:
   rekordbox 7.2.19.0342, the 7 `fixtures/audio` files in one playlist on a
   freshly erased FAT32 stick. The maintainer's My Tag tags were removed and
   `exportExt.pdb` left out; see `fixtures/README.md`.
2. [x] **Open the reference**: using `rusqlite` with `bundled-sqlcipher`, try the
   stock SQLCipher 4 profile with the user-supplied key; if it fails, try the
   parameter variants reported in research.md; record what works in
   `docs/decisions.md`. Dump schema DDL and all rows (metadata only) to
   fixtures. This task resolves the UNVERIFIED cipher question. Done: stock
   SQLCipher 4 opens it (D14); `reks onelibrary dump` wrote
   `fixtures/onelibrary/ref-7.2.19/dump.json`. SQLCipher replaces the bundled
   SQLite workspace-wide; the library DB is unaffected (D14). An ignored test
   (`REKS_ONELIBRARY_KEY`) checks the dump against the encrypted file.
3. [x] **Schema + seed module**: embed DDL and seed JSON per rekordbox version;
   `create_empty(version)` test that produces a DB whose schema diff against
   the reference is empty (`sqlite3 .schema` comparison). Done: SQL files
   instead of JSON (`data/7.2.19/schema.sql`, `seed.sql`, generated from the
   dump); `tests/reference.rs` checks schema and seed rows equal the reference.
4. [x] **Content/lookup writer**: fill `content` and lookups from an `ExportPlan`;
   unit tests comparing a 5-track plan to the reference rows field by field
   (ignoring ids and timestamps), listing every differing column. Done:
   rows come from the stick's `export.pdb` (the plan carries no metadata;
   baken builds the PDB from it), so both libraries agree by construction.
   Built from the reference's own `export.pdb`, every row equals
   rekordbox's, ids and dates included, except the `myTag` defaults
   (strict diff in `tests/reference.rs`).
5. [x] **Playlists writer**: tree + ordering; compare to reference. Done in
   the same mapping and test; folders UNVERIFIED (none in the reference).
6. [x] **Encryption + atomic write**: build in temp, `PRAGMA integrity_check`,
   encrypt (or build encrypted from the start), fsync, rename into place;
   remove rekordbox's auxiliary OneLibrary files if present (list them from
   the reference stick; baken's `ONELIBRARY_FILES` constant is a hint).
   The write is `OneLibraryExport::write(plan, key, device_root)`
   ([onelibrary.rs](../../crates/reks-export-rb/src/onelibrary.rs)); `apply`
   calls it and already removes the auxiliary files first (see task 8). The
   writer touches only `exportLibrary.db` and a temp file beside it, and
   leaves no macOS `._` file (baken #192, #196). Done
   ([writer.rs](../../crates/reks-export-onelibrary/src/writer.rs)): built
   encrypted in a local temp dir in WAL mode (as rekordbox's header says),
   checkpointed to one file, reopened for `integrity_check` and a row-by-row
   read-back, then written with a plain `write` (no `fs::copy`, so no `._`
   file), fsynced and renamed over `exportLibrary.db` beside `export.pdb`.
   The reference stick's auxiliary files are `-wal`, `-shm` and
   `exportExt.pdb`, baken's `ONELIBRARY_FILES`. The app uses this writer;
   the `onelibrary-stub` feature is gone (`NoWriter` stays for tests).
   UNVERIFIED on players: a checkpointed file without `-wal` (rekordbox
   leaves one).
7. [x] **Reader + validator**: open what we wrote, cross-check counts and paths
   against `export.pdb` (parsed by rekordcrate) and ANLZ file existence.
   Done: `validate` (counts, paths, `analysisDataFilePath` vs PDB and on
   disk, `property.numberOfContents`, playlist orphans); `reks onelibrary
   validate <stick>`. rekordbox's own reference stick validates clean.
8. [x] **Planner integration + UI**: experimental toggle, key entry in Settings
   (removed 2026-10-21: release builds have the key built in), warnings, dry-run shows the extra file. Done: the seam is
   [`onelibrary.rs`](../../crates/reks-export-rb/src/onelibrary.rs) in
   reks-export-rb: the trait `OneLibraryExport::plan(&ExportPlan,
   &OneLibraryKey) -> OneLibraryPlan` (planned write, auxiliary files
   removed, existing database's history, warnings/problems). The writer
   crate depends on reks-export-rb, so it implements the trait and the app
   picks the implementation (`OneLibraryWriter`, tasks 4–6).
   `ExportOptions::onelibrary` (on by default since 2026-10-30, labelled
   UNVERIFIED) is read by
   [`plan_onelibrary`](../../crates/reks-export-rb/src/plan/onelibrary.rs),
   which every export runs after the export guard. With the option and a key
   it adds an experimental warning and lists `exportLibrary.db` plus the
   auxiliary files (baken's `ONELIBRARY_FILES` minus the database). Without
   a key it warns and writes the Device Library only; with the option off or
   no key, the stick's OneLibrary files are deletes and an `exportLibrary.db`
   there needs `onelibrary_removal_confirmed` (review F02). Sessions in an
   existing `exportLibrary.db` go through the export guard whenever a key
   opens it (review F08)
   (`OneLibraryHistoryNotImported` until a choice; Import and Back up keep the
   file in the backup). A database it cannot read keeps the Phase 2 backup.
   The key (`reks_core::OneLibraryKey`: redacted `Debug`, no `Serialize`,
   zeroed on drop) comes from `resolve_key` (key policy above); the UI only
   learns `onelibraryKeyBuiltin`.
   Apply side (2026-10-20): when the plan writes `exportLibrary.db`, `apply`
   takes the writer and key in `ApplyEnv::onelibrary` (`OneLibraryEnv`, never
   in the plan) and refuses without them (`NoOneLibraryWriter`); a plan that
   skipped the step is refused too (`NoOneLibraryPlan`). After the
   baken write (which removes rekordbox's OneLibrary files once the new
   `export.pdb` is in place), the `oneLibrary` stage removes what is left of
   the plan's auxiliary files and calls `OneLibraryExport::write`, before the
   fsync and the read-back. It runs only after a clean write: with failed
   tracks or a writer error `exportLibrary.db` would not match `export.pdb`,
   so it is skipped. A skip or a failure is an error in the report (status
   Problems, no manifest); `ExportReport::onelibrary` and the report's
   "OneLibrary (experimental)" line say what was written. The app resolves
   the key at export time and passes `OneLibraryWriter` to the dry run and
   to `apply`. End to end:
   [tests/apply.rs](../../crates/reks-export-onelibrary/tests/apply.rs)
   plans and applies with the real writer on a temp-dir stick that holds
   rekordbox's OneLibrary files, then reads the database back with the key
   and validates it against `export.pdb` (no player has read it: task 9).
9. **Hardware test**: checklist below on a OneLibrary-only player (borrow or
   rent an OPUS-QUAD/XDJ-AZ/CDJ-3000X). Log results in `docs/hw-test-log.md` ([#24](https://github.com/ninoman/reks/issues/24)).
10. [x] **Diff tool**: `reks-cli onelibrary diff <ref.db> <ours.db>` to speed up
    iteration after hardware failures. Done: `reks onelibrary diff <ours>
    <theirs> [--strict]` (databases or `dump.json`; ids, master ids and dates
    ignored unless `--strict`) and `reks onelibrary dump`.
11. [x] **History (H2, D23)**: read `history` / `history_content` from an existing
    `exportLibrary.db` (key supplied at runtime, as above) into Phase 2.5
    sessions with `source = 'onelibrary'`, from a stick or a Phase 2 backup.
    A stick can hold different histories in each library, so both are
    imported. The Phase 2.5 export guard covers `exportLibrary.db` before
    task 6 replaces it. Columns from the task 2 reference (UNVERIFIED until
    then). Test: a reference DB with history imports with entries in order;
    re-import adds nothing.
    Done (2026-10-21) in
    [history.rs](../../crates/reks-export-onelibrary/src/history.rs). The
    reference's columns are `history(history_id, sequenceNo, name,
    attribute, history_id_parent)` and `history_content(history_id,
    content_id, sequenceNo)`; its history is empty, so how players fill
    them is **UNVERIFIED** and the reader assumes the `playlist` layout:
    entries in `sequenceNo` order joined to `content` (path, title,
    `length` seconds, `artist_id_artist`); a row without entries that is a
    folder (`attribute` 1 or a parent) is skipped, any row with entries is a
    session; sessions in `history_id` order; no date column, so all are
    undated. Model: each `SourceSession`/`PlannedSession` carries a
    `HistoryLibrary` (`deviceLibrary` | `oneLibrary`), stored as
    `play_session.source` and in the fingerprint, so one plan holds both
    libraries and each library's session extends only its own (legacy
    fingerprints unchanged); `SessionRow.library` reads it back. A backup
    is read only when `exportLibrary.db` and its `-wal`/`-shm` hash as its
    manifest records. Export guard: the writer's `plan` returns the
    sessions in `ExistingLibrary::Read`; `plan_onelibrary` (now given the
    library) scans them into `ExportPlan::onelibrary_history_import`, and
    only sessions not imported fill `HistoryGuard::onelibrary_sessions` and
    block. Import now imports both libraries' scans in one transaction
    before the backup; the backup (`exportLibrary.db`, `-wal`, `-shm`) runs
    before baken removes the file and the writer replaces it. Dry run:
    OneLibrary sessions are listed as `HISTORY 001 (OneLibrary): 3 plays`.
    App: History → import reads both libraries with the stored key (the
    dialog badges OneLibrary sessions and says when the database was not
    read). CLI: `reks history scan|import` reads both (`--practice
    "HISTORY 001 (OneLibrary)"`). Tests: `history::tests` (reference rows
    plus history left in `-wal`, read in order, import and re-import adds
    nothing, backup hash checks), `plan::onelibrary::tests` (both libraries
    through Import now), [tests/apply.rs](../../crates/reks-export-onelibrary/tests/apply.rs)
    (a player's session in a reks-written database: blocked, imported in
    play order through the manifest, backed up, then a backup re-import
    skips), `reks_core::history::tests::each_stick_library_imports_its_own_sessions`
    and the CLI. Open (UNVERIFIED): whether a player that reads both
    libraries logs the same plays in both (they would import as two
    sessions; the dry run says to mark one practice).

## Acceptance criteria

- `exportLibrary.db` written by reks opens in pyrekordbox 0.5+ and in our
  reader; schema diff vs reference is empty; row-level diff for a 5-track
  playlist shows only ids/timestamps/uuids.
- A stick with both libraries loads on a legacy player (unchanged Phase 2
  behaviour) **and** on at least one OneLibrary-only player: tracks browse,
  playlists in order, track loads with waveform, grid, hot cues and memory
  cues, key and BPM shown.
- Plugging the stick into rekordbox 7 shows both "Device Library" and
  "OneLibrary" nodes with matching contents and no "out of matching" caution.
- Feature on by default (D14, 2026-10-30) and labelled UNVERIFIED until this
  passes; release notes state it is experimental.

## Tests

- Cipher open/close round-trip with a known key on a tiny DB.
- Schema equality test against each captured reference version.
- Field-by-field row comparison tests (content, playlists, lookups).
- Path string tests: separators, leading slash, NFC, 48-char file name limit
  parity with PDB rows.
- Validator catches: ANLZ path mismatch, content count ≠ property count,
  playlist orphan.

## Manual CDJ test checklist (OneLibrary-only player)

1. Fresh FAT32 (MBR) stick, exported with "OneLibrary (experimental)" on.
2. Insert: device recognised, no error code; note mount time.
3. Browse: Playlists tree order and names; Artists/Albums/Genres/Keys menus
   populated; Search finds a track by title and by artist (non-ASCII too).
4. Load 3 tracks (MP3, FLAC, WAV): title/artist/key/BPM display, waveform
   (colour/3-band as the player supports), beatgrid aligned, hot cues A–H with
   colours, memory cues, active loop.
5. Set a hot cue on the player, re-insert into a computer: confirm the player
   wrote its own files and did not corrupt ours (rekordbox then shows both
   libraries without the "out of matching" caution).
6. Repeat on a legacy player with the same stick (regression).
7. Play 2 tracks on the OneLibrary player; on the computer
   `reks-cli history scan` lists them from `exportLibrary.db`; a re-export
   asks to import first.
8. Record firmware versions and results.

## Risks

- **Hardware rejection with no diagnostics**. Mitigation: start from a byte-
  level diff against a reference export; change one thing at a time; keep the
  diff tool.
- **Cipher parameter uncertainty**. Resolved for 7.2.19 (task 2); a new
  rekordbox version needs a new reference before `RbVersion` gains it.
- **Format churn** (AlphaTheta "reviewing specifications", CDJ-3000X firmware
  "more flexible library loading" promised). Mitigation: version-tagged
  schema/seed fixtures; refuse to write for an unknown reference version.
- **Legal**: shipping a writer for an encrypted vendor format. Mitigation: key
  policy above; no key in repo; experimental flag; be ready to remove.
- **Two libraries drifting**: a user edits the stick on a legacy player
  (history/hot cues written by the player) and the two libraries diverge.
  Mitigation: reks always regenerates both from the plan; history is
  imported first (task 11); document that other on-player edits are not
  merged back.
