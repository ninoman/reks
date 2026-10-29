import { describe, expect, it } from "vitest";

import {
  barBeatAt,
  formatBarBeat,
  formatOffset,
  gridKey,
  pivotMs,
  removableMarker,
} from "./gridMode";
import type { BeatMark, GridAnchor } from "./ipc";

/** 120 BPM from 250 ms, beat 4 first: downbeats at 750, 2750, … */
const beats: BeatMark[] = Array.from({ length: 12 }, (_, n) => ({
  positionUs: (250 + n * 500) * 1000,
  beatInBar: ((3 + n) % 4) + 1,
}));

const anchor = (positionMs: number, bpmX100 = 12000): GridAnchor => ({
  positionMs,
  bpmX100,
  beatInBar: 1,
  beatsPerBar: 4,
});

const key = (k: Partial<Parameters<typeof gridKey>[0]>) =>
  gridKey({
    key: "",
    code: "",
    repeat: false,
    shiftKey: false,
    altKey: false,
    metaKey: false,
    ctrlKey: false,
    ...k,
  });

describe("barBeatAt", () => {
  it("names the nearest beat and the offset from it", () => {
    expect(barBeatAt(beats, 760)).toEqual({ bar: 1, beat: 1, offsetMs: 10 });
    expect(barBeatAt(beats, 2_740)).toEqual({ bar: 2, beat: 1, offsetMs: -10 });
    expect(barBeatAt(beats, 1_300)).toEqual({ bar: 1, beat: 2, offsetMs: 50 });
  });

  it("counts bar 0 before the first downbeat and nothing without beats", () => {
    expect(barBeatAt(beats, 0)).toEqual({ bar: 0, beat: 4, offsetMs: -250 });
    expect(barBeatAt([], 100)).toBeNull();
  });

  it("formats", () => {
    expect(formatBarBeat({ bar: 18, beat: 1, offsetMs: 0 })).toBe("18.1");
    expect(formatOffset(6)).toBe("+6 ms");
    expect(formatOffset(-3)).toBe("−3 ms");
    expect(formatOffset(0)).toBe("0 ms");
  });
});

describe("pivotMs", () => {
  const two = [anchor(0), anchor(60_000, 17400)];

  it("uses the pin in its own section", () => {
    expect(pivotMs(two, 8_000, 20_000)).toBe(8_000);
  });

  it("falls back to the section's marker", () => {
    expect(pivotMs(two, 8_000, 61_000)).toBe(60_000);
    expect(pivotMs(two, null, 20_000)).toBe(0);
  });
});

it("removableMarker finds a later marker within a quarter beat", () => {
  const g = [anchor(0), anchor(10_000), anchor(20_000)];
  expect(removableMarker(g, 10_100)).toBe(1);
  expect(removableMarker(g, 10_200)).toBeNull();
  // Never the first marker.
  expect(removableMarker(g, 0)).toBeNull();
});

describe("gridKey", () => {
  it("shifts and retempos with arrows; Shift or a repeat takes the big step", () => {
    expect(key({ key: "ArrowLeft" })).toEqual({
      type: "shift",
      dir: -1,
      phase: "press",
      big: false,
    });
    expect(key({ key: "ArrowRight", shiftKey: true })).toEqual({
      type: "shift",
      dir: 1,
      phase: "press",
      big: true,
    });
    expect(key({ key: "ArrowUp", repeat: true })).toEqual({
      type: "bpm",
      dir: 1,
      phase: "repeat",
      big: true,
    });
  });

  it("maps letters by code, Alt places exactly", () => {
    expect(key({ code: "KeyB" })).toEqual({ type: "beatOne", exact: false });
    expect(key({ code: "KeyS", altKey: true })).toEqual({ type: "stretch", exact: true });
    expect(key({ code: "KeyM" })).toEqual({ type: "marker", exact: false });
    expect(key({ code: "KeyM", repeat: true })).toBeNull();
  });

  it("has undo, redo, halve, double and exit, and leaves other keys", () => {
    expect(key({ code: "KeyZ", metaKey: true })).toEqual({ type: "undo" });
    expect(key({ code: "KeyZ", metaKey: true, shiftKey: true })).toEqual({ type: "redo" });
    expect(key({ key: "/" })).toEqual({ type: "halve" });
    expect(key({ key: "*", shiftKey: true })).toEqual({ type: "double" });
    expect(key({ key: "Escape" })).toEqual({ type: "exit" });
    expect(key({ code: "KeyQ" })).toBeNull();
    expect(key({ key: "ArrowLeft", altKey: true })).toBeNull();
  });
});
