import { describe, expect, it } from "vitest";

import {
  emptySelection,
  selectionReducer,
  type Selection,
  type SelectionAction,
} from "./selection";

function run(...actions: SelectionAction[]): Selection {
  return actions.reduce(selectionReducer, emptySelection);
}

const sel = (s: Selection) => [...s.selected].sort((a, b) => a - b);

describe("selectionReducer", () => {
  it("click selects one row and sets anchor and focus", () => {
    const s = run({ type: "click", index: 3 }, { type: "click", index: 5 });
    expect(sel(s)).toEqual([5]);
    expect(s.anchor).toBe(5);
    expect(s.focus).toBe(5);
  });

  it("toggle adds and removes rows", () => {
    const s = run(
      { type: "click", index: 1 },
      { type: "toggle", index: 4 },
      { type: "toggle", index: 6 },
    );
    expect(sel(s)).toEqual([1, 4, 6]);
    expect(sel(selectionReducer(s, { type: "toggle", index: 4 }))).toEqual([1, 6]);
  });

  it("range selects from the anchor in both directions", () => {
    expect(sel(run({ type: "click", index: 2 }, { type: "range", index: 5 }))).toEqual([
      2, 3, 4, 5,
    ]);
    expect(sel(run({ type: "click", index: 5 }, { type: "range", index: 2 }))).toEqual([
      2, 3, 4, 5,
    ]);
    // The anchor stays put, so a second shift-click re-ranges from it.
    const s = run(
      { type: "click", index: 5 },
      { type: "range", index: 8 },
      { type: "range", index: 3 },
    );
    expect(sel(s)).toEqual([3, 4, 5]);
    expect(s.anchor).toBe(5);
    expect(s.focus).toBe(3);
  });

  it("range without an anchor acts as a click", () => {
    expect(sel(run({ type: "range", index: 4 }))).toEqual([4]);
  });

  it("range after toggle anchors on the toggled row", () => {
    const s = run(
      { type: "click", index: 0 },
      { type: "toggle", index: 6 },
      { type: "range", index: 8 },
    );
    expect(sel(s)).toEqual([6, 7, 8]);
  });

  it("move steps the focus and clamps to the result", () => {
    const count = 10;
    expect(sel(run({ type: "move", delta: 1, extend: false, count }))).toEqual([0]);
    expect(sel(run({ type: "move", delta: -1, extend: false, count }))).toEqual([9]);
    const s = run({ type: "click", index: 8 }, { type: "move", delta: 5, extend: false, count });
    expect(sel(s)).toEqual([9]);
    expect(
      sel(
        run({ type: "click", index: 2 }, { type: "move", delta: -Infinity, extend: false, count }),
      ),
    ).toEqual([0]);
    expect(
      sel(
        run({ type: "click", index: 2 }, { type: "move", delta: Infinity, extend: false, count }),
      ),
    ).toEqual([9]);
  });

  it("move with shift extends from the anchor", () => {
    const count = 10;
    const s = run(
      { type: "click", index: 4 },
      { type: "move", delta: 1, extend: true, count },
      { type: "move", delta: 1, extend: true, count },
    );
    expect(sel(s)).toEqual([4, 5, 6]);
    const back = selectionReducer(s, { type: "move", delta: -4, extend: true, count });
    expect(sel(back)).toEqual([2, 3, 4]);
    expect(back.anchor).toBe(4);
  });

  it("move on an empty result does nothing", () => {
    expect(run({ type: "move", delta: 1, extend: false, count: 0 })).toBe(emptySelection);
  });

  it("selectAll and clear", () => {
    const all = run({ type: "click", index: 1 }, { type: "selectAll", count: 4 });
    expect(sel(all)).toEqual([0, 1, 2, 3]);
    expect(all.focus).toBe(1);
    expect(selectionReducer(all, { type: "clear" })).toBe(emptySelection);
  });
});
