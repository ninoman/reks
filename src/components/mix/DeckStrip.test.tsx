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
  masterTempo: false,
  master: false,
  loopRegion: null,
  loopIn: null,
  lastLoop: null,
  cuePreview: false,
  ...over,
});

const loadedMeta = (over: Partial<DeckMeta> = {}): DeckMeta => ({
  ...emptyDeck(),
  track: {
    kind: "library",
    id: 7,
    title: "Dawn",
    artist: "Ron Trent",
    key: "Am",
    artworkId: null,
    artworkMime: "",
  },
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
      "toggleMasterTempo",
      "hotCuePad",
      "editCues",
      "callCue",
      "callAdjacentCue",
      "beatJump",
      "loopPad",
      "toggleLoop",
      "loopIn",
      "loopOut",
      "reloop",
      "loopResize",
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
      zoom={2}
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
    setup({
      meta: { ...emptyDeck(), loading: { kind: "library", id: 3, title: "Echo", artist: null } },
    });
    expect(screen.getByText("Loading Echo…")).toBeTruthy();
    expect(screen.getByText("Decoding…")).toBeTruthy();
  });

  it("error: the reason with Retry and Eject", () => {
    const a = setup({
      meta: {
        ...emptyDeck(),
        error: "decode failed",
        failed: { kind: "library", id: 3, title: null, artist: null },
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

  it("MEMORY pads show the memory cues like hot cues: call, rename, delete, set", () => {
    const a = setup({ view: view(), meta: loadedMeta({ padMode: "memory" }) });
    const cue = screen.getByRole("button", { name: "Deck 2 memory cue at 0:06.0" });
    expect(cue.textContent).toContain("Break");
    fireEvent.click(cue);
    expect(a.callCue).toHaveBeenLastCalledWith(6_000, null);
    expect(
      screen.getByRole("button", { name: "Deck 2 memory loop at 0:20.0" }).textContent,
    ).toContain("LOOP");
    fireEvent.click(screen.getByRole("button", { name: "Delete deck 2 memory loop at 0:20.0" }));
    expect(a.editCues).toHaveBeenLastCalledWith({ type: "delete", cueId: 22 });
    fireEvent.doubleClick(cue);
    const field = screen.getByRole("textbox", { name: "Deck 2 memory cue at 0:06.0 name" });
    fireEvent.change(field, { target: { value: "Intro" } });
    fireEvent.keyDown(field, { key: "Enter" });
    expect(a.editCues).toHaveBeenLastCalledWith({ type: "rename", cueId: 21, name: "Intro" });
    fireEvent.click(screen.getByRole("button", { name: "Deck 2 set memory cue (pad 3)" }));
    expect(a.editCues).toHaveBeenLastCalledWith({ type: "addMemoryCue" });
  });

  it("right-click on a memory loop turns AUTO on", async () => {
    const a = setup({ view: view(), meta: loadedMeta({ padMode: "memory" }) });
    fireEvent.contextMenu(screen.getByRole("button", { name: "Deck 2 memory loop at 0:20.0" }));
    fireEvent.click(await screen.findByRole("menuitemcheckbox", { name: /Auto loop/ }));
    expect(a.editCues).toHaveBeenLastCalledWith({ type: "setActiveLoop", cueId: 22, on: true });
  });

  it("pages through memory cues eight at a time", () => {
    const memoryCues = Array.from({ length: 10 }, (_, i) => ({
      id: 100 + i,
      positionMs: (i + 1) * 10_000,
      endMs: null,
      activeLoop: false,
      colorRgb: null,
      name: null,
    }));
    setup({ view: view(), meta: loadedMeta({ padMode: "memory", memoryCues }) });
    expect(screen.queryByRole("button", { name: "Deck 2 memory cue at 1:30.0" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Deck 2 memory cues: next page" }));
    expect(screen.getByRole("button", { name: "Deck 2 memory cue at 1:30.0" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Deck 2 memory cue at 0:10.0" })).toBeNull();
  });

  it("M toggles master tempo and is lit while it is on", () => {
    const a = setup({ view: view({ masterTempo: true }), meta: loadedMeta() });
    const m = screen.getByRole("button", { name: "Deck 2 master tempo" });
    expect(m.getAttribute("aria-pressed")).toBe("true");
    fireEvent.click(m);
    expect(a.toggleMasterTempo).toHaveBeenCalled();
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

  it("switches pad mode; every mode shows page arrows", () => {
    const a = setup({ view: view(), meta: loadedMeta() });
    fireEvent.click(screen.getByRole("radio", { name: "LOOP" }));
    expect(a.setPadMode).toHaveBeenCalledWith("beatLoop");
    fireEvent.click(screen.getByRole("radio", { name: "MEMORY" }));
    expect(a.setPadMode).toHaveBeenCalledWith("memory");
    // HOT CUE keeps a disabled picker so the row doesn't shift.
    const hot = screen.getByRole("button", { name: "Deck 2 hot cues: next page" });
    expect((hot as HTMLButtonElement).disabled).toBe(true);
    cleanup();
    setup({ view: view(), meta: loadedMeta({ padMode: "memory" }) });
    expect(screen.getByRole("button", { name: "Deck 2 memory cues: next page" })).toBeTruthy();
    cleanup();
    setup({ view: view(), meta: loadedMeta({ padMode: "beatJump" }) });
    expect(screen.getByRole("button", { name: "Deck 2 jump sizes: next page" })).toBeTruthy();
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

describe("DeckStrip quick-loop block", () => {
  const btn = (name: string) => screen.getByRole("button", { name }) as HTMLButtonElement;
  const readout = () => screen.getByRole("img", { name: /Deck 2 (next )?loop size/ });
  const loop = { startMs: 1_000, endMs: 3_000, beats: 4 };

  it("is on the deck in every pad mode, with IN, OUT, ½, size, 2×, LOOP and RELOOP", () => {
    for (const padMode of ["hotCue", "memory", "beatJump", "beatLoop"] as const) {
      setup({ view: view(), meta: loadedMeta({ padMode }) });
      const block = screen.getByRole("group", { name: "Deck 2 quick loop" });
      expect([...block.querySelectorAll("button")].map((b) => b.textContent)).toEqual([
        "IN",
        "OUT",
        "½",
        "2×",
        "LOOP",
        "RELOOP",
      ]);
      expect(readout()).toBeTruthy();
      cleanup();
    }
  });

  it("empty deck: all of it is disabled", () => {
    setup();
    for (const name of [
      "Deck 2 loop in",
      "Deck 2 loop out",
      "Deck 2 halve loop",
      "Deck 2 double loop",
      "Deck 2 loop",
      "Deck 2 reloop",
    ]) {
      expect(btn(name).disabled).toBe(true);
    }
  });

  it("no loop: LOOP and the next size are on, the readout is dimmed, OUT and RELOOP wait", () => {
    const a = setup({ view: view(), meta: loadedMeta({ lastLoop: 1 / 8 }) });
    expect(readout().textContent).toBe("1/8");
    expect(readout().dataset.dimmed).toBe("true");
    expect(btn("Deck 2 loop").getAttribute("aria-pressed")).toBe("false");
    expect(btn("Deck 2 loop out").disabled).toBe(true);
    expect(btn("Deck 2 reloop").disabled).toBe(true);
    fireEvent.click(btn("Deck 2 halve loop"));
    fireEvent.click(btn("Deck 2 double loop"));
    expect(a.loopResize).toHaveBeenNthCalledWith(1, false);
    expect(a.loopResize).toHaveBeenNthCalledWith(2, true);
    fireEvent.click(btn("Deck 2 loop"));
    expect(a.toggleLoop).toHaveBeenCalledTimes(1);
    fireEvent.click(btn("Deck 2 loop in"));
    expect(a.loopIn).toHaveBeenCalledTimes(1);
  });

  it("no grid: IN works, LOOP and ½ / 2× do not", () => {
    setup({ view: view({ beat: null, bpm: null }), meta: loadedMeta({ beats: [] }) });
    expect(btn("Deck 2 loop in").disabled).toBe(false);
    expect(btn("Deck 2 loop").disabled).toBe(true);
    expect(btn("Deck 2 halve loop").disabled).toBe(true);
    expect(btn("Deck 2 double loop").disabled).toBe(true);
  });

  it("pending IN: IN is lit, and OUT is on once the head is past it", () => {
    const a = setup({ view: view({ loopIn: 20_000 }), meta: loadedMeta() });
    expect(btn("Deck 2 loop in").getAttribute("aria-pressed")).toBe("true");
    expect(btn("Deck 2 loop out").disabled).toBe(true);
    cleanup();
    const b = setup({ view: view({ loopIn: 4_000 }), meta: loadedMeta() });
    expect(btn("Deck 2 loop out").disabled).toBe(false);
    fireEvent.click(btn("Deck 2 loop out"));
    expect(b.loopOut).toHaveBeenCalledTimes(1);
    expect(a.loopOut).not.toHaveBeenCalled();
  });

  it("looping: LOOP is lit, the readout is the loop's length, RELOOP leaves it", () => {
    const a = setup({
      view: view({ loopRegion: { ...loop, beats: 3.87 }, lastLoop: loop }),
      meta: loadedMeta({ lastLoop: 8 }),
    });
    expect(btn("Deck 2 loop").getAttribute("aria-pressed")).toBe("true");
    expect(readout().textContent).toBe("3.87");
    expect(readout().dataset.dimmed).toBeUndefined();
    expect(btn("Deck 2 loop out").disabled).toBe(false);
    fireEvent.click(btn("Deck 2 reloop"));
    expect(a.reloop).toHaveBeenCalledTimes(1);
  });

  it("looping without a grid: the readout is in seconds", () => {
    setup({
      view: view({ loopRegion: { startMs: 1_000, endMs: 2_500, beats: null }, beat: null }),
      meta: loadedMeta({ beats: [] }),
    });
    expect(readout().textContent).toBe("1.5s");
    expect(btn("Deck 2 loop").disabled).toBe(false);
    expect(btn("Deck 2 halve loop").disabled).toBe(false);
  });

  it("reloop armed: a loop was left, or a memory loop was called", () => {
    const a = setup({ view: view({ lastLoop: loop }), meta: loadedMeta() });
    expect(btn("Deck 2 reloop").disabled).toBe(false);
    fireEvent.click(btn("Deck 2 reloop"));
    expect(a.reloop).toHaveBeenCalledTimes(1);
  });

  it("a memory loop pad sends its end with the call, so RELOOP can enter it", () => {
    const a = setup({ view: view(), meta: loadedMeta({ padMode: "memory" }) });
    fireEvent.click(screen.getByRole("button", { name: "Deck 2 memory loop at 0:20.0" }));
    expect(a.callCue).toHaveBeenLastCalledWith(20_000, 22_000);
  });

  it("the player deck alone has the block too", () => {
    setup({ solo: true, view: view({ loopIn: 4_000 }), meta: loadedMeta() });
    expect(screen.getByRole("group", { name: "Deck 2 quick loop" })).toBeTruthy();
    expect(btn("Deck 2 loop out").disabled).toBe(false);
  });
});

describe("DeckStrip sizes", () => {
  const deck = () => screen.getByRole("region", { name: "Deck 2" });
  const block = () => screen.getByRole("group", { name: "Deck 2 quick loop" });
  const pads = () => screen.getByRole("radiogroup", { name: "Deck 2 pad mode" });

  /** The controls row holds CUE / PLAY, the pads, the quick loop and MASTER / SYNC, in that order. */
  function expectControlsRow() {
    const row = block().parentElement;
    expect(row?.className).toContain("grid-cols-(--deck-ctl-cols)");
    expect(row?.contains(pads())).toBe(true);
    const order = ["Deck 2 cue", "Deck 2 pad mode", "Deck 2 quick loop", "Deck 2 master"].map(
      (name) =>
        Array.from(row?.children ?? []).findIndex(
          (c) =>
            c.querySelector(`[aria-label="${name}"]`) !== null ||
            c.getAttribute("aria-label") === name,
        ),
    );
    expect(order).toEqual([0, 1, 2, 3]);
  }

  it("2 decks: as tall as the mixer, the quick loop right of the pads", () => {
    setup({ view: view(), meta: loadedMeta() });
    expect(deck().className).toContain("h-(--deck-h)");
    expect(deck().dataset.deckSize).toBe("mix");
    expectControlsRow();
  });

  it("4 decks: 216 px, the same controls row as 2 decks", () => {
    setup({ view: view(), meta: loadedMeta(), compact: true });
    expect(deck().className).toContain("h-(--deck-h-compact)");
    // The compact header, overview, art and controls row sizes hang off this (tokens.css).
    expect(deck().dataset.deckSize).toBe("compact");
    expectControlsRow();
  });

  it("4 decks keep every control: Q, CUE, PLAY, MASTER and SYNC", () => {
    setup({ view: view(), meta: loadedMeta(), compact: true });
    for (const name of [
      "Deck 2 quantise",
      "Deck 2 cue",
      "Play deck 2",
      "Deck 2 master",
      "Deck 2 sync",
    ])
      expect(screen.getByRole("button", { name })).toBeTruthy();
  });
});
