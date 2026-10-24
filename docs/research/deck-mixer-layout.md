# Deck quick-looping and deck/mixer layout (research, 2026-10-22)

Status: **proposal**. Nothing here is built. Concept A (Console) with the
simple and advanced mixer views was picked on 2026-10-23. It is broken down
as phase 5 tasks 19–25 ([`../phases/phase-5.md`](../phases/phase-5.md),
status proposed). The quick-loop scope needs a D21 amendment, recorded in
[`../decisions.md`](../decisions.md) as proposed.

- Wireframes (to scale; concept × 1/2/4 decks × 1280/1440/1920):
  <https://claude.ai/artifact/7CQDMaXzUJrWAKFzy1wZmu>. Source:
  [`wireframes/deck-mixer-layout.html`](wireframes/deck-mixer-layout.html).
- Wireframe for #56, the 3-band mixer (Simple and Advanced) next to Full and Simple
  (to scale, 2 and 4 decks, light and dark, maintainer answers):
  <https://claude.ai/artifact/5akPftFpBaHN3xjNzxSp8V>. Source:
  [`../wireframes/mix-view-56.html`](../wireframes/mix-view-56.html).
- Current layout: [`../ui.md`](../ui.md#mix-panel-phase-5) and phase 5 tasks
  9, 12–18.

## 1. Measurements of today's layout

Dev build with the dev mock (headless, DPR 2), 2026-10-22. "Library rows"
counts whole 26 px rows of the track table. The library toolbar and filter
bar above the table take 116 px.

| Window | Decks | Deck card | Waveform | Mixer | Deck section | Track table |
|---|---|---|---|---|---|---|
| 1280×800 | 1 | 1256 × 162 | ~793 × 72 | – | 170 | 434 px |
| 1280×800 | 2 | 441 × 260 | 362 × 72 | 358 × 493 | 496 (scroll cap) | 108 px, **3 rows** |
| 1280×800 | 4 | 441 × 260 | 362 × 72 | 358 × 493 | 496 of 528, **scrolls** | 108 px, **3 rows** |
| 1440×900 | 1 | 1416 × 162 | 953 × 72 | – | 170 | 534 px |
| 1440×900 | 2 | 521 × 260 | 442 × 72 | 358 × 493 | 501 | 203 px, 6 rows |
| 1440×900 | 4 | 521 × 260 | 442 × 72 | 358 × 493 | 536 | 168 px, 5 rows |
| 1920×1080 | 2 | 761 × 260 | 682 × 72 | 358 × 493 | 501 | 417 px |
| 1920×1080 | 4 | 761 × 260 | 682 × 72 | 358 × 493 | 536 | 382 px |

Inside the mixer at 1440×900: channel strips 58 × 363, legend 46, meters
26 px wide each (10 px LEDs plus 14 px labels, five copies of the scale),
knobs 22 (LEVEL) and 28 (EQ, FREQ), channel faders 108 px, crossfader track
**119 px**, filter units 90 × 95. Deck pads are 80 × 32 at 1440 and
60 × 32 at 1280.

## 2. Findings

Pain points, from the code and the measurements above.

**Loops**

1. Loop controls exist only in the LOOP pad mode
   ([`DeckPads.tsx:369`](../../src/components/mix/DeckPads.tsx)) and on
   Shift F / Shift L ([`mixKeys.ts`](../../src/lib/mixKeys.ts)). To change a
   loop while hot cues are showing, you switch modes (or press Shift S /
   Shift J). Outside LOOP
   mode the only sign of a loop is the band on the waveform.
2. There is no loop length readout. The pads cover 1/8…32
   ([`mixView.ts:179`](../../src/lib/mixView.ts)), but the engine resizes to
   1/64…512 (`LOOP_MIN_BEATS` / `LOOP_MAX_BEATS`,
   [`graph.rs:46`](../../crates/reks-mix/src/graph.rs)). A loop doubled past
   32 lights no pad ([`DeckPads.tsx:381`](../../src/components/mix/DeckPads.tsx)).
   An AUTO memory loop entered on a grid-less deck reports 0 beats
   ([`deck.rs:585`](../../crates/reks-mix/src/deck.rs)).
3. There are no halve/double controls. `LoopResize` is reachable only by
   pressing a different size pad (`loopPadControls`,
   [`mixView.ts:223`](../../src/lib/mixView.ts)).
4. There is no reloop. `LoopExit` drops the loop
   ([`deck.rs:314`](../../crates/reks-mix/src/deck.rs)), and seeking, cueing
   or calling a hot cue out of a loop drops it too (`leave_loop_unless`).
5. There are no manual loops (IN/OUT). Every loop needs a beatgrid: without
   one, `beat_loop` does nothing ([`deck.rs:541`](../../crates/reks-mix/src/deck.rs)),
   and neither does `resize_loop`, through `loop_end`
   ([`deck.rs:513`](../../crates/reks-mix/src/deck.rs)).
6. Calling a memory loop moves only the cue point and the head
   (`call_cue`, [`deck.rs:418`](../../crates/reks-mix/src/deck.rs)). The
   loop itself can't be entered from the call.

**Space**

7. In 2-deck mode, each deck card is 260 px tall next to a 493 px mixer, so
   **233 px under each deck is empty**, while the waveform gets 72 px
   ([`DeckStrip.tsx:237`](../../src/components/mix/DeckStrip.tsx),
   `--deck-h`, [`tokens.css:280`](../../src/styles/tokens.css)).
8. At 1280×800 the panel cap (`--mix-panel-max-h: 62vh`,
   [`tokens.css:301`](../../src/styles/tokens.css),
   [`MixPanel.tsx:60`](../../src/components/mix/MixPanel.tsx)) leaves the
   track table **3 rows**. In 4-deck mode the decks (528 px) scroll inside a
   496 px panel, which hides the bottom of decks 2 and 4.
9. The deck height is fixed whatever space is free. At 1920×1080 the
   waveform is still 72 px tall.

**Mixer**

10. The crossfader is 119 px, shorter than a channel fader. It gets only the
    width left between the two 90 px filter units
    ([`MixerPanel.tsx:60`](../../src/components/mix/MixerPanel.tsx)). It is
    the main control for auditioning a transition.
11. The dB scale is printed five times: each channel meter and the master
    meter carry their own labels
    ([`ChannelStrip.tsx:86`](../../src/components/mix/ChannelStrip.tsx),
    `Meter`). Meanwhile the legend column's fader row is empty
    ([`ChannelStrip.tsx:105`](../../src/components/mix/ChannelStrip.tsx)).
12. Row captions sit only left of channel 1, four strips from channel 4.
    Knob rows are as tall as the knobs (`--mixer-row-h` 28 = `--knob-md`
    28), so the eye has no gutter to follow a row.
13. Filter FREQ, the filter unit's main control, is the same 28 px as an EQ
    knob ([`FilterUnit.tsx:52`](../../src/components/mix/FilterUnit.tsx)).
14. The channel number is a plain caption. Nothing ties channel N visually
    to deck N's lit badge.

**Modes**

15. **A deck plays unseen.** Switching from 4 decks to 2 or 1
    ([`PlayerBar.tsx:43`](../../src/components/PlayerBar.tsx)) leaves decks
    1/4 (or 1/3/4) playing with no deck shown. This was reproduced with the
    dev mock: deck 1 kept playing after switching to 2 decks and then to
    1 deck.
16. In 1-deck mode, channel 2's EQ, filter assign, fader and the master
    still apply, but none of them are visible. A killed LO left from the Mix
    panel makes the player sound wrong for no visible reason.
17. In 1-deck mode the deck keys are off
    ([`App.tsx:68`](../../src/App.tsx), `useMixShortcuts(mixOpen)`), so
    the loop, jump and hot cue keys do nothing on the player deck. Only Space
    and − / + work.

**Small**

18. In HOT CUE mode the page picker stays, disabled, so the row doesn't
    shift ([`DeckPads.tsx:133`](../../src/components/mix/DeckPads.tsx)).
    It costs 56 px of pad-tab width on every deck. This is acceptable and
    kept.
19. Q is 20 px (`xs`) above 32 px CUE / PLAY, and the nudge buttons are
    20 px. Both have keys (nudge) or are rarely used (Q), so this is fine.

What already works well and stays in every concept:

- The header reads in the order you check a track: title, then key, BPM,
  then time.
- CDJ-style tempo column on the right edge.
- MASTER and SYNC next to the tempo column.
- One deck design for 2 and 4 decks.
- Deck N on channel N's side of the mixer.
- A state for every slot (empty, loading, analysing, error).

## 3. Conventions in other software and hardware

All sources accessed 2026-10-22. Dates are the document's own, or
"undated".

| Product | Always-visible loop controls | Notes |
|---|---|---|
| rekordbox 7.2.18 Performance mode | JOG panel toggles **AU** (`<` size `>`; click the size to loop and again to exit) and **MA** (IN, OUT, RELOOP/EXIT) | Auto loops 1/64–512 beats, starting at the playhead. IN snaps to the grid with quantize. Halve/double keep the loop-in point. Fractional loops disable BEAT SYNC. Beat jump moves a loop. Default keys: Loop In D/K, Out F/L, Exit/Reloop G/;, auto loop Shift F/L, halve/double Shift D/G (deck 1). [RB] p.100, 130–135, 159–167; [RBK] |
| Traktor Pro 4 | Size selector with arrows, IN, OUT, ACTIVE, on every deck | Sizes 1/32–32 beats, set at the playhead. ACTIVE with no loop enables the next stored loop. Markers stay after a loop is disabled. SNAP puts loops on the nearest beat. The MOVE page moves a loop or its in/out point. No default keys. [TP4] PDF p.21–22, 31, 37, 73–75 |
| Serato DJ Pro | Loops tab (not always visible) | Auto loops snap to the nearest beat and need a grid. Pressing IN or OUT during a loop edits that point. Keys: in O/L, out P/;, on/off [ / ', reloop Alt+[ / Alt+'. [S-Auto], [S-Manual] |
| Engine DJ 5.1 desktop | Loop & Beat Jump deck mode: value, /2, X2, Auto Loop, IN, OUT, Enable, Re-Loop, shift | Optional "Smart Loops" rounding. Default loop size can be None (keep the last). [END] p.6–7, 20, 43 |
| SC6000 / SC5000 | Auto Loop knob (turn = size, press = loop, turn while looping = halve/double, Shift+turn = move), IN, OUT | No dedicated reloop. [SC] p.39 |
| CDJ-3000 | 4 BEAT (1/2X), 8 BEAT (2X), IN/CUE (IN ADJUST), OUT (OUT ADJUST), RELOOP/EXIT, CUE/LOOP CALL ◀ ▶ | IN or OUT during a loop adjusts that point. RELOOP/EXIT exits, then restarts from IN. Beat jump during a loop moves it. Quantize snaps IN/OUT. Not documented: reloop with no loop, sync during loop moves, loop limits. [CDJ] p.14, 21, 54–64 |

On layout, rekordbox offers 2/4-deck Horizontal (full-width waveforms) and
Vertical layouts, with a mixer panel toggle that hides automatically when a
controller is connected ([RB] p.128, 146, 173, 178). Traktor has a Parallel
layout with waveforms stacked full width, a Preparation layout with one big
deck, an Essential layout that hides the mixer, and a preference to show or
hide mixer groups ([TP4] PDF p.11–13). Serato has no full on-screen mixer;
Practice mode adds only faders, meters and a crossfader. Engine DJ desktop
has no mixer at all ([END] p.5–6). So **a collapsible or hidden mixer is the
norm in software**. reks's full Xone-style mixer is the exception, by design
(D21).

Hardware channel order is as follows. Xone:92: LEVEL, 4-band EQ, FILTER
assign, XFADE assign, CUE, 9-LED meter, 60 mm fader ([X92] p.13–15).
DJM-900NXS2: TRIM, EQ, meter, COLOR, CUE, fader, crossfader assign ([N2]
p.7–8). DJM-A9: TRIM, EQ, COLOR, CUE A/B, fader, crossfader assign ([A9]).
Two things are not verified, because the panel diagrams could not be read:
whether the Xone manual's numbering is the physical top-to-bottom order, and
where the master section sits.

No published usability guidance on waveform height or knob size for DJ
software was found. An RWTH Aachen thesis on waveform design
(<https://almeida.informatik.rwth-aachen.de/publications/burger2013.pdf>)
could not be fetched.

Patterns taken: an always-visible loop strip with a size readout, ½/2×, IN,
OUT and RELOOP (Traktor, rekordbox MA, Engine). IN/OUT during a loop edits
that point (Serato, CDJ adjust). Reloop on the CDJ's exit/re-enter model.
A mixer that can fold (rekordbox, Traktor). Full-width stacked waveforms
(rekordbox Horizontal, Traktor Parallel) as concept B. No look is copied.

Sources:

- [RB] rekordbox 7.2.18 manual, <https://cdn.rekordbox.com/files/20260807093645/rekordbox7.2.18_manual_EN.pdf> (2026-08-04)
- [RBK] rekordbox 7.0.5 default keyboard shortcuts, <https://cdn.rekordbox.com/files/20241203185020/rekordbox7.0.5_default_keyboard_shortcut_reference_EN.pdf> (2024-12-03)
- [CDJ] CDJ-3000 manual DRI1586A, <https://support.pioneerdj.com/hc/en-us/articles/4404617347865>; read from the mirror <https://cdn01.4wall.com/cms/rentals/files/f6835e4eabc543.pdf> (2020-06-16)
- [TP4] Traktor Pro 4 manual, <https://docs.native-instruments.com/pdf-guides/traktor/Traktor-Pro-4-Manual-English-170724.pdf> (2024-07-17)
- [END] Engine DJ 5.1.0 user guide, <https://cdn.inmusicbrands.com/engine/51/Engine%20DJ%20-%20User%20Guide%20-%20v5.1.0.pdf> (2026-08-19)
- [SC] SC6000/SC5000 PRIME user guide 5.0.0, <https://cdn.inmusicbrands.com/Software/ENDJ5/SC6000%20PRIME%2C%20SC6000M%20PRIME%2C%20SC5000%20PRIME%2C%20SC5000M%20PRIME%20-%20User%20Guide%20-%20v5.0.0.pdf> (2026-05-06)
- [S-Auto] <https://support.serato.com/hc/en-us/articles/226382007-Auto-Looping>; [S-Manual] <https://support.serato.com/hc/en-us/articles/226514068-Manual-Looping>; Display modes <https://support.serato.com/hc/en-us/articles/225122768-Display-Modes>; Practice mode <https://support.serato.com/hc/en-us/articles/360001274635-Practice-mode> (all undated)
- [X92] Xone:92 Limited Edition user guide, issue 2, <https://www.allen-heath.com/content/uploads/2026/07/Xone92-Limited-Edition-UG-iss_2.pdf> (2026-07-02)
- [N2] DJM-900NXS2 manual DRI1300A, <https://downloads.support.alphatheta.com/manuals/DJM_900NXS2_DRI1300A_manual.pdf> (2024-08-30)
- [A9] DJM-A9 manual, <https://support.pioneerdj.com/hc/en-us/articles/15916598774553>; read from the mirror <https://cdn01.4wall.com/cms/rentals/files/f6835eb526b3d2.pdf> (2024-03-28)

## 4. Layout concepts

Each concept is drawn at 1, 2 and 4 decks in the wireframes, with
rationale, pros and cons, quick-loop placement, behaviour at 1280×800 and
the controls that move. One line each here:

- **A · Console**: today's model, decks around a central mixer, with the
  mixer re-laid to 440 px. Two decks grow to the mixer's height (waveform
  72 → 226 px); four decks stack in the same height. The quick-loop row sits
  under the overview, or a 3 × 2 block beside the pads.
- **B · Lanes**: every zoomed waveform becomes a full-width lane on top
  (1416 px), sharing one playhead line. Deck controls without waveforms and
  a wide mixer sit below.
- **C · Docked**: decks are full-width rows, the shape of today's player
  deck, stacked 1–4. The mixer docks at the right and can fold to a rail
  with faders, cues, meters, master and crossfader.
- **D · Columns**: each deck stands above its own channel, as
  1 | 2 | master, filters and crossfader | 3 | 4.

Computed from the frames, with the Advanced mixer unless stated:

| | Today | A | B | C | D |
|---|---|---|---|---|---|
| Section height, 2 / 4 decks (1440×900) | 501 / 536 | 448 / 448 | 550 / 558 | 422 / 422 | 510 / 510 |
| Library rows, 2 / 4 decks (1440×900) | 6 / 5 | 8 / 8 | 4 / 4 | 9 / 9 | 6 / 6 |
| Library rows, 2 / 4 decks (1280×800) | 3 / 3 | 5 / 5 | 1 / 0 | 6 / 6 | 2 / 2 |
| Library rows, 2 decks, Advanced / Simple / Simple without filters (1440×900) | 6 | 8 / 12 / 14 | 4 / 8 / 8 | 9 / 12 / 13 | 6 / 6 / 6 |
| Same at 1280×800 | 3 | 5 / 8 / 10 | 1 / 4 / 4 | 6 / 9 / 10 | 2 / 2 / 2 |
| Waveform, 2 decks (1440) | 444 × 72 | 451 × 226 (Simple 142, no filters 78) | 1416 × 72 | 459 × 121 | 411 × 106 |
| Waveform, 4 decks (1440) | 444 × 72 | 451 × 66 | 1416 × 44 | 275 × 65 (469 folded) | 199 × 64 |
| Waveform, 4 decks (1280) | 364 × 72 | 371 × 66 | 1256 × 44 | 115 × 65 (309 folded) | 159 × 64 |
| Crossfader | 119 | 234 | 172 | 234 | 166 |
| Loop reach | LOOP pad mode or Shift F | 1 click / key | 1 click / key | 1 click / key | 1 click / key |
| Mixer readability | fair | good | good | good open, partial folded | mixed |
| Cost | – | M | L | M–L | L |

**Recommendation: A.** It solves the measured problems: dead space in
2-deck mode, the library squeezed at 1280×800, the short crossfader, the
repeated scale and loop reach. It keeps the hardware picture and today's
components, and it is the cheapest. B gives the best waveforms but costs the
library more than today does. C gives the most library but narrows the
4-deck waveforms below today's unless the mixer is folded. D's 4-deck
columns are too narrow. If width for waveforms matters more later, C's
fold-to-rail can be added to A as a separate step (mixer in place,
collapsed to the rail). The Simple mixer view (§6) helps A most: in
2-deck mode the decks follow the mixer's height, so the library gets
12–14 rows at 1440×900.

Controls behind a toggle or hover: in C's folded mixer (trim, EQ,
filters, assigns, curve, cue section), only when the user folds it, since
open is the default; and in the Simple mixer view (§6), only by the
setting, with the hidden controls held at values that change nothing. Zoom − / + stays on hover/focus as today, with
keys in every concept.

### Inventory: every control and where it goes

From `src/components/mix/*.tsx` and `PlayerBar.tsx` (2026-10-22). Nothing is
removed. **new** = added, _moved_ = different place from today,
**hidden by a setting or toggle** = not on screen in C's folded mixer or in
the Simple mixer view (§6).

| Control or readout | Today | A Console | B Lanes | C Docked | D Columns |
|---|---|---|---|---|---|
| **Deck header** | | | | | |
| Deck number (lit when loaded) | Header, left | Same | Deck block header; repeated as the lane tag | Header (2 decks); info column (4 decks) | Column header |
| Artwork; eject on hover/focus | Header | Same | Deck block header | Header; info column (4 decks) | Column header |
| Title / artist | Header | Same | Deck block header; title also on the lane | Header; info column (4 decks) | Column header |
| Key readout | Header | Same | Deck block header | Header; info column (4 decks) | Column header |
| BPM (double-click to type) + tempo % | Header | Same | Deck block header; BPM on the lane tag | Header; info column (4 decks) | Column header |
| Remaining / elapsed time | Header, right | Same | Deck block header | Header; info column (4 decks) | Column header |
| **Waveforms** | | | | | |
| Zoomed waveform (drag to skim, Cmd/Ctrl+scroll) | Under header, 72 px | Same, 2 decks ~220 px tall | Full-width lane on top _moved_ | In the row, between transport and loop block _moved_ | In the column |
| Marks: hot cues, memory, cue point, loop band | On both views | Same; adds pending IN mark | Lane and overview | Same | Same |
| Zoom − / + and level (hover, focus) | Waveform top right (hover) | Same (hover) | Lane top right (hover) | Same (hover) | Same (hover) |
| Sync phase meter (followers) | Waveform top edge | Same | Lane top edge | Same | Same |
| Warning line (no grid, grid pending, errors) | Waveform bottom edge | Same | Lane bottom edge | Same | Same |
| Overview (keyboard seek slider) | Under the waveform | Same | Deck block, under the header _moved_ | Under the waveform | Under the waveform |
| Empty / loading / analysing / error with Retry, Eject | Waveform slot | Same | Lane and deck block | Same | Same |
| **Transport** | | | | | |
| Q (quantise) | Over CUE | Same | Same | Same; over CUE·PLAY in 4-deck rows | Same |
| CUE (hold to preview) | Bottom left, round | Same | Same | Same | Same |
| PLAY / PAUSE | Bottom left, round | Same | Same | Same; beside CUE in 4-deck rows | Same |
| **Pads** | | | | | |
| Pad mode tabs HOT CUE · MEMORY · JUMP · LOOP | Over the pads | Same | Same | Same | Same |
| Page picker ◀ n/m ▶ | Right of the tabs | Same | Same | Same | Same |
| 8 pads (hot cues A–H, memory, jump, loop sizes) | 2 × 4 | Same | 2 × 4; 1 × 8 in 1-deck | Same | Same |
| Pad rename (double-click), × delete, memory right-click menu | On the pads | Same | Same | Same | Same |
| **Loops** | | | | | |
| Beat loop sizes (LOOP pad mode) | Pads | Kept | Kept | Kept | Kept |
| Quick loop: IN · OUT · ½ · size · 2× · LOOP · RELOOP | None (Shift F / Shift L only) | Row under overview; block beside pads **new** | Row in deck block **new** | Block beside pads **new** | Row under overview **new** |
| **Sync** | | | | | |
| MASTER | Right of the pads | Same | Same | Same | Bottom row of the column |
| SYNC | Right of the pads | Same | Same | Same | Bottom row of the column |
| **Tempo** | | | | | |
| Tempo range ±6 / ±10 / ±16 / WIDE | Tempo column top | Same | Same | Same | Same |
| M (master tempo, disabled) | Tempo column top | Same | Same | Same | Same |
| Tempo fader (detent, double-click resets) | Tempo column, full height | Same | Same | Same, row height | Deck part of the column |
| Nudge − / + (hold) | Tempo column bottom | Same | Same | Same | Same |
| Drop a track onto a deck | Whole deck card | Same | Deck block and its lane | Whole row | Whole column |
| **Mixer channel (× 4)** | | | | | |
| Channel number | Strip top, caption | Lit badge like the deck's | Same as A | Same as A | Block header |
| LEVEL (trim) | Strip row 1 | Same | Same | Same; folded: behind toggle **hidden by a setting or toggle** | Knob row of the block |
| EQ HI · MID 1 · MID 2 · LO | Strip rows 2–5 | Same | Same | Same; folded: behind toggle **hidden by a setting or toggle** | Knob row of the block |
| FILTER assign X · off · Y | Strip row 6 | Same | Same | Same; folded: behind toggle **hidden by a setting or toggle** | Switch row of the block |
| CUE (Shift+click adds) | Strip row 7 | Same | Same | Same; also on the rail | Switch row of the block |
| Channel fader | Strip | Same, 96 px | Same | Same; also on the rail | Block, bottom |
| Channel meter (9 LEDs) | Beside the fader, dB labels on each | Beside the fader; one dB scale in the legend _moved_ | Same as A | Same as A; rail too | Beside the block's fader |
| X-F assign X · off · Y | Strip bottom | Same | Same | Same; folded: behind toggle **hidden by a setting or toggle** | Switch row of the block |
| Row captions (legend) | Left of channel 1 | Same, plus the dB scale | Same | Same | Above each block's knobs |
| **Master and cue** | | | | | |
| Master level | Master column | Same | Same | Same; also on the rail | Centre column |
| Master meter | Master column | Same | Same | Same; also on the rail | Centre column |
| Cue level | Master column | Same | Same | Same; folded: behind toggle **hidden by a setting or toggle** | Centre column |
| Cue / Mix | Master column | Same | Same | Same; folded: behind toggle **hidden by a setting or toggle** | Centre column |
| Split | Master column | Same | Same | Same; folded: behind toggle **hidden by a setting or toggle** | Centre column |
| Post EQ | Master column | Same | Same | Same; folded: behind toggle **hidden by a setting or toggle** | Centre column |
| **Filters X and Y** | | | | | |
| ON | Beside the crossfader | Filter row above the crossfader _moved_ | Mixer outer edges _moved_ | As A; folded: behind toggle **hidden by a setting or toggle** | Centre column _moved_ |
| FREQ | Beside the crossfader, 28 px | Filter row, 32 px _moved_ | Mixer outer edges, 32 px _moved_ | As A; folded: behind toggle **hidden by a setting or toggle** | Centre column _moved_ |
| RES | Beside the crossfader | Filter row _moved_ | Mixer outer edges _moved_ | As A; folded: behind toggle **hidden by a setting or toggle** | Centre column _moved_ |
| HP · BP · LP (Shift+click combines) | Beside the crossfader | Filter row _moved_ | Mixer outer edges _moved_ | As A; folded: behind toggle **hidden by a setting or toggle** | Centre column _moved_ |
| **Crossfader** | | | | | |
| Crossfader | Between the filter units, 119 px | Across the strips _moved_ | Under the strips _moved_ | Across the mixer; on the rail when folded | Centre column bottom _moved_ |
| Curve | Under the crossfader | Right end of the crossfader row _moved_ | Same as A | Same as A; folded: behind toggle **hidden by a setting or toggle** | Centre column |
| **Player bar** | | | | | |
| 1 deck / 2 decks / 4 decks | Player bar left | Same | Same | Same | Same |
| Status line and errors | Player bar | Same | Same | Same | Same |
| Shortcuts popover | Player bar (2/4 decks) | Same | Same | Same | Same |
| Start / Stop output | Player bar | Same | Same | Same | Same |
| Volume | Player bar | Same | Same | Same | Same |
| Output device | Player bar right | Same | Same | Same | Same |
| **Mixer view (new, every concept)** | | | | | |
| Mixer settings button ⚙ (opens Settings at Mixer) | None | Mixer top-left corner **new** | Mixer top-left corner **new** | Mixer top-left; rail top-right **new** | Centre column **new** |
| Settings → Mixer: view Simple / Advanced, filters Show / Hide | None | Settings dialog, Mixer tab **new** | Settings dialog **new** | Settings dialog, Mixer tab **new** | Settings dialog **new** |
| FILTER assign, X-F assign, crossfader, curve in Simple view | Always shown | Hidden; defaults apply **hidden by a setting or toggle** | Hidden; defaults apply **hidden by a setting or toggle** | Hidden; defaults apply **hidden by a setting or toggle** | Hidden; defaults apply **hidden by a setting or toggle** |
| Filter units with Filters: Hide | Always shown | Hidden; both off **hidden by a setting or toggle** | Hidden; both off **hidden by a setting or toggle** | Hidden; both off **hidden by a setting or toggle** | Hidden; both off **hidden by a setting or toggle** |
| **New in every concept (fixes)** | | | | | |
| Deck playing out of view (e.g. deck 1 in 2-deck mode) | Nothing shows it | Chip in the player bar: 'Deck 1 playing' **new** | Same **new** | Same **new** | Same **new** |
| Channel 2 not flat in 1-deck mode | Nothing shows it | Chip in the player deck header **new** | Same **new** | Rail shows channel 2 **new** | Channel 2 block shown **new** |

Single-deck view: it exists today. **1 deck** in the player bar shows
`PlayerDeck` (deck 2, `DeckStrip solo`), and **2 decks** / **4 decks** go
back. No concept changes how you enter or leave it.

## 5. Quick-loop spec

### UI

A block of seven controls on every deck, in every pad mode and on the 1-deck
player:

```
[IN] [OUT]   [½] [ 4 beats ] [2×]   [LOOP] [RELOOP]
```

It is a 24 px row where the deck has height to spare (A 2-deck, B, D), and a
3 × 2 block (`--loop-block-w` 104 px) beside the pads where it doesn't (A
4-deck and 1-deck, C). The block needs a grid for LOOP and for ½/2× of the
next size. IN, OUT and RELOOP work without one. Disabled controls stay
visible, as on the rest of the deck.

