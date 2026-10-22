// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { useMix } from "../../store/mix";
import { MixerPanel, type MixerPanelProps } from "./MixerPanel";

afterEach(cleanup);

function setup(cues: boolean[] = [false, false, false, false]) {
  const s = useMix.getState();
  const props: MixerPanelProps = {
    channels: s.channels.map((c, i) => ({ ...c, cue: cues[i] ?? false })),
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
    expect(screen.queryByText("VCF")).toBeNull();
    expect(screen.getByRole("button", { name: "Filter Y linked to X" })).toBeTruthy();
  });

  it("forwards switches as typed changes", () => {
    const p = setup();
    fireEvent.click(screen.getByRole("button", { name: "Channel 2 cue" }));
    expect(p.onChannel).toHaveBeenCalledWith(1, "cue", true);
    const assign = screen.getByRole("radiogroup", { name: "Channel 3 filter assign" });
    fireEvent.click(within(assign).getByRole("radio", { name: "X" }));
    expect(p.onChannel).toHaveBeenCalledWith(2, "filterAssign", "x");
    fireEvent.click(screen.getByRole("button", { name: "Filter Y band-pass" }));
    expect(p.onFilter).toHaveBeenCalledWith("y", "bandPass", true);
    fireEvent.keyDown(screen.getByRole("slider", { name: "Channel 1 EQ Lo" }), { key: "Home" });
    expect(p.onChannel).toHaveBeenCalledWith(0, "eqLo", 0);
    fireEvent.click(screen.getByRole("button", { name: "Split" }));
    expect(p.onMaster).toHaveBeenCalledWith("splitCue", true);
  });

  it("a click picks one filter mode, Shift+click combines them", () => {
    const p = setup();
    // Y starts on HP: picking BP alone turns HP off.
    fireEvent.click(screen.getByRole("button", { name: "Filter Y band-pass" }));
    expect(p.onFilter).toHaveBeenCalledWith("y", "bandPass", true);
    expect(p.onFilter).toHaveBeenCalledWith("y", "highPass", false);
    vi.mocked(p.onFilter).mockClear();
    fireEvent.click(screen.getByRole("button", { name: "Filter X high-pass" }), { shiftKey: true });
    expect(vi.mocked(p.onFilter).mock.calls).toEqual([["x", "highPass", true]]);
  });

  it("a click cues one channel alone, Shift+click adds it", () => {
    const p = setup([true, false, true, false]);
    fireEvent.click(screen.getByRole("button", { name: "Channel 2 cue" }));
    expect(vi.mocked(p.onChannel).mock.calls).toEqual([
      [0, "cue", false],
      [1, "cue", true],
      [2, "cue", false],
    ]);
    vi.mocked(p.onChannel).mockClear();
    fireEvent.click(screen.getByRole("button", { name: "Channel 4 cue" }), { shiftKey: true });
    expect(vi.mocked(p.onChannel).mock.calls).toEqual([[3, "cue", true]]);
  });

  it("puts channels 1–2 on filter X and 3–4 on Y by default", () => {
    setup();
    for (const [n, side] of [
      [1, "X"],
      [2, "X"],
      [3, "Y"],
      [4, "Y"],
    ]) {
      const assign = screen.getByRole("radiogroup", { name: `Channel ${n} filter assign` });
      expect(
        within(assign)
          .getByRole("radio", { name: side as string })
          .getAttribute("aria-checked"),
      ).toBe("true");
    }
  });

  it("names no trademarks", () => {
    setup();
    expect(document.body.textContent).not.toMatch(/xone|allen/i);
  });
});
