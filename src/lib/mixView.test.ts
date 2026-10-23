import { describe, expect, it } from "vitest";

import type { CueRouting, DeckView, MixerEq, MixerFilters, MixerView, MixState } from "./ipc";
import {
  cueFits,
  deckSide,
  effectiveMix,
  FX_DEAD_ZONE,
  FX_KINDS,
  formatBeats,
  formatClock,
  formatLoopBeats,
  formatLoopSeconds,
  formatTempo,
  fxName,
  fxParameterName,
  JUMP_PAGES,
  LOOP_PAGES,
  LOOP_SIZE_MAX,
  LOOP_SIZE_MIN,
  loopBlockState,
  loopPadControls,
  type MixerLook,
  mixerParts,
  mixerSize,
  nextPadMode,
  nextTempoRange,
  offCentre,
  outputSummary,
  phaseOffset,
  pickSwitches,
  scrubTarget,
  shownDecks,
  snapToDetent,
  stepLoopSize,
  tempoFader,
  tempoForBpm,
  tempoRangeLabel,
  tempoRatio,
} from "./mixView";

describe("tempo fader", () => {
  it("is slower at the top and faster at the bottom", () => {
    expect(tempoRatio(0.5, 0.1)).toBe(1);
    expect(tempoRatio(1, 0.06)).toBeCloseTo(0.94);
    expect(tempoRatio(0, 0.16)).toBeCloseTo(1.16);
  });

  it("WIDE runs from standing still to double speed", () => {
    expect(tempoRatio(1, 1)).toBe(0);
    expect(tempoRatio(0, 1)).toBe(2);
    expect(formatTempo(0)).toBe("-100.00%");
  });

  it("round-trips and clamps", () => {
    expect(tempoFader(tempoRatio(0.3, 0.16), 0.16)).toBeCloseTo(0.3);
    expect(tempoFader(1.3, 0.1)).toBe(0);
  });

  it("cycles the ranges like a CDJ-3000", () => {
    const seen = [0.06, 0.1, 0.16, 1].map((r) => {
      const next = nextTempoRange(r as 0.06);
      return `${tempoRangeLabel(r as 0.06)}>${tempoRangeLabel(next)}`;
    });
    expect(seen).toEqual(["±6>±10", "±10>±16", "±16>W", "W>±6"]);
  });

  it("formats as a signed percentage", () => {
    expect(formatTempo(1.025)).toBe("+2.50%");
    expect(formatTempo(0.997)).toBe("-0.30%");
    expect(formatTempo(1.00001)).toBe("0.00%");
  });
});

describe("phaseOffset", () => {
  it("wraps into half a beat either side", () => {
    expect(phaseOffset(10.1, 3.0)).toBeCloseTo(0.1);
    expect(phaseOffset(9.9, 3.0)).toBeCloseTo(-0.1);
    expect(phaseOffset(3.5, 0)).toBeCloseTo(-0.5);
  });

  it("compares bar phase with a span of 4", () => {
    expect(phaseOffset(6, 0, 4)).toBeCloseTo(-2);
    expect(phaseOffset(5, 0, 4)).toBeCloseTo(1);
  });
});

describe("formatClock", () => {
  it("shows tenths, remaining with a minus", () => {
    expect(formatClock(83_456)).toBe("1:23.4");
    expect(formatClock(5_000, true)).toBe("-0:05.0");
    expect(formatClock(-3)).toBe("0:00.0");
  });
});

describe("outputSummary", () => {
  const live = { device: "Speakers", sampleRate: 44_100, bufferFrames: 256, xruns: 0 } as MixState;
  it("lists device, rate and buffer, and dropouts only when there are any", () => {
    expect(outputSummary(live)).toBe("Speakers · 44.1 kHz · 256 frames");
    expect(outputSummary({ ...live, bufferFrames: null, xruns: 3 })).toBe(
      "Speakers · 44.1 kHz · 3 dropouts",
    );
  });
});

describe("cueFits", () => {
  it("matches the outputs a routing needs", () => {
    const outputs: CueRouting = { type: "outputs", master: [0, 1], cue: [2, 3] };
    expect(cueFits({ type: "off" }, 1)).toBe(true);
    expect(cueFits({ type: "splitMono" }, 1)).toBe(false);
    expect(cueFits({ type: "splitMono" }, 2)).toBe(true);
    expect(cueFits(outputs, 2)).toBe(false);
    expect(cueFits(outputs, 4)).toBe(true);
  });
});

