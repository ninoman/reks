import { describe, expect, it } from "vitest";

import {
  DECK_ZOOM_S,
  DEFAULT_DECK_ZOOM,
  followPlayhead,
  frameMs,
  frameX,
  MAX_AHEAD_MS,
  predict,
  scrollFrame,
  stepZoom,
  zoomSeconds,
} from "./deckScroll";

describe("zoom", () => {
  it("steps between the levels and stops at the ends", () => {
    expect(zoomSeconds(DEFAULT_DECK_ZOOM)).toBe(8);
    expect(stepZoom(DEFAULT_DECK_ZOOM, 1)).toBe(DEFAULT_DECK_ZOOM + 1);
    expect(stepZoom(0, -1)).toBe(0);
    expect(stepZoom(DECK_ZOOM_S.length - 1, 1)).toBe(DECK_ZOOM_S.length - 1);
  });
});

describe("scrollFrame", () => {
  it("keeps the playhead centred at every zoom level", () => {
    for (const s of DECK_ZOOM_S) {
      const f = scrollFrame(12_345.6, s, 901);
      expect(f.mid).toBe(450);
      expect(frameX(f, 12_345.6)).toBe(450);
    }
  });

  it("puts beats on the waveform's pixel grid and maps x back to time", () => {
    const f = scrollFrame(10_000, 8, 1000);
    // 125 px/s: a beat 1 s after the playhead is 125 px right of it.
    expect(frameX(f, 11_000)).toBe(f.mid + 125);
    expect(frameMs(f, f.mid + 125)).toBeCloseTo(11_000);
    // Moving by less than a pixel moves nothing; the grid stays put.
    const g = scrollFrame(10_003, 8, 1000);
    expect(g.left).toBe(f.left);
    expect(frameX(g, 11_000)).toBe(frameX(f, 11_000));
  });

  it("scales with zoom: half the seconds, twice the pixels", () => {
    const wide = scrollFrame(5_000, 8, 800);
    const close = scrollFrame(5_000, 4, 800);
    expect(frameX(close, 6_000) - close.mid).toBe(2 * (frameX(wide, 6_000) - wide.mid));
  });
});

describe("playhead", () => {
  it("predicts on from the last state, but not past the stall limit", () => {
    const s = { positionMs: 1_000, speed: 1.5, at: 100 };
    expect(predict(s, 120)).toBe(1_030);
    expect(predict(s, 10_000)).toBe(1_000 + MAX_AHEAD_MS * 1.5);
    expect(predict(s, 50)).toBe(1_000);
  });

  it("runs on smoothly, eases out jitter and jumps on a seek", () => {
    expect(followPlayhead(null, 16, 1, 500)).toBe(500);
    // On time: just runs on.
    expect(followPlayhead(1_000, 16, 1, 1_016)).toBe(1_016);
    // 4 ms of jitter is eased, not jumped.
    const eased = followPlayhead(1_000, 16, 1, 1_020);
    expect(eased).toBeGreaterThan(1_016);
    expect(eased).toBeLessThan(1_020);
    // A seek is followed at once.
    expect(followPlayhead(1_000, 16, 1, 30_000)).toBe(30_000);
    // Stopped (skimming): exactly where the engine is.
    expect(followPlayhead(1_000, 16, 0, 1_010)).toBe(1_010);
  });
});
