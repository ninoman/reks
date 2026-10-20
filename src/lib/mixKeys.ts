/**
 * Mixer keyboard shortcuts (pure: key → action). Decks 1 and 2 sit under
 * the left and right hand; Shift moves them to decks 3 and 4.
 *
 * | Deck 1 / 2 | Key |
 * |---|---|
 * | cue | A / J |
 * | play/pause | S / K |
 * | sync | D / L |
 * | nudge slower, faster (held) | Q, W / U, I |
 * | hot cue A–D | 1–4 / 7–0 |
 *
 * Crossfader: Z left, X centre, C right (a tenth per press).
 */

/** A deck action without its deck. */
type DeckAction =
  | { type: "cue" }
  | { type: "play" }
  | { type: "sync" }
  | { type: "nudge"; dir: -1 | 1 }
  | { type: "hotCue"; slot: number };

export type MixKeyAction =
  | (DeckAction & { deck: number })
  | { type: "crossfader"; step: -1 | 0 | 1 };

/** `[left-hand key, right-hand key, action]`. */
const DECK_KEYS: [string, string, DeckAction][] = [
  ["KeyA", "KeyJ", { type: "cue" }],
  ["KeyS", "KeyK", { type: "play" }],
  ["KeyD", "KeyL", { type: "sync" }],
  ["KeyQ", "KeyU", { type: "nudge", dir: -1 }],
  ["KeyW", "KeyI", { type: "nudge", dir: 1 }],
  ["Digit1", "Digit7", { type: "hotCue", slot: 0 }],
  ["Digit2", "Digit8", { type: "hotCue", slot: 1 }],
  ["Digit3", "Digit9", { type: "hotCue", slot: 2 }],
  ["Digit4", "Digit0", { type: "hotCue", slot: 3 }],
];

const BY_CODE = new Map<string, { side: number; action: DeckAction }>(
  DECK_KEYS.flatMap(([left, right, action]) => [
    [left, { side: 0, action }],
    [right, { side: 1, action }],
  ]),
);

const CROSSFADER: Record<string, -1 | 0 | 1> = { KeyZ: -1, KeyX: 0, KeyC: 1 };

export interface KeyPress {
  code: string;
  shiftKey: boolean;
  metaKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
}

/**
 * The action for a key press, or null. `decks` is how many decks are shown:
 * Shift reaches decks 3 and 4 only when four are.
 */
export function mixKeyAction(e: KeyPress, decks: number): MixKeyAction | null {
  if (e.metaKey || e.ctrlKey || e.altKey) return null;
  const step = CROSSFADER[e.code];
  if (step !== undefined) return e.shiftKey ? null : { type: "crossfader", step };
  const hit = BY_CODE.get(e.code);
  if (hit === undefined || (e.shiftKey && decks < 4)) return null;
  return { ...hit.action, deck: hit.side + (e.shiftKey ? 2 : 0) };
}
