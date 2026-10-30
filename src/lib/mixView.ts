/**
 * Display helpers for the mixer (presentation only: tempo, sync and the
 * mixer laws are in Rust, `reks-mix`).
 */

import type {
  CueRouting,
  DeckView,
  FilterUnit,
  FxKind,
  MixerEq,
  MixerFilters,
  MixerView,
  MixState,
} from "./ipc";

/**
 * Tempo fader ranges (CDJ-3000), as a fraction either side of 1. WIDE is
 * ±100%: at the slow end the deck stands still.
 */
export const TEMPO_RANGES = [0.06, 0.1, 0.16, 1] as const;
export type TempoRange = (typeof TEMPO_RANGES)[number];

/** ±10%, the CDJ default. */
export const DEFAULT_TEMPO_RANGE: TempoRange = 0.1;

const RANGE_LABEL: Record<TempoRange, string> = { 0.06: "±6", 0.1: "±10", 0.16: "±16", 1: "W" };

/** `±6`, `±10`, `±16`, `W` (WIDE). */
export function tempoRangeLabel(r: TempoRange): string {
  return RANGE_LABEL[r];
}

/** The range after `r`, as the range button cycles. */
export function nextTempoRange(r: TempoRange): TempoRange {
  return TEMPO_RANGES[(TEMPO_RANGES.indexOf(r) + 1) % TEMPO_RANGES.length] ?? DEFAULT_TEMPO_RANGE;
}

const FOUR_DECKS: readonly number[] = [0, 1, 2, 3];
const TWO_DECKS: readonly number[] = [1, 2];

/**
 * Decks on screen, by index. With two shown they are decks 2 and 3, the
 * centre channels of a 4-channel mixer, as with two CDJs on a DJM. The same
 * array every call, so a memoised mixer that takes it skips re-renders.
 */
export function shownDecks(count: 2 | 4): readonly number[] {
  return count === 4 ? FOUR_DECKS : TWO_DECKS;
}

/** Decks 1 and 2 sit left of the mixer, 3 and 4 right. */
export function deckSide(deck: number): "left" | "right" {
  return deck < 2 ? "left" : "right";
}

/** Pointer travel (px) before a press on a waveform counts as a drag. */
export const SCRUB_SLOP_PX = 3;
/** At most one seek per this many ms while dragging. */
export const SCRUB_INTERVAL_MS = 30;

/** Where a drag on a deck waveform puts the playhead, ms. */
export type ScrubGrip =
  /** The overview: the point under the pointer. */
  | { kind: "overview"; widthPx: number; durationMs: number }
  /**
   * The zoomed view: the track moves with the pointer, like a record under
   * a hand (drag left to go forward). `fromMs` is the position at the press.
   */
  | { kind: "zoomed"; pxPerS: number; fromMs: number; durationMs: number };

/** `x`: pointer x from the element's left; `dx`: travel since the press. */
export function scrubTarget(grip: ScrubGrip, x: number, dx: number): number {
  const ms =
    grip.kind === "overview"
      ? grip.widthPx > 0
        ? (x / grip.widthPx) * grip.durationMs
        : 0
      : grip.pxPerS > 0
        ? grip.fromMs - (dx / grip.pxPerS) * 1000
        : grip.fromMs;
  return Math.min(grip.durationMs, Math.max(0, ms));
}

/** Nudge (pitch bend) while held, as a fraction. */
export const NUDGE = 0.04;

/** A follower within this many beats of the leader counts as in phase. */
export const PHASE_LOCK = 1 / 32;

/**
 * Speed ratio of a tempo fader position (0..1, 1 = top). Like a CDJ, the
 * fader is slower at the top and faster at the bottom.
 */
export function tempoRatio(fader: number, range: TempoRange): number {
  return 1 + (0.5 - fader) * 2 * range;
}

/** The fader position of `ratio` at `range`, clamped to the travel. */
export function tempoFader(ratio: number, range: TempoRange): number {
  return Math.min(1, Math.max(0, 0.5 - (ratio - 1) / (2 * range)));
}

