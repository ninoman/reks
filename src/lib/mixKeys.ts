/**
 * Mixer keyboard shortcuts (pure: key → action). The left hand plays the
 * left deck, the right hand the right one: decks 2 and 3, the centre pair;
 * Shift moves them to the outer decks 1 and 4 when four are shown.
 *
 * | Deck 2 / 3 | Key |
 * |---|---|
 * | cue (held from the cue point: preview) | A / J |
 * | play/pause | S / K |
 * | sync | D / L |
 * | nudge slower, faster (held) | Q, W / U, I |
 * | hot cue A–D (any pad mode) | 1–4 / 7–0 |
 * | beat jump back, forward by the last jump size | E, R / O, P |
 * | loop on/off at the last loop size | F / ; |
 * | take master | G / H |
 * | next pad mode | T / Y |
 *
 * Crossfader: Z left, X centre, C right (a tenth per press). Waveform zoom
 * (− / +) works with the panel closed too: `keys.ts`.
 */

/** A deck action without its deck. */
export type DeckAction =
  | { type: "cue" }
  | { type: "play" }
  | { type: "sync" }
  | { type: "nudge"; dir: -1 | 1 }
  | { type: "hotCue"; slot: number }
  | { type: "jump"; dir: -1 | 1 }
  | { type: "loop" }
  | { type: "master" }
  | { type: "padMode" };

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
  ["KeyE", "KeyO", { type: "jump", dir: -1 }],
  ["KeyR", "KeyP", { type: "jump", dir: 1 }],
  ["KeyF", "Semicolon", { type: "loop" }],
  ["KeyG", "KeyH", { type: "master" }],
  ["KeyT", "KeyY", { type: "padMode" }],
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

/** Deck for each hand: `[left, right]`, plain and with Shift. */
const HANDS = { plain: [1, 2], shift: [0, 3] } as const;

/**
 * The action for a key press, or null. `decks` is how many decks are shown:
 * Shift reaches decks 1 and 4 only when four are.
 */
export function mixKeyAction(e: KeyPress, decks: number): MixKeyAction | null {
  if (e.metaKey || e.ctrlKey || e.altKey) return null;
  const step = CROSSFADER[e.code];
  if (step !== undefined) return e.shiftKey ? null : { type: "crossfader", step };
  const hit = BY_CODE.get(e.code);
  if (hit === undefined || (e.shiftKey && decks < 4)) return null;
  return { ...hit.action, deck: HANDS[e.shiftKey ? "shift" : "plain"][hit.side] ?? 1 };
}

/** `KeyA` → "A", `Digit7` → "7", `Semicolon` → ";". */
const keyName = (code: string) => (code === "Semicolon" ? ";" : code.replace(/^(Key|Digit)/, ""));

/** Same action, including its direction or slot. */
const sameAction = (a: DeckAction, b: DeckAction) =>
  a.type === b.type &&
  ("dir" in a ? "dir" in b && a.dir === b.dir : true) &&
  ("slot" in a ? "slot" in b && a.slot === b.slot : true);

/**
 * How the key for `action` on `deck` (0-based) reads in a hint, taken from
 * the same table as `mixKeyAction`: "S", or "Shift S" for the outer decks.
 */
export function deckKeyHint(action: DeckAction, deck: number): string | undefined {
  const side = deck === 0 || deck === 1 ? 0 : 1;
  const shift = deck === 0 || deck === 3;
  const row = DECK_KEYS.find(([, , a]) => sameAction(a, action));
  const code = row?.[side];
  if (code === undefined) return undefined;
  return `${shift ? "Shift " : ""}${keyName(code)}`;
}

/** The crossfader keys as one hint: "Z X C". */
export function crossfaderKeyHint(): string {
  return Object.keys(CROSSFADER).map(keyName).join(" ");
}
