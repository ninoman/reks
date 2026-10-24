/**
 * The grid editor's keyboard shortcuts (the dialog's own; the app's global
 * keys leave dialogs alone, `keys.ts`). Text fields keep their keys.
 */
export type GridKeyAction =
  | { type: "nudge"; deltaMs: number }
  | { type: "moveMarker"; deltaMs: number }
  | { type: "addMarker" }
  | { type: "deleteMarker" }
  | { type: "setDownbeat" }
  | { type: "tap" }
  | { type: "halve" }
  | { type: "double" }
  | { type: "select"; step: -1 | 1 }
  | { type: "zoom"; step: -1 | 1 }
  | { type: "snap" }
  | { type: "undo" }
  | { type: "redo" };

/** What a key press does in the editor, as shown in its shortcut list. */
export const GRID_SHORTCUTS: [keys: string, what: string][] = [
  ["← →", "shift the grid 1 ms (Shift: 10 ms)"],
  ["Alt ← →", "move the selected marker"],
  ["M", "add a marker at the cursor"],
  ["Del", "delete the selected marker"],
  ["D", "downbeat at the cursor"],
  ["T", "tap tempo"],
  ["/ *", "halve / double the section"],
  ["[ ]", "previous / next section"],
  ["− +", "zoom"],
  ["S", "snap to transients"],
  ["⌘Z ⇧⌘Z", "undo / redo"],
];

interface KeyLike {
  key: string;
  code: string;
  shiftKey: boolean;
  altKey: boolean;
  metaKey: boolean;
  ctrlKey: boolean;
  target: EventTarget | null;
}

/** The action for `e`, or null for a key the editor leaves alone. */
export function gridKey(e: KeyLike): GridKeyAction | null {
  const target = e.target instanceof HTMLElement ? e.target : null;
  // Text fields keep their keys.
  if (target !== null && target.closest("input, textarea, [contenteditable='true']") !== null) {
    return null;
  }
  const mod = e.metaKey || e.ctrlKey;
  if (mod) {
    if (e.code === "KeyZ") return e.shiftKey ? { type: "redo" } : { type: "undo" };
    if (e.code === "KeyY") return { type: "redo" };
    return null;
  }
  const step = e.shiftKey ? 10 : 1;
  switch (e.key) {
    case "ArrowLeft":
    case "ArrowRight": {
      const deltaMs = e.key === "ArrowLeft" ? -step : step;
      return e.altKey ? { type: "moveMarker", deltaMs } : { type: "nudge", deltaMs };
    }
    case "Delete":
    case "Backspace":
      return { type: "deleteMarker" };
    case "/":
      return { type: "halve" };
    case "*":
      return { type: "double" };
    case "[":
      return { type: "select", step: -1 };
    case "]":
      return { type: "select", step: 1 };
    case "-":
      return { type: "zoom", step: -1 };
    case "+":
    case "=":
      return { type: "zoom", step: 1 };
  }
  if (e.altKey) return null;
  switch (e.code) {
    case "KeyM":
      return { type: "addMarker" };
    case "KeyD":
      return { type: "setDownbeat" };
    case "KeyT":
      return { type: "tap" };
    case "KeyS":
      return { type: "snap" };
  }
  return null;
}
