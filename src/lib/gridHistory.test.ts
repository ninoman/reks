import { describe, expect, it } from "vitest";

import {
  EMPTY_HISTORY,
  HISTORY_LIMIT,
  recordEdit,
  redoStep,
  sameAnchors,
  undoStep,
} from "./gridHistory";
import type { GridAnchor } from "./ipc";

const g = (positionMs: number, bpmX100 = 12800): GridAnchor[] => [
  { positionMs, bpmX100, beatInBar: 1, beatsPerBar: 4 },
];

describe("gridHistory", () => {
  it("undoes and redoes in order", () => {
    let h = recordEdit(EMPTY_HISTORY, g(0));
    h = recordEdit(h, g(10));
    // Current: 20.
    const u1 = undoStep(h, g(20));
    expect(u1?.edit).toEqual({ op: "setAnchors", anchors: g(10) });
    const u2 = u1 && undoStep(u1.history, g(10));
    expect(u2?.edit).toEqual({ op: "setAnchors", anchors: g(0) });
    expect(u2 && undoStep(u2.history, g(0))).toBeNull();
    const r = u2 && redoStep(u2.history, g(0));
    expect(r?.edit).toEqual({ op: "setAnchors", anchors: g(10) });
    // A new edit drops the redo branch.
    expect(r && recordEdit(r.history, g(10)).future).toEqual([]);
  });

  it("undoes back to no grid with a reset", () => {
    const h = recordEdit(EMPTY_HISTORY, []);
    expect(undoStep(h, g(0))?.edit).toEqual({ op: "reset", section: null });
    expect(redoStep(EMPTY_HISTORY, g(0))).toBeNull();
  });

  it("keeps a bounded history", () => {
    let h = EMPTY_HISTORY;
    for (let i = 0; i < HISTORY_LIMIT + 5; i++) h = recordEdit(h, g(i));
    expect(h.past).toHaveLength(HISTORY_LIMIT);
    expect(h.past[0]).toEqual(g(5));
  });

  it("compares markers", () => {
    expect(sameAnchors(g(1), g(1))).toBe(true);
    expect(sameAnchors(g(1), g(2))).toBe(false);
    expect(sameAnchors(g(1), g(1, 12000))).toBe(false);
    expect(sameAnchors(g(1), [])).toBe(false);
  });
});
