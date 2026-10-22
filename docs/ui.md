# UI design system

How the React UI is styled. Why this stack: [D26](decisions.md#d26-ui-styling-tailwind-v4--cva--radix).
The look is adapted from a design analysis of Cursor's marketing site (pale
canvas, dark ink, hairline depth, one scarce accent, 8px control radius) at
desktop density, with system fonts and a derived dark theme. The reference's
warm cream and orange were swapped for an indigo accent on cool, faintly
indigo-tinted neutrals ("mist"). Orange failed AA contrast for small text on
cream (3.4:1). A violet accent on the warm cream looked muddy, because the two
hues are nearly opposite, so the neutrals now lean towards the accent's hue.
Red, pink, green, amber and aqua already mean something here (danger, success,
rating/warning, track colours). Indigo is 5.9:1 on the light canvas and 6.9:1
on the dark one, and white button text on it is 6.2:1.

## Where things live

| Path | What |
|---|---|
| [`src/styles/tokens.css`](../src/styles/tokens.css) | **The only place that sets the look**: palette, semantic colours (light + dark), type, radii, shadow, layers, motion, control sizes, row heights |
| [`src/styles/base.css`](../src/styles/base.css) | Our reset (instead of Tailwind preflight) and element defaults (body, `user-select: none` for chrome, focus outline) |
| [`src/styles/index.css`](../src/styles/index.css) | Entry: Tailwind, tokens, base |
| [`src/components/ui/`](../src/components/ui/) | Base components (one per file, each exports its `cva` variants) |
| [`src/components/ui/control.ts`](../src/components/ui/control.ts) | The control size scale and shared focus / disabled / hover states |
| [`src/lib/cn.ts`](../src/lib/cn.ts) | `cn()` = `clsx` + `tailwind-merge`; a later class wins |
| [`src/dev/mockBackend.ts`](../src/dev/mockBackend.ts) | Dev-only stand-in for the Rust commands: `npm run dev` and open the page in a browser to work on the UI without Tauri |

Feature components (`src/components/*.tsx`) do layout and composition only:
utilities for spacing and flex, base components for everything interactive,
semantic colour utilities (`bg-surface`, `text-fg-muted`, `border-border`).
No raw colours or one-off sizes outside `tokens.css`.

## Tokens

Three layers in `tokens.css`:

1. **Primitives** (`--mist-*`, `--indigo-*`, `--red-*`, `--rb-*` …) in plain
   `:root`. Raw values; no utilities. The only hex values in `src/`.
2. **Semantic tokens** in `@theme static`. These become utilities:
   `--color-surface` → `bg-surface`, `--radius-md` → `rounded-md`,
   `--shadow-overlay` → `shadow-overlay`. Tailwind's default palette, font
   sizes, radii and shadows are dropped, so `bg-blue-500` does not exist.
3. **Dark theme**: the semantic colours redefined under
   `@media (prefers-color-scheme: dark)`. Components never branch on theme.

Semantic colours: `bg`, `bg-subtle` (chrome), `row-alt`, `surface`,
`surface-strong`, `hover`, `selected`/`selected-fg`, `knob`, `fg`,
`fg-muted`, `fg-subtle`, `border`, `border-subtle`, `border-strong`,
`accent` (text, lines, focus ring), `accent-solid` (+ `-hover`, `-active`;
filled controls) with `accent-fg` on it, `danger` (+ `-fg`), `success`,
`warning`, `rating`, `focus-ring`, `overlay`, `lit`/`lit-fg` (a switch that
is on, with `shadow-lit`), `led-*` (meter LEDs), `waveform-*`, `track-*`
(rekordbox colours: data, the same in both themes). The grid editor draws on
its own dark deck in both themes: `deck`, `deck-fg`, `waveform-low/mid/high`
(3-band), `grid-beat`, `grid-downbeat`, `grid-cursor`; the mixer's decks add
`deck-cue`, `deck-hotcue` and the other waveform styles' `waveform-rgb-*` and
`waveform-blue(-high)`.

The indigo `accent` stays scarce: primary buttons, the loaded track, the played
waveform, focus rings, drop indicators. Selection and toggles that are on
use the neutral `selected` tint, except hardware-style switches in the Mix
panel (`variant="control"`), which light up in `lit` so their state reads at a
glance.