| Control | Not looping | Looping |
|---|---|---|
| IN | Sets a pending loop-in at the playhead (nearest beat with Q), drawn as a mark on both waveforms; lit while pending | Moves the loop's start to the playhead (nearest beat with Q), end kept; ignored if it would leave a loop under 8 ms |
| OUT | With a pending IN behind the head: loop IN → playhead (nearest beat with Q), looping at once. Disabled without a pending IN | Moves the loop's end to the playhead (nearest beat with Q), start kept |
| ½ / 2× | Halve or double the size LOOP uses next (`DeckMeta.lastLoop`, 1/64 … 512); the readout dims | Halve or double the loop, start kept (`LoopResize`) |
| Size readout | The next size, dimmed ("4 beats") | The active loop's length: beats ("1/16", "64", "3.87" for an unquantised manual loop), or seconds without a grid |
| LOOP | Loop of the shown size from the playhead (`BeatLoop`; quantised start as today) | Leaves the loop, keeping it for RELOOP. Lit while any loop plays: beat, manual, memory or AUTO |
| RELOOP | Jumps back into the last loop (or a memory loop just called) and loops. Disabled when there is none | Same as LOOP: leaves it, keeping it |

The LOOP pad mode stays as it is. A loop size picked there also becomes the
quick-loop size. That is today's "last size" behaviour, so the pads and the
block agree.

