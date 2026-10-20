import { beforeEach, describe, expect, it, vi } from "vitest";

import type { AnalysisEvent, AnalysisStatus } from "../lib/ipc";

vi.mock("../lib/ipc", async (original) => ({
  ...(await original<typeof import("../lib/ipc")>()),
  analysisSubscribe: vi.fn(),
  analysisPause: vi.fn(),
  analysisCancel: vi.fn(),
  analysisReanalyse: vi.fn(),
  libraryTrack: vi.fn(),
}));

const ipc = await import("../lib/ipc");
const { isIdle, useAnalysis } = await import("./analysis");
const { useLibrary } = await import("./library");

const IDLE: AnalysisStatus = { queued: 0, running: 0, done: 0, failed: 0, total: 0, paused: false };

describe("analysis store", () => {
  let send: (e: AnalysisEvent) => void = () => {};

  beforeEach(() => {
    vi.clearAllMocks();
    useAnalysis.setState({ status: null, error: null });
    vi.mocked(ipc.analysisSubscribe).mockImplementation(async (onEvent) => {
      send = onEvent;
      return IDLE;
    });
  });

  it("follows status events", async () => {
    await useAnalysis.getState().subscribe();
    expect(useAnalysis.getState().status).toEqual(IDLE);
    send({ type: "status", status: { ...IDLE, queued: 3, running: 1, total: 4 } });
    const s = useAnalysis.getState().status as AnalysisStatus;
    expect(isIdle(s)).toBe(false);
    expect(s.total).toBe(4);
  });

  it("swaps in the row of a track whose grid or key changed", async () => {
    const replaceRow = vi.fn();
    useLibrary.setState({ replaceRow });
    vi.mocked(ipc.libraryTrack).mockResolvedValue({ track: { id: 9 } } as never);
    await useAnalysis.getState().subscribe();
    send({
      type: "analysed",
      trackId: 8,
      cached: false,
      stored: { grid: false, bpm: false, key: false },
    });
    send({
      type: "analysed",
      trackId: 9,
      cached: true,
      stored: { grid: true, bpm: true, key: false },
    });
    await vi.waitFor(() => expect(replaceRow).toHaveBeenCalledTimes(1));
    expect(ipc.libraryTrack).toHaveBeenCalledWith(9);
    expect(replaceRow).toHaveBeenCalledWith({ id: 9 });
  });

  it("pauses, cancels and re-analyses through the backend", async () => {
    vi.mocked(ipc.analysisPause).mockResolvedValue({ ...IDLE, paused: true, queued: 2 });
    await useAnalysis.getState().pause();
    expect(useAnalysis.getState().status?.paused).toBe(true);
    vi.mocked(ipc.analysisCancel).mockRejectedValue("analysis is off");
    await useAnalysis.getState().cancel();
    expect(useAnalysis.getState().error).toBe("analysis is off");
    await useAnalysis.getState().reanalyse([]);
    expect(ipc.analysisReanalyse).not.toHaveBeenCalled();
    vi.mocked(ipc.analysisReanalyse).mockResolvedValue(2);
    await useAnalysis.getState().reanalyse([4, 5]);
    expect(ipc.analysisReanalyse).toHaveBeenCalledWith([4, 5]);
  });
});