/** `+2.50%`, `-0.30%`, `0.00%`. */
export function formatTempo(ratio: number): string {
  const pct = (ratio - 1) * 100;
  const text = Math.abs(pct).toFixed(2);
  if (text === "0.00") return "0.00%";
  return `${pct > 0 ? "+" : "-"}${text}%`;
}

/** How far `beat` is from the leader's beat phase, in beats, in [-1/2, 1/2). Positive: ahead. */
export function phaseOffset(beat: number, leaderBeat: number): number {
  const d = (((beat - leaderBeat) % 1) + 1) % 1;
  return d >= 0.5 ? d - 1 : d;
}

/** Elapsed `m:ss.t` (tenths), or remaining with a leading minus. */
export function formatClock(ms: number, remaining = false): string {
  const t = Math.max(0, Math.floor(ms / 100));
  const m = Math.floor(t / 600);
  const s = Math.floor((t % 600) / 10);
  return `${remaining ? "-" : ""}${m}:${String(s).padStart(2, "0")}.${t % 10}`;
}

/** The running output in one line: device, rate, buffer and any dropouts. */
export function outputSummary(s: MixState): string {
  const parts = [s.device, `${(s.sampleRate / 1000).toFixed(1)} kHz`];
  if (s.bufferFrames !== null) parts.push(`${s.bufferFrames} frames`);
  if (s.xruns > 0) parts.push(`${s.xruns} dropouts`);
  return parts.join(" · ");
}

/** X / off / Y switch positions as the engine takes them. */
export type Assign = "x" | "off" | "y";
export const ASSIGN_VALUE: Record<Assign, number> = { x: 0, off: 0.5, y: 1 };

/** Whether a device with `channels` outputs can take `cue` (as reks-mix checks at start). */
export function cueFits(cue: CueRouting, channels: number): boolean {
  switch (cue.type) {
    case "off":
      return true;
    case "splitMono":
      return channels >= 2;
    case "outputs":
      return [...cue.master, ...cue.cue].every((c) => c < channels);
  }
}

/**
 * Click on switch `i` of a group (filter modes, channel cues): a plain click
 * picks that switch alone, Shift+click (`additive`) toggles it on top of the
 * others. Clicking the only switch on turns it off, unless `keepOne` (filter
 * modes: a unit always has a mode), when it stays on.
 */
export function pickSwitches(
  on: readonly boolean[],
  i: number,
  additive: boolean,
  keepOne: boolean,
): boolean[] {
  const next = additive
    ? on.map((v, j) => (j === i ? !v : v))
    : on.map((v, j) => (j === i ? !(v && on.every((w, k) => k === j || !w)) : false));
  return keepOne && !next.some(Boolean) ? [...on] : next;
}

/** What the deck's 2 × 4 pads do (rekordbox's pad modes we have, plus the memory cues). */
export type PadMode = "hotCue" | "memory" | "beatJump" | "beatLoop";
export const PAD_MODES: readonly PadMode[] = ["hotCue", "memory", "beatJump", "beatLoop"];

/** The mode after `m`, as the pad mode key cycles. */
export function nextPadMode(m: PadMode): PadMode {
  return PAD_MODES[(PAD_MODES.indexOf(m) + 1) % PAD_MODES.length] ?? "hotCue";
}

/** Hot cue pads: A–H, one page. */
export const HOT_CUE_PADS = 8;

/** Memory cue pads: the first eight by position; the list (count button) has them all. */
export const MEMORY_PADS = 8;

/** Beat jump sizes per page, in beats; FINE is 1/32 beat (maintainer, 2026-10-22). */
export const JUMP_PAGES: readonly (readonly number[])[] = [
  [1 / 32, 1 / 8, 1 / 4, 1 / 2, 1, 2, 4, 8],
  [1, 2, 4, 8, 16, 32, 64, 128],
];

/** Beat loop sizes per page, in beats, as rekordbox's (maintainer, 2026-10-24). */
export const LOOP_PAGES: readonly (readonly number[])[] = [
  [1 / 64, 1 / 32, 1 / 16, 1 / 8, 1 / 4, 1 / 2, 1, 2],
  [4, 8, 16, 32, 64, 128, 256, 512],
];

/** The jump and loop size a deck starts with, and the keys use until a pad is pressed. */
export const DEFAULT_PAD_BEATS = 4;

