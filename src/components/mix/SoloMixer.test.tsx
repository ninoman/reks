// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { Assign, MixerLook } from "../../lib/mixView";
import { useMix } from "../../store/mix";
import { SoloMixer, type SoloMixerProps } from "./SoloMixer";

afterEach(cleanup);

const SIMPLE: MixerLook = { eq: "fourBand", view: "simple", filters: "show" };

function setup(look: MixerLook = SIMPLE, filterAssign: Assign = "x") {
  const s = useMix.getState();
  const values = s.channels[1];
  if (values === undefined) throw new Error("no channel 2");
  const props: SoloMixerProps = {
    ch: 1,
    values: { ...values, filterAssign },
    filters: s.filters,
    fx: s.fx,
    look,
    onChannel: vi.fn(),
    onFilter: vi.fn(),
    onFx: vi.fn(),
  };
  render(<SoloMixer {...props} />);
  return props;
}

const sliders = () => screen.getAllByRole("slider").map((el) => el.getAttribute("aria-label"));

describe("SoloMixer", () => {
  it("4-band: channel 2's level and EQ, filter X, and no fader, master or cue", () => {
    const p = setup();
    expect(sliders()).toEqual([
      "Filter X frequency",
      "Filter X resonance",
      "Channel 2 level",
      "Channel 2 EQ Hi",
      "Channel 2 EQ HM",
      "Channel 2 EQ LM",
      "Channel 2 EQ Lo",
    ]);
    expect(screen.queryByRole("button", { name: /cue/i })).toBeNull();
    expect(screen.queryByRole("region", { name: "Master" })).toBeNull();
    fireEvent.keyDown(screen.getByRole("slider", { name: "Channel 2 EQ Lo" }), { key: "Home" });
    expect(p.onChannel).toHaveBeenCalledWith(1, "eqLo", 0);
    fireEvent.click(screen.getByRole("button", { name: "Filter X on" }));
    expect(p.onFilter).toHaveBeenCalledWith("x", "on", true);
  });

  it("Advanced: the assign picks the unit, and off shows none", () => {
    const advanced: MixerLook = { ...SIMPLE, view: "advanced" };
    const p = setup(advanced, "y");
    expect(screen.getByRole("region", { name: "Filter Y" })).toBeTruthy();
    const assign = screen.getByRole("radiogroup", { name: "Channel 2 filter assign" });
    fireEvent.click(within(assign).getByRole("radio", { name: "Off" }));
    expect(p.onChannel).toHaveBeenCalledWith(1, "filterAssign", "off");
    cleanup();
    setup(advanced, "off");
    expect(screen.queryByRole("region", { name: /^Filter/ })).toBeNull();
  });

  it("4-band Simple with filters hidden: the knobs only", () => {
    setup({ ...SIMPLE, filters: "hide" });
    expect(screen.queryByRole("region", { name: /^Filter/ })).toBeNull();
    expect(sliders()).toHaveLength(5);
  });

  it("3-band: the EQ ends at LOW; the FX select, FX knob and PARAMETER stand in the column", () => {
    const p = setup({ ...SIMPLE, eq: "threeBand" });
    expect(sliders()).toEqual([
      "Channel 2 FX",
      "FX resonance",
      "Channel 2 level",
      "Channel 2 EQ Hi",
      "Channel 2 EQ Mid",
      "Channel 2 EQ Low",
    ]);
    expect(screen.getByRole("button", { name: /^FX effect/ })).toBeTruthy();
    fireEvent.keyDown(screen.getByRole("slider", { name: "Channel 2 FX" }), { key: "Home" });
    expect(p.onChannel).toHaveBeenCalledWith(1, "fx", 0);
  });
});
