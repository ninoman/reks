/**
 * Display helpers for the mixer (presentation only: tempo, sync and the
 * mixer laws are in Rust, `reks-mix`).
 */

import type { CueRouting } from "./ipc";

/** Tempo fader ranges, as a fraction either side of 1. */
export const TEMPO_RANGES = [0.08, 0.16, 0.5] as const;
export type TempoRange = (typeof TEMPO_RANGES)[number];

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
