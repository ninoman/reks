# Phase 4 — OneLibrary export (`exportLibrary.db`)

Status: in progress (task 8 done with a stub writer). Depends on: Phase 2 (legacy export, ANLZ layout, planner)
and Phase 3 (own analysis, so every exported track has ANLZ files). Read
`docs/decisions.md` §OneLibrary and `docs/research.md` §2 first.

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
- The file is a SQLite database encrypted with SQLCipher (v4-style profile).
  The passphrase is static across all sticks and has been published by several
  community projects. The exact KDF/cipher parameters are reported
  inconsistently (stock SQLCipher 4 profile vs. "standard params failed");
  **UNVERIFIED** until we open a real rekordbox 7.2.13+ export ourselves.
- Schema: 22 tables (`property`, `content`, `artist`, `album`, `genre`,
  `label`, `key`, `color`, `image`, `playlist`, `playlist_content`, `cue`,
  `history`, `history_content`, `hotCueBankList`, `hotCueBankList_cue`,
  `myTag`, `myTag_content`, `menuItem`, `category`, `sort`, `recommendedLike`)
  plus 4 indexes. rekordbox exports `cue` empty; grids, waveforms and cues
  stay in ANLZ. `content.analysisDataFilePath` must match the ANLZ path.
- Seed rows (`menuItem`, `category`, `sort`, `color`, default `myTag`) and
  `property.dbVersion` must be copied from a real export; published values
  conflict (`1000` vs `'10000'`).
