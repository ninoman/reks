import { describe, expect, it } from "vitest";
import type { BeatMark } from "../../lib/ipc";
import { firstBeatFrom } from "./deckCanvas";

describe("firstBeatFrom", () => {
  const beats = [0, 500_000, 1_000_000].map((positionUs) => ({ positionUs }) as BeatMark);
  it("finds the first beat at or after a time", () => {
    expect(firstBeatFrom(beats, -1)).toBe(0);
    expect(firstBeatFrom(beats, 500_000)).toBe(1);
    expect(firstBeatFrom(beats, 500_001)).toBe(2);
    expect(firstBeatFrom(beats, 2_000_000)).toBe(3);
  });
});
