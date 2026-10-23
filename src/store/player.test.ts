import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { DeckView, MixState } from "../lib/ipc";

vi.mock("../lib/ipc", () => ({
  DECKS: 4,
  audioDevices: vi.fn(),
  mixDeviceOptions: vi.fn(),
  mixStart: vi.fn(),
  mixStop: vi.fn(),
  mixLoad: vi.fn(),
  mixControl: vi.fn(),
  libraryTrack: vi.fn(),
  analysisReanalyse: vi.fn(),
  waveformColumns: vi.fn(),
  beatgridGet: vi.fn(),
}));

const ipc = await import("../lib/ipc");
const { useMix } = await import("./mix");
const { PLAYER_DECK, activateTrack, playTrack, togglePlayer } = await import("./player");
const { usePanels } = await import("./panels");
const { useToasts } = await import("./toasts");

const view = (over: Partial<DeckView>): DeckView => ({
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
  master: false,
  ...over,
});

const state = (decks: DeckView[]): MixState => ({
  device: "Out",
  sampleRate: 44_100,
  channels: 2,
  bufferFrames: null,
  deviceLost: false,
  xruns: 0,
  masterDeck: null,
  decks,
  meters: [],
  masterMeter: { level: 0, clip: false },
});

const initial = useMix.getState();

beforeEach(() => {
  vi.mocked(ipc.mixStart).mockResolvedValue(state([]));
  vi.mocked(ipc.mixControl).mockResolvedValue(undefined);
  vi.mocked(ipc.mixLoad).mockResolvedValue({
    deck: 1,
    trackId: 7,
    durationMs: 1,
    hotCues: [],
    memoryCues: [],
  });
  vi.mocked(ipc.waveformColumns).mockResolvedValue(new ArrayBuffer(0));
  vi.mocked(ipc.beatgridGet).mockRejectedValue("no grid");
});

afterEach(() => {
  vi.clearAllMocks();
  useMix.setState(initial, true);
  usePanels.getState().setMixOpen(false);
  useToasts.getState().dismiss();
});

describe("player", () => {
  it("plays a track on the player deck, starting the output", async () => {
    await playTrack({ kind: "library", id: 7, title: "Seven", artist: null });
    expect(ipc.mixStart).toHaveBeenCalledTimes(1);
    expect(ipc.mixLoad).toHaveBeenCalledWith(PLAYER_DECK, 7);
    expect(ipc.mixControl).toHaveBeenCalledWith({ type: "play", deck: PLAYER_DECK });
    expect(useMix.getState().decks[PLAYER_DECK]?.track?.title).toBe("Seven");
  });

  it("toggles the player deck", () => {
    const decks = [view({}), view({ loaded: true, playing: true })];
    useMix.setState({ status: "running", live: state(decks) });
    togglePlayer();
    expect(ipc.mixControl).toHaveBeenCalledWith({ type: "pause", deck: PLAYER_DECK });
  });

  it("does nothing on toggle with no track", () => {
    togglePlayer();
    expect(ipc.mixControl).not.toHaveBeenCalled();
  });
});

describe("activateTrack", () => {
  const seven = { kind: "library", id: 7, title: "Seven", artist: null } as const;
  const loaded = {
    kind: "library" as const,
    id: 1,
    title: "One",
    artist: null,
    key: null,
    artworkId: null,
    artworkMime: "",
  };

  it("plays on the player deck without the Mix panel", async () => {
    await activateTrack(seven);
    expect(ipc.mixLoad).toHaveBeenCalledWith(PLAYER_DECK, 7);
    expect(ipc.mixControl).toHaveBeenCalledWith({ type: "play", deck: PLAYER_DECK });
  });

  it("loads paused onto a spare deck while mixing", async () => {
    usePanels.getState().setMixOpen(true);
    const decks = initial.decks.map((d, i) => (i === 1 ? { ...d, track: loaded } : d));
    useMix.setState({
      status: "running",
      live: state([view({}), view({ loaded: true, playing: true }), view({}), view({})]),
      decks,
    });
    await activateTrack(seven);
    expect(ipc.mixLoad).toHaveBeenCalledWith(2, 7);
    expect(ipc.mixControl).not.toHaveBeenCalledWith({ type: "play", deck: 2 });
  });

  it("loads nothing and says so when no deck is spare", async () => {
    usePanels.getState().setMixOpen(true);
    const decks = initial.decks.map((d) => ({ ...d, track: loaded }));
    useMix.setState({
      status: "running",
      live: state([
        view({}),
        view({ loaded: true, playing: true }),
        view({ loaded: true }),
        view({}),
      ]),
      decks,
    });
    await activateTrack(seven);
    expect(ipc.mixLoad).not.toHaveBeenCalled();
    expect(useToasts.getState().toast?.message).toMatch(/No free deck/);
  });
});