/** `FINE` (1/32), `1/8`, `4`, `128`. */
export function formatBeats(beats: number): string {
  if (beats === 1 / 32) return "FINE";
  if (beats >= 1) return String(beats);
  return `1/${Math.round(1 / beats)}`;
}

/** A fader position within `zone` (0..1 of the travel) of `detent` lands on it. */
export function snapToDetent(value: number, detent: number, zone: number): number {
  return Math.abs(value - detent) <= zone ? detent : value;
}

/**
 * The tempo that plays a track at `targetBpm`, given its own BPM at the
 * playhead (`trackBpm`, at tempo 1), and the range to show it in: `range`,
 * or the smallest one that reaches it. Null when no range does (WIDE's slow
 * end is 0) or the input is not a BPM.
 */
export function tempoForBpm(
  targetBpm: number,
  trackBpm: number,
  range: TempoRange,
): { ratio: number; range: TempoRange } | null {
  if (!(targetBpm > 0) || !(trackBpm > 0)) return null;
  const ratio = targetBpm / trackBpm;
  const fits = (r: TempoRange) => Math.abs(ratio - 1) <= r + 1e-9;
  if (fits(range)) return { ratio, range };
  const wider = TEMPO_RANGES.find((r) => r > range && fits(r));
  return wider === undefined ? null : { ratio, range: wider };
}

/**
 * Controls to set a beat loop of `beats` from a LOOP pad: a new loop, the
 * loop left when it is that size already, or the active loop halved or
 * doubled to it (sizes are powers of two, and resizing keeps the start).
 */
export function loopPadControls(
  active: number | null,
  beats: number,
): (
  | { type: "beatLoop"; beats: number }
  | { type: "loopExit" }
  | { type: "loopResize"; double: boolean }
)[] {
  if (active === null) return [{ type: "beatLoop", beats }];
  const steps = Math.round(Math.log2(beats / active));
  if (steps === 0) return [{ type: "loopExit" }];
  return Array.from({ length: Math.abs(steps) }, () => ({
    type: "loopResize" as const,
    double: steps > 0,
  }));
}

/** Longest cue name, as `reks-core` `MAX_CUE_NAME_CHARS` allows. */
export const MAX_CUE_NAME_CHARS = 64;

/** A memory cue this close to the playhead counts as "here", not before or after it. */
export const CUE_HERE_MS = 5;

/**
 * Memory cue call ◀ / ▶: the position of the nearest memory cue before
 * (`dir` -1) or after the playhead, skipping one it is already on; null at
 * either end. `cues` by position.
 */
export function adjacentCue(
  cues: readonly { positionMs: number }[],
  positionMs: number,
  dir: -1 | 1,
): number | null {
  if (dir > 0) return cues.find((c) => c.positionMs > positionMs + CUE_HERE_MS)?.positionMs ?? null;
  let before: number | null = null;
  for (const c of cues) if (c.positionMs < positionMs - CUE_HERE_MS) before = c.positionMs;
  return before;
}

/** The sizes LOOP can take: 1/64 … 512 beats, powers of two. */
export const LOOP_SIZE_MIN = 1 / 64;
export const LOOP_SIZE_MAX = 512;

/** `size` halved or doubled, kept within `LOOP_SIZE_MIN` … `LOOP_SIZE_MAX`. */
export function stepLoopSize(size: number, double: boolean): number {
  return Math.min(LOOP_SIZE_MAX, Math.max(LOOP_SIZE_MIN, double ? size * 2 : size / 2));
}

/** A loop length in beats: `64`, `1/16`, or `3.87` when it is no power of two (a manual loop). */
export function formatLoopBeats(beats: number): string {
  if (beats >= 1 && Number.isInteger(beats)) return String(beats);
  const inverse = 1 / beats;
  if (Math.abs(inverse - Math.round(inverse)) < 1e-6) return `1/${Math.round(inverse)}`;
  return String(Number(beats.toFixed(2)));
}

/** A loop length in ms on a track without a grid: `0.25s`, `12.5s`. */
export function formatLoopSeconds(ms: number): string {
  const s = ms / 1000;
  return `${Number(s.toFixed(s < 10 ? 2 : 1))}s`;
}

