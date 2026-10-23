import { beforeEach, describe, expect, it, vi } from "vitest";

import type { DeckView, MixState } from "../lib/ipc";

vi.mock("../lib/ipc", () => ({
  DECKS: 4,
  PREVIEW_DECK: 4,
  mixLoad: vi.fn(),
  mixControl: vi.fn(),
}));

const ipc = await import("../lib/ipc");
const { useMix } = await import("./mix");
const { seekPreview, togglePreview } = await import("./preview");

const deck = (over: Partial<DeckView>) => ({ trackId: null, loaded: false, ...over }) as DeckView;
const live = (preview: DeckView) => ({ decks: [], preview }) as unknown as MixState;

beforeEach(() => {
  vi.mocked(ipc.mixLoad).mockReset().mockResolvedValue({
    deck: 4,
    trackId: 7,
    durationMs: 1000,
    hotCues: [],
    memoryCues: [],
  });
  vi.mocked(ipc.mixControl).mockReset().mockResolvedValue(undefined);
  useMix.setState({ status: "running", live: live(deck({})) });
});

describe("preview", () => {
  it("loads onto the preview deck and plays, never a mixer deck", async () => {
    await togglePreview(7);
    expect(ipc.mixLoad).toHaveBeenCalledWith(4, 7);
    expect(ipc.mixControl).toHaveBeenCalledWith({ type: "play", deck: 4 });
  });

  it("pauses the track it is playing without reloading", async () => {
    useMix.setState({ live: live(deck({ trackId: 7, loaded: true, playing: true })) });
    await togglePreview(7);
    expect(ipc.mixLoad).not.toHaveBeenCalled();
    expect(ipc.mixControl).toHaveBeenCalledWith({ type: "pause", deck: 4 });
  });

  it("seeks a loaded track and plays it from there", async () => {
    useMix.setState({ live: live(deck({ trackId: 7, loaded: true })) });
    await seekPreview(7, 500);
    expect(vi.mocked(ipc.mixControl).mock.calls.map((c) => c[0])).toEqual([
      { type: "seek", deck: 4, ms: 500 },
      { type: "play", deck: 4 },
    ]);
  });
});