describe("pickSwitches", () => {
  const f = false;
  const t = true;
  it("a plain click picks one switch alone", () => {
    expect(pickSwitches([t, t, f], 2, false, false)).toEqual([f, f, t]);
    expect(pickSwitches([t, t, f], 0, false, false)).toEqual([t, f, f]);
  });
  it("a plain click on the only switch on turns it off, unless one must stay on", () => {
    expect(pickSwitches([f, t, f], 1, false, false)).toEqual([f, f, f]);
    expect(pickSwitches([f, t, f], 1, false, true)).toEqual([f, t, f]);
  });
  it("Shift+click adds and removes", () => {
    expect(pickSwitches([t, f, f], 2, true, false)).toEqual([t, f, t]);
    expect(pickSwitches([t, f, t], 0, true, false)).toEqual([f, f, t]);
    expect(pickSwitches([t, f, f], 0, true, false)).toEqual([f, f, f]);
    expect(pickSwitches([t, f, f], 0, true, true)).toEqual([t, f, f]);
  });
});

describe("decks on screen", () => {
  it("shows decks 2 and 3 of two, the centre channels", () => {
    expect(shownDecks(2)).toEqual([1, 2]);
    expect(shownDecks(4)).toEqual([0, 1, 2, 3]);
    expect([0, 1, 2, 3].map(deckSide)).toEqual(["left", "left", "right", "right"]);
  });
});

describe("scrubTarget", () => {
  it("overview: the point under the pointer, clamped", () => {
    const grip = { kind: "overview", widthPx: 200, durationMs: 30_000 } as const;
    expect(scrubTarget(grip, 50, 0)).toBe(7_500);
    expect(scrubTarget(grip, 260, 0)).toBe(30_000);
    expect(scrubTarget(grip, -5, 0)).toBe(0);
  });

  it("zoomed: dragging left moves forward", () => {
    const grip = { kind: "zoomed", pxPerS: 100, fromMs: 10_000, durationMs: 30_000 } as const;
    expect(scrubTarget(grip, 0, -50)).toBe(10_500);
    expect(scrubTarget(grip, 0, 200)).toBe(8_000);
    expect(scrubTarget(grip, 0, 5_000)).toBe(0);
  });
});

describe("pads", () => {
  it("cycle HOT CUE, BEAT JUMP, BEAT LOOP", () => {
    expect(nextPadMode("hotCue")).toBe("memory");
    expect(nextPadMode("memory")).toBe("beatJump");
    expect(nextPadMode("beatJump")).toBe("beatLoop");
    expect(nextPadMode("beatLoop")).toBe("hotCue");
  });

  it("page through eight sizes each, FINE first", () => {
    expect(JUMP_PAGES.map((p) => p.map(formatBeats))).toEqual([
      ["FINE", "1/8", "1/4", "1/2", "1", "2", "4", "8"],
      ["1", "2", "4", "8", "16", "32", "64", "128"],
    ]);
    expect(LOOP_PAGES.map((p) => p.map(formatBeats))).toEqual([
      ["1/8", "1/4", "1/2", "1", "2", "4", "8", "16"],
      ["1/4", "1/2", "1", "2", "4", "8", "16", "32"],
    ]);
    expect(JUMP_PAGES[0]?.[0]).toBe(1 / 32);
  });

  it("a loop pad sets, leaves or resizes the loop", () => {
    expect(loopPadControls(null, 4)).toEqual([{ type: "beatLoop", beats: 4 }]);
    expect(loopPadControls(4, 4)).toEqual([{ type: "loopExit" }]);
    expect(loopPadControls(4, 16)).toEqual([
      { type: "loopResize", double: true },
      { type: "loopResize", double: true },
    ]);
    expect(loopPadControls(1, 1 / 2)).toEqual([{ type: "loopResize", double: false }]);
  });
});

describe("snapToDetent", () => {
  it("lands on the detent within the zone only", () => {
    expect(snapToDetent(0.52, 0.5, 0.03)).toBe(0.5);
    expect(snapToDetent(0.48, 0.5, 0.03)).toBe(0.5);
    expect(snapToDetent(0.54, 0.5, 0.03)).toBe(0.54);
  });
});

