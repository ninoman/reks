import type { Channel } from "@tauri-apps/api/core";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { DeckCues, DeckLoaded, MixControl, MixState } from "../lib/ipc";
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
  it("enters an AUTO memory loop at its start; MEM while looping saves the loop", async () => {
    const p = mockMix("mix_load", { deck: 2, id: 50 }, () => true) as Promise<DeckLoaded>;
    run(300);
    const loaded = await p;
    const auto = loaded.memoryCues.find((m) => m.activeLoop);
    if (auto?.endMs == null) throw new Error("no auto loop");
    send({ type: "seek", deck: 2, ms: auto.positionMs - 200 });
    send({ type: "play", deck: 2 });
    run(400);
    expect(state().decks[2]?.loopRegion).toMatchObject({
      startMs: auto.positionMs,
      endMs: auto.endMs,
    });

    const edit = (e: unknown) =>
      mockMix("mix_cue_edit", { deck: 2, edit: e }, () => true) as DeckCues;
    send({ type: "loopExit", deck: 2 });
    send({ type: "beatLoop", deck: 2, beats: 2 });
    const saved = edit({ type: "addMemoryCue" });
    run(20);
    const region = state().decks[2]?.loopRegion;
    const loop = saved.memoryCues.find((m) => m.endMs !== null && !m.activeLoop);
    expect(loop).toMatchObject({ positionMs: region?.startMs, endMs: region?.endMs });
    const on = edit({ type: "setActiveLoop", cueId: loop?.id, on: true });
    expect(on.memoryCues.filter((m) => m.activeLoop)).toHaveLength(2);
    expect(edit({ type: "setHotCue", slot: 2 }).hotCues.map((h) => h.slot)).toEqual([0, 1, 2, 3]);
  });

  it("hands master to a playing synced deck at its tempo", () => {
    send({ type: "tempo", deck: 0, ratio: 1.06 });
    send({ type: "play", deck: 0 });
    run(100);
    send({ type: "sync", deck: 1, on: true });
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

  it("loops IN to OUT, moves its ends, and keeps it for RELOOP", () => {
    send({ type: "quantise", deck: 0, on: false });
    send({ type: "seek", deck: 0, ms: 1_000 });
    send({ type: "loopOut", deck: 0 });
    send({ type: "loopIn", deck: 0 });
    run(20);
    expect(state().decks[0]).toMatchObject({ loopIn: 1_000, loopRegion: null });
    // Paused: the loop is made and the head goes to IN.
    send({ type: "seek", deck: 0, ms: 1_005 });
    send({ type: "loopOut", deck: 0 });
    run(20);
    expect(state().decks[0]?.loopRegion).toBeNull();
    send({ type: "seek", deck: 0, ms: 2_000 });
    send({ type: "loopOut", deck: 0 });
    run(20);
    let d = state().decks[0];
    expect(d?.loopRegion).toEqual({ startMs: 1_000, endMs: 2_000, beats: 1_000 / 468.75 });
    expect(d).toMatchObject({ loopIn: null, positionMs: 1_000 });
    send({ type: "seek", deck: 0, ms: 1_500 });
    send({ type: "loopIn", deck: 0 });
    run(20);
    expect(state().decks[0]?.loopRegion).toMatchObject({ startMs: 1_500, endMs: 2_000 });

    send({ type: "reloop", deck: 0 });
    send({ type: "seek", deck: 0, ms: 9_000 });
    run(20);
    d = state().decks[0];
    expect(d?.loopRegion).toBeNull();
    expect(d?.lastLoop).toMatchObject({ startMs: 1_500, endMs: 2_000 });
    send({ type: "reloop", deck: 0 });
    run(20);
    expect(state().decks[0]).toMatchObject({ positionMs: 1_500, playing: false });
    expect(state().decks[0]?.loopRegion).toMatchObject({ startMs: 1_500 });

    // A memory loop call arms RELOOP without looping; loading clears it.
    send({ type: "callCue", deck: 0, ms: 4_000, endMs: 5_875 });
    run(20);
    d = state().decks[0];
    expect(d?.loopRegion).toBeNull();
    expect(d?.lastLoop).toEqual({ startMs: 4_000, endMs: 5_875, beats: 4 });
    mockMix("mix_load", { deck: 0, id: 1 }, () => true);
    run(20);
    expect(state().decks[0]).toMatchObject({ loopIn: null, lastLoop: null });
  });

  it("resizes a loop by time without a grid", () => {
    mockMix("mix_load", { deck: 2, id: 9 }, () => false);
    send({ type: "seek", deck: 2, ms: 1_000 });
    send({ type: "loopIn", deck: 2 });
    send({ type: "seek", deck: 2, ms: 1_600 });
    send({ type: "loopOut", deck: 2 });
    send({ type: "loopResize", deck: 2, double: false });
    run(20);
    expect(state().decks[2]?.loopRegion).toEqual({ startMs: 1_000, endMs: 1_300, beats: null });
  });

  it("gives no phase from a master looping part of a beat", () => {
    send({ type: "quantise", deck: 0, on: false });
    send({ type: "play", deck: 0 });
    run(500);
    send({ type: "loopIn", deck: 0 });
    run(700);
    send({ type: "loopOut", deck: 0 });
    send({ type: "sync", deck: 1, on: true });
    send({ type: "play", deck: 1 });
    run(17);
    const before = state().decks[1]?.positionMs ?? 0;
    run(1_000);
    // Plays straight on: no jumps into the master's loop.
    expect(state().decks[1]?.positionMs).toBeCloseTo(before + 1_000, -2);
  });
});
