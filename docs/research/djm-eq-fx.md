# Three-band EQ and channel filter view (research, 2026-10-23)

Status: **proposal**, no code yet. Spec for the engine side of issue #56: a
mixer view with a 3-band EQ (HI / MID / LOW) and a one-knob filter per
channel in the fourth knob slot, modelled on the DJM-900NXS2 / DJM-A9.
Layout and wireframe: [`deck-mixer-layout.md`](deck-mixer-layout.md) §9.
Sources with dates: [`../research.md`](../research.md) §9.5. Nothing here is
checked on a real DJM; every figure we chose rather than read in a manual
is marked **UNVERIFIED**, and so is every published figure whose meaning
the manuals leave open.

## 1. What the manuals say

| | DJM-900nexus (2011) | DJM-900NXS2 (2015) | DJM-A9 (2022) | Xone:96 (our 4-band, R§9.1) |
|---|---|---|---|---|
| Bands | HI, MID, LOW | HI, MID, LOW | HI, MID, LOW | HI, HI MID, LO MID, LO |
| Range, EQ mode | −26 / +6 dB, all bands | −26 / +6 dB, all bands | −26 / +6 dB, all bands | shelves −∞ / +6, mids −27 / +10 |
| Figure printed next to each band | HI 13 kHz, MID 1 kHz, LOW 70 Hz | HI 30 kHz (sic), MID 1 kHz, LOW 20 Hz | HI 20 kHz, MID 1 kHz, LOW 20 Hz | HI 3 kHz, 1.1 kHz, 350 Hz, LO 180 Hz |
| Isolator | EQ CURVE switch: ISOLATOR / EQ | same; spec sheets: isolator −∞ / +6 | same switch, no range printed | – (HI and LO kill) |
| Curve shapes, slopes, Q | not stated | not stated | not stated (A9 "retuned" isolator, no figures) | not stated |
| Per-channel FX knob | COLOR, 6 effects | COLOR, 6 effects | COLOR, 6 effects, CENTER LOCK | – |

How to read the frequencies: they are where the range was measured, not
turning points. The printed HI and LOW figures moved to the ends of the
audio band (20 Hz, 20 kHz) between models while the range stayed the same,
which fits shelves that reach full cut at the band ends. MID is −26 dB at
1 kHz, so it is a bell centred there. No manual gives crossover or corner
frequencies, slopes or Q, and forum posts that quote 70 Hz / 1 kHz / 13 kHz
as "crossovers" are disputed in the same threads. **UNVERIFIED**: every
corner frequency and Q below.

## 2. EQ: recommendation

A new 3-band EQ block that runs **in series with** today's 4-band EQ. Each
view sends the other EQ's knobs at noon, where both blocks return their
input bit for bit, so the engine needs no EQ mode and no mode switch (see §4).

| Band | Filter | Corner / centre | Range | Taper (knob v, noon 0.5) |
|---|---|---|---|---|
| LOW | 2nd-order SVF low shelf, Q 0.707 | 250 Hz, half-gain point (**UNVERIFIED**) | −26 / +6 dB | dB, linear on each side: −26 at 0, 0 at noon, +6 at 1 (**UNVERIFIED** shape) |
| MID | the existing SVF bell, same Q on boost and cut | 1 kHz (manual), Q 0.7 (**UNVERIFIED**) | −26 / +6 dB | same |
| HI | 2nd-order SVF high shelf, Q 0.707 | 3 kHz, half-gain point (**UNVERIFIED**; same as the Xone:96 HI) | −26 / +6 dB | same |

Why these choices:

- **MID is one real bell, not Mid1 + Mid2 together.** The manuals give one
  band at 1 kHz with −26 dB. Our two mids (1.1 kHz and 350 Hz, −27 dB notches
  at Q 2) driven together give two −29 dB notches 1.65 octaves apart with
  only −13.6 dB between them (620 Hz), and +11 dB of combined boost at
  each centre. That is neither a 3-band mid nor within +6 dB.
