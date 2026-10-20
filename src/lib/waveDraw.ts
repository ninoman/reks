/**
 * Canvas drawing for deck waveforms (presentation only). Colours come from
 * tokens.css through the canvas's computed style: a canvas can't use classes.
 */

import { bandPeaks, visibleBeats } from "./gridView";
import type { BeatMark } from "./ipc";

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

/** Low, mid and high bands mirrored round the middle (high scaled down, as in the grid editor). */
export function drawBands(
  ctx: CanvasRenderingContext2D,
  token: (name: string) => string,
  columns: Uint8Array,
  v: WaveView,
) {
  const peaks = bandPeaks(columns, v.startMs, v.pxPerS, v.width);
  const mid = v.height / 2;
  const bands: [number, string][] = [
    [0, token("--color-waveform-low")],
    [1, token("--color-waveform-mid")],
    [2, token("--color-waveform-high")],
  ];
  for (const [band, colour] of bands) {
    ctx.fillStyle = colour;
    for (let x = 0; x < v.width; x++) {
      const h = (peaks[3 * x + band] ?? 0) * mid * (band === 2 ? 0.6 : 1);
      if (h > 0.5) ctx.fillRect(x, mid - h, 1, 2 * h);
    }
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
