# Phase 5 — Mix-prep mixer (decks, beat sync, Xone:92-style mixer)

Status: not started; scheduled after Phase 4 and before Engine DJ export
(Phase 6) (D21, reordered 2026-10-20). Depends on: Phase 1
(library, decode, imported grids) and Phase 3 (waveform columns, grid editor).
Read `docs/decisions.md` D5, D10, D21 and `docs/research.md` §9 first.

## Goal

While building a playlist, the DJ loads 2–4 library tracks onto decks, syncs
them by beatgrid, and tries a transition or a techno layer through a mixer that
behaves like the music section of an Allen & Heath Xone:92, on headphones and
speakers. It is a preparation tool. Nothing here is meant for playing a gig.

## Scope

- `reks-mix` crate: real-time engine on a raw `cpal` stream, decks, sync, mixer
  DSP, and an offline `render()` that runs the same graph without a device.
  The Phase 1 preview player already runs on this engine (D5); since D30 the
  library's player is this engine's deck 2.
- Decks (2 shown, decks 2 and 3; 4 on demand): load from the track table or a
  playlist, play/pause, CDJ-style cue (set, return, preview while held), jump
  to the track's hot cues, drag the waveform to skim, tempo fader
  (±6/±10/±16% and WIDE ±100%, CDJ-3000; ±10 default), pitch-bend nudge, quantise.
- Sync: one leader deck (first playing, or chosen). SYNC on a deck matches the
  leader's tempo and beat phase from `beatgrid` rows; bar sync also matches the
  downbeat. Varispeed: pitch follows tempo.
- Mixer, 4 channels, modelled on the Xone:92 music channels (R§9.1):
  - LEVEL trim (20 dB range); 4-band asymmetric EQ: HI shelf above 2.5 kHz,
    MID 1 bell at 2 kHz, MID 2 bell at 350 Hz, LO shelf below 250 Hz; +6 dB
    max boost; HI and LO fully cut become 12 dB/oct low-pass/high-pass (kill);
    mids cut to −30 dB; Q narrow on boost, wide on cut.
  - FILTER assign (X / off / Y), XFADE assign (X / off = direct / Y), CUE,
    channel fader.
  - Two state-variable filters, X and Y: FREQ 30 Hz–20 kHz, HP/BP/LP in any
    combination, RESONANCE mild to wild, ON. No LFOs (D21). VCF TO XFADER and
    LINK were dropped (task 14).
  - Crossfader with curve: progressive with a 4 dB centre dip, through to a
    near-instant cut.
  - Master level; channel and master meters; a safety limiter on master and
    cue, always on (the hardware has none; headphones make it necessary).
- Headphone cue: per-channel CUE, pre-EQ by default with a POST EQ switch,
  cue/mix balance, cue level, split cue. Master on outputs 1/2 and cue on 3/4
  when the device has 4 or more outputs; otherwise split mono (L cue, R
  master) or no cue.
- UI: mixer panel laid out like the hardware (channel strips, filter units on
  either side of the crossfader), 2 or 4 deck strips with scrolling waveforms
  and beat markers, keyboard shortcuts. No "Xone" or "Allen & Heath" in UI strings.

## Non-goals

- Live performance: MIDI controller mapping, recording the mix, booth/aux
  outputs, effects other than the filters, DVS, streaming services.
