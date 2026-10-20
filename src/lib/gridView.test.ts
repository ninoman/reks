import { describe, expect, it } from "vitest";

import { bandPeaks, columnsDurationMs, visibleBeats } from "./gridView";

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

describe("visibleBeats", () => {
  const beats = [0, 500_000, 1_000_000, 1_500_000].map((positionUs, i) => ({
    positionUs,
    beatInBar: (i % 4) + 1,
  }));

  it("places beats in pixels and marks downbeats", () => {
    expect(visibleBeats(beats, 400, 100, 80)).toEqual([
      { x: 10, downbeat: false },
      { x: 60, downbeat: false },
    ]);
    expect(visibleBeats(beats, 0, 100, 10)).toEqual([{ x: 0, downbeat: true }]);
  });
});

it("columnsDurationMs counts 150 columns per second", () => {
  expect(columnsDurationMs(cols(300))).toBe(2000);
});
