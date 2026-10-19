import { describe, expect, it } from "vitest";

import { formatBpmRange, formatPosition, formatRgb, hotCueLabel, keyLabel } from "./format";

describe("detail formatters", () => {
  it("formats positions as m:ss.mmm", () => {
    expect(formatPosition(0)).toBe("0:00.000");
    expect(formatPosition(120.5)).toBe("0:00.121");
    expect(formatPosition(61_234)).toBe("1:01.234");
    expect(formatPosition(3_599_999.6)).toBe("60:00.000");
  });

  it("labels hot cues A–H", () => {
    expect([0, 1, 7].map(hotCueLabel)).toEqual(["A", "B", "H"]);
  });

  it("shows one BPM or a range", () => {
    expect(formatBpmRange(12800, 12800)).toBe("128.00");
    expect(formatBpmRange(12000, 12850)).toBe("120.00–128.50");
  });

  it("formats RGB", () => {
    expect(formatRgb(0xff0000)).toBe("#ff0000");
    expect(formatRgb(0x25)).toBe("#000025");
  });
});

describe("keyLabel", () => {
  const keys = [{ key: "Am", camelot: "8A", openKey: "1m" }];
  it("looks the key up in the chosen notation", () => {
    expect(keyLabel("Am", "classical", keys)).toBe("Am");
    expect(keyLabel("Am", "camelot", keys)).toBe("8A");
    expect(keyLabel("Am", "openKey", keys)).toBe("1m");
  });
  it("falls back to the canonical name before facets load", () => {
    expect(keyLabel("Am", "camelot", undefined)).toBe("Am");
  });
});