- **No open-source writer has a published success on a real OneLibrary player**
  as of 2026-10-18 (baken: "No OneLibrary player has read the --onelibrary
  output yet"). AlphaTheta is "reviewing the specifications" and has withdrawn
  its own firmware (CDJ-3000 3.30) and rekordbox (7.2.12) over library bugs.

## Scope

- `reks-export-onelibrary` crate: schema DDL per observed rekordbox version,
  seed data, writer from the Phase 2 export plan, SQLCipher encryption, a
  reader for validation, and a "compare to reference export" tool.
- Integration into the export planner: optional step "also write OneLibrary",
  off by default, labelled Experimental.
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

- The passphrase is **not committed to this repository** and is not compiled
  into release binaries by default. Reason: Mixxx and libdjinterop both chose
  not to ship it; the legal position (DMCA §1201(f) interoperability defence)
  is untested; the project has no legal budget.
- The writer takes the key at runtime from, in order: (1) the `setting` table
  (user pasted it in Settings → Experimental), (2) an environment variable
  `REKS_ONELIBRARY_KEY` (CI/testing), (3) a locally installed rekordbox 7 via
  a documented extraction routine **only if** that routine is itself
  legally reviewed later (placeholder; not implemented in this phase).
- The UI explains where the key comes from and that the feature is
  experimental and unsupported by AlphaTheta.
- Note: the Phase 2 dependency `baken-export` contains its own obfuscated copy
  of the key in its `onelibrary` module. We do not call that module and we do
  not enable `Options::onelibrary`. Revisit if the maintainers object.

## Data structures and interfaces

```rust
pub struct OneLibraryTarget { pub rekordbox_version: RbVersion /* e.g. 7.2.19 */ }

pub struct OneLibraryWriter<'a> { plan: &'a ExportPlan, key: Zeroizing<String>, target: OneLibraryTarget }
impl OneLibraryWriter<'_> {
    /// Builds the DB in a temp file, verifies it, then moves it into place.
    pub fn write(&self, usb_root: &Path, progress: impl Fn(Progress)) -> Result<WriteReport>;
}

pub struct OneLibraryReader { conn: rusqlite::Connection }
impl OneLibraryReader {
    pub fn open(path: &Path, key: &str) -> Result<Self>;           // PRAGMA key, cipher_compatibility=4 (to verify)
    pub fn property(&self) -> Result<Property>;
    pub fn contents(&self) -> Result<Vec<Content>>;
    pub fn playlists(&self) -> Result<Vec<Playlist>>;
    pub fn dump_seed_tables(&self) -> Result<SeedTables>;            // menuItem, category, sort, color, myTag
}

pub struct SeedTables { /* rows captured from a real export; serialised to fixtures/onelibrary/seed-<ver>.json */ }
```

Mapping from the export plan (same source as Phase 2's PDB):
- `content`: one row per planned track. `path` = absolute-from-root
  `/Contents/<Artist>/<Album>/<file>` (confirm separator and leading slash
  against reference), `fileName`, `fileSize`, `fileType` (same codes as PDB),
  `bpmx100`, `length` (s), `key_id` → `key.name` written in the user's notation
  (reference stores "as written"), `analysisDataFilePath` =
  `/PIONEER/USBANLZ/.../ANLZ0000.DAT` (verify whether DAT or folder),
  `analysedBits` (reference shows 41 for fully analysed; UNVERIFIED meaning),
  `contentLink` (reference constant; UNVERIFIED), `masterDbId`/`masterContentId`
  (our library UUID and track id), dates in rekordbox's string format.
- Lookups `artist/album/genre/label/key/color` de-duplicated by name;
  `album.isComplation` spelled as in the reference schema.
- `playlist` with `playlist_id_parent`, `sequenceNo`, `attribute` (folder vs
  playlist code from reference); `playlist_content` ordered by `sequenceNo`.
- `image`: only when Phase 2 artwork is enabled; paths under `PIONEER/Artwork`.
- `property`: `deviceName`, `dbVersion`, `numberOfContents`, `createdDate`,
  `backGroundColorType`, `myTagMasterDBID` copied from reference semantics.

## Tasks

1. **Reference capture**: on the maintainer's machine, export a 5-track test
   playlist from rekordbox 7.2.x to a fresh stick; copy `exportLibrary.db`,
   `export.pdb`, `exportExt.pdb`, ANLZ into `fixtures/onelibrary/ref-<ver>/`
   (audio excluded). Record rekordbox version and date.
2. **Open the reference**: using `rusqlite` with `bundled-sqlcipher`, try the
   stock SQLCipher 4 profile with the user-supplied key; if it fails, try the
   parameter variants reported in research.md; record what works in
   `docs/decisions.md`. Dump schema DDL and all rows (metadata only) to
   fixtures. This task resolves the UNVERIFIED cipher question.
3. **Schema + seed module**: embed DDL and seed JSON per rekordbox version;
   `create_empty(version)` test that produces a DB whose schema diff against
   the reference is empty (`sqlite3 .schema` comparison).
4. **Content/lookup writer**: fill `content` and lookups from an `ExportPlan`;
   unit tests comparing a 5-track plan to the reference rows field by field
   (ignoring ids and timestamps), listing every differing column.
5. **Playlists writer**: tree + ordering; compare to reference.
6. **Encryption + atomic write**: build in temp, `PRAGMA integrity_check`,
   encrypt (or build encrypted from the start), fsync, rename into place;
   remove rekordbox's auxiliary OneLibrary files if present (list them from
   the reference stick; baken's `ONELIBRARY_FILES` constant is a hint).
7. **Reader + validator**: open what we wrote, cross-check counts and paths
   against `export.pdb` (parsed by rekordcrate) and ANLZ file existence.
8. [x] **Planner integration + UI**: experimental toggle, key entry in Settings,
   warnings, dry-run shows the extra file. Done (stub until the writer lands):
   the seam is [`onelibrary.rs`](../../crates/reks-export-rb/src/onelibrary.rs)
   in reks-export-rb: the trait `OneLibraryExport::plan(&ExportPlan,
   &OneLibraryKey) -> OneLibraryPlan` (planned write, auxiliary files
   removed, existing database's history, warnings/problems). The writer
   crate depends on reks-export-rb, so it implements the trait and the app
   picks the implementation; until then the app uses `NoWriter` (feature
   `onelibrary-stub`), which lists the file and blocks with "this build cannot
   write exportLibrary.db". `ExportOptions::onelibrary` (off by default,
   labelled Experimental) runs
   [`plan_onelibrary`](../../crates/reks-export-rb/src/plan/onelibrary.rs)
   after the export guard. It adds an experimental warning, blocks on a
   missing key, and lists `exportLibrary.db` plus the auxiliary files
   (baken's `ONELIBRARY_FILES` minus the database). Sessions the writer reads
   in an existing `exportLibrary.db` go through the export guard
   (`OneLibraryHistoryNotImported` until a choice; Import and Back up keep the
   file in the backup). A database it cannot read keeps the Phase 2 backup.
   The key (`reks_core::OneLibraryKey`: redacted `Debug`, no `Serialize`,
   zeroed on drop) is stored under `onelibrary.key` in the `setting` table
   from Settings → Experimental. The UI only learns `onelibraryKeySet`, and
   `REKS_ONELIBRARY_KEY` is the fallback. Left for the writer's landing:
   `apply` refuses a plan with the option on (`OneLibraryNotWired`) until it
   calls the writer after baken, and src-tauri swaps `NoWriter` for the
   writer.
9. **Hardware test**: checklist below on a OneLibrary-only player (borrow or
   rent an OPUS-QUAD/XDJ-AZ/CDJ-3000X). Log results in `docs/hw-test-log.md`.
10. **Diff tool**: `reks-cli onelibrary diff <ref.db> <ours.db>` to speed up
    iteration after hardware failures.
11. **History (H2, D23)**: read `history` / `history_content` from an existing
    `exportLibrary.db` (key supplied at runtime, as above) into Phase 2.5
    sessions with `source = 'onelibrary'`, from a stick or a Phase 2 backup.
    A stick can hold different histories in each library, so both are
    imported. The Phase 2.5 export guard covers `exportLibrary.db` before
    task 6 replaces it. Columns from the task 2 reference (UNVERIFIED until
    then). Test: a reference DB with history imports with entries in order;
    re-import adds nothing.

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
- Feature off by default; release notes state it is experimental.

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
- **Cipher parameter uncertainty**. Mitigation: task 2 is first and blocks
  everything.
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
