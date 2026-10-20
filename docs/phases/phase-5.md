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
  The Phase 1 preview player already runs on this engine (D5).
- Decks (2 shown, 4 on demand): load from the track table or a playlist,
  play/pause, CDJ-style cue (set and return), jump to the track's hot cues, seek
  on the waveform, tempo fader (±8/±16/±50%), pitch-bend nudge, quantise.
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
    combination, RESONANCE mild to wild, ON. No LFOs (D21). VCF TO
    XFADER (crossfader sweeps the cutoff) and LINK (Y follows X's FREQ).
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
- Editing grids or cues from a deck (grid editor is Phase 3 task 13).
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
    Cue { deck: u8 }, Seek { deck: u8, sample: i64 }, HotCue { deck: u8, slot: u8 },
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
   lists what it offers), 4 deck slots summed until the channel strips land,
   `Control` in `control.rs` with every variant above plus `Quantise`;
   `Leader` takes `Option<u8>` (`None` = automatic) and `Seek` keeps its
   `frame` field. Mixer variants are accepted and ignored until tasks 4–7.
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
   `dsp::eq::ChannelEq`, standalone (not yet in the graph); laws and
   tolerances in [`../mixer-dsp.md`](../mixer-dsp.md).
5. [x] **Filters**: two state-variable filters (trapezoidal SVF from the
   published papers, our own code), mode combinations, resonance taper,
   VCF TO XFADER, LINK. Tests: −3 dB at the set
   cutoff for LP and HP at minimum resonance; stable at maximum resonance across
   30 Hz–20 kHz at 44.1, 48 and 96 kHz. Done: `dsp::filter` (`Filter`, `FilterPanel`
   for VCF TO XFADER and LINK), standalone; MILD is Butterworth, not the
   hardware's likely overdamped setting ([`../mixer-dsp.md`](../mixer-dsp.md)).
6. **Channels, crossfader, master**: trim, fader taper, X/off/Y assign,
   crossfader curve law, master level, meters, limiter. Tests: crossfader gain
   at 0, 0.5 and 1 for both curve extremes (−4 dB at centre on progressive);
   limiter ceiling holds with resonance at maximum. Partial: the DSP blocks
   are done and tested (`dsp::level`, `dsp::xfader`, `dsp::meter`,
   `dsp::limiter`, [`../mixer-dsp.md`](../mixer-dsp.md)). Still to do: wire
   channel strips, EQ, filter and crossfader assigns, master and limiter into
   `graph.rs`, with `Control::Channel`/`Filter`/`Crossfader`/`Master`.
7. **Cue system**: cue bus, pre/post EQ, cue/mix balance, split cue, routing.
   Test: each routing mode puts the expected signal on each output channel.
8. **Tauri commands and IPC**: thin `mix_*` commands, typed wrappers in
   `src/lib/ipc.ts`, engine state on a `Channel`, waveform columns as `ArrayBuffer`.
9. **UI**: mixer panel, 2/4-deck layout, scrolling waveforms with beat markers
   and sync phase, "load to deck" from the track table and playlists, shortcuts.
10. **Key lock evaluation**: listen to `timestretch`, `signalsmith-stretch` and
    `rubberband` at ±6% on three techno tracks and one vocal track; record the
    choice in D21; add it as a per-deck toggle, or record why not.
11. **Listening checklist** below, run and logged.

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
4. Filter X LP on channel 1, filter Y HP on channel 2, LINK on: sweeping FREQ
   swaps lows and highs smoothly.
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
