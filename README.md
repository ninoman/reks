# reks

A free, open-source desktop DJ library manager and USB exporter. A standalone
alternative to rekordbox for managing your music collection and preparing USB
sticks that play on Pioneer DJ / AlphaTheta CDJs and XDJs, and later on Denon
Engine DJ players.

## What it does

None of the USB exports has been tested on a real player yet
([`docs/hw-test-log.md`](docs/hw-test-log.md) is empty). Until it has, treat
every stick as untested.

- Keeps your own library: folder scan, tags, artwork, a searchable, filterable
  track table, playlists.
- Imports an existing rekordbox collection via `collection.xml` (beatgrids,
  cues, loops, keys, playlists) and writes one back.
- Exports a USB stick in the legacy rekordbox format (`export.pdb` + ANLZ
  files), with a dry run before anything is written
  ([`docs/phases/phase-2.md`](docs/phases/phase-2.md)).
- Playlist dividers on the stick, and play history read back from played
  sticks, with practice sessions left out of play counts
  ([`docs/phases/phase-2.5.md`](docs/phases/phase-2.5.md)).
- Stick profiles: a saved list of playlists per stick, synced with a dry run;
  sticks written by rekordbox can be adopted
  ([`docs/stick-profiles.md`](docs/stick-profiles.md)).
- Check a stick before a gig: a read-only report on any stick, whoever made
  it ([`docs/check-a-stick.md`](docs/check-a-stick.md)).
- Converts lossless tracks to WAV, AIFF or FLAC, keeping cues and playlists
  ([`docs/phases/phase-2.6.md`](docs/phases/phase-2.6.md)).
- Analyses tracks itself: BPM, beatgrid, key and every Pioneer waveform type
  ([`docs/phases/phase-3.md`](docs/phases/phase-3.md),
  [`docs/analysis-parity.md`](docs/analysis-parity.md)).
- Exports the newer rekordbox Device Library Plus format (`exportLibrary.db`)
  ([`docs/phases/phase-4.md`](docs/phases/phase-4.md)).
- Mix-prep: up to 4 beat-synced decks with hot cues, loops and scrolling
  waveforms through a 4-channel mixer (EQ, filters, crossfader, headphone
  cue), in a simple or an advanced mixer view
  ([`docs/phases/phase-5.md`](docs/phases/phase-5.md)).
- Next: Denon Engine DJ export
  ([`docs/phases/phase-6.md`](docs/phases/phase-6.md)).

## Status

Pre-alpha (October 2026). The software work for Phases 1–5 is in `main`.
What is still open:

| Phase | Open |
| --- | --- |
| 1 Library | Acceptance criteria and manual checklist |
| 2 Legacy export | Hardware campaign on a CDJ-2000NXS2 and a CDJ-3000; eject and volume serial on Windows and Linux; a fixture from a played stick |
| 2.5 / 2.6 | Hardware sessions |
| 3 Analysis | Key tonic accuracy (12a); hardware check |
| 4 Device Library Plus | Hardware test |
| 5 Mixer | Key lock evaluation, listening checklist, a 3-band EQ + filter mixer view |
| 6 Engine DJ | Not started |

No players are available yet, so the software work goes ahead and anything a
player has to confirm is marked UNVERIFIED. Open work is tracked on the
[reks roadmap](https://github.com/users/ninoman/projects/1) board (D33).

## Documentation

| Doc | What it holds |
| --- | --- |
| [`CLAUDE.md`](CLAUDE.md) | Brief for contributors and coding agents: stack, rules, conventions |
| [`docs/decisions.md`](docs/decisions.md) | Architecture and product decisions, with status |
| [`docs/phases/`](docs/phases/) | Phase 1–6 plans (plus 2.5 and 2.6) with task checklists |
| [`docs/stick-profiles.md`](docs/stick-profiles.md) | Stick profiles and adopting foreign sticks (spec, D27) |
| [`docs/check-a-stick.md`](docs/check-a-stick.md) | Check a stick before a gig: what the report checks (user guide, D38) |
| [`docs/research.md`](docs/research.md) | Evidence and sources behind the decisions |
| [`docs/research/`](docs/research/) | Raw research notes per topic |
| [`docs/analysis-parity.md`](docs/analysis-parity.md) | reks analysis vs rekordbox on the maintainer's library: method, numbers, tuning plan |
| [`docs/hw-test-log.md`](docs/hw-test-log.md) | Real-player test results |
| [`docs/analysis-parity/`](docs/analysis-parity/waveforms.md) | Analysis vs rekordbox: waveform calibration (method, error, pictures) |
| [`docs/ui.md`](docs/ui.md) | UI design system: tokens, size scale, components |
| [`docs/mixer-dsp.md`](docs/mixer-dsp.md) | Mix-prep mixer DSP: EQ, filter, crossfader and limiter laws, sources, test tolerances |
| [`docs/releasing.md`](docs/releasing.md) | What CI runs when, and how to cut a release |
| [`docs/feedback.md`](docs/feedback.md) | In-app bug reports and feedback: what is sent, relay setup, triage |
| [`fixtures/README.md`](fixtures/README.md) | Test fixtures: generated audio signal, ffmpeg commands, sample rekordbox XML |

## Building from source

Prerequisites: Rust (see `rust-toolchain.toml`), Node 24, and the Tauri 2
system prerequisites for your OS (https://tauri.app/start/prerequisites/).

```sh
npm ci
cargo install tauri-cli --version '^2' --locked
cargo tauri dev
```

To build a local macOS DMG, run `npm run bundle:dmg`
(see [`docs/releasing.md`](docs/releasing.md#local-builds)).

Unsigned developer builds for macOS, Windows and Linux are attached to draft
GitHub releases cut with `npm run release -- X.Y.Z` (see
[`docs/releasing.md`](docs/releasing.md)). They are not code-signed, so the OS will
warn on first launch.

Checks run by CI (Linux on PRs, all three OSes at release): `npm run lint`, `npm test`,
`npm run build`, `cargo fmt --all --check`,
`cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`,
`cargo deny check licenses`, `npm run licenses:check`.

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
