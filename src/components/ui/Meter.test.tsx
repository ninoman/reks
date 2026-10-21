// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { FLOOR_DB, levelDb, Meter } from "./Meter";

afterEach(cleanup);

describe("Meter", () => {
  it("maps linear level to dBFS with a floor", () => {
    expect(levelDb(1)).toBeCloseTo(0);
    expect(levelDb(0.5)).toBeCloseTo(-6.02, 1);
    expect(levelDb(0)).toBe(FLOOR_DB);
    expect(levelDb(1e-9)).toBe(FLOOR_DB);
  });

  it("reports its level and clipping to assistive tech", () => {
    const { rerender } = render(<Meter level={0.5} clip={false} aria-label="Master meter" />);
    const m = screen.getByRole("meter", { name: "Master meter" });
    expect(m.getAttribute("aria-valuenow")).toBe("-6");
    expect(m.getAttribute("aria-valuetext")).toBe("-6 dB");
    rerender(<Meter level={1.4} clip aria-label="Master meter" />);
    expect(m.getAttribute("aria-valuenow")).toBe("0");
    expect(m.getAttribute("aria-valuetext")).toBe("Clipping");
  });
});
