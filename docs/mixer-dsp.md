# Mixer DSP

The Phase 5 mixer's signal processing ([`phases/phase-5.md`](phases/phase-5.md)
tasks 4–7, decision D21). Reference: the Xone:92 music section as documented in
the original user guide, except the EQ, which follows the Xone:96 channels 1–4
(since 2026-10-22; [`research.md`](research.md) §9.1). The target is the
documented response, not an analogue emulation.

Code: [`crates/reks-mix/src/taper.rs`](../crates/reks-mix/src/taper.rs),
[`crates/reks-mix/src/dsp/`](../crates/reks-mix/src/dsp/) (the blocks) and
[`strip.rs`](../crates/reks-mix/src/strip.rs) /
[`mixer.rs`](../crates/reks-mix/src/mixer.rs) (the signal flow). Every processor
works in place on interleaved stereo `f32`, keeps `f64` filter state, and never
allocates after `new`. Filter states are flushed to zero together below
1e-20 so silence never leaves subnormals or a limit cycle behind.

## Tapers and smoothing

Every control is a 0..=1 knob value; `taper` maps it. NaN or infinity becomes
the neutral position (noon for EQ and trim, closed for faders).

| Control | Law | Source |
|---|---|---|
| HI / LO shelf | amplitude: boost `10^(6·(2v−1)/20)` above noon; cut `(2v)³` below (kill at 0) | range R§9.1; cut shape ours |
| HI MID / LO MID | dB, linear each side: −27 at 0, 0 at noon, +10 at 1 | range R§9.1; shape ours |
| LEVEL trim | dB, linear: −10 … +10 | 20 dB range R§9.1; ±10 split **UNVERIFIED** |
| 3-band HI / MID / LOW | dB, linear each side: −26 at 0, 0 at noon, +6 at 1 | range R§9.5; shape ours |
| Channel FX knob | `d = 2v − 1`, dead zone ±0.03, `t = (|d| − 0.03)/0.97`, side = sign | ours (`fx_send`) |

Parameters ramp linearly to new values over `SMOOTH_MS` = 10 ms (the plan asks
for at least 5 ms), so moving a knob through noon or to a kill does not step.

## EQ (task 4)

`dsp::eq::ChannelEq`, bands in series HI → HI MID → LO MID → LO (`EqBand::Mid1` / `Mid2`).

- **HI** (3 kHz) and **LO** (180 Hz) shelves, +6 dB / kill: with `F` a fixed 2-pole low-pass
  (HI) or high-pass (LO) and `g` the shelf amplitude,
  cut is `y = F + g·(x − F)` and boost is `y = x + (g − 1)·(x − F')`.
  At `g = 1` both are exactly `x`; at `g = 0` the output is `F` alone, the
  12 dB/oct kill (the Xone:92's slope; the Xone:96 guide gives none,
  **UNVERIFIED**). `F` is Butterworth (Q 0.707); `F'` is damped (Q 0.4) so full
  boost peaks at +6.4 dB rather than +6.8 dB. Both run all the time.
- `F` and `F'` use Vicanek's matched second-order design (M. Vicanek,
  "Matched Second Order Digital Filters", 2016), not the bilinear transform.
  A bilinear 2.5 kHz low-pass is cramped towards Nyquist and falls at about
  14 dB/oct between 5 and 10 kHz at 44.1 kHz; the matched one falls at 11.8,
  as the analogue prototype does.
- **HI MID** (1.1 kHz) and **LO MID** (350 Hz), +10 / −27 dB: trapezoidal SVF bells (see the
  filters section), `y = x + k·(A² − 1)·band`, `k = 1/(Q·A)`, `A = 10^(dB/40)`,
  exactly `dB` at the centre. Q is 1.2 on boost and 2.0 on cut. Q here is the
  width at half the gain in dB, so the earlier cut Q of 0.5 made a full cut
  remove 10–17 dB an octave or more away and the mids acted as kills like HI
  and LO (maintainer report, 2026-10-22). At Q 2 a full cut is a notch,
  above −3 dB two octaves away. The Xone:92 guide asks for a low Q on cut;
  we depart from it on the maintainer's ear. The figures are **UNVERIFIED**
  (no guide gives any).

Tests (sine measurement at 44.1, 48 and 96 kHz; `dsp::eq` tests):