### Common edits (one line each in `tokens.css`)

- Accent colour: `--color-accent` and `--color-accent-solid` (+ `-hover`,
  `-active`), in the light block and again in the dark block. They are the
  same in light. In dark, `accent` is a light indigo (readable as text on the
  dark background) and `accent-solid` the deeper light-theme one, so white
  `accent-fg` passes AA on primary buttons (6.2:1). Neutrals are tinted
  towards the accent hue; if you change the accent's hue, re-tint `--mist-*`.
- Control radius: `--radius-md: 8px;` (sizes `md`/`lg` read it via `--ctl-r-*`).
- Height of `md` controls: `--ctl-h-md: 28px;`.
- Row height: `--row-h: 26px;` and `ROW_HEIGHT` in `trackTableModel.ts`
  (the virtualiser needs a number; `tokens.test.ts` fails if they differ).
- Slide timings: `--duration-slide-*` and `--ease-*`, mirrored in
  `src/lib/motion.ts` for tests (also checked by `tokens.test.ts`).
- Flat overlays (no shadow at all): `--shadow-overlay: none;`.

## Reset

`base.css` replaces Tailwind's preflight with a reset of the same intent
(`box-sizing`, zero margins, `border: 0 solid`, form controls inherit the
font, lists unstyled, media block). It leaves out preflight's universal
`::before, ::after, ::backdrop` selectors: they made every track-table row
pay for pseudo-element styles while scrolling (dev build, 20-row scroll
step: p50 8.9 → 8.3 ms). If a component needs a pseudo-element, give it
`before:`/`after:` utilities; nothing resets those.

Measured on the mock library (3,000 rows, 1280×800), production build, per
20-row scroll step including style and layout: p50 1.6 ms before and after
the redesign; p95 3.8–4.0 ms before, 2.5–3.1 ms after.

## Control size scale

Every interactive control takes `size` and reads `--ctl-*-{size}`, so
`Button`, `IconButton`, `Input`, `Select` and `SegmentedControl` of one size
have the same height (tested in `ui.test.tsx`).

| | xs | sm | md | lg |
|---|---|---|---|---|
| height | 20 | 24 | 28 | 32 |
| padding-x | 6 | 8 | 10 | 12 |
| font | 11 | 12 | 13 | 13 |
| icon | 12 | 14 | 16 | 16 |
| radius | 4 | 6 | 8 | 8 |
| gap | 4 | 4 | 6 | 8 |

Use: `xs` inside rows (tree "+", sheet close, key grid); `sm` in bars (app
header, library toolbar, filter bar, player bar, decks); `md` (default) in dialogs and
forms; `lg` for large standalone controls. Table and tree rows use `--row-h`,
menu items `--menu-item-h`.

## Components

| Component | Variants / props | Behaviour from |
|---|---|---|
| `Button` | `variant`: primary, secondary, outline, ghost, danger, quiet, link, control (lit when pressed); `size`; `aria-pressed` = toggle | Radix Slot (`asChild`) |
| `IconButton` | as Button (default ghost) + `shape`: square, round; `aria-label` required | Radix Slot |
| `Input` | `variant`: default, search; `size` | native |
| `Textarea` | `size`: sm, md | native |
| `Select` | `size`; native `<select>` with our chevron | native |
| `Switch` | `size`: sm, md | Radix Switch |
| `Checkbox` | `size`: sm, md; `indeterminate` (mixed); native, wrap in a `<label>` (Biome knows it as an input) | native |
| `Slider` | `size`: sm, md; one value | Radix Slider |
| `SegmentedControl` + `Segment` | `size`; `variant`: default, control (chosen segment lit); one choice, can't be cleared (radiogroup) | Radix ToggleGroup |
| `Chip` | removable label pill (`onRemove`, `removeLabel`) | – |
| `Toast` | bottom-centre message with an optional action (Undo); one at a time via `store/toasts` + `ToastHost` | – |
| `Badge` | `variant`: neutral, accent, muted; `size`: sm, md | – |
| `Kbd` | `variant`: default, inverse | – |
| `Tooltip` | `content`, `shortcut`, `side` | Radix Tooltip |
| `DropdownMenu*` | Content, Item (`variant`: default, danger), CheckboxItem, Separator | Radix DropdownMenu |
| `ContextMenu*` | Content, Item, CheckboxItem (`checked="indeterminate"` shows a dash), Sub / SubTrigger / SubContent, Group / Label, Separator; wrap the whole area, not each row | Radix ContextMenu |
| `Popover*` | Content (`align`, `sideOffset`) | Radix Popover |
| `Dialog*` | Content `size`: sm, md, lg, xl (960 px, the grid editor), `closeOnOutsideClick`; Header, Title, Description, Body, Footer | Radix Dialog |
| `Sheet` + `SheetHeader` | right-hand slide-over (non-modal, inside its parent) | Web Animations |
| `ScrollArea` | themed scrollbars; not for the virtualised table | Radix ScrollArea |
| `Separator` | `orientation` (decorative) | – |
| `Field` | `orientation`: vertical, horizontal, inline; `active`; `hint`, `error` | – |
| `Card` | `variant`: subtle, surface; `divided` | – |
| `Progress` | `size`: sm, md; native `<progress>` | native |
| `Swatch` | `color` (track colour name) or `style`; `size`: xs, sm, md | – |
| `Caption` | `tone`: muted, subtle, default; small upper-case label | – |
| `Knob` | `size`: sm, md; `bipolar` (arc from the centre), `defaultValue` (double-click / Delete); drag up/down, arrows, Page Up/Down, Home/End | own (`role="slider"`; Radix has no knob) |
| `Fader` | `orientation`: vertical, horizontal; `defaultValue` (double-click the cap) | Radix Slider |

