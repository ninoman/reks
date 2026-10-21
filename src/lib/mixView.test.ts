import { describe, expect, it } from "vitest";

import type { CueRouting } from "./ipc";
import {
  cueFits,
  deckSide,
  formatClock,
  formatTempo,
  nextTempoRange,
  phaseOffset,
  scrubTarget,
  shownDecks,
  tempoFader,
  tempoRangeLabel,
  tempoRatio,
} from "./mixView";

describe("tempo fader", () => {
  it("is slower at the top and faster at the bottom", () => {
    expect(tempoRatio(0.5, 0.1)).toBe(1);
    expect(tempoRatio(1, 0.06)).toBeCloseTo(0.94);
    expect(tempoRatio(0, 0.16)).toBeCloseTo(1.16);
  });

  it("WIDE runs from standing still to double speed", () => {
    expect(tempoRatio(1, 1)).toBe(0);
    expect(tempoRatio(0, 1)).toBe(2);
    expect(formatTempo(0)).toBe("-100.00%");
  });

  it("round-trips and clamps", () => {
    expect(tempoFader(tempoRatio(0.3, 0.16), 0.16)).toBeCloseTo(0.3);
    expect(tempoFader(1.3, 0.1)).toBe(0);
  });

  it("cycles the ranges like a CDJ-3000", () => {
    const seen = [0.06, 0.1, 0.16, 1].map((r) => {
      const next = nextTempoRange(r as 0.06);
      return `${tempoRangeLabel(r as 0.06)}>${tempoRangeLabel(next)}`;
    });
    expect(seen).toEqual(["±6>±10", "±10>±16", "±16>WIDE", "WIDE>±6"]);
  });

  it("formats as a signed percentage", () => {
    expect(formatTempo(1.025)).toBe("+2.50%");
    expect(formatTempo(0.997)).toBe("-0.30%");
    expect(formatTempo(1.00001)).toBe("0.00%");
  });
});

describe("phaseOffset", () => {
  it("wraps into half a beat either side", () => {
    expect(phaseOffset(10.1, 3.0)).toBeCloseTo(0.1);
    expect(phaseOffset(9.9, 3.0)).toBeCloseTo(-0.1);
    expect(phaseOffset(3.5, 0)).toBeCloseTo(-0.5);
  });

  it("compares bar phase with a span of 4", () => {
    expect(phaseOffset(6, 0, 4)).toBeCloseTo(-2);
    expect(phaseOffset(5, 0, 4)).toBeCloseTo(1);
  });
});

describe("formatClock", () => {
  it("shows tenths, remaining with a minus", () => {
    expect(formatClock(83_456)).toBe("1:23.4");
    expect(formatClock(5_000, true)).toBe("-0:05.0");
    expect(formatClock(-3)).toBe("0:00.0");
  });
});

describe("cueFits", () => {
  it("matches the outputs a routing needs", () => {
    const outputs: CueRouting = { type: "outputs", master: [0, 1], cue: [2, 3] };
    expect(cueFits({ type: "off" }, 1)).toBe(true);
    expect(cueFits({ type: "splitMono" }, 1)).toBe(false);
    expect(cueFits({ type: "splitMono" }, 2)).toBe(true);
    expect(cueFits(outputs, 2)).toBe(false);
    expect(cueFits(outputs, 4)).toBe(true);
  });
});

describe("decks on screen", () => {
  it("shows decks 2 and 3 of two, the centre channels", () => {
    expect(shownDecks(2)).toEqual([1, 2]);
    expect(shownDecks(4)).toEqual([0, 1, 2, 3]);
    expect([0, 1, 2, 3].map(deckSide)).toEqual(["left", "left", "right", "right"]);
  });
});

describe("scrubTarget", () => {
  it("overview: the point under the pointer, clamped", () => {
    const grip = { kind: "overview", widthPx: 200, durationMs: 30_000 } as const;
    expect(scrubTarget(grip, 50, 0)).toBe(7_500);
    expect(scrubTarget(grip, 260, 0)).toBe(30_000);
    expect(scrubTarget(grip, -5, 0)).toBe(0);
  });

  it("zoomed: dragging left moves forward", () => {
    const grip = { kind: "zoomed", pxPerS: 100, fromMs: 10_000, durationMs: 30_000 } as const;
    expect(scrubTarget(grip, 0, -50)).toBe(10_500);
    expect(scrubTarget(grip, 0, 200)).toBe(8_000);
    expect(scrubTarget(grip, 0, 5_000)).toBe(0);
  });
});
