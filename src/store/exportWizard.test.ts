import { beforeEach, describe, expect, it, vi } from "vitest";

import type {
  Device,
  DeviceInfo,
  DryRun,
  ExportOptions,
  ExportProgress,
  ExportReportView,
  PlaylistNode,
} from "../lib/ipc";

vi.mock("../lib/ipc", async (original) => ({
  ...(await original<typeof import("../lib/ipc")>()),
  exportDefaultOptions: vi.fn(),
  exportPlan: vi.fn(),
  exportApply: vi.fn(),
  exportCancel: vi.fn(),
  deviceEject: vi.fn(),
  devicesList: vi.fn(),
  profileSelection: vi.fn(),
  profileSetSelection: vi.fn(),
  profileSetTarget: vi.fn(),
  profileStatus: vi.fn(),
  analysisReanalyse: vi.fn(),
}));

const ipc = await import("../lib/ipc");
const exportPlan = vi.mocked(ipc.exportPlan);
const exportDefaultOptions = vi.mocked(ipc.exportDefaultOptions);
const exportApply = vi.mocked(ipc.exportApply);
const exportCancel = vi.mocked(ipc.exportCancel);
const deviceEject = vi.mocked(ipc.deviceEject);
vi.mocked(ipc.devicesList).mockResolvedValue([]);
const profileSelection = vi.mocked(ipc.profileSelection);
const profileSetSelection = vi.mocked(ipc.profileSetSelection);
const { useAnalysis } = await import("./analysis");
const { useProfiles, coveredIds, selectionOf } = await import("./profiles");
const { useExportWizard } = await import("./exportWizard");

/** What `export_default_options` returns (stands in for Settings › Export). */
const DEFAULTS: ExportOptions = {
  target: "Generic",
  keepStale: false,
  missingAnalysis: "generate",
  artwork: false,
  history: "backUp",
  discardHistoryConfirmed: false,
  historyChosen: false,
  dividers: true,
  activeLoops: false,
  anlzPreference: "rekordboxFirst",
  onelibrary: false,
  onelibraryRemovalConfirmed: false,
};

const node = (id: number, parentId: number | null, isFolder = false): PlaylistNode => ({
  id,
  parentId,
  name: `n${id}`,
  isFolder,
  depth: 0,
  position: 0,
  entries: 0,
  smart: false,
});

/** Gigs/ (1) → Friday (2), Old/ (3) → Older (4); Other (5). Depth first. */
const NODES = [node(1, null, true), node(2, 1), node(3, 1, true), node(4, 3), node(5, null)];

const DEVICE: Device = {
  id: 1,
  volumeSerial: "4993-1CFB",
  label: "REKS",
  sizeBytes: 32_000_000_000,
  name: "Main",
  target: "Cdj2000Nxs2",
  createdAt: "2026-10-19T12:00:00.000Z",
  lastSyncedAt: null,
};

const INFO = { mountPoint: "/Volumes/REKS", label: "REKS" } as DeviceInfo;

const ready = (planId: number): DryRun =>
  ({ planId, ready: true, issues: [] }) as unknown as DryRun;

const progress = (p: Partial<ExportProgress>): ExportProgress => ({
  stage: "write",
  title: "Writing tracks",
  detail: null,
  done: 0,
  total: 0,
  cancellable: true,
  warnings: [],
  ...p,
});

const report = (over: Partial<ExportReportView> = {}): ExportReportView =>
  ({ outcome: "success", mountPoint: "/Volumes/REKS", ...over }) as ExportReportView;

const w = useExportWizard.getState;

beforeEach(() => {
  exportDefaultOptions.mockReset().mockResolvedValue(DEFAULTS);
  exportPlan.mockReset().mockResolvedValue(ready(7));
  exportApply.mockReset();
  exportCancel.mockReset().mockResolvedValue();
  deviceEject.mockReset();
  profileSelection.mockReset().mockResolvedValue({ playlists: [5] });
  profileSetSelection.mockReset().mockResolvedValue();
  vi.mocked(ipc.profileStatus)
    .mockReset()
    .mockResolvedValue(null as never);
  useProfiles.setState({
    known: [DEVICE],
    mounted: {
      [INFO.mountPoint]: { info: INFO, match: { type: "known", device: DEVICE }, error: null },
    },
    selectedId: null,
    ticks: null,
    status: null,
  });
  useExportWizard.setState({ step: "closed" });
  w().close();
});

describe("selection", () => {
  it("a ticked folder covers everything under it", () => {
    const checked = new Set([1, 2, 4, 5]);
    expect([...coveredIds(NODES, checked)]).toEqual([2, 3, 4]);
    expect(selectionOf(NODES, checked)).toEqual({ playlists: [1, 5] });
  });
});

