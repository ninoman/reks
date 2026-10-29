/**
 * Deck grid mode (D10, phase 5 task 32): the panel's steps, hold timing and
 * keys, and what it reads off a grid. Pure; the grid maths are in Rust
 * (`reks-core` `beatgrid`).
 */
import { sectionAt } from "./gridView";
import type { BeatMark, GridAnchor } from "./ipc";

/** SHIFT: a press moves the grid 1 ms; held (or with Shift), 10 ms a step. */
export const SHIFT_STEP_MS = { tap: 1, hold: 10 } as const;
/** BPM − / +: 0.01 a press; held (or with Shift), 0.10 a step. */
export const BPM_STEP_X100 = { tap: 1, hold: 10 } as const;
/** A pad held this long starts repeating with the hold step. */
export const HOLD_DELAY_MS = 300;
export const HOLD_REPEAT_MS = 80;
/** A marker within this many CSS px of the pointer is dragged instead of the track. */
export const MARKER_GRAB_PX = 6;

/** A press, or a repeat while held: repeats take the hold step and share the press's undo step. */
export type HoldPhase = "press" | "repeat";

export interface BarBeat {
  /** Counted from the first downbeat, 1-based; 0 before it. */
  bar: number;
  beat: number;
  /** `ms` minus the nearest beat. */
  offsetMs: number;
}

/** Bar and beat of the beat nearest `ms`, and how far `ms` is from it; null without beats. */
export function barBeatAt(beats: BeatMark[], ms: number): BarBeat | null {
  if (beats.length === 0) return null;
  const us = ms * 1000;
  let lo = 0;
  let hi = beats.length;
  while (lo < hi) {
    const m = (lo + hi) >> 1;
    if ((beats[m]?.positionUs ?? 0) < us) lo = m + 1;
    else hi = m;
  }
  const after = beats[lo];
  const before = beats[lo - 1];
  const k =
    after === undefined || (before !== undefined && us - before.positionUs < after.positionUs - us)
      ? lo - 1
      : lo;
  const b = beats[k] as BeatMark;
  let bar = 0;
  for (let i = 0; i <= k; i++) if (beats[i]?.beatInBar === 1) bar++;
  return { bar, beat: b.beatInBar, offsetMs: Math.round(ms - b.positionUs / 1000) };
}

export function formatBarBeat(b: BarBeat): string {
  return `${b.bar}.${b.beat}`;
}

/** "+6 ms", "−3 ms", "0 ms". */
export function formatOffset(ms: number): string {
  return `${ms > 0 ? "+" : ms < 0 ? "−" : ""}${Math.abs(ms)} ms`;
}

/**
 * The beat stretch and BPM edits pivot on: the pin when it is in the
 * section at `atMs`, else that section's marker. Rust takes the beat
 * nearest it.
 */
export function pivotMs(anchors: GridAnchor[], pinMs: number | null, atMs: number): number {
  const i = sectionAt(anchors, atMs) ?? 0;
  if (pinMs !== null && sectionAt(anchors, pinMs) === i) return pinMs;
  return anchors[i]?.positionMs ?? 0;
}

/**
 * The marker "− Marker" removes at `ms`: one within a quarter beat, never
 * the first (it starts the grid).
 */
export function removableMarker(anchors: GridAnchor[], ms: number): number | null {
  let best: number | null = null;
  for (let i = 1; i < anchors.length; i++) {
    const a = anchors[i] as GridAnchor;
    const d = Math.abs(a.positionMs - ms);
    if (d <= 1_500_000 / a.bpmX100 && (best === null || d < bestDistance(anchors, best, ms))) {
      best = i;
    }
  }
  return best;
}

function bestDistance(anchors: GridAnchor[], i: number, ms: number): number {
  return Math.abs((anchors[i]?.positionMs ?? 0) - ms);
}

/** Grid mode's key actions on the focused deck. `big`: Shift held, or a key repeat. `exact`: Alt, no snap. */
export type GridKeyAction =
  | { type: "shift"; dir: -1 | 1; phase: HoldPhase; big: boolean }
  | { type: "bpm"; dir: -1 | 1; phase: HoldPhase; big: boolean }
  | { type: "beatOne"; exact: boolean }
  | { type: "stretch"; exact: boolean }
  | { type: "marker"; exact: boolean }
  | { type: "halve" }
  | { type: "double" }
  | { type: "undo" }
  | { type: "redo" }
  | { type: "exit" };

/** How the keys read in hints. */
export const GRID_KEYS = {
  shiftBack: "←",
  shiftForward: "→",
  bpmDown: "↓",
  bpmUp: "↑",
  beatOne: "B",
  stretch: "S",
  marker: "M",
  halve: "/",
  double: "*",
  undo: "⌘Z",
  redo: "⇧⌘Z",
  exit: "Esc",
} as const;

interface KeyLike {
  key: string;
  code: string;
  repeat: boolean;
  shiftKey: boolean;
  altKey: boolean;
  metaKey: boolean;
  ctrlKey: boolean;
}

/** The action for `e`, or null for a key grid mode leaves alone. */
export function gridKey(e: KeyLike): GridKeyAction | null {
  if (e.metaKey || e.ctrlKey) {
    if (e.code === "KeyZ") return e.shiftKey ? { type: "redo" } : { type: "undo" };
    return null;
  }
  const phase: HoldPhase = e.repeat ? "repeat" : "press";
  const big = e.shiftKey || e.repeat;
  if (!e.altKey) {
    switch (e.key) {
      case "ArrowLeft":
        return { type: "shift", dir: -1, phase, big };
      case "ArrowRight":
        return { type: "shift", dir: 1, phase, big };
      case "ArrowDown":
        return { type: "bpm", dir: -1, phase, big };
      case "ArrowUp":
        return { type: "bpm", dir: 1, phase, big };
      case "/":
        return e.repeat ? null : { type: "halve" };
      case "*":
        return e.repeat ? null : { type: "double" };
      case "Escape":
        return { type: "exit" };
    }
  }
  if (e.repeat || e.shiftKey) return null;
  const exact = e.altKey;
  switch (e.code) {
    case "KeyB":
      return { type: "beatOne", exact };
    case "KeyS":
      return { type: "stretch", exact };
    case "KeyM":
      return { type: "marker", exact };
  }
  return null;
}
