/**
 * Mixer keyboard shortcuts (pure: key → action), laid out like rekordbox's
 * default PERFORMANCE keys ("Performance 1", rekordbox 7.0.5 reference) so
 * the same fingers work in both. The left hand plays the left deck, the
 * right hand the right one: decks 2 and 3, the centre pair; Option moves
 * them to the outer decks 1 and 4 when four are shown (rekordbox: decks 3
 * and 4). Shift is the second function of a key, ⌘ (or Ctrl) the third.
 *
 * | Deck 2 / 3 | Key |
 * |---|---|
 * | cue (held from the cue point: preview) | A / H |
 * | play/pause | Z / N |
 * | sync, take master | Q, Shift Q / Y, Shift Y |
 * | nudge slower, faster (held) | W, E / U, I |
 * | hot cue A–D (any pad mode) | 1–4 / 6–9 |
 * | previous, next memory cue | R, T / O, P |
 * | set memory cue at the playhead | Shift T / Shift P |
 * | beat jump back, forward by the last jump size | ⌘ R, ⌘ T / ⌘ O, ⌘ P |
 * | loop on/off at the last loop size | Shift F / Shift L |
 * | loop IN, OUT | D, F / K, L |
 * | reloop (or leave the loop) | G / ; |
 * | halve, double the loop (or the next loop size) | Shift D, Shift G / Shift K, Shift ; |
 * | next pad mode | Shift S / Shift J |
 *
 * Pad mode has no rekordbox key; Shift S / J is free there. Crossfader (no
 * rekordbox key): X left, C centre, V right (a tenth per press). Waveform
 * zoom (− / +) works with the panel closed too: `keys.ts`.
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
  | { type: "loopIn" }
  | { type: "loopOut" }
  | { type: "reloop" }
  | { type: "loopResize"; double: boolean }
  | { type: "master" }
  | { type: "padMode" }
  | { type: "memoryCue" }
  | { type: "callCue"; dir: -1 | 1 };

export type MixKeyAction =
  | (DeckAction & { deck: number })
  | { type: "crossfader"; step: -1 | 0 | 1 };

/** Which function of a key: plain, Shift, or ⌘/Ctrl. */
type Layer = "" | "Shift" | "Cmd";

/** `[left-hand key, right-hand key, action, layer]`. */
const DECK_KEYS: [string, string, DeckAction, Layer][] = [
  ["KeyA", "KeyH", { type: "cue" }, ""],
  ["KeyZ", "KeyN", { type: "play" }, ""],
  ["KeyQ", "KeyY", { type: "sync" }, ""],
  ["KeyQ", "KeyY", { type: "master" }, "Shift"],
  ["KeyW", "KeyU", { type: "nudge", dir: -1 }, ""],
  ["KeyE", "KeyI", { type: "nudge", dir: 1 }, ""],
  ["Digit1", "Digit6", { type: "hotCue", slot: 0 }, ""],
  ["Digit2", "Digit7", { type: "hotCue", slot: 1 }, ""],
  ["Digit3", "Digit8", { type: "hotCue", slot: 2 }, ""],
  ["Digit4", "Digit9", { type: "hotCue", slot: 3 }, ""],
  ["KeyR", "KeyO", { type: "callCue", dir: -1 }, ""],
  ["KeyT", "KeyP", { type: "callCue", dir: 1 }, ""],
  ["KeyT", "KeyP", { type: "memoryCue" }, "Shift"],
  ["KeyR", "KeyO", { type: "jump", dir: -1 }, "Cmd"],
  ["KeyT", "KeyP", { type: "jump", dir: 1 }, "Cmd"],
  ["KeyF", "KeyL", { type: "loop" }, "Shift"],
  ["KeyD", "KeyK", { type: "loopIn" }, ""],
  ["KeyF", "KeyL", { type: "loopOut" }, ""],
  ["KeyG", "Semicolon", { type: "reloop" }, ""],
  ["KeyD", "KeyK", { type: "loopResize", double: false }, "Shift"],
  ["KeyG", "Semicolon", { type: "loopResize", double: true }, "Shift"],
  ["KeyS", "KeyJ", { type: "padMode" }, "Shift"],
];