New shared pieces:

- `LoopBlock` in `src/components/mix/`, with `row` and `grid` layouts, built
  from `Button variant="control"`.
- A `Readout` in `src/components/ui/` for a small mono value box with a
  dimmed state. The BPM, key and loop size all want one.

### Engine (`reks-mix` `Control`, IPC `MixControl`, dev mock)

Existing commands: `BeatLoop { beats }`, `LoopExit`,
`LoopResize { double }`, `BeatJump` (moves an active loop), `AutoLoop`,
`CallCue { frame }`, `Quantise`.

New commands and changes:

| Command | Shape | Behaviour |
|---|---|---|
| `LoopIn` | `{ deck }` | Not looping: sets `loop_in` at the head (Q: `beats().snap`, like a cue point). Looping: moves the loop's start there, keeping its end. Without a grid it uses the exact head |
| `LoopOut` | `{ deck }` | With `loop_in` before the head: loop `[loop_in, out)`, where `out` is the head (Q: snapped; if that is ≤ IN, IN + 1 beat). Looping starts at once: the head wraps when it reaches `out`, or now if snapping put `out` behind it (existing `wrap_loop`). While looping, it moves the loop's end. Ignored when the loop would be shorter than 2 × `LOOP_XFADE_MS` (8 ms), past the track, or there is no IN |
| `Reloop` | `{ deck }` | Looping: as `LoopExit`. Not looping: if there is a `last_loop`, jump to its start and loop. With Q while playing, the jump keeps the beat phase (`keep_phase`, as hot cues do), which lands inside the loop for loops of a beat or more. Under a beat it lands on the start |
| `CallCue` | `{ deck, frame, end: Option<i64> }` (was `{ deck, frame }`) | A memory loop call also sets `last_loop` to `(frame, end)`, so RELOOP enters it. The call itself does not start looping, as rekordbox's memory loop call pauses on it ([RB] p.164) |
| `LoopResize` | unchanged | Without a grid it halves or doubles by time (frames). Today it is ignored |

