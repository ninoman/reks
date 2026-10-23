import { describe, expect, it } from "vitest";

import { type DeckOccupancy, spareDeck } from "./spareDeck";

const deck = (deck: number, over: Partial<DeckOccupancy> = {}): DeckOccupancy => ({
  deck,
  empty: false,
  playing: false,
  fader: 1,
  ...over,
});

describe("spareDeck", () => {
  it("takes an empty deck", () => {
    expect(spareDeck([deck(1, { playing: true }), deck(2, { empty: true })])).toBe(2);
  });

  it("prefers an empty deck to a stopped one with its fader closed", () => {
    expect(spareDeck([deck(1, { fader: 0 }), deck(2, { empty: true })])).toBe(2);
  });

  it("takes the lowest-numbered empty deck", () => {
    expect(spareDeck([deck(3, { empty: true }), deck(0, { empty: true })])).toBe(0);
  });

  it("takes a stopped deck with its fader fully closed", () => {
    expect(spareDeck([deck(1, { playing: true }), deck(2, { fader: 0 })])).toBe(2);
  });

  it("leaves a stopped deck with its fader open", () => {
    expect(spareDeck([deck(1, { fader: 0.01 })])).toBeNull();
  });

  it("leaves a playing deck even with its fader closed", () => {
    expect(spareDeck([deck(1, { playing: true, fader: 0 })])).toBeNull();
  });

  it("finds none when every deck is busy", () => {
    expect(
      spareDeck([deck(0, { playing: true }), deck(1), deck(2, { playing: true }), deck(3)]),
    ).toBeNull();
  });
});
