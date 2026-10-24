/**
 * The close-zoom deck waveform (2 s across the view and closer): signed
 * peaks at `DETAIL_POINTS_PER_SECOND` that Rust builds from the loaded deck's
 * audio (reks-analysis `detail`), fetched in chunks nearest the playhead
 * first. This module holds the chunks and turns them into per-pixel peaks
 * (pure); `useDeckDetail` fetches them.
 */
import { DETAIL_POINT_BYTES, DETAIL_POINTS_PER_SECOND } from "./ipc";

/** Points per chunk (30 s): about 450 KB, tens of ms to build. */
export const CHUNK_POINTS = 30 * DETAIL_POINTS_PER_SECOND;
/** Chunks kept per deck (10 min, about 4.5 MB); a long mix keeps those nearest the playhead. */
export const MAX_CHUNKS = 20;
/** Widest zoom (seconds across the view) drawn from the detail. */
export const DETAIL_MAX_ZOOM_S = 2;

/** Detail point at track time `ms`. */
export function pointAt(ms: number): number {
  return Math.floor((ms * DETAIL_POINTS_PER_SECOND) / 1000);
}

/**
 * The chunks worth holding with the playhead in chunk `at`, nearest first
 * (ahead before behind at the same distance): at most `max` of `count`.
 */
export function wantedChunks(at: number, count: number, max = MAX_CHUNKS): number[] {
  const out: number[] = [];
  const centre = Math.min(Math.max(0, at), count - 1);
  for (let d = 0; out.length < Math.min(max, count); d++) {
    for (const i of d === 0 ? [centre] : [centre + d, centre - d]) {
      if (i >= 0 && i < count && out.length < max) out.push(i);
    }
  }
  return out;
}

/** One deck load's detail chunks. */
export class DetailChunks {
  /** Points in the track (from its duration; the last chunk may come back shorter). */
  readonly totalPoints: number;
  /** Bumped on every change, so a strip can tell its fallback tiles may now draw from it. */
  version = 0;
  private chunks = new Map<number, { min: Int8Array; bytes: Uint8Array }>();

  constructor(totalPoints: number) {
    this.totalPoints = Math.max(0, Math.ceil(totalPoints));
  }

  get chunkCount(): number {
    return Math.ceil(this.totalPoints / CHUNK_POINTS);
  }

  has(chunk: number): boolean {
    return this.chunks.has(chunk);
  }

  /** The nearest wanted chunk not held yet with the playhead at point `at`; null when all are. */
  next(at: number): number | null {
    const wanted = wantedChunks(Math.floor(at / CHUNK_POINTS), this.chunkCount);
    return wanted.find((i) => !this.chunks.has(i)) ?? null;
  }

  /** Adds `chunk` and drops the held chunks no longer wanted with the playhead at point `at`. */
  add(chunk: number, bytes: Uint8Array, at: number) {
    this.chunks.set(chunk, {
      bytes,
      min: new Int8Array(bytes.buffer, bytes.byteOffset, bytes.byteLength),
    });
    const wanted = new Set(wantedChunks(Math.floor(at / CHUNK_POINTS), this.chunkCount));
    for (const i of this.chunks.keys()) if (!wanted.has(i)) this.chunks.delete(i);
    this.version++;
  }

  /** Every point of `first..end` inside the track is held. */
  covers(first: number, end: number): boolean {
    const lo = Math.max(0, first);
    const hi = Math.min(this.totalPoints, end);
    if (hi <= lo) return true;
    for (let c = Math.floor(lo / CHUNK_POINTS); c <= Math.floor((hi - 1) / CHUNK_POINTS); c++) {
      if (!this.chunks.has(c)) return false;
    }
    return true;
  }

