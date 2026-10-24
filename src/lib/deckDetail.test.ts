import { describe, expect, it } from "vitest";

import { CHUNK_POINTS, DetailChunks, detailPeaks, MAX_CHUNKS, wantedChunks } from "./deckDetail";
import { DETAIL_POINT_BYTES } from "./ipc";

/** A chunk from `[min, max, low, mid, high]` rows (min and max as −127..127). */
function chunk(rows: number[][]): Uint8Array {
  const out = new Uint8Array(rows.length * DETAIL_POINT_BYTES);
  const signed = new Int8Array(out.buffer);
  rows.forEach(([min = 0, max = 0, low = 0, mid = 0, high = 0], i) => {
    const o = i * DETAIL_POINT_BYTES;
    signed[o] = min;
    signed[o + 1] = max;
    out.set([low, mid, high], o + 2);
  });
  return out;
}

const round = (a: Float32Array) => [...a].map((v) => Math.round(v * 100) / 100);

describe("wantedChunks", () => {
  it("takes the playhead's chunk, then nearest outward, ahead first", () => {
    expect(wantedChunks(3, 10, 4)).toEqual([3, 4, 2, 5]);
    expect(wantedChunks(0, 10, 3)).toEqual([0, 1, 2]);
    expect(wantedChunks(9, 10, 3)).toEqual([9, 8, 7]);
    expect(wantedChunks(50, 3, 20)).toEqual([2, 1, 0]);
    expect(wantedChunks(0, 0)).toEqual([]);
  });
});

describe("DetailChunks", () => {
  it("fetches nearest the playhead first and keeps a long mix's chunks near it", () => {
    // Two hours: 240 chunks.
    const c = new DetailChunks(240 * CHUNK_POINTS);
    const at = 100 * CHUNK_POINTS + 5;
    expect(c.next(at)).toBe(100);
    c.add(100, chunk([[0, 0]]), at);
    expect(c.next(at)).toBe(101);
    for (let i = 0; i < 40; i++) {
      const n = c.next(at);
      if (n === null) break;
      c.add(n, chunk([[0, 0]]), at);
    }
    expect(c.next(at)).toBeNull();
    // Moved far on: the old chunks go as new ones arrive.
    const later = 200 * CHUNK_POINTS;
    c.add(200, chunk([[0, 0]]), later);
    expect(c.has(100)).toBe(false);
    expect(c.has(200)).toBe(true);
    expect(MAX_CHUNKS).toBeLessThan(240);
  });

  it("covers only ranges whose chunks are held, ignoring points outside the track", () => {
    const c = new DetailChunks(CHUNK_POINTS + 10);
    expect(c.covers(-5, 3)).toBe(false);
    c.add(0, chunk([[1, 2]]), 0);
    expect(c.covers(-5, 3)).toBe(true);
    expect(c.covers(CHUNK_POINTS - 1, CHUNK_POINTS + 1)).toBe(false);
    c.add(1, chunk([[1, 2]]), 0);
    expect(c.covers(CHUNK_POINTS - 1, CHUNK_POINTS + 999)).toBe(true);
    expect(c.version).toBe(2);
  });
});

describe("detailPeaks", () => {
  // Points: a rising edge, a crest, a dip below centre.
  const c = new DetailChunks(4);
  c.add(
    0,
    chunk([
      [-127, -64, 255, 0, 0],
      [-64, 64, 0, 255, 0],
      [64, 127, 0, 0, 255],
      [-32, 0, 0, 0, 0],
    ]),
    0,
  );

  it("spans each pixel's points from lowest min to highest max when zoomed out", () => {
    const p = detailPeaks(c, 0, 2, 2);
    expect(p).not.toBeNull();
    if (p === null) return;
    expect(round(p.lo)).toEqual([-1, -0.25]);
    expect(round(p.hi)).toEqual([0.5, 1]);
    expect(round(p.bands)).toEqual([1, 1, 0, 0, 0, 1]);
  });

  it("joins neighbouring pixels by interpolating at both pixel edges when zoomed in", () => {
    // Two pixels a point: centres at x = 1 and 3.
    const p = detailPeaks(c, 1, 0.5, 2);
    if (p === null) throw new Error("not covered");
    // Pixel 1 runs from point 0's centre to halfway to point 1's.
    expect(round(p.lo)).toEqual([-1, -0.75]);
    expect(round(p.hi)).toEqual([0, 0.5]);
    // The two lines meet: pixel 2 reaches down to where pixel 1 tops out.
    expect(p.lo[1] ?? 1).toBeLessThanOrEqual(p.hi[0] ?? -1);
  });

  it("is null until the chunks it reads arrive", () => {
    expect(detailPeaks(new DetailChunks(CHUNK_POINTS * 2), 0, 1, 4)).toBeNull();
  });
});
