import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { DECK_ZOOM_S, DEFAULT_DECK_ZOOM } from "../lib/deckScroll";
import type { CueRouting, DeckView, MixState } from "../lib/ipc";

vi.mock("../lib/ipc", () => ({
  DECKS: 4,
  audioDevices: vi.fn(),
  mixDeviceOptions: vi.fn(),
  mixStart: vi.fn(),
  mixStop: vi.fn(),
  mixLoad: vi.fn(),
  mixLoadStick: vi.fn(),
  mixControl: vi.fn(),
  mixCueEdit: vi.fn(),
  libraryTrack: vi.fn(),
  analysisReanalyse: vi.fn(),
  waveformColumns: vi.fn(),
  beatgridGet: vi.fn(),
  deviceWaveformColumns: vi.fn(),
  deviceBeatgrid: vi.fn(),
}));

const ipc = await import("../lib/ipc");
const { libraryId, useMix } = await import("./mix");

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
  loopRegion: null,
  loopIn: null,
  lastLoop: null,
  sync: false,
  syncBar: false,
  locked: false,
  quantise: true,
  masterTempo: false,
  master: false,
  ...over,
});

const running: MixState = {
  device: "Out",
  sampleRate: 44_100,
  channels: 2,
  bufferFrames: null,
  deviceLost: false,
  xruns: 0,
  masterDeck: null,
  decks: [],
  preview: deckView(),
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

const sent = () => vi.mocked(ipc.mixControl).mock.calls.map((c) => c[0]);

/** `QUEUE` in crates/reks-mix/src/engine.rs. */
const ENGINE_QUEUE = 256;

/** `library_track` as the store reads it: title, artist, key, artwork. */
const detail = (id: number) =>
  ({
    track: {
      id,
      title: `T${id}`,
      artist: null,
      key: "Am",
      artwork: { id: 50 + id, mime: "image/jpeg" },
    },
    cues: [],
    beatgrid: null,
  }) as unknown as Awaited<ReturnType<typeof ipc.libraryTrack>>;

beforeEach(() => {
  vi.mocked(ipc.libraryTrack).mockImplementation(async (id) => detail(id));
  vi.mocked(ipc.analysisReanalyse).mockResolvedValue(1);
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
      load: 1,
      trackId: 9,
      durationMs: 1000,
      hotCues: [{ id: 1, slot: 0, positionMs: 10, colorRgb: null, name: null }],
      memoryCues: [],
    });
    await useMix.getState().load(1, { kind: "library", id: 9, title: "T", artist: null });
    expect(ipc.mixStart).toHaveBeenCalledTimes(1);
    expect(ipc.mixLoad).toHaveBeenCalledWith(1, 9);
    const deck = useMix.getState().decks[1];
    expect(libraryId(deck?.track)).toBe(9);
    expect(deck?.loading).toBeNull();
    expect(deck?.hotCues).toEqual([{ id: 1, slot: 0, positionMs: 10, colorRgb: null, name: null }]);
    expect(deck?.columns?.length).toBe(4);
    expect(deck?.beats).toHaveLength(1);
    expect(deck?.track).toMatchObject({ key: "Am", artworkId: 59, artworkMime: "image/jpeg" });
    expect(deck?.analysing).toBe(false);
    expect(ipc.analysisReanalyse).not.toHaveBeenCalled();
  });

  it("a track without a grid is analysed; the deck reloads with the grid once paused", async () => {
    const loaded = { deck: 1, load: 1, trackId: 9, durationMs: 1000, hotCues: [], memoryCues: [] };
    vi.mocked(ipc.mixLoad).mockResolvedValue(loaded);
    vi.mocked(ipc.beatgridGet).mockResolvedValueOnce({
      anchors: [],
      source: null,
      locked: false,
      beats: [],
    });
    await useMix.getState().load(1, 9);
    expect(ipc.analysisReanalyse).toHaveBeenCalledWith([9]);
    expect(useMix.getState().decks[1]?.analysing).toBe(true);
    // Playing when the grid arrives: it waits for a pause.
    const playing = deckView({
      trackId: 9,
      loaded: true,
      playing: true,
      positionMs: 400,
      cueMs: 100,
    });
    useMix.setState({ live: { ...running, decks: [deckView(), playing] } });
    vi.mocked(ipc.mixLoad).mockClear();
    vi.mocked(ipc.mixControl).mockClear();
    useMix.getState().analysisDone(9, true);
    await vi.waitFor(() => expect(useMix.getState().decks[1]?.gridPending).toBe(true));
    expect(useMix.getState().decks[1]?.analysing).toBe(false);
    expect(useMix.getState().decks[1]?.beats).toHaveLength(1);
    expect(ipc.mixLoad).not.toHaveBeenCalled();
    // Paused: reloaded, with the cue point, position and tempo put back.
    const state = { ...running, decks: [deckView(), { ...playing, playing: false, tempo: 1.02 }] };
    useMix.setState({ live: state });
    vi.mocked(ipc.mixStart).mock.lastCall?.[1](state);
    await vi.waitFor(() => expect(ipc.mixLoad).toHaveBeenCalledWith(1, 9));
    await vi.waitFor(() =>
      expect(sent()).toEqual([
        { type: "seek", deck: 1, ms: 100 },
        { type: "cue", deck: 1 },
        { type: "seek", deck: 1, ms: 400 },
        { type: "tempo", deck: 1, ratio: 1.02 },
      ]),
    );
    expect(useMix.getState().decks[1]?.gridPending).toBe(false);
  });

  it("analysis that fails leaves the deck without a grid", async () => {
    vi.mocked(ipc.mixLoad).mockResolvedValue({
      deck: 0,
      load: 1,
      trackId: 4,
      durationMs: 1,
      hotCues: [],
      memoryCues: [],
    });
    vi.mocked(ipc.beatgridGet).mockResolvedValueOnce({
      anchors: [],
      source: null,
      locked: false,
      beats: [],
    });
    await useMix.getState().load(0, 4);
    useMix.getState().analysisDone(4, false);
    expect(useMix.getState().decks[0]).toMatchObject({ analysing: false, gridPending: false });
  });

  it("a newer load on the same deck wins", async () => {
    let first: (v: Awaited<ReturnType<typeof ipc.mixLoad>>) => void = () => {};
    vi.mocked(ipc.mixLoad)
      .mockImplementationOnce(() => new Promise((r) => (first = r)))
      .mockResolvedValueOnce({ deck: 0, load: 1, trackId: 2, durationMs: 1, hotCues: [], memoryCues: [] });
    useMix.setState({ status: "running", live: running });
    const a = useMix.getState().load(0, { kind: "library", id: 1, title: "A", artist: null });
    await useMix.getState().load(0, { kind: "library", id: 2, title: "B", artist: null });
    first({ deck: 0, load: 1, trackId: 1, durationMs: 1, hotCues: [], memoryCues: [] });
    await a;
    expect(libraryId(useMix.getState().decks[0]?.track)).toBe(2);
  });

  it("sends knob values and switches as engine values", () => {
    useMix.setState({
      status: "running",
      live: running,
      look: { eq: "fourBand", view: "advanced", filters: "show" },
    });
    const s = useMix.getState();
    s.setChannel(2, "xfadeAssign", "y");
    s.setChannel(0, "cue", true);
    s.setFilter("y", "highPass", true);
    s.setMaster("level", 0.6);
    s.nudge(1, -1);
    expect(vi.mocked(ipc.mixControl).mock.calls.map((c) => c[0])).toEqual([
      { type: "channel", ch: 2, param: "xfadeAssign", value: 1 },
      { type: "channel", ch: 0, param: "cue", value: 1 },
      { type: "filter", unit: "y", param: "highPass", value: 1 },
      { type: "master", param: "level", value: 0.6 },
      { type: "nudge", deck: 1, ratio: 0.96 },
    ]);
    expect(useMix.getState().channels[2]?.xfadeAssign).toBe("y");
  });

  it("sends the hidden controls' defaults in Simple and keeps the user's positions", () => {
    useMix.setState({ status: "running", live: running });
    const s = useMix.getState();
    s.setChannel(2, "xfadeAssign", "y");
    s.setChannel(0, "filterAssign", "y");
    s.setChannel(1, "fader", 0.4);
    s.setFilter("x", "on", true);
    expect(vi.mocked(ipc.mixControl).mock.calls.map((c) => c[0])).toEqual([
      { type: "channel", ch: 2, param: "xfadeAssign", value: 0.5 },
      { type: "channel", ch: 0, param: "filterAssign", value: 0 },
      { type: "channel", ch: 1, param: "fader", value: 0.4 },
      { type: "filter", unit: "x", param: "on", value: 1 },
    ]);
    expect(useMix.getState().channels[2]?.xfadeAssign).toBe("y");
    expect(useMix.getState().channels[0]?.filterAssign).toBe("y");
  });

  it("sends both filters off when they are hidden", () => {
    useMix.setState({
      status: "running",
      live: running,
      look: { eq: "fourBand", view: "simple", filters: "hide" },
    });
    useMix.getState().setFilter("x", "on", true);
    expect(vi.mocked(ipc.mixControl).mock.calls.map((c) => c[0])).toEqual([
      { type: "filter", unit: "x", param: "on", value: 0 },
    ]);
    expect(useMix.getState().filters.x.on).toBe(true);
  });

  it("a view switch sends the effective values and Advanced brings the positions back", () => {
    useMix.setState({
      status: "running",
      live: running,
      look: { eq: "fourBand", view: "advanced", filters: "show" },
    });
    const s = useMix.getState();
    s.setChannel(2, "xfadeAssign", "y");
    s.setChannel(0, "filterAssign", "off");
    s.setFilter("y", "on", true);
    vi.mocked(ipc.mixControl).mockClear();
    const sent = () => vi.mocked(ipc.mixControl).mock.calls.map((c) => c[0]);
    s.setLook({ eq: "fourBand", view: "simple", filters: "hide" });
    expect(sent()).toContainEqual({ type: "channel", ch: 2, param: "xfadeAssign", value: 0.5 });
    expect(sent()).toContainEqual({ type: "channel", ch: 0, param: "filterAssign", value: 0 });
    expect(sent()).toContainEqual({ type: "filter", unit: "y", param: "on", value: 0 });
    vi.mocked(ipc.mixControl).mockClear();
    s.setLook({ eq: "fourBand", view: "advanced", filters: "hide" });
    expect(sent()).toContainEqual({ type: "channel", ch: 2, param: "xfadeAssign", value: 1 });
    expect(sent()).toContainEqual({ type: "channel", ch: 0, param: "filterAssign", value: 0.5 });
    expect(sent()).toContainEqual({ type: "filter", unit: "y", param: "on", value: 1 });
  });

  it("an EQ layout switch flattens one EQ and sends the other's own positions", () => {
    useMix.setState({ status: "running", live: running });
    const s = useMix.getState();
    s.setChannel(1, "eqLo", 0.2);
    s.setChannel(1, "eq3Lo", 0.7);
    s.setChannel(1, "fx", 0.9);
    s.setFilter("x", "on", true);
    const sent = () => vi.mocked(ipc.mixControl).mock.calls.map((c) => c[0]);
    // 4-band: the 3-band knobs are stored but sent at noon.
    expect(sent()).toContainEqual({ type: "channel", ch: 1, param: "eqLo", value: 0.2 });
    expect(sent()).toContainEqual({ type: "channel", ch: 1, param: "eq3Lo", value: 0.5 });
    expect(sent()).toContainEqual({ type: "channel", ch: 1, param: "fx", value: 0.5 });
    vi.mocked(ipc.mixControl).mockClear();
    s.setLook({ eq: "threeBand", view: "simple", filters: "show" });
    expect(sent()).toContainEqual({ type: "channel", ch: 1, param: "eqLo", value: 0.5 });
    expect(sent()).toContainEqual({ type: "channel", ch: 1, param: "eq3Lo", value: 0.7 });
    expect(sent()).toContainEqual({ type: "channel", ch: 1, param: "fx", value: 0.9 });
    expect(sent()).toContainEqual({ type: "filter", unit: "x", param: "on", value: 0 });
    expect(sent()).toContainEqual({ type: "channelFx", kind: "filter" });
    expect(sent()).toContainEqual({ type: "fxParameter", value: 0.5 });
    // The resync is one burst: it must fit the engine's control queue
    // (`QUEUE` in crates/reks-mix/src/engine.rs), or its tail is refused.
    expect(sent().length).toBeLessThanOrEqual(ENGINE_QUEUE);
    vi.mocked(ipc.mixControl).mockClear();
    s.setLook({ eq: "fourBand", view: "simple", filters: "show" });
    expect(sent()).toContainEqual({ type: "channel", ch: 1, param: "eqLo", value: 0.2 });
    expect(sent()).toContainEqual({ type: "channel", ch: 1, param: "fx", value: 0.5 });
    expect(sent()).toContainEqual({ type: "filter", unit: "x", param: "on", value: 1 });
    expect(useMix.getState().channels[1]).toMatchObject({ eqLo: 0.2, eq3Lo: 0.7, fx: 0.9 });
  });

  it("3-band starts flat, with FILTER selected and PARAMETER at noon", () => {
    const s = useMix.getState();
    for (const c of s.channels)
      expect(c).toMatchObject({ eq3Hi: 0.5, eq3Mid: 0.5, eq3Lo: 0.5, fx: 0.5 });
    expect(s.fx).toEqual({ kind: "filter", parameter: 0.5 });
  });

  it("sends the FX select and PARAMETER", () => {
    useMix.setState({ status: "running", live: running });
    const s = useMix.getState();
    s.setFx({ kind: "echo" });
    s.setFx({ parameter: 0.8 });
    expect(vi.mocked(ipc.mixControl).mock.calls.map((c) => c[0])).toEqual([
      { type: "channelFx", kind: "echo" },
      { type: "fxParameter", value: 0.8 },
    ]);
    expect(useMix.getState().fx).toEqual({ kind: "echo", parameter: 0.8 });
  });

  it("starts the engine with the effective values", async () => {
    const s = useMix.getState();
    s.setChannel(2, "xfadeAssign", "y");
    await s.start();
    expect(vi.mocked(ipc.mixControl).mock.calls.map((c) => c[0])).toContainEqual({
      type: "channel",
      ch: 2,
      param: "xfadeAssign",
      value: 0.5,
    });
  });

  it("has no VCF TO XFADER (removed 2026-10-22)", () => {
    const { x, y } = useMix.getState().filters;
    expect(Object.keys(x)).not.toContain("vcfToXfader");
    expect(Object.keys(y)).not.toContain("vcfToXfader");
  });

  it("zooms every deck's waveform together, within the levels, across an eject", () => {
    useMix.setState({ status: "running", live: running });
    const s = useMix.getState();
    expect(s.waveZoom).toBe(DEFAULT_DECK_ZOOM);
    s.zoom(1);
    s.unload(1);
    expect(useMix.getState().waveZoom).toBe(DEFAULT_DECK_ZOOM + 1);
    for (let i = 0; i < 10; i++) s.zoom(1);
    expect(useMix.getState().waveZoom).toBe(DECK_ZOOM_S.length - 1);
    for (let i = 0; i < 10; i++) s.zoom(-1);
    expect(useMix.getState().waveZoom).toBe(0);
  });

  it("pads: jumps and loops remember their size; loop pads set, resize or leave", () => {
    const looping = (beats: number | null) =>
      useMix.setState({
        status: "running",
        live: {
          ...running,
          decks: [
            deckView({
              loaded: true,
              loopRegion: beats === null ? null : { startMs: 0, endMs: 1, beats },
            }),
          ],
        },
      });
    looping(null);
    const mix = useMix.getState();
    mix.beatJump(0, -2);
    mix.jumpLast(0, 1);
    mix.loopPad(0, 8);
    expect(useMix.getState().decks[0]).toMatchObject({ lastJump: 2, lastLoop: 8 });
    looping(8);
    mix.loopPad(0, 2);
    mix.loopPad(0, 2);
    looping(2);
    mix.loopPad(0, 2);
    mix.toggleLoop(0);
    looping(null);
    mix.toggleLoop(0);
    expect(sent()).toEqual([
      { type: "beatJump", deck: 0, beats: -2 },
      { type: "beatJump", deck: 0, beats: 2 },
      { type: "beatLoop", deck: 0, beats: 8 },
      { type: "loopResize", deck: 0, double: false },
      { type: "loopResize", deck: 0, double: false },
      { type: "loopResize", deck: 0, double: false },
      { type: "loopResize", deck: 0, double: false },
      { type: "loopExit", deck: 0 },
      { type: "loopExit", deck: 0 },
      { type: "beatLoop", deck: 0, beats: 2 },
    ]);
  });

  it("resizing a playing beat loop keeps the new size for the next loop", () => {
    useMix.setState({
      status: "running",
      live: {
        ...running,
        decks: [deckView({ loaded: true, loopRegion: { startMs: 0, endMs: 1, beats: 4 } })],
      },
    });
    useMix.getState().loopResize(0, true);
    expect(sent()).toEqual([{ type: "loopResize", deck: 0, double: true }]);
    expect(useMix.getState().decks[0]?.lastLoop).toBe(8);
  });

  it("cues: a pad jumps or sets; edits go to the library and the deck takes the result", async () => {
    const track = {
      kind: "library" as const,
      id: 9,
      title: "T",
      artist: null,
      key: null,
      artworkId: null,
      artworkMime: "",
    };
    const hot = { id: 1, slot: 0, positionMs: 10, colorRgb: 0x28e214, name: null };
    useMix.setState({
      status: "running",
      live: { ...running, decks: [deckView({ loaded: true, positionMs: 5_000 })] },
      decks: useMix.getState().decks.map((d, i) => (i === 0 ? { ...d, track, hotCues: [hot] } : d)),
    });
    const after = {
      hotCues: [hot, { ...hot, id: 2, slot: 1, positionMs: 5_000 }],
      memoryCues: [
        { id: 3, positionMs: 1_000, endMs: null, activeLoop: false, colorRgb: null, name: null },
        { id: 4, positionMs: 9_000, endMs: null, activeLoop: false, colorRgb: null, name: "Out" },
      ],
    };
    vi.mocked(ipc.mixCueEdit).mockResolvedValue(after);
    const mix = useMix.getState();
    mix.hotCuePad(0, 0);
    expect(sent()).toEqual([{ type: "hotCue", deck: 0, slot: 0 }]);
    expect(ipc.mixCueEdit).not.toHaveBeenCalled();

    mix.hotCuePad(0, 1);
    await vi.waitFor(() => expect(useMix.getState().decks[0]?.hotCues).toHaveLength(2));
    expect(ipc.mixCueEdit).toHaveBeenCalledWith(0, { type: "setHotCue", slot: 1 });
    expect(useMix.getState().decks[0]?.memoryCues).toEqual(after.memoryCues);
    expect(useMix.getState().cueRevisions[9]).toBe(1);

    await mix.editCues(0, { type: "rename", cueId: 4, name: "Outro" });
    expect(ipc.mixCueEdit).toHaveBeenLastCalledWith(0, { type: "rename", cueId: 4, name: "Outro" });
    expect(useMix.getState().cueRevisions[9]).toBe(2);

    // Memory cue call from the playhead (5 s): back to 1 s, on to 9 s.
    mix.callAdjacentCue(0, -1);
    mix.callAdjacentCue(0, 1);
    expect(sent().slice(1)).toEqual([
      { type: "callCue", deck: 0, ms: 1_000, endMs: null },
      { type: "callCue", deck: 0, ms: 9_000, endMs: null },
    ]);

    vi.mocked(ipc.mixCueEdit).mockRejectedValue(new Error("hot cue A is already set"));
    await mix.editCues(0, { type: "setHotCue", slot: 0 });
    expect(useMix.getState().error).toBe("hot cue A is already set");
    expect(useMix.getState().decks[0]?.hotCues).toHaveLength(2);
  });

  it("keeps pad mode, pages, sizes and the loop page across an eject", () => {
    const mix = useMix.getState();
    mix.setLoopPage(2, "manual");
    mix.setPadMode(2, "beatLoop");
    mix.cyclePadMode(2);
    mix.setPadPage(2, "beatJump", 1);
    mix.beatJump(2, 16);
    mix.unload(2);
    expect(useMix.getState().decks[2]).toMatchObject({
      padMode: "hotCue",
      padPage: { beatJump: 1, beatLoop: 1 },
      lastJump: 16,
      loopPage: "manual",
      track: null,
    });
  });

  it("a typed BPM sets the tempo and steps the range up when needed", () => {
    useMix.setState({
      status: "running",
      live: { ...running, decks: [deckView({ loaded: true, bpm: 128, speed: 1 })] },
    });
    useMix.getState().setBpm(0, 145);
    expect(useMix.getState().decks[0]?.tempoRange).toBe(0.16);
    const t = sent()[0];
    expect(t?.type === "tempo" && t.ratio).toBeCloseTo(145 / 128, 6);
    // Synced: ignored.
    useMix.setState({
      live: { ...running, decks: [deckView({ loaded: true, bpm: 128, speed: 1, sync: true })] },
    });
    useMix.getState().setBpm(0, 120);
    expect(sent()).toHaveLength(1);
  });

  it("sync uses the bar setting it is given; take master", () => {
    useMix.setState({
      status: "running",
      live: { ...running, decks: [deckView({ loaded: true })] },
    });
    useMix.getState().toggleSync(0, false);
    useMix.getState().takeMaster(0);
    expect(sent()).toEqual([
      { type: "sync", deck: 0, on: true, bar: false },
      { type: "takeMaster", deck: 0 },
    ]);
  });

  it("sends nothing while the output is off", () => {
    useMix.getState().setCrossfader({ pos: 0 });
    expect(ipc.mixControl).not.toHaveBeenCalled();
    expect(useMix.getState().crossfader.pos).toBe(0);
  });

  it("a stick track loads off the stick with the stick's views, and is never analysed", async () => {
    vi.mocked(ipc.mixLoadStick).mockResolvedValue({
      deck: 1,
      load: 1,
      trackId: null,
      durationMs: 1000,
      hotCues: [],
      memoryCues: [],
    });
    vi.mocked(ipc.deviceWaveformColumns).mockResolvedValue(new Uint8Array(4).buffer);
    vi.mocked(ipc.deviceBeatgrid).mockResolvedValue({
      anchors: [],
      source: "stick",
      locked: true,
      beats: [],
    });
    const ref = {
      kind: "stick" as const,
      mountPoint: "/Volumes/REKS",
      stickId: 5,
      title: "S",
      artist: null,
      key: "Am",
    };
    await useMix.getState().load(1, ref);
    expect(ipc.mixLoadStick).toHaveBeenCalledWith(1, "/Volumes/REKS", 5);
    expect(ipc.deviceBeatgrid).toHaveBeenCalledWith("/Volumes/REKS", 5);
    expect(ipc.mixLoad).not.toHaveBeenCalled();
    expect(ipc.libraryTrack).not.toHaveBeenCalled();
    // No grid on the stick: it stays without one.
    expect(ipc.analysisReanalyse).not.toHaveBeenCalled();
    const deck = useMix.getState().decks[1];
    expect(deck?.track).toMatchObject({ kind: "stick", stickId: 5, key: "Am" });
    expect(libraryId(deck?.track)).toBeNull();

    // Its cues are the stick's.
    await useMix.getState().editCues(1, { type: "addMemoryCue" });
    expect(ipc.mixCueEdit).not.toHaveBeenCalled();
    expect(useMix.getState().error).toMatch(/stick/);
  });

  it("load with play plays once loaded, and leaves a deck already playing that track", async () => {
    vi.mocked(ipc.mixLoad).mockResolvedValue({
      deck: 1,
      load: 1,
      trackId: 9,
      durationMs: 1,
      hotCues: [],
      memoryCues: [],
    });
    await useMix.getState().load(1, { kind: "library", id: 9, title: "T", artist: null }, true);
    expect(sent()).toContainEqual({ type: "play", deck: 1 });

    const decks = [deckView(), deckView({ loaded: true, playing: true, trackId: 9 })];
    useMix.setState({ live: { ...running, decks } });
    vi.mocked(ipc.mixLoad).mockClear();
    await useMix.getState().load(1, { kind: "library", id: 9, title: "T", artist: null }, true);
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
    vi.mocked(ipc.mixLoad).mockResolvedValue({
      deck: 1,
      load: 1,
      trackId: 9,
      durationMs: 1,
      hotCues: [],
      memoryCues: [],
    });
    await useMix.getState().load(1, { kind: "library", id: 9, title: "T", artist: null });
    const decks = [
      deckView(),
      deckView({ loaded: true, playing: true, trackId: 9, positionMs: 4200, tempo: 1.02 }),
    ];
    useMix.setState({ live: { ...running, decks } });
    vi.mocked(ipc.mixControl).mockClear();
    await useMix.getState().setOutput({ device: "Interface" });
    expect(ipc.mixStart).toHaveBeenCalledTimes(2);
    expect(vi.mocked(ipc.mixStart).mock.lastCall?.[0].device).toBe("Interface");
    expect(libraryId(useMix.getState().decks[1]?.track)).toBe(9);
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
    await useMix.getState().setOutput({ device: "Interface" });
    expect(useMix.getState().config.device).toBe("Interface");
    expect(ipc.mixStart).not.toHaveBeenCalled();
  });
});