describe("tempoForBpm", () => {
  it("divides the typed BPM by the track's", () => {
    const t = tempoForBpm(131.5, 128, 0.1);
    expect(t?.range).toBe(0.1);
    expect(t?.ratio).toBeCloseTo(1.02734, 4);
  });

  it("steps the range up to the smallest that fits", () => {
    expect(tempoForBpm(145, 128, 0.1)?.range).toBe(0.16);
    expect(tempoForBpm(118, 128, 0.06)?.range).toBe(0.1);
    expect(tempoForBpm(200, 128, 0.1)?.range).toBe(1);
    // A narrower range is never picked.
    expect(tempoForBpm(129, 128, 0.16)?.range).toBe(0.16);
  });

  it("rejects what no range reaches or is not a BPM", () => {
    expect(tempoForBpm(300, 128, 0.1)).toBeNull();
    expect(tempoForBpm(Number.NaN, 128, 0.1)).toBeNull();
    expect(tempoForBpm(0, 128, 0.1)).toBeNull();
    expect(tempoForBpm(128, 0, 0.1)).toBeNull();
  });
});

describe("loop size", () => {
  it("halves and doubles within 1/32 … 512", () => {
    expect(stepLoopSize(4, false)).toBe(2);
    expect(stepLoopSize(4, true)).toBe(8);
    expect(stepLoopSize(1, false)).toBe(1 / 2);
    expect(stepLoopSize(LOOP_SIZE_MIN, false)).toBe(LOOP_SIZE_MIN);
    expect(stepLoopSize(LOOP_SIZE_MAX, true)).toBe(LOOP_SIZE_MAX);
    expect(stepLoopSize(256, true)).toBe(512);
    expect(stepLoopSize(1 / 16, false)).toBe(1 / 32);
  });

  it("reads as beats, fractions, or two decimals for an unquantised manual loop", () => {
    expect([512, 64, 4, 1, 1 / 2, 1 / 16, 1 / 32].map(formatLoopBeats)).toEqual([
      "512",
      "64",
      "4",
      "1",
      "1/2",
      "1/16",
      "1/32",
    ]);
    expect(formatLoopBeats(3.8731)).toBe("3.87");
    expect(formatLoopBeats(2.5)).toBe("2.5");
    expect(formatLoopBeats(0.3)).toBe("0.3");
  });

  it("reads as seconds without a grid", () => {
    expect(formatLoopSeconds(250)).toBe("0.25s");
    expect(formatLoopSeconds(1_500)).toBe("1.5s");
    expect(formatLoopSeconds(12_345)).toBe("12.3s");
  });
});

describe("loopBlockState", () => {
  const deck = (over: Partial<DeckView> = {}) =>
    ({
      positionMs: 5_000,
      loopRegion: null,
      loopIn: null,
      lastLoop: null,
      ...over,
    }) as DeckView;
  const loop = { startMs: 1_000, endMs: 2_000, beats: 4 };

  it("nothing loaded: everything is off", () => {
    const s = loopBlockState(undefined, false, false, 4);
    expect([s.inLit, s.outEnabled, s.resizeEnabled, s.loopEnabled, s.reloopEnabled]).toEqual([
      false,
      false,
      false,
      false,
      false,
    ]);
    expect(s.sizeText).toBe("4");
    expect(s.pageDot).toBe(false);
  });

  it("no grid: IN, OUT and RELOOP work, the size button and the next size do not", () => {
    const pending = loopBlockState(deck({ loopIn: 3_000 }), true, false, 4);
    expect(pending).toMatchObject({ inLit: true, outEnabled: true, loopEnabled: false });
    // A waiting IN marks the hidden page's tab.
    expect(pending.pageDot).toBe(true);
    expect(pending.resizeEnabled).toBe(false);
    expect(loopBlockState(deck({ lastLoop: loop }), true, false, 4).reloopEnabled).toBe(true);
  });

  it("a pending IN ahead of the head does not allow OUT", () => {
    expect(loopBlockState(deck({ loopIn: 9_000 }), true, true, 4)).toMatchObject({
      inLit: true,
      outEnabled: false,
    });
  });

  it("with a grid and no loop: the size button and ½ / 2× are on, the size is the next one", () => {
    const s = loopBlockState(deck(), true, true, 1 / 8);
    expect(s).toMatchObject({ looping: false, loopEnabled: true, resizeEnabled: true });
    expect(s).toMatchObject({ reloopEnabled: false, outEnabled: false, inLit: false });
    expect(s.sizeText).toBe("1/8");
    expect(s.pageDot).toBe(false);
  });

  it("looping: the size is lit and shows the loop's own length", () => {
    const s = loopBlockState(deck({ loopRegion: { ...loop, beats: 3.87 } }), true, true, 4);
    expect(s).toMatchObject({ looping: true, outEnabled: true, reloopEnabled: true, inLit: false });
    expect(s.sizeText).toBe("3.87");
    expect(s.pageDot).toBe(true);
  });

  it("looping without a grid: seconds, and the loop can still be resized and left", () => {
    const s = loopBlockState(
      deck({ loopRegion: { startMs: 1_000, endMs: 2_250, beats: null } }),
      true,
      false,
      4,
    );
    expect(s).toMatchObject({ loopEnabled: true, resizeEnabled: true });
    expect(s.sizeText).toBe("1.25s");
  });
});

