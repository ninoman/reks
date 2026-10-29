import { describe, expect, it } from "vitest";

import { DEFAULT_CHANNEL, DEFAULT_MASTER } from "../store/mix";
import { hiddenPlaying, hiddenPlayingLabel, mixerDeviations } from "./mixHidden";

describe("hiddenPlaying", () => {
  const all = [true, true, true, true];

  it("lists the decks off screen for each layout", () => {
    expect(hiddenPlaying(all, true, 4)).toEqual([]);
    expect(hiddenPlaying(all, true, 2)).toEqual([0, 3]);
    expect(hiddenPlaying(all, false, 2)).toEqual([0, 2, 3]);
  });

  it("ignores decks that are stopped", () => {
    expect(hiddenPlaying([true, false, false, false], true, 2)).toEqual([0]);
    expect(hiddenPlaying([false, true, false, false], false, 4)).toEqual([]);
  });

  it("words the chip", () => {
    expect(hiddenPlayingLabel([0])).toBe("Deck 1 playing");
    expect(hiddenPlayingLabel([0, 3])).toBe("Decks 1, 4 playing");
  });
});

describe("mixerDeviations", () => {
  const reference = { channel: DEFAULT_CHANNEL, master: DEFAULT_MASTER };
  const run = (channel = DEFAULT_CHANNEL, master = DEFAULT_MASTER) =>
    mixerDeviations(channel, master, reference);

  it("is empty at the defaults", () => {
    expect(run()).toEqual([]);
  });

  it("names the fader, then the master level", () => {
    expect(run({ ...DEFAULT_CHANNEL, fader: 0.4 }, { ...DEFAULT_MASTER, level: 0.5 })).toEqual([
      "Channel 2 fader not up",
      "Master level changed",
    ]);
  });

  it("leaves out what the one-channel mixer shows", () => {
    expect(run({ ...DEFAULT_CHANNEL, eqLo: 0, trim: 0.7, fx: 0.2, filterAssign: "y" })).toEqual([]);
  });
});
