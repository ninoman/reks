# Mixer DSP

The Phase 5 mixer's signal processing ([`phases/phase-5.md`](phases/phase-5.md)
tasks 4–6, decision D21). Reference: the Xone:92 music section as documented in
the original user guide ([`research.md`](research.md) §9.1). The target is the
documented response, not an analogue emulation.

Code: [`crates/reks-mix/src/taper.rs`](../crates/reks-mix/src/taper.rs) and
[`crates/reks-mix/src/dsp/`](../crates/reks-mix/src/dsp/). Every processor
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