State:

- Every exit (LoopExit, LOOP, RELOOP, a seek/cue/hot cue out of the loop)
  keeps the loop as `last_loop`. Load and eject clear `loop`, `loop_in` and
  `last_loop`.
- `DeckState` adds `loop_in: Option<i64>` and
  `last_loop: Option<LoopRegion>`. `LoopRegion.beats` becomes
  `Option<f64>` (none without a grid; today an AUTO loop without a grid
  reports 0). IPC: `loopIn`, `lastLoop`, both in ms.

Interactions:

- **Sync.** Existing rules hold: a looping follower keeps tempo only, a
  master looping under a beat gives no phase, and part of a bar gives beat
  phase only. New: a master playing a loop whose length is not a whole
  number of beats (within 1/64) gives no phase, so a manual unquantised
  loop never drags followers.
- **Quantise.** IN and OUT snap to the nearest beat. LOOP starts on the last
  grid step, as now. RELOOP keeps phase while playing.
- **Beat jump.** It moves an active loop, as now. A pending IN stays where
  it is (it is a place in the track). Jumping before it makes OUT ignored
  until IN is set again.
- **Memory loops.** MEM while looping saves the loop as now (manual loops
  included). A called memory loop arms RELOOP. AUTO loops entered by
  playback become `last_loop` when left.
