# Three-band EQ and channel FX (research, 2026-10-23)

Status: **accepted**, implemented in phase 5 task 27 (2026-10-23); as built:
[`../mixer-dsp.md`](../mixer-dsp.md). Spec for the engine side of issue #56: an
EQ layout setting (4-band · 3-band) whose 3-band layout has a 3-band EQ (HI / MID / LOW) and a per-channel FX knob in
the fourth knob slot, with an effect selector and a PARAMETER knob, modelled on the DJM-900NXS2 / DJM-A9.
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
- `strip.rs`: trim → 4-band EQ → **3-band EQ → channel FX** → FILTER
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

Maintainer's call, 2026-10-23: the 3-band layout gets an effect selector and a
PARAMETER knob, as on the DJM.

- **v1: FILTER, ECHO, REVERB** (the DJM's FILTER, DUB ECHO and SPACE, under
  neutral names). FILTER is the transition tool, an echo-out and a reverb
  tail are the two other common exits, and all three are cheap.
- **Not planned:** NOISE, CRUSH, SWEEP (little use while preparing a set),
  CENTER LOCK as a switch (the knob's centre detent does that job, §5),
  BEAT FX.

Controls:

| Control | Scope | Values | Start | Engine |
|---|---|---|---|---|
| FX select | mixer, one for all channels | FILTER · ECHO · REVERB, one always lit (no off: centre is off) | FILTER | `Control::ChannelFx { kind: FxKind }` |
| PARAMETER | mixer, one knob | 0..1; FILTER resonance, ECHO feedback, REVERB decay | 0.5 | `Control::FxParameter { value }`, smoothed |
| FX knob | per channel, the 4th knob | 0..1, centre 0.5 off, dead zone 0.485–0.515 | 0.5 | `ChannelParam::Fx` |

Engine shape:

- Each strip holds all three blocks (`dsp/channel_filter.rs`,
  `dsp/channel_echo.rs`, `dsp/channel_reverb.rs`), all memory allocated in
  `new()` for the stream's rate. The selected block gets the channel's
  knob; the others get 0.5.
- Switching the effect is therefore a knob move: the old effect closes
  and the new one opens over `SMOOTH_MS`. Echo and reverb tails keep
  ringing after their send closes, as on the DJM.
- A block runs while its knob is off centre or its tail is audible; once
  its knob is settled in the dead zone and its output has stayed below
  1e-6 for 100 ms, it is skipped and its state cleared. So a centred
  channel is bit for bit again once the tail has died.
- All three sit after the 3-band EQ and before the channel fader, as COLOR
  FX does on the DJM, so closing the fader also cuts a tail.
- Common law, pure in `taper.rs`: `d = 2v − 1`, `DZ = 0.03`,
  `t = clamp((|d| − DZ) / (1 − DZ), 0, 1)`, side = sign of `d`.

### FILTER

Two trapezoidal SVFs in series, a low-pass then a high-pass, both running
with cutoffs that are continuous functions of the smoothed knob position.
One SVF that flips from LP to HP at centre would carry its state across
the flip and click; two SVFs that are each wide open on the other side
never jump.

```
LP cutoff = d < 0 ? 20 kHz · (LP_END / 20 kHz)^t : 20 kHz   (LP_END = 50 Hz)
HP cutoff = d > 0 ? 20 Hz  · (HP_END / 20 Hz)^t  : 20 Hz    (HP_END = 10 kHz)
Q_RES = taper::resonance_q(PARAMETER)        0.707 … 10, 1.37 at noon
Q of the moving filter = 0.707 + (Q_RES − 0.707) · min(t / 0.2, 1)
Q of the open filter   = 0.707
wet = min(t / 0.05, 1)                      y = x + wet · (filtered − x)
```

With PARAMETER at noon:

| Knob | 0 | 0.1 | 0.2 | 0.3 | 0.4 | 0.47 | 0.485–0.515 | 0.53 | 0.6 | 0.7 | 0.8 | 0.9 | 1 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Mode | LP | LP | LP | LP | LP | LP | off | HP | HP | HP | HP | HP | HP |
| Cutoff (Hz) | 50 | 172 | 592 | 2 035 | 7 000 | 16 600 | – | 24 | 59 | 214 | 771 | 2 777 | 10 000 |
| Q | 1.37 | 1.37 | 1.37 | 1.37 | 1.29 | 0.81 | – | 0.81 | 1.29 | 1.37 | 1.37 | 1.37 | 1.37 |
| wet | 1 | 1 | 1 | 1 | 1 | 0.62 | 0 | 0.62 | 1 | 1 | 1 | 1 | 1 |

- **Direction**: left low-pass, right high-pass, as in every DJM manual
  (some third-party guides have it backwards). The same sides as our X/Y
  FREQ knobs, where LP closes to the left.
- **Cutoff taper**: exponential, equal octaves per travel (8.6 octaves LP,
  9 octaves HP over 97% of the travel). Ends 50 Hz and 10 kHz:
  **UNVERIFIED** (no DJM figure), chosen so full travel leaves a sub
  rumble or a thin top, not silence. Constants, for tuning by ear.
- **Resonance**: PARAMETER with the X/Y RESONANCE law, so the two filters
  feel alike; faded in over the first fifth of the travel so the edge of
  the dead zone is flat. Owners call the DJM's resonance near 2–3 o'clock
  "way too high" (R§9.5); our v² law keeps the first half of the knob
  under +3 dB. Slope 12 dB/oct (2-pole), **UNVERIFIED** as on the X/Y units.
- **Not the X/Y `Filter`**: the strip's `filters[2]` are copies of the
  shared X and Y units, set alike on every channel by `filter_mut`. The
  channel filter has its own law, so it is a separate block that reuses
  `SvfState` / `SvfCoeffs` only.

### ECHO

The DJM's DUB ECHO: "echo on mids" to the left, "on highs" to the right,
PARAMETER = feedback. A send into a feedback delay; the dry signal stays at
unity.

```
send   = t                                   echo input = send · band(x)
band   = left: SVF band-pass 1 kHz, Q 0.7 (unity peak, k·band)
         right: SVF high-pass 2.5 kHz, Q 0.707
time   = 3/4 beat of the channel's deck at its current tempo,
         clamped 50 ms – 2 s; 375 ms (3/4 beat at 120 BPM) with no grid
fb     = 0.3 + 0.6 · PARAMETER               0.3 … 0.9, 0.6 at noon (−4.4 dB a repeat)
loop   = delay → 1-pole LP 6 kHz → 1-pole HP 150 Hz → · fb → back in
y      = x + delay output
```

- The two band filters both run and are crossfaded by side, so the send
  never switches filter state.
- Delay time: `Strip::set_beat_frames(f64)` from the graph each block (the
  deck that feeds the channel); the read head follows a one-pole glide
  (100 ms) with linear interpolation, so a tempo change bends the repeats
  slightly instead of clicking. Buffer: 2 s at the stream rate, stereo.
- Stable by construction: loop gain ≤ fb ≤ 0.9 at every frequency (both
  loop filters ≤ 0 dB), no clipper; the master limiter catches peaks.
- **UNVERIFIED**: the time (the manuals give none; a dotted eighth is the
  usual dub delay), band filters, loop filters and feedback range.

### REVERB

The DJM's SPACE: reverb on "mids and lows" to the left, "mids and highs"
to the right, PARAMETER = feedback, i.e. decay time.

```
send   = t                                   reverb input = send · band(x)
band   = left: SVF low-pass 2 kHz, Q 0.707; right: SVF high-pass 500 Hz, Q 0.707
tank   = Freeverb: 8 parallel damped feedback combs + 4 series all-passes
         per side, right side's delays 23 samples longer
         combs 1116 1188 1277 1356 1422 1491 1557 1617, all-passes 556 441 341 225
         (samples at 44.1 kHz, scaled by fs / 44 100), all-pass gain 0.5,
         damping 0.3, input gain 0.015
room   = 0.70 + 0.25 · PARAMETER             comb feedback; RT60 ≈ 0.6 / 1.1 / 4.2 s at 0 / 0.5 / 1 (from the mean comb length, before damping)
y      = x + WET · tank output               WET set so §tests' level holds
```

- Freeverb is Jezar's public-domain design (2000, R§9.5); our own code.
  Room ≤ 0.95 keeps every comb's loop gain below 1.
- **UNVERIFIED**: everything except the structure (the manuals give no
  figures).

### Tests

At 44.1, 48 and 96 kHz unless stated; the allocation guard runs every
control below.

| Block | Check | Tolerance |
|---|---|---|
| all | knob 0.485, 0.5, 0.515 from start; and once a tail has died | bit for bit |
| all | knob 0 → 1 → 0.5 in single steps, swept, and effect switched while off centre, on 50 Hz and 5 kHz tones | no sample step above 0.01 (filter); echo and reverb: dry path the same, tails finite |
| all | full-scale noise, knob and PARAMETER swept for 60 s | finite, peak below 8 |
| all | silence after a full-scale burst and the tail | exact zeros, no NaN or subnormals |
| filter | `taper::channel_filter` at the table's positions | cutoff ±0.5%, Q ±0.01, wet exact |
| filter | knob 0: 1 kHz tone; knob 1: 100 Hz tone | below −40 dB |
| filter | knob 0.3 and 0.7 at the cutoff, PARAMETER 0 / 0.5 | −3.0 / +2.7 ±0.3 dB |
| echo | impulse, knob 1, 120 BPM grid | first repeat at 375 ms ±1 sample; repeat ratio = fb × the loop filters' gain at the tone ±0.01 (3 kHz burst) |
| echo | knob back to 0.5 after a burst | repeats continue; dry untouched |
| echo | tempo 120 → 128 BPM while ringing | read time reaches 352 ms within 0.5 s; no step above 0.01 |
| reverb | RT60 from an impulse at PARAMETER 0, 0.5, 1 | ±25% of 0.6 / 1.1 / 4.2 s |
| reverb | pink noise, knob 1 (right), PARAMETER 0.5 | wet RMS −6 ±1 dB under the dry |

## 4. Layout switching

Same rule as `effectiveMix` (`deck-mixer-layout.md` §6): the store keeps
the user's positions, hidden controls send neutral values.

Two settings (§5): **EQ** 4-band · 3-band and the existing **Mixer view**
Simple · Advanced, which applies to both EQ layouts.

| Engine value | 4-band (Simple, Advanced as today) | 3-band Simple | 3-band Advanced |
|---|---|---|---|
| `EqHi`, `EqMid1`, `EqMid2`, `EqLo` | stored 4-band positions | 0.5 | 0.5 |
| `Eq3Hi`, `Eq3Mid`, `Eq3Lo` | 0.5 | stored 3-band positions | stored 3-band positions |
| `Fx` (per channel) | 0.5 | stored | stored |
| FX select, PARAMETER | unchanged (no effect while every `Fx` is 0.5) | stored | stored |
| X and Y filter units | as today | off | off |
| FILTER assign | as today | defaults (1–2 X, 3–4 Y) | defaults |
| X-F assign | as today | off (THRU) | stored |
| Crossfader position, curve | as today | no effect (all THRU) | stored |

- Positions are kept per EQ layout: the 3-band and 4-band EQ positions are
  stored separately, since the same knob angle means different things.
  3-band starts flat: `Eq3*` and every `Fx` at 0.5, FILTER selected,
  PARAMETER 0.5.
- Switching EQ layouts ramps one EQ to noon and the other to its stored values
  in the same 10 ms, so the change sounds like turning knobs, never a
  click. The engine needs no EQ mode.
- Engine start state: everything at 0.5, so a single deck still plays
  through bit for bit (the existing graph test covers it once
  `ThreeBandEq` and the FX blocks are in the strip).
- Leaving 3-band while an echo or reverb rings: its knob goes to 0.5, so
  the tail rings out as it would on the DJM.

## 5. Open questions from the issue

- **Settings** (maintainer's call, 2026-10-23, following the wireframe in
  #58). Settings → Mixer gets a new **EQ** setting, **4-band · 3-band**,
  stored as `mix.mixer_eq` (`MixerEq { FourBand, ThreeBand }` in `reks-core`
  `settings.rs`, default 4-band; `mixer_eq` in `Settings` and an optional
  field in `SettingsPatch`, like `mixer_view`), next to the existing **Mixer view** Simple · Advanced, which
  applies to both. No third `MixerView` value. **Filters** Show · Hide stays
  for 4-band Simple only (disabled in 3-band and in Advanced). Knob rows in
  3-band: **HI · MID · LOW · FX**; the FX row's label shows the selected
  effect (FILTER, ECHO, REVERB) and a knob is lit when off centre. FX select
  (three buttons) and PARAMETER are one row in the mixer's shared section;
  the PARAMETER label follows the effect (RESONANCE, FEEDBACK, DECAY). No
  vendor names.
- **Crossfader.** By Mixer view, as in 4-band: Advanced adds the X-F
  assigns and the crossfader, Simple hides them.
  - 3-band Simple: no X-F assigns, no crossfader; every channel THRU.
  - 3-band Advanced: 3-band Simple plus the X-F assign row, the crossfader
    and its curve. No FILTER assigns and no X/Y row in either: the FX knob
    replaces them.
- **Mixer heights** (wireframe #58, commit b28ecaa, with the FX row; 2 decks
  at 1440×900 / 1280×800):

  | Layout | Mixer | Waveform | Library rows |
  |---|---|---|---|
  | 3-band Simple | ~333 px | ~119 px | 12 / 9 |
  | 3-band Advanced | ~393 px | ~179 px | 10 / 6 |

  With 4 decks the mixer is 440 px; its faders take up to 60 px of the
  spare height as fits (3-band Advanced +47), and any rest goes into row
  spacing.
- **Filter resonance.** The global PARAMETER knob while FILTER is
  selected, with the X/Y RESONANCE law (§3). No setting.
- **Shortcuts.** There are no filter or FX shortcuts today (`mixKeys.ts` has
  decks and the crossfader only), and v1 adds none. X C V follow the
  crossfader: they work in 3-band Advanced and do nothing in 3-band Simple,
  as in Simple today; the Shortcuts popover leaves them out there. The knob itself
  takes the shared knob keys (arrows) and double-click to centre.
- **Knob UI.** Centre detent: a drag that crosses 0.5 sticks there for
  ±0.03 of travel (the dead zone), the software form of CENTER LOCK.
