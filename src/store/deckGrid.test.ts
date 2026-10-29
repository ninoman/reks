import { beforeEach, describe, expect, it, vi } from "vitest";

import type { Beatgrid, GridAnchor, GridEdit } from "../lib/ipc";

vi.mock("../lib/ipc", () => ({
  DECKS: 4,
  beatgridGet: vi.fn(),
  mixGridEdit: vi.fn(),
}));

const ipc = await import("../lib/ipc");
const { useDeckGrid } = await import("./deckGrid");
const { useMix } = await import("./mix");

const anchor = (positionMs: number, bpmX100 = 12000): GridAnchor => ({
  positionMs,
  bpmX100,
  beatInBar: 1,
  beatsPerBar: 4,
});
const grid = (anchors: GridAnchor[]): Beatgrid => ({
  anchors,
  source: "manual",
  locked: false,
  beats: [],
});

/** Lets queued edits go out and come back. */
const settle = () => new Promise((r) => setTimeout(r, 0));

let sent: GridEdit[];
let current: Beatgrid;

beforeEach(async () => {
  sent = [];
  current = grid([anchor(100)]);
  vi.mocked(ipc.beatgridGet).mockResolvedValue(current);
  vi.mocked(ipc.mixGridEdit).mockImplementation(async (_deck, edit) => {
    sent.push(edit);
    if (edit.op === "nudge") {
      current = grid(
        current.anchors.map((a) => ({ ...a, positionMs: a.positionMs + edit.deltaMs })),
      );
    } else if (edit.op === "setAnchors") {
      current = grid(edit.anchors);
    }
    return current;
  });
  useMix.setState({ setDeckBeats: vi.fn() });
  await useDeckGrid.getState().open(1, 7);
});

describe("deck grid mode", () => {
  it("adds up steps that arrive before an edit goes out, as one undo step", async () => {
    const g = useDeckGrid.getState();
    g.shift(0, 1, "press", false);
    g.shift(0, 1, "repeat", false);
    g.shift(0, 1, "repeat", false);
    await settle();
    // A press of 1 ms and two held steps of 10 ms.
    expect(sent).toEqual([{ op: "nudge", deltaMs: 21, section: null }]);
    g.shift(0, 1, "repeat", false);
    await settle();
    expect(sent.at(-1)).toEqual({ op: "nudge", deltaMs: 10, section: null });
    // Only the press records an undo step.
    expect(useDeckGrid.getState().past).toEqual([[anchor(100)]]);

    useDeckGrid.getState().undo();
    await settle();
    expect(useDeckGrid.getState().grid?.anchors).toEqual([anchor(100)]);
    expect(useDeckGrid.getState().future).toHaveLength(1);
  });

  it("moves the pin with the grid and retempos about it", async () => {
    useDeckGrid.setState({ pinMs: 2_100 });
    useDeckGrid.getState().shift(5_000, -1, "press", true);
    await settle();
    expect(useDeckGrid.getState().pinMs).toBe(2_090);
    useDeckGrid.getState().bpmStep(5_000, 1, "press", false);
    await settle();
    expect(sent.at(-1)).toEqual({ op: "retempo", atMs: 5_000, pivotMs: 2_090, bpmX100: 12001 });
  });

  it("keeps the error and drops edits after close", async () => {
    vi.mocked(ipc.mixGridEdit).mockRejectedValueOnce("the beatgrid is locked");
    useDeckGrid.getState().halve(0);
    await settle();
    expect(useDeckGrid.getState().error).toBe("the beatgrid is locked");

    useDeckGrid.getState().double(0);
    useDeckGrid.getState().close();
    await settle();
    expect(sent.some((e) => e.op === "double")).toBe(false);
    expect(useDeckGrid.getState().deck).toBeNull();
  });
});