- **A real shelf, not the 4-band EQ's `Shelf`.** That structure,
  `F + g·(x − F)`, reaches a clean kill at g = 0, but at the −26 dB floor
  (g = 0.05) the residue and the filter cancel and leave a notch about
  −33 dB deep at 0.22 × the corner (45 Hz for a 200 Hz LOW). Simper's SVF shelf is minimum-phase:
  flat to the floor with no dip, exactly 0 at noon, and safe to modulate
  every sample like the bells. Formulas (A. Simper, "Linear Trapezoidal
  Integrated SVF", Cytomic, 2013; same paper as `dsp/svf.rs`), with
  `A = 10^(dB/40)`, `k = 1/Q`, outputs `v1` (band) and `v2` (low) of an SVF tuned to `g`:
  - low shelf: `g = tan(π·fc/fs) / √A`; `y = x + k·(A − 1)·v1 + (A² − 1)·v2`
  - high shelf: `g = tan(π·fc/fs) · √A`; `y = A²·x + k·(1 − A)·A·v1 + (1 − A²)·v2`
  - Check: both reduce to `y = x` at 0 dB (A = 1), as the noon bit-for-bit rule needs.
  The existing `SvfCoeffs::new(fc, k, fs)` computes `g` from `fc`; pass
  `fc / √A` (low) or `fc · √A` (high), clamped below Nyquist as today.
- **Corners 250 Hz and 3 kHz, mid Q 0.7**, picked from a sweep of
  150–300 Hz × 2.5–5 kHz × Q 0.5–1.0 (analogue prototypes, 1/6 octave):
  - LOW full cut: −26.0 at 20 Hz, −24.2 at 100 Hz, −13 at 250 Hz, −6 at 400 Hz.
    Kick and bass fundamentals lose 24 dB or more, enough for a bass swap.
  - HI full cut: −1.0 at 1 kHz, −13 at 3 kHz, −24.6 at 8 kHz, −26 at 20 kHz.
  - MID full cut: −0.6 at 60 Hz, −5.9 at 250 Hz and 4 kHz, −12.6 at 500 Hz
    and 2 kHz, −26 at 1 kHz. A broad scoop, as a 3-band mid has to cover
    everything between the shelves. The 4-band mids stay narrow notches.
  - All three at full cut: −16.0 to −27.3 dB across 20 Hz–20 kHz. All three
    at full boost: +2.8 to +6.1 dB.
  - Lower LOW corners (150 Hz) leave −19 dB at 100 Hz; higher HI corners
    (4–5 kHz) stop touching the hats' body. Mixxx's 3-band default split,
    246 Hz / 2.5 kHz, is in the same place (R§9.5).
- **No kill in v1.** The manuals' EQ mode is −26 dB, and −26 dB on LOW is
  what a bass swap needs. The ISOLATOR curve (−∞) would need a 3-way
  crossover (e.g. Linkwitz–Riley, as Mixxx does) so MID can kill too. That
  is a different block, left for later and only if asked.
- Smoothing as in `dsp/eq.rs`: one `Smoother` per knob (`SMOOTH_MS`), coefficients
  recomputed every sample while a knob ramps.

Comparison with the 4-band EQ we ship (both at full cut / full boost):

| | 4-band (Xone:96) | 3-band (this spec) |
|---|---|---|
| HI | shelf 3 kHz, kill (12 dB/oct) / +6 | shelf 3 kHz, −26 / +6 |
| Mids | 1.1 kHz and 350 Hz bells, −27 notch (Q 2) / +10 (Q 1.2) | one 1 kHz bell, −26 / +6, Q 0.7 both ways |
| LO / LOW | shelf 180 Hz, kill / +6 | shelf 250 Hz, −26 / +6 |
| At noon | bit for bit | bit for bit |

### Engine changes

- `dsp/bell.rs`: move `Bell` out of `eq.rs`, `pub(crate)`, with its centre,
  boost Q and cut Q as arguments (4-band: 1.2 / 2.0; 3-band: 0.7 / 0.7).
- `dsp/eq3.rs`: `ThreeBandEq` (low shelf → bell → high shelf, stereo, `f64`
  state, no allocation). Constants `EQ3_LOW_HZ = 250.0`, `EQ3_MID_HZ =
  1_000.0`, `EQ3_HI_HZ = 3_000.0`, `EQ3_MID_Q = 0.7`. When all three knobs
  are settled at noon it skips processing (it would return `x` anyway) and
  clears its state, so an unused block costs nothing.
- `taper.rs`: `eq3_db(v)`, −26 at 0, 0 at 0.5, +6 at 1, linear in dB each
  side; NaN → noon.
- `control.rs`: `ChannelParam::Eq3Hi`, `Eq3Mid`, `Eq3Lo` (0.5 = flat), and
  the same in IPC `MixControl` and the dev mock.
- `strip.rs`: trim → 4-band EQ → **3-band EQ → channel filter** → FILTER
  assign → meter → fader. The post-EQ cue tap stays after the filters.

Tests (`dsp::eq3`, sine measurement at 44.1, 48 and 96 kHz, like `dsp::eq`):

| Check | Tolerance | Where |
|---|---|---|
| all knobs at noon | bit for bit | 1 s of noise |
| LOW full cut | −26 ±1 dB; ≤ −23 dB | 20 Hz; 100 Hz |
| LOW full cut, no dip | never below −27 dB | 20 Hz–20 kHz, 1/6 octave |
| HI full cut | −26 ±1 dB; ≤ −23 dB | 16 kHz; 8 kHz |
| MID full cut | −26 ±0.5 dB; above −7 dB two octaves away | 1 kHz; 250 Hz and 4 kHz |
| full boost, each band | +6 ±0.5 dB, never above +6.5 | LOW 20 Hz, MID 1 kHz, HI 16 kHz; sweep 1/6 octave |
| shelf half-gain point | −13 ±1 dB at full cut | 250 Hz, 3 kHz |
| knob sweep end to end in 10 ms, 50 Hz and 5 kHz tones | no sample step above 0.01 | – |
| silence after a full-scale burst | exact zeros, no NaN or subnormals | – |

## 3. The fourth knob

### What the DJM COLOR FX offers

One effect is picked for all four channels (buttons), each channel's COLOR
knob sets the amount and direction, and one global PARAMETER knob sets a
second value. Centre is off. The A9 adds CENTER LOCK: a quick turn stops at
centre instead of running into the other side's effect.

| Effect | COLOR left | COLOR right | PARAMETER | DSP, cost | Click-free | Mix-prep value |
|---|---|---|---|---|---|---|
| FILTER | LPF, cutoff falls | HPF, cutoff rises | resonance | 2 SVFs, trivial | yes | **high**: the transition tool |
| SPACE | reverb on mids + lows | reverb on mids + highs | feedback | reverb, moderate | needs tails | medium: outros |
| DUB ECHO | echo on mids | echo on highs | feedback | delay line + filter, low | needs tails | medium: echo-outs |
| SWEEP | NXS2: gate; A9: notch, widening | band-pass, narrowing | centre frequency | SVF (+ gate on NXS2), low | yes | low |
| NOISE | filtered white noise, cutoff falls | cutoff rises | noise level | noise + SVF, trivial | yes | low: build-ups |
| CRUSH | bit crush | crush through HPF | amount | trivial, aliases on purpose | yes | low |

The original DJM-900nexus had GATE/COMP where the NXS2 has SWEEP. Every
DJM keeps BEAT FX (tempo-synced delays, rolls and more) as a separate
send unit, out of scope here.

### Recommendation

- **v1: FILTER only.** No effect selector, no PARAMETER knob, no CENTER LOCK
  switch (the knob's centre detent does that job, below). It is the only
  effect DJs use in nearly every transition, it costs two SVFs per
  channel, and it can be made exactly transparent at centre.
- **Later, only on a maintainer decision:** DUB ECHO and SPACE, with a
  global selector and a global PARAMETER (feedback). They need delay
  memory allocated in `new()`, and tails that keep ringing after the knob
  returns to centre (the DJM's behaviour). They lean towards performance,
  which D21 keeps out of scope, so they need a D21 note first. NOISE,
  CRUSH and SWEEP: not planned.

### Channel filter law

New block `dsp/channel_filter.rs`, `ChannelFilter`: **two** trapezoidal
SVFs in series, a low-pass then a high-pass, both running with cutoffs
that are continuous functions of the smoothed knob position. One SVF that
flips from LP to HP at centre would carry its state across the flip and
click; two SVFs that are each wide open on the other side never jump.

With `v` the smoothed knob (one `Smoother`, `SMOOTH_MS`), `d = 2v − 1`:

```
DZ = 0.03                                  dead zone (knob 0.485–0.515)
t  = clamp((|d| − DZ) / (1 − DZ), 0, 1)     travel past the dead zone
LP cutoff = d < 0 ? 20 kHz · (LP_END / 20 kHz)^t : 20 kHz   (LP_END = 50 Hz)
HP cutoff = d > 0 ? 20 Hz  · (HP_END / 20 Hz)^t  : 20 Hz    (HP_END = 10 kHz)
Q of the moving filter = 0.707 + (Q_RES − 0.707) · min(t / 0.2, 1)   (Q_RES = 1.4)
Q of the open filter   = 0.707
wet = min(t / 0.05, 1)                      y = x + wet · (filtered − x)
```

| Knob | 0 | 0.1 | 0.2 | 0.3 | 0.4 | 0.47 | 0.485–0.515 | 0.53 | 0.6 | 0.7 | 0.8 | 0.9 | 1 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Mode | LP | LP | LP | LP | LP | LP | off | HP | HP | HP | HP | HP | HP |
| Cutoff (Hz) | 50 | 172 | 592 | 2 035 | 7 000 | 16 600 | – | 24 | 59 | 214 | 771 | 2 777 | 10 000 |
| Q | 1.4 | 1.4 | 1.4 | 1.4 | 1.31 | 0.81 | – | 0.81 | 1.31 | 1.4 | 1.4 | 1.4 | 1.4 |
| wet | 1 | 1 | 1 | 1 | 1 | 0.62 | 0 | 0.62 | 1 | 1 | 1 | 1 | 1 |

- **Direction**: left low-pass, right high-pass, as on every DJM manual
  (some third-party guides have it backwards). The same sides as our X/Y
  FREQ knobs, where LP closes to the left.
- **Cutoff taper**: exponential, equal octaves per travel (8.6 octaves LP,
  9 octaves HP over 97% of the travel). Ends 50 Hz and 10 kHz:
  **UNVERIFIED** (no DJM figure), chosen so full travel leaves a sub
  rumble or a thin top, not silence. Constants, for tuning by ear.
- **Resonance**: fixed, Q 1.4 (+3.5 dB peak), faded in over the first fifth
  of the travel so the edge of the dead zone is flat. The DJM's PARAMETER is
  a global resonance knob, and its own forum has owners calling the
  resonance near 2–3 o'clock "way too high". Q 1.4 is between our X/Y
  filter's MILD (0.707) and its noon (1.37). **UNVERIFIED**. Slope 12
  dB/oct (2-pole), **UNVERIFIED** as on the X/Y units.
- **Centre**: `wet` is 0 inside the dead zone, so the output is exactly `x`.
  When the smoothed knob is settled inside the dead zone the block skips
  both SVFs and clears their state (both are wide open at that point, so a
  restart from zero is inaudible).
- **Not the X/Y `Filter`**: the strip's `filters[2]` are copies of the shared
  X and Y units, set alike on every channel by `filter_mut`. The channel
  filter is per channel and has its own law, so it is a separate block. It
  reuses `SvfState` / `SvfCoeffs` only.
- `control.rs`: `ChannelParam::ChannelFilter` (0.5 = off), IPC
  `MixControl` and the dev mock alike. `taper.rs`: the law above as a pure
  `channel_filter(v) -> { lp_hz, hp_hz, q_lp, q_hp, wet }`, tested on its own.

Tests (`dsp::channel_filter`, at 44.1, 48 and 96 kHz):

| Check | Tolerance |
|---|---|
| knob 0.485, 0.5, 0.515 | bit for bit |
| `channel_filter` at the table's positions | cutoff ±0.5%, Q ±0.01, wet exact |
| knob 0: 1 kHz tone | below −40 dB |
| knob 1: 100 Hz tone | below −40 dB |
| knob 0.3 (LP 2 035 Hz) and 0.7 (HP 214 Hz): response at the cutoff | +2.9 ±0.3 dB (Q 1.4) |
| knob 0 → 1 → 0.5 in single steps, and swept, on 50 Hz and 5 kHz tones | no sample step above 0.01 |
| full-scale noise, knob swept end to end for 10 s | finite, peak below 4 |
| silence | exact zeros, no NaN or subnormals |

## 4. Layout switching

Same rule as `effectiveMix` (`deck-mixer-layout.md` §6): the store keeps
the user's positions, hidden controls send neutral values.

| Engine value | Simple / Advanced | 3-band view |
|---|---|---|
| `EqHi`, `EqMid1`, `EqMid2`, `EqLo` | stored 4-band positions | 0.5 |
| `Eq3Hi`, `Eq3Mid`, `Eq3Lo` | 0.5 | stored 3-band positions |
| `ChannelFilter` | 0.5 | stored position |
| X and Y filter units | as today | off |
| FILTER assign | as today | defaults (1–2 X, 3–4 Y) |
| X-F assign | as today | off (THRU) |

- The 3-band and 4-band EQ positions are stored separately, since the
  same knob angle means different things.
- Switching views ramps one EQ to noon and the other to its stored values
  in the same 10 ms, so the change sounds like turning knobs, never a
  click. The engine needs no EQ mode.
- Engine start state: everything at 0.5, so a single deck still plays
  through bit for bit (the existing graph test covers it once
  `ThreeBandEq` and `ChannelFilter` are in the strip).

## 5. Open questions from the issue

- **UI label.** A third `MixerView` value, not a separate EQ setting: the
  fourth knob replaces the X/Y row and the assigns, so "3-band" combined
  with Advanced would not make sense. Settings → Mixer, Mixer view:
  **Simple · Advanced · 3-band**, with the hint "Three-band EQ and a filter
  knob on each channel". Stored as `three_band`. Knob rows **HI · MID · LOW
  · FILTER**; FILTER is lit when off centre. The Filters show/hide setting
  is disabled in 3-band, as in Advanced. No vendor names.
- **Crossfader.** Not in v1. The view is Simple's layout with a different
  EQ, every channel is THRU, so a crossfader would do nothing. If wanted
  later: a separate Crossfader show/hide setting that adds the X-F
  assigns and crossfader to Simple and 3-band alike. There is no DSP
  coupling.
- **Filter resonance.** Fixed (Q 1.4, above), no setting. A global setting
  is cheap to add later (`Q_RES` is one constant) if the fixed value
  proves wrong by ear.
- **Shortcuts.** There are no filter shortcuts today (`mixKeys.ts` has decks
  and the crossfader only), so nothing changes. X C V do nothing in 3-band,
  as in Simple; the Shortcuts popover leaves them out. The knob itself
  takes the shared knob keys (arrows) and double-click to centre.
- **Knob UI.** Centre detent: a drag that crosses 0.5 sticks there for
  ±0.03 of travel (the dead zone), the software form of CENTER LOCK.
