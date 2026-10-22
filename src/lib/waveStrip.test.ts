import { describe, expect, it } from "vitest";

import { stripPeaks } from "./waveStrip";

/** Columns from `[low, mid, high, mono]` rows. */
const cols = (rows: number[][]) => new Uint8Array(rows.flat());

describe("stripPeaks", () => {
  it("takes the peak of the columns under each pixel when zoomed out", () => {
    const c = cols([
      [255, 0, 0, 31],
      [0, 51, 0, 0],
      [0, 0, 102, 0],
      [0, 0, 0, 0],
    ]);
    const { bands, mono } = stripPeaks(c, 0, 2, 2);
    expect([...bands].map((v) => Math.round(v * 100) / 100)).toEqual([1, 0.2, 0, 0, 0, 0.4]);
    expect([...mono]).toEqual([1, 0]);
  });

  it("is fixed to the track: a pixel reads the same columns wherever the view starts", () => {
    const c = cols(Array.from({ length: 40 }, (_, i) => [i * 6, 0, 0, i % 31]));
    const whole = stripPeaks(c, 0, 1.5, 20);
    const later = stripPeaks(c, 7, 1.5, 13);
    expect([...later.mono]).toEqual([...whole.mono.slice(7)]);
  });

  it("interpolates between column centres when zoomed past one column a pixel", () => {
    const c = cols([
      [0, 0, 0, 0],
      [255, 0, 0, 31],
    ]);
    // Four pixels a column: centres at x = 1.5 and 5.5, a straight ramp between.
    const { mono } = stripPeaks(c, 0, 0.25, 6);
    expect(mono[1]).toBeCloseTo(0);
    expect(mono[2]).toBeCloseTo(0.125);
    expect(mono[3]).toBeCloseTo(0.375);
    expect(mono[4]).toBeCloseTo(0.625);
    expect(mono[5]).toBeCloseTo(0.875);
  });

  it("is silent outside the track", () => {
    const { mono } = stripPeaks(cols([[0, 0, 0, 31]]), -3, 1, 2);
    expect([...mono]).toEqual([0, 0]);
  });
});
