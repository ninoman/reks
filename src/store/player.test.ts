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
const { PLAYER_DECK, playTrack, togglePlayer } = await import("./player");

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