- **Paused.** LOOP while paused sets the loop at the head, as now. OUT while
  paused makes the loop and moves the head to IN, so PLAY starts the loop.
  RELOOP while paused moves the head to the loop start.
- **Track end.** Loops never run past the track (existing `loop_end`). OUT
  at the end clamps to the last frame.
- **Out of scope** (D21 needs a decision first): slip, loop roll, emergency
  loop, IN/OUT adjust by jog.

Tests:

- Render tests for IN/OUT with and without Q and a grid; reloop phase; a
  sync master with a non-integer loop giving no phase; a memory call arming
  RELOOP; LoopResize without a grid.
- The engine's real-time test sends the new controls; no allocation.
- `mixKeys.test.ts`, `mixView.test.ts` (readout format), `DeckStrip.test.tsx`
  (block states).

UNVERIFIED against a CDJ (hardware checks deferred): reloop with no loop,
IN/OUT edit while looping, and behaviour when paused.

### Shortcuts

Since 2026-10-23 the deck keys follow rekordbox's default "Performance 1"
layout (`mixKeys.ts`). The loop keys take rekordbox's own loop keys from the
same layout ([RBK]); none are in use today.

| Action | Deck 2 | Deck 3 | rekordbox P1 |
|---|---|---|---|
| LOOP on/off (today) | Shift F | Shift L | Auto beat loop |
| Loop IN · OUT | D · F | K · L | Loop In · Loop Out |
| RELOOP / exit | G | ; | Exit/Reloop |
| Loop ½ · 2× | Shift D · Shift G | Shift K · Shift ; | Halve · Double loop |

