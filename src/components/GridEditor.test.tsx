// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { Beatgrid, GridAnchor } from "../lib/ipc";
import { GridEditorDialog } from "./GridEditor";

vi.mock("../lib/ipc", async (orig) => ({
  ...(await orig<typeof import("../lib/ipc")>()),
  beatgridGet: vi.fn(),
  beatgridEdit: vi.fn(),
  beatgridTapTempo: vi.fn(),
  waveformColumns: vi.fn(),
  waveformTransients: vi.fn(),
}));

const ipc = await import("../lib/ipc");
const beatgridGet = vi.mocked(ipc.beatgridGet);
const beatgridEdit = vi.mocked(ipc.beatgridEdit);
const beatgridTapTempo = vi.mocked(ipc.beatgridTapTempo);
const waveformTransients = vi.mocked(ipc.waveformTransients);

const anchor = (positionMs: number, bpmX100: number, beatInBar = 1): GridAnchor => ({
  positionMs,
  bpmX100,
  beatInBar,
  beatsPerBar: 4,
});

function grid(patch: Partial<Beatgrid> = {}): Beatgrid {
  return {
    anchors: [anchor(12, 12800)],
    source: "analysis",
    locked: false,
    beats: [{ positionUs: 12_000, beatInBar: 1 }],
    ...patch,
  };
}

/** 140 BPM from 0, 174 BPM from 1 s. */
const twoTempo = () =>
  grid({ anchors: [anchor(0, 14000), anchor(1_000, 17400)], source: "manual" });

const onChanged = vi.fn();

function open() {
  render(<GridEditorDialog trackId={7} title="Song" onClose={() => {}} onChanged={onChanged} />);
}

const button = (name: string | RegExp) => screen.getByRole("button", { name });
const disabled = (name: string | RegExp) => (button(name) as HTMLButtonElement).disabled;
const waveform = () => screen.getByRole("img", { name: "Waveform and beatgrid" });
const bpm = () => screen.getByTestId("grid-bpm").textContent;
/** jsdom has no layout: x px from the left is x × 10 ms at the default zoom (100 px/s). */
const pick = (x: number) => fireEvent.pointerDown(waveform(), { clientX: x });

async function loaded() {
  await waitFor(() => expect(bpm()).not.toBe(""));
}

