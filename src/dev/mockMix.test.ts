import type { Channel } from "@tauri-apps/api/core";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { MixControl, MixState } from "../lib/ipc";
import { mockMix } from "./mockMix";

let last: MixState | null = null;
const send = (control: MixControl) => mockMix("mix_control", { control }, () => true);
const run = (ms: number) => vi.advanceTimersByTime(ms);
const state = () => {
  if (last === null) throw new Error("no state yet");
  return last;
};

beforeEach(() => {
  vi.useFakeTimers();
  const onState = { onmessage: (s: MixState) => (last = s) } as unknown as Channel<MixState>;
  mockMix("mix_start", { config: { device: null, cue: { type: "off" } }, onState }, () => true);
  for (const deck of [0, 1]) mockMix("mix_load", { deck, id: deck + 1 }, () => true);
});

afterEach(() => {
  mockMix("mix_stop", {}, () => true);
  vi.useRealTimers();
});

describe("mock mixer", () => {
  it("hands master to a playing synced deck at its tempo", () => {
    send({ type: "tempo", deck: 0, ratio: 1.06 });
    send({ type: "play", deck: 0 });
    run(100);
    send({ type: "sync", deck: 1, on: true, bar: false });
    send({ type: "play", deck: 1 });
    run(100);
    expect(state().masterDeck).toBe(0);
    send({ type: "pause", deck: 0 });
    run(100);
    const s = state();
    expect(s.masterDeck).toBe(1);
    expect(s.decks[1]?.master).toBe(true);
    expect(s.decks[1]?.speed).toBeCloseTo(1.06);
    expect(s.decks[1]?.tempo).toBeCloseTo(1.06);
    send({ type: "pause", deck: 1 });
    run(100);
    expect(state().masterDeck).toBeNull();
  });

  it("loops, jumps the loop with the head, and exits", () => {
    send({ type: "play", deck: 0 });
    run(1000);
    send({ type: "beatLoop", deck: 0, beats: 1 });
    run(2000);
    const loop = state().decks[0]?.loopRegion;
    expect(loop?.beats).toBe(1);
    const pos = state().decks[0]?.positionMs ?? 0;
    expect(pos).toBeGreaterThanOrEqual(loop?.startMs ?? 0);
    expect(pos).toBeLessThan(loop?.endMs ?? 0);
    send({ type: "beatJump", deck: 0, beats: 4 });
    run(20);
    expect(state().decks[0]?.loopRegion?.startMs).toBeCloseTo((loop?.startMs ?? 0) + 4 * 468.75);
    send({ type: "loopResize", deck: 0, double: false });
    run(20);
    expect(state().decks[0]?.loopRegion?.beats).toBe(0.5);
    send({ type: "loopExit", deck: 0 });
    run(20);
    expect(state().decks[0]?.loopRegion).toBeNull();
  });
});
