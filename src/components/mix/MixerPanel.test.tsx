// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { useMix } from "../../store/mix";
import { MixerPanel, type MixerPanelProps } from "./MixerPanel";

afterEach(cleanup);

function setup() {
  const s = useMix.getState();
  const props: MixerPanelProps = {
    channels: s.channels,
    active: [true, false, false, false],
    filters: s.filters,
    crossfader: s.crossfader,
    master: s.master,
    onChannel: vi.fn(),
    onFilter: vi.fn(),
    onCrossfader: vi.fn(),
    onMaster: vi.fn(),
  };
  render(<MixerPanel {...props} />);
  return props;
}

describe("MixerPanel", () => {
  it("lays out four channels, the master, both filters and the crossfader", () => {
    setup();
    for (const n of [1, 2, 3, 4]) {
      const strip = screen.getByRole("region", { name: `Channel ${n}` });
      for (const name of ["level", "EQ Hi", "EQ Mid 1", "EQ Mid 2", "EQ Lo", "fader"]) {
        expect(within(strip).getByRole("slider", { name: `Channel ${n} ${name}` })).toBeTruthy();
      }
    }
    expect(screen.getByRole("region", { name: "Master" })).toBeTruthy();
    expect(screen.getByRole("region", { name: "Filter X" })).toBeTruthy();
    expect(screen.getByRole("region", { name: "Filter Y" })).toBeTruthy();
    expect(screen.getByRole("slider", { name: "Crossfader" })).toBeTruthy();
    // LINK is on Y only.
    expect(screen.queryByRole("button", { name: "Filter X linked to X" })).toBeNull();
    expect(screen.getByRole("button", { name: "Filter Y linked to X" })).toBeTruthy();
  });

  it("forwards switches as typed changes", () => {
    const p = setup();
    fireEvent.click(screen.getByRole("button", { name: "Channel 2 cue" }));
    expect(p.onChannel).toHaveBeenCalledWith(1, "cue", true);
    const assign = screen.getByRole("radiogroup", { name: "Channel 3 filter assign" });
    fireEvent.click(within(assign).getByRole("radio", { name: "Y" }));
    expect(p.onChannel).toHaveBeenCalledWith(2, "filterAssign", "y");
    fireEvent.click(screen.getByRole("button", { name: "Filter Y band-pass" }));
    expect(p.onFilter).toHaveBeenCalledWith("y", "bandPass", true);
    fireEvent.keyDown(screen.getByRole("slider", { name: "Channel 1 EQ Lo" }), { key: "Home" });
    expect(p.onChannel).toHaveBeenCalledWith(0, "eqLo", 0);
    fireEvent.click(screen.getByRole("button", { name: "Split" }));
    expect(p.onMaster).toHaveBeenCalledWith("splitCue", true);
  });

  it("names no trademarks", () => {
    setup();
    expect(document.body.textContent).not.toMatch(/xone|allen/i);
  });
});
