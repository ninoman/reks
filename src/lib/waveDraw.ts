/**
 * Canvas drawing for deck waveforms (presentation only). Colours come from
 * tokens.css through the canvas's computed style: a canvas can't use classes.
 */

import { bandPeaks, monoPeaks, visibleBeats } from "./gridView";
import type { BeatMark, DeckWaveform } from "./ipc";

export interface WaveView {
  startMs: number;
  pxPerS: number;
  width: number;
  height: number;
}

/** Sizes the canvas for the device pixel ratio and returns its context and token reader. */
export function prepare(
  canvas: HTMLCanvasElement,
  width: number,
  height: number,
): { ctx: CanvasRenderingContext2D; token: (name: string) => string } | null {
  const ctx = canvas.getContext("2d");
  if (ctx === null) return null;
  const dpr = window.devicePixelRatio || 1;
  if (canvas.width !== width * dpr || canvas.height !== height * dpr) {
    canvas.width = width * dpr;
    canvas.height = height * dpr;
  }
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, width, height);
  const style = getComputedStyle(canvas);
  return { ctx, token: (name) => style.getPropertyValue(name).trim() };
}

/** The colours a waveform style draws with, read from tokens.css. */
export interface WavePalette {
  low: string;
  mid: string;
  high: string;
  rgbLow: string;
  rgbMid: string;
  rgbHigh: string;
  blue: string;
  blueHigh: string;
}

export function wavePalette(token: (name: string) => string): WavePalette {
  return {
    low: token("--color-waveform-low"),
    mid: token("--color-waveform-mid"),
    high: token("--color-waveform-high"),
    rgbLow: token("--color-waveform-rgb-low"),
    rgbMid: token("--color-waveform-rgb-mid"),
    rgbHigh: token("--color-waveform-rgb-high"),
    blue: token("--color-waveform-blue"),
    blueHigh: token("--color-waveform-blue-high"),
  };
}

/** One filled column: `x`, `half` height either side of the middle, CSS colour. */
export interface WaveBar {
  x: number;
  half: number;
  colour: string;
}

/** Bars shorter than this (px either side) are skipped. */
const MIN_HALF = 0.5;
/** Highs are scaled down in the 3-band view, as in the grid editor. */
const HIGH_SCALE = 0.6;

/** `#rrggbb` as `[r, g, b]`; anything else is black (tokens are hex). */
export function hexRgb(hex: string): [number, number, number] {
  const m = /^#([0-9a-f]{6})$/i.exec(hex.trim());
  if (m === null) return [0, 0, 0];
  const n = Number.parseInt(m[1] ?? "0", 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

function css([r, g, b]: [number, number, number]): string {
  return `rgb(${Math.round(r)} ${Math.round(g)} ${Math.round(b)})`;
}

/**
 * The band colours weighted by the band levels, brightened so the strongest
 * channel is full: the hue says which bands dominate, the height says how loud.
 */
export function mixColour(colours: [number, number, number][], weights: number[]): string {
  const rgb: [number, number, number] = [0, 0, 0];
  colours.forEach((c, i) => {
    const w = weights[i] ?? 0;
    for (let k = 0; k < 3; k++) rgb[k] = (rgb[k] ?? 0) + w * (c[k] ?? 0);
  });
  const top = Math.max(...rgb);
  return css(top > 0 ? (rgb.map((v) => (v * 255) / top) as [number, number, number]) : rgb);
}

/** `a` towards `b` by `t` (0..1). */
function lerpColour(a: [number, number, number], b: [number, number, number], t: number): string {
  return css(a.map((v, k) => v + ((b[k] ?? 0) - v) * t) as [number, number, number]);
}

/**
 * The bars for one view in `style`, from per-pixel `bands` (`[low, mid, high]`,
 * from `bandPeaks`) and `mono` (`monoPeaks`), for a view `height` px tall.
 * 3-band draws three layers (low, mid, high on top); RGB and Blue draw one
 * mono-height bar per pixel, coloured by the band mix.
 */
export function waveBars(
  style: DeckWaveform,
  bands: Float32Array,
  mono: Float32Array,
  height: number,
  palette: WavePalette,
): WaveBar[] {
  const mid = height / 2;
  const width = mono.length;
  const out: WaveBar[] = [];
  if (style === "threeBand") {
    const layers: [number, string, number][] = [
      [0, palette.low, 1],
      [1, palette.mid, 1],
      [2, palette.high, HIGH_SCALE],
    ];
    for (const [band, colour, scale] of layers) {
      for (let x = 0; x < width; x++) {
        const half = (bands[3 * x + band] ?? 0) * mid * scale;
        if (half > MIN_HALF) out.push({ x, half, colour });
      }
    }
    return out;
  }
  const rgb = [palette.rgbLow, palette.rgbMid, palette.rgbHigh].map(hexRgb);
  const blue = hexRgb(palette.blue);
  const blueHigh = hexRgb(palette.blueHigh);
  for (let x = 0; x < width; x++) {
    const half = (mono[x] ?? 0) * mid;
    if (half <= MIN_HALF) continue;
    const low = bands[3 * x] ?? 0;
    const midBand = bands[3 * x + 1] ?? 0;
    const high = bands[3 * x + 2] ?? 0;
    const sum = low + midBand + high;
    const colour =
      style === "rgb"
        ? mixColour(rgb, [low, midBand, high])
        : lerpColour(blue, blueHigh, sum > 0 ? high / sum : 0);
    out.push({ x, half, colour });
  }
  return out;
}

/** The waveform in `style`, mirrored round the middle of the view. */
export function drawWave(
  ctx: CanvasRenderingContext2D,
  palette: WavePalette,
  style: DeckWaveform,
  columns: Uint8Array,
  v: WaveView,
) {
  const bands = bandPeaks(columns, v.startMs, v.pxPerS, v.width);
  const mono = monoPeaks(columns, v.startMs, v.pxPerS, v.width);
  const mid = v.height / 2;
  for (const bar of waveBars(style, bands, mono, v.height, palette)) {
    ctx.fillStyle = bar.colour;
    ctx.fillRect(bar.x, mid - bar.half, 1, 2 * bar.half);
  }
}

export function drawBeats(
  ctx: CanvasRenderingContext2D,
  token: (name: string) => string,
  beats: BeatMark[],
  v: WaveView,
) {
  const beat = token("--color-grid-beat");
  const downbeat = token("--color-grid-downbeat");
  for (const b of visibleBeats(beats, v.startMs, v.pxPerS, v.width)) {
    ctx.fillStyle = b.downbeat ? downbeat : beat;
    ctx.fillRect(Math.round(b.x), 0, b.downbeat ? 2 : 1, v.height);
  }
}

/** A vertical line at `ms` in `colour`, `w` px wide. */
export function drawMarker(
  ctx: CanvasRenderingContext2D,
  colour: string,
  ms: number,
  v: WaveView,
  w = 1,
) {
  const x = Math.round(((ms - v.startMs) * v.pxPerS) / 1000);
  if (x < -w || x > v.width + w) return;
  ctx.fillStyle = colour;
  ctx.fillRect(x - Math.floor(w / 2), 0, w, v.height);
}
