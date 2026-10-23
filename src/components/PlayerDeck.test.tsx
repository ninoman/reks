// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("../lib/ipc", () => ({
  DECKS: 4,
  mixControl: vi.fn(),
  waveformColumns: vi.fn(),
  beatgridGet: vi.fn(),
}));

const { useMix } = await import("../store/mix");
const { usePanels } = await import("../store/panels");
const { PlayerDeck } = await import("./PlayerDeck");

const initial = useMix.getState();

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  useMix.setState(initial, true);
  usePanels.setState({ mixOpen: false });
});

const channel = (patch: object) =>
  useMix.getState().channels.map((c, i) => (i === 1 ? { ...c, ...patch } : c));

describe("PlayerDeck mixer chip", () => {
  it("is absent with channel 2 and the master at their defaults", () => {
    render(<PlayerDeck />);
    expect(screen.queryByRole("button", { name: /show the mixer/ })).toBeNull();
  });

  it("names a channel 2 EQ that is not flat and opens 2 decks on click", () => {
    useMix.setState({ channels: channel({ eqLo: 0 }) });
    render(<PlayerDeck />);
    expect(screen.getByText("Channel 2 EQ not flat")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /show the mixer/ }));
    expect(usePanels.getState().mixOpen).toBe(true);
    expect(useMix.getState().deckCount).toBe(2);
  });

  it("counts further deviations and follows the store", () => {
    useMix.setState({ channels: channel({ fader: 0.5, eqHi: 0 }) });
    render(<PlayerDeck />);
    expect(screen.getByText("Channel 2 fader not up +1")).toBeTruthy();
    act(() => useMix.setState({ channels: initial.channels }));
    expect(screen.queryByRole("button", { name: /show the mixer/ })).toBeNull();
    act(() => useMix.setState({ master: { ...useMix.getState().master, level: 0.3 } }));
    expect(screen.getByText("Master level changed")).toBeTruthy();
  });
});