Menus, popovers, dialogs and tooltips portal to `<body>`; tests query
`screen` / `document`, not the render container.

Performance rule for the track table: rows and cells are plain `div`s with
constant class strings; per-row state is `data-*` attributes styled with
`data-[selected]:…`. No base component, Radix or `cn()` call per cell.

### Split track lists

Shift+click a playlist or folder in the tree to open it in a pane under the
track table ([`TrackLists.tsx`](../src/components/TrackLists.tsx)); the lists
stack top to bottom and share the height. With more than one list, each has a
header (name, track count, × to close); closing the main list moves the first
pane up into it. Each pane is its own library store
([`panes.ts`](../src/store/panes.ts)) with its own sort and selection, handed
to `TrackTable` through `LibraryStoreContext`. Search, filters, the rule
builder and the info panel stay with the main list. A playlist shows in one
list at a time. Rows dragged from one list onto a playlist pane sorted by
position are added at the drop line. Edits that change rows reach every list
(`allLibraries()`). An (i) on the tree's Playlists row explains all this in a
tooltip.

## Start-up splash

The library opens off the main thread
([`src-tauri/src/startup.rs`](../src-tauri/src/startup.rs)); done in Tauri's
`setup` it blocked the event loop for seconds and the window sat blank white.
Now:

1. The window starts hidden (`"visible": false` in `tauri.conf.json`; the
   window-state plugin leaves visibility alone) and is shown on the page's
   first load, so the native white background never shows.
2. `index.html` holds the splash: the wordmark with a band of light sweeping
   across it ([`splash.css`](../src/styles/splash.css), theme tokens only, so
   it follows the system theme; a slow fade with reduced motion).
3. `main.tsx` awaits `appReady()` before mounting React, so no command runs
   before `AppState` exists. App's mount effect fades the splash out
   ([`src/lib/splash.ts`](../src/lib/splash.ts), at least 900 ms on screen so
   it never blinks). If the library cannot be opened, the splash stops and
   shows the reason instead.

There is no in-app theme setting yet; the app and splash follow the system.

## Player (D30)

The library plays on deck 2 of the mix engine ([`src/store/player.ts`](../src/store/player.ts)).
Along the bottom of the window:

- `PlayerBar`: the header row. Volume (`MixControl::Volume`, saved as
  `ui.state.volume`), output device (stored in Settings; a running output
  reopens on the new device with every loaded deck back where it was), the
  **Mix** toggle, and with the Mix panel off, why the output is not playing
  (opening, failed start, lost device).
- Under it, `PlayerDeck` (Mix off): deck 2 alone as a `DeckStrip` with
  `solo` (no SYNC, BAR, LEADER or phase marker), so it has the deck's
  waveforms, cue, hot cues, nudge, quantise and tempo fader. Empty, it says
  to double-click a track or drag one onto it. With Mix on, the Mix panel
  takes its place and shows the same deck among the others.
