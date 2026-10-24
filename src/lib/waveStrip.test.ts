// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";

import { CHUNK_POINTS, DetailChunks } from "./deckDetail";
import { DETAIL_POINT_BYTES } from "./ipc";
import type { WavePalette } from "./waveDraw";
import { type StripSpec, stripPeaks, TILE_PX, WaveStrip } from "./waveStrip";

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

/** A 2D context that records its `fillRect` calls (jsdom has no canvas). */
function recorder() {
  const rects: { x: number; y: number; w: number; h: number; fill: string }[] = [];
  const ctx = {
    fillStyle: "",
    fillRect(x: number, y: number, w: number, h: number) {
      rects.push({ x, y, w, h, fill: ctx.fillStyle });
    },
    clearRect() {},
    drawImage: vi.fn(),
  };
  return { ctx, rects };
}

describe("WaveStrip close-zoom tiles", () => {
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
  /** Tile contexts in creation order. */
  let tiles: ReturnType<typeof recorder>[] = [];

  afterEach(() => {
    vi.restoreAllMocks();
    tiles = [];
  });

  function stubCanvas() {
    vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockImplementation(function (
      this: HTMLCanvasElement,
    ) {
      const r = recorder();
      tiles.push(r);
      return r.ctx as unknown as CanvasRenderingContext2D;
    } as HTMLCanvasElement["getContext"]);
  }

  /** One second of a wave riding above the centre: every point spans +0.25..+0.75. */
  function chunk(points: number): Uint8Array {
    const out = new Uint8Array(points * DETAIL_POINT_BYTES);
    const signed = new Int8Array(out.buffer);
    for (let i = 0; i < points; i++) {
      signed[i * DETAIL_POINT_BYTES] = 32;
      signed[i * DETAIL_POINT_BYTES + 1] = 95;
      out[i * DETAIL_POINT_BYTES + 2] = 255;
    }
    return out;
  }

  // 3000 points a second over 1500 px: two points a pixel; 1 s of columns.
  const spec = (detail: DetailChunks): StripSpec => ({
    columns: new Uint8Array(150 * 4).fill(20),
    style: "rgb",
    palette,
    pxPerS: 1500,
    height: 100,
    detail,
  });
  const target = () => recorder().ctx as unknown as CanvasRenderingContext2D;
  /** The waveform bars of a tile: everything but its 1 px centre line. */
  const bars = (r: ReturnType<typeof recorder>) => r.rects.filter((b) => b.w === 1);

  it("draws a pixel as a line from its min to its max, above the centre", () => {
    stubCanvas();
    const detail = new DetailChunks(3000);
    detail.add(0, chunk(3000), 0);
    new WaveStrip(spec(detail)).draw(target(), 0, TILE_PX);
    const drawn = bars(tiles[0] ?? recorder());
    expect(drawn).toHaveLength(TILE_PX);
    // +0.75..+0.25 of a 50 px half: rows 12 to 38, all above the middle (50).
    expect(drawn[10]).toEqual({ x: 10, y: 12, w: 1, h: 26, fill: "rgb(217 0 0)" });
    expect(drawn.every((b) => b.y + b.h < 50)).toBe(true);
  });

  it("falls back to the columns, then redraws the tile once its detail arrives", () => {
    stubCanvas();
    const detail = new DetailChunks(3000);
    const strip = new WaveStrip(spec(detail));
    strip.draw(target(), 0, TILE_PX);
    // Columns: symmetric bars about the middle.
    const before = bars(tiles[0] ?? recorder());
    expect(before.length).toBeGreaterThan(0);
    expect(before.every((b) => Math.abs(b.y + b.h / 2 - 50) < 0.01)).toBe(true);

    const version = strip.detailVersion;
    detail.add(0, chunk(3000), 0);
    expect(strip.detailVersion).toBe(version + 1);
    strip.draw(target(), 0, TILE_PX);
    // The same tile canvas, drawn again from the detail.
    const after = bars(tiles[1] ?? recorder()).slice(0);
    expect(tiles).toHaveLength(2);
    expect(after.every((b) => b.y + b.h < 50)).toBe(true);

    // Drawn from the detail now: later frames only copy it.
    strip.draw(target(), 0, TILE_PX);
    expect(tiles).toHaveLength(2);
  });

  it("keeps the columns where the detail has not reached", () => {
    stubCanvas();
    const detail = new DetailChunks(2 * CHUNK_POINTS);
    detail.add(1, chunk(10), 0);
    new WaveStrip(spec(detail)).draw(target(), 0, TILE_PX);
    const drawn = bars(tiles[0] ?? recorder());
    expect(drawn.every((b) => Math.abs(b.y + b.h / 2 - 50) < 0.01)).toBe(true);
  });
});