Option moves each key to deck 1 / 4, as for the other deck keys.
`ShortcutHelp`, `ui.md` and the tooltips all read from `mixKeys.ts`, as now. Decided 2026-10-23: the deck-2
(left-hand) keys work in 1-deck mode too (finding 17 described them as off),
except SYNC, MASTER and the crossfader, which the player deck lacks.
None of these keys conflict with library keys (checked `keys.ts`); the Space
and − / + rules stay.

## 6. Simple and advanced mixer views

Maintainer's request, 2026-10-23. It applies to every concept and changes no
DSP: the engine is the same in both views. Wireframes: **Mixer view** in the
toolbar, and the "Simple and advanced mixer" section.

**Settings → Mixer**, a new tab in the Settings dialog's sidebar after Decks:

| Setting | Values | Default | Stored as |
|---|---|---|---|
| Mixer view | Simple · Advanced | Simple (proposed: nothing audible changes, since every crossfader assign starts at THRU) | `mix.mixer_view`, `MixerView` in `reks-core` `settings.rs` |
| Filters | Show · Hide | Show; disabled in Advanced, which shows everything | `mix.mixer_filters`, `MixerFilters` |

What each view shows:

| Control | Advanced | Simple | Simple, filters hidden | What the engine gets while hidden |
|---|---|---|---|---|
| FILTER assign X · off · Y | shown | hidden | hidden | The defaults: channels 1–2 on X, 3–4 on Y |
| X-F assign X · off · Y | shown | hidden | hidden | Off (THRU) on every channel |
| Crossfader and curve | shown | hidden | hidden | No effect, since nothing is assigned |
| Filter X and Y (ON, FREQ, RES, HP · BP · LP) | shown | shown | hidden | Both units off |
| Trim, EQ, CUE, faders, meters, master, cue section | shown | shown | shown | – |

