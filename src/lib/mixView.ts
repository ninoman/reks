/**
 * Display helpers for the mixer (presentation only: tempo, sync and the
 * mixer laws are in Rust, `reks-mix`).
 */

import type { CueRouting, MixState } from "./ipc";

/**
 * Tempo fader ranges (CDJ-3000), as a fraction either side of 1. WIDE is
 * ±100%: at the slow end the deck stands still.
 */
export const TEMPO_RANGES = [0.06, 0.1, 0.16, 1] as const;
export type TempoRange = (typeof TEMPO_RANGES)[number];

/** ±10%, the CDJ default. */
export const DEFAULT_TEMPO_RANGE: TempoRange = 0.1;

const RANGE_LABEL: Record<TempoRange, string> = { 0.06: "±6", 0.1: "±10", 0.16: "±16", 1: "WIDE" };

/** `±6`, `±10`, `±16`, `WIDE`. */
export function tempoRangeLabel(r: TempoRange): string {
  return RANGE_LABEL[r];
}

/** The range after `r`, as the range button cycles. */
export function nextTempoRange(r: TempoRange): TempoRange {
  return TEMPO_RANGES[(TEMPO_RANGES.indexOf(r) + 1) % TEMPO_RANGES.length] ?? DEFAULT_TEMPO_RANGE;
}

/**
 * Decks on screen, by index. With two shown they are decks 2 and 3, the
 * centre channels of a 4-channel mixer, as with two CDJs on a DJM.
 */
export function shownDecks(count: 2 | 4): number[] {
  return count === 4 ? [0, 1, 2, 3] : [1, 2];
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

/**
 * How far `beat` is from the leader's beat phase, in beats, in
 * [-span/2, span/2) (`span` 4 compares bar phase). Positive: ahead.
 */
export function phaseOffset(beat: number, leaderBeat: number, span = 1): number {
  const d = (((beat - leaderBeat) % span) + span) % span;
  return d >= span / 2 ? d - span : d;
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

/** Beat loop sizes per page, in beats. */
export const LOOP_PAGES: readonly (readonly number[])[] = [
  [1 / 8, 1 / 4, 1 / 2, 1, 2, 4, 8, 16],
  [1 / 4, 1 / 2, 1, 2, 4, 8, 16, 32],
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
