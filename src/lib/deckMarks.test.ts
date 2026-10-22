import { describe, expect, it } from "vitest";

import { darkText, deckMarks, loopRegion } from "./deckMarks";

const colours = { hot: "#00ff00", memory: "#ff0000", cue: "#ffaa00" };

describe("deckMarks", () => {
  it("lists memory cues, the cue point, then lettered hot cues in their own colours", () => {
    const marks = deckMarks(
      [
        { slot: 0, positionMs: 1_000, colorRgb: 0x30d158 },
        { slot: 3, positionMs: 2_000, colorRgb: null },
      ],
      [{ positionMs: 500, colorRgb: null }],
      250,
      colours,
    );
    expect(marks).toEqual([
      { kind: "memory", ms: 500, colour: "#ff0000", label: null },
      { kind: "cue", ms: 250, colour: "#ffaa00", label: null },
      { kind: "hot", ms: 1_000, colour: "#30d158", label: "A" },
      { kind: "hot", ms: 2_000, colour: "#00ff00", label: "D" },
    ]);
  });
});

describe("darkText", () => {
  it("is dark on light flags and light on dark ones", () => {
    expect(darkText("#30d158")).toBe(true);
    expect(darkText("#ffff00")).toBe(true);
    expect(darkText("#0000ff")).toBe(false);
  });
});

describe("loopRegion", () => {
  it("is the loop when the engine reports one, else nothing", () => {
    expect(loopRegion(undefined)).toBeNull();
    expect(loopRegion(null)).toBeNull();
    expect(loopRegion({ startMs: 10, endMs: 10 })).toBeNull();
    expect(loopRegion({ startMs: 10, endMs: 20 })).toEqual({ startMs: 10, endMs: 20 });
  });
});
