/**
 * Drawing helpers for the grid editor's waveform (presentation only: the
 * grid maths are in Rust, `reks-core` `beatgrid/`).
 */
import { type BeatMark, COLUMNS_PER_SECOND, type GridAnchor } from "./ipc";

/** Zoom levels, pixels per second; 150 = one column per pixel. */
export const ZOOM_PX_PER_S = [25, 50, 100, 150, 300, 600] as const;
export const DEFAULT_ZOOM = 2;
/** Top of a column's mono byte (`waveformColumns`). */
const MONO_MAX = 31;

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

/** Peak mono height (0..1) per pixel, mapped like [`bandPeaks`]. */
export function monoPeaks(
  columns: Uint8Array,
  startMs: number,
  pxPerS: number,
  width: number,
): Float32Array {
  const out = new Float32Array(width);
  const count = columns.length / 4;
  const colsPerPx = COLUMNS_PER_SECOND / pxPerS;
  const first = (startMs / 1000) * COLUMNS_PER_SECOND;
  for (let x = 0; x < width; x++) {
    const c0 = Math.floor(first + x * colsPerPx);
    const c1 = Math.max(c0 + 1, Math.floor(first + (x + 1) * colsPerPx));
    let mono = 0;
    for (let c = Math.max(0, c0); c < Math.min(count, c1); c++) {
      mono = Math.max(mono, columns[4 * c + 3] ?? 0);
    }
    out[x] = mono / MONO_MAX;
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

/** A tempo section as the editor lists it: from its marker (the first from 0) to the next. */
export interface SectionView {
  index: number;
  startMs: number;
  endMs: number;
  bpmX100: number;
}

/** The sections of `anchors` (sorted) over a track `durationMs` long. */
export function sectionViews(anchors: GridAnchor[], durationMs: number): SectionView[] {
  return anchors.map((a, index) => ({
    index,
    startMs: index === 0 ? 0 : a.positionMs,
    endMs: anchors[index + 1]?.positionMs ?? Math.max(durationMs, a.positionMs),
    bpmX100: a.bpmX100,
  }));
}

/** Index of the section holding `ms` (the last marker at or before it, else the first); null without markers. */
export function sectionAt(anchors: GridAnchor[], ms: number): number | null {
  if (anchors.length === 0) return null;
  let i = 0;
  while (i + 1 < anchors.length && (anchors[i + 1]?.positionMs ?? Infinity) <= ms) i++;
  return i;
}

/** The marker within `tolMs` of `ms` (the nearest), or null. */
export function markerNear(anchors: GridAnchor[], ms: number, tolMs: number): number | null {
  let best: number | null = null;
  anchors.forEach((a, i) => {
    const d = Math.abs(a.positionMs - ms);
    const bestD = best === null ? Infinity : Math.abs((anchors[best]?.positionMs ?? 0) - ms);
    if (d <= tolMs && d < bestD) best = i;
  });
  return best;
}

/** Position (ms) of the beat line within `tolMs` of `ms` (the nearest), or null. */
export function beatNear(beats: BeatMark[], ms: number, tolMs: number): number | null {
  let best: number | null = null;
  for (const b of beats) {
    const at = b.positionUs / 1000;
    if (Math.abs(at - ms) <= tolMs && (best === null || Math.abs(at - ms) < Math.abs(best - ms))) {
      best = at;
    }
  }
  return best === null ? null : Math.round(best);
}