| Check | Tolerance | Measured where |
|---|---|---|
| all knobs at noon | ±0.1 dB | 20 Hz–20 kHz, 1/3 octave |
| full boost | shelves +6, mids +10, ±0.5 dB; never 0.5 dB above | HI at 12 kHz, mids at centre, LO at 30 Hz; sweep 1/6 octave |
| mid full cut | −27 ±1 dB; above −3 dB two octaves away | centre; centre ÷4 and ×4 |
| HI / LO kill slope | 12 ±1 dB/oct | HI 6→12 kHz; LO 90→45 and 45→22.5 Hz |
| silence | exact zeros, no NaN or subnormals | 10 s after a full-scale burst |

## Filters (task 5)

`dsp::filter::Filter`, one per unit (X, Y), stereo. A trapezoidal
(zero-delay-feedback) state-variable filter from A. Simper's Cytomic papers
("Solving the continuous SVF equations using trapezoidal integration and
equivalent currents", 2013, https://cytomic.com/technical-papers), our own
code: `g = tan(π·fc/fs)`, `k = 1/Q`, outputs `low`, `band`, `high` with
`low + k·band + high = x`. It stays stable and click-free while cutoff and Q
change every sample. 2-pole; the hardware slope is **UNVERIFIED** (R§9.1).

| Control | Law |
|---|---|
| FREQ | `30·(20000/30)^v` Hz, the −3 dB point (R§9.1) |
| RESONANCE | `Q = 0.707·(10/0.707)^(v²)`: MILD is Butterworth (Q 0.707), noon Q 1.37 (+2.7 dB), WILD a +20 dB peak (Q 10 **UNVERIFIED**). Halved from Q 20 with a v² curve on 2026-10-21: the maintainer found it too sensitive and too much at maximum |

- Modes: `y = lp·low + bp·k·band + hp·high + none·x`, each weight smoothed.
  HP+LP is a notch at the cutoff; all three give back the input (the guide's
  "all-pass effect"); BP alone has unity peak gain. No mode selected passes the
  signal rather than muting it (our choice).
- ON crossfades between dry and filtered; the SVF keeps running while off.
- Each unit has its own FREQ. VCF TO XFADER and LINK were removed
  2026-10-22 (maintainer: unused).
- Deviation: the guide says resonance is "flattest at about two o'clock",
  so on the hardware MILD is probably overdamped. We make MILD the flat
  (Butterworth) setting so the set cutoff is the −3 dB point at minimum
  resonance, as task 5's test requires.

Tests (`dsp::filter` tests):

| Check | Tolerance |
|---|---|
| LP and HP at minimum resonance, at the set cutoff (30 Hz–15 kHz) | −3.01 ±0.1 dB at 44.1, 48, 96 kHz |
| maximum resonance at the cutoff | +20.0 ±0.2 dB |
| LP with FREQ fully left / HP fully right, 60 Hz and 8 kHz sines | LP cuts 8 kHz below −60 dB, HP cuts 60 Hz below −60 dB; the open end passes both |
| LP+BP+HP, and no mode | 0 ±0.01 dB; HP+LP notch below −40 dB |
| maximum resonance, LP/HP/BP, 30 Hz–20 kHz in 1/3 octaves and while FREQ sweeps end to end | finite, below 200 on full-scale noise, rings down; at 44.1, 48, 96 kHz |
| mode switch plus ON → off on a 50 Hz tone | no sample step above 0.01 |

## 3-band EQ and channel FX (task 26)

The 3-band layout's blocks (`docs/research/djm-eq-fx.md`, R§9.5). Both
layouts' blocks sit in every strip; the UI sends the hidden layout's knobs
at noon / centre, where each block is bypassed and returns its input bit
for bit, so the engine has no EQ mode.

- `dsp::eq3::ThreeBandEq`: LOW shelf 250 Hz → MID bell 1 kHz (Q 0.7) →
  HI shelf 3 kHz, −26 / +6 dB, no kill. The shelves are Simper SVF
  shelves (`g = tan(π·fc/fs)/√A` low, `·√A` high), flat to the floor with
  no dip; the bell is the 4-band mids' `dsp::bell::Bell` at another Q.
  Corners and Q **UNVERIFIED**.
- `channel_fx::ChannelFx`: the FILTER / ECHO / REVERB blocks of a channel.
  The selected one (`Control::ChannelFx`) gets the channel's knob
  (`ChannelParam::Fx`), the others centre, so switching is a knob move and
  tails ring out. PARAMETER (`Control::FxParameter`) goes to all three.
- `dsp::channel_filter`: a low-pass then a high-pass SVF, each wide open
  while the knob is on the other side; cutoff exponential 20 kHz → 50 Hz
  (left) and 20 Hz → 10 kHz (right); resonance `resonance_q(PARAMETER)`
  faded in over the first fifth of travel; wet faded in over the first 5%.
  Ends **UNVERIFIED**.
- `dsp::channel_echo`, `dsp::channel_reverb`: sends into a feedback delay
  (3/4 beat of the channel's deck, `Deck::beat_frames`, fed every block)
  and a Freeverb tank; dry stays at unity. A block is skipped and cleared
  once its knob is centred and its tail is quiet (below 1e-6): reverb
  output for 100 ms; echo, its whole 2 s line, since a gap between repeats
  is longer than 100 ms. The reverb's comb damping is rate-scaled
  (`0.3^(44100/fs)`) and each side's tank is fed its own channel, so the
  wet level does not move with the rate or the stereo image. Figures
  **UNVERIFIED**; details in the spec §3.

| Check | Tolerance |
|---|---|
| 3-band at noon, FX centred (also after a move back) | bit for bit |
| LOW / HI full cut | −26 ±1 dB at 20 Hz / 16 kHz; never below −27 dB |
| MID full cut | −26 ±0.5 dB at 1 kHz; above −7 dB at 250 Hz and 4 kHz |
| full boost | +6 ±0.5 dB, never above +6.5 |
| FILTER at the table's positions | cutoff ±1%, Q ±0.01 (`taper` tests) |
| FILTER full travel | 1 kHz / 100 Hz below −40 dB |
| FILTER swept at −12 dBFS, 50 Hz and 5 kHz | no step above 0.01 beyond the tone's own (×1.45 for resonance) |
| FILTER 0 → 1 → 0.5 in single jumps | bounded: peak ≤ 2.2× input. A 17-octave sweep in 10 ms lets the closed low-pass release its bass as a ~10 ms +6 dB swell at 50 Hz (steps to 0.012); the spec asked for 0.01 |

## Signal flow (task 6)

The graph renders 32-frame blocks: each deck fills its channel's block, the
mixer runs the strips and the master bus.

```
deck → trim → EQ → 3-band EQ → channel FX → FILTER assign (dry / X / Y) → [channel meter] → fader → XFADE assign (X / direct / Y) ─┐
                                                                                                  sum ←┘
sum → master level × preview volume → [master meter] → safety limiter → output
```

- Each channel has its own copy of filter units X and Y, all set alike.
  The filters are linear, so this sounds the same as one shared unit fed the
  sum of its channels, while the fader can still follow the filter as the
  task order asks. A copy only runs while its side of the FILTER assign is
  heard, and starts from cleared state.
- FILTER and XFADE assigns are three smoothed weights (dry/X/Y,
  X/direct/Y), so moving a switch crossfades; filter modes and ON are
  smoothed inside the filter, and so is FREQ.
- Switch values arrive as knob values: assigns X below 1/3, Y above 2/3,
  else off; on/off switches on from 0.5.
- Start state: every channel flat with its fader up, XFADE assign off,
  FILTER assign X for channels 1–2 and Y for 3–4 with both units off
  (an off unit returns its input exactly), master up, so a single deck (the library's player, D30) plays through bit for
  bit, delayed by the limiter's look-ahead (`Graph::latency()`, 1.5 ms).
  The HI and LO shelves return the input exactly at noon for this.
- `Control::Volume` (the player's volume slider, `MixControl::Volume`) stays as a squared gain on top of the
  master level.
- Meters: channel meters read after EQ and filter, before the fader, so they
  show the level the trim sets; the master meter reads before the limiter,
  so it shows what the limiter catches. `EngineState` carries both; each
  snapshot clears the clip latches.

## Headphone cue (task 7)

`cue.rs`, `routing.rs`.

```
strip: trim ─┬─ EQ → filter ─┬─ fader …
             └ pre tap       └ post tap      (× CUE button, smoothed)
pre/post (POST EQ switch) → CUE MIX with the master (after its level, before its limiter)
  or split (cue mono L, master mono R) → CUE LEVEL → safety limiter → phones
```

| Control | Law |
|---|---|
| CUE (per channel) | on/off, smoothed; pre-fader |
| POST EQ | crossfades the pre-EQ tap (after the trim) to the post-EQ tap (after EQ and filter, before the fader; that the filter is included is **UNVERIFIED**) |
| CUE MIX | equal power: cue `cos(π/2·v)`, master `sin(π/2·v)`; −3 dB each at noon (**UNVERIFIED**) |
| SPLIT CUE | crossfades to cue (mono) left, master (mono) right; CUE MIX is not used while split |
| CUE LEVEL | fader law `v³`, at most 0 dB |

The phones limiter has the master's look-ahead, so phones and master stay
aligned. Start state: pre-EQ, cue only, level up, not split; nothing cued
means silent phones.

Routing (`EngineConfig::cue`): `Outputs { master, cue }` puts each on the
named 0-based outputs, `SplitMono` sends the phones signal with split forced
on to outputs 1/2 (one stereo output, e.g. a laptop), `Off` master only.
Other outputs are silent; a mono device gets the master summed. The engine
refuses a routing whose outputs the device lacks. When the routing is
`Off`, the cue bus does not run.

Tests: `routing.rs` per mode; the engine callback renders each mode
(master, cue and silent channels, 4 and 6 outputs, split, mono);
`tests/cue.rs` through the graph: pre-fader, pre/post EQ around a LO kill,
CUE MIX ends, split sides, CUE LEVEL, phones limiter with a maximum-
resonance sweep, and no clicks from any cue switch. The engine's allocation
guard runs every cue control with master on 1/2 and cue on 3/4, then split
mono.

## Channels, crossfader, master (task 6)

| Block | Law |
|---|---|
| `level::ChannelLevel` | trim (table above) × fader `v³` (0 dB at the top, −18 dB at half travel; shape **UNVERIFIED**) |
| `level::MasterLevel` | `v³`, never above 0 dB, so the limiter stays a safety net |
| `xfader::crossfader_gains` | side gain `sin(π/2·t)^p`, `t = clamp(distance from the far end / w, 0, 1)`; `p = 1.329` puts the progressive centre at −4 dB (R§9.1); `w = (1/40)^curve`, from the whole travel (progressive) to 1/40 of it (near-instant cut, 1.5 mm of 60 mm, **UNVERIFIED**) |
| `xfader::AssignSwitch` | X / off / Y as three smoothed weights: `wX·gX + wOff + wY·gY`, so moving the switch crossfades |
| `xfader::Crossfader` | position and curve smoothed like any knob |
| `meter::PeakMeter` | instant attack, 20 dB/s fall (PPM-like; the hardware's LED ballistics are not documented), clip latch |
| `limiter::Limiter` | always on, master and cue; see below |

Limiter: stereo-linked look-ahead peak limiter after G. Burton (Signalsmith
Audio), "Designing a straightforward limiter" (2022). Per frame: the gain that
brings the peak to the ceiling; an exponential release (100 ms); a moving
minimum over the look-ahead `L` (1.5 ms); a moving average over `L`; the
signal delayed by `L − 1` frames (the latency). Every averaged gain is a
minimum over a window that contains the delayed frame, so the ceiling
(−1 dBFS, our choice) holds by construction; a final clamp only covers float
rounding. NaN or infinite input becomes silence.

Tests: crossfader gains at 0, 0.5 and 1 for both curve extremes (progressive
centre −4 ±0.01 dB; sharpest centre 0 dB, full level 1/40 off the stop),
both as a function and through the whole graph,
monotonic and symmetric curves, assign and crossfader ramps; the limiter's
ceiling on a full-scale step and on a maximum-resonance LP, HP and LP+BP
sweep 30 Hz → 20 kHz → 30 Hz at 44.1, 48 and 96 kHz (input peaks above +12 dB),
checked before the rounding clamp; `tests/dsp_realtime.rs` runs every block
with all parameters moving under a counting allocator and requires zero
allocations.

Render tests through the graph (`tests/mixer.rs`): crossfader gains as
above; the limiter ceiling with trim at +10 dB into a maximum-resonance LP
and HP swept 30 Hz → 20 kHz → 30 Hz at 44.1, 48 and 96 kHz (master meter
above +12 dB); assign, filter-mode and ON switches on a 50 Hz tone with no
frame-to-frame step above 0.01; LO kill, fader taper and master level;
each unit's own FREQ; meters and clip latches. The engine's allocation
guard test sends every mixer control while both filter units and the
assigns move.
