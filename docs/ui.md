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
`deck-cue`, `deck-hotcue`, `deck-memory`, `deck-loop` and the other waveform styles' `waveform-rgb-*` and
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
| `SegmentedControl` + `Segment` | `size`; `variant`: default, control (chosen segment lit), outline (chosen segment ringed in `accent`, no fill: mode tabs); one choice, can't be cleared (radiogroup); segment corners nest inside the outer radius (`--seg-r`) | Radix ToggleGroup |
| `Chip` | removable label pill (`onRemove`, `removeLabel`) | – |
| `Toast` | bottom-centre message with an optional action (Undo); one at a time via `store/toasts` + `ToastHost` | – |
| `Badge` | `variant`: neutral, accent, muted; `size`: sm, md | – |
| `Kbd` | `variant`: default, inverse | – |
| `Tooltip` | `content`, `shortcut`, `side` | Radix Tooltip |
| Shortcut hint | `Button` / `IconButton` take `shortcut` (+ `hint`, default `aria-label`) and show it in a tooltip on hover/focus; the key text comes from `lib/mixKeys` (`deckKeyHint`, `crossfaderKeyHint`) or `lib/keys` (`PLAYER_PLAY_KEY`), never a literal | `ShortcutTooltip` |
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
| `Fader` | `orientation`: vertical, horizontal; `defaultValue` (double-click the cap or Delete); `detent` (a tick that drags snap to within `--fader-detent`; keys step past it); `ticks` (evenly spaced scale marks, ends included: ends and detent bold, the rest thin and short) | Radix Slider |
| `Pad` + `SplitPad` | deck pads: `tone`: default, set (outlined in `color`, a data colour, else `deck-hotcue`; no fill, `fg` text), empty (dashed), lit; `marked` (heavy outline); `SplitPad`: back / forward halves under one label | – |
| `InlineNumber` | text that becomes a number field on double-click or Enter; Enter / blur commit, Esc cancels, non-numbers are dropped | native |

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
The deck section sits under the app header, above the library, as in
rekordbox and Traktor (2026-10-22; it was along the bottom):

- `PlayerBar`: the section's one toolbar. **1 deck / 2 decks / 4 decks**
  (`Decks shown`): 1 is the player deck alone, 2 and 4 open the Mix panel
  (`panels.mixOpen`, `MixStore.deckCount`). Then the status line: device ·
  rate · buffer · dropouts while the output runs (`outputSummary`), "Opening
  the audio output…", "Audio output off", or the error (failed start, lost
  device; `role="alert"`, the only place errors show). **Start output** /
  **Stop output** (loading a track starts it too). With the Mix panel open,
  **Shortcuts**. Then volume (`MixControl::Volume`, saved as
  `ui.state.volume`) and output device.
- How the output opens is in Settings → Playback (`OutputSettings`), each
  saved: output device (`audio.device`), sample rate (`audio.sample_rate`;
  Automatic = 44.1 kHz when offered), buffer (`audio.buffer_frames`; only
  sizes the device takes) and headphone cue (`audio.cue`; the device's own
  routing until the user picks one, routings it has too few outputs for
  disabled). A change reopens a running output with every loaded deck back
  where it was (`MixStore.setOutput`, `chooseOutput`); a new device goes
  back to its own rate and buffer.
