# Stick profiles and adopting foreign sticks

Status: proposed (2026-10-19). Owner: maintainer. Decision: D27 (proposed).
Placement: profiles in Phase 2 task 15 (replaces task 10's "persist last
selection per device label"); adopt in Phase 2.5 task 12. Read
[`decisions.md`](decisions.md) D7, D23, D27 and
[`phases/phase-2.md`](phases/phase-2.md) first.

## 1. Problem

In rekordbox it is hard to see and control which playlists are on a stick,
and to drop old ones while adding new ones. DJs end up reformatting sticks or
carrying stale playlists.

## 2. Goals

1. Each stick has a saved, visible list of playlists that should be on it.
2. Ticking or unticking a playlist and pressing Sync makes the stick match,
   with a dry run first. Only new audio is copied; only orphaned files are
   deleted.
3. A stick written by rekordbox (or another tool) can be adopted: its
   playlists become a reks profile instead of being silently replaced.
4. Nothing here weakens export correctness or the D23 history safeguard.

## 3. Non-goals (v1)

- Editing a stick's DB in place (baken rebuilds `export.pdb` every run, D7).
- Frozen/snapshot entries in a profile. Use "Duplicate playlist" in the library.
- Rule-based selection ("changed in the last 30 days"), auto-sync on mount.
- Profile clone to a backup stick (later, S).
- Writing any reks file onto the stick.

## 4. Behaviour

### 4.1 Profiles (Phase 2)

- Devices panel lists known sticks (mounted ones first) by user-given name.
- A profile entry is a playlist or a folder (folder = all children, live).
  Entries are live: library edits flow into the next sync.
- Status per playlist, computed in Rust:
  `InSync | Changed { added, removed, reordered, moved_from } | New | WillRemove`,
  plus "frees X MB" per ticked entry (playlist or folder) for unticking it:
  the audio of its tracks that no other ticked entry holds. A stick playlist
  pairs with the library playlist it was written from, else with one at the
  same path. A track the export leaves out (unsupported format, skipped)
  shows as added until it is fixed or removed from the playlist.
- Capacity bar: planned bytes vs stick size; shared tracks count once.
- Sync = Phase 2 pipeline unchanged: plan → dry run → D23 backup → write →
  validate → manifest → record `device_synced_playlist`.
- Stick tree on the player mirrors the library folder path of each entry.
- A library playlist deleted while in a profile drops from the profile
  (cascade) and shows as `WillRemove` in the next dry run, never silently.

### 4.2 Stick identity

- Key: the OS's volume ID, `device::volume_serial` (D28): `VolumeUUID` on
  macOS, the FAT/exFAT serial on Windows and Linux. Never a file on the stick.
- Unknown serial: if label and size match an existing device, ask
  "Is this '<name>'?" (covers a reformat; several matches: "Which of your
  sticks is this?"). Otherwise offer "New stick", or, when it holds a Device
  Library with no `export_manifest` rows for its serial (`Foreign`), "New
  stick" with a note that its playlists are replaced (Adopt: Phase 2.5).
  The same prompt relinks every stick once after the library moves to
  another OS; on yes the stored ID is replaced (`devices::link`).
- No serial available on this OS: fall back to label + size with an explicit
  user confirm every time; no silent match.

### 4.3 Adopt a foreign stick (Phase 2.5)

1. Mount a stick with `PIONEER/rekordbox/export.pdb` not written by reks
   (no `export_manifest` rows for its serial). reks reads it read-only with
   rekordcrate.
2. History first: if it holds un-imported sessions, run the history import
   (D23) in the same flow.
3. Match each stick track to the library with the history matcher
   ([Phase 2.5](phases/phase-2.5.md) task 6): manifest, then stick path →
   library path suffix, then (artist, title, duration ± 1 s).
4. Dry-run dialog per stick playlist: matched/unmatched counts and a choice:
   - **Link** to an existing library playlist (offered only when name and
     ordered matched tracks are identical), or
   - **Create** a new library playlist under folder `From stick/<name>/`
     (default), or
   - **Skip** (will be removed on first reks sync).
5. On confirm: playlists created/linked, profile saved. The stick is not
   touched. Unmatched tracks are listed: "removed from the stick on first
   reks sync unless you add the files to your library".
6. First sync afterwards shows the usual Phase 2 warning for a
   rekordbox-written stick (rekordbox's "out of matching" caution,
   `exportLibrary.db` removed).

## 5. Data model and API

Rows live in `reks-core` ([`device.rs`](../crates/reks-core/src/device.rs),
migration `0003_device_profiles.sql`); the API is
[`reks_export_rb::devices`](../crates/reks-export-rb/src/devices/mod.rs),
because it needs `DeviceInfo`, `Selection`, the stick read-back and
`ExportReport`, which live in that crate (as the export manifest: stored in
core, filled by the export).

```sql
CREATE TABLE device (
  id             INTEGER PRIMARY KEY,
  volume_serial  TEXT UNIQUE,              -- the OS's volume ID (D28); NULL if none; relink replaces it
  label          TEXT NOT NULL DEFAULT '', -- when last seen
  size_bytes     INTEGER NOT NULL,         -- when last seen
  name           TEXT NOT NULL,            -- user-given
  target         TEXT NOT NULL,            -- TargetProfile ('Cdj3000', ...)
  created_at TEXT NOT NULL, last_synced_at TEXT
);                                          -- + index (label, size_bytes)
CREATE TABLE device_selection (
  device_id   INTEGER NOT NULL REFERENCES device(id) ON DELETE CASCADE,
  playlist_id INTEGER NOT NULL REFERENCES playlist(id) ON DELETE CASCADE,
  PRIMARY KEY (device_id, playlist_id)
);
CREATE TABLE device_synced_playlist (    -- what the last successful sync wrote
  device_id     INTEGER NOT NULL REFERENCES device(id) ON DELETE CASCADE,
  position      INTEGER NOT NULL,        -- stick tree order, depth first
  playlist_id   INTEGER REFERENCES playlist(id) ON DELETE SET NULL,
  path          TEXT NOT NULL,           -- JSON array: folder names + playlist name, as on the stick
  track_count   INTEGER NOT NULL,
  entries       TEXT NOT NULL,           -- JSON array of library track ids in order (null = unknown)
  content_hash  TEXT NOT NULL,           -- blake3 of the ordered entries
  synced_at     TEXT NOT NULL,
  PRIMARY KEY (device_id, position)
);
```

`position` is the key, not `path`: two library playlists with the same name
in the same folder give two stick playlists with the same path. `path` is a
JSON array so a `/` in a name stays unambiguous. `entries` is kept because
the added/removed counts need the old track list, not only its hash.

API (UI renders only; every `*_text`, `prompt` and `note` is worded in Rust):

```rust
pub mod devices {   // reks-export-rb
    pub fn identify(info: &DeviceInfo, lib: &Library) -> Result<DeviceMatch>;   // asks the OS for the serial
    pub fn identify_serial(info: &DeviceInfo, serial: Option<&str>, lib: &Library) -> Result<DeviceMatch>;
    // DeviceMatch: Known { device } | Ambiguous { volume_serial, reason, candidates, prompt, note }
    //            | New { volume_serial, suggested_name, prompt, note } | Foreign { same as New }
    pub fn create(lib, info: &DeviceInfo, serial: Option<&str>, name: &str, target: TargetProfile) -> Result<Device>;
    pub fn link(lib, dev: DeviceId, info: &DeviceInfo, serial: Option<&str>) -> Result<Device>;  // confirm Ambiguous
    pub fn rename(lib, dev, name: &str) -> Result<Device>;
    pub fn set_target(lib, dev, target: TargetProfile) -> Result<Device>;
    pub fn set_selection(lib, dev, sel: &[PlaylistId]) -> Result<()>;
    pub fn selection(lib, dev) -> Result<Selection>;                 // profile → plan input
    pub fn profile_status(lib, dev, mounted: Option<&Path>) -> Result<ProfileStatus>;
    pub fn record_sync(lib, dev, plan: &ExportPlan, report: &ExportReport) -> Result<usize>;
}
pub mod adopt {   // Phase 2.5, read-only on the stick
    pub fn scan(root: &Path, lib: &Library) -> Result<AdoptPlan>;
    pub fn apply(plan: &AdoptPlan, choices: &AdoptChoices, lib: &Library) -> Result<AdoptReport>;
}
```

When the stick is mounted, status reads it (ground truth, `validate::read`,
read-only) and maps its entries to library tracks through `export_manifest`;
playlist ids come from the record by path. Without a volume ID, or when the
stick can't be read, it uses `device_synced_playlist` and says so in
`notes`. `record_sync` stores the export's read-back (so a `Problems` export
records what the stick really holds) with playlist ids from the plan; it
refuses a cancelled export, a report without a read-back and a report from
another volume ID.

## 6. Effort

| Part | Phase | Effort |
|---|---|---|
| Device identity, profile tables, status, Devices panel | 2 | M |
| Adopt (scan, match, create/link playlists, UI) | 2.5 | S–M |

## 7. Acceptance criteria

- Untick 2 of 10 playlists and sync: the dry run lists exactly the files
  unique to them as deletes; after sync the validator reports zero findings
  and the stick's playlist tree equals the profile.
- A second identical sync copies and deletes nothing.
- Deleting a profiled playlist in the library shows `WillRemove` in the next
  dry run.
- Re-inserting a known stick is recognised by serial with no prompt; a
  reformatted stick with the same label and size prompts to link.
- Adopt on a rekordbox-written fixture stick: no byte on the stick changes
  (hash check); matched playlists are recreated in order; unmatched tracks are
  listed; history import runs first when sessions exist.

## 8. Tests

- Status: property test over random selections and library edits
  (added/removed counts, unique-bytes freed).
- Identity: serial hit, label+size prompt, no-serial fallback.
- Planner integration: profile → `Selection` → plan equals the
  hand-built selection plan.
- Adopt: fixture `fixtures/rekordbox/ref-<ver>/` stick → golden `AdoptPlan`
  JSON; stick files byte-identical afterwards; Link only offered on exact
  ordered match.

## 9. Open items

- **S1** Volume serial per OS and after reformat (shared with R§8 item 14).
- **S2** Player limits on playlist count and folder depth (Phase 2 manual CDJ
  checklist; add an item). UNVERIFIED.
- **S3** Does baken keep existing `/Contents` paths so unchanged tracks are
  not re-copied? Already a Phase 2 acceptance criterion; profiles depend on it.
- **S4** Can rekordcrate read a current rekordbox 7 stick's playlist tree
  without errors? Check with the reference fixture before building adopt.
