import { describe, expect, it, vi } from "vitest";
import type { MixStore } from "../../store/mix";
import { deckActions } from "./deckActions";

function mixWith(playing: boolean | undefined) {
  const load = vi.fn().mockResolvedValue(undefined);
  const live = playing === undefined ? null : { decks: [{ playing }] };
  return { mix: { load, live } as unknown as MixStore, load };
}

describe("deckActions drop", () => {
  it("plays the new track when the deck was playing", () => {
    const { mix, load } = mixWith(true);
    deckActions(mix, 0).drop(7);
    expect(load).toHaveBeenCalledWith(0, 7, true);
  });

  it("keeps a paused or empty deck stopped", () => {
    for (const playing of [false, undefined]) {
      const { mix, load } = mixWith(playing);
      deckActions(mix, 0).drop(7);
      expect(load).toHaveBeenCalledWith(0, 7, false);
    }
  });
});