- Double-click or Enter on a row, and double-click in a history session,
  load the track on deck 2 and play it (it keeps playing if it already is);
  loading opens the output when it is off. Space plays and pauses deck 2
  wherever focus is (except in text fields, dialogs and menus). The row of
  the track on deck 2 is marked as loaded; the grid editor draws deck 2's
  playhead when it shows that track.

## Mix panel (phase 5)

[`src/components/mix/`](../src/components/mix/), opened with **Mix** in the
player bar (bottom edge), in place of the player deck. `MixPanel` is the only part that reads the store
([`src/store/mix.ts`](../src/store/mix.ts)); `DeckStrip`, `DeckWaveform`,
`MixerPanel`, `ChannelStrip`, `FilterUnit`, `MasterSection` and `SyncPhase`
take props. The mixer follows the hardware: four channel strips (level, HI,
MID 1, MID 2, LO, filter assign X/off/Y, cue, fader, crossfader assign) with
one caption column, the master and cue section, then filter X, the crossfader
and filter Y. Decks 1 and 2 sit left of it, 3 and 4 right. Two decks by
default, and they are decks 2 and 3, the centre channels, as with two CDJs
on a 4-channel mixer; 4 with the toggle (shorter waveforms). Deck cards are
as tall as their content (the columns no longer stretch them to the mixer's
height).

- States: output off (start form: device, rate, buffer, cue routing; the
  device's cue routing is preselected until the user picks one, and routings
  it has too few outputs for are disabled), opening,
  error (failed start, device lost), running. A deck is empty (how to load),
  loading, failed or loaded; a track without a grid says sync needs one.
- Waveforms draw on the dark `deck` like the grid editor: 8 s scrolling with
  the playhead in the middle, beat lines, cue (`deck-cue`) and hot cues
  (`deck-hotcue`); the overview under it is the keyboard seek slider.
  Drag to skim (`mix/useDeckScrub`, a wrapper around `DeckWaveform`, which
  only draws): on the overview the playhead follows the pointer and a press
  jumps there; on the zoomed view the track moves with the pointer (drag
  left to go forward). A playing deck pauses while dragged and plays on from
  the release point. Seeks are throttled to one per 30 ms, the last always sent.
- CUE is held, not clicked (pointer down/up, Enter/Space, and the A/J keys
  down/up): paused on the cue point it previews while held and returns on
  release; PLAY during the preview keeps it playing.
- Tempo range button cycles ±6, ±10 (default), ±16 and WIDE (±100%; the
  slow end stands still), as on a CDJ-3000.
- Waveform style (Settings → Decks, `display.deck_waveform`), applied to both
  views live, on the player deck too: **RGB** (default since D30; mono height,
  colour mixed from `waveform-rgb-low/mid/high` by band level), **3-band**
  (low/mid/high overlaid, `waveform-low/mid/high`) and **Blue** (mono height in `waveform-blue`, lightening to
  `waveform-blue-high` as highs dominate). All three come from the same
  analysis columns; drawing is pure in [`waveDraw.ts`](../src/lib/waveDraw.ts)
  (`waveBars`). Only styles our columns can feed are offered.
- Sync phase: a marker half a beat either side (two beats with bar sync),
  green within 1/32 beat. The tempo fader is CDJ-style: faster towards the
  bottom.
- Load: the table's context menu ("Load to deck N", also in playlist views)
  or drag a track onto a deck. Loading starts the output if it is off.
- Shortcuts while the panel is open ([`src/lib/mixKeys.ts`](../src/lib/mixKeys.ts),
  also under **Shortcuts** in the panel): A/S/D cue, play, sync deck 2 and
  J/K/L deck 3, the decks on screen with two (Shift: the outer decks 1 and 4), Q/W and U/I nudge while held, 1–4 and
  7–0 hot cues A–D, Z/X/C crossfader. Space stays with the player (deck 2).
- The mixer is memoised away from the 60 Hz engine state; only the decks
  re-render with it.
