# Mixer DSP

The Phase 5 mixer's signal processing ([`phases/phase-5.md`](phases/phase-5.md)
tasks 4–7, decision D21). Reference: the Xone:92 music section as documented in
the original user guide ([`research.md`](research.md) §9.1). The target is the
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
| MID 1 / MID 2 | dB, linear each side: −30 at 0, 0 at noon, +6 at 1 | range R§9.1; shape ours |
| LEVEL trim | dB, linear: −10 … +10 | 20 dB range R§9.1; ±10 split **UNVERIFIED** |

Parameters ramp linearly to new values over `SMOOTH_MS` = 10 ms (the plan asks
for at least 5 ms), so moving a knob through noon or to a kill does not step.

## EQ (task 4)

`dsp::eq::ChannelEq`, bands in series HI → MID 1 → MID 2 → LO.

- **HI** (2.5 kHz) and **LO** (250 Hz) shelves: with `F` a fixed 2-pole low-pass
  (HI) or high-pass (LO) and `g` the shelf amplitude,
  cut is `y = F + g·(x − F)` and boost is `y = x + (g − 1)·(x − F')`.
  At `g = 1` both are exactly `x`; at `g = 0` the output is `F` alone, the
  12 dB/oct kill. `F` is Butterworth (Q 0.707); `F'` is damped (Q 0.4) so full
  boost peaks at +6.4 dB rather than +6.8 dB. Both run all the time.
- `F` and `F'` use Vicanek's matched second-order design (M. Vicanek,
  "Matched Second Order Digital Filters", 2016), not the bilinear transform.
  A bilinear 2.5 kHz low-pass is cramped towards Nyquist and falls at about
  14 dB/oct between 5 and 10 kHz at 44.1 kHz; the matched one falls at 11.8,
  as the analogue prototype does.
- **MID 1** (2 kHz) and **MID 2** (350 Hz): trapezoidal SVF bells (see the
  filters section), `y = x + k·(A² − 1)·band`, `k = 1/(Q·A)`, `A = 10^(dB/40)`,
  exactly `dB` at the centre. Q is 1.2 on boost and 0.5 on cut ("narrow on
  boost, wide on cut"; the figures are **UNVERIFIED**, the guide gives none).

Tests (sine measurement at 44.1, 48 and 96 kHz; `dsp::eq` tests):

| Check | Tolerance | Measured where |
|---|---|---|
| all knobs at noon | ±0.1 dB | 20 Hz–20 kHz, 1/3 octave |
| full boost | +6 ±0.5 dB, never above +6.5 | HI at 10 kHz, mids at centre, LO at 40 Hz; sweep 1/6 octave |
| mid full cut | −30 ±1 dB | centre |
| HI / LO kill slope | 12 ±1 dB/oct | HI 5→10 kHz; LO 125→62.5 and 62.5→31.25 Hz |
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
| RESONANCE | `Q = 0.707·(20/0.707)^v`: MILD is Butterworth, WILD a +26 dB peak (Q 20 **UNVERIFIED**) |

- Modes: `y = lp·low + bp·k·band + hp·high + none·x`, each weight smoothed.
  HP+LP is a notch at the cutoff; all three give back the input (the guide's
  "all-pass effect"); BP alone has unity peak gain. No mode selected passes the
  signal rather than muting it (our choice).
- ON crossfades between dry and filtered; the SVF keeps running while off.
- `FilterPanel::effective_freq` (pure): VCF TO XFADER makes a unit's FREQ the
  crossfader position; LINK makes Y follow X's effective FREQ and wins over
  Y's own VCF TO XFADER. Whether the hardware stops crossfading in VCF TO
  XFADER mode is **UNVERIFIED**; that is graph wiring.
- Deviation: the guide says resonance is "flattest at about two o'clock",
  so on the hardware MILD is probably overdamped. We make MILD the flat
  (Butterworth) setting so the set cutoff is the −3 dB point at minimum
  resonance, as task 5's test requires.

Tests (`dsp::filter` tests):

| Check | Tolerance |
|---|---|
| LP and HP at minimum resonance, at the set cutoff (30 Hz–15 kHz) | −3.01 ±0.1 dB at 44.1, 48, 96 kHz |
| maximum resonance at the cutoff | +26.0 ±0.2 dB |
| LP+BP+HP, and no mode | 0 ±0.01 dB; HP+LP notch below −40 dB |
| maximum resonance, LP/HP/BP, 30 Hz–20 kHz in 1/3 octaves and while FREQ sweeps end to end | finite, below 200 on full-scale noise, rings down; at 44.1, 48, 96 kHz |
| mode switch plus ON → off on a 50 Hz tone | no sample step above 0.01 |

## Signal flow (task 6)

The graph renders 32-frame blocks: each deck fills its channel's block, the
mixer runs the strips and the master bus.

```
deck → trim → EQ → FILTER assign (dry / X / Y) → [channel meter] → fader → XFADE assign (X / direct / Y) ─┐
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
  smoothed inside the filter. VCF TO XFADER and LINK set the cutoff once a
  block from the smoothed crossfader position, and the filter smooths it.
  The crossfader keeps crossfading in VCF TO XFADER mode (**UNVERIFIED**).
- Switch values arrive as knob values: assigns X below 1/3, Y above 2/3,
  else off; on/off switches on from 0.5. LINK is a panel switch, taken from
  either unit's control.
- Start state: every channel flat with its fader up, assigns off, master
  up, so a single deck (the Phase 1 preview player) plays through bit for
  bit, delayed by the limiter's look-ahead (`Graph::latency()`, 1.5 ms).
  The HI and LO shelves return the input exactly at noon for this.
- `Control::Volume` (preview player) stays as a squared gain on top of the
  master level.
- Meters: channel meters read after EQ and filter, before the fader, so they
  show the level the trim sets; the master meter reads before the limiter,
  so it shows what the limiter catches. `EngineState` carries both; each
  snapshot clears the clip latches.

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
LINK and VCF TO XFADER; meters and clip latches. The engine's allocation
guard test sends every mixer control while both filter units and the
assigns move.
