# Hardware test log

Append one entry per real-player test. Never claim compatibility in docs,
README or UI strings without an entry here.

| Date | Phase/task | Player + firmware | Stick (fs, size, partition) | What was exported | Result | Notes / error codes / photos |
|---|---|---|---|---|---|---|
| (none yet) | | | | | | |

## Mixer (Phase 5, task 11 and task 27)

Listening checklist from [`phases/phase-5.md`](phases/phase-5.md#listening-checklist).
Release build `e6d9306`. Tracks are the maintainer's own (never committed).

- Machine: MacBook Pro, Apple M4 Pro, 48 GB
- OS: macOS 26.6.2
- Audio device: Focusrite Scarlett 2i2 (2 outputs, headphones on the unit)
- Date: 2026-10-30
- Tracks: Sleep D – Hydralite (128, rekordbox grid), Priori – Winged (Aurora
  Halal Remix) (126, reks-analysed grid), buttechno – bleeps (128, kick-driven),
  Trajano – Sp!re Dub (126, bright top end)

| Step | Check | Result | Notes |
|---|---|---|---|
| 1 | 128 + 126 BPM synced and layered 5 min, speakers | Pass | Hydralite + Winged; no drift or flanging |
| 2 | Four decks layered, every control moved | Pass | No dropouts. CPU not recorded |
| 3 | LO kill, HI kill, mid cut | Pass | on bleeps |
| 4 | Filter X LP / Y HP sweeps independent and smooth | Pass | channels 2 and 3 |
| 5 | Max resonance sweep, headphones | Pass | limiter held, nothing harsh |
| 6 | Master 1/2 + cue 3/4; split cue on laptop output | Not run | 2i2 has 2 outputs and master and cue on two devices is a non-goal; maintainer skipped split cue |
| 7 | Unplug device while playing, recover without restart | Pass | |
| 8 | rekordbox grid and reks-analysed grid both sync | Pass | covered by step 1 |
| 27 | Channel FX: filter ends, echo time and feedback, reverb decay and level | Pass | by ear; figures not measured |
