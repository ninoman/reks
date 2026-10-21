// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { Fader } from "./Fader";
import { Knob, knobKeyValue } from "./Knob";

afterEach(cleanup);

describe("Knob", () => {
  it("is a focusable slider with its value", () => {
    render(<Knob value={0.25} onValueChange={() => {}} aria-label="Hi" />);
    const knob = screen.getByRole("slider", { name: "Hi" });
    expect(knob.getAttribute("aria-valuenow")).toBe("25");
    expect(knob.tabIndex).toBe(0);
  });

  it("steps with the keyboard and resets to its default", () => {
    const change = vi.fn();
    render(<Knob value={0.5} defaultValue={0.5} onValueChange={change} aria-label="Lo" />);
    const knob = screen.getByRole("slider", { name: "Lo" });
    fireEvent.keyDown(knob, { key: "ArrowUp" });
    expect(change).toHaveBeenLastCalledWith(0.51);
    fireEvent.keyDown(knob, { key: "End" });
    expect(change).toHaveBeenLastCalledWith(1);
    change.mockClear();
    // Already at the default: nothing to send.
    fireEvent.doubleClick(knob);
    expect(change).not.toHaveBeenCalled();
  });

  it("does nothing while disabled", () => {
    const change = vi.fn();
    render(<Knob disabled value={0.3} onValueChange={change} aria-label="Mid" />);
    const knob = screen.getByRole("slider", { name: "Mid" });
    expect(knob.tabIndex).toBe(-1);
    fireEvent.keyDown(knob, { key: "ArrowUp" });
    expect(change).not.toHaveBeenCalled();
  });

  it("maps keys to values", () => {
    expect(knobKeyValue("PageDown", 0.05, false, 0.5)).toBe(0);
    expect(knobKeyValue("ArrowLeft", 0.5, true, 0.5)).toBeCloseTo(0.498);
    expect(knobKeyValue("Delete", 0.9, false, 0.5)).toBe(0.5);
    expect(knobKeyValue("a", 0.5, false, 0.5)).toBeNull();
  });
});

describe("Fader", () => {
  it("is a named slider, vertical by default", () => {
    render(
      <Fader
        value={0.75}
        onValueChange={() => {}}
        aria-label="Channel 1 fader"
        valueText="-1 dB"
      />,
    );
    const thumb = screen.getByRole("slider", { name: "Channel 1 fader" });
    expect(thumb.getAttribute("aria-orientation")).toBe("vertical");
    expect(thumb.getAttribute("aria-valuetext")).toBe("-1 dB");
  });

  it("returns to its default on double-click", () => {
    const change = vi.fn();
    render(
      <Fader
        orientation="horizontal"
        value={0.1}
        defaultValue={0.5}
        onValueChange={change}
        aria-label="Crossfader"
      />,
    );
    fireEvent.doubleClick(screen.getByRole("slider", { name: "Crossfader" }));
    expect(change).toHaveBeenCalledWith(0.5);
  });
});
