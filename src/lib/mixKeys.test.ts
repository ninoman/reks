import { describe, expect, it } from "vitest";

import {
  crossfaderKeyHint,
  type DeckAction,
  deckKeyHint,
  deckKeysHelp,
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

describe("mixKeyAction with one deck (the player)", () => {
  it("plays the left-hand keys on deck 2", () => {
    expect(mixKeyAction(key("KeyZ"), 1)).toEqual({ type: "play", deck: 1 });
    expect(mixKeyAction(key("KeyA"), 1)).toEqual({ type: "cue", deck: 1 });
    expect(mixKeyAction(key("Digit3"), 1)).toEqual({ type: "hotCue", slot: 2, deck: 1 });
    expect(mixKeyAction(key("KeyF", { shiftKey: true }), 1)).toEqual({ type: "loop", deck: 1 });
    expect(mixKeyAction(key("KeyR", { metaKey: true }), 1)).toEqual({
      type: "jump",
      dir: -1,
      deck: 1,
    });
  });

  it("ignores the right hand, Option, sync, master and the crossfader", () => {
    expect(mixKeyAction(key("KeyN"), 1)).toBeNull();
    expect(mixKeyAction(key("Digit8"), 1)).toBeNull();
    expect(mixKeyAction(key("KeyA", { altKey: true }), 1)).toBeNull();
    expect(mixKeyAction(key("KeyQ"), 1)).toBeNull();
    expect(mixKeyAction(key("KeyQ", { shiftKey: true }), 1)).toBeNull();
    expect(mixKeyAction(key("KeyX"), 1)).toBeNull();
  });
});

describe("mixKeyAction", () => {
  it("maps each hand to its deck, as rekordbox's default keys", () => {
    expect(mixKeyAction(key("KeyA"), 2)).toEqual({ type: "cue", deck: 1 });
    expect(mixKeyAction(key("KeyH"), 2)).toEqual({ type: "cue", deck: 2 });
    expect(mixKeyAction(key("KeyZ"), 2)).toEqual({ type: "play", deck: 1 });
    expect(mixKeyAction(key("KeyN"), 2)).toEqual({ type: "play", deck: 2 });
    expect(mixKeyAction(key("KeyQ"), 2)).toEqual({ type: "sync", deck: 1 });
    expect(mixKeyAction(key("KeyU"), 2)).toEqual({ type: "nudge", dir: -1, deck: 2 });
    expect(mixKeyAction(key("Digit8"), 2)).toEqual({ type: "hotCue", slot: 2, deck: 2 });
    expect(mixKeyAction(key("KeyR"), 2)).toEqual({ type: "callCue", dir: -1, deck: 1 });
    expect(mixKeyAction(key("KeyP"), 2)).toEqual({ type: "callCue", dir: 1, deck: 2 });
    expect(mixKeyAction(key("Digit0"), 2)).toBeNull();
    expect(mixKeyAction(key("KeyS"), 2)).toBeNull();
  });

  it("Shift is each key's second function", () => {
    const shift = { shiftKey: true };
    expect(mixKeyAction(key("KeyQ", shift), 2)).toEqual({ type: "master", deck: 1 });
    expect(mixKeyAction(key("KeyY", shift), 2)).toEqual({ type: "master", deck: 2 });
    expect(mixKeyAction(key("KeyT", shift), 2)).toEqual({ type: "memoryCue", deck: 1 });
    expect(mixKeyAction(key("KeyP", shift), 2)).toEqual({ type: "memoryCue", deck: 2 });
    expect(mixKeyAction(key("KeyF", shift), 2)).toEqual({ type: "loop", deck: 1 });
    expect(mixKeyAction(key("KeyL", shift), 2)).toEqual({ type: "loop", deck: 2 });
    expect(mixKeyAction(key("KeyS", shift), 2)).toEqual({ type: "padMode", deck: 1 });
    expect(mixKeyAction(key("KeyJ", shift), 2)).toEqual({ type: "padMode", deck: 2 });
    expect(mixKeyAction(key("KeyA", shift), 2)).toBeNull();
    expect(mixKeyAction(key("KeyX", shift), 2)).toBeNull();
  });

  it("D F / K L set loop IN and OUT, G / ; reloop, Shift D G / Shift K ; halve and double", () => {
    expect(mixKeyAction(key("KeyD"), 2)).toEqual({ type: "loopIn", deck: 1 });
    expect(mixKeyAction(key("KeyK"), 2)).toEqual({ type: "loopIn", deck: 2 });
    expect(mixKeyAction(key("KeyF"), 2)).toEqual({ type: "loopOut", deck: 1 });
    expect(mixKeyAction(key("KeyL"), 2)).toEqual({ type: "loopOut", deck: 2 });
    expect(mixKeyAction(key("KeyG"), 2)).toEqual({ type: "reloop", deck: 1 });
    expect(mixKeyAction(key("Semicolon"), 2)).toEqual({ type: "reloop", deck: 2 });
    const shift = { shiftKey: true };
    expect(mixKeyAction(key("KeyD", shift), 2)).toEqual({
      type: "loopResize",
      double: false,
      deck: 1,
    });
    expect(mixKeyAction(key("KeyG", shift), 2)).toEqual({
      type: "loopResize",
      double: true,
      deck: 1,
    });
    expect(mixKeyAction(key("KeyK", shift), 2)).toEqual({
      type: "loopResize",
      double: false,
      deck: 2,
    });
    expect(mixKeyAction(key("Semicolon", shift), 2)).toEqual({
      type: "loopResize",
      double: true,
      deck: 2,
    });
    // Shift F stays LOOP on/off; ⌘ D is no loop key.
    expect(mixKeyAction(key("KeyF", shift), 2)).toEqual({ type: "loop", deck: 1 });
    expect(mixKeyAction(key("KeyD", { metaKey: true }), 2)).toBeNull();
  });

  it("Option moves the loop keys to decks 1 and 4", () => {
    expect(mixKeyAction(key("KeyD", { altKey: true }), 4)).toEqual({ type: "loopIn", deck: 0 });
    expect(mixKeyAction(key("Semicolon", { altKey: true }), 4)).toEqual({
      type: "reloop",
      deck: 3,
    });
    expect(mixKeyAction(key("KeyK", { altKey: true, shiftKey: true }), 4)).toEqual({
      type: "loopResize",
      double: false,
      deck: 3,
    });
    expect(mixKeyAction(key("KeyD", { altKey: true }), 2)).toBeNull();
  });

  it("⌘ or Ctrl on the memory cue keys beat jumps", () => {
    expect(mixKeyAction(key("KeyR", { metaKey: true }), 2)).toEqual({
      type: "jump",
      dir: -1,
      deck: 1,
    });
    expect(mixKeyAction(key("KeyP", { ctrlKey: true }), 2)).toEqual({
      type: "jump",
      dir: 1,
      deck: 2,
    });
    expect(mixKeyAction(key("KeyZ", { metaKey: true }), 2)).toBeNull();
    expect(mixKeyAction(key("KeyR", { metaKey: true, shiftKey: true }), 2)).toBeNull();
  });

  it("Option reaches the outer decks 1 and 4 only when four are shown", () => {
    const alt = { altKey: true };
    expect(mixKeyAction(key("KeyA", alt), 4)).toEqual({ type: "cue", deck: 0 });
    expect(mixKeyAction(key("KeyY", alt), 4)).toEqual({ type: "sync", deck: 3 });
    expect(mixKeyAction(key("KeyO", { ...alt, metaKey: true }), 4)).toEqual({
      type: "jump",
      dir: -1,
      deck: 3,
    });
    expect(mixKeyAction(key("KeyT", { ...alt, shiftKey: true }), 4)).toEqual({
      type: "memoryCue",
      deck: 0,
    });
    expect(mixKeyAction(key("KeyA", alt), 2)).toBeNull();
  });

  it("moves the crossfader and leaves other keys alone", () => {
    expect(mixKeyAction(key("KeyX"), 2)).toEqual({ type: "crossfader", step: -1 });
    expect(mixKeyAction(key("KeyC"), 2)).toEqual({ type: "crossfader", step: 0 });
    expect(mixKeyAction(key("KeyV"), 2)).toEqual({ type: "crossfader", step: 1 });
    expect(mixKeyAction(key("KeyX", { altKey: true }), 4)).toBeNull();
    expect(mixKeyAction(key("KeyC", { metaKey: true }), 2)).toBeNull();
    expect(mixKeyAction(key("Backquote"), 2)).toBeNull();
    // Zoom is not a deck key: keys.ts handles it everywhere.
    expect(mixKeyAction(key("Minus"), 2)).toBeNull();
  });
});

describe("key hints", () => {
  it("name the key that mixKeyAction maps to the same action", () => {
    expect(deckKeyHint({ type: "play" }, 1)).toBe("Z");
    expect(deckKeyHint({ type: "play" }, 2)).toBe("N");
    expect(deckKeyHint({ type: "play" }, 0)).toBe("⌥ Z");
    expect(deckKeyHint({ type: "hotCue", slot: 2 }, 3)).toBe("⌥ 8");
    expect(deckKeyHint({ type: "hotCue", slot: 4 }, 1)).toBeUndefined();
    expect(deckKeyHint({ type: "padMode" }, 3)).toBe("⌥ Shift J");
    expect(deckKeyHint({ type: "jump", dir: 1 }, 0)).toBe("⌥ ⌘ T");
  });

  it("match the table in docs/ui.md", () => {
    const row = (action: DeckAction) => [deckKeyHint(action, 1), deckKeyHint(action, 2)];
    expect(row({ type: "cue" })).toEqual(["A", "H"]);
    expect(row({ type: "play" })).toEqual(["Z", "N"]);
    expect(row({ type: "sync" })).toEqual(["Q", "Y"]);
    expect(row({ type: "master" })).toEqual(["Shift Q", "Shift Y"]);
    expect(row({ type: "nudge", dir: -1 })).toEqual(["W", "U"]);
    expect(row({ type: "nudge", dir: 1 })).toEqual(["E", "I"]);
    expect(row({ type: "hotCue", slot: 0 })).toEqual(["1", "6"]);
    expect(row({ type: "hotCue", slot: 3 })).toEqual(["4", "9"]);
    expect(row({ type: "callCue", dir: -1 })).toEqual(["R", "O"]);
    expect(row({ type: "callCue", dir: 1 })).toEqual(["T", "P"]);
    expect(row({ type: "memoryCue" })).toEqual(["Shift T", "Shift P"]);
    expect(row({ type: "jump", dir: -1 })).toEqual(["⌘ R", "⌘ O"]);
    expect(row({ type: "jump", dir: 1 })).toEqual(["⌘ T", "⌘ P"]);
    expect(row({ type: "loop" })).toEqual(["Shift F", "Shift L"]);
    expect(row({ type: "padMode" })).toEqual(["Shift S", "Shift J"]);
    expect(row({ type: "loopIn" })).toEqual(["D", "K"]);
    expect(row({ type: "loopOut" })).toEqual(["F", "L"]);
    expect(row({ type: "reloop" })).toEqual(["G", ";"]);
    expect(row({ type: "loopResize", double: false })).toEqual(["Shift D", "Shift K"]);
    expect(row({ type: "loopResize", double: true })).toEqual(["Shift G", "Shift ;"]);
    expect(deckKeyHint({ type: "reloop" }, 3)).toBe("⌥ ;");
  });

  it("read as one line for the shortcut list", () => {
    expect(deckKeysHelp([{ type: "play" }])).toBe("Z / N");
    expect(deckKeysHelp([{ type: "loopIn" }, { type: "loopOut" }])).toBe("D F / K L");
    expect(
      deckKeysHelp([
        { type: "loopResize", double: false },
        { type: "loopResize", double: true },
      ]),
    ).toBe("Shift D G / Shift K ;");
    expect(deckKeysHelp([{ type: "memoryCue" }])).toBe("Shift T / Shift P");
    expect(
      deckKeysHelp(
        [
          { type: "hotCue", slot: 0 },
          { type: "hotCue", slot: 3 },
        ],
        true,
      ),
    ).toBe("1–4 / 6–9");
  });

  it("round-trips through mixKeyAction for every deck and action", () => {
    const actions: DeckAction[] = [
      { type: "cue" },
      { type: "play" },
      { type: "sync" },
      { type: "master" },
      { type: "nudge", dir: 1 },
      { type: "hotCue", slot: 3 },
      { type: "jump", dir: -1 },
      { type: "jump", dir: 1 },
      { type: "loop" },
      { type: "loopIn" },
      { type: "loopOut" },
      { type: "reloop" },
      { type: "loopResize", double: false },
      { type: "loopResize", double: true },
      { type: "padMode" },
      { type: "memoryCue" },
      { type: "callCue", dir: 1 },
    ];
    for (const action of actions) {
      for (let deck = 0; deck < 4; deck++) {
        const parts = (deckKeyHint(action, deck) ?? "").split(" ");
        const name = parts.at(-1) ?? "";
        const code = name === ";" ? "Semicolon" : /\d/.test(name) ? `Digit${name}` : `Key${name}`;
        const press = key(code, {
          altKey: parts.includes("⌥"),
          shiftKey: parts.includes("Shift"),
          metaKey: parts.includes("⌘"),
        });
        expect(mixKeyAction(press, 4)).toEqual({ ...action, deck });
      }
    }
  });

  it("lists the crossfader keys", () => {
    expect(crossfaderKeyHint()).toBe("X C V");
  });
});

describe("mixKeyAction in the Simple mixer view", () => {
  it("does nothing on X C V, and keeps the other keys", () => {
    for (const code of ["KeyX", "KeyC", "KeyV"]) {
      expect(mixKeyAction(key(code), 2, true)).not.toBeNull();
      expect(mixKeyAction(key(code), 2, false)).toBeNull();
    }
    expect(mixKeyAction(key("KeyZ"), 2, false)).toEqual({ type: "play", deck: 1 });
  });
});
