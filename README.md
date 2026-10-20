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
- Then: playlist dividers that show on the player, and play history imported
  from played sticks, with practice sessions left out of play counts
  ([`docs/phases/phase-2.5.md`](docs/phases/phase-2.5.md)).
- Converts lossless tracks to WAV, AIFF or FLAC, for older players or to save
  space, keeping cues and playlists
  ([`docs/phases/phase-2.6.md`](docs/phases/phase-2.6.md)).
- Stick profiles: a saved, visible list of playlists per stick, synced with a
  dry run; sticks written by rekordbox can be adopted
  ([`docs/stick-profiles.md`](docs/stick-profiles.md)).
- Analyses tracks itself (BPM, beatgrid, key, waveforms) in Pioneer formats.
- Later: the newer rekordbox "Device Library Plus" (`exportLibrary.db`) format
  and Denon Engine DJ libraries.
- Later still: a mix-prep tool, 2–4 beat-synced decks through a 4-channel mixer
  with EQ kills and filters, to try transitions while building playlists
  ([`docs/phases/phase-5.md`](docs/phases/phase-5.md)).

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
| [`docs/phases/`](docs/phases/) | Phase 1–6 plans (plus 2.5 and 2.6) with task checklists |
| [`docs/stick-profiles.md`](docs/stick-profiles.md) | Stick profiles and adopting foreign sticks (spec, D27) |
| [`docs/research.md`](docs/research.md) | Evidence and sources behind the decisions |
| [`docs/research/`](docs/research/) | Raw research notes per topic |
| [`docs/analysis-parity.md`](docs/analysis-parity.md) | reks analysis vs rekordbox on the maintainer's library: method, numbers, tuning plan |
| [`docs/hw-test-log.md`](docs/hw-test-log.md) | Real-player test results |
| [`docs/analysis-parity/`](docs/analysis-parity/waveforms.md) | Analysis vs rekordbox: waveform calibration (method, error, pictures) |
| [`docs/ui.md`](docs/ui.md) | UI design system: tokens, size scale, components |
| [`docs/mixer-dsp.md`](docs/mixer-dsp.md) | Mix-prep mixer DSP: EQ, filter, crossfader and limiter laws, sources, test tolerances |
| [`fixtures/README.md`](fixtures/README.md) | Test fixtures: generated audio signal, ffmpeg commands, sample rekordbox XML |

## Building from source

Prerequisites: Rust (see `rust-toolchain.toml`), Node 24, and the Tauri 2
system prerequisites for your OS (https://tauri.app/start/prerequisites/).

```sh
npm ci
cargo install tauri-cli --version '^2' --locked
cargo tauri dev
```

Unsigned developer builds for macOS, Windows and Linux are attached to draft
GitHub releases when a `vX.Y.Z` tag is pushed
([`.github/workflows/release.yml`](.github/workflows/release.yml)). They are not
code-signed, so the OS will warn on first launch.

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