describe("useExportWizard", () => {
  it("opens on the review with the profile, the settings for the stick's target, and plans", async () => {
    await w().open("/Volumes/REKS");
    expect(w().step).toBe("review");
    expect(w().device).toEqual(DEVICE);
    expect(exportDefaultOptions).toHaveBeenCalledWith("Cdj2000Nxs2");
    expect(exportPlan).toHaveBeenCalledWith("/Volumes/REKS", { playlists: [5] }, DEFAULTS, 1);
    expect(w().dryRun).toEqual(ready(7));
  });

  it("refuses a stick that isn't set up", async () => {
    await w().open("/Volumes/OTHER");
    expect(w().error).toBe("Set this stick up in Sticks first.");
    expect(exportPlan).not.toHaveBeenCalled();
  });

  it("says when the settings can't be read", async () => {
    exportDefaultOptions.mockRejectedValue("db locked");
    await w().open("/Volumes/REKS");
    expect(w().error).toBe("Could not read the stick's settings: db locked");
  });

  it("keeps only the newest plan and reports failures", async () => {
    await w().open("/Volumes/REKS");
    let first: (d: DryRun) => void = () => {};
    exportPlan.mockReturnValueOnce(new Promise((r) => (first = r)));
    const a = w().replan();
    exportPlan.mockResolvedValueOnce(ready(9));
    await w().replan();
    first(ready(8));
    await a;
    expect(w().dryRun).toEqual(ready(9));

    exportPlan.mockRejectedValueOnce("stick gone");
    await w().replan();
    expect(w().error).toBe("Could not plan: stick gone");
  });

  it("Leave them out plans this run without the unanalysed tracks", async () => {
    await w().open("/Volumes/REKS");
    await w().leaveOutUnanalysed();
    expect(exportPlan).toHaveBeenLastCalledWith(
      "/Volumes/REKS",
      { playlists: [5] },
      { ...DEFAULTS, missingAnalysis: "skip" },
      1,
    );
  });

  it("keep collapses one card or all of them", async () => {
    exportPlan.mockResolvedValue({
      ...ready(7),
      issues: [{ kind: "missing" }, { kind: "unsupported" }],
    } as DryRun);
    await w().open("/Volumes/REKS");
    w().keep("missing");
    expect([...w().kept]).toEqual(["missing"]);
    w().keep(null);
    expect([...w().kept].sort()).toEqual(["missing", "unsupported"]);
  });

  it("Analyse now waits for the analysis, then plans again", async () => {
    vi.mocked(ipc.analysisReanalyse).mockResolvedValue({} as never);
    useAnalysis.setState({ status: { queued: 1, running: 1 } as never, error: null });
    await w().open("/Volumes/REKS");
    const fix = w().runFix("analyse", [40]);
    await vi.waitFor(() => expect(w().away).toBe("analyse"));
    expect(exportPlan).toHaveBeenCalledTimes(1);
    useAnalysis.setState({ status: { queued: 0, running: 0 } as never });
    await fix;
    expect(w().away).toBeNull();
    expect(exportPlan).toHaveBeenCalledTimes(2);
  });

  it("the history guard's choice plans again", async () => {
    await w().open("/Volumes/REKS");
    await w().chooseHistory("import");
    expect(exportPlan).toHaveBeenLastCalledWith(
      "/Volumes/REKS",
      { playlists: [5] },
      expect.objectContaining({ history: "import", historyChosen: true }),
      1,
    );
  });

  it("exports the shown plan, streams progress and lands on done", async () => {
    await w().open("/Volumes/REKS");
    let send: (p: ExportProgress) => void = () => {};
    let finish: (r: ExportReportView) => void = () => {};
    exportApply.mockImplementation((_id, on) => {
      send = on;
      return new Promise((r) => (finish = r));
    });
    const run = w().startExport();
    expect(w().step).toBe("writing");
    expect(exportApply).toHaveBeenCalledWith(7, expect.any(Function));
    send(progress({ done: 1, total: 3 }));
    expect(w().progress?.done).toBe(1);
    // Closing while writing does nothing.
    w().close();
    expect(w().step).toBe("writing");
    finish(report());
    await run;
    expect(w().step).toBe("done");
  });

  it("cancels once, and not after the database is being written", async () => {
    await w().open("/Volumes/REKS");
    exportApply.mockImplementation(() => new Promise(() => {}));
    void w().startExport();
    await w().cancelExport();
    await w().cancelExport();
    expect(exportCancel).toHaveBeenCalledTimes(1);
    useExportWizard.setState({ cancelling: false, progress: progress({ cancellable: false }) });
    await w().cancelExport();
    expect(exportCancel).toHaveBeenCalledTimes(1);
  });

  it("back to review withdraws the run's choices and plans again", async () => {
    await w().open("/Volumes/REKS");
    await w().chooseHistory("import");
    exportApply.mockResolvedValue(report({ outcome: "problems" }));
    await w().startExport();
    await w().backToReview();
    expect(w().step).toBe("review");
    expect(exportPlan).toHaveBeenLastCalledWith(
      "/Volumes/REKS",
      { playlists: [5] },
      expect.objectContaining({ history: "backUp", historyChosen: false }),
      1,
    );
  });

  it("ejects the reported stick", async () => {
    await w().open("/Volumes/REKS");
    exportApply.mockResolvedValue(report());
    await w().startExport();
    deviceEject.mockResolvedValue();
    await w().ejectStick();
    expect(deviceEject).toHaveBeenCalledWith("/Volumes/REKS");
    expect(w().eject.state).toBe("done");
  });
});

describe("stick ticks", () => {
  it("loads the picked stick's ticks and saves each change as the profile", async () => {
    profileSelection.mockResolvedValue({ playlists: [2] });
    useProfiles.getState().select(1);
    await vi.waitFor(() => expect(useProfiles.getState().ticks).not.toBeNull());
    expect([...(useProfiles.getState().ticks ?? [])]).toEqual([2]);
    await useProfiles.getState().toggleTick(1, NODES);
    // The folder covers Friday: only the folder is saved.
    expect(profileSetSelection).toHaveBeenCalledWith(1, [1]);
  });

  it("puts the ticks back when saving fails", async () => {
    profileSelection.mockResolvedValue({ playlists: [2] });
    useProfiles.getState().select(1);
    await vi.waitFor(() => expect(useProfiles.getState().ticks).not.toBeNull());
    profileSetSelection.mockRejectedValue("db locked");
    await useProfiles.getState().toggleTick(5, NODES);
    expect([...(useProfiles.getState().ticks ?? [])]).toEqual([2]);
    expect(useProfiles.getState().error).toBe("Could not save the stick's playlists: db locked");
  });
});
