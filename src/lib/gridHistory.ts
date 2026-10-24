/**
 * Undo and redo for one grid editor session: each entry is the markers
 * before an edit. Restoring goes through Rust like any edit (`setAnchors`,
 * or `reset` back to no grid), so it is validated and stored the same way.
 */
import type { GridAnchor, GridEdit } from "./ipc";

/** Most edits kept to undo. */
export const HISTORY_LIMIT = 100;

export interface GridHistory {
  past: GridAnchor[][];
  future: GridAnchor[][];
}

export const EMPTY_HISTORY: GridHistory = { past: [], future: [] };

/** After an edit that changed the markers from `before`. */
export function recordEdit(h: GridHistory, before: GridAnchor[]): GridHistory {
  return { past: [...h.past, before].slice(-HISTORY_LIMIT), future: [] };
}

/** One step back from `current`: the edit to send and the history after it. */
export function undoStep(
  h: GridHistory,
  current: GridAnchor[],
): { edit: GridEdit; history: GridHistory } | null {
  const target = h.past.at(-1);
  if (target === undefined) return null;
  return {
    edit: restore(target),
    history: { past: h.past.slice(0, -1), future: [...h.future, current] },
  };
}

/** One step forward again. */
export function redoStep(
  h: GridHistory,
  current: GridAnchor[],
): { edit: GridEdit; history: GridHistory } | null {
  const target = h.future.at(-1);
  if (target === undefined) return null;
  return {
    edit: restore(target),
    history: { past: [...h.past, current], future: h.future.slice(0, -1) },
  };
}

/** The edit that puts `anchors` back; no markers is the grid before the first edit (none). */
function restore(anchors: GridAnchor[]): GridEdit {
  return anchors.length === 0 ? { op: "reset", section: null } : { op: "setAnchors", anchors };
}

/** Whether two marker lists are the same grid. */
export function sameAnchors(a: GridAnchor[], b: GridAnchor[]): boolean {
  return (
    a.length === b.length &&
    a.every((x, i) => {
      const y = b[i];
      return (
        y !== undefined &&
        x.positionMs === y.positionMs &&
        x.bpmX100 === y.bpmX100 &&
        x.beatInBar === y.beatInBar &&
        x.beatsPerBar === y.beatsPerBar
      );
    })
  );
}
