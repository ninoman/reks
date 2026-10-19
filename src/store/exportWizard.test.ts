import { beforeEach, describe, expect, it, vi } from "vitest";

import type { DryRun, PlaylistNode } from "../lib/ipc";

vi.mock("../lib/ipc", async (original) => ({
  ...(await original<typeof import("../lib/ipc")>()),
  exportPlan: vi.fn(),
}));

const ipc = await import("../lib/ipc");
const exportPlan = vi.mocked(ipc.exportPlan);
const { coveredIds, DEFAULT_EXPORT_OPTIONS, selectionOf, useExportWizard } = await import(
  "./exportWizard"
);

const node = (id: number, parentId: number | null, isFolder = false): PlaylistNode => ({
  id,
  parentId,
  name: `n${id}`,
  isFolder,
  depth: 0,
  position: 0,
  entries: 0,
});

/** Gigs/ (1) → Friday (2), Old/ (3) → Older (4); Other (5). Depth first. */
const NODES = [node(1, null, true), node(2, 1), node(3, 1, true), node(4, 3), node(5, null)];

const plan = (status: string): DryRun => ({ status }) as DryRun;

beforeEach(() => {
  exportPlan.mockReset();
  useExportWizard.getState().close();
});

describe("selection", () => {
  it("a ticked folder covers everything under it", () => {
    expect([...coveredIds(NODES, new Set([1]))].sort()).toEqual([2, 3, 4]);
    expect(selectionOf(NODES, new Set([1, 2, 4, 5]))).toEqual({ playlists: [1, 5] });
    expect(selectionOf(NODES, new Set([4, 2]))).toEqual({ playlists: [2, 4] });
    expect(selectionOf(NODES, new Set())).toEqual({ playlists: [] });
  });
});

describe("useExportWizard", () => {
  it("opens on the device step with the given stick and playlists", () => {
    useExportWizard.getState().open({ mountPoint: "/Volumes/REKS", playlists: [2] });
    const s = useExportWizard.getState();
    expect(s.step).toBe("device");
    expect(s.mountPoint).toBe("/Volumes/REKS");
    expect([...s.checked]).toEqual([2]);
    expect(s.options).toEqual(DEFAULT_EXPORT_OPTIONS);
  });

  it("does not go on until the step is complete", async () => {
    const w = useExportWizard.getState;
    w().open();
    await w().next(NODES);
    expect(w().step).toBe("device");
    w().setDevice("/Volumes/REKS");
    await w().next(NODES);
    expect(w().step).toBe("playlists");
    await w().next(NODES);
    expect(w().step).toBe("playlists");
    w().toggle(5);
    await w().next(NODES);
    expect(w().step).toBe("options");

    w().setOptions({ history: "discard" });
    await w().next(NODES);
    expect(w().step).toBe("options");
    expect(exportPlan).not.toHaveBeenCalled();
  });

  it("plans on entering the dry run with the selection and options", async () => {
    const w = useExportWizard.getState;
    exportPlan.mockResolvedValue(plan("ready"));
    w().open({ mountPoint: "/Volumes/REKS", playlists: [1, 2] });
    await w().next(NODES);
    await w().next(NODES);
    w().setOptions({ history: "discard", discardHistoryConfirmed: true, artwork: true });
    await w().next(NODES);
    expect(w().step).toBe("dryRun");
    expect(exportPlan).toHaveBeenCalledWith(
      "/Volumes/REKS",
      { playlists: [1] },
      {
        ...DEFAULT_EXPORT_OPTIONS,
        history: "discard",
        discardHistoryConfirmed: true,
        artwork: true,
      },
    );
    expect(w().dryRun?.status).toBe("ready");

    w().back();
    expect(w().step).toBe("options");
    expect(w().dryRun).toBeNull();
  });

  it("leaving discard withdraws the confirmation", () => {
    const w = useExportWizard.getState;
    w().setOptions({ history: "discard", discardHistoryConfirmed: true });
    w().setOptions({ history: "backUp" });
    expect(w().options.discardHistoryConfirmed).toBe(false);
  });

  it("keeps only the newest plan and reports failures", async () => {
    const w = useExportWizard.getState;
    let first: (d: DryRun) => void = () => {};
    exportPlan
      .mockReturnValueOnce(
        new Promise((resolve) => {
          first = resolve;
        }),
      )
      .mockResolvedValueOnce(plan("second"))
      .mockRejectedValueOnce("stick unplugged");
    w().open({ mountPoint: "/Volumes/REKS", playlists: [5] });
    const a = w().replan(NODES);
    await w().replan(NODES);
    first(plan("first"));
    await a;
    expect(w().dryRun?.status).toBe("second");

    await w().replan(NODES);
    expect(w().dryRun).toBeNull();
    expect(w().error).toBe("Could not plan: stick unplugged");
  });
});
