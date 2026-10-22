import { describe, expect, it } from "vitest";

import { activeLoop, darkText, deckMarks, markAt, markTitle } from "./deckMarks";

const colours = { hot: "#00ff00", memory: "#ff0000", autoLoop: "#839cff", cue: "#ffaa00" };

describe("deckMarks", () => {
  it("lists memory cues, the cue point, then lettered hot cues in their own colours", () => {
    const marks = deckMarks(
      [
        { id: 1, slot: 0, positionMs: 1_000, colorRgb: 0x30d158, name: null },
        { id: 2, slot: 3, positionMs: 2_000, colorRgb: null, name: "Drop" },
      ],
      [{ id: 3, positionMs: 500, endMs: null, activeLoop: false, colorRgb: null, name: "Break" }],
      250,
      colours,
    );
    expect(marks).toEqual([
      {
        kind: "memory",
        ms: 500,
        colour: "#ff0000",
        label: null,
        name: "Break",
        endMs: null,
        auto: false,
      },
      {
        kind: "cue",
        ms: 250,
        colour: "#ffaa00",
        label: null,
        name: null,
        endMs: null,
        auto: false,
      },
      {
        kind: "hot",
        ms: 1_000,
        colour: "#30d158",
        label: "A",
        name: null,
        endMs: null,
        auto: false,
      },
      {
        kind: "hot",
        ms: 2_000,
        colour: "#00ff00",
        label: "D",
        name: "Drop",
        endMs: null,
        auto: false,
      },
    ]);
  });

  it("names the mark under the pointer, hot cues first", () => {
    const marks = deckMarks(
      [{ id: 1, slot: 1, positionMs: 1_000, colorRgb: null, name: "Drop" }],
      [{ id: 2, positionMs: 998, endMs: null, activeLoop: false, colorRgb: null, name: null }],
      5_000,
      colours,
    );
    const at = (ms: number) => {
      const m = markAt(marks, ms, 10);
      return m === null ? null : markTitle(m);
    };
    expect(at(996)).toBe("Hot cue B: Drop");
    expect(at(5_008)).toBe("Cue point");
    expect(at(3_000)).toBeNull();
    expect(at(988)).toBe("Memory cue");
  });

  it("marks memory loops with their end, auto loops in their own colour", () => {
    const loop = (id: number, positionMs: number, activeLoop: boolean) => ({
      id,
      positionMs,
      endMs: positionMs + 500,
      activeLoop,
      colorRgb: 0x112233,
      name: null,
    });
    const marks = deckMarks([], [loop(1, 1_000, false), loop(2, 3_000, true)], 0, colours);
    expect(marks.slice(0, 2).map((m) => [m.endMs, m.auto, m.colour])).toEqual([
      [1_500, false, "#112233"],
      [3_500, true, "#839cff"],
    ]);
    expect(marks.slice(0, 2).map(markTitle)).toEqual(["Memory loop", "Auto loop"]);
  });
});

describe("darkText", () => {
  it("is dark on light flags and light on dark ones", () => {
    expect(darkText("#30d158")).toBe(true);
    expect(darkText("#ffff00")).toBe(true);
    expect(darkText("#0000ff")).toBe(false);
  });
});

describe("activeLoop", () => {
  it("is the loop when the engine reports one, else nothing", () => {
    expect(activeLoop(null)).toBeNull();
    expect(activeLoop({ startMs: 10, endMs: 10, beats: 1 })).toBeNull();
    expect(activeLoop({ startMs: 10, endMs: 20, beats: 1 })).toEqual({
      startMs: 10,
      endMs: 20,
      beats: 1,
    });
  });
});