- Meters (`ui/Meter`): a DJM-style column of nine round LEDs on the deck
  colour, labelled beside them: +10 red, +6/+3 white, 0 blue, −3…−20 lime
  (`led-*` tokens). The scale is `src/lib/meterScale.ts`: +10 is 0 dBFS, so 0
  (blue) is −10 dBFS (our calibration, UNVERIFIED against a DJM). The red LED
  also lights while a clip is held (1 s; the engine reports a clip once).
  Each meter subscribes to its own lit-LED count (`useMeter`,
  `mix/LiveMeter`), so it renders only when an LED changes and the memoised
  mixer still skips the 60 Hz renders.
- Mixer switches (CUE, filter ON/HP/BP/LP/VCF/LINK, SPLIT, POST EQ) are
  `Button variant="control"` and the assign switches `SegmentedControl
  variant="control"`: when on they are lit (`lit` fill, `lit-fg` text,
  `shadow-lit` glow), the same look for every hardware-style switch.
  HP / BP / LP and channel CUE: a click picks one, Shift+click combines
  (tooltip on each); FILTER assign starts at X for channels 1–2, Y for 3–4.
- Sizes: `--knob-*`, `--fader-*`, `--meter-w`, `--meter-dot`, `--meter-label-*`, `--deck-wave-h*`, `--deck-overview-h`,
  `--mixer-strip-w`, `--mixer-row-h`, `--mix-panel-max-h` (the panel scrolls
  inside 62% of the window height).

### Deck layout (task 16, approved 2026-10-22, not built yet)

Replaces the deck described above once [phase 5 task 16](phases/phase-5.md)
lands; until then the bullets above describe the app. Modelled on rekordbox's
2Deck Horizontal. Approved wireframe:
<https://claude.ai/artifact/1bLg3dZwjRfL4nXbYvFdzD> (revision 3).

**Size.** 248 px tall in both 2- and 4-deck mode (no compact deck). The mixer
is 358 × 511 px, so two stacked decks (504 px) fit beside it and 4-deck mode
costs no height. Width is what's left on each side: 441 px at a 1280 px
window (minimum, `--deck-min-w` 440), 521 at 1440, 761 at 1920; extra width
goes to the header and waveforms, the pads stop at 336 px.

**Rows**, top to bottom, with the tempo column down the right edge:

| Row | Contents |
|---|---|
| Header (40 px) | Deck number (lit when loaded) · artwork (eject ⏏ on hover or focus; dashed box without artwork) · title / artist · key · BPM with tempo % under it · remaining over elapsed time |
| Zoomed waveform (72 px, full width) | Playhead centred, beat lines, hot cue and loop markers, sync phase meter on the top edge when synced. Zoom − / + float top right on hover or focus-within, zoom level between them. Drag to skim |
| Overview (20 px) | Whole track, played part shaded, hot cue ticks in cue colour, cue point, loop bar. Keyboard seek slider |
| Bottom (80 px) | CUE and PLAY (round, 32 px) · pad mode (HOT CUE / BEAT JUMP / BEAT LOOP) · Q · page ◀ ▶ · 2 × 4 pads · MASTER and SYNC |
| Tempo column (44 px) | Range button (±6 / ±10 / ±16 / WIDE) and **M** (master tempo, ghost; disabled until task 10 picks an engine) · full-height fader · nudge |

- **BPM edit**: double-click (or Enter when focused) turns the BPM into a
  number field; Enter or blur saves, Esc cancels. Tempo = typed ÷ the
  track's BPM at the playhead; out of range, the range steps up to the
  smallest that fits. Disabled without a grid and while synced.
- **Tempo fader**: detent at 0 % that snaps within `--fader-detent`;
  double-click or Delete resets. No Reset button.
- **Nudge**: the CDJ jog-wheel arrows, two arcs of the wheel's lower rim
  pointing down: left (− REV, anticlockwise) slows, right (+ FWD,
  clockwise) speeds up. Held.
- **MASTER** is lit while the deck is master; a click takes master. SYNC as
  before. Bar sync leaves the deck: Settings → Decks → "Sync aligns: beat /
  bar" (default bar).