/** The quick-loop block's pages: the beat loop (size, ½ / 2×) and the manual loop (IN, OUT, RELOOP). */
export type LoopPage = "beat" | "manual";

/** What the quick-loop block shows and allows, from the deck's engine state. */
export interface LoopBlockState {
  /** IN is lit while it waits for OUT. */
  inLit: boolean;
  outEnabled: boolean;
  /** ½ and 2×: the active loop, or the size button's next size (that needs a grid). */
  resizeEnabled: boolean;
  /** Any loop plays (beat, manual, memory or auto): the size is lit and RELOOP reads EXIT. */
  looping: boolean;
  /** The size button: a loop of that size, or out of the playing loop. */
  loopEnabled: boolean;
  reloopEnabled: boolean;
  /** The playing loop's length, or the size the size button loops next. */
  sizeText: string;
}

/** `nextBeats` is `DeckMeta.lastLoop`; `grid` that the engine has one for the track. */
export function loopBlockState(
  view: DeckView | undefined,
  loaded: boolean,
  grid: boolean,
  nextBeats: number,
): LoopBlockState {
  const loop = loaded ? (view?.loopRegion ?? null) : null;
  const loopIn = loaded ? (view?.loopIn ?? null) : null;
  const looping = loop !== null;
  const text =
    loop === null
      ? formatLoopBeats(nextBeats)
      : loop.beats === null
        ? formatLoopSeconds(loop.endMs - loop.startMs)
        : formatLoopBeats(loop.beats);
  return {
    inLit: loopIn !== null && !looping,
    outEnabled: looping || (loopIn !== null && loopIn < (view?.positionMs ?? 0)),
    resizeEnabled: loaded && (looping || grid),
    looping,
    loopEnabled: loaded && (looping || grid),
    reloopEnabled: looping || (loaded && (view?.lastLoop ?? null) !== null),
    sizeText: text,
  };
}

/**
 * The mixer's look: the EQ layout, and Simple, which hides the crossfader and
 * the assign rows; `filters` only matters in 4-band Simple.
 */
export interface MixerLook {
  eq: MixerEq;
  view: MixerView;
  filters: MixerFilters;
}

export const DEFAULT_LOOK: MixerLook = { eq: "fourBand", view: "simple", filters: "show" };

/** Which rows of the mixer are on screen. */
export interface MixerParts {
  /** HI · MID · LOW · FX knob rows and the FX row, in place of the 4-band EQ, the X/Y units and FILTER assigns. */
  threeBand: boolean;
  /** Every strip's FILTER assign row (4-band Advanced). */
  filterAssigns: boolean;
  /** Every strip's X-F assign row, and the crossfader row (Advanced). */
  crossfader: boolean;
  /** Filter units X and Y. */
  filterRow: boolean;
}

export function mixerParts(look: MixerLook): MixerParts {
  const advanced = look.view === "advanced";
  const threeBand = look.eq === "threeBand";
  return {
    threeBand,
    filterAssigns: advanced && !threeBand,
    crossfader: advanced,
    filterRow: !threeBand && (advanced || look.filters === "show"),
  };
}

export type KnobParam =
  | "trim"
  | "eqHi"
  | "eqMid1"
  | "eqMid2"
  | "eqLo"
  | "eq3Hi"
  | "eq3Mid"
  | "eq3Lo"
  | "fx";

/** `[param, caption]` per knob row; the FX row's caption is the FX select. */
const FOUR_BAND: [KnobParam, string][] = [
  ["trim", "Level"],
  ["eqHi", "Hi"],
  ["eqMid1", "HM"],
  ["eqMid2", "LM"],
  ["eqLo", "Lo"],
];
const THREE_BAND: [KnobParam, string][] = [
  ["trim", "Level"],
  ["eq3Hi", "Hi"],
  ["eq3Mid", "Mid"],
  ["eq3Lo", "Low"],
  ["fx", "FX"],
];

/** A channel strip's knob rows, top to bottom, for the EQ layout. */
export function knobRows(parts: MixerParts): [KnobParam, string][] {
  return parts.threeBand ? THREE_BAND : FOUR_BAND;
}

/**
 * The mixer's height class (`--mixer-h` per `data-mixer` in tokens.css): 4-band
 * 440 / 356 / 292 px, 3-band 393 / 333 px.
 */
