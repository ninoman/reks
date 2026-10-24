import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import * as ipc from "../lib/ipc";
import { clearOverviews, overviewOf, watchOverview } from "./overviews";

vi.mock("../lib/ipc", () => ({ waveformOverview: vi.fn() }));

/** Pending `waveformOverview` calls, answered by the test. */
let calls: { id: number; decode: boolean; done: boolean; resolve: (bytes: number[]) => void }[] =
  [];

beforeEach(() => {
  calls = [];
  vi.mocked(ipc.waveformOverview)
    .mockReset()
    .mockImplementation(
      (id, _width, decode) =>
        new Promise((resolve) => {
          const call = {
            id,
            decode,
            done: false,
            resolve: (b: number[]) => {
              call.done = true;
              resolve(new Uint8Array(b).buffer);
            },
          };
          calls.push(call);
        }),
    );
});
// The store's lanes are module state: answer every request before the next test.
afterEach(async () => {
  for (let c = calls.find((x) => !x.done); c !== undefined; c = calls.find((x) => !x.done)) {
    c.resolve([0, 0, 0, 0]);
    await flush();
  }
  clearOverviews();
});

const flush = () => new Promise((r) => setTimeout(r, 0));

describe("overviews", () => {
  it("reads stored columns four at a time, newest first, and decodes misses one at a time", async () => {
    const stops = [1, 2, 3, 4, 5, 6].map((id) => watchOverview(id, () => {}));
    expect(calls.map((c) => [c.id, c.decode])).toEqual([
      [1, false],
      [2, false],
      [3, false],
      [4, false],
    ]);
    // Track 4 has nothing on disk: the decode lane takes it, a fast lane frees up for 6.
    calls[3]?.resolve([]);
    await flush();
    expect(calls.slice(4).map((c) => [c.id, c.decode])).toEqual([
      [6, false],
      [4, true],
    ]);
    calls[4]?.resolve([1, 2, 3, 4]);
    calls[5]?.resolve([5, 6, 7, 8]);
    await flush();
    expect(overviewOf(6)).toEqual(new Uint8Array([1, 2, 3, 4]));
    expect(overviewOf(4)).toEqual(new Uint8Array([5, 6, 7, 8]));
    expect(calls.slice(6).map((c) => [c.id, c.decode])).toEqual([[5, false]]);
    for (const stop of stops) stop();
  });

  it("keeps answers for rows that left and skips rows that left before their turn", async () => {
    const stops = [1, 2, 3, 4, 5].map((id) => watchOverview(id, () => {}));
    stops[0]?.();
    stops[4]?.();
    calls[0]?.resolve([1, 1, 1, 1]);
    await flush();
    expect(overviewOf(1)).toEqual(new Uint8Array([1, 1, 1, 1]));
    expect(calls.map((c) => c.id)).toEqual([1, 2, 3, 4]);
    for (const stop of stops) stop();
  });
});
