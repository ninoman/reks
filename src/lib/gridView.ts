/**
 * Reading waveform columns and grid anchors for views (presentation only:
 * the grid maths are in Rust, `reks-core` `beatgrid/`).
 */
import { COLUMNS_PER_SECOND, type GridAnchor } from "./ipc";

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

/** Index of the section holding `ms` (the last marker at or before it, else the first); null without markers. */
export function sectionAt(anchors: GridAnchor[], ms: number): number | null {
  if (anchors.length === 0) return null;
  let i = 0;
  while (i + 1 < anchors.length && (anchors[i + 1]?.positionMs ?? Infinity) <= ms) i++;
  return i;
}