export type MixerSize = "advanced" | "simple" | "simple-bare" | "three-advanced" | "three";

export function mixerSize(look: MixerLook): MixerSize {
  if (look.eq === "threeBand") return look.view === "advanced" ? "three-advanced" : "three";
  if (look.view === "advanced") return "advanced";
  return look.filters === "show" ? "simple" : "simple-bare";
}

/** The channel filter assign a hidden FILTER row stands for: channels 1–2 on X, 3–4 on Y. */
export function defaultFilterAssign(ch: number): Assign {
  return ch < 2 ? "x" : "y";
}

/** The FX knob's dead zone either side of centre (0.485–0.515 is off), as reks-mix `taper` has it. */
export const FX_DEAD_ZONE = 0.015;

/** Whether a bipolar knob is off centre (lit); `zone` is a dead zone that still counts as centre. */
export function offCentre(value: number, zone = 0): boolean {
  return Math.abs(value - 0.5) > zone + 1e-6;
}

/** The effects of the FX select, in button order. */
export const FX_KINDS: readonly FxKind[] = ["filter", "echo", "reverb"];

/** `caption` is the short form that fits the mixer legend's PARAMETER row. */
const FX_LABEL: Record<FxKind, { name: string; parameter: string; caption: string }> = {
  filter: { name: "Filter", parameter: "Resonance", caption: "Reso" },
  echo: { name: "Echo", parameter: "Feedback", caption: "Fdbk" },
  reverb: { name: "Reverb", parameter: "Decay", caption: "Decay" },
};

/** The effect's name: the FX select's caption and its menu. */
export function fxName(kind: FxKind): string {
  return FX_LABEL[kind].name;
}

/** What PARAMETER sets for the effect: its spoken name and the FX menu's hint. */
export function fxParameterName(kind: FxKind): string {
  return FX_LABEL[kind].parameter;
}

/** PARAMETER's caption in the legend, short. */
export function fxParameterCaption(kind: FxKind): string {
  return FX_LABEL[kind].caption;
}

/** The 4-band EQ knobs, at noon in 3-band. */
const NEUTRAL_FOUR_BAND = { eqHi: 0.5, eqMid1: 0.5, eqMid2: 0.5, eqLo: 0.5 } as const;
/** The 3-band EQ and FX knobs, at noon in 4-band. */
const NEUTRAL_THREE_BAND = { eq3Hi: 0.5, eq3Mid: 0.5, eq3Lo: 0.5, fx: 0.5 } as const;

/** The channel values `effectiveMix` stands in for. */
export type MixChannel = { -readonly [K in keyof typeof NEUTRAL_FOUR_BAND]: number } & {
  -readonly [K in keyof typeof NEUTRAL_THREE_BAND]: number;
} & { filterAssign: Assign; xfadeAssign: Assign };

/** The values the engine takes; the user's own positions stay where they are. */
export interface MixerValues<C extends MixChannel, F extends { on: boolean }> {
  channels: C[];
  filters: Record<FilterUnit, F>;
}

/**
 * What the engine gets while controls are hidden: the other EQ layout's knobs
 * at noon (each layout keeps its own positions); without FILTER assigns, the
 * default assigns; without the crossfader, THRU on every channel; without
 * the filter row, both filter units off. 4-band Advanced passes the values
 * as they are, bar the 3-band knobs.
 */
export function effectiveMix<C extends MixChannel, F extends { on: boolean }>(
  values: MixerValues<C, F>,
  look: MixerLook,
): MixerValues<C, F> {
  const parts = mixerParts(look);
  const neutral = parts.threeBand ? NEUTRAL_FOUR_BAND : NEUTRAL_THREE_BAND;
  const channels = values.channels.map((c, ch) => ({
    ...c,
    ...neutral,
    filterAssign: parts.filterAssigns ? c.filterAssign : defaultFilterAssign(ch),
    xfadeAssign: parts.crossfader ? c.xfadeAssign : ("off" as const),
  }));
  const filters = parts.filterRow
    ? values.filters
    : { x: { ...values.filters.x, on: false }, y: { ...values.filters.y, on: false } };
  return { channels, filters };
}
