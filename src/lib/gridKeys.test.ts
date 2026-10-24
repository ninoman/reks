// @vitest-environment jsdom
import { describe, expect, it } from "vitest";

import { gridKey } from "./gridKeys";

type Mods = Partial<{
  shiftKey: boolean;
  altKey: boolean;
  metaKey: boolean;
  ctrlKey: boolean;
  target: EventTarget | null;
}>;

const key = (key: string, code: string, mods: Mods = {}) => ({
  key,
  code,
  shiftKey: false,
  altKey: false,
  metaKey: false,
  ctrlKey: false,
  target: null,
  ...mods,
});

describe("gridKey", () => {
  it("maps the editor's shortcuts", () => {
    expect(gridKey(key("ArrowRight", "ArrowRight"))).toEqual({ type: "nudge", deltaMs: 1 });
    expect(gridKey(key("ArrowLeft", "ArrowLeft", { shiftKey: true }))).toEqual({
      type: "nudge",
      deltaMs: -10,
    });
    expect(gridKey(key("ArrowRight", "ArrowRight", { altKey: true }))).toEqual({
      type: "moveMarker",
      deltaMs: 1,
    });
    expect(gridKey(key("m", "KeyM"))).toEqual({ type: "addMarker" });
    expect(gridKey(key("Delete", "Delete"))).toEqual({ type: "deleteMarker" });
    expect(gridKey(key("d", "KeyD"))).toEqual({ type: "setDownbeat" });
    expect(gridKey(key("t", "KeyT"))).toEqual({ type: "tap" });
    expect(gridKey(key("/", "Slash"))).toEqual({ type: "halve" });
    expect(gridKey(key("*", "Digit8", { shiftKey: true }))).toEqual({ type: "double" });
    expect(gridKey(key("]", "BracketRight"))).toEqual({ type: "select", step: 1 });
    expect(gridKey(key("-", "Minus"))).toEqual({ type: "zoom", step: -1 });
    expect(gridKey(key("s", "KeyS"))).toEqual({ type: "snap" });
    expect(gridKey(key("z", "KeyZ", { metaKey: true }))).toEqual({ type: "undo" });
    expect(gridKey(key("z", "KeyZ", { ctrlKey: true, shiftKey: true }))).toEqual({
      type: "redo",
    });
    expect(gridKey(key("y", "KeyY", { ctrlKey: true }))).toEqual({ type: "redo" });
    expect(gridKey(key("q", "KeyQ"))).toBeNull();
    expect(gridKey(key("m", "KeyM", { metaKey: true }))).toBeNull();
  });

  it("leaves text fields alone", () => {
    const input = document.createElement("input");
    expect(gridKey(key("ArrowLeft", "ArrowLeft", { target: input }))).toBeNull();
    expect(gridKey(key("z", "KeyZ", { metaKey: true, target: input }))).toBeNull();
    const button = document.createElement("button");
    expect(gridKey(key("m", "KeyM", { target: button }))).toEqual({ type: "addMarker" });
  });
});