describe("effectiveMix", () => {
  const knobs = {
    eqHi: 0.1,
    eqMid1: 0.2,
    eqMid2: 0.3,
    eqLo: 0.4,
    eq3Hi: 0.6,
    eq3Mid: 0.7,
    eq3Lo: 0.8,
    fx: 0.9,
  };
  const values = {
    channels: [0, 1, 2, 3].map(() => ({
      ...knobs,
      filterAssign: "off" as const,
      xfadeAssign: "y" as const,
      fader: 0.3,
    })),
    filters: { x: { on: true, freq: 0.2 }, y: { on: true, freq: 0.8 } },
  };
  const four = { eqHi: 0.1, eqMid1: 0.2, eqMid2: 0.3, eqLo: 0.4 };
  const fourFlat = { eqHi: 0.5, eqMid1: 0.5, eqMid2: 0.5, eqLo: 0.5 };
  const three = { eq3Hi: 0.6, eq3Mid: 0.7, eq3Lo: 0.8, fx: 0.9 };
  const threeFlat = { eq3Hi: 0.5, eq3Mid: 0.5, eq3Lo: 0.5, fx: 0.5 };
  const look = (eq: MixerEq, view: MixerView, filters: MixerFilters = "show"): MixerLook => ({
    eq,
    view,
    filters,
  });
  const assigns = (e: { channels: { filterAssign: string; xfadeAssign: string }[] }) => ({
    filter: e.channels.map((c) => c.filterAssign),
    xfade: e.channels.map((c) => c.xfadeAssign),
  });

  it("4-band Advanced: as stored, the 3-band knobs at noon, whatever the filters setting", () => {
    for (const filters of ["show", "hide"] as const) {
      const e = effectiveMix(values, look("fourBand", "advanced", filters));
      for (const c of e.channels) expect(c).toMatchObject({ ...four, ...threeFlat, fader: 0.3 });
      expect(assigns(e)).toEqual({
        filter: ["off", "off", "off", "off"],
        xfade: ["y", "y", "y", "y"],
      });
      expect(e.filters).toBe(values.filters);
    }
  });

  it("4-band Simple: default filter assigns and THRU, filters as they are", () => {
    const e = effectiveMix(values, look("fourBand", "simple"));
    for (const c of e.channels) expect(c).toMatchObject({ ...four, ...threeFlat, fader: 0.3 });
    expect(assigns(e)).toEqual({
      filter: ["x", "x", "y", "y"],
      xfade: ["off", "off", "off", "off"],
    });
    expect(e.filters).toBe(values.filters);
  });

  it("4-band Simple with the filters hidden: both units off, their other values kept", () => {
    const e = effectiveMix(values, look("fourBand", "simple", "hide"));
    expect(e.filters.x).toEqual({ on: false, freq: 0.2 });
    expect(e.filters.y).toEqual({ on: false, freq: 0.8 });
  });

  it("3-band Simple: 3-band knobs as stored, 4-band EQ at noon, filters off, default assigns, THRU", () => {
    for (const filters of ["show", "hide"] as const) {
      const e = effectiveMix(values, look("threeBand", "simple", filters));
      for (const c of e.channels) expect(c).toMatchObject({ ...fourFlat, ...three, fader: 0.3 });
      expect(assigns(e)).toEqual({
        filter: ["x", "x", "y", "y"],
        xfade: ["off", "off", "off", "off"],
      });
      expect(e.filters.x).toEqual({ on: false, freq: 0.2 });
      expect(e.filters.y).toEqual({ on: false, freq: 0.8 });
    }
  });

  it("3-band Advanced: as 3-band Simple, but the stored crossfader assigns", () => {
    const e = effectiveMix(values, look("threeBand", "advanced"));
    for (const c of e.channels) expect(c).toMatchObject({ ...fourFlat, ...three, fader: 0.3 });
    expect(assigns(e)).toEqual({ filter: ["x", "x", "y", "y"], xfade: ["y", "y", "y", "y"] });
    expect(e.filters.x.on).toBe(false);
    expect(e.filters.y.on).toBe(false);
  });

  it("keeps each layout's positions: switching back sends them again", () => {
    const there = effectiveMix(values, look("threeBand", "simple"));
    const back = effectiveMix(values, look("fourBand", "simple"));
    expect(there.channels[0]).toMatchObject(three);
    expect(back.channels[0]).toMatchObject(four);
  });

  it("leaves the stored values alone", () => {
    effectiveMix(values, look("threeBand", "simple", "hide"));
    expect(values.channels[0]).toMatchObject({ ...knobs, xfadeAssign: "y", filterAssign: "off" });
    expect(values.filters.x.on).toBe(true);
  });
});

