// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { Beatgrid } from "../lib/ipc";
import { GridEditorDialog } from "./GridEditor";

vi.mock("../lib/ipc", async (orig) => ({
  ...(await orig<typeof import("../lib/ipc")>()),
  beatgridGet: vi.fn(),
  beatgridEdit: vi.fn(),
  beatgridTapTempo: vi.fn(),
  waveformColumns: vi.fn(),
}));

const ipc = await import("../lib/ipc");
const beatgridGet = vi.mocked(ipc.beatgridGet);
const beatgridEdit = vi.mocked(ipc.beatgridEdit);
const beatgridTapTempo = vi.mocked(ipc.beatgridTapTempo);

function grid(patch: Partial<Beatgrid> = {}): Beatgrid {
  return {
    anchors: [{ positionMs: 12, bpmX100: 12800, beatInBar: 1, beatsPerBar: 4 }],
    source: "analysis",
    locked: false,
    beats: [{ positionUs: 12_000, beatInBar: 1 }],
    ...patch,
  };
}

const onChanged = vi.fn();

function open() {
  render(<GridEditorDialog trackId={7} title="Song" onClose={() => {}} onChanged={onChanged} />);
}

const button = (name: string | RegExp) => screen.getByRole("button", { name });

beforeEach(() => {
  vi.mocked(ipc.waveformColumns).mockResolvedValue(new Uint8Array(4 * 150).buffer);
  beatgridGet.mockResolvedValue(grid());
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("GridEditorDialog", () => {
  it("shows the grid and sends nudges to Rust", async () => {
    open();
    expect(await screen.findByText("128.00")).toBeTruthy();
    expect(screen.getByText("analysis")).toBeTruthy();
    beatgridEdit.mockResolvedValue(grid({ source: "manual" }));
    fireEvent.click(button("+10 ms"));
    await waitFor(() => expect(onChanged).toHaveBeenCalled());
    expect(beatgridEdit).toHaveBeenCalledWith(7, { op: "nudge", deltaMs: 10 });
    expect(screen.getByText("manual")).toBeTruthy();
    fireEvent.click(button("−1 ms"));
    expect(beatgridEdit).toHaveBeenLastCalledWith(7, { op: "nudge", deltaMs: -1 });
  });

  it("sets the downbeat at the cursor placed on the waveform", async () => {
    open();
    await screen.findByText("128.00");
    const setDownbeat = button("Set downbeat at cursor") as HTMLButtonElement;
    expect(setDownbeat.disabled).toBe(true);
    fireEvent.pointerDown(screen.getByRole("img", { name: "Waveform and beatgrid" }), {
      clientX: 0,
    });
    expect(screen.getByText("Cursor 0:00.000")).toBeTruthy();
    beatgridEdit.mockResolvedValue(grid());
    fireEvent.click(setDownbeat);
    expect(beatgridEdit).toHaveBeenCalledWith(7, { op: "setDownbeat", atMs: 0 });
    fireEvent.click(button("Double BPM"));
    fireEvent.click(button("Halve BPM"));
    await waitFor(() => expect(beatgridEdit).toHaveBeenCalledTimes(1));
  });

  it("disables edits on a locked grid and unlocks it", async () => {
    beatgridGet.mockResolvedValue(grid({ locked: true }));
    open();
    await screen.findByText("128.00");
    for (const name of ["+1 ms", "Double BPM", "Halve BPM", "Tap"]) {
      expect((button(name) as HTMLButtonElement).disabled).toBe(true);
    }
    beatgridEdit.mockResolvedValue(grid());
    fireEvent.click(screen.getByRole("switch", { name: "Lock grid" }));
    expect(beatgridEdit).toHaveBeenCalledWith(7, { op: "setLocked", locked: false });
    await waitFor(() => expect((button("+1 ms") as HTMLButtonElement).disabled).toBe(false));
  });

  it("taps a tempo and applies it", async () => {
    open();
    await screen.findByText("128.00");
    beatgridTapTempo.mockResolvedValue(12450);
    fireEvent.click(button("Tap"));
    fireEvent.click(button("Tap"));
    expect(await screen.findByText("124.50")).toBeTruthy();
    expect(beatgridTapTempo).toHaveBeenLastCalledWith([expect.any(Number), expect.any(Number)]);
    beatgridEdit.mockResolvedValue(grid());
    fireEvent.click(button("Use tapped BPM"));
    expect(beatgridEdit).toHaveBeenCalledWith(7, { op: "setBpm", bpmX100: 12450, anchorMs: 0 });
  });

  it("creates a grid by tapping when there is none, and shows errors", async () => {
    beatgridGet.mockResolvedValue(grid({ anchors: [], source: null, beats: [] }));
    open();
    expect(await screen.findByText("No grid")).toBeTruthy();
    expect((button("+1 ms") as HTMLButtonElement).disabled).toBe(true);
    beatgridTapTempo.mockResolvedValue(12000);
    fireEvent.click(button("Tap"));
    await screen.findByText("120.00");
    beatgridEdit.mockRejectedValue("bpm: out of range");
    fireEvent.click(button("Create grid"));
    expect((await screen.findByRole("alert")).textContent).toBe("bpm: out of range");
  });
});
