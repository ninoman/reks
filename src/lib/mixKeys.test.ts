import { describe, expect, it } from "vitest";

import {
  crossfaderKeyHint,
  type DeckAction,
  deckKeyHint,
  type KeyPress,
  mixKeyAction,
} from "./mixKeys";

const key = (code: string, mods: Partial<KeyPress> = {}): KeyPress => ({
  code,
  shiftKey: false,
  metaKey: false,
  ctrlKey: false,
  altKey: false,
  ...mods,
});

describe("mixKeyAction", () => {
  it("maps each hand to its deck", () => {
    expect(mixKeyAction(key("KeyS"), 2)).toEqual({ type: "play", deck: 1 });
    expect(mixKeyAction(key("KeyK"), 2)).toEqual({ type: "play", deck: 2 });
    expect(mixKeyAction(key("KeyU"), 2)).toEqual({ type: "nudge", dir: -1, deck: 2 });
    expect(mixKeyAction(key("Digit9"), 2)).toEqual({ type: "hotCue", slot: 2, deck: 2 });
    expect(mixKeyAction(key("KeyE"), 2)).toEqual({ type: "jump", dir: -1, deck: 1 });
    expect(mixKeyAction(key("KeyP"), 2)).toEqual({ type: "jump", dir: 1, deck: 2 });
    expect(mixKeyAction(key("KeyF"), 2)).toEqual({ type: "loop", deck: 1 });
    expect(mixKeyAction(key("Semicolon"), 2)).toEqual({ type: "loop", deck: 2 });
    expect(mixKeyAction(key("KeyG"), 2)).toEqual({ type: "master", deck: 1 });
    expect(mixKeyAction(key("KeyH"), 2)).toEqual({ type: "master", deck: 2 });
    expect(mixKeyAction(key("KeyT"), 2)).toEqual({ type: "padMode", deck: 1 });
    expect(mixKeyAction(key("KeyY"), 2)).toEqual({ type: "padMode", deck: 2 });
  });

  it("Shift reaches the outer decks 1 and 4 only when four are shown", () => {
    expect(mixKeyAction(key("KeyA", { shiftKey: true }), 4)).toEqual({ type: "cue", deck: 0 });
    expect(mixKeyAction(key("KeyL", { shiftKey: true }), 4)).toEqual({ type: "sync", deck: 3 });
    expect(mixKeyAction(key("KeyA", { shiftKey: true }), 2)).toBeNull();
  });

  it("moves the crossfader and leaves other keys and shortcuts alone", () => {
    expect(mixKeyAction(key("KeyZ"), 2)).toEqual({ type: "crossfader", step: -1 });
    expect(mixKeyAction(key("KeyX"), 2)).toEqual({ type: "crossfader", step: 0 });
    expect(mixKeyAction(key("KeyS", { metaKey: true }), 2)).toBeNull();
    expect(mixKeyAction(key("KeyZ", { shiftKey: true }), 4)).toBeNull();
    expect(mixKeyAction(key("Backquote"), 2)).toBeNull();
    // Zoom is not a deck key: keys.ts handles it everywhere.
    expect(mixKeyAction(key("KeyV"), 2)).toBeNull();
    expect(mixKeyAction(key("Minus"), 2)).toBeNull();
  });
});

describe("key hints", () => {
  it("name the key that mixKeyAction maps to the same action", () => {
    expect(deckKeyHint({ type: "play" }, 1)).toBe("S");
    expect(deckKeyHint({ type: "play" }, 2)).toBe("K");
    expect(deckKeyHint({ type: "play" }, 0)).toBe("Shift S");
    expect(deckKeyHint({ type: "hotCue", slot: 2 }, 3)).toBe("Shift 9");
    expect(deckKeyHint({ type: "nudge", dir: 1 }, 2)).toBe("I");
    expect(deckKeyHint({ type: "hotCue", slot: 4 }, 1)).toBeUndefined();
    expect(deckKeyHint({ type: "jump", dir: 1 }, 1)).toBe("R");
    expect(deckKeyHint({ type: "loop" }, 2)).toBe(";");
    expect(deckKeyHint({ type: "padMode" }, 3)).toBe("Shift Y");
  });

  it("match the table in docs/ui.md", () => {
    const row = (action: DeckAction) => [deckKeyHint(action, 1), deckKeyHint(action, 2)];
    expect(row({ type: "cue" })).toEqual(["A", "J"]);
    expect(row({ type: "play" })).toEqual(["S", "K"]);
    expect(row({ type: "sync" })).toEqual(["D", "L"]);
    expect(row({ type: "nudge", dir: -1 })).toEqual(["Q", "U"]);
    expect(row({ type: "nudge", dir: 1 })).toEqual(["W", "I"]);
    expect(row({ type: "hotCue", slot: 0 })).toEqual(["1", "7"]);
    expect(row({ type: "hotCue", slot: 3 })).toEqual(["4", "0"]);
    expect(row({ type: "jump", dir: -1 })).toEqual(["E", "O"]);
    expect(row({ type: "jump", dir: 1 })).toEqual(["R", "P"]);
    expect(row({ type: "loop" })).toEqual(["F", ";"]);
    expect(row({ type: "master" })).toEqual(["G", "H"]);
    expect(row({ type: "padMode" })).toEqual(["T", "Y"]);
  });

  it("round-trips through mixKeyAction for every deck and action", () => {
    const actions: DeckAction[] = [
      { type: "cue" },
      { type: "jump", dir: 1 },
      { type: "loop" },
      { type: "master" },
      { type: "padMode" },
    ];
    const codeOf = (name: string) => (name === ";" ? "Semicolon" : `Key${name}`);
    for (const action of actions) {
      for (let deck = 0; deck < 4; deck++) {
        const hint = deckKeyHint(action, deck) ?? "";
        const shiftKey = hint.startsWith("Shift");
        const code = codeOf(hint.replace("Shift ", ""));
        expect(mixKeyAction(key(code, { shiftKey }), 4)).toEqual({ ...action, deck });
      }
    }
  });

  it("lists the crossfader keys", () => {
    expect(crossfaderKeyHint()).toBe("Z X C");
  });
});
