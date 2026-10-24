import { describe, expect, it } from "vitest";

import {
  bandPeaks,
  beatNear,
  columnsDurationMs,
  markerNear,
  monoPeaks,
  sectionAt,
  sectionViews,
  visibleBeats,
} from "./gridView";
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

describe("monoPeaks", () => {
  it("takes the peak mono byte under each pixel, scaled to 0..1", () => {
    const c = new Uint8Array(4 * 4);
    for (const [i, m] of [4, 31, 8, 0].entries()) c.set([0, 0, 0, m], 4 * i);
    // 75 px/s: two columns per pixel.
    const p = monoPeaks(c, 0, 75, 3);
    expect(p[0]).toBe(1);
    expect(p[1]).toBeCloseTo(8 / 31);
    expect(p[2]).toBe(0);
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

  it("lists sections from 0 to the end of the track", () => {
    expect(sectionViews(two, 120_000)).toEqual([
      { index: 0, startMs: 0, endMs: 60_000, bpmX100: 14000 },
      { index: 1, startMs: 60_000, endMs: 120_000, bpmX100: 17400 },
    ]);
    expect(sectionViews([], 1_000)).toEqual([]);
  });

  it("finds the section at a time", () => {
    expect(sectionAt(two, 0)).toBe(0);
    expect(sectionAt(two, 59_999)).toBe(0);
    expect(sectionAt(two, 60_000)).toBe(1);
    expect(sectionAt([], 5)).toBeNull();
  });

  it("finds a marker or beat near a time", () => {
    expect(markerNear(two, 59_990, 20)).toBe(1);
    expect(markerNear(two, 59_970, 20)).toBeNull();
    const beats = [
      { positionUs: 300_000, beatInBar: 1 },
      { positionUs: 728_571, beatInBar: 2 },
    ];
    expect(beatNear(beats, 720, 20)).toBe(729);
    expect(beatNear(beats, 500, 20)).toBeNull();
  });
});
