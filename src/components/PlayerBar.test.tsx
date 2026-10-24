// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { DeckView, MixState } from "../lib/ipc";

vi.mock("../lib/ipc", () => ({
  DECKS: 4,
  audioDevices: vi.fn(),
  mixDeviceOptions: vi.fn(),
  mixControl: vi.fn(),
}));

const ipc = await import("../lib/ipc");
const { useMix } = await import("../store/mix");
const { usePanels } = await import("../store/panels");
const { PlayerBar } = await import("./PlayerBar");

const deck = (playing: boolean): DeckView =>
  ({ loaded: playing, playing, positionMs: 0, durationMs: 1000 }) as DeckView;

/** A running engine with the given decks (0-based) playing. */
function run(playing: number[]) {
  const live = {
    device: "Out",
    sampleRate: 48_000,
    channels: 2,
    bufferFrames: 256,
    deviceLost: false,
    xruns: 0,
    masterDeck: null,
    decks: [0, 1, 2, 3].map((d) => deck(playing.includes(d))),
    preview: deck(false),
    meters: [],
    masterMeter: { level: 0, clip: false },
  } as MixState;
  act(() => useMix.setState({ status: "running", live }));
}

const initial = useMix.getState();

beforeEach(() => {
  vi.mocked(ipc.audioDevices).mockResolvedValue([]);
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  useMix.setState(initial, true);
  usePanels.setState({ mixOpen: false });
});

describe("PlayerBar hidden-deck chip", () => {
  it("shows no chip when the playing decks are shown", () => {
    usePanels.setState({ mixOpen: true });
    useMix.setState({ deckCount: 2 });
    render(<PlayerBar />);
    run([1, 2]);
    expect(screen.queryByText(/playing/)).toBeNull();
  });

  it("names a deck playing off screen in 2-deck mode and shows 4 decks on click", () => {
    usePanels.setState({ mixOpen: true });
    useMix.setState({ deckCount: 2 });
    render(<PlayerBar />);
    run([0, 1]);
    fireEvent.click(screen.getByRole("button", { name: /Deck 1 playing/ }));
    expect(useMix.getState().deckCount).toBe(4);
    expect(usePanels.getState().mixOpen).toBe(true);
    expect(screen.queryByText(/playing/)).toBeNull();
  });

  it("lists every unseen deck in 1-deck mode", () => {
    render(<PlayerBar />);
    run([0, 1, 3]);
    expect(screen.getByText("Decks 1, 4 playing")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /show 4 decks/ }));
    expect(usePanels.getState().mixOpen).toBe(true);
    expect(useMix.getState().deckCount).toBe(4);
  });

  it("shows no chip while the output is stopped", () => {
    render(<PlayerBar />);
    expect(screen.queryByText(/playing/)).toBeNull();
  });
});
