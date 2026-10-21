// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { TRACKS_MIME } from "../../lib/dnd";
import type { DeckView } from "../../lib/ipc";
import { type DeckMeta, emptyDeck } from "../../store/mix";
import { type DeckActions, DeckStrip, type DeckStripProps } from "./DeckStrip";

afterEach(cleanup);

const view = (over: Partial<DeckView> = {}): DeckView => ({
  trackId: 7,
  loaded: true,
  playing: false,
  positionMs: 12_300,
  durationMs: 30_000,
  cueMs: 0,
  tempo: 1.025,
  nudge: 1,
  speed: 1.025,
  bpm: 131.2,
  beat: 24.1,
  sync: false,
  syncBar: false,
  locked: false,
  quantise: true,
  ...over,
});

const loadedMeta = (over: Partial<DeckMeta> = {}): DeckMeta => ({
  ...emptyDeck(),
  track: { id: 7, title: "Dawn", artist: "Ron Trent" },
  hotCues: [{ slot: 1, positionMs: 4_000 }],
  columns: new Uint8Array(0),
  beats: [{ positionUs: 12_000, beatInBar: 1 }],
  ...over,
});

function setup(props: Partial<DeckStripProps> = {}) {
  const actions = Object.fromEntries(
    [
      "togglePlay",
      "cue",
      "toggleSync",
      "toggleBar",
      "toggleLeader",
      "toggleQuantise",
      "hotCue",
      "nudge",
      "setTempo",
      "setTempoRange",
      "seek",
      "eject",
      "drop",
    ].map((k) => [k, vi.fn()]),
  ) as unknown as DeckActions;
  render(
    <DeckStrip
      deck={1}
      view={undefined}
      meta={emptyDeck()}
      leader={false}
      leaderChosen={false}
      phase={null}
      compact={false}
      actions={actions}
      {...props}
    />,
  );
  return actions;
}

describe("DeckStrip", () => {
  it("empty: says how to load and disables the transport", () => {
    setup();
    expect(screen.getByText("Empty")).toBeTruthy();
    expect(screen.getByText(/Load to deck 2/)).toBeTruthy();
    expect(
      (screen.getByRole("button", { name: "Play deck 2" }) as HTMLButtonElement).disabled,
    ).toBe(true);
  });

  it("loading and error states", () => {
    setup({
      meta: {
        ...emptyDeck(),
        loading: { id: 3, title: "Echo", artist: null },
        error: "decode failed",
      },
    });
    expect(screen.getByText("Loading Echo…")).toBeTruthy();
    expect(screen.getByRole("alert").textContent).toBe("decode failed");
  });

  it("loaded: shows the track, tempo and time, and forwards actions", () => {
    const a = setup({ view: view(), meta: loadedMeta() });
    expect(screen.getByText("Dawn")).toBeTruthy();
    expect(screen.getByText("131.20")).toBeTruthy();
    expect(screen.getByText("+2.50%")).toBeTruthy();
    expect(screen.getByText("0:12.3")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Play deck 2" }));
    expect(a.togglePlay).toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Deck 2 sync" }));
    expect(a.toggleSync).toHaveBeenCalled();
    // Only set hot cues are enabled.
    const b = screen.getByRole("button", { name: "Deck 2 hot cue B" }) as HTMLButtonElement;
    const c = screen.getByRole("button", { name: "Deck 2 hot cue C" }) as HTMLButtonElement;
    expect(c.disabled).toBe(true);
    fireEvent.click(b);
    expect(a.hotCue).toHaveBeenCalledWith(1);
    fireEvent.click(screen.getByRole("button", { name: /tempo range/ }));
    expect(a.setTempoRange).toHaveBeenCalledWith(0.16);
    fireEvent.keyDown(screen.getByRole("slider", { name: "Deck 2 position" }), { key: "Home" });
    expect(a.seek).toHaveBeenCalledWith(0);
  });

  it("nudges while held", () => {
    const a = setup({ view: view(), meta: loadedMeta() });
    const faster = screen.getByRole("button", { name: "Deck 2 nudge faster" });
    fireEvent.pointerDown(faster);
    expect(a.nudge).toHaveBeenLastCalledWith(1);
    fireEvent.pointerUp(faster);
    expect(a.nudge).toHaveBeenLastCalledWith(0);
  });

  it("shows sync phase against the leader, and the leader itself", () => {
    setup({ view: view({ sync: true, locked: true }), meta: loadedMeta(), phase: -0.25 });
    expect(screen.getByRole("img", { name: "Phase: −0.25 beat" })).toBeTruthy();
    cleanup();
    setup({ view: view({ sync: true }), meta: loadedMeta(), leader: true });
    expect(screen.getByText("Leader")).toBeTruthy();
  });

  it("warns when the track has no grid", () => {
    setup({ view: view({ bpm: null, beat: null }), meta: loadedMeta({ beats: [] }) });
    expect(screen.getByText(/No beatgrid/)).toBeTruthy();
  });

  it("loads the first dropped track", () => {
    const a = setup();
    const data = JSON.stringify({ trackIds: [42, 43], playlistId: null, entryPositions: [] });
    const dataTransfer = { types: [TRACKS_MIME], getData: () => data, dropEffect: "none" };
    const deck = screen.getByRole("region", { name: "Deck 2" });
    fireEvent.dragOver(deck, { dataTransfer });
    fireEvent.drop(deck, { dataTransfer });
    expect(a.drop).toHaveBeenCalledWith(42);
  });
});
