import { describe, expect, it, vi } from "vitest";

vi.mock("../lib/ipc", () => ({
  DECKS: 4,
  audioDevices: vi.fn(),
  mixDeviceOptions: vi.fn(),
  mixStart: vi.fn(),
  mixStop: vi.fn().mockResolvedValue(undefined),
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
const { useMix } = await import("./mix");
const { releaseStaleEngine } = await import("./mixEngine");

describe("releaseStaleEngine", () => {
  it("stops the engine a reloaded page left running, so the UI's 'stopped' is true", async () => {
    expect(useMix.getState().status).toBe("stopped");
    await releaseStaleEngine();
    expect(ipc.mixStop).toHaveBeenCalledTimes(1);
    expect(useMix.getState().status).toBe("stopped");
  });
});
