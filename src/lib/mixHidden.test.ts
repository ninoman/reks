import { describe, expect, it } from "vitest";

import { DEFAULT_CHANNEL, DEFAULT_MASTER, type FilterValues } from "../store/mix";
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
  const filter: FilterValues = {
    freq: 0.5,
    resonance: 0,
    highPass: false,
    bandPass: false,
    lowPass: true,
    on: false,
  };
  const filters = { x: filter, y: filter };
  const reference = { channel: DEFAULT_CHANNEL, master: DEFAULT_MASTER };
  const run = (channel = DEFAULT_CHANNEL, master = DEFAULT_MASTER, f = filters) =>
    mixerDeviations(channel, f, master, reference);

  it("is empty at the defaults, whatever the filter assign", () => {
    expect(run({ ...DEFAULT_CHANNEL, filterAssign: "x" })).toEqual([]);
  });

  it("names what is off, the fader first", () => {
    expect(run({ ...DEFAULT_CHANNEL, eqLo: 0, fader: 0.4 })).toEqual([
      "Channel 2 fader not up",
      "Channel 2 EQ not flat",
    ]);
    expect(run({ ...DEFAULT_CHANNEL, trim: 0.7 })).toEqual(["Channel 2 trim not centred"]);
    expect(run(DEFAULT_CHANNEL, { ...DEFAULT_MASTER, level: 0.5 })).toEqual([
      "Master level changed",
    ]);
  });

  it("counts the 3-band EQ and the FX knob outside its dead zone", () => {
    expect(run({ ...DEFAULT_CHANNEL, eq3Mid: 0.3 })).toEqual(["Channel 2 EQ not flat"]);
    expect(run({ ...DEFAULT_CHANNEL, fx: 0.51 })).toEqual([]);
    expect(run({ ...DEFAULT_CHANNEL, fx: 0.2 })).toEqual(["Channel 2 FX on"]);
  });

  it("counts the filter only when it is assigned and on", () => {
    const on = { x: { ...filter, on: true }, y: filter };
    expect(run({ ...DEFAULT_CHANNEL, filterAssign: "x" }, DEFAULT_MASTER, on)).toEqual([
      "Channel 2 filter on",
    ]);
    expect(run({ ...DEFAULT_CHANNEL, filterAssign: "y" }, DEFAULT_MASTER, on)).toEqual([]);
    expect(run({ ...DEFAULT_CHANNEL, filterAssign: "off" }, DEFAULT_MASTER, on)).toEqual([]);
  });
});
