import { describe, expect, it } from "vitest";

import { hexRgb, mixColour, type WavePalette, waveBars } from "./waveDraw";

const palette: WavePalette = {
  low: "#0000ff",
  mid: "#ff8000",
  high: "#ffffff",
  rgbLow: "#ff0000",
  rgbMid: "#00ff00",
  rgbHigh: "#0000ff",
  blue: "#0000ff",
  blueHigh: "#ffffff",
};

/** Two pixels: a bass hit (low only), then a hi-hat (high only). */
const bands = new Float32Array([1, 0, 0, 0, 0, 1]);
const mono = new Float32Array([1, 0.5]);

describe("hexRgb", () => {
  it("reads #rrggbb and treats anything else as black", () => {
    expect(hexRgb(" #2f7bFF ")).toEqual([47, 123, 255]);
    expect(hexRgb("var(--x)")).toEqual([0, 0, 0]);
  });
});

describe("mixColour", () => {
  it("weights the colours by level and brightens the top channel to full", () => {
    const rgb = [hexRgb("#ff0000"), hexRgb("#00ff00"), hexRgb("#0000ff")];
    expect(mixColour(rgb, [0.5, 0, 0])).toBe("rgb(255 0 0)");
    expect(mixColour(rgb, [0.4, 0.2, 0])).toBe("rgb(255 128 0)");
    expect(mixColour(rgb, [0, 0, 0])).toBe("rgb(0 0 0)");
  });
});

describe("waveBars", () => {
  it("3-band: one layer per band, low first, highs scaled down", () => {
    const bars = waveBars("threeBand", bands, mono, 100, palette);
    expect(bars).toEqual([
      { x: 0, half: 50, colour: "#0000ff" },
      { x: 1, half: 30, colour: "#ffffff" },
    ]);
  });

  it("RGB: one mono-height bar per pixel, hue from the band mix", () => {
    expect(waveBars("rgb", bands, mono, 100, palette)).toEqual([
      { x: 0, half: 50, colour: "rgb(255 0 0)" },
      { x: 1, half: 25, colour: "rgb(0 0 255)" },
    ]);
  });

  it("Blue: mono height, lighter as highs take over", () => {
    expect(waveBars("blue", bands, mono, 100, palette)).toEqual([
      { x: 0, half: 50, colour: "rgb(0 0 255)" },
      { x: 1, half: 25, colour: "rgb(255 255 255)" },
    ]);
  });

  it("skips silent pixels", () => {
    const quiet = new Float32Array([0.001, 0]);
    expect(waveBars("rgb", new Float32Array(6), quiet, 100, palette)).toEqual([]);
    expect(waveBars("threeBand", new Float32Array(6), quiet, 100, palette)).toEqual([]);
  });
});