- Under it, `PlayerDeck` (1 deck): deck 2 alone as a `DeckStrip` with
  `solo` (no MASTER, SYNC, phase meter or mixer keys), laid out flat (see
  **Solo** under [Deck layout](#deck-layout-task-16)), with the deck's
  waveforms, cue, pads, nudge, quantise and tempo fader. Empty, it says to
  double-click a track or drag one onto it. With 2 or 4 decks the Mix panel
  takes its place and shows the same deck among the others; "Load to deck N"
  in the table opens it.
- Double-click or Enter on a row, and double-click in a history session or
  a stick list (Devices, below), load the track on deck 2 and play it (it keeps playing if it already is);
  loading opens the output when it is off. Space plays and pauses deck 2
  wherever focus is (except in text fields, dialogs and menus). The row of
  the track on deck 2 is marked as loaded; the grid editor draws deck 2's
  playhead when it shows that track.

## Devices (D32)

`DeviceTree` sits between the playlist tree and the Histories in the sidebar
(each of the two lower trees takes at most a third of the sidebar). The store
is [`deviceBrowser.ts`](../src/store/deviceBrowser.ts); everything in it is
read-only.

- Root row "Devices" with a gear that opens the USB devices dialog. While it
  is open, the drive list refreshes every two seconds (`useDevicePolling`), so
  a stick shows up when it is plugged in; a drive that goes away is forgotten
  and its open list closes.
- A drive row (USB icon, eject button on hover) opens to "Playlists" (folders
  and playlists as on the stick) and "Histories" (sessions players logged,
  with a count). Its library is read each time it is opened (after the first
  time the last read stays up meanwhile, so an export shows up); states:
  "Reading…", read error with "Try again", "No rekordbox library on this
  drive", "No playlists", "Not played on a player yet".
- Click a playlist or session (Up/Down with the tree focused) and
  `StickListView` replaces the toolbar and table, as a history session does:
  #, title, artist, genre, BPM, key, time, with a play button per row (hover
  or focus) and double-click to play. Entries whose track the stick no longer
  has show as "Not on the stick". The track on the player deck is marked.
- Selecting anything in the playlist tree or a library history session closes
  the stick list, and the other way round.
- A stick track on a deck is a `TrackRef` of kind `stick` (`mix.ts`): its
  grid and waveform come from `device_beatgrid` / `device_waveform_columns`
  (the stick's own analysis, so both show at once; D32),
  an empty hot cue pad stays dark and cue edits say the cues are the stick's.
- Dev: `npm run dev` serves the REKS stick from
  [`mockBrowse.ts`](../src/dev/mockBrowse.ts).

## Mix panel (phase 5)

[`src/components/mix/`](../src/components/mix/), opened with **2 decks** or
**4 decks** in the player bar, in place of the player deck. Its controls and
status are in the player bar ([Player](#player-d30)), so the panel is the
decks and mixer only, shown whether the output runs or not (2026-10-22; it
had a start form before the output opened). `MixPanel` is the only part that reads the store
([`src/store/mix.ts`](../src/store/mix.ts)); `DeckStrip`, `DeckWaveform`,
`MixerPanel`, `ChannelStrip`, `FilterUnit`, `MasterSection` and `SyncPhase`
take props. The mixer follows the hardware: four channel strips (level, HI,
MID 1, MID 2, LO, filter assign X/off/Y, cue, fader, crossfader assign) with
one caption column, the master and cue section, then filter X, the crossfader
and filter Y. Decks 1 and 2 sit left of it, 3 and 4 right. Two decks by
default, and they are decks 2 and 3, the centre channels, as with two CDJs
on a 4-channel mixer; 4 with the toggle. Decks are one size in both modes
(two stacked fit beside the mixer); the deck itself is described under
[Deck layout](#deck-layout-task-16).

- States: output off (decks empty or as left, mixer set; loading a track or
  **Start output** opens it, and the mixer's positions reach the engine on
  start), opening, error (failed start, device lost; the decks empty),
  running. Deck states: see
  [Deck layout](#deck-layout-task-16).
- Waveforms draw on the dark `deck` like the grid editor: a scrolling view
  with the playhead in the middle and beat lines, and the overview under it,
  which is the keyboard seek slider. Both draw in device pixels from one
  animation-frame loop (`mix/DeckWaveform`, `mix/deckCanvas`): the waveform
  is cached in tiles on a pixel grid fixed to the track (`lib/waveStrip.ts`)
  and copied at whole-pixel offsets; the playhead runs on between engine
  states (`lib/deckScroll.ts`). Zoom is one level for every deck (so they
  compare at a glance; `MixStore.waveZoom`): 32/16/8/4/2 s across, 8 s by
  default, with − / + on a view, the − / + keys (anywhere outside text
  fields, dialogs and menus, Mix panel open or not; `waveZoomKey` in
  `lib/keys.ts`), or Cmd/Ctrl + scroll.
- Marks (`lib/deckMarks.ts`), on both views: hot cues are a flag with the
  slot letter in the cue's library colour (`deck-hotcue` without one; text
  dark or light by the flag's brightness), memory cues a triangle on the
  bottom edge (`deck-memory` without a colour), the cue point the same in
  `deck-cue`, and an active loop a `deck-loop` band.
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
- Sync aligns (Settings → Decks, `mix.sync_align`): **Bar** (default; SYNC
  lines up the downbeats too) or **Beat**. Replaces the deck's BAR button;
  a deck already synced keeps its mode until SYNC is pressed again.
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
  also under **Shortcuts** in the player bar): the table under
  [Deck layout](#deck-layout-task-16). Every deck control with a key shows
  it in its tooltip (`deckKeyHint`, from the same table as the handler).
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
- Mixer switches (CUE, filter ON/HP/BP/LP, SPLIT, POST EQ) are
  `Button variant="control"` and the assign switches `SegmentedControl
  variant="control"`: when on they are lit (`lit` fill, `lit-fg` text,
  `shadow-lit` glow), the same look for every hardware-style switch.
  HP / BP / LP and channel CUE: a click picks one, Shift+click combines
  (tooltip on each); FILTER assign starts at X for channels 1–2, Y for 3–4.
- Knobs draw a position mark (`knobMark`, `--color-knob-mark`: dark ink on the light cap in both themes).
- Sizes: `--knob-*`, `--fader-*`, `--meter-w`, `--meter-dot`, `--meter-label-*`, `--deck-*`, `--pad-h`,
  `--mixer-strip-w`, `--mixer-row-h`, `--mix-panel-max-h` (the panel scrolls
  inside 62% of the window height).

### Deck layout (task 16)

[Phase 5 task 16](phases/phase-5.md), approved 2026-10-22, built the same day.
Modelled on rekordbox's 2Deck Horizontal. Approved wireframe:
<https://claude.ai/artifact/1bLg3dZwjRfL4nXbYvFdzD> (revision 3).

**Size.** 260 px tall in both 2- and 4-deck mode (no compact deck; 248
until Q moved over CUE and PLAY, 2026-10-22). The mixer is 358 × 511 px, so
two stacked decks (528 px) make 4-deck mode 17 px taller than the mixer. Width is what's left on each side: 441 px at a 1280 px
window (minimum, `--deck-min-w` 440), 521 at 1440, 761 at 1920; extra width
goes to the header and waveforms, the pads stop at 336 px.

**Rows**, top to bottom, with the tempo column down the right edge:

| Row | Contents |
|---|---|
| Header (40 px) | Deck number (lit when loaded) · artwork (eject ⏏ on hover or focus; dashed box without artwork) · title / artist · key (in the Settings notation; a readout with KEY under it, not a pill) · BPM with tempo % under it · remaining over elapsed time |
| Zoomed waveform (72 px, full width) | Playhead centred, beat lines, hot cue and loop markers, sync phase meter on the top edge when synced. Zoom − / + float top right on hover or focus-within, zoom level between them. Drag to skim |
| Overview (20 px) | Whole track, played part shaded, hot cue ticks in cue colour, cue point, loop bar. Keyboard seek slider |
| Bottom (92 px) | Q over CUE and PLAY (round, 32 px), at the bottom of their column · pad mode (HOT CUE / MEMORY / JUMP / LOOP: equal tabs, the chosen one outlined in `accent`) · page ◀ ▶ (JUMP, LOOP, MEMORY; disabled in HOT CUE so nothing shifts) · 2 × 4 pads · MASTER and SYNC (bottom-aligned like Q, CUE and PLAY) |
| Tempo column (44 px) | Range button (±6 / ±10 / ±16 / WIDE) and **M** (master tempo, ghost; disabled until task 10 picks an engine) · full-height fader · nudge |

- **BPM edit**: double-click (or Enter when focused) turns the BPM into a
  number field; Enter or blur saves, Esc cancels. Tempo = typed ÷ the
  track's BPM at the playhead; out of range, the range steps up to the
  smallest that fits. Disabled without a grid and while synced.
- **Tempo fader**: scale marks, bold at both ends and at 0 %, thin between: 13 on the mixer decks, 7 on the shorter player deck; detent at 0 % that drags snap to within `--fader-detent`;
  double-click or Delete resets. No Reset button.
- **Nudge**: the CDJ jog-wheel arrows, two arcs of the wheel's lower rim
  pointing down: left (− REV, anticlockwise) slows, right (+ FWD,
  clockwise) speeds up. Held.
- **MASTER** is lit while the deck is master; a click takes master. SYNC as
  before. Bar sync leaves the deck: Settings → Decks → "Sync aligns: beat /
  bar" (default bar). The phase meter shows on synced decks other than the
  master.
- **Pads** (pad mode is a tab strip: `SegmentedControl` `variant="outline"`, four equal tabs, the chosen one outlined like the set hot cues; lit is for switches like Q, not modes; same 2 × 4 in every mode; ◀ ▶ page through sizes in BEAT JUMP
  and BEAT LOOP, and through the memory cues in MEMORY; HOT CUE shows them disabled at 1/1 so the row doesn't shift; with the pads under 20rem the labels read CUE / MEM):
  - HOT CUE: A–H, a set pad outlined in its cue's colour (`HotCueView.color`,
    else `deck-hotcue`) and its name; empty pads dashed. See
    [Cue editing](#cue-editing-task-17).
  - MEMORY (added 2026-10-22, maintainer's request): the first eight
    memory cues by position per page (`MEMORY_PADS`), shown
    like hot cues: outlined in the cue's colour (else `deck-memory`), the
    time, LOOP or AUTO on a memory loop, and the name. Click calls the cue,
    double-click renames, × or Shift+click deletes, right-click toggles
    AUTO on a memory loop; an empty pad sets a memory cue at the playhead;
    ◀ ▶ page through them eight at a time. T / Y cycle HOT CUE → MEMORY → JUMP → LOOP.
  - BEAT JUMP: page 1 FINE (1/32 beat), 1/8, 1/4, 1/2, 1, 2, 4, 8; page 2
    1 … 128. Split pads: left half ◀ back, right half ▶ forward. Jumps keep
    the beat phase while playing.
  - BEAT LOOP: page 1 1/8 … 16, page 2 1/4 … 32. A pad sets a loop from the
    playhead (quantised) or resizes the active one (halved / doubled to the
    size, start kept: `loopPadControls`); the active size is lit and pressing
    it again exits.
  - The size used last (jump and loop, default 4) has a heavy outline and
    carries the E/R and F hints; the keys use it. Pad mode, pages and sizes
    are kept per deck across loads.
- **Solo** (player deck, full window width): no MASTER or SYNC. The header
  stays on top; under it one row holds CUE / PLAY, the two views and the
  pads (336 px; they grow to the views' height), so the deck is
  `--deck-solo-h` (162 px) instead of 260. The views get the width that is
  left (953 px at 1440, 310 at the 800 px minimum window).
- **States**, each at full size with disabled controls rather than hidden
  ones: empty (how to load in the waveform slot), loading ("Decoding…"),
  analysing (playable; BPM, MASTER, SYNC, Q, jumps and loops wait for the
  grid), no grid (warning inside the waveform's bottom edge), playing,
  synced / looping, error (reason with Retry and Eject in the waveform slot;
  a waveform-only failure keeps the deck playing and offers Retry only).
- **Analysis on load**: a track whose grid is empty is queued for analysis
  (`analysisReanalyse`). When the `analysed` event arrives the deck refetches
  waveform and grid; the engine only takes a grid at load, so a paused deck
  is reloaded in place at once (cue point, position and tempo put back) and
  a playing one at its next pause, saying so on the waveform meanwhile.

**Components** ([`src/components/mix/`](../src/components/mix/)). `MixPanel`
and `PlayerDeck` stay the only store readers. `DeckStrip` (layout, drop
target, which state shows) composes one file per part: `DeckHeader`
(`ArtworkThumb`, `InlineNumber` for the BPM), `DeckWaveform` (both views:
they share one animation-frame loop, so the overview is not a file of its
own) with `DeckWaveformRow` as its overlay (zoom buttons and level,
`SyncPhase`, the warning line), `DeckSlot` (empty / loading / analysing /
error), `DeckTransport`, `DeckPads` (`SegmentedControl` tabs,
Q, `PagePicker`, `Pad` / `SplitPad`), `DeckSyncButtons`, `DeckTempo`; `hold.ts`
gives CUE and nudge their held handlers. Shared pieces: `ui/Pad.tsx`,
`ui/InlineNumber.tsx`, `Fader` `detent`, `ArtworkThumb` + `useArtworkUrl`
(shared with `TrackDetailPanel`), `NudgeDownIcon` / `NudgeUpIcon`. Controls
with a key show it in their tooltip (`shortcut` from `deckKeyHint()`).

**Data.** `DeckMeta`: `padMode`, `padPage`, `lastJump`, `lastLoop`,
`analysing`, `gridPending`, `failed` (for Retry); `DeckTrack.key`,
`artworkId`, `artworkMime` (from `library_track` on load); `HotCueView.id` /
`MemoryCueView.id` and `name` (task 17); `HotCueView.colorRgb`
(task 15) and `name` (from `Cue.name`, through `mix_load`). From tasks 13 and
15: `DeckView.loopRegion`, `master`, beat jump / loop / loop exit / resize /
take-master controls; task 15's per-deck zoom became one `MixStore.waveZoom`
(maintainer, 2026-10-22). Pure helpers in `mixView.ts`:
`JUMP_PAGES`, `LOOP_PAGES` (FINE = a 1/32 `beatJump`; the engine's 10 ms
`fineJump` is unused by the deck), `formatBeats`, `snapToDetent`,
`tempoForBpm`, `nextPadMode`, `loopPadControls`. Tokens: `--deck-h` 260,
`--deck-header-h` 40, `--deck-min-w` 440, `--deck-wave-h` 72,
`--deck-overview-h` 20, `--deck-views-h`, `--deck-bottom-h` 92, `--deck-solo-h`, `--deck-art`
40, `--deck-pads-max-w` 336, `--deck-transport`, `--deck-sync-w`,
`--deck-tempo-w`, `--pad-h` 26, `--fader-detent` 6.

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

| Set memory cue at the playhead | Alt A | Alt J |
| Previous / next memory cue | Alt E, Alt R | Alt O, Alt P |

Every plain letter for each hand is taken, so the memory cue keys are Alt
on the keys they extend (CUE, beat jump). Crossfader Z / X / C, Space
(player deck) and waveform zoom − / + (every deck at once, also with the
Mix panel closed) are not per deck. Mouse or Tab only: hot cues E–H, other
pads, page arrows, Q, M, tempo range, BPM edit, eject, the memory cue list,
renames.

### Cue editing (task 17)

[Phase 5 task 17](phases/phase-5.md). Every edit is written to the track's
cues in the library first (`mix_cue_edit` → `reks-core` `cue.rs`), so it
shows in the track's details and reaches the rekordbox XML and USB exports;
the command returns the deck's cues and the store takes them.

- **Hot cue pads** (HOT CUE mode, and keys 1–4 / 7–0): an empty (dashed)
  pad sets its cue at the playhead (on the nearest beat with Q) in the next
  colour of `HOT_CUE_PALETTE` (green, blue, red, yellow, purple, cyan,
  orange, pink: rekordbox palette entries, so each exports as itself); a
  set pad jumps as before. Double-click a set pad to rename it in place
  (`InlineTextField`; Enter saves, Esc cancels, blank clears; up to 64
  characters). The double-click's first click still triggers the cue, as a
  click does. Delete with the pad's × (shown on hover or focus; a real
  button after the pad in the Tab order) or Shift+click. The pad's tooltip
  says all of this.
- **Memory cues and loops** live on the MEMORY pads (`DeckPads`
  `MemoryPads`), eight per page with ◀ page ▶ like JUMP and LOOP (the
  page is not kept per deck; it follows the track's cues). An empty pad
  (or Alt A / Alt J) sets a memory cue at the playhead (on the beat with
  Q), or, while a loop plays, saves that loop as a memory loop; a second
  press at the same place adds nothing. A pad calls its cue; double-click
  renames; × or Shift+click deletes; right-click has Jump here, Rename,
  **Auto loop** (memory loops: playback enters it by itself when the
  playhead reaches its start, rekordbox's active loop) and Delete. Alt E R
  / Alt O P call the cue before / after the playhead. A call moves the cue
  point and the playhead there exactly; a playing deck plays on
  (rekordbox's memory list; a CDJ's CALL is UNVERIFIED). There is no + or
  count button and no list (2026-10-22, maintainer's call).
- **Marks**: a memory loop draws a bar along the bottom edge to its end
  and a short end line; an AUTO loop draws both in `deck-auto-loop`, the
  bar thicker. Hovering a mark on either waveform names it ("Hot cue A:
  Drop", "Memory cue", "Memory loop", "Auto loop", "Cue point"; native
  title, `markAt` / `markTitle` in `lib/deckMarks.ts`).
- **Export**: AUTO loops reach a USB stick only with **Active loops**
  (UNVERIFIED badge, off by default) in the export wizard; see phase 5
  task 17.
- Shared pieces: `ui/InlineText.tsx` (`InlineText`, `InlineTextField`);
  `Pad` shows its `hint` in a tooltip even without a key and takes
  `remove` (the corner ×); hint tooltips are
  not hoverable (`disableHoverableContent`), so one flipped over the
  controls above its trigger closes as the pointer leaves and never takes
  their click.

## My Tags

Two-level tags (category → tag, `tag.rs`). The store
([`src/store/tags.ts`](../src/store/tags.ts)) holds the tree and runs every
edit through `tags_mutate`; assigning patches the loaded table rows in place
(selection and scroll stay, so several tags can be ticked in a row), deleting
re-queries. Pure helpers (coverage, search, create offers) are in
[`tagModel.ts`](../src/store/tagModel.ts).

- `TagDrawer`: a `Sheet` on the right, over the info panel (toolbar tag
  button, "Manage tags…", the info panel's "Edit
  tags"). Attach and manage in one place: a checkbox per tag for the main
  list's selection, following it live (`tagTargets.ts`; mixed when only some
  tracks have it; no checkboxes with nothing selected), names shown as text that a click turns into a field
  (`InlineText` `editOn="click"`), drag a row's grip to reorder
  (`useTagDnd.ts`; a tag dropped on another category's tag or header moves
  there, categories reorder among themselves; `dropMove` in `tagModel.ts`
  turns the drop into a `move`; mouse only), × per row deletes after a
  confirmation, add rows; a tag added while
  tracks are selected is attached to them. `move` positions count the node
  where it still is (reks-core). Escape or × closes it; the info button
  brings the info panel back to the front.
- `TagSubmenu`: "My Tags" in the track context menu. One checkbox per tag
  for the whole selection; a tag only some selected tracks have is mixed and a
  click adds it to all; the menu stays open; the search field offers
  "Create 'X' in <category>".
- `TagPicker` (Popover body, search + checkboxes by category): the filter bar's
  My Tags field (`FilterState.tagIds` → `tagAny`, counted in the Filters badge).
- `TrackTags`: the info panel's removable chips and the "Edit tags" button.
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
