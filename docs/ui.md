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
`warning`, `rating`, `focus-ring`, `overlay`, `waveform-*`, `track-*`
(rekordbox colours: data, the same in both themes). The grid editor draws on
its own dark deck in both themes: `deck`, `deck-fg`, `waveform-low/mid/high`
(3-band), `grid-beat`, `grid-downbeat`, `grid-cursor`; the mixer's decks add
`deck-cue` and `deck-hotcue`.

The indigo `accent` stays scarce: primary buttons, the loaded track, the played
waveform, focus rings, drop indicators. Selection and toggles that are on
use the neutral `selected` tint.

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
header, library toolbar, filter bar, transport); `md` (default) in dialogs and
forms; `lg` for the transport play button. Table and tree rows use `--row-h`,
menu items `--menu-item-h`.

## Components

| Component | Variants / props | Behaviour from |
|---|---|---|
| `Button` | `variant`: primary, secondary, outline, ghost, danger, quiet, link; `size`; `aria-pressed` = toggle | Radix Slot (`asChild`) |
| `IconButton` | as Button (default ghost) + `shape`: square, round; `aria-label` required | Radix Slot |
| `Input` | `variant`: default, search; `size` | native |
| `Textarea` | `size`: sm, md | native |
| `Select` | `size`; native `<select>` with our chevron | native |
| `Switch` | `size`: sm, md | Radix Switch |
| `Checkbox` | `size`: sm, md; native, wrap in a `<label>` (Biome knows it as an input) | native |
| `Slider` | `size`: sm, md; one value | Radix Slider |
| `SegmentedControl` + `Segment` | `size`; one choice, can't be cleared (radiogroup) | Radix ToggleGroup |
| `Badge` | `variant`: neutral, accent, muted; `size`: sm, md | – |
| `Kbd` | `variant`: default, inverse | – |
| `Tooltip` | `content`, `shortcut`, `side` | Radix Tooltip |
| `DropdownMenu*` | Content, Item (`variant`: default, danger), CheckboxItem, Separator | Radix DropdownMenu |
| `ContextMenu*` | Content, Item, Separator; wrap the whole area, not each row | Radix ContextMenu |
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

## Mix panel (phase 5)

[`src/components/mix/`](../src/components/mix/), opened with **Mix** in the
app header, under the table. `MixPanel` is the only part that reads the store
([`src/store/mix.ts`](../src/store/mix.ts)); `DeckStrip`, `DeckWaveform`,
`MixerPanel`, `ChannelStrip`, `FilterUnit`, `MasterSection` and `SyncPhase`
take props. The mixer follows the hardware: four channel strips (level, HI,
MID 1, MID 2, LO, filter assign X/off/Y, cue, fader, crossfader assign) with
one caption column, the master and cue section, then filter X, the crossfader
and filter Y. Decks 1 and 3 sit left of it, 2 and 4 right; 2 decks by
default, 4 with the toggle (shorter waveforms).

- States: output off (start form: device, rate, buffer, cue routing; the
  device's cue routing is preselected until the user picks one, and routings
  it has too few outputs for are disabled), opening,
  error (failed start, device lost), running. A deck is empty (how to load),
  loading, failed or loaded; a track without a grid says sync needs one.
- Waveforms draw on the dark `deck` like the grid editor: 8 s scrolling with
  the playhead in the middle, beat lines, cue (`deck-cue`) and hot cues
  (`deck-hotcue`); the overview under it is the keyboard seek slider.
- Sync phase: a marker half a beat either side (two beats with bar sync),
  green within 1/32 beat. The tempo fader is CDJ-style: faster towards the
  bottom.
- Load: the table's context menu ("Load to deck N", also in playlist views)
  or drag a track onto a deck. Loading starts the output if it is off.
- Shortcuts while the panel is open ([`src/lib/mixKeys.ts`](../src/lib/mixKeys.ts),
  also under **Shortcuts** in the panel): A/S/D cue, play, sync deck 1 and
  J/K/L deck 2 (Shift: decks 3 and 4), Q/W and U/I nudge while held, 1–4 and
  7–0 hot cues A–D, Z/X/C crossfader. Space stays with the preview player.
- The mixer is memoised away from the 60 Hz engine state; only the decks
  re-render with it.
- Meters (`ui/Meter`): peak on a −48…0 dBFS scale, warning colour above
  −6 dB, a clip cap held for 1 s (the engine reports a clip once). Each meter
  subscribes to its own value (`useMeter`, `mix/LiveMeter`), so the
  memoised mixer still skips the 60 Hz renders.
- Sizes: `--knob-*`, `--fader-*`, `--meter-w`, `--deck-wave-h*`, `--deck-overview-h`,
  `--mixer-strip-w`, `--mixer-row-h`, `--mix-panel-max-h` (the panel scrolls
  inside 62% of the window height).

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
