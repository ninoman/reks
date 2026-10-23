/**
 * The scrolling deck waveform as cached tiles in device pixels. Each tile is
 * drawn once from the analysis columns on a pixel grid fixed to the track,
 * then every frame only copies tiles at a whole-pixel offset: crisp on HiDPI
 * screens, no shimmer as peaks would re-bin under a moving view, and a few
 * `drawImage` calls a frame instead of a bar per pixel.
 */
import { COLUMNS_PER_SECOND, type DeckWaveform } from "./ipc";
import { centreLineColour, type WavePalette, waveBars } from "./waveDraw";

/** Device pixels per tile. */
export const TILE_PX = 512;
/** Tiles kept per strip: the view plus some either side. */
const MAX_TILES = 16;
/** Top of a column's mono byte (`waveformColumns`). */
const MONO_MAX = 31;

/**
 * Per-pixel `[low, mid, high]` and mono levels (0..1) for `width` pixels
 * from track pixel `first`, at `colsPerPx` columns a pixel. Wider than a
 * pixel, a pixel takes the peak of its columns; zoomed past one column a
 * pixel, levels are interpolated between column centres (no blocky steps).
 */
export function stripPeaks(
  columns: Uint8Array,
  first: number,
  colsPerPx: number,
  width: number,
): { bands: Float32Array; mono: Float32Array } {
  const bands = new Float32Array(width * 3);
  const mono = new Float32Array(width);
  const count = columns.length / 4;
  const at = (c: number, k: number) => (c >= 0 && c < count ? (columns[4 * c + k] ?? 0) : 0);
  const level = new Float32Array(4);
  for (let x = 0; x < width; x++) {
    const px = first + x;
    level.fill(0);
    if (colsPerPx >= 1) {
      const c0 = Math.floor(px * colsPerPx);
      const c1 = Math.max(c0 + 1, Math.floor((px + 1) * colsPerPx));
      for (let c = Math.max(0, c0); c < Math.min(count, c1); c++) {
        for (let k = 0; k < 4; k++) level[k] = Math.max(level[k] ?? 0, at(c, k));
      }
    } else {
      const p = (px + 0.5) * colsPerPx - 0.5;
      const c = Math.floor(p);
      const t = p - c;
      for (let k = 0; k < 4; k++) level[k] = at(c, k) * (1 - t) + at(c + 1, k) * t;
    }
    bands[3 * x] = (level[0] ?? 0) / 255;
    bands[3 * x + 1] = (level[1] ?? 0) / 255;
    bands[3 * x + 2] = (level[2] ?? 0) / 255;
    mono[x] = (level[3] ?? 0) / MONO_MAX;
  }
  return { bands, mono };
}

/**
 * A whole track's `columns` stretched over a canvas `width` × `height`
 * device pixels (no transform): the track-list thumbnail.
 */
export function drawOverview(
  ctx: CanvasRenderingContext2D,
  palette: WavePalette,
  style: DeckWaveform,
  columns: Uint8Array,
  width: number,
  height: number,
) {
  ctx.clearRect(0, 0, width, height);
  const count = columns.length / 4;
  if (count === 0 || width === 0) return;
  const { bands, mono } = stripPeaks(columns, 0, count / width, width);
  const mid = height / 2;
  for (const bar of waveBars(style, bands, mono, height, palette)) {
    ctx.fillStyle = bar.colour;
    ctx.fillRect(bar.x, mid - bar.half, 1, 2 * bar.half);
  }
}

/** What a strip draws; a new strip is made when any of it changes. */
export interface StripSpec {
  columns: Uint8Array;
  style: DeckWaveform;
  palette: WavePalette;
  /** Device pixels per second. */
  pxPerS: number;
  /** Device pixels. */
  height: number;
}

export class WaveStrip {
  readonly spec: StripSpec;
  private tiles = new Map<number, HTMLCanvasElement>();

  constructor(spec: StripSpec) {
    this.spec = spec;
  }

  /** Copies the strip from track pixel `left` into `ctx` (device pixels, no transform). */
  draw(ctx: CanvasRenderingContext2D, left: number, width: number) {
    const first = Math.floor(left / TILE_PX);
    const last = Math.floor((left + width - 1) / TILE_PX);
    for (let i = first; i <= last; i++) {
      const tile = this.tile(i);
      if (tile !== null) ctx.drawImage(tile, i * TILE_PX - left, 0);
    }
  }

  private tile(i: number): HTMLCanvasElement | null {
    const cached = this.tiles.get(i);
    if (cached !== undefined) {
      // Most recently used last, so the oldest goes first.
      this.tiles.delete(i);
      this.tiles.set(i, cached);
      return cached;
    }
    const { columns, style, palette, pxPerS, height } = this.spec;
    const colsPerPx = COLUMNS_PER_SECOND / pxPerS;
    const start = i * TILE_PX;
    if (start + TILE_PX < 0 || start * colsPerPx > columns.length / 4) return null;
    const tile = document.createElement("canvas");
    tile.width = TILE_PX;
    tile.height = height;
    const ctx = tile.getContext("2d");
    if (ctx === null) return null;
    const { bands, mono } = stripPeaks(columns, start, colsPerPx, TILE_PX);
    const mid = height / 2;
    // A flat line under the bars keeps silent stretches continuous; it stops
    // at the track's ends, where the view stays empty.
    const from = Math.max(0, -start);
    const to = Math.min(TILE_PX, Math.ceil(columns.length / 4 / colsPerPx) - start);
    if (to > from) {
      ctx.fillStyle = centreLineColour(style, palette);
      ctx.fillRect(from, Math.floor(mid), to - from, 1);
    }
    for (const bar of waveBars(style, bands, mono, height, palette)) {
      ctx.fillStyle = bar.colour;
      ctx.fillRect(bar.x, mid - bar.half, 1, 2 * bar.half);
    }
    this.tiles.set(i, tile);
    if (this.tiles.size > MAX_TILES) {
      const oldest = this.tiles.keys().next().value;
      if (oldest !== undefined) this.tiles.delete(oldest);
    }
    return tile;
  }
}
