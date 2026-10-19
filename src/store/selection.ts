/**
 * Row selection for the track table, as a pure reducer over row indices of
 * the current result. Index-based so a shift-range can span pages that are
 * not loaded yet; the store clears it whenever the query changes.
 */

export interface Selection {
  selected: ReadonlySet<number>;
  /** Fixed end of a shift-range. */
  anchor: number | null;
  /** Keyboard cursor. */
  focus: number | null;
}

export type SelectionAction =
  /** Plain click: select only this row. */
  | { type: "click"; index: number }
  /** Cmd/Ctrl-click: add or remove this row. */
  | { type: "toggle"; index: number }
  /** Shift-click: select from the anchor to this row. */
  | { type: "range"; index: number }
  /** Arrow keys, Page Up/Down, Home/End; `extend` = Shift held. */
  | { type: "move"; delta: number; extend: boolean; count: number }
  | { type: "selectAll"; count: number }
  | { type: "clear" };

export const emptySelection: Selection = { selected: new Set(), anchor: null, focus: null };

function range(a: number, b: number): Set<number> {
  const [lo, hi] = a <= b ? [a, b] : [b, a];
  const out = new Set<number>();
  for (let i = lo; i <= hi; i++) out.add(i);
  return out;
}

export function selectionReducer(state: Selection, action: SelectionAction): Selection {
  switch (action.type) {
    case "click":
      return { selected: new Set([action.index]), anchor: action.index, focus: action.index };
    case "toggle": {
      const selected = new Set(state.selected);
      if (selected.has(action.index)) selected.delete(action.index);
      else selected.add(action.index);
      return { selected, anchor: action.index, focus: action.index };
    }
    case "range": {
      if (state.anchor === null)
        return selectionReducer(state, { type: "click", index: action.index });
      return {
        selected: range(state.anchor, action.index),
        anchor: state.anchor,
        focus: action.index,
      };
    }
    case "move": {
      const { delta, extend, count } = action;
      if (count <= 0) return state;
      const base = state.focus ?? (delta > 0 ? -1 : count);
      const target = Math.min(count - 1, Math.max(0, base + delta));
      if (extend && state.anchor !== null) {
        return { selected: range(state.anchor, target), anchor: state.anchor, focus: target };
      }
      return { selected: new Set([target]), anchor: target, focus: target };
    }
    case "selectAll":
      if (action.count <= 0) return state;
      return { selected: range(0, action.count - 1), anchor: 0, focus: state.focus ?? 0 };
    case "clear":
      return emptySelection;
  }
}
