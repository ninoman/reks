import { describe, expect, it } from "vitest";

import type { CueRouting, MixState } from "./ipc";
import {
  cueFits,
  deckSide,
  formatBeats,
  formatClock,
  formatTempo,
  JUMP_PAGES,
  LOOP_PAGES,
  loopPadControls,
  nextPadMode,
  nextTempoRange,
  outputSummary,
  phaseOffset,
  pickSwitches,
  scrubTarget,
  shownDecks,
  snapToDetent,
  tempoFader,
  tempoForBpm,
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

describe("outputSummary", () => {
  const live = { device: "Speakers", sampleRate: 44_100, bufferFrames: 256, xruns: 0 } as MixState;
  it("lists device, rate and buffer, and dropouts only when there are any", () => {
    expect(outputSummary(live)).toBe("Speakers · 44.1 kHz · 256 frames");
    expect(outputSummary({ ...live, bufferFrames: null, xruns: 3 })).toBe(
      "Speakers · 44.1 kHz · 3 dropouts",
    );
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

describe("pickSwitches", () => {
  const f = false;
  const t = true;
  it("a plain click picks one switch alone", () => {
    expect(pickSwitches([t, t, f], 2, false, false)).toEqual([f, f, t]);
    expect(pickSwitches([t, t, f], 0, false, false)).toEqual([t, f, f]);
  });
  it("a plain click on the only switch on turns it off, unless one must stay on", () => {
    expect(pickSwitches([f, t, f], 1, false, false)).toEqual([f, f, f]);
    expect(pickSwitches([f, t, f], 1, false, true)).toEqual([f, t, f]);
  });
  it("Shift+click adds and removes", () => {
    expect(pickSwitches([t, f, f], 2, true, false)).toEqual([t, f, t]);
    expect(pickSwitches([t, f, t], 0, true, false)).toEqual([f, f, t]);
    expect(pickSwitches([t, f, f], 0, true, false)).toEqual([f, f, f]);
    expect(pickSwitches([t, f, f], 0, true, true)).toEqual([t, f, f]);
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

describe("pads", () => {
  it("cycle HOT CUE, BEAT JUMP, BEAT LOOP", () => {
    expect(nextPadMode("hotCue")).toBe("memory");
    expect(nextPadMode("memory")).toBe("beatJump");
    expect(nextPadMode("beatJump")).toBe("beatLoop");
    expect(nextPadMode("beatLoop")).toBe("hotCue");
  });

  it("page through eight sizes each, FINE first", () => {
    expect(JUMP_PAGES.map((p) => p.map(formatBeats))).toEqual([
      ["FINE", "1/8", "1/4", "1/2", "1", "2", "4", "8"],
      ["1", "2", "4", "8", "16", "32", "64", "128"],
    ]);
    expect(LOOP_PAGES.map((p) => p.map(formatBeats))).toEqual([
      ["1/8", "1/4", "1/2", "1", "2", "4", "8", "16"],
      ["1/4", "1/2", "1", "2", "4", "8", "16", "32"],
    ]);
    expect(JUMP_PAGES[0]?.[0]).toBe(1 / 32);
  });

  it("a loop pad sets, leaves or resizes the loop", () => {
    expect(loopPadControls(null, 4)).toEqual([{ type: "beatLoop", beats: 4 }]);
    expect(loopPadControls(4, 4)).toEqual([{ type: "loopExit" }]);
    expect(loopPadControls(4, 16)).toEqual([
      { type: "loopResize", double: true },
      { type: "loopResize", double: true },
    ]);
    expect(loopPadControls(1, 1 / 2)).toEqual([{ type: "loopResize", double: false }]);
  });
});

describe("snapToDetent", () => {
  it("lands on the detent within the zone only", () => {
    expect(snapToDetent(0.52, 0.5, 0.03)).toBe(0.5);
    expect(snapToDetent(0.48, 0.5, 0.03)).toBe(0.5);
    expect(snapToDetent(0.54, 0.5, 0.03)).toBe(0.54);
  });
});

describe("tempoForBpm", () => {
  it("divides the typed BPM by the track's", () => {
    const t = tempoForBpm(131.5, 128, 0.1);
    expect(t?.range).toBe(0.1);
    expect(t?.ratio).toBeCloseTo(1.02734, 4);
  });

  it("steps the range up to the smallest that fits", () => {
    expect(tempoForBpm(145, 128, 0.1)?.range).toBe(0.16);
    expect(tempoForBpm(118, 128, 0.06)?.range).toBe(0.1);
    expect(tempoForBpm(200, 128, 0.1)?.range).toBe(1);
    // A narrower range is never picked.
    expect(tempoForBpm(129, 128, 0.16)?.range).toBe(0.16);
  });

  it("rejects what no range reaches or is not a BPM", () => {
    expect(tempoForBpm(300, 128, 0.1)).toBeNull();
    expect(tempoForBpm(Number.NaN, 128, 0.1)).toBeNull();
    expect(tempoForBpm(0, 128, 0.1)).toBeNull();
    expect(tempoForBpm(128, 0, 0.1)).toBeNull();
  });
});
