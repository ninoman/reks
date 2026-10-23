/**
 * Cue and loop marks on the deck waveforms: which marks a deck shows, in
 * which colours (pure), and drawing them on a canvas in device pixels.
 * Hot cues are a line with a flag carrying the slot letter in the cue's own
 * library colour; memory cues and the deck's cue point are a line with a
 * triangle on the bottom edge, a memory loop also a bar along the bottom to
 * its end (an auto-entered one in `deck-auto-loop`, thicker); a pending loop
 * IN is a line with a bracket; the playing loop is a shaded band.
 */
import { formatRgb, hotCueLabel } from "./format";
import type { HotCueView, LoopView, MemoryCueView } from "./ipc";
import { hexRgb } from "./waveDraw";

export type MarkKind = "hot" | "memory" | "cue" | "loopIn";

export interface DeckMark {
  kind: MarkKind;
  ms: number;
  colour: string;
  /** The hot cue letter. */
  label: string | null;
  /** The cue's name from the library, for the hover hint. */
  name: string | null;
  /** A memory loop's end. */
  endMs: number | null;
  /** A memory loop playback enters by itself. */
  auto: boolean;
}

/** Colours (tokens) for marks without a library colour. */
export interface MarkColours {
  hot: string;
  memory: string;
  autoLoop: string;
  cue: string;
  loopIn: string;
}

/**
 * A deck's marks, drawn in order: memory cues, the cue point, the pending
 * loop IN (`loopInMs`, null when none), then hot cues on top.
 */
export function deckMarks(
  hotCues: HotCueView[],
  memoryCues: MemoryCueView[],
  cueMs: number,
  loopInMs: number | null,
  colours: MarkColours,
): DeckMark[] {
  const colour = (rgb: number | null, fallback: string) =>
    rgb === null ? fallback : formatRgb(rgb);
  return [
    ...memoryCues.map((c) => ({
      kind: "memory" as const,
      ms: c.positionMs,
      colour: c.activeLoop ? colours.autoLoop : colour(c.colorRgb, colours.memory),
      label: null,
      name: c.name,
      endMs: c.endMs,
      auto: c.activeLoop,
    })),
    {
      kind: "cue",
      ms: cueMs,
      colour: colours.cue,
      label: null,
      name: null,
      endMs: null,
      auto: false,
    },
    ...(loopInMs === null
      ? []
      : [
          {
            kind: "loopIn" as const,
            ms: loopInMs,
            colour: colours.loopIn,
            label: null,
            name: null,
            endMs: null,
            auto: false,
          },
        ]),
    ...hotCues.map((c) => ({
      kind: "hot" as const,
      ms: c.positionMs,
      colour: colour(c.colorRgb, colours.hot),
      label: hotCueLabel(c.slot),
      name: c.name,
      endMs: null,
      auto: false,
    })),
  ];
}

/** The mark nearest `ms` within `slopMs`, hot cues first (they draw on top). */
export function markAt(marks: DeckMark[], ms: number, slopMs: number): DeckMark | null {
  let best: DeckMark | null = null;
  for (const m of marks) {
    const d = Math.abs(m.ms - ms);
    if (d > slopMs) continue;
    if (best === null || (m.kind === "hot") > (best.kind === "hot") || d < Math.abs(best.ms - ms))
      best = m;
  }
  return best;
}

/** A mark's hover hint: "Hot cue A: Drop", "Memory cue", "Memory loop", "Auto loop", "Cue point", "Loop IN". */
export function markTitle(m: DeckMark): string {
  const what =
    m.kind === "hot"
      ? `Hot cue ${m.label}`
      : m.kind === "cue"
        ? "Cue point"
        : m.kind === "loopIn"
          ? "Loop IN"
          : m.auto
            ? "Auto loop"
            : m.endMs !== null
              ? "Memory loop"
              : "Memory cue";
  return m.name === null ? what : `${what}: ${m.name}`;
}

/** Whether text on a `#rrggbb` flag should be dark (light flag) or light. */
export function darkText(hex: string): boolean {
  const [r, g, b] = hexRgb(hex);
  // Rec. 601 luma: cheap and close enough to pick a readable text colour.
  return 0.299 * r + 0.587 * g + 0.114 * b > 140;
}

/** The active loop, if the engine reports one with a positive length. */
export function activeLoop(loop: LoopView | null): LoopView | null {
  return loop != null && loop.endMs > loop.startMs ? loop : null;
}

export interface MarkStyle {
  /** Device pixels per CSS pixel. */
  dpr: number;
  /** Flag height in CSS px (the letter fits inside). */
  flag: number;
  font: string;
  dark: string;
  light: string;
}

/** Draws `marks` at `xOf(ms)` (device px) on a view `width` × `height` device px. */
export function drawMarks(
  ctx: CanvasRenderingContext2D,
  marks: DeckMark[],
  xOf: (ms: number) => number,
  width: number,
  height: number,
  s: MarkStyle,
) {
  const line = Math.max(1, Math.round(s.dpr));
  const flagH = Math.round(s.flag * s.dpr);
  const tri = Math.round((s.flag / 2) * s.dpr);
  ctx.font = `600 ${Math.round((s.flag - 2) * s.dpr)}px ${s.font}`;
  ctx.textBaseline = "middle";
  ctx.textAlign = "center";
  for (const m of marks) {
    const x = xOf(m.ms);
    const xEnd = m.endMs === null ? x : xOf(m.endMs);
    if (xEnd < -flagH * 2 || x > width + line) continue;
    ctx.fillStyle = m.colour;
    if (m.endMs !== null) {
      // Bar along the bottom to the end, and a short end line.
      const bar = line * (m.auto ? 3 : 1);
      ctx.fillRect(x, height - bar, Math.max(line, xEnd - x), bar);
      ctx.fillRect(xEnd, height - Math.round(height / 3), line, Math.round(height / 3));
    }
    ctx.fillRect(x, 0, line, height);
    if (m.kind === "hot") {
      const w = flagH;
      ctx.fillRect(x, 0, w, flagH);
      ctx.fillStyle = darkText(m.colour) ? s.dark : s.light;
      ctx.fillText(m.label ?? "", x + w / 2 + line / 2, flagH / 2 + s.dpr / 2);
    } else if (m.kind === "loopIn") {
      // A bracket "[" opening to the right: the loop starts here.
      const arm = Math.round(s.flag * 0.6 * s.dpr);
      ctx.fillRect(x, 0, arm, line * 2);
      ctx.fillRect(x, height - line * 2, arm, line * 2);
    } else {
      ctx.beginPath();
      ctx.moveTo(x + line / 2 - tri, height);
      ctx.lineTo(x + line / 2 + tri, height);
      ctx.lineTo(x + line / 2, height - tri);
      ctx.closePath();
      ctx.fill();
    }
  }
}

/** Shades the loop between `x0` and `x1` (device px) with its edges in `edge`. */
export function drawLoop(
  ctx: CanvasRenderingContext2D,
  x0: number,
  x1: number,
  height: number,
  fill: string,
  edge: string,
  dpr: number,
) {
  const line = Math.max(1, Math.round(dpr));
  ctx.fillStyle = fill;
  ctx.fillRect(x0, 0, x1 - x0, height);
  ctx.fillStyle = edge;
  ctx.fillRect(x0, 0, line, height);
  ctx.fillRect(x1 - line, 0, line, height);
}
