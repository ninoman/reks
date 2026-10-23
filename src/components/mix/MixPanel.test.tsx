// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { DeckView, MixState } from "../../lib/ipc";

vi.mock("../../lib/ipc", () => ({
  DECKS: 4,
  audioDevices: vi.fn(),
  mixDeviceOptions: vi.fn(),
  mixStart: vi.fn(),
  mixStop: vi.fn(),
  mixLoad: vi.fn(),
  mixControl: vi.fn(),
  libraryTrack: vi.fn(),
  waveformColumns: vi.fn(),
  beatgridGet: vi.fn(),
}));

const ipc = await import("../../lib/ipc");
const mixStart = vi.mocked(ipc.mixStart);
const mixControl = vi.mocked(ipc.mixControl);
const { useMix } = await import("../../store/mix");
const { usePanels } = await import("../../store/panels");
const { MixPanel } = await import("./MixPanel");
const { PlayerBar } = await import("../PlayerBar");

/** The panel under its toolbar, which shows the status line and errors. */
function renderMix() {
  usePanels.setState({ mixOpen: true });
  return render(
    <>
      <PlayerBar />
      <MixPanel />
    </>,
  );
}

const idleDeck: DeckView = {
  trackId: null,
  loaded: false,
  playing: false,
  positionMs: 0,
  durationMs: 0,
  cueMs: 0,
  tempo: 1,
  nudge: 1,
  speed: 1,
  bpm: null,
  beat: null,
  sync: false,
  syncBar: false,
  locked: false,
  quantise: false,
  master: false,
  loopRegion: null,
  loopIn: null,
  lastLoop: null,
  cuePreview: false,
};

const state = (over: Partial<MixState> = {}): MixState => ({
  device: "Laptop Speakers",
  sampleRate: 48_000,
  channels: 2,
  bufferFrames: 256,
  deviceLost: false,
  xruns: 0,
  masterDeck: null,
  decks: [idleDeck, idleDeck, idleDeck, idleDeck],
  meters: Array.from({ length: 4 }, () => ({ level: 0, clip: false })),
  masterMeter: { level: 0, clip: false },
  ...over,
});

const initial = useMix.getState();

beforeEach(() => {
  vi.mocked(ipc.audioDevices).mockResolvedValue(["Laptop Speakers"]);
  vi.mocked(ipc.mixDeviceOptions).mockResolvedValue({
    rates: [44_100, 48_000],
    defaultRate: 48_000,
    channels: 2,
    defaultCue: { type: "off" },
    bufferFrames: [64, 1024],
  });
  mixControl.mockResolvedValue(undefined);
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  useMix.setState(initial, true);
  usePanels.setState({ mixOpen: false });
});

describe("MixPanel", () => {
  it("stopped: shows the decks and mixer, and starts from the bar", async () => {
    let push: ((s: MixState) => void) | undefined;
    mixStart.mockImplementation(async (_config, onState) => {
      push = onState;
      return state();
    });
    renderMix();
    expect(screen.getByText("Audio output off")).toBeTruthy();
    // No start form: the decks and mixer are there before the output runs.
    expect(screen.getByRole("region", { name: "Mixer" })).toBeTruthy();
    expect(screen.getByRole("region", { name: "Deck 2" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Start output" }));

    await screen.findByRole("button", { name: "Stop output" });
    expect(screen.getByRole("status").textContent).toBe("Laptop Speakers · 48.0 kHz · 256 frames");
    // Two decks: 2 and 3, the centre channels.
    expect(screen.getByRole("region", { name: "Deck 2" })).toBeTruthy();
    expect(screen.getByRole("region", { name: "Deck 3" })).toBeTruthy();
    expect(screen.queryByRole("region", { name: "Deck 1" })).toBeNull();
    // The engine is told every mixer position after a start.
    await waitFor(() =>
      expect(mixControl).toHaveBeenCalledWith({ type: "crossfader", pos: 0.5, curve: 0 }),
    );

    act(() => push?.(state({ masterMeter: { level: 0.5, clip: false } })));
    const master = screen.getByRole("meter", { name: "Master meter" });
    // −6 dBFS is +4 on the LED scale: lit up to +3.
    expect(master.getAttribute("aria-valuenow")).toBe("3");
    act(() => push?.(state({ masterMeter: { level: 1, clip: true } })));
    // The engine reports a clip once; the meter holds it.
    act(() => push?.(state({ masterMeter: { level: 0.5, clip: false } })));
    expect(master.getAttribute("aria-valuetext")).toBe("Clipping");

    act(() => push?.(state({ deviceLost: true })));
    expect(screen.getByRole("alert").textContent).toMatch(/went away/);
    expect(screen.getByRole("button", { name: "Start output" })).toBeTruthy();
    // The decks stay on screen, empty, ready for the next load.
    expect(screen.getByRole("region", { name: "Deck 2" })).toBeTruthy();
  });

  it("shows a failed start and four decks on demand", async () => {
    mixStart.mockRejectedValueOnce(new Error("no output device"));
    renderMix();
    fireEvent.click(screen.getByRole("button", { name: "Start output" }));
    expect((await screen.findByRole("alert")).textContent).toBe("no output device");

    mixStart.mockResolvedValue(state());
    fireEvent.click(screen.getByRole("radio", { name: "4 decks" }));
    fireEvent.click(screen.getByRole("button", { name: "Start output" }));
    await screen.findByRole("button", { name: "Stop output" });
    expect(screen.getByRole("region", { name: "Deck 4" })).toBeTruthy();
    expect(screen.queryByRole("alert")).toBeNull();

    fireEvent.click(screen.getByRole("radio", { name: "1 deck" }));
    expect(usePanels.getState().mixOpen).toBe(false);
  });

  it("sizes the decks to the mixer: full-height decks at 2, compact at 4, and the panel capped for the library", async () => {
    mixStart.mockResolvedValue(state());
    renderMix();
    const panel = screen.getByRole("region", { name: "Mix" });
    expect(panel.className).toContain("max-h-(--mix-panel-max-h)");
    expect(screen.getByRole("region", { name: "Deck 2" }).className).toContain("h-(--deck-h)");
    expect(screen.getByRole("region", { name: "Deck 2" }).className).not.toContain("compact");

    fireEvent.click(screen.getByRole("radio", { name: "4 decks" }));
    for (const n of [1, 2, 3, 4])
      expect(screen.getByRole("region", { name: `Deck ${n}` }).className).toContain(
        "h-(--deck-h-compact)",
      );
  });
});