- Key lock in the first cut (task 10 evaluates it).
- Master and cue on two separate audio devices (clock drift). Windows ASIO.
- Editing grids from a deck (grid editor is Phase 3 task 13). Cue editing
  on the decks was added by task 17 (maintainer's request, 2026-10-22).
- Analogue sound emulation. The target is the documented response:
  frequencies, slopes, ranges, curves.
- Saving mixer state or transitions in the library DB.

## Data structures and interfaces

```rust
// reks-mix
pub struct EngineConfig { pub device: Option<String>, pub sample_rate: u32, pub buffer_frames: u32,
                          pub cue: CueRouting }
pub enum CueRouting { Outputs { master: [u16; 2], cue: [u16; 2] }, SplitMono, Off }

pub struct DeckSource { .. }      // built off the audio thread by the app layer:
// DeckSource::new(pcm: Vec<i16>, sample_rate)   interleaved stereo, ~10 MB per minute
//     .with_grid(&[GridAnchor { frame, bpm, beat_in_bar, beats_per_bar }])  // `beatgrid` rows, source frames
//     .with_hot_cues(vec![(slot, frame)])

impl Engine {
    pub fn start(cfg: EngineConfig) -> Result<Engine>;
    pub fn send(&self, c: Control) -> Result<()>;      // never blocks; Err when the queue is full
    pub fn poll_state(&self) -> Option<EngineState>;   // playheads, meters, sync phase; ~60 Hz
}
pub fn render(graph: &mut Graph, out: &mut [f32]);     // same graph, no device; deterministic

pub enum Control {
    Load { deck: u8, src: Arc<DeckSource> }, Play { deck: u8 }, Pause { deck: u8 },
    Cue { deck: u8 }, CueRelease { deck: u8 }, Seek { deck: u8, sample: i64 }, HotCue { deck: u8, slot: u8 },
    Tempo { deck: u8, ratio: f64 }, Nudge { deck: u8, ratio: f64 },
    Sync { deck: u8, on: bool, bar: bool }, Leader { deck: u8 },
    Channel { ch: u8, param: ChannelParam, value: f32 },   // trim, eq_*, fader, filter/xfade assign, cue
    Filter { unit: FilterUnit, param: FilterParam, value: f32 },
    Crossfader { pos: f32, curve: f32 },
    Master { param: MasterParam, value: f32 },
}
```

- Control values are 0..=1 like a knob; `reks-mix::taper` maps each to dB, Hz
  or a curve, with a test per taper. Every parameter is smoothed (≥ 5 ms ramp).
  Seek, cue and hot-cue jumps are instant with a short equal-power crossfade
  from the old position (`SEEK_XFADE_MS`, Phase 1 player).
  Filter-type and assign switches crossfade, so they don't click (the hardware does).
- Whole tracks are decoded to memory by `reks-audio` before `Load`, so seeks and
  sync corrections never wait on the decoder. Four 6-minute decks use about 250 MB.
- The audio thread never allocates, locks or does I/O. Debug builds fail a
  test if the callback allocates.

## Tasks

1. [x] **Engine skeleton**: done in Phase 1 task 11 (D5): `reks-mix` crate, cpal
   stream with device selection, `rtrb` control/state queues, `render()`,
   allocation guard, one deck. Left for this phase: rate and buffer-size
   selection, more than one deck, and the `Control`/`EngineState` variants below.
   Done 2026-10-21: `EngineConfig { sample_rate, buffer_frames, cue }` (an
   explicit rate or buffer the device lacks is an error; `device_options()`
   lists what it offers), 4 deck slots (into the mixer since task 6),
   `Control` in `control.rs` with every variant above plus `Quantise`;
   `Leader` takes `Option<u8>` (`None` = automatic) and `Seek` keeps its
   `frame` field.
2. [x] **Deck**: `DeckSource` loading, play/pause/cue/seek/hot cue, varispeed with
   `rubato`, nudge, quantise. Tests: at ratio 1.0 the render equals the decode
   sample for sample; at 1.05 click spacing is 1/1.05 of the original within 1 sample.
   Done 2026-10-21 (`deck.rs`, `varispeed.rs`, `beatmap.rs`, `source.rs`;
   render tests in `crates/reks-mix/tests/deck.rs`). At exactly unit step the
   deck copies frames (bit-exact); otherwise it reads through rubato's async
   sinc resampler (128 taps, 32-frame chunks) with its own source-time clock,
   so the reported position matches what is heard to under half a frame. CUE:
   playing → back to the cue point and pause; paused → set the cue point.
   HOT CUE jumps and plays. Quantise: cue points and paused seeks snap to the
   nearest beat; jumps while playing keep the beat phase. Dynamic grids run
   linearly between anchors, so beats land on the anchors.
3. [x] **Sync**: leader, tempo match, beat and bar phase, continuous phase
   correction on dynamic grids. Tests: 128 and 125 BPM click fixtures synced
   keep onsets within 1 ms for 64 bars; a tempo-change fixture stays within 2 ms
   across the change. Done 2026-10-21 (`sync.rs`; render tests in
   `crates/reks-mix/tests/sync.rs`, click fixtures generated in
   `tests/common`). Leader: the chosen deck, else the loaded deck with a grid
   that has been playing longest (kept while all are stopped); it plays at its
   own tempo × nudge. Every 32-frame block a follower's speed is set from both
   local grid tempos (so dynamic grids are followed beat by beat), and its
   beat (bar sync: place in the bar) is compared with the leader's: an error
   over 1/32 beat jumps into phase (seek crossfade, never before the track's
   start), a smaller one bends the speed by at most 1% (time constant 0.5 s).
   While nudged, a follower matches tempo only and re-locks on release. A
   stopped leader passes on its tempo, not its phase. Measured worst gaps:
   128/125 over 64 bars 0.0005 ms; tempo change in the follower 0.043 ms, in
   the leader 0.025 ms.
4. [x] **EQ**: 4-band asymmetric EQ with tapers. Sine-sweep tests: knobs at noon
   flat within ±0.1 dB; full boost +6 dB ±0.5 dB; mids full cut −30 dB ±1 dB;
   HI/LO full cut fall at 12 dB/oct ±1 dB past the turning point; silence in,
   silence out (no NaN or denormals). Done: `reks-mix::taper` and
   `dsp::eq::ChannelEq`, in every channel strip since task 6; laws and
   tolerances in [`../mixer-dsp.md`](../mixer-dsp.md).
5. [x] **Filters**: two state-variable filters (trapezoidal SVF from the
   published papers, our own code), mode combinations, resonance taper,
   LINK and VCF TO XFADER (both built here and removed in task 14). Tests: −3 dB at the set
   cutoff for LP and HP at minimum resonance; stable at maximum resonance across
   30 Hz–20 kHz at 44.1, 48 and 96 kHz. Done: `dsp::filter` (`Filter`, `FilterPanel`
   for LINK, since removed), wired in task 6; MILD is Butterworth, not the
   hardware's likely overdamped setting ([`../mixer-dsp.md`](../mixer-dsp.md)).
   2026-10-21 (maintainer's testing): resonance halved to Q 0.707–10 (WILD
   +20 dB, was +26 dB) on a v² curve (noon +2.7 dB); a test pins the knob
   direction (LP cuts highs to the left, HP cuts lows to the right).
6. [x] **Channels, crossfader, master**: trim, fader taper, X/off/Y assign,
   crossfader curve law, master level, meters, limiter. Tests: crossfader gain
   at 0, 0.5 and 1 for both curve extremes (−4 dB at centre on progressive);
   limiter ceiling holds with resonance at maximum. Done 2026-10-21: DSP in
   `dsp::level`, `dsp::xfader`, `dsp::meter`, `dsp::limiter`; wired in
   `strip.rs` (trim → EQ → FILTER assign → fader → XFADE assign) and
   `mixer.rs` (filter units, crossfader, master level → meter → limiter),
   driven by `Control::Channel`/`Filter`/`Crossfader`/`Master`. Channel and
   master meters are in `EngineState`. The limiter delays the output by
   1.5 ms (`Graph::latency()`). A flat strip passes a deck bit for bit.
   Render tests in `crates/reks-mix/tests/mixer.rs`
   ([`../mixer-dsp.md`](../mixer-dsp.md)).
7. [x] **Cue system**: cue bus, pre/post EQ, cue/mix balance, split cue, routing.
   Test: each routing mode puts the expected signal on each output channel.
   Done 2026-10-21 (`cue.rs`, `routing.rs`): per-channel CUE (pre-fader),
   pre-EQ by default, POST EQ taps before the fader; CUE MIX is equal power;
   split cue puts cue (mono) left and master (mono) right; CUE LEVEL, then
   its own safety limiter, aligned with the master's. `CueRouting::Outputs`
   puts master and cue on the named outputs, `SplitMono` sends the split
   phones signal to outputs 1/2, `Off` master only; `Engine::start` refuses
   a routing the device has no outputs for. `Graph::render_outputs` renders
   master and phones. Tests: routing modes on the engine callback, cue render
   tests in `crates/reks-mix/tests/cue.rs`.
8. [x] **Tauri commands and IPC**: thin `mix_*` commands, typed wrappers in
   `src/lib/ipc.ts`, engine state on a `Channel`, waveform columns as `ArrayBuffer`.
   Done 2026-10-21 ([`src-tauri/src/mix.rs`](../../src-tauri/src/mix.rs)):
   `mix_device_options`, `mix_start` (device, rate, buffer, cue routing; the
   state goes to a `Channel` at 60 Hz until `mix_stop`), `mix_load` (decodes
   at the engine rate on a worker with grid and hot cues in frames,
   `reks_core::deck`, then builds the `DeckSource` there; a newer load for
   the deck wins), `mix_control` (`MixControl` mirrors `Control`; seek in ms;
   `Load` is `mix_load`). Waveforms and beat lines reuse `waveform_columns`
   (raw `ArrayBuffer`) and `beatgrid_get`. Dev mock: `src/dev/mockMix.ts`.
   `MixState` carries the channel and master meters. `mix_device_options`
   reports the device's output count and the cue routing to preselect
   (`reks_mix::default_cue_routing`: cue on 3/4 with four outputs, else off).
9. [x] **UI**: mixer panel, 2/4-deck layout, scrolling waveforms with beat markers
   and sync phase, "load to deck" from the track table and playlists, shortcuts.
   Done 2026-10-21: the Mix panel ([`../ui.md`](../ui.md#mix-panel-phase-5),
   `src/components/mix/`, `src/store/mix.ts`), new base components `Knob`,
   `Fader`, `Caption`. Mixer positions live in the UI store and are all sent
   after each start, so the engine matches the panel. Verified in the
   browser with the dev mock only, not yet on the real engine. Channel meters
   sit beside the faders and the master meter in the master section.
   2026-10-21 (maintainer's testing): meters are DJM-style LED columns
   (−20…+10, +10 = 0 dBFS, UNVERIFIED calibration), and every mixer switch
   that is on is lit (`variant="control"`, `lit` tokens).
   Filter modes and channel cues are single-select on click, Shift+click
   combines (`pickSwitches` in `src/lib/mixView.ts`, with a tooltip); FILTER
   assign defaults to X for channels 1–2 and Y for 3–4 (`reks-mix` and the UI
   store). See D21.
   Waveform style setting (2026-10-21, maintainer request): Settings → Decks
   picks 3-band, RGB or Blue for the deck waveforms and overviews, applied
   live ([`../ui.md`](../ui.md#mix-panel-phase-5); `display.deck_waveform`
   in `reks-core` `settings.rs`). Verified in the browser with the dev mock.
10. **Key lock evaluation**: listen to `timestretch`, `signalsmith-stretch` and
    `rubberband` at ±6% on three techno tracks and one vocal track; record the
    choice in D21; add it as a per-deck toggle, or record why not.
    Partial (2026-10-21): research and render tool done, listening pending.
    Licences, builds and maintenance: [R§9.3](../research.md). All three are
    GPLv3-compatible (timestretch and Signalsmith MIT, Rubber Band
    GPL-2.0-or-later). `reks keylock-render` ([`keylock/`](../../crates/reks-cli/src/keylock/mod.rs))
    decodes an excerpt and renders it through each engine at each ratio, plus
    plain varispeed for reference, aligned by each engine's latency so the
    files can be switched in sync. timestretch is in the default build; the
    C++ engines are opt-in features (`keylock-signalsmith`,
    `keylock-rubberband`, or `keylock-all`; need a C++ compiler and libclang).

    Cost on one deck (M4 Pro, release, 256-frame blocks = 5.8 ms at 44.1 kHz,
    60 s synthetic 128 BPM loop; CPU ms per second of audio at 0.94 / 1.00 /
    1.06, worst block as % of the block period, latency reported/measured on
    a tone burst):

    | Engine | CPU ms/s | Worst block | Latency | Rust heap events in process |
    |---|---|---|---|---|
    | varispeed (reference) | 1.5 / 1.4 / 1.4 | 1% | 1.5 ms | 0 |
    | timestretch-keylock | 12.9 / 12.3 / 15.3 | 4% | 12.7 ms / 5.7–14.7 | 0 |
    | timestretch-wide | 28.9 / 26.0 / 34.5 | 19% | 0 ms / ±1.5 | 15–27 |
    | signalsmith (default preset) | 5.4 / 4.6 / 5.3 | 5% | 60 ms / 60 | 0 (C++ heap not visible) |
    | rubberband-r3 (finer) | 40.2 / 36.6 / 41.3 | 10% | 46.4 ms / 46–47 | ~41k: the binding's `Vec` per call |
    | rubberband-r2 (faster) | 11.4 / 7.2 / 9.9 | 3% | 23.2 ms / 23–24 | ~21–44k: same |

    Four decks at the worst case (rubberband-r3) is about 17% of one core.
    Per-block cost for Rubber Band is uneven (it runs in its own hops behind a
    FIFO); the worst block above is the peak seen.

    Listening (maintainer, own tracks; never commit them or the renders):

    ```sh
    cargo build --release -p reks-cli --features keylock-all
    # one run per track; out-dir outside the repo
    target/release/reks keylock-render ~/Music/techno-1.flac --out-dir ~/keylock/techno-1
    target/release/reks keylock-render ~/Music/vocal-1.flac --out-dir ~/keylock/vocal-1 --from 45
    # blind variant: files named <stem>.<ratio>.<letter>.wav, key in keylock-report.json
    target/release/reks keylock-render ~/Music/techno-2.flac --out-dir ~/keylock/techno-2-blind --blind
    ```

    Defaults: ratios 0.94, 1.00, 1.06; 90 s from 60 s in (`--from`,
    `--seconds`); `--engines timestretch-keylock,signalsmith` limits the set.
    Outputs: `<stem>.<engine>.<ratio>.wav` (24-bit, the file's rate) and
    `keylock-report.json` (cost and latency per render). Compare each engine
    against `varispeed` at the same ratio and against the 1.00 render; listen
    for kick smear, hi-hat flamming, bass warble, phasiness on pads, and
    pitch wobble or doubling on the vocal. Score 1–5 (5 = sounds untouched).

    | Track (genre) | Engine | 0.94 | 1.06 | Notes |
    |---|---|---|---|---|
    | techno 1 | timestretch-keylock | | | |
    | techno 1 | timestretch-wide | | | |
    | techno 1 | signalsmith | | | |
    | techno 1 | rubberband-r3 | | | |
    | techno 1 | rubberband-r2 | | | |
    | techno 2 | timestretch-keylock | | | |
    | techno 2 | timestretch-wide | | | |
    | techno 2 | signalsmith | | | |
    | techno 2 | rubberband-r3 | | | |
    | techno 2 | rubberband-r2 | | | |
    | techno 3 | timestretch-keylock | | | |
    | techno 3 | timestretch-wide | | | |
    | techno 3 | signalsmith | | | |
    | techno 3 | rubberband-r3 | | | |
    | techno 3 | rubberband-r2 | | | |
    | vocal 1 | timestretch-keylock | | | |
    | vocal 1 | timestretch-wide | | | |
    | vocal 1 | signalsmith | | | |
    | vocal 1 | rubberband-r3 | | | |
    | vocal 1 | rubberband-r2 | | | |

    Listener, date, headphones/speakers: _______. Choice: pending (D21).
11. **Listening checklist** below, run and logged.
12. [x] **Deck refinements** (maintainer review of tasks 8–9, 2026-10-21):
    - Tempo ranges ±6, ±10, ±16 and WIDE (±100%), CDJ-3000; ±10 is the
      default, as on the CDJ. The engine takes tempo 0–2 (`TEMPO_MIN`/`MAX`);
      the varispeed head goes to 8:1 steps (`MAX_STEP`), so slower than 1/8
      of the source rate the deck fades out over 5 ms and holds its place
      (speed reported as 0) until it is fast enough again.
    - CUE preview: `Control::Cue` is the press, `Control::CueRelease` the
      release. Paused on the cue point, the press plays until the release,
      which returns to the cue point and pauses; PLAY during the preview
      keeps playing. Setting the cue point while paused now moves the head
      onto it (with quantise it was left off the beat), as on a CDJ.
      `DeckState::cue_preview` / `cuePreview` reports it.
    - Drag to skim on the deck's overview and zoomed waveform
      ([`../ui.md`](../ui.md#mix-panel-phase-5)).
    - Deck cards no longer stretch to the mixer's height: each deck was
      `flex-1` inside a column that stretches to the mixer, so in 2-deck mode
      the card grew by the mixer's spare height. Nothing used that space.
    - Two decks shown are decks 2 and 3 (channels 2 and 3, the centre pair
      around a 4-channel mixer); 1–2 sit left of the mixer, 3–4 right.
      Deck N still plays into channel N and every crossfader assign stays
      off (THRU), so routing needs no change. Keys: A/S/D… drive deck 2 and
      J/K/L… deck 3; Shift reaches the outer decks 1 and 4.
    Tests: `holding_cue_on_the_cue_point_previews`,
    `wide_tempo_doubles_and_stands_still_at_the_ends` (`graph.rs`),
    `mixView.test.ts`, `mixKeys.test.ts`, `DeckStrip.test.tsx`. Checked in
    the browser with the dev mock only.
13. [x] **Engine: master hand-off, beat jump, beat loop** (maintainer review,
    2026-10-22). Engine, IPC and dev mock only; the deck redesign builds the
    controls (D21).
    - [x] Master hand-off. Bug: deck A master, B synced; when A ended, B fell
      back to its own tempo (the automatic leader moved to B, which then
      played its own fader). Now the master (`EngineState::master_deck`,
      `DeckState::master`; `masterDeck` / `master` in `MixState`) is a
      sticky role, as on a CDJ: the first deck with a grid to play takes it,
      or the deck sent `Control::TakeMaster` (`takeMaster`; replaces
      `Leader`, there is no "automatic" mode any more). When it stops, ends
      or is unloaded or reloaded, a playing synced deck takes over (else the
      deck playing longest) and keeps the tempo it had matched: its tempo
      fader is set to it, so `tempo` changes in the state. With no deck
      playing, nobody is master, and paused followers keep their matched
      tempo too. A follower taking master keeps its tempo the same way.
      Supersedes task 3's leader rule.
    - [x] Beat jump: `BeatJump { beats }` (signed; UI sizes FINE, 1/8 … 32,
      `BEAT_JUMP_SIZES` in `ipc.ts`) moves along the grid with quantise (the
      place in the beat is kept on dynamic grids too), else by `beats` of the
      local beat length; ignored without a grid or when it would leave the
      track. During a loop the loop moves with it. `FineJump { forward }`
      moves 10 ms (`FINE_JUMP_MS`; rekordbox's FINE amount is UNVERIFIED).
    - [x] Beat loop: `BeatLoop { beats }` (1/8 … 32; halve/double reach
      1/32 … 512) from the playhead, or with quantise from the last grid step
      (whole beats, or 1/`beats` for loops under a beat); `LoopExit`;
      `LoopResize { double }` keeps the start. At the end the head goes back
      by the loop length exactly (no drift against sync) with a 4 ms
      equal-power crossfade (`LOOP_XFADE_MS`) from the head running on. No
      allocation (checked by the engine's real-time test). A seek, hot cue or
      cue outside the loop ends it; a load clears it. `DeckState::loop_region`
      / `loopRegion` (ms in the UI). Sync: a looping follower matches tempo
      only; a master looping under a beat gives no phase, and one looping
      part of a bar gives beat phase only.
    Tests (`graph.rs`): `the_master_ending_hands_over_to_the_synced_deck_at_the_same_tempo`,
    `the_master_pausing_hands_over_to_the_synced_deck_at_the_same_tempo`,
    `with_no_synced_deck_playing_nobody_is_master`,
    `taking_master_from_a_follower_keeps_its_tempo`,
    `the_master_is_the_first_deck_to_play_until_another_takes_it`,
    `beat_jump_moves_by_beats_and_keeps_the_place_in_the_beat`,
    `quantised_beat_jumps_follow_a_dynamic_grid`,
    `beat_loop_repeats_from_the_quantised_beat`,
    `unquantised_loops_start_at_the_head_and_seeks_out_of_them_end_them`,
    `the_loop_point_crossfades_without_a_click`,
    `a_synced_loop_stays_in_phase_with_the_master`;
    `loops_limit_sync_to_what_keeps_time` (`sync.rs`); the real-time test in
    `engine.rs`; `controls_parse_from_the_ui_shapes` (`src-tauri/src/mix.rs`);
    `src/dev/mockMix.test.ts`. Not checked by ear or against a CDJ
    (UNVERIFIED: CDJ-3000 hand-off details and loop/jump behaviour are
    modelled from the manuals' descriptions, not tested on hardware).
14. [x] **Mixer polish** (maintainer review, 2026-10-22):
    - Every knob draws a position mark: a short line from near the centre out past the cap's rim at
      the current angle, in `fg` on the `knob` fill (both themes). One change
      in `ui/Knob.tsx` (`knobMark`), so every knob has it.
    - **VCF TO XFADER removed** (maintainer: unused, removed 2026-10-22): the
      toggle, `FilterParam::VcfToXfader`, `vcfToXfader` in IPC and store, the
      block-rate cutoff logic and `Crossfader::pos`. No decision in
      `decisions.md` named it.
    - **LINK removed** (maintainer: unused, removed 2026-10-22): the Y toggle,
      `FilterParam::Link`, `link` in IPC and store, and `FilterPanel`; FREQ
      now goes straight to each unit's filters.
    - Shortcut hints: `Button` takes `shortcut` and shows it in a tooltip on
      hover/focus ("Play · S"). The text comes from `mixKeys.ts`
      (`mixKeyLabel`), so a key and its hint cannot drift apart. Applied to
      the deck buttons (cue, play, sync, nudge, hot cues) and the crossfader
      resets.
    Tests: `Knob.test.tsx` (mark angle), `Button` hint rendered from the key
    map (`ui.test.tsx`), `mixKeys.test.ts`, `mix.test.ts` (no VCF in IPC).
15. [x] **Deck waveforms** (maintainer review, 2026-10-22): sharpness, zoom,
    cue marks ([`../ui.md`](../ui.md#mix-panel-phase-5)).
    - Why it looked low-res (measured, not the frame budget: 60 fps with
      4 decks before and after). Peaks were binned per CSS pixel and drawn
      as 1-CSS-px bars, so on a 2× screen every bar was two device pixels
      (half the resolution the columns have: 8 s over a 450 px deck is
      1.3 columns per device pixel). The bins also moved with the
      fractional view start each frame, so peaks re-binned and shimmered,
      and the scroll stepped in whole CSS pixels (2 device px) whenever a
      ~60 Hz engine state arrived, not on the display's frames. Data was
      not the cause: the deck already drew the 150/s analysis columns (the
      PWV5 rate), not the overview.
    - Fix: the waveform is drawn once into 512-device-pixel tiles on a pixel
      grid fixed to the track (`src/lib/waveStrip.ts`); each animation frame
      copies tiles at a whole-device-pixel offset, with beat lines and marks
      on the same grid (`src/lib/deckScroll.ts`). Canvases are sized to whole
      device pixels (no resampling). The playhead runs on between engine
      states at the deck's speed and eases out arrival jitter; a seek, or a
      stopped deck, snaps. Paused decks don't redraw.
    - Frame time, headless Chrome at DPR 2, 1440×900, dev mock, 4 decks
      playing, 5 s: before 60.2 fps, worst frame 16.8 ms, 6.7 ms main thread
      per frame (whole app: React renders of the 60 Hz state and the mock
      engine); after 60.2 fps, worst 16.8 ms, 5.6 ms (2 s zoom 5.7 ms, 32 s
      zoom 5.8 ms), no frame over 20 ms. One deck's waveform per frame:
      1.6 ms before, 0.9 ms after at twice the pixels (both including a GPU
      read-back to time it); a new tile costs ~3 ms, every ~0.6 s per deck
      at the closest zoom. Not yet measured in the Tauri WebView on a
      Retina screen.
    - Zoom: 32, 16, 8 (default), 4, 2 s across the view, per deck, kept
      across loads; − / + on the view, E/R (deck 2) and O/P (deck 3), Shift
      for decks 1/4, or Cmd/Ctrl + scroll. Past one column per device pixel
      (4 s and 2 s on a wide deck) levels are interpolated between columns.
      Skimming the zoomed view follows the zoom.
    - Marks on both views: hot cues as a flag with the slot letter in the
      cue's library colour (`deck-hotcue` when it has none), memory cues and
      loops (by their start) as a triangle on the bottom edge
      (`deck-memory`), the deck's cue point likewise in `deck-cue`.
      `mix_load` now returns `hotCues[].colorRgb` and `memoryCues`
      (`reks-core` `cue_marks`). The active loop draws as a `deck-loop` band
      from `DeckView.loopRegion` (task 13's field; null on this branch until
      the engine reports loops, so nothing draws yet).
    Tests: `cue_marks_list_hot_cues_with_colour_then_memory_cues_by_position`
    (`deck.rs`), `deckScroll.test.ts`, `waveStrip.test.ts`,
    `deckMarks.test.ts`, `mixKeys.test.ts`. Checked in headless Chrome with
    the dev mock only.
16. [x] **Deck redesign** (maintainer review, 2026-10-22). Built on tasks 13
    (master handoff, beat jump, beat loop, loop region), 14 (tooltip
    `shortcut`, control polish) and 15 (per-deck zoom, waveform markers).
    Layout, states, components, data and keys:
    [`../ui.md`](../ui.md#deck-layout-task-16) (wireframe approved, revision
    3). UI only, no engine work beyond what 13–15 add, except:
    - `HotCueView.name` through `mix_load` (`CueMark.name` from `Cue.name`);
      the colour was already there from task 15 (`colorRgb`).
    - Bar sync is a setting (Settings → Decks → "Sync aligns: beat / bar",
      `mix.sync_align`, `SyncAlign` in `reks-core` `settings.rs`, default
      bar), replacing the deck's BAR button.
    - A track loaded without a grid is queued for analysis first; when it is
      done the deck refetches columns and grid, and reloads the track in
      place once paused so the engine has the grid (cue point, position and
      tempo put back).
    - The deck's 2 × 4 pads: HOT CUE (A–H in their cue colours and names),
      BEAT JUMP (FINE = 1/32 beat … 128, split back / forward), BEAT LOOP
      (1/8 … 32; the lit size exits, another resizes). New keys E/R O/P jump,
      F ; loop, G H take master, T Y pad mode.
    - Waveform zoom is one level for all decks (maintainer, 2026-10-22;
      replaces task 15's per-deck zoom), on the − / + keys wherever focus is
      (not in text fields, dialogs or menus), Mix panel open or not.
    - Tempo: no Reset button (double-click or Delete; drags snap at 0%), BPM
      typed into the header steps the range up when needed; nudge uses the
      CDJ jog arrows. Master tempo (**M**) stays disabled until task 10.
    - `tsconfig.app.json` type errors from task 14 fixed (`shortcut` under
      `exactOptionalPropertyTypes`; the root `tsc --noEmit` checks no files,
      so `npx tsc --noEmit -p tsconfig.app.json` is the real check).
    Tests: `mixView.test.ts` (pages, loop pad controls, detent snap, BPM →
    tempo and range step-up), `mixKeys.test.ts` (new keys, hints match the
    `ui.md` table, round trip), `keys.test.ts` (zoom keys), `ui.test.tsx` (`Pad` height and ink,
    `SplitPad`, `InlineNumber` commit / cancel / parse), `DeckStrip.test.tsx`
    (every state, pad modes and pages, eject on artwork, BPM edit, master),
    `mix.test.ts` (analysis on load and reload when paused, pads, BPM,
    sync bar), `settings.rs` (`SyncAlign`), `deck.rs` (cue names).
    Checked in headless Chrome (DPR 2) with the dev mock: 2- and 4-deck mode
    at 1280 × 800 (deck 441 px), 1440 × 900 (521 × 248 px, as specified) and
    1920 × 1080, each pad mode, empty, loading, analysing → grid, error and
    BPM edit. Not checked in the Tauri WebView. CDJ-like behaviour (pad sizes, FINE,
    jump and loop feel) is UNVERIFIED on hardware.

17. [x] **Cue editing on the decks** (maintainer request, 2026-10-22;
    revised the same day: "auto cue" means memory loops playback enters by
    itself, not a cue point chosen on load, and deleting gets a × as well
    as Shift+click). UI: [`../ui.md`](../ui.md#cue-editing-task-17).
    - [x] Library: `reks-core` `cue.rs`: `set_hot_cue` (empty slot only;
      next `HOT_CUE_PALETTE` colour), `add_memory_cue` (a point, or a loop
      with its end; the same start and end within 1 ms is the same cue),
      `rename_cue` (trimmed, blank clears, ≤ 64 characters, no control
      characters), `set_active_loop` (memory loops only), `delete_cue`.
      Migration 0011 adds `cue.active_loop` (default 0; `Cue::active_loop`,
      copied by the converter). Rows are `kind` 0 or 4, `source =
      'manual'` (a rekordbox re-import leaves them); `updated_at` is bumped.
    - [x] Formats (checked 2026-10-22): the rekordbox XML has no active
      loop field (`POSITION_MARK` has `Name`, `Type`, `Start`, `End`,
      `Num`, colour), so an XML export for rekordbox writes AUTO loops as
      plain memory loops. In ANLZ the `.DAT` memory `PCOB`'s `PCPT`
      `status` 4 marks the active loop ([`../research/raw-3-analysis-anlz.md`](../research/raw-3-analysis-anlz.md),
      Deep Symmetry's anlz docs; `rekordcrate` reads it), and the `PCP2`
      entry has no flag. The PDB holds no cues. baken-export 4.6.1 writes
      status 4 for a memory loop whose XML `Name` starts with `[active]`
      (its issue #210; its test matches rekordbox 7 bytes), one per track
      (the earliest).
    - [x] Export, behind the `export.active_loops` flag (`ExportOptions::active_loops`,
      wizard switch "Active loops", off by default, UNVERIFIED until a
      player enters the loop in a hardware test): the temp XML handed to
      baken prefixes the earliest AUTO memory loop's name with `[active]`
      (`rbxml::ActiveLoops::BakenMarker`), which baken strips. Our own ANLZ
      builder still writes status 0: baken splices the cue lists from the
      XML over it. OneLibrary's `cue.isActiveLoop` is not written (phase 4
      writer unchanged).
    - [x] Engine: `Control::SetHotCue { slot, frame: Option }` (hot cues in
      a fixed array, filled from the source at load), `Control::CallCue
      { frame }` (cue point and head move there exactly, playing on),
      `Control::AutoLoop { index, region }` and `DeckSource::with_auto_loops`
      (up to `AUTO_LOOP_SLOTS` = 8 AUTO loops per deck). A playing head
      that reaches an AUTO loop's start (`start` in `before..now` for the
      frame, so a head parked on the start enters too) enters it as a beat
      loop: same wrap and `LOOP_XFADE_MS` crossfade, LOOP EXIT leaves it,
      and it is entered again only by passing the start again. No
      allocation (the real-time test sends the new controls).
      `DeckSource::cue_frame` snaps a cue set at the head with quantise.
    - [x] App: `mix_cue_edit { deck, edit: CueEdit }` writes the library,
      then the engine (hot cue, all AUTO loop slots), and returns the
      deck's cues. MEM while the engine reports a loop saves that loop. A
      cue set at the playhead takes the position from the latest engine
      state (up to one state, ~17 ms, early on a playing deck; quantise
      snaps it to the beat either way). `MemoryCueView.endMs` /
      `activeLoop`, `callCue` control in ms.
    - [x] UI: empty pad sets, set pad triggers, double-click renames, × or
      Shift+click deletes; MEM, ◀ ▶ and the memory list (jump, rename,
      AUTO, ×) in HOT CUE mode; memory and AUTO loop marks; mark names on
      hover; keys Alt A / Alt J (memory cue), Alt E R / Alt O P (previous /
      next); track details refetch cues after an edit (`cueRevisions`).
    Tests: `tests/cue_edit.rs` (round trip, memory loops and the flag,
    refused edits), `rbxml_export.rs` `cues_edited_on_a_deck_reach_the_export`,
    `reks-export-rb` `tests/active_loop.rs` (marker equals baken's; baken
    finds the earliest AUTO loop only with the flag), `cue.rs` (palette,
    names), `deck.rs` (loop ends, auto loops), `reks-anlz` `cue.rs`
    (palette exports as distinct player colours), `source.rs` and
    `graph.rs` (set / clear / call, entering an AUTO loop, leaving it,
    re-entry, a parked head, live edits), the engine's real-time test,
    `src-tauri/src/mix.rs` (`CueEdit`, `callCue`), `DeckStrip.test.tsx`
    (empty pad sets, double-click renames, Esc cancels, Shift+click and ×
    delete, the × follows its pad in the Tab order, memory list jump /
    AUTO / delete), `ExportWizard.test.tsx` (Active loops off, UNVERIFIED,
    sent), `mix.test.ts`, `mixKeys.test.ts`, `deckMarks.test.ts`,
    `mockMix.test.ts`. Checked in headless Chrome (DPR 2) with the dev
    mock at 1280 × 800 (4 decks) and 1440 × 900 with real pointer and key
    events: every edit above, the × by hover and by Tab + Enter, MEM
    during a loop, AUTO, and a deck entering the loop as it plays. Not
    checked in the Tauri WebView. Not tried on a CDJ: memory call and
    active loops on players are UNVERIFIED.

## Acceptance criteria

- 4 decks, both filters and all EQs at 48 kHz with 256-frame buffers: no
  underruns in a 60-minute soak on the maintainer's laptop; the callback uses
  under 25% of the buffer period (measured and logged).
- Two synced constant-grid tracks stay within 2 ms of each other for
  10 minutes (render test). Met 2026-10-21: 128/126 BPM, both decks
  resampling, worst gap 0.022 ms
  (`two_synced_constant_grids_stay_within_2_ms_for_10_minutes`).
- EQ, filter and crossfader responses meet the tolerances in tasks 4–6.
- All of the above except the soak test runs in `cargo test --workspace` with
  no audio device (CI has none).

## Listening checklist

Record machine, OS, audio device, date and outcome in a "Mixer" table in
`docs/hw-test-log.md`.

1. Two techno tracks at 128 and 126 BPM, synced and layered for 5 minutes on
   speakers: no audible drift or flanging.
2. Four decks layered (kick from one, hats from another with LO killed, etc.):
   no dropouts while moving every control.
3. LO kill removes the kick; HI kill leaves the body; mid cut is deep, not total.
4. Filter X LP on channel 1, filter Y HP on channel 2: sweeping each FREQ
   moves only its own unit, smoothly.
5. Maximum resonance while sweeping FREQ: the limiter catches
   peaks, nothing harsh on headphones.
6. 4-output interface: master on 1/2, cue on 3/4. Laptop output: split cue on
   the correct sides.
7. Unplug the device while playing: the engine stops cleanly and reports it;
   reconnect or switch device without restarting the app.
8. One rekordbox-imported grid and one reks-analysed grid both sync.

## Risks

- **Real-time discipline**: one allocation or lock in the callback can drop
  out. Mitigation: allocation guard, offline render tests, soak test.
- **Sync depends on the grid**: a wrong grid makes sync drift or sit off-beat.
  Mitigation: Phase 3 grid editor first; show sync phase on the deck.
- **Model fidelity**: figures come from the original Xone:92 user guide; the
  Mk2 may differ (UNVERIFIED). The filter slope is not documented
  (UNVERIFIED); we use a 2-pole SVF.
- **Scope creep** towards a performance app (loops, MIDI, recording, LFOs). Each
  needs its own decision (D21).