/** `layer+code` → hand and action. */
const BY_CODE = new Map<string, { side: number; action: DeckAction }>(
  DECK_KEYS.flatMap(([left, right, action, layer]) => [
    [`${layer}+${left}`, { side: 0, action }],
    [`${layer}+${right}`, { side: 1, action }],
  ]),
);

const CROSSFADER: Record<string, -1 | 0 | 1> = { KeyX: -1, KeyC: 0, KeyV: 1 };

export interface KeyPress {
  code: string;
  shiftKey: boolean;
  metaKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
}

/** Deck for each hand: `[left, right]`, plain and with Option. */
const HANDS = { plain: [1, 2], outer: [0, 3] } as const;

/**
 * The action for a key press, or null. `decks` is how many decks are shown:
 * Option reaches decks 1 and 4 only when four are.
 */
export function mixKeyAction(e: KeyPress, decks: number): MixKeyAction | null {
  const cmd = e.metaKey || e.ctrlKey;
  // Shift and ⌘ together are no layer (⌘ Shift 1–4 clears hot cues in rekordbox).
  if (cmd && e.shiftKey) return null;
  const layer: Layer = cmd ? "Cmd" : e.shiftKey ? "Shift" : "";
  const step = layer === "" && !e.altKey ? CROSSFADER[e.code] : undefined;
  if (step !== undefined) return { type: "crossfader", step };
  const hit = BY_CODE.get(`${layer}+${e.code}`);
  if (hit === undefined || (e.altKey && decks < 4)) return null;
  return { ...hit.action, deck: HANDS[e.altKey ? "outer" : "plain"][hit.side] ?? 1 };
}

/** `KeyA` → "A", `Digit7` → "7", `Semicolon` → ";". */
const keyName = (code: string) => (code === "Semicolon" ? ";" : code.replace(/^(Key|Digit)/, ""));

const LAYER_NAME: Record<Layer, string> = { "": "", Shift: "Shift ", Cmd: "⌘ " };

/** Same action, including its direction or slot. */
const sameAction = (a: DeckAction, b: DeckAction) =>
  a.type === b.type &&
  ("dir" in a ? "dir" in b && a.dir === b.dir : true) &&
  ("slot" in a ? "slot" in b && a.slot === b.slot : true) &&
  ("double" in a ? "double" in b && a.double === b.double : true);

/**
 * How the key for `action` on `deck` (0-based) reads in a hint, taken from
 * the same table as `mixKeyAction`: "Z", or "⌥ Z" for the outer decks.
 */
export function deckKeyHint(action: DeckAction, deck: number): string | undefined {
  const side = deck === 0 || deck === 1 ? 0 : 1;
  const outer = deck === 0 || deck === 3;
  const row = DECK_KEYS.find(([, , a]) => sameAction(a, action));
  if (row === undefined) return undefined;
  return `${outer ? "⌥ " : ""}${LAYER_NAME[row[3]]}${keyName(row[side])}`;
}

/** The crossfader keys as one hint: "X C V". */
export function crossfaderKeyHint(): string {
  return Object.keys(CROSSFADER).map(keyName).join(" ");
}

/**
 * The keys of `actions` for both hands, as the shortcut list reads them:
 * "W E / U I", "Shift T / Shift P", "1–4 / 6–9" (`range`: first to last).
 * The layer ("Shift", "⌘") is named once, before the keys.
 */
export function deckKeysHelp(actions: DeckAction[], range = false): string {
  const rows = actions.map((a) => DECK_KEYS.find(([, , row]) => sameAction(row, a)));
  const layer = rows[0]?.[3] ?? "";
  const hand = (side: 0 | 1) => {
    const keys = rows.map((r) => (r === undefined ? "" : keyName(r[side])));
    const first = keys[0] ?? "";
    const last = keys[keys.length - 1] ?? "";
    return `${LAYER_NAME[layer]}${range ? `${first}–${last}` : keys.join(" ")}`;
  };
  return `${hand(0)} / ${hand(1)}`;
}
