// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { useMix } from "../../store/mix";
import { useSettings } from "../../store/settings";
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
  it("the gear opens Settings on the Mixer tab", () => {
    setup();
    fireEvent.click(screen.getByRole("button", { name: "Mixer settings" }));
    expect(useSettings.getState().dialogOpen).toBe(true);
    expect(useSettings.getState().dialogTab).toBe("mixer");
    useSettings.setState({ dialogOpen: false });
  });

  it("lays out four channels, the master, both filters and the crossfader", () => {
    setup();
    for (const n of [1, 2, 3, 4]) {
      const strip = screen.getByRole("region", { name: `Channel ${n}` });
      for (const name of ["level", "EQ Hi", "EQ HM", "EQ LM", "EQ Lo", "fader"]) {
        expect(within(strip).getByRole("slider", { name: `Channel ${n} ${name}` })).toBeTruthy();
      }
    }
    expect(screen.getByRole("region", { name: "Master" })).toBeTruthy();
    expect(screen.getByRole("region", { name: "Filter X" })).toBeTruthy();
    expect(screen.getByRole("region", { name: "Filter Y" })).toBeTruthy();
    expect(screen.getByRole("slider", { name: "Crossfader" })).toBeTruthy();
    expect(screen.queryByText("VCF")).toBeNull();
    expect(screen.queryByText("Link")).toBeNull();
  });

  it("keeps every control in order: strips, master, filter row, crossfader row", () => {
    setup();
    const names = screen
      .getAllByRole("slider")
      .map((el) => el.getAttribute("aria-label"))
      .filter((n): n is string => n !== null);
    const channel = (n: number) => [
      `Channel ${n} level`,
      `Channel ${n} EQ Hi`,
      `Channel ${n} EQ HM`,
      `Channel ${n} EQ LM`,
      `Channel ${n} EQ Lo`,
      `Channel ${n} fader`,
    ];
    expect(names).toEqual([
      ...[1, 2, 3, 4].flatMap(channel),
      "Master level",
      "Cue level",
      "Cue and mix balance",
      "Filter X frequency",
      "Filter X resonance",
      "Filter Y frequency",
      "Filter Y resonance",
      "Crossfader",
      "Crossfader curve",
    ]);
    // The crossfader follows both filter units, and the curve its right end.
    const x = screen.getByRole("region", { name: "Filter X" });
    const y = screen.getByRole("region", { name: "Filter Y" });
    const fader = screen.getByRole("slider", { name: "Crossfader" });
    const curve = screen.getByRole("slider", { name: "Crossfader curve" });
    const follows = (a: Node, b: Node) =>
      a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING;
    expect(follows(x, y)).toBeTruthy();
    expect(follows(y, fader)).toBeTruthy();
    expect(follows(fader, curve)).toBeTruthy();
  });

  it("shows one dB scale and no labels on the channel meters", () => {
    setup();
    for (const n of [1, 2, 3, 4]) {
      expect(screen.getByRole("meter", { name: `Channel ${n} meter` }).textContent).toBe("");
    }
    expect(screen.getAllByText("+10")).toHaveLength(2); // the legend's scale and the master meter
  });

  it("lights the channel number badge of a loaded deck", () => {
    setup();
    const badge = (n: number) =>
      within(screen.getByRole("region", { name: `Channel ${n}` })).getByText(String(n));
    expect(badge(1).className).toContain("bg-accent-solid");
    expect(badge(2).className).not.toContain("bg-accent-solid");
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
