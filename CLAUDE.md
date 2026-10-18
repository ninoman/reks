# reks — brief for coding sessions

Read this file, then `docs/decisions.md`, then the phase doc you are working on
(`docs/phases/phase-N.md`) **before writing any code**. `docs/research.md` holds
the evidence behind the decisions; consult it when a decision looks wrong.

## What this is

A free, open-source (GPL-3.0-or-later, donation-funded) desktop DJ library app.
A standalone rekordbox alternative: own library, own audio analysis, USB export
that plays on Pioneer DJ / AlphaTheta CDJs and XDJs (legacy `export.pdb` + ANLZ
files; later the newer OneLibrary `exportLibrary.db`), and later Denon Engine DJ.

First user: the maintainer, ~3,000 tracks currently in rekordbox, imported via
`collection.xml`. Exports are tested on real CDJs. A stick that fails on stage
is the worst possible bug; correctness and conservative defaults beat features.

## Stack (fixed; see docs/decisions.md for why)

- Tauri 2 (stable 2.x). Rust core owns all state, file I/O, audio and exports.
- UI: React + TypeScript + Vite. The UI never touches the filesystem directly;
  it calls Rust commands.
- Library: SQLite via `rusqlite` (bundled) + `rusqlite_migration`. WAL mode.
- Decoding: Symphonia 0.6 (features: mp3, aac, alac, isomp4, aiff, flac, wav).
- Playback: Rust-side (rodio/cpal), not the WebView `<audio>` element.
- rekordbox legacy export: `baken-export` (MIT) behind our own adapter trait;
  `rekordcrate` (MPL-2.0) for read-back validation. Spec of record: Deep
  Symmetry `crate-digger` `.ksy` + djl-analysis docs.
- Engine DJ export: own Rust writer (Phase 5). OneLibrary: own writer (Phase 4).

## Licence rules (hard)

- The project is GPL-3.0-or-later. Every dependency (Rust and npm, including
  transitive) must be GPLv3-compatible: MIT, Apache-2.0, BSD-2/3, ISC, Zlib,
  MPL-2.0, LGPL-2.1+/3.0, GPL-2.0-or-later, GPL-3.0, CC0, BSL-1.0, Unicode.
- Not allowed: GPL-2.0-only, EPL-1.0, CDDL, SSPL, BUSL, proprietary, "no licence".
  AGPL-3.0 is technically combinable but would re-licence the combined work;
  do not add AGPL code without a maintainer decision recorded in decisions.md.
- `cargo deny check licenses` must pass (see `deny.toml`). Add a new licence to
  the allowlist only with a note in `docs/decisions.md`.
- Never copy code from unlicensed repos (e.g. `rex`, `djbooth`) even as "reference".
  Reading format documentation is fine; copying code is not.
- Do not commit any decryption key or passphrase for vendor databases. See
  Phase 4 for the OneLibrary key policy.

## Repo structure (target)

```
reks/
  CLAUDE.md  README.md  LICENSE  deny.toml
  docs/           research.md, decisions.md, phases/phase-N.md
  crates/
    reks-core/      library model, SQLite schema + migrations, scan/import, tags,
                    artwork, playlists, rekordbox XML import/export, services
    reks-audio/     Symphonia decode, PCM cache, preview player
    reks-analysis/  BPM/beatgrid, key, waveform generation (Phase 3)
    reks-anlz/      Pioneer ANLZ model + writer/reader helpers (Phase 2/3)
    reks-export-rb/ legacy rekordbox USB export (plan/dry-run/write/validate)
    reks-export-onelibrary/   Phase 4
    reks-export-engine/       Phase 5
    reks-cli/       thin CLI over the crates (headless testing, CI fixtures)
  src-tauri/      Tauri app: commands, state, events; no business logic here
  src/            React UI
  fixtures/       small synthetic audio, sample XML, sanitised rekordbox exports
```

Business logic lives in `crates/*`, never in `src-tauri/` commands or the UI.
Every crate must build and test without Tauri.

## Coding conventions

Rust
- Edition 2021, stable toolchain pinned in `rust-toolchain.toml`. `cargo fmt`
  and `cargo clippy --all-targets -- -D warnings` must pass.
- Library crates: `thiserror` error enums, no `unwrap()`/`expect()` outside
  tests, no `panic!` on user data. App/CLI: `anyhow` is fine.
- `tracing` for logs. No `println!` in library code.
- Binary formats: use `binrw` or hand-written little-endian writers with
  explicit offsets; every struct that goes to disk has a round-trip test.
- All USB writes go through the export planner: `plan()` produces a pure data
  structure (files to copy/write/delete) that is shown as a dry run; `apply()`
  executes it. Never write to a removable volume outside `apply()`.
- Paths on exported media: ASCII-safe, NFC-normalised, FAT32-legal, `/`
  separators in ANLZ/PDB path strings. Helpers live in `reks-core::fsname`.
- Timestamps in the DB are UTC ISO-8601 strings. Durations/positions are
  stored in milliseconds (i64) or samples (i64), never floats of seconds.

TypeScript / React
- Strict TS, ESLint + Prettier. Functional components, hooks, Zustand store.
- Tauri calls go through `src/lib/ipc.ts` typed wrappers only. Large binary
  payloads (waveforms) come back as `ArrayBuffer` via `tauri::ipc::Response`,
  never as JSON arrays.
- Keep the UI dumb: list, filter, select, invoke. No format logic in TS.

Git
- Conventional Commits (`feat(core): ...`, `fix(export-rb): ...`).
- Small PRs; one phase task per PR where possible. Update the phase doc's task
  checklist in the same PR.

## Testing rules

- `cargo test --workspace` and `npm test` must pass before a task is done.
- Each binary writer has (a) unit tests for field encoding, (b) a round-trip
  test through an independent parser (`rekordcrate` for PDB/ANLZ; our own
  reader for Engine), (c) a golden-file comparison against a real rekordbox /
  Engine export where one exists in `fixtures/` (sanitised: metadata only, no
  copyrighted audio).
- Audio fixtures are generated (sine/click tracks at known BPM/key) by
  `fixtures/gen/`; never commit commercial audio.
- Export code is tested with a dry-run on a temp directory that mimics a stick
  (FAT32 rules enforced by our validator), not on a real device.
- Real-hardware checks follow the manual checklist in the phase doc. Record
  results (player model, firmware, date, outcome) in `docs/hw-test-log.md`.
- Anything that cannot be verified against a current source or a real player is
  marked `UNVERIFIED` in docs and guarded by a feature flag in code.

## Documentation rules

- The project keeps proper, current documentation in `docs/`. It is part of the
  deliverable, not an afterthought.
- Docs are short and efficient: say what a reader needs, link to code and
  `docs/research.md` instead of repeating them, no filler or duplicated content.
- Layout: `docs/decisions.md` (why), `docs/phases/phase-N.md` (what/when, task
  checklists), `docs/research.md` + `docs/research/` (evidence), `docs/hw-test-log.md`
  (hardware results). Add new topic docs (architecture, formats, user guide) under
  `docs/` and link them from `README.md`.
- Any change that affects behaviour, architecture, file formats or a decision
  updates the relevant doc in the same PR. Stale docs are bugs.

## Working style for sessions

- Start by reading `docs/decisions.md` and the current phase doc. Pick the
  next unchecked task. Do not skip ahead across phases.
- If a task reveals that a decision is wrong, stop, write the finding into
  `docs/decisions.md` (status: proposed), and ask before changing direction.
- Keep `docs/research.md` sources intact; add new sources with dates.
- Never claim CDJ compatibility in docs or UI strings without a logged
  hardware test.
