import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { CueRouting, DeckView, MixState } from "../lib/ipc";

vi.mock("../lib/ipc", () => ({
  DECKS: 4,
  audioDevices: vi.fn(),
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
  masterDeck: null,
  decks: [],
  meters: Array.from({ length: 4 }, () => ({ level: 0, clip: false })),
  masterMeter: { level: 0, clip: false },
};

const outputs4: CueRouting = { type: "outputs", master: [0, 1], cue: [2, 3] };
const options = (channels: number) => ({
  rates: [48_000],
  defaultRate: 48_000,
  channels,
  defaultCue: channels >= 4 ? outputs4 : ({ type: "off" } satisfies CueRouting),
  bufferFrames: null,
});

const initial = useMix.getState();

const deckView = (over: Partial<DeckView> = {}): DeckView => ({
  trackId: null,
  loaded: false,
  playing: false,
  positionMs: 0,
  durationMs: 0,
  cueMs: 0,
  tempo: 1,
  nudge: 1,
  speed: 1,
  bpm: null,
  beat: null,
  cuePreview: false,
  sync: false,
  syncBar: false,
  locked: false,
  quantise: true,
  master: false,
  loopRegion: null,
  ...over,
});

const sent = () => vi.mocked(ipc.mixControl).mock.calls.map((c) => c[0]);

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
  it("preselects each device's cue routing until the user picks one", async () => {
    vi.mocked(ipc.audioDevices).mockResolvedValue(["Laptop", "Interface"]);
    vi.mocked(ipc.mixDeviceOptions).mockImplementation(async (d) =>
      options(d === "Interface" ? 4 : 2),
    );
    await useMix.getState().loadDevices();
    expect(useMix.getState().config.cue).toEqual({ type: "off" });
    await useMix.getState().setConfig({ device: "Interface" });
    expect(useMix.getState().config.cue).toEqual(outputs4);

    // A choice the next device can take is kept; one it cannot falls back.
    await useMix.getState().setConfig({ cue: { type: "splitMono" } });
    await useMix.getState().setConfig({ device: "Laptop" });
    expect(useMix.getState().config.cue).toEqual({ type: "splitMono" });
    await useMix.getState().setConfig({ device: "Interface" });
    await useMix.getState().setConfig({ cue: outputs4 });
    await useMix.getState().setConfig({ device: "Laptop" });
    expect(useMix.getState().config.cue).toEqual({ type: "off" });
  });

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

  it("load with play plays once loaded, and leaves a deck already playing that track", async () => {
    vi.mocked(ipc.mixLoad).mockResolvedValue({ deck: 1, trackId: 9, durationMs: 1, hotCues: [] });
    await useMix.getState().load(1, { id: 9, title: "T", artist: null }, true);
    expect(sent()).toContainEqual({ type: "play", deck: 1 });

    const decks = [deckView(), deckView({ loaded: true, playing: true, trackId: 9 })];
    useMix.setState({ live: { ...running, decks } });
    vi.mocked(ipc.mixLoad).mockClear();
    await useMix.getState().load(1, { id: 9, title: "T", artist: null }, true);
    expect(ipc.mixLoad).not.toHaveBeenCalled();
  });

  it("sends the volume with the mixer positions on start", async () => {
    useMix.getState().setVolume(0.3);
    expect(ipc.mixControl).not.toHaveBeenCalled();
    await useMix.getState().start();
    expect(sent()).toContainEqual({ type: "volume", value: 0.3 });
  });

  it("a new device reopens the output with the loaded tracks where they were", async () => {
    vi.mocked(ipc.mixDeviceOptions).mockResolvedValue(options(2));
    vi.mocked(ipc.mixLoad).mockResolvedValue({ deck: 1, trackId: 9, durationMs: 1, hotCues: [] });
    await useMix.getState().load(1, { id: 9, title: "T", artist: null });
    const decks = [
      deckView(),
      deckView({ loaded: true, playing: true, trackId: 9, positionMs: 4200, tempo: 1.02 }),
    ];
    useMix.setState({ live: { ...running, decks } });
    vi.mocked(ipc.mixControl).mockClear();
    await useMix.getState().setDevice("Interface");
    expect(ipc.mixStart).toHaveBeenCalledTimes(2);
    expect(vi.mocked(ipc.mixStart).mock.lastCall?.[0].device).toBe("Interface");
    expect(useMix.getState().decks[1]?.track?.id).toBe(9);
    expect(sent()).toEqual(
      expect.arrayContaining([
        { type: "seek", deck: 1, ms: 4200 },
        { type: "tempo", deck: 1, ratio: 1.02 },
        { type: "play", deck: 1 },
      ]),
    );
  });

  it("a new device while the output is off only changes the config", async () => {
    vi.mocked(ipc.mixDeviceOptions).mockResolvedValue(options(2));
    await useMix.getState().setDevice("Interface");
    expect(useMix.getState().config.device).toBe("Interface");
    expect(ipc.mixStart).not.toHaveBeenCalled();
  });
});
