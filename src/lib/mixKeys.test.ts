import { describe, expect, it } from "vitest";

import { crossfaderKeyHint, deckKeyHint, type KeyPress, mixKeyAction } from "./mixKeys";

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
    expect(mixKeyAction(key("KeyE"), 2)).toEqual({ type: "zoom", dir: -1, deck: 1 });
    expect(mixKeyAction(key("KeyP"), 2)).toEqual({ type: "zoom", dir: 1, deck: 2 });
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
    expect(mixKeyAction(key("KeyT"), 2)).toBeNull();
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
  });

  it("round-trips through mixKeyAction for every deck", () => {
    for (let deck = 0; deck < 4; deck++) {
      const hint = deckKeyHint({ type: "cue" }, deck) ?? "";
      const code = `Key${hint.replace("Shift ", "")}`;
      const shiftKey = hint.startsWith("Shift");
      expect(mixKeyAction(key(code, { shiftKey }), 4)).toEqual({ type: "cue", deck });
    }
  });

  it("lists the crossfader keys", () => {
    expect(crossfaderKeyHint()).toBe("Z X C");
  });
});