- **Pads** (same 2 × 4 in every mode; ◀ ▶ page through sizes, disabled in
  HOT CUE):
  - HOT CUE: A–H, a set pad filled with its cue's colour (`HotCueView.color`,
    else `deck-hotcue`) and its name; empty pads dashed, dimmed, disabled.
  - BEAT JUMP: page 1 FINE (1/32 beat), 1/8, 1/4, 1/2, 1, 2, 4, 8; page 2
    1 … 128. Split pads: left half ◀ back, right half ▶ forward. Jumps keep
    the beat phase while playing.
  - BEAT LOOP: page 1 1/8 … 16, page 2 1/4 … 32. A pad sets a loop from the
    playhead (quantised) or resizes the active one; the active size is lit and
    pressing it again exits.
  - The size used last (jump and loop, default 4) has a heavy outline; the
    E/R and F keys use it.
- **Solo** (player deck): MASTER and SYNC hidden, space kept.
- **States**, each at full size with disabled controls rather than hidden
  ones: empty (how to load in the waveform slot), loading ("Decoding…"),
  analysing (playable; BPM, MASTER, SYNC, Q, jumps and loops wait for the
  grid), no grid (warning inside the waveform's bottom edge), playing,
  synced / looping, error (reason with Retry and Eject in the waveform slot;
  a waveform-only failure keeps the deck playing).

**Components.** `MixPanel` stays the only store reader. `DeckStrip` (layout
and drop target) composes one file per part: `DeckHeader`
(`ArtworkThumb`, `InlineNumber` for the BPM), `DeckWaveformRow` (zoom
buttons, `SyncPhase`), `DeckOverview`, `DeckSlot` (empty / loading /
analysing / error), `DeckTransport`, `DeckPads` (`SegmentedControl`
`variant="control"`, Q, `PagePicker`, `Pad` / `SplitPad`), `DeckSyncButtons`,
`DeckTempo`. New shared pieces: `ui/Pad.tsx` (`Pad`, `SplitPad`; `tone`,
`marked`, `color` via `--pad-color`), `ui/InlineNumber.tsx`, a `detent` prop
on `Fader`, `ArtworkThumb` (shared with `TrackDetailPanel`), `NudgeDownIcon` /
`NudgeUpIcon`. Every control has a `Tooltip` with its `shortcut`, taken from
`deckKeyHint()` in `mixKeys.ts` (same table as the handler).

**Data.** New in `DeckMeta`: `padMode`, `padPage`, `lastJump`, `lastLoop`,
`analysing`; `DeckTrack.key`, `artworkId`; `HotCueView.color` (`#rrggbb` from
`Cue.color_rgb`). From tasks 13 and 15: `DeckView.loop`, beat jump / loop /
loop exit / take-master controls, `DeckMeta.zoom`. Pure helpers in
`mixView.ts`: `JUMP_PAGES`, `LOOP_PAGES`, `formatBeats`, `snapToDetent`,
`tempoForBpm`, `nextPadMode`. Tokens: `--deck-h` 248, `--deck-min-w` 440,
`--deck-wave-h` 72, `--deck-overview-h` 20, `--deck-art` 40,
`--deck-pads-max-w` 336, `--pad-h` 26, `--fader-detent`; `--deck-wave-h-compact`
and the `compact` prop go.

**Keys** (left hand deck 2, right hand deck 3, Shift: decks 1 / 4 with four
shown):

| Action | Left | Right |
|---|---|---|
| Cue (hold) · play · sync | A · S · D | J · K · L |
| Nudge slower / faster (hold) | Q W | U I |
| Hot cues A–D (any pad mode) | 1–4 | 7–0 |
| Beat jump back / forward, last jump size | E R | O P |
| Loop on / off, last loop size | F | ; |
| Take master | G | H |
| Next pad mode | T | Y |
| Zoom out / in | V B | N M |

Crossfader Z / X / C and Space (player deck) unchanged. Mouse or Tab only:
hot cues E–H, other pads, page arrows, Q, M, tempo range, BPM edit, eject.

## My Tags

Two-level tags (category → tag, `tag.rs`). The store
([`src/store/tags.ts`](../src/store/tags.ts)) holds the tree and runs every
edit through `tags_mutate`; assigning patches the loaded table rows in place
(selection and scroll stay, so several tags can be ticked in a row), deleting
re-queries. Pure helpers (coverage, search, create offers) are in
[`tagModel.ts`](../src/store/tagModel.ts).

- `TagManagerDialog` (toolbar tag button, sidebar "My Tags" row, "Manage tags…"):
  rename in place, up/down reorder, delete with a confirmation, add rows.
- `TagSubmenu`: "My Tags" in the track context menu. One checkbox per tag
  for the whole selection; a tag only some selected tracks have is mixed and a
  click adds it to all; the menu stays open; the search field offers
  "Create 'X' in <category>".
- `TagPicker` (Popover body, search + checkboxes by category): the filter bar's
  My Tags field (`FilterState.tagIds` → `tagAny`, counted in the Filters badge)
  and the info panel's `TrackTags` chip editor.
- "My Tags" table column, hidden until switched on.
- States: loading, load error with Retry, no tags yet ("Create your first
  tag"), no match for the search.
- Dev: `npm run dev` serves tags from [`mockTags.ts`](../src/dev/mockTags.ts).

## Rule builder and smart playlists (D29)

The **Advanced** button in the filter bar opens the builder under it
(`RuleBuilder` → `RuleGroup` → `RuleRow` → `RuleValueEditor`; the last three
take props, `RuleBuilder` reads the stores). State is
[`store/rules.ts`](../src/store/rules.ts), the tree edits are pure functions in
[`ruleTree.ts`](../src/store/ruleTree.ts). The builder edits a *draft* (rows may
be unfinished); `toRule` turns the finished rows into the `RuleNode` that
`library.rule` sends with every query, ANDed with the bar, only while the
Filters switch is on.

- Plain language: a group reads "Tracks that match **all / any / none** of
  these" (none = `any` with `not`); rows read "has / doesn't have" tag, "in /
  not in" playlist, "is / is not" for the rest. A summary line above the
  rows says what the conditions mean and the live `total`.
- No Apply button: finished edits re-query after 50 ms and only when the rule
  changed; the table keeps the old rows until the new first page lands. An
  unfinished row is muted ("Not applied yet") and left out.
- Three levels at most (the core allows 8); nested groups hang off an indented
  left rule in `border`. "Last played" rows take a fixed date: a saved rule
  cannot hold "last 30 days".
- Keyboard: Enter in a row's fields adds a row below and focuses its field
  Select; Delete / Backspace on the field Select of an empty row removes it.
  Groups are `fieldset`s labelled "Group: match any".
- The Filters badge and Advanced badge count bar and builder; **Clear all**
  clears both with one Undo toast (`ui/Toast`, `store/toasts`).
- Quick paths: a playlist's context menu "Show tracks not in this playlist"
  and a tag chip's label ("Filter by tag X") add a row and open the builder.
- **Save as smart playlist…** sends `createSmart` and clears the builder (the
  rule lives in the playlist now). In the tree a smart list has its own icon;
  with one shown, `SmartListHeader` offers a "Smart" badge (tooltip says why
  it can't be edited by hand), "Updates automatically" and **Edit rules**,
  which shows the whole collection as a live preview with the rule in the
  builder and Save rules / Cancel (`setRule`). Dropping tracks on it, "Insert
  divider" and "Remove from" are refused, with the reason beside them. The
  export wizard marks smart lists and the dry run notes that they are written
  as fixed lists.
- States: empty builder, unfinished rows, no match (`EmptyResult`, also for a
  smart list and an empty playlist), save errors in the dialog.
- Dev: `mockSmart.ts` evaluates rules and smart lists like the core.

## Adding a component or variant

1. **Variant of an existing component**: add a key to its `cva` `variants`
   using semantic utilities only. If it needs a new colour, add a semantic
   token to `tokens.css` (light and dark) first.
2. **New component**: `src/components/ui/Name.tsx`, exporting `nameVariants`
   (`cva`) and `Name`. Take `variant` / `size` where they apply, pass
   `className` through `cn()` last, spread the rest of the props (React 19
   passes `ref` as a prop), offer `asChild` via Radix Slot when the root is a
   single element. Interactive controls compose `controlSizes` and
   `focusRing` / `disabledState` from `control.ts`.
3. Behaviour (focus management, menus, keyboard): reach for a Radix primitive
   before writing it. A new `@radix-ui/*` package must pass
   `npm run licenses:check`.
4. Add a case to `ui.test.tsx` if it is a control (same-height check) or has
   behaviour of its own.

Biome's fast-refresh rule is off for `src/components/ui/` because the files
export variants next to components; editing one reloads the page instead of
hot-swapping it.
