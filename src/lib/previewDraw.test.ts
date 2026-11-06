import { describe, expect, it } from "vitest";

import { previewBars, previewPixels } from "./previewDraw";

const palette = { low: "#ff0000", mid: "#00ff00", high: "#0000ff" };
/** A payload of `n` identical `[height, low, mid, high]` columns. */
const steady = (col: number[], n: number) =>
  new Uint8Array(Array.from({ length: n }, () => col).flat());

describe("previewBars", () => {
  it("bass with half the mids reads orange, the strongest channel leads", () => {
    const [bar] = previewBars(steady([127, 80, 40, 0], 1200), 10, palette);
    expect(bar?.half).toBe(1);
    const [r, g, b] = (bar?.rgb ?? []).map((v) => Math.round(v * 100) / 100);
    expect([r, g, b]).toEqual([1, 0.55, 0.1]);
  });

  it("maps 1200 columns onto any width, and nothing onto nothing", () => {
    expect(previewBars(steady([60, 10, 10, 10], 1200), 300, palette)).toHaveLength(300);
    expect(previewBars(new Uint8Array(0), 300, palette)).toEqual([]);
  });
});

describe("previewPixels", () => {
  it("is brightest on the centre line, dims outwards and leaves the rest clear", () => {
    const px = previewPixels(steady([64, 100, 0, 0], 1200), 4, 40, palette);
    const red = (y: number) => px[4 * (y * 4)] ?? 0;
    const alpha = (y: number) => px[4 * (y * 4) + 3] ?? 0;
    // Height 64/127 of the half: about ±10 px around 20.
    expect(alpha(0)).toBe(0);
    expect(alpha(20)).toBe(255);
    expect(red(20)).toBeGreaterThan(red(13));
    expect(red(13)).toBeGreaterThan(0);
    expect(alpha(39)).toBe(0);
  });
});
