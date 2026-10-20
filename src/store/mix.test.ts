import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { MixState } from "../lib/ipc";

vi.mock("../lib/ipc", () => ({
  DECKS: 4,
  playerDevices: vi.fn(),
  mixDeviceOptions: vi.fn(),
  mixStart: vi.fn(),
  mixStop: vi.fn(),
  mixLoad: vi.fn(),
  mixControl: vi.fn(),
  libraryTrack: vi.fn(),
  waveformColumns: vi.fn(),
  beatgridGet: vi.fn(),
}));

const ipc = await import("../lib/ipc");
const { useMix } = await import("./mix");

const running: MixState = {
  device: "Out",
  sampleRate: 44_100,
  channels: 2,
  bufferFrames: null,
  deviceLost: false,
  xruns: 0,
  leader: null,
  leaderChosen: false,
  decks: [],
};

const initial = useMix.getState();

beforeEach(() => {
  vi.mocked(ipc.mixStart).mockResolvedValue(running);
  vi.mocked(ipc.mixControl).mockResolvedValue(undefined);
  vi.mocked(ipc.waveformColumns).mockResolvedValue(new Uint8Array([1, 2, 3, 4]).buffer);
  vi.mocked(ipc.beatgridGet).mockResolvedValue({
    anchors: [],
    source: null,
    locked: false,
    beats: [{ positionUs: 0, beatInBar: 1 }],
  });
});

afterEach(() => {
  vi.clearAllMocks();
  useMix.setState(initial, true);
});

describe("mix store", () => {
  it("load starts the output, then fetches the waveform and beats", async () => {
    vi.mocked(ipc.mixLoad).mockResolvedValue({
      deck: 1,
      trackId: 9,
      durationMs: 1000,
      hotCues: [{ slot: 0, positionMs: 10 }],
    });
    await useMix.getState().load(1, { id: 9, title: "T", artist: null });
    expect(ipc.mixStart).toHaveBeenCalledTimes(1);
    expect(ipc.mixLoad).toHaveBeenCalledWith(1, 9);
    const deck = useMix.getState().decks[1];
    expect(deck?.track?.id).toBe(9);
    expect(deck?.loading).toBeNull();
    expect(deck?.hotCues).toEqual([{ slot: 0, positionMs: 10 }]);
    expect(deck?.columns?.length).toBe(4);
    expect(deck?.beats).toHaveLength(1);
  });

  it("a newer load on the same deck wins", async () => {
    let first: (v: Awaited<ReturnType<typeof ipc.mixLoad>>) => void = () => {};
    vi.mocked(ipc.mixLoad)
      .mockImplementationOnce(() => new Promise((r) => (first = r)))
      .mockResolvedValueOnce({ deck: 0, trackId: 2, durationMs: 1, hotCues: [] });
    useMix.setState({ status: "running", live: running });
    const a = useMix.getState().load(0, { id: 1, title: "A", artist: null });
    await useMix.getState().load(0, { id: 2, title: "B", artist: null });
    first({ deck: 0, trackId: 1, durationMs: 1, hotCues: [] });
    await a;
    expect(useMix.getState().decks[0]?.track?.id).toBe(2);
  });

  it("sends knob values and switches as engine values", () => {
    useMix.setState({ status: "running", live: running });
    const s = useMix.getState();
    s.setChannel(2, "xfadeAssign", "y");
    s.setChannel(0, "cue", true);
    s.setFilter("y", "link", true);
    s.setMaster("level", 0.6);
    s.nudge(1, -1);
    expect(vi.mocked(ipc.mixControl).mock.calls.map((c) => c[0])).toEqual([
      { type: "channel", ch: 2, param: "xfadeAssign", value: 1 },
      { type: "channel", ch: 0, param: "cue", value: 1 },
      { type: "filter", unit: "y", param: "link", value: 1 },
      { type: "master", param: "level", value: 0.6 },
      { type: "nudge", deck: 1, ratio: 0.96 },
    ]);
    expect(useMix.getState().channels[2]?.xfadeAssign).toBe("y");
  });

  it("sends nothing while the output is off", () => {
    useMix.getState().setCrossfader({ pos: 0 });
    expect(ipc.mixControl).not.toHaveBeenCalled();
    expect(useMix.getState().crossfader.pos).toBe(0);
  });
});