  /**
   * Point `k` into `out` as `[min, max, low, mid, high]`: min and max in
   * −1..1, bands 0..1. Silent outside the track or a held chunk.
   */
  read(k: number, out: Float32Array) {
    out.fill(0);
    if (k < 0) return;
    const c = this.chunks.get(Math.floor(k / CHUNK_POINTS));
    if (c === undefined) return;
    const o = (k % CHUNK_POINTS) * DETAIL_POINT_BYTES;
    if (o + DETAIL_POINT_BYTES > c.bytes.length) return;
    out[0] = (c.min[o] ?? 0) / 127;
    out[1] = (c.min[o + 1] ?? 0) / 127;
    out[2] = (c.bytes[o + 2] ?? 0) / 255;
    out[3] = (c.bytes[o + 3] ?? 0) / 255;
    out[4] = (c.bytes[o + 4] ?? 0) / 255;
  }
}

/** Per-pixel signed peaks: `lo`/`hi` in −1..1, `bands` `[low, mid, high]` 0..1. */
export interface DetailPeaks {
  lo: Float32Array;
  hi: Float32Array;
  bands: Float32Array;
}

/** The points a strip of `width` pixels from track pixel `first` reads, at `ptsPerPx`. */
export function detailSpan(first: number, ptsPerPx: number, width: number): [number, number] {
  return [Math.floor(first * ptsPerPx) - 1, Math.ceil((first + width) * ptsPerPx) + 2];
}

/**
 * Peaks for `width` pixels from track pixel `first`, at `ptsPerPx` points a
 * pixel; null unless `chunks` holds them all. Wider than a pixel, a pixel
 * spans its points' lowest min to highest max. Zoomed past one point a
 * pixel, mins and maxes are interpolated between point centres at both
 * pixel edges, so neighbouring pixels join into one continuous line.
 */
export function detailPeaks(
  chunks: DetailChunks,
  first: number,
  ptsPerPx: number,
  width: number,
): DetailPeaks | null {
  const [from, to] = detailSpan(first, ptsPerPx, width);
  if (!chunks.covers(from, to)) return null;
  const lo = new Float32Array(width);
  const hi = new Float32Array(width);
  const bands = new Float32Array(width * 3);
  const a = new Float32Array(5);
  const b = new Float32Array(5);
  /** Interpolated `[min, max, low, mid, high]` at point position `p` (centres at k + 0.5). */
  const at = (p: number, out: Float32Array) => {
    const q = p - 0.5;
    const k = Math.floor(q);
    const t = q - k;
    chunks.read(k, a);
    chunks.read(k + 1, b);
    for (let j = 0; j < 5; j++) out[j] = (a[j] ?? 0) * (1 - t) + (b[j] ?? 0) * t;
  };
  const left = new Float32Array(5);
  const right = new Float32Array(5);
  for (let x = 0; x < width; x++) {
    const px = first + x;
    if (ptsPerPx >= 1) {
      const k0 = Math.floor(px * ptsPerPx);
      const k1 = Math.max(k0 + 1, Math.floor((px + 1) * ptsPerPx));
      let min = 1;
      let max = -1;
      let low = 0;
      let mid = 0;
      let high = 0;
      for (let k = k0; k < k1; k++) {
        chunks.read(k, a);
        min = Math.min(min, a[0] ?? 0);
        max = Math.max(max, a[1] ?? 0);
        low = Math.max(low, a[2] ?? 0);
        mid = Math.max(mid, a[3] ?? 0);
        high = Math.max(high, a[4] ?? 0);
      }
      lo[x] = min;
      hi[x] = max;
      bands[3 * x] = low;
      bands[3 * x + 1] = mid;
      bands[3 * x + 2] = high;
    } else {
      at(px * ptsPerPx, left);
      at((px + 1) * ptsPerPx, right);
      lo[x] = Math.min(left[0] ?? 0, right[0] ?? 0);
      hi[x] = Math.max(left[1] ?? 0, right[1] ?? 0);
      for (let j = 0; j < 3; j++) {
        bands[3 * x + j] = ((left[2 + j] ?? 0) + (right[2 + j] ?? 0)) / 2;
      }
    }
  }
  return { lo, hi, bands };
}
