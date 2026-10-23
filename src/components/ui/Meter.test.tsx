// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { litCount, METER_LEDS, scaleDb } from "../../lib/meterScale";
import { Meter, MeterScale } from "./Meter";

afterEach(cleanup);

const lit = (m: HTMLElement) => m.querySelectorAll("[data-lit]").length;

describe("meter scale", () => {
  it("puts 0 dBFS on the red +10 LED", () => {
    expect(scaleDb(1)).toBeCloseTo(10);
    expect(scaleDb(0.5)).toBeCloseTo(3.98, 1);
    expect(scaleDb(0)).toBe(Number.NEGATIVE_INFINITY);
  });

  it("lights LEDs from the bottom up to the level", () => {
    expect(litCount(0)).toBe(0);
    expect(litCount(10 ** (-31 / 20))).toBe(0); // −21 on the scale
    expect(litCount(10 ** (-30 / 20))).toBe(1); // −20
    expect(litCount(10 ** (-10 / 20))).toBe(6); // 0, the blue LED
    expect(litCount(0.5)).toBe(7); // +4: through +3
    expect(litCount(1)).toBe(METER_LEDS.length);
    expect(litCount(4)).toBe(METER_LEDS.length);
  });
});

describe("Meter", () => {
  it("draws nine labelled LEDs, top +10 red to bottom -20", () => {
    render(<Meter lit={0} clip={false} aria-label="Master meter" />);
    const m = screen.getByRole("meter", { name: "Master meter" });
    expect(m.textContent).toBe("+10+6+30-3-6-10-15-20");
    expect(lit(m)).toBe(0);
    expect(m.getAttribute("aria-valuetext")).toBe("Silent");
  });

  it("reports its level and clipping to assistive tech", () => {
    const { rerender } = render(<Meter lit={7} clip={false} aria-label="Master meter" />);
    const m = screen.getByRole("meter", { name: "Master meter" });
    expect(lit(m)).toBe(7);
    expect(m.getAttribute("aria-valuenow")).toBe("3");
    expect(m.getAttribute("aria-valuetext")).toBe("+3 dB");
    // A held clip lights the red LED on its own.
    rerender(<Meter lit={2} clip aria-label="Master meter" />);
    expect(lit(m)).toBe(3);
    expect(m.getAttribute("aria-valuetext")).toBe("Clipping");
  });

  it("can leave the labels out", () => {
    render(<Meter lit={9} clip={false} labels={false} aria-label="Ch" />);
    expect(screen.getByRole("meter").textContent).toBe("");
  });

  it("draws the labels alone as a scale", () => {
    const { container } = render(<MeterScale />);
    expect(container.textContent).toBe("+10+6+30-3-6-10-15-20");
    expect(container.firstElementChild?.getAttribute("aria-hidden")).toBe("true");
  });
});
