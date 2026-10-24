// @vitest-environment jsdom
import { beforeEach, describe, expect, it } from "vitest";
import { DEFAULT_DECK_ZOOM } from "./deckScroll";
import { loadDeckZoom, saveDeckZoom } from "./deckZoomMemory";

describe("deck zoom memory", () => {
  beforeEach(() => localStorage.clear());

  it("defaults to 8 s when nothing is saved", () => {
    expect(loadDeckZoom()).toBe(DEFAULT_DECK_ZOOM);
  });

  it("returns the saved zoom", () => {
    saveDeckZoom(5);
    expect(loadDeckZoom()).toBe(5);
  });

  it("ignores a value out of range", () => {
    localStorage.setItem("reks.deckZoom", "99");
    expect(loadDeckZoom()).toBe(DEFAULT_DECK_ZOOM);
  });
});
