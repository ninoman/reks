import { describe, expect, it } from "vitest";

import { sampleWave } from "./sampleWave";

describe("sampleWave", () => {
  it("gives three bands and a mono peak per pixel, all within 0..1", () => {
    const { bands, mono } = sampleWave(96);
    expect(bands.length).toBe(3 * 96);
    expect(mono.length).toBe(96);
    for (const v of [...bands, ...mono]) {
      expect(v).toBeGreaterThanOrEqual(0);
      expect(v).toBeLessThanOrEqual(1);
    }
  });

  it("puts a kick on every beat: the low band peaks at each beat's start", () => {
    const { bands } = sampleWave(80);
    // 4 beats of 20 px: the start of a beat is louder than its end.
    expect(bands[3 * 20] ?? 0).toBeGreaterThan(bands[3 * 39] ?? 0);
  });
});
