/**
 * Canvas drawing for deck waveforms (presentation only). Colours come from
 * tokens.css through the canvas's computed style: a canvas can't use classes.
 */

import type { DeckWaveform } from "./ipc";

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
 * Band levels are raised to this power before mixing so the dominant band sets
 * the hue; a linear mix of three similar levels washes out towards white.
 */
const MIX_SHARPNESS = 3;
/** The strongest channel's level after mixing (0..255), below full to keep it from glaring. */
const MIX_PEAK = 217;

/**
 * The band colours weighted by the band levels, scaled so the strongest
 * channel is `MIX_PEAK`: the hue says which bands dominate, the height says how loud.
 */
export function mixColour(colours: [number, number, number][], weights: number[]): string {
  const rgb: [number, number, number] = [0, 0, 0];
  colours.forEach((c, i) => {
    const w = (weights[i] ?? 0) ** MIX_SHARPNESS;
    for (let k = 0; k < 3; k++) rgb[k] = (rgb[k] ?? 0) + w * (c[k] ?? 0);
  });
  const top = Math.max(...rgb);
  return css(top > 0 ? (rgb.map((v) => (v * MIX_PEAK) / top) as [number, number, number]) : rgb);
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
  const colour = bandColour(style, palette);
  for (let x = 0; x < width; x++) {
    const half = (mono[x] ?? 0) * mid;
    if (half <= MIN_HALF) continue;
    out.push({
      x,
      half,
      colour: colour(bands[3 * x] ?? 0, bands[3 * x + 1] ?? 0, bands[3 * x + 2] ?? 0),
    });
  }
  return out;
}

/**
 * A pixel's colour in `style` from its band levels (0..1). RGB mixes the RGB
 * band colours and Blue shades towards its high colour; 3-band, drawn as one
 * line at close zoom, mixes its own band colours like RGB.
 */
export function bandColour(
  style: DeckWaveform,
  palette: WavePalette,
): (low: number, mid: number, high: number) => string {
  if (style === "blue") {
    const blue = hexRgb(palette.blue);
    const blueHigh = hexRgb(palette.blueHigh);
    return (low, mid, high) => {
      const sum = low + mid + high;
      return lerpColour(blue, blueHigh, sum > 0 ? high / sum : 0);
    };
  }
  const colours = (
    style === "rgb"
      ? [palette.rgbLow, palette.rgbMid, palette.rgbHigh]
      : [palette.low, palette.mid, palette.high]
  ).map(hexRgb);
  return (low, mid, high) => mixColour(colours, [low, mid, high]);
}

/** One filled column from `top` down `h` px, CSS colour. */
export interface WaveRect {
  x: number;
  top: number;
  h: number;
  colour: string;
}

/**
 * The close-zoom bars from per-pixel signed peaks (`detailPeaks`): each pixel
 * a line from its min to its max, uneven about the middle of a view `height`
 * px tall (up is positive), at least 1 px. Silent pixels are skipped; the
 * strip's centre line covers them.
 */
export function detailBars(
  style: DeckWaveform,
  lo: Float32Array,
  hi: Float32Array,
  bands: Float32Array,
  height: number,
  palette: WavePalette,
): WaveRect[] {
  const mid = height / 2;
  const colour = bandColour(style, palette);
  const out: WaveRect[] = [];
  for (let x = 0; x < lo.length; x++) {
    const l = lo[x] ?? 0;
    const h = hi[x] ?? 0;
    if (Math.max(Math.abs(l), Math.abs(h)) * mid <= MIN_HALF) continue;
    const top = Math.floor(mid - h * mid);
    const bottom = Math.ceil(mid - l * mid);
    out.push({
      x,
      top,
      h: Math.max(1, bottom - top),
      colour: colour(bands[3 * x] ?? 0, bands[3 * x + 1] ?? 0, bands[3 * x + 2] ?? 0),
    });
  }
  return out;
}

/** The colour of the flat centre line drawn where the waveform is silent. */
export function centreLineColour(style: DeckWaveform, palette: WavePalette): string {
  if (style === "threeBand") return palette.mid;
  return style === "rgb" ? palette.rgbMid : palette.blue;
}
