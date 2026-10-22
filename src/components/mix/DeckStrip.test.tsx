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
  master: false,
  loopRegion: null,
  cuePreview: false,
  ...over,
});

const loadedMeta = (over: Partial<DeckMeta> = {}): DeckMeta => ({
  ...emptyDeck(),
  track: { id: 7, title: "Dawn", artist: "Ron Trent", key: "Am", artworkId: null, artworkMime: "" },
  hotCues: [
    { id: 11, slot: 1, positionMs: 4_000, colorRgb: null, name: null },
    { id: 12, slot: 2, positionMs: 8_000, colorRgb: 0xff_00_00, name: "Drop" },
  ],
  memoryCues: [
    { id: 21, positionMs: 6_000, endMs: null, activeLoop: false, colorRgb: null, name: "Break" },
    { id: 22, positionMs: 20_000, endMs: 22_000, activeLoop: false, colorRgb: null, name: null },
  ],
  columns: new Uint8Array(0),
  beats: [{ positionUs: 12_000, beatInBar: 1 }],
  ...over,
});

function setup(props: Partial<DeckStripProps> = {}) {
  const actions = Object.fromEntries(
    [
      "togglePlay",
      "play",
      "pause",
      "cue",
      "toggleSync",
      "takeMaster",
      "toggleQuantise",
      "hotCuePad",
      "editCues",
      "callCue",
      "callAdjacentCue",
      "beatJump",
      "loopPad",
      "setPadMode",
      "setPadPage",
      "nudge",
      "setTempo",
      "setTempoRange",
      "setBpm",
      "zoom",
      "seek",
      "eject",
      "drop",
      "retry",
    ].map((k) => [k, vi.fn()]),
  ) as unknown as DeckActions;
  render(
    <DeckStrip
      deck={1}
      view={undefined}
      meta={emptyDeck()}
      keyText={null}
      phase={null}
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

  it("loading: the title at once, decoding in the waveform slot", () => {
    setup({ meta: { ...emptyDeck(), loading: { id: 3, title: "Echo", artist: null } } });
    expect(screen.getByText("Loading Echo…")).toBeTruthy();
    expect(screen.getByText("Decoding…")).toBeTruthy();
  });

  it("error: the reason with Retry and Eject", () => {
    const a = setup({
      meta: {
        ...emptyDeck(),
        error: "decode failed",
        failed: { id: 3, title: null, artist: null },
      },
    });
    expect(screen.getByRole("alert").textContent).toContain("decode failed");
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(a.retry).toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Eject" }));
    expect(a.eject).toHaveBeenCalled();
  });

  it("analysing: playable, but what needs a grid waits", () => {
    setup({
      view: view({ bpm: null, beat: null }),
      meta: loadedMeta({ beats: [], analysing: true }),
    });
    expect(screen.getByText("Analysing the beatgrid…")).toBeTruthy();
    const enabled = (name: string) =>
      !(screen.getByRole("button", { name }) as HTMLButtonElement).disabled;
    expect(enabled("Play deck 2")).toBe(true);
    expect(enabled("Deck 2 hot cue B")).toBe(true);
    expect(enabled("Deck 2 sync")).toBe(false);
    expect(enabled("Deck 2 master")).toBe(false);
    expect(enabled("Deck 2 quantise")).toBe(false);
    expect(enabled("Deck 2 BPM")).toBe(false);
  });

  it("loaded: shows the track, tempo and time, and forwards actions", () => {
    const a = setup({ view: view(), meta: loadedMeta() });
    expect(screen.getByText("Dawn")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Deck 2 BPM" }).textContent).toBe("131.20");
    expect(screen.getByText("+2.50%")).toBeTruthy();
    expect(screen.getByText("0:12.3")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Play deck 2" }));
    expect(a.togglePlay).toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Deck 2 sync" }));
    expect(a.toggleSync).toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Deck 2 master" }));
    expect(a.takeMaster).toHaveBeenCalled();
    // Set and empty hot cue pads both act: jump, or set at the playhead.
    const b = screen.getByRole("button", { name: "Deck 2 hot cue B" }) as HTMLButtonElement;
    const d = screen.getByRole("button", { name: "Deck 2 hot cue D" }) as HTMLButtonElement;
    expect(d.disabled).toBe(false);
    fireEvent.click(b);
    expect(a.hotCuePad).toHaveBeenCalledWith(1);
    // A named cue shows its name; its colour fills the pad.
    const drop = screen.getByRole("button", { name: "Deck 2 hot cue C" });
    expect(drop.textContent).toBe("CDrop");
    expect(drop.style.getPropertyValue("--pad-color")).toBe("#ff0000");
    const range = screen.getByRole("button", { name: "Deck 2 tempo range ±10%" });
    expect(range.textContent).toBe("±10");
    fireEvent.click(range);
    expect(a.setTempoRange).toHaveBeenCalledWith(0.16);
    fireEvent.keyDown(screen.getByRole("slider", { name: "Deck 2 position" }), { key: "Home" });
    expect(a.seek).toHaveBeenCalledWith(0);
  });

  it("edits hot cues: an empty pad sets, double-click renames, Shift+click deletes", () => {
    const a = setup({ view: view(), meta: loadedMeta() });
    fireEvent.click(screen.getByRole("button", { name: "Deck 2 hot cue A" }));
    expect(a.hotCuePad).toHaveBeenLastCalledWith(0);
    expect(a.editCues).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "Deck 2 hot cue C" }), { shiftKey: true });
    expect(a.editCues).toHaveBeenLastCalledWith({ type: "delete", cueId: 12 });
    expect(a.hotCuePad).toHaveBeenCalledTimes(1);

    fireEvent.doubleClick(screen.getByRole("button", { name: "Deck 2 hot cue C" }));
    const field = screen.getByRole("textbox", { name: "Deck 2 hot cue C name" });
    expect((field as HTMLInputElement).value).toBe("Drop");
    fireEvent.change(field, { target: { value: "  Big drop " } });
    fireEvent.keyDown(field, { key: "Enter" });
    expect(a.editCues).toHaveBeenLastCalledWith({ type: "rename", cueId: 12, name: "Big drop" });
    expect(screen.queryByRole("textbox")).toBeNull();

    // Esc leaves the name as it was.
    fireEvent.doubleClick(screen.getByRole("button", { name: "Deck 2 hot cue B" }));
    const other = screen.getByRole("textbox", { name: "Deck 2 hot cue B name" });
    fireEvent.change(other, { target: { value: "Nope" } });
    fireEvent.keyDown(other, { key: "Escape" });
    expect(a.editCues).toHaveBeenCalledTimes(2);
    expect(screen.getByRole("button", { name: "Deck 2 hot cue B" })).toBeTruthy();
  });

  it("deletes a hot cue from its ×, reachable with Tab", () => {
    const a = setup({ view: view(), meta: loadedMeta() });
    const pad = screen.getByRole("button", { name: "Deck 2 hot cue C" });
    const x = screen.getByRole("button", { name: "Delete deck 2 hot cue C" });
    // Next in the tab order after its pad; empty pads have none.
    expect(pad.compareDocumentPosition(x) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Delete deck 2 hot cue A" })).toBeNull();
    fireEvent.click(x);
    expect(a.editCues).toHaveBeenLastCalledWith({ type: "delete", cueId: 12 });
    expect(a.hotCuePad).not.toHaveBeenCalled();
  });

  it("the memory list jumps, toggles AUTO on loops and deletes from ×", () => {
    const a = setup({ view: view(), meta: loadedMeta() });
    fireEvent.click(screen.getByRole("button", { name: "Deck 2 memory cues (2)" }));
    fireEvent.click(screen.getByRole("button", { name: "Jump to memory cue at 0:06.000" }));
    expect(a.callCue).toHaveBeenLastCalledWith(6_000);
    expect(screen.queryByRole("button", { name: "Auto loop at 0:06.000" })).toBeNull();
    const auto = screen.getByRole("button", { name: "Auto loop at 0:20.000" });
    expect(auto.getAttribute("aria-pressed")).toBe("false");
    fireEvent.click(auto);
    expect(a.editCues).toHaveBeenLastCalledWith({ type: "setActiveLoop", cueId: 22, on: true });
    fireEvent.click(screen.getByRole("button", { name: "Delete memory loop at 0:20.000" }));
    expect(a.editCues).toHaveBeenLastCalledWith({ type: "delete", cueId: 22 });
  });

  it("sets and calls memory cues in HOT CUE mode", () => {
    const a = setup({ view: view({ positionMs: 12_300 }), meta: loadedMeta() });
    fireEvent.click(screen.getByRole("button", { name: "Deck 2 set memory cue" }));
    expect(a.editCues).toHaveBeenLastCalledWith({ type: "addMemoryCue" });
    fireEvent.click(screen.getByRole("button", { name: "Deck 2 previous memory cue" }));
    expect(a.callAdjacentCue).toHaveBeenLastCalledWith(-1);
    fireEvent.click(screen.getByRole("button", { name: "Deck 2 next memory cue" }));
    expect(a.callAdjacentCue).toHaveBeenLastCalledWith(1);
    expect(screen.getByRole("button", { name: "Deck 2 memory cues (2)" }).textContent).toBe("2");
  });

  it("disables memory call past the last cue", () => {
    setup({ view: view({ positionMs: 25_000 }), meta: loadedMeta() });
    const next = screen.getByRole("button", {
      name: "Deck 2 next memory cue",
    }) as HTMLButtonElement;
    const prev = screen.getByRole("button", {
      name: "Deck 2 previous memory cue",
    }) as HTMLButtonElement;
    expect(next.disabled).toBe(true);
    expect(prev.disabled).toBe(false);
  });

  it("nudges while held", () => {
    const a = setup({ view: view(), meta: loadedMeta() });
    const faster = screen.getByRole("button", { name: "Deck 2 nudge faster" });
    fireEvent.pointerDown(faster);
    expect(a.nudge).toHaveBeenLastCalledWith(1);
    fireEvent.pointerUp(faster);
    expect(a.nudge).toHaveBeenLastCalledWith(0);
  });

  it("holds CUE from pointer down to up, and from key down to up", () => {
    const a = setup({ view: view(), meta: loadedMeta() });
    const cue = screen.getByRole("button", { name: "Deck 2 cue" });
    fireEvent.pointerDown(cue, { button: 0 });
    expect(a.cue).toHaveBeenLastCalledWith(true);
    fireEvent.pointerUp(cue);
    expect(a.cue).toHaveBeenLastCalledWith(false);
    fireEvent.keyDown(cue, { key: " " });
    expect(a.cue).toHaveBeenLastCalledWith(true);
    fireEvent.keyUp(cue, { key: " " });
    expect(a.cue).toHaveBeenLastCalledWith(false);
  });

  it("drag on the overview skims; a playing deck pauses and plays on", () => {
    vi.useFakeTimers();
    try {
      const a = setup({ view: view({ playing: true }), meta: loadedMeta() });
      const overview = screen.getByRole("slider", { name: "Deck 2 position" });
      overview.getBoundingClientRect = () => ({ left: 0, width: 300 }) as DOMRect;
      // A press jumps there and keeps playing.
      fireEvent.pointerDown(overview, { button: 0, pointerId: 1, clientX: 100 });
      expect(a.seek).toHaveBeenLastCalledWith(10_000);
      expect(a.pause).not.toHaveBeenCalled();
      // Moves within the throttle window send only the last one, on time.
      fireEvent.pointerMove(overview, { pointerId: 1, clientX: 150 });
      expect(a.pause).toHaveBeenCalled();
      fireEvent.pointerMove(overview, { pointerId: 1, clientX: 200 });
      expect(a.seek).toHaveBeenCalledTimes(1);
      vi.advanceTimersByTime(40);
      expect(a.seek).toHaveBeenLastCalledWith(20_000);
      fireEvent.pointerMove(overview, { pointerId: 1, clientX: 210 });
      fireEvent.pointerUp(overview, { pointerId: 1, clientX: 210 });
      expect(a.seek).toHaveBeenLastCalledWith(21_000);
      expect(a.play).toHaveBeenCalledTimes(1);
      // The canvas's own click does not seek again.
      fireEvent.click(overview);
      expect(a.seek).toHaveBeenCalledTimes(3);
    } finally {
      vi.useRealTimers();
    }
  });

  it("shows sync phase against the master; the master is lit and has none", () => {
    setup({ view: view({ sync: true, locked: true }), meta: loadedMeta(), phase: -0.25 });
    expect(screen.getByRole("img", { name: "Phase: −0.25 beat" })).toBeTruthy();
    cleanup();
    setup({ view: view({ sync: true, master: true }), meta: loadedMeta() });
    expect(screen.queryByRole("img", { name: /Phase/ })).toBeNull();
    expect(screen.getByRole("button", { name: "Deck 2 master" }).getAttribute("aria-pressed")).toBe(
      "true",
    );
  });

  it("BEAT JUMP: split pads jump back and forward; the last size is marked", () => {
    const a = setup({ view: view(), meta: loadedMeta({ padMode: "beatJump", lastJump: 2 }) });
    fireEvent.click(screen.getByRole("button", { name: "Deck 2 jump back FINE (1/32 beat)" }));
    expect(a.beatJump).toHaveBeenLastCalledWith(-1 / 32);
    fireEvent.click(screen.getByRole("button", { name: "Deck 2 jump forward 8 beats" }));
    expect(a.beatJump).toHaveBeenLastCalledWith(8);
    fireEvent.click(screen.getByRole("button", { name: "Deck 2 jump sizes: next page" }));
    expect(a.setPadPage).toHaveBeenCalledWith("beatJump", 1);
    cleanup();
    setup({
      view: view(),
      meta: loadedMeta({ padMode: "beatJump", padPage: { beatJump: 1, beatLoop: 0 } }),
    });
    expect(screen.getByRole("button", { name: "Deck 2 jump forward 128 beats" })).toBeTruthy();
  });

  it("BEAT LOOP: the active size is lit; pads set or leave the loop", () => {
    const a = setup({
      view: view({ loopRegion: { startMs: 1_000, endMs: 2_875, beats: 4 } }),
      meta: loadedMeta({ padMode: "beatLoop" }),
    });
    const four = screen.getByRole("button", { name: "Deck 2 loop 4 beats" });
    expect(four.getAttribute("aria-pressed")).toBe("true");
    fireEvent.click(four);
    expect(a.loopPad).toHaveBeenLastCalledWith(4);
    fireEvent.click(screen.getByRole("button", { name: "Deck 2 loop 1/8 beats" }));
    expect(a.loopPad).toHaveBeenLastCalledWith(1 / 8);
  });

  it("switches pad mode; HOT CUE shows the memory controls in place of the page arrows", () => {
    const a = setup({ view: view(), meta: loadedMeta() });
    fireEvent.click(screen.getByRole("radio", { name: "BEAT LOOP" }));
    expect(a.setPadMode).toHaveBeenCalledWith("beatLoop");
    expect(screen.queryByRole("button", { name: "Deck 2 jump sizes: next page" })).toBeNull();
    expect(screen.getByRole("button", { name: "Deck 2 set memory cue" })).toBeTruthy();
    cleanup();
    setup({ view: view(), meta: loadedMeta({ padMode: "beatJump" }) });
    expect(screen.getByRole("button", { name: "Deck 2 jump sizes: next page" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Deck 2 set memory cue" })).toBeNull();
  });

  it("ejects from the artwork", () => {
    const a = setup({ view: view(), meta: loadedMeta() });
    fireEvent.click(screen.getByRole("button", { name: "Eject deck 2" }));
    expect(a.eject).toHaveBeenCalled();
    cleanup();
    setup();
    expect(screen.queryByRole("button", { name: "Eject deck 2" })).toBeNull();
  });

  it("BPM: double-click to type one; Enter sets it, Esc cancels", () => {
    const a = setup({ view: view(), meta: loadedMeta() });
    fireEvent.doubleClick(screen.getByRole("button", { name: "Deck 2 BPM" }));
    const field = screen.getByRole("textbox", { name: "Deck 2 BPM" });
    fireEvent.change(field, { target: { value: "126.5" } });
    fireEvent.keyDown(field, { key: "Enter" });
    expect(a.setBpm).toHaveBeenCalledWith(126.5);
    fireEvent.doubleClick(screen.getByRole("button", { name: "Deck 2 BPM" }));
    fireEvent.change(screen.getByRole("textbox", { name: "Deck 2 BPM" }), {
      target: { value: "90" },
    });
    fireEvent.keyDown(screen.getByRole("textbox", { name: "Deck 2 BPM" }), { key: "Escape" });
    expect(a.setBpm).toHaveBeenCalledTimes(1);
    cleanup();
    // Synced: the BPM follows the master.
    setup({ view: view({ sync: true }), meta: loadedMeta() });
    expect((screen.getByRole("button", { name: "Deck 2 BPM" }) as HTMLButtonElement).disabled).toBe(
      true,
    );
  });

  it("shows the key in the chosen notation", () => {
    setup({ view: view(), meta: loadedMeta(), keyText: "8A" });
    expect(screen.getByText("8A")).toBeTruthy();
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

  it("solo (the player deck): no sync controls, says how to play", () => {
    setup({ solo: true });
    expect(screen.getByText("Double-click a track to play it, or drag one here")).toBeTruthy();
    cleanup();
    setup({ solo: true, view: view({ sync: true }), meta: loadedMeta() });
    expect(screen.queryByRole("button", { name: "Deck 2 sync" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Deck 2 master" })).toBeNull();
    expect(screen.getByRole("button", { name: "Deck 2 cue" })).toBeTruthy();
    expect(screen.getByRole("slider", { name: "Deck 2 tempo" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Deck 2 hot cue B" })).toBeTruthy();
  });
});