**What you hear matches what you see.** The store keeps the positions the
user set (`MixStore.channels`, `filters`, `crossfader`). A pure
`effectiveMix(values, view)` in `lib/mixView.ts` maps them to what the
engine gets, following the last column above. The store sends the effective
values whenever a value or the view changes, and after each start (today it
sends every position after start). Switching back to Advanced sends the
stored values again, so an Advanced setup survives a trip to Simple. Assign
and ON switches already crossfade in the engine, so switching views doesn't
click.

Two alternatives were rejected:

- Resetting the hidden controls on the way into Simple: this loses an
  Advanced setup.
- Hiding them while they keep acting: this is the trap of findings 15–16, a
  sound with no visible cause.

**Keys.** X C V (crossfader) do nothing in Simple. The Shortcuts popover and
the tooltips leave them out.

**Settings button.** An `IconButton` (gear icon, `aria-label="Mixer
settings"`, hint "Mixer settings") sits in the mixer's top-left corner, in
the legend column's header cell. In C it is on the rail when folded; in D it
is in the centre column. It opens the Settings dialog on the Mixer tab, with
focus on Mixer view: `useSettings.setDialogOpen(true, "mixer")` picks the
tab (`Tabs` value). New shared piece: a gear icon in `icons.tsx`.

**Layout** (numbers from the frames, 1440×900 unless stated):

- **A.** The mixer is 440 / 356 / 292 px tall (Advanced / Simple / filters
  hidden).
  - 2 decks take the mixer's height. The waveform is 226 / 142 / 78 px
    tall, and the library shows 8 / 12 / 14 rows (5 / 8 / 10 at 1280×800).
  - 4 decks still need 440 px, so a shorter mixer stretches its faders
    instead.
