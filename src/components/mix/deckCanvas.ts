/**
 * Drawing one frame of a deck's scrolling view and overview, in device
 * pixels (no transform): loop band, waveform tiles, beat lines, cue marks,
 * playhead. Colours come from tokens.css (a canvas can't use classes).
 */
import { type DeckMark, drawLoop, drawMarks } from "../../lib/deckMarks";
import { frameMs, frameX, scrollFrame } from "../../lib/deckScroll";
import type { BeatMark, LoopView } from "../../lib/ipc";
import { wavePalette } from "../../lib/waveDraw";
import type { WaveStrip } from "../../lib/waveStrip";
import type { CanvasSize } from "./useCanvasSize";

/** Hot cue flag height, CSS px, on the scrolling view and the overview. */
const FLAG_PX = 12;
const OVERVIEW_FLAG_PX = 9;

export function sizeCanvas(canvas: HTMLCanvasElement, size: CanvasSize) {
  if (canvas.width !== size.width || canvas.height !== size.height) {
    canvas.width = size.width;
    canvas.height = size.height;
    canvas.style.width = `${size.width / size.dpr}px`;
    canvas.style.height = `${size.height / size.dpr}px`;
  }
}

/** Token colours the canvases draw with (a canvas can't use classes). */
export function readTokens(el: Element) {
  const style = getComputedStyle(el);
  const token = (name: string) => style.getPropertyValue(name).trim();
  return {
    palette: wavePalette(token),
    deck: token("--color-deck"),
    fg: token("--color-deck-fg"),
    overlay: token("--color-overlay"),
    beat: token("--color-grid-beat"),
    downbeat: token("--color-grid-downbeat"),
    loop: token("--color-deck-loop"),
    marks: {
      hot: token("--color-deck-hotcue"),
      memory: token("--color-deck-memory"),
      autoLoop: token("--color-deck-auto-loop"),
      cue: token("--color-deck-cue"),
      loopIn: token("--color-deck-loop-in"),
    },
    font: token("--font-sans"),
  };
}

export type Tokens = ReturnType<typeof readTokens>;

/** Everything a frame draws from, refreshed on each render. */
export interface Inputs {
  scrollSize: CanvasSize | null;
  overviewSize: CanvasSize | null;
  strip: WaveStrip | null;
  overviewStrip: WaveStrip | null;
  tokens: Tokens | null;
  beats: BeatMark[] | null;
  marks: DeckMark[];
  loop: Pick<LoopView, "startMs" | "endMs"> | null;
  seconds: number;
  durationMs: number;
}

/** Index of the first beat at or after `us` (beats are sorted). */
export function firstBeatFrom(beats: BeatMark[], us: number): number {
  let lo = 0;
  let hi = beats.length;
  while (lo < hi) {
    const m = (lo + hi) >> 1;
    if ((beats[m]?.positionUs ?? 0) < us) lo = m + 1;
    else hi = m;
  }
  return lo;
}

export function drawScroll(
  canvas: HTMLCanvasElement,
  size: CanvasSize,
  i: Inputs,
  t: Tokens,
  at: number,
) {
  sizeCanvas(canvas, size);
  const ctx = canvas.getContext("2d");
  if (ctx === null) return;
  const { width, height, dpr } = size;
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.clearRect(0, 0, width, height);
  const f = scrollFrame(at, i.seconds, width);
  const xOf = (ms: number) => frameX(f, ms);
  if (i.loop !== null) {
    drawLoop(ctx, xOf(i.loop.startMs), xOf(i.loop.endMs), height, t.loop, t.marks.cue, dpr);
  }
  i.strip?.draw(ctx, f.left, width);
  if (i.beats !== null) {
    const line = Math.max(1, Math.round(dpr));
    const beats = i.beats;
    // Skip to the view instead of walking the track's beats from the start each frame.
    for (let k = firstBeatFrom(beats, frameMs(f, -2 * line) * 1000); k < beats.length; k++) {
      const b = beats[k];
      if (b === undefined) break;
      const x = xOf(b.positionUs / 1000);
      if (x < -2 * line) continue;
      if (x > width) break;
      const downbeat = b.beatInBar === 1;
      ctx.fillStyle = downbeat ? t.downbeat : t.beat;
      ctx.fillRect(x, 0, downbeat ? 2 * line : line, height);
    }
  }
  drawMarks(ctx, i.marks, xOf, width, height, {
    dpr,
    flag: FLAG_PX,
    font: t.font,
    dark: t.deck,
    light: t.fg,
  });
  ctx.fillStyle = t.fg;
  const playhead = 2 * Math.max(1, Math.round(dpr));
  ctx.fillRect(f.mid - playhead / 2, 0, playhead, height);
}

export function drawOverview(
  canvas: HTMLCanvasElement,
  size: CanvasSize,
  i: Inputs,
  t: Tokens,
  at: number,
) {
  sizeCanvas(canvas, size);
  const ctx = canvas.getContext("2d");
  if (ctx === null || i.durationMs <= 0) return;
  const { width, height, dpr } = size;
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.clearRect(0, 0, width, height);
  const xOf = (ms: number) => Math.round((ms / i.durationMs) * width);
  if (i.loop !== null) {
    drawLoop(ctx, xOf(i.loop.startMs), xOf(i.loop.endMs), height, t.loop, t.marks.cue, dpr);
  }
  i.overviewStrip?.draw(ctx, 0, width);
  ctx.fillStyle = t.overlay;
  ctx.fillRect(0, 0, xOf(at), height);
  drawMarks(ctx, i.marks, xOf, width, height, {
    dpr,
    flag: OVERVIEW_FLAG_PX,
    font: t.font,
    dark: t.deck,
    light: t.fg,
  });
  ctx.fillStyle = t.fg;
  const playhead = 2 * Math.max(1, Math.round(dpr));
  ctx.fillRect(xOf(at) - playhead / 2, 0, playhead, height);
}
