import { describe, expect, it } from "vitest";

import { formatClock, formatTempo, phaseOffset, tempoFader, tempoRatio } from "./mixView";

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
