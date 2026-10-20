/**
 * Drawing helpers for the grid editor's waveform (presentation only: the
 * grid maths are in Rust, `reks-core` `beatgrid.rs`).
 */
import { type BeatMark, COLUMNS_PER_SECOND } from "./ipc";

/** Zoom levels, pixels per second; 150 = one column per pixel. */
export const ZOOM_PX_PER_S = [25, 50, 100, 150, 300, 600] as const;
export const DEFAULT_ZOOM = 2;

/** Peak `[low, mid, high]` (0..1) per pixel of a view `width` px wide from `startMs`. */
export function bandPeaks(
  columns: Uint8Array,
  startMs: number,
  pxPerS: number,
  width: number,
): Float32Array {
  const out = new Float32Array(width * 3);
  const count = columns.length / 4;
  const colsPerPx = COLUMNS_PER_SECOND / pxPerS;
  const first = (startMs / 1000) * COLUMNS_PER_SECOND;
  for (let x = 0; x < width; x++) {
    const c0 = Math.floor(first + x * colsPerPx);
    const c1 = Math.max(c0 + 1, Math.floor(first + (x + 1) * colsPerPx));
    let low = 0;
    let mid = 0;
    let high = 0;
    for (let c = Math.max(0, c0); c < Math.min(count, c1); c++) {
      low = Math.max(low, columns[4 * c] ?? 0);
      mid = Math.max(mid, columns[4 * c + 1] ?? 0);
      high = Math.max(high, columns[4 * c + 2] ?? 0);
    }
    out[3 * x] = low / 255;
    out[3 * x + 1] = mid / 255;
    out[3 * x + 2] = high / 255;
  }
  return out;
}

/** Beats inside the view, with their x (px, fractional). */
export function visibleBeats(
  beats: BeatMark[],
  startMs: number,
  pxPerS: number,
  width: number,
): { x: number; downbeat: boolean }[] {
  const out: { x: number; downbeat: boolean }[] = [];
  for (const b of beats) {
    const x = ((b.positionUs / 1000 - startMs) * pxPerS) / 1000;
    if (x < -1) continue;
    if (x > width + 1) break;
    out.push({ x, downbeat: b.beatInBar === 1 });
  }
  return out;
}

/** Track length of `columns` in ms. */
export function columnsDurationMs(columns: Uint8Array): number {
  return Math.round(((columns.length / 4) * 1000) / COLUMNS_PER_SECOND);
}
