import { describe, expect, it } from "vitest";

import type { CueRouting } from "./ipc";
import {
  cueFits,
  formatClock,
  formatTempo,
  phaseOffset,
  pickSwitches,
  tempoFader,
  tempoRatio,
} from "./mixView";

describe("tempo fader", () => {
  it("is slower at the top and faster at the bottom", () => {
    expect(tempoRatio(0.5, 0.08)).toBe(1);
    expect(tempoRatio(1, 0.08)).toBeCloseTo(0.92);
    expect(tempoRatio(0, 0.16)).toBeCloseTo(1.16);
  });

  it("round-trips and clamps", () => {
    expect(tempoFader(tempoRatio(0.3, 0.16), 0.16)).toBeCloseTo(0.3);
    expect(tempoFader(1.3, 0.08)).toBe(0);
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
