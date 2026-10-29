import { describe, expect, it } from "vitest";

import { bandPeaks, sectionAt } from "./gridView";
import type { GridAnchor } from "./ipc";

/** `n` columns where column i has low = i, mid = 2i, high = 255 - i. */
function cols(n: number): Uint8Array {
  const out = new Uint8Array(n * 4);
  for (let i = 0; i < n; i++) out.set([i, 2 * i, 255 - i, 0], 4 * i);
  return out;
}

describe("bandPeaks", () => {
  it("maps one column per pixel at 150 px/s", () => {
    const p = bandPeaks(cols(10), 0, 150, 3);
    expect([...p.slice(0, 3)]).toEqual([0, 0, 1]);
    expect(p[3]).toBeCloseTo(1 / 255);
    expect(p[7]).toBeCloseTo(4 / 255);
  });

  it("takes the peak of the columns under a pixel when zoomed out", () => {
    // 75 px/s: two columns per pixel; pixel 1 covers columns 2 and 3.
    const p = bandPeaks(cols(10), 0, 75, 2);
    expect(p[3]).toBeCloseTo(3 / 255);
    expect(p[5]).toBeCloseTo(253 / 255);
  });

  it("starts at the view start and is empty past the end", () => {
    // 1000 ms = column 150; only 10 columns exist.
    expect([...bandPeaks(cols(10), 1000, 150, 2)]).toEqual([0, 0, 0, 0, 0, 0]);
    const p = bandPeaks(cols(300), 1000, 150, 1);
    expect(p[0]).toBeCloseTo(150 / 255);
  });
});

const anchor = (positionMs: number, bpmX100: number): GridAnchor => ({
  positionMs,
  bpmX100,
  beatInBar: 1,
  beatsPerBar: 4,
});

describe("sections", () => {
  const two = [anchor(300, 14000), anchor(60_000, 17400)];

  it("finds the section at a time", () => {
    expect(sectionAt(two, 0)).toBe(0);
    expect(sectionAt(two, 59_999)).toBe(0);
    expect(sectionAt(two, 60_000)).toBe(1);
    expect(sectionAt([], 5)).toBeNull();
  });
});
