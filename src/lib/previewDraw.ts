/**
 * The track list's waveform thumbnail, drawn the way rekordbox draws its
 * colour preview (presentation only). The payload is `waveform_overview`'s:
 * 4 bytes per column, `[height, low, mid, high]`, each 0..=127
 * (`reks-anlz::preview`).
 *
 * Each column's colour is its low, mid and high levels mixed from the
 * preview tokens (pure R, G, B, as rekordbox does) and normalised so the
 * strongest channel leads, lifted a little towards white. Brightness falls
 * off from the centre line towards each column's edge.
 */
import { hexRgb } from "./waveDraw";

/** Largest payload value. */
export const PREVIEW_LEVEL_MAX = 127;
/** Share of white mixed into every colour. */
const LIFT = 0.1;
/** Brightness on the centre line (0..1). */
const PEAK = 0.91;
/** Brightness lost from the centre line to a column's edge. */
const FALLOFF = 0.45;

export interface PreviewPalette {
  low: string;
  mid: string;
  high: string;
}

export function previewPalette(token: (name: string) => string): PreviewPalette {
  return {
    low: token("--color-waveform-preview-low"),
    mid: token("--color-waveform-preview-mid"),
    high: token("--color-waveform-preview-high"),
  };
}

/** One pixel column: `half` height (0..1 of the half-height) and its colour (0..1 per channel). */
export interface PreviewBar {
  half: number;
  rgb: [number, number, number];
}

/** The bars for a view `width` pixels wide: each pixel averages its share of the columns. */
export function previewBars(
  payload: Uint8Array,
  width: number,
  palette: PreviewPalette,
): PreviewBar[] {
  const n = Math.floor(payload.length / 4);
  if (n === 0 || width <= 0) return [];
  const colours = [palette.low, palette.mid, palette.high].map(hexRgb);
  const bars: PreviewBar[] = [];
  for (let x = 0; x < width; x++) {
    const a = Math.min(n - 1, Math.floor((x * n) / width));
    const b = Math.max(a + 1, Math.floor(((x + 1) * n) / width));
    const v = [0, 0, 0, 0];
    for (let c = a; c < b; c++)
      for (let k = 0; k < 4; k++) v[k] = (v[k] ?? 0) + (payload[4 * c + k] ?? 0);
    const count = b - a;
    const rgb: [number, number, number] = [0, 0, 0];
    colours.forEach((colour, band) => {
      const level = (v[band + 1] ?? 0) / count;
      for (let k = 0; k < 3; k++) rgb[k] = (rgb[k] ?? 0) + level * (colour[k] ?? 0);
    });
    const top = Math.max(...rgb);
    bars.push({
      half: Math.min(1, (v[0] ?? 0) / count / PREVIEW_LEVEL_MAX),
      rgb: top > 0 ? (rgb.map((c) => (c / top) * (1 - LIFT) + LIFT) as PreviewBar["rgb"]) : rgb,
    });
  }
  return bars;
}

/**
 * RGBA pixels for a `width` × `height` canvas. Brightness falls off from the
 * centre line towards each column's edge; the edge pixel is partly covered
 * (alpha), so the outline is smooth on any row background.
 */
export function previewPixels(
  payload: Uint8Array,
  width: number,
  height: number,
  palette: PreviewPalette,
): Uint8ClampedArray<ArrayBuffer> {
  const px = new Uint8ClampedArray(width * height * 4);
  const mid = height / 2;
  previewBars(payload, width, palette).forEach((bar, x) => {
    const half = bar.half * mid;
    if (half <= 0) return;
    for (let y = 0; y < height; y++) {
      const dy = Math.abs(y + 0.5 - mid);
      const cover = Math.min(1, half - dy + 0.5);
      if (cover <= 0) continue;
      const bright = PEAK * (1 - (FALLOFF * Math.min(dy, half)) / Math.max(half, 1));
      const i = 4 * (y * width + x);
      px[i] = 255 * bar.rgb[0] * bright;
      px[i + 1] = 255 * bar.rgb[1] * bright;
      px[i + 2] = 255 * bar.rgb[2] * bright;
      px[i + 3] = 255 * cover;
    }
  });
  return px;
}
