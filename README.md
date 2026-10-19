# reks

A free, open-source desktop DJ library manager and USB exporter. A standalone
alternative to rekordbox for managing your music collection and preparing USB
sticks that play on Pioneer DJ / AlphaTheta CDJs and XDJs, and later on Denon
Engine DJ players.

## What it does (planned)

- Keeps your own library: folder import, tags, artwork, playlists, search.
- Imports an existing rekordbox collection via `collection.xml` (including
  beatgrids, cues, loops, keys and playlists).
- Exports a USB stick in the legacy rekordbox format (`export.pdb` + ANLZ
  analysis files), targeting CDJ-2000 / NXS / NXS2 / CDJ-3000 / XDJ players
  (not yet hardware-tested; see `docs/hw-test-log.md`).
- Analyses tracks itself (BPM, beatgrid, key, waveforms) in Pioneer formats.
- Later: the newer rekordbox "Device Library Plus" (`exportLibrary.db`) format
  and Denon Engine DJ libraries.

## Status

Pre-alpha. Phase 1 in progress: the library core (SQLite schema, folder scan,
rekordbox XML parse/write/import, track queries) is tested, and the app shows the
library in a searchable, filterable track table, filled by scanning a folder or
importing a rekordbox XML, with a playlist tree to create, edit and reorder
playlists.
Progress: [`docs/phases/phase-1.md`](docs/phases/phase-1.md).

## Documentation

| Doc | What it holds |
| --- | --- |
| [`CLAUDE.md`](CLAUDE.md) | Brief for contributors and coding agents: stack, rules, conventions |
| [`docs/decisions.md`](docs/decisions.md) | Architecture and product decisions, with status |
| [`docs/phases/`](docs/phases/) | Phase 1–5 plans with task checklists |
| [`docs/research.md`](docs/research.md) | Evidence and sources behind the decisions |
| [`docs/research/`](docs/research/) | Raw research notes per topic |
| [`docs/hw-test-log.md`](docs/hw-test-log.md) | Real-player test results |
| [`fixtures/README.md`](fixtures/README.md) | Test fixtures: generated audio signal, ffmpeg commands, sample rekordbox XML |

## Building from source

Prerequisites: Rust (see `rust-toolchain.toml`), Node 24, and the Tauri 2
system prerequisites for your OS (https://tauri.app/start/prerequisites/).

```sh
npm ci
cargo install tauri-cli --version '^2' --locked
cargo tauri dev
```

Checks run by CI: `npm run lint`, `npm test`, `npm run build`,
`cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace`, `cargo deny check licenses`, `npm run licenses:check`.

## Stack

Tauri 2 (Rust core, React + TypeScript + Vite UI), SQLite library, Symphonia
for audio decoding.

## Licence

GPL-3.0-or-later. See `LICENSE`. All dependencies must be GPLv3-compatible.

This project is not affiliated with or endorsed by AlphaTheta / Pioneer DJ,
inMusic / Denon DJ, or any other manufacturer. rekordbox, CDJ, XDJ and Engine
DJ are trademarks of their respective owners.

## Funding

Donation-funded. There are no paid tiers and no telemetry.
