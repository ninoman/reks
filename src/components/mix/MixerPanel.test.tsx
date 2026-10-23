// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { FxKind } from "../../lib/ipc";
import type { MixerLook } from "../../lib/mixView";
import { useMix } from "../../store/mix";
import { useSettings } from "../../store/settings";
import { MixerPanel, type MixerPanelProps } from "./MixerPanel";

afterEach(cleanup);

const ADVANCED: MixerLook = { eq: "fourBand", view: "advanced", filters: "show" };

function setup(
  cues: boolean[] = [false, false, false, false],
  look: MixerLook = ADVANCED,
  fxKind: FxKind = "filter",
) {
  const s = useMix.getState();
  const props: MixerPanelProps = {
    channels: s.channels.map((c, i) => ({ ...c, cue: cues[i] ?? false })),
    active: [true, false, false, false],
    filters: s.filters,
    crossfader: s.crossfader,
    fx: { ...s.fx, kind: fxKind },
    master: s.master,
    look,
    onChannel: vi.fn(),
    onFilter: vi.fn(),
    onCrossfader: vi.fn(),
    onFx: vi.fn(),
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

  it("mirrors filter Y around the centre line and lights the ON lamp", () => {
    setup();
    const x = screen.getByRole("region", { name: "Filter X" });
    const y = screen.getByRole("region", { name: "Filter Y" });
    expect(x.className).not.toContain("flex-row-reverse");
    expect(y.className).toContain("flex-row-reverse");
    expect(
      screen.getByRole("button", { name: "Filter X on" }).getAttribute("aria-pressed"),
    ).toBeTruthy();
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

  it("Simple hides the FILTER and X-F rows, the crossfader and the curve", () => {
    setup([false, false, false, false], { eq: "fourBand", view: "simple", filters: "show" });
    expect(screen.queryByRole("radiogroup", { name: /assign/ })).toBeNull();
    expect(screen.queryByRole("slider", { name: "Crossfader" })).toBeNull();
    expect(screen.queryByRole("slider", { name: "Crossfader curve" })).toBeNull();
    expect(screen.queryByText("X-F")).toBeNull();
    expect(screen.queryByText("Filter")).toBeNull();
    expect(screen.getByRole("region", { name: "Filter X" })).toBeTruthy();
    expect(screen.getByRole("region", { name: "Filter Y" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Channel 1 cue" })).toBeTruthy();
  });

  it("Simple with the filters hidden also hides the filter row", () => {
    setup([false, false, false, false], { eq: "fourBand", view: "simple", filters: "hide" });
    expect(screen.queryByRole("region", { name: "Filter X" })).toBeNull();
    expect(screen.queryByRole("region", { name: "Filter Y" })).toBeNull();
    expect(screen.getByRole("slider", { name: "Channel 1 fader" })).toBeTruthy();
  });

  it("Advanced shows everything whatever the filters setting says", () => {
    setup([false, false, false, false], { eq: "fourBand", view: "advanced", filters: "hide" });
    expect(screen.getByRole("region", { name: "Filter X" })).toBeTruthy();
    expect(screen.getByRole("slider", { name: "Crossfader" })).toBeTruthy();
    expect(screen.getAllByRole("radiogroup", { name: /assign/ })).toHaveLength(8);
  });

  it("4-band has no FX select, PARAMETER or FX knobs", () => {
    setup();
    expect(screen.queryByRole("button", { name: /^FX effect/ })).toBeNull();
    expect(screen.queryByRole("slider", { name: /^FX / })).toBeNull();
    expect(screen.queryByRole("slider", { name: "Channel 1 FX" })).toBeNull();
  });

  describe("3-band", () => {
    const SIMPLE3: MixerLook = { eq: "threeBand", view: "simple", filters: "show" };
    const ADVANCED3: MixerLook = { eq: "threeBand", view: "advanced", filters: "show" };

    it("strips have LEVEL, HI, MID, LOW and FX knobs, in that order", () => {
      setup(undefined, SIMPLE3);
      for (const n of [1, 2, 3, 4]) {
        const strip = screen.getByRole("region", { name: `Channel ${n}` });
        const names = within(strip)
          .getAllByRole("slider")
          .map((el) => el.getAttribute("aria-label"));
        expect(names).toEqual([
          `Channel ${n} level`,
          `Channel ${n} EQ Hi`,
          `Channel ${n} EQ Mid`,
          `Channel ${n} EQ Low`,
          `Channel ${n} FX`,
          `Channel ${n} fader`,
        ]);
      }
    });

    it("sends the 3-band knobs as their own params", () => {
      const p = setup(undefined, SIMPLE3);
      fireEvent.keyDown(screen.getByRole("slider", { name: "Channel 2 EQ Mid" }), { key: "Home" });
      expect(p.onChannel).toHaveBeenCalledWith(1, "eq3Mid", 0);
      fireEvent.keyDown(screen.getByRole("slider", { name: "Channel 3 FX" }), { key: "End" });
      expect(p.onChannel).toHaveBeenCalledWith(2, "fx", 1);
    });

    it("has no X/Y filter row, no assigns and no crossfader in Simple", () => {
      setup(undefined, SIMPLE3);
      expect(screen.queryByRole("region", { name: "Filter X" })).toBeNull();
      expect(screen.queryByRole("region", { name: "Filter Y" })).toBeNull();
      expect(screen.queryByRole("radiogroup", { name: /assign/ })).toBeNull();
      expect(screen.queryByRole("slider", { name: "Crossfader" })).toBeNull();
      expect(screen.getByRole("button", { name: /^FX effect/ })).toBeTruthy();
    });

    it("Advanced adds the X-F assigns and the crossfader, still no FILTER assigns", () => {
      setup(undefined, ADVANCED3);
      expect(screen.getAllByRole("radiogroup", { name: /crossfader assign/ })).toHaveLength(4);
      expect(screen.queryByRole("radiogroup", { name: /filter assign/ })).toBeNull();
      expect(screen.queryByRole("region", { name: "Filter X" })).toBeNull();
      const fx = screen.getByRole("button", { name: /^FX effect/ });
      const fader = screen.getByRole("slider", { name: "Crossfader" });
      // The FX select sits above the crossfader.
      expect(fx.compareDocumentPosition(fader) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    });

    it("FX select, the FX row's caption, lists the effects and picks another", () => {
      const p = setup(undefined, SIMPLE3, "echo");
      const select = screen.getByRole("button", { name: "FX effect: Echo" });
      fireEvent.keyDown(select, { key: "Enter" });
      const items = screen
        .getAllByRole("menuitemradio")
        .map((el) => [el.textContent, el.getAttribute("aria-checked")]);
      expect(items).toEqual([
        ["FilterResonance", "false"],
        ["EchoFeedback", "true"],
        ["ReverbDecay", "false"],
      ]);
      fireEvent.click(screen.getByRole("menuitemradio", { name: /Reverb/ }));
      expect(p.onFx).toHaveBeenCalledWith({ kind: "reverb" });
    });

    it("PARAMETER, under the FX select, is captioned and named for the effect", () => {
      for (const [kind, caption, param] of [
        ["filter", "Reso", "Resonance"],
        ["echo", "Fdbk", "Feedback"],
        ["reverb", "Decay", "Decay"],
      ] as const) {
        const p = setup(undefined, SIMPLE3, kind);
        expect(screen.getByText(caption)).toBeTruthy();
        const knob = screen.getByRole("slider", { name: `FX ${param.toLowerCase()}` });
        const select = screen.getByRole("button", { name: /^FX effect/ });
        expect(
          select.compareDocumentPosition(knob) & Node.DOCUMENT_POSITION_FOLLOWING,
        ).toBeTruthy();
        fireEvent.keyDown(knob, { key: "End" });
        expect(p.onFx).toHaveBeenCalledWith({ parameter: 1 });
        cleanup();
      }
    });

    it("lights a channel knob off centre, the FX knob outside its dead zone", () => {
      const s = useMix.getState();
      const channels = s.channels.map((c, i) =>
        i === 0 ? { ...c, eq3Lo: 0.2, fx: 0.51 } : i === 1 ? { ...c, fx: 0.8 } : c,
      );
      render(
        <MixerPanel
          channels={channels}
          active={[false, false, false, false]}
          filters={s.filters}
          crossfader={s.crossfader}
          fx={s.fx}
          master={s.master}
          look={SIMPLE3}
          onChannel={vi.fn()}
          onFilter={vi.fn()}
          onCrossfader={vi.fn()}
          onFx={vi.fn()}
          onMaster={vi.fn()}
        />,
      );
      const lit = (name: string) => screen.getByRole("slider", { name }).hasAttribute("data-lit");
      expect(lit("Channel 1 EQ Low")).toBe(true);
      expect(lit("Channel 1 EQ Hi")).toBe(false);
      expect(lit("Channel 1 FX")).toBe(false);
      expect(lit("Channel 2 FX")).toBe(true);
    });
  });

  it("names no trademarks", () => {
    setup();
    expect(document.body.textContent).not.toMatch(/xone|allen/i);
  });
});