describe("mixer parts and size", () => {
  it("4-band: assign rows and crossfader in Advanced only; the filter row unless hidden in Simple", () => {
    expect(mixerParts({ eq: "fourBand", view: "advanced", filters: "hide" })).toEqual({
      threeBand: false,
      filterAssigns: true,
      crossfader: true,
      filterRow: true,
    });
    expect(mixerParts({ eq: "fourBand", view: "simple", filters: "show" })).toEqual({
      threeBand: false,
      filterAssigns: false,
      crossfader: false,
      filterRow: true,
    });
    expect(mixerParts({ eq: "fourBand", view: "simple", filters: "hide" })).toEqual({
      threeBand: false,
      filterAssigns: false,
      crossfader: false,
      filterRow: false,
    });
  });

  it("3-band: no filter row or FILTER assigns; Advanced adds the crossfader", () => {
    for (const filters of ["show", "hide"] as const) {
      expect(mixerParts({ eq: "threeBand", view: "simple", filters })).toEqual({
        threeBand: true,
        filterAssigns: false,
        crossfader: false,
        filterRow: false,
      });
      expect(mixerParts({ eq: "threeBand", view: "advanced", filters })).toEqual({
        threeBand: true,
        filterAssigns: false,
        crossfader: true,
        filterRow: false,
      });
    }
  });

  it("names the height class", () => {
    expect(mixerSize({ eq: "fourBand", view: "advanced", filters: "hide" })).toBe("advanced");
    expect(mixerSize({ eq: "fourBand", view: "simple", filters: "show" })).toBe("simple");
    expect(mixerSize({ eq: "fourBand", view: "simple", filters: "hide" })).toBe("simple-bare");
    expect(mixerSize({ eq: "threeBand", view: "simple", filters: "hide" })).toBe("three");
    expect(mixerSize({ eq: "threeBand", view: "advanced", filters: "show" })).toBe(
      "three-advanced",
    );
  });
});

describe("channel FX labels and knob light", () => {
  it("names each effect and what PARAMETER sets for it", () => {
    expect(FX_KINDS.map(fxName)).toEqual(["Filter", "Echo", "Reverb"]);
    expect(FX_KINDS.map(fxParameterName)).toEqual(["Resonance", "Feedback", "Decay"]);
  });

  it("lights a knob off centre; the FX knob only outside its dead zone", () => {
    expect(offCentre(0.5)).toBe(false);
    expect(offCentre(0.51)).toBe(true);
    expect(offCentre(0.49, FX_DEAD_ZONE)).toBe(false);
    expect(offCentre(0.515, FX_DEAD_ZONE)).toBe(false);
    expect(offCentre(0.52, FX_DEAD_ZONE)).toBe(true);
    expect(offCentre(0.48, FX_DEAD_ZONE)).toBe(true);
  });
});