beforeEach(() => {
  vi.mocked(ipc.waveformColumns).mockResolvedValue(new Uint8Array(4 * 150 * 30).buffer);
  beatgridGet.mockResolvedValue(grid());
  beatgridEdit.mockImplementation(async () => grid({ source: "manual" }));
  waveformTransients.mockResolvedValue([100, 500]);
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("GridEditorDialog", () => {
  it("shows the grid and shifts it", async () => {
    open();
    await loaded();
    expect(bpm()).toBe("128.00");
    expect(screen.getByText("analysis")).toBeTruthy();
    fireEvent.click(button("+10 ms"));
    await waitFor(() => expect(onChanged).toHaveBeenCalled());
    expect(beatgridEdit).toHaveBeenCalledWith(7, { op: "nudge", deltaMs: 10 }, false);
    expect(screen.getByText("manual")).toBeTruthy();
    fireEvent.click(button("−1 ms"));
    await waitFor(() =>
      expect(beatgridEdit).toHaveBeenLastCalledWith(7, { op: "nudge", deltaMs: -1 }, false),
    );
  });

  it("edits at the cursor placed on the waveform", async () => {
    open();
    await loaded();
    expect(disabled("Downbeat at cursor")).toBe(true);
    expect(disabled("Add marker at cursor")).toBe(true);
    pick(150);
    expect(screen.getByText("Cursor 0:01.500")).toBeTruthy();
    fireEvent.click(button("Downbeat at cursor"));
    await waitFor(() =>
      expect(beatgridEdit).toHaveBeenLastCalledWith(7, { op: "setDownbeat", atMs: 1500 }, false),
    );
    fireEvent.click(button("Add marker at cursor"));
    await waitFor(() =>
      expect(beatgridEdit).toHaveBeenLastCalledWith(7, { op: "addMarker", atMs: 1500 }, false),
    );
    fireEvent.click(button("Stretch to cursor"));
    await waitFor(() =>
      expect(beatgridEdit).toHaveBeenLastCalledWith(
        7,
        { op: "stretch", section: 0, beatMs: 1500, toMs: 1500 },
        false,
      ),
    );
  });

  it("halves, doubles and retempos the selected section", async () => {
    beatgridGet.mockResolvedValue(twoTempo());
    beatgridEdit.mockImplementation(async () => twoTempo());
    open();
    await loaded();
    const sections = screen.getByRole("group", { name: "Tempo sections" });
    expect(within(sections).getAllByRole("button")).toHaveLength(2);
    fireEvent.click(within(sections).getByRole("button", { name: /Section 2: 174.00 BPM/ }));
    expect(bpm()).toBe("174.00");
    expect(screen.getByText("Section 2 of 2")).toBeTruthy();
    fireEvent.click(button("½"));
    await waitFor(() =>
      expect(beatgridEdit).toHaveBeenLastCalledWith(7, { op: "halve", section: 1 }, false),
    );
    fireEvent.click(button("×2"));
    await waitFor(() =>
      expect(beatgridEdit).toHaveBeenLastCalledWith(7, { op: "double", section: 1 }, false),
    );
    // Type a BPM: double-click the value, type, Enter.
    fireEvent.doubleClick(button("Section BPM"));
    const field = screen.getByRole("textbox", { name: "Section BPM" });
    fireEvent.change(field, { target: { value: "172.5" } });
    fireEvent.keyDown(field, { key: "Enter" });
    await waitFor(() =>
      expect(beatgridEdit).toHaveBeenLastCalledWith(
        7,
        { op: "setSectionBpm", section: 1, bpmX100: 17250 },
        false,
      ),
    );
    fireEvent.click(button("Reset section"));
    await waitFor(() =>
      expect(beatgridEdit).toHaveBeenLastCalledWith(7, { op: "reset", section: 1 }, false),
    );
    fireEvent.click(button("Delete marker"));
    await waitFor(() =>
      expect(beatgridEdit).toHaveBeenLastCalledWith(7, { op: "deleteMarker", section: 1 }, false),
    );
  });

  it("selects the section the cursor is in", async () => {
    beatgridGet.mockResolvedValue(twoTempo());
    open();
    await loaded();
    expect(bpm()).toBe("140.00");
    pick(120); // 1.2 s: the 174 BPM section
    expect(bpm()).toBe("174.00");
  });

  it("drags a marker and Alt-drags a beat to stretch", async () => {
    open();
    await loaded();
    // The marker at 12 ms is at 1.2 px: grabbed from 1 px.
    fireEvent.pointerDown(waveform(), { clientX: 1 });
    fireEvent.pointerMove(waveform(), { clientX: 30 });
    fireEvent.pointerUp(waveform(), { clientX: 50 });
    await waitFor(() =>
      expect(beatgridEdit).toHaveBeenLastCalledWith(
        7,
        { op: "moveMarker", section: 0, toMs: 500 },
        false,
      ),
    );
    fireEvent.pointerDown(waveform(), { clientX: 1, altKey: true });
    fireEvent.pointerUp(waveform(), { clientX: 60, altKey: true });
    await waitFor(() =>
      expect(beatgridEdit).toHaveBeenLastCalledWith(
        7,
        { op: "stretch", section: 0, beatMs: 12, toMs: 600 },
        false,
      ),
    );
  });

  it("undoes and redoes through Rust", async () => {
    const after = grid({ anchors: [anchor(22, 12800)], source: "manual" });
    beatgridEdit.mockResolvedValue(after);
    open();
    await loaded();
    expect(disabled("Undo")).toBe(true);
    fireEvent.click(button("+10 ms"));
    await waitFor(() => expect(disabled("Undo")).toBe(false));
    beatgridEdit.mockResolvedValue(grid());
    fireEvent.click(button("Undo"));
    await waitFor(() =>
      expect(beatgridEdit).toHaveBeenLastCalledWith(
        7,
        { op: "setAnchors", anchors: [anchor(12, 12800)] },
        false,
      ),
    );
    await waitFor(() => expect(disabled("Redo")).toBe(false));
    beatgridEdit.mockResolvedValue(after);
    fireEvent.click(button("Redo"));
    await waitFor(() =>
      expect(beatgridEdit).toHaveBeenLastCalledWith(
        7,
        { op: "setAnchors", anchors: [anchor(22, 12800)] },
        false,
      ),
    );
  });

  it("has keyboard shortcuts", async () => {
    beatgridEdit.mockResolvedValue(grid({ anchors: [anchor(13, 12800)], source: "manual" }));
    open();
    await loaded();
    const dialog = screen.getByRole("dialog");
    // One edit at a time: each key waits for the one before to be stored.
    let stored = 0;
    const key = async (init: Parameters<typeof fireEvent.keyDown>[1], edit: unknown) => {
      fireEvent.keyDown(dialog, init);
      stored += 1;
      await waitFor(() => expect(onChanged).toHaveBeenCalledTimes(stored));
      expect(beatgridEdit).toHaveBeenLastCalledWith(7, edit, false);
    };
    await key({ key: "ArrowRight", code: "ArrowRight" }, { op: "nudge", deltaMs: 1 });
    await key(
      { key: "ArrowLeft", code: "ArrowLeft", shiftKey: true },
      { op: "nudge", deltaMs: -10 },
    );
    pick(100);
    await key({ key: "m", code: "KeyM" }, { op: "addMarker", atMs: 1000 });
    await key({ key: "/", code: "Slash" }, { op: "halve", section: 0 });
    // The mock returns the same grid each time: one step, back to the start.
    await key(
      { key: "z", code: "KeyZ", metaKey: true },
      { op: "setAnchors", anchors: [anchor(12, 12800)] },
    );
  });

  it("snaps to transients when asked", async () => {
    open();
    await loaded();
    fireEvent.click(screen.getByRole("switch", { name: "Snap to transients" }));
    await waitFor(() => expect(waveformTransients).toHaveBeenCalledWith(7));
    pick(48);
    fireEvent.click(button("Add marker at cursor"));
    await waitFor(() =>
      expect(beatgridEdit).toHaveBeenLastCalledWith(7, { op: "addMarker", atMs: 480 }, true),
    );
  });

  it("disables edits on a locked grid and unlocks it", async () => {
    beatgridGet.mockResolvedValue(grid({ locked: true }));
    open();
    await loaded();
    for (const name of ["+1 ms", "×2", "½", "Tap", "Reset section"]) {
      expect(disabled(name)).toBe(true);
    }
    beatgridEdit.mockResolvedValue(grid());
    fireEvent.click(screen.getByRole("switch", { name: "Lock grid" }));
    expect(beatgridEdit).toHaveBeenCalledWith(7, { op: "setLocked", locked: false }, false);
    await waitFor(() => expect(disabled("+1 ms")).toBe(false));
    // Locking is no edit to undo.
    expect(disabled("Undo")).toBe(true);
  });

  it("taps a tempo onto the section", async () => {
    open();
    await loaded();
    beatgridTapTempo.mockResolvedValue(12450);
    fireEvent.click(button("Tap"));
    fireEvent.click(button("Tap"));
    expect(await screen.findByText("124.50")).toBeTruthy();
    expect(beatgridTapTempo).toHaveBeenLastCalledWith([expect.any(Number), expect.any(Number)]);
    fireEvent.click(button("Use tapped BPM"));
    await waitFor(() =>
      expect(beatgridEdit).toHaveBeenCalledWith(
        7,
        { op: "setSectionBpm", section: 0, bpmX100: 12450 },
        false,
      ),
    );
  });

  it("creates a grid by tapping when there is none, and shows errors", async () => {
    beatgridGet.mockResolvedValue(grid({ anchors: [], source: null, beats: [] }));
    open();
    expect(await screen.findByText("No grid")).toBeTruthy();
    expect(disabled("+1 ms")).toBe(true);
    beatgridTapTempo.mockResolvedValue(12000);
    fireEvent.click(button("Tap"));
    await screen.findByText("120.00");
    beatgridEdit.mockRejectedValue("bpm: out of range");
    fireEvent.click(button("Create grid"));
    expect((await screen.findByRole("alert")).textContent).toBe("bpm: out of range");
    expect(beatgridEdit).toHaveBeenCalledWith(
      7,
      { op: "setBpm", bpmX100: 12000, anchorMs: 0 },
      false,
    );
  });
});
