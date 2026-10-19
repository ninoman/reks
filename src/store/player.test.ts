import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { PlayerStatus } from "../lib/ipc";

vi.mock("../lib/ipc", () => ({
  overviewWaveform: vi.fn(),
  playerDevices: vi.fn(),
  playerLoad: vi.fn(),
  playerPause: vi.fn(),
  playerPlay: vi.fn(),
  playerSeek: vi.fn(),
  playerSetDevice: vi.fn(),
  playerState: vi.fn(),
  playerVolume: vi.fn(),
}));

const ipc = await import("../lib/ipc");
const { usePlayer, idleStatus, POLL_MS } = await import("./player");

const row = { id: 7, title: "Sine Click", artist: "reks" };

function status(over: Partial<PlayerStatus>): PlayerStatus {
  return { ...idleStatus, device: "Speakers", ...over };
}

const playing = status({ loaded: true, playing: true, durationMs: 30_000, trackId: 7 });
const paused = { ...playing, playing: false };

beforeEach(() => {
  vi.clearAllMocks();
  vi.useFakeTimers();
  usePlayer.setState({
    status: idleStatus,
    track: null,
    overview: null,
    loading: null,
    devices: [],
    error: null,
  });
  vi.mocked(ipc.overviewWaveform).mockResolvedValue(new Float32Array([-0.5, 0.5]).buffer);
});

afterEach(async () => {
  // Stop any poll timer left running.
  vi.mocked(ipc.playerPause).mockResolvedValue(paused);
  usePlayer.setState({ status: playing });
  await usePlayer.getState().toggle();
  vi.useRealTimers();
});

describe("player store", () => {
  it("loads a track, shows it and fetches its overview", async () => {
    vi.mocked(ipc.playerLoad).mockResolvedValue(playing);
    await usePlayer.getState().load(row);
    const s = usePlayer.getState();
    expect(ipc.playerLoad).toHaveBeenCalledWith(7, true);
    expect(s.track).toEqual(row);
    expect(s.status.playing).toBe(true);
    expect(s.loading).toBeNull();
    expect([...(s.overview?.columns ?? [])]).toEqual([-0.5, 0.5]);
    expect(s.overview?.trackId).toBe(7);
  });

  it("keeps the shown track when a newer load won in Rust", async () => {
    usePlayer.setState({ track: { id: 3, title: "Other", artist: null } });
    vi.mocked(ipc.playerLoad).mockResolvedValue({ ...playing, trackId: 3 });
    await usePlayer.getState().load(row);
    expect(usePlayer.getState().track?.id).toBe(3);
    expect(ipc.overviewWaveform).not.toHaveBeenCalled();
  });

  it("polls while playing and stops when paused", async () => {
    vi.mocked(ipc.playerLoad).mockResolvedValue(playing);
    vi.mocked(ipc.playerState).mockResolvedValue({ ...playing, positionMs: 500 });
    await usePlayer.getState().load(row);
    await vi.advanceTimersByTimeAsync(POLL_MS * 3);
    expect(ipc.playerState).toHaveBeenCalledTimes(3);
    expect(usePlayer.getState().status.positionMs).toBe(500);

    vi.mocked(ipc.playerPause).mockResolvedValue(paused);
    await usePlayer.getState().toggle();
    expect(ipc.playerPause).toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(POLL_MS * 3);
    expect(ipc.playerState).toHaveBeenCalledTimes(3);
  });

  it("seek shows the target at once and sends only the newest while one is in flight", async () => {
    usePlayer.setState({ status: paused });
    let release: (s: PlayerStatus) => void = () => {};
    vi.mocked(ipc.playerSeek)
      .mockImplementationOnce(() => new Promise((r) => (release = r)))
      .mockImplementation(async (ms) => ({ ...paused, positionMs: ms }));
    const first = usePlayer.getState().seek(1_000);
    expect(usePlayer.getState().status.positionMs).toBe(1_000);
    void usePlayer.getState().seek(2_000);
    void usePlayer.getState().seek(99_000);
    // Clamped to the track, shown before Rust answers.
    expect(usePlayer.getState().status.positionMs).toBe(30_000);
    release({ ...paused, positionMs: 1_000 });
    await first;
    expect(vi.mocked(ipc.playerSeek).mock.calls).toEqual([[1_000], [30_000]]);
    expect(usePlayer.getState().status.positionMs).toBe(30_000);
  });

  it("toggle does nothing with no track", async () => {
    await usePlayer.getState().toggle();
    expect(ipc.playerPlay).not.toHaveBeenCalled();
    expect(ipc.playerPause).not.toHaveBeenCalled();
  });

  it("shows a load error and clears loading", async () => {
    vi.mocked(ipc.playerLoad).mockRejectedValue("no audio output device None");
    await usePlayer.getState().load(row);
    expect(usePlayer.getState().error).toBe("no audio output device None");
    expect(usePlayer.getState().loading).toBeNull();
  });

  it("clears the track when Rust reports none loaded", async () => {
    usePlayer.setState({ track: row, status: paused });
    vi.mocked(ipc.playerSetDevice).mockResolvedValue(status({ device: "Interface" }));
    await usePlayer.getState().setDevice("Interface");
    expect(ipc.playerSetDevice).toHaveBeenCalledWith("Interface");
    expect(usePlayer.getState().track).toBeNull();
  });
});