- **B.** The side filter columns go when filters are hidden (mixer 520 →
  344 px wide). The library goes from 4 to 8 rows with 2 decks.
- **C.** The library shows 9 / 12 / 13 rows with 2 decks.
- **D.** The assign switches and crossfader drop out, but the column heights
  don't change.

**Tests.**

- `settings.rs`: round trip and defaults.
- `mixView.test.ts`: `effectiveMix` for every view.
- `mix.test.ts`: the engine gets the defaults in Simple and the stored
  values back in Advanced.
- `MixerPanel.test.tsx`: hidden rows, and the gear opening Settings on the
  Mixer tab.
- `mixKeys.test.ts`: X C V ignored in Simple.

## 7. Sizing rules (as tokens)

The proposal for concept A. The other concepts reuse the mixer and loop
rules.

| Token / rule | Today | Proposed | Why |
|---|---|---|---|
| `--mix-panel-max-h` | `62vh` | `calc(100vh - var(--app-chrome-h) - var(--library-min-h))` | Guarantees library rows instead of a percentage |
| `--library-min-h` (new) | – | 246 px (toolbar and filters 116 + header + 4 rows) | At least 4 rows at 1280×800 |
| `--mixer-h` (new) | 493 (content) | Advanced 440, Simple 356, Simple without filters 292 px | Decks size to the mixer |
| `--mixer-w` (new) | 358 | 344 px | One dB scale, narrower strips |
| `--mixer-strip-w` | 58 | 52 px | Meters without per-strip labels |
| `--mixer-row-h` | 28 | 26 px | Room on 26 px knobs, still one row per control |
| `--fader-len` | 108 | 96 px | Fits the 440 px mixer |
| `--knob-lg` (new) | – | 32 px | Filter FREQ |
| `--crossfader-min-len` (new) | 119 (actual) | ≥ 200 px, spans the strips | Main transition control |
| `--deck-h` | 260 | 2 decks: `var(--mixer-h)`; 4 decks: 216 px (440 px stack) | No dead space beside the mixer |
| Mixer fader stretch | – | A mixer shorter than the 4-deck stack gives up to 60 px to its faders; the rest stays empty at the bottom | Longer faders where there is room |
| `--deck-wave-h` | 72 | min 60 px; fills the rest of the deck | Space goes to the waveform first |
| `--deck-header-h` | 40 | 40; 30 in 4-deck | Fits 4 decks in 440 |
| `--deck-overview-h` | 20 | 20; 14 in 4-deck | Same |
| `--loop-row-h` (new) | – | 24 px | Quick-loop row |
| `--loop-block-w` (new) | – | 104 px | Quick-loop 3 × 2 block |
| `--deck-min-w` | 440 | 440 (unchanged) | A's decks are 448 at 1280 |
| `--pad-h` | 26 | min 26; grows with the bottom row | – |

Constraints:

- The waveform never drops below 60 px tall or 300 px wide. Where it would
  (C 4-deck at 1280, D 4-deck), that layout is not offered at that size.
- Knobs keep at least 2 px of row gutter.
- Every control stays at least `--ctl-h-xs` (20 px) for pointer use.
- Pads stay at least 52 × 26.

## 8. Decision impact

- The 3-band EQ layout (§9) adds blocks to the D21 mixer and changes none.
  Record it under D21 as proposed when it is built.

- D21 accepts beat loops as preparation and says "loop rolls, slip mode and
  saved loops still need a decision". Manual IN/OUT, reloop and
  halve/double are a scope extension. They are recorded in D21 as
  **proposed** (2026-10-22), pending the maintainer's answer.
- Simple and advanced mixer views (maintainer's request, 2026-10-23) hide
  parts of the D21 model without changing it, so they are not a conflict.
  They are noted under D21 with the spec in §6 as proposed.
- No other decision conflicts. D30 (1-deck player) and D26 (UI system) are
  unchanged. Concept C's folded rail would need a line in `ui.md` only.

## 9. Three-band EQ layout (issue #56)

An EQ layout setting, research 2026-10-23: 3-band gives a 3-band EQ (HI ·
MID · LOW), an FX knob per channel in the fourth knob slot, and an effect
selector with a PARAMETER knob, in place of the X/Y filters and FILTER
assigns. Mixer view Simple · Advanced (§6) applies to both layouts. Full engine spec, curves and tests:
[`djm-eq-fx.md`](djm-eq-fx.md). In short:

| | Value |
|---|---|
| Settings | new EQ 4-band · 3-band (`mix.mixer_eq`); Mixer view Simple · Advanced for both; Filters show/hide only in 4-band Simple |
| EQ | LOW shelf 250 Hz, MID bell 1 kHz (Q 0.7), HI shelf 3 kHz; −26 / +6 dB each (corners and Q **UNVERIFIED**) |
| FX knob (per channel) | centre off, dead zone 0.485–0.515; FILTER: left LP 20 kHz → 50 Hz, right HP 20 Hz → 10 kHz; ECHO: mids left, highs right; REVERB: mids+lows left, mids+highs right (figures **UNVERIFIED**) |
| FX select, PARAMETER | one for the mixer: FILTER · ECHO · REVERB; PARAMETER = resonance, feedback or decay |
| Hidden controls | 4-band EQ at noon, X/Y filters off, FILTER assigns default; in Simple also THRU, as in §6 |
| Crossfader, X C V | 3-band Advanced only |
| Mixer height (2 decks) | ~333 px Simple, ~393 px Advanced (wireframe #58) |
