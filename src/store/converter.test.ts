import { beforeEach, describe, expect, it, vi } from "vitest";

import type { ConvertDryRun, ConvertEvent, ConvertReportView } from "../lib/ipc";

vi.mock("../lib/ipc", async (original) => ({
  ...(await original<typeof import("../lib/ipc")>()),
  convertPlan: vi.fn(),
  convertApply: vi.fn(),
  convertCancel: vi.fn(),
  libraryQuery: vi.fn(),
  libraryFacets: vi.fn(),
  playlistsGet: vi.fn(),
}));

const ipc = await import("../lib/ipc");
const convertPlan = vi.mocked(ipc.convertPlan);
const convertApply = vi.mocked(ipc.convertApply);
const convertCancel = vi.mocked(ipc.convertCancel);
vi.mocked(ipc.libraryQuery).mockResolvedValue({ rows: [], total: 0 } as never);
vi.mocked(ipc.libraryFacets).mockResolvedValue({
  genres: [],
  keys: [],
  colours: [],
  playlists: [],
} as never);
vi.mocked(ipc.playlistsGet).mockResolvedValue({ nodes: [] } as never);
const { DEFAULT_SETTINGS, presetOf, toRequest, useConverter } = await import("./converter");

const DRY_RUN: ConvertDryRun = {
  planId: 7,
  ready: true,
  status: "Ready to convert 1 file. Nothing has been written yet.",
  facts: [],
  items: [
    {
      trackId: 2,
      label: "A – Two",
      source: "/m/two.flac",
      dest: "/m/two.aiff",
      from: "FLAC · 24-bit · 96 kHz · stereo",
      to: "AIFF · 24-bit · 48 kHz · stereo",
      sizeText: "41.2 MB → 23.0 MB",
    },
  ],
  skipped: [{ trackId: 3, label: "B – Three", reason: "MP3 or AAC: only lossless files convert" }],
  warnings: [],
};

const REPORT: ConvertReportView = {
  outcome: "success",
  title: "Conversion finished",
  status: "Converted 1 of 1 file.",
  facts: [],
  converted: [],
  failed: [],
  skipped: [],
};

beforeEach(() => {
  vi.clearAllMocks();
  useConverter.setState({ step: "closed", hidden: false, settings: DEFAULT_SETTINGS });
});

describe("settings", () => {
  it("maps presets and the rate cap to the Rust request", () => {
    expect(presetOf(DEFAULT_SETTINGS)).toBe("olderPlayers");
    expect(toRequest(DEFAULT_SETTINGS)).toEqual({
      spec: { target: "aiff", depth: "keep", rate: "keep" },
      options: {
        destination: "nextToOriginal",
        library: "replace",
        originals: "keep",
        capRate48k: true,
      },
    });
    const custom = { ...DEFAULT_SETTINGS, rate: "hz44100" as const, folder: "/out" };
    expect(presetOf(custom)).toBeNull();
    expect(toRequest(custom).spec.rate).toBe("hz44100");
    expect(toRequest(custom).options.capRate48k).toBe(false);
    expect(toRequest(custom).options.destination).toEqual({ folder: "/out" });
  });

  it("keeps originals when adding as new", () => {
    const c = useConverter.getState();
    c.open([1]);
    c.set({ originals: "trash" });
    c.set({ library: "addAsNew" });
    expect(useConverter.getState().settings.originals).toBe("keep");
    expect(
      toRequest({ ...DEFAULT_SETTINGS, library: "addAsNew", originals: "trash" }).options,
    ).toMatchObject({ originals: "keep" });
  });

  it("opens with a preset and drops duplicate ids", () => {
    useConverter.getState().open([4, 4, 5], "saveSpace");
    const s = useConverter.getState();
    expect(s.step).toBe("settings");
    expect(s.trackIds).toEqual([4, 5]);
    expect(presetOf(s.settings)).toBe("saveSpace");
  });
});

describe("plan and convert", () => {
  it("plans, converts with progress, reports and refreshes the library", async () => {
    convertPlan.mockResolvedValue(DRY_RUN);
    let send: (e: ConvertEvent) => void = () => {};
    let finish: (r: ConvertReportView) => void = () => {};
    convertApply.mockImplementation((_id, onEvent) => {
      send = onEvent;
      return new Promise((resolve) => {
        finish = resolve;
      });
    });
    const c = useConverter.getState();
    c.open([2, 3]);
    await c.plan();
    expect(convertPlan).toHaveBeenCalledWith(
      [2, 3],
      { target: "aiff", depth: "keep", rate: "keep" },
      expect.objectContaining({ capRate48k: true }),
    );
    expect(useConverter.getState().dryRun).toEqual(DRY_RUN);

    const running = c.start();
    expect(useConverter.getState().step).toBe("converting");
    expect(convertApply).toHaveBeenCalledWith(7, expect.any(Function));
    send({ type: "started", trackId: 2, index: 0, total: 1 });
    send({ type: "progress", trackId: 2, fraction: 0.5 });
    expect(useConverter.getState().progress).toMatchObject({
      index: 0,
      total: 1,
      fraction: 0.5,
      label: "A – Two",
    });

    // Closing while converting only hides the dialog.
    c.close();
    expect(useConverter.getState()).toMatchObject({ step: "converting", hidden: true });

    await c.cancel();
    expect(convertCancel).toHaveBeenCalledTimes(1);
    expect(useConverter.getState().stopping).toBe(true);

    finish(REPORT);
    await running;
    expect(useConverter.getState()).toMatchObject({ step: "report", report: REPORT });
    expect(ipc.libraryFacets).toHaveBeenCalled();
  });

  it("a stale plan goes back to the dry run with the error", async () => {
    convertPlan.mockResolvedValue(DRY_RUN);
    convertApply.mockRejectedValue("this dry run is out of date");
    const c = useConverter.getState();
    c.open([2]);
    await c.plan();
    await c.start();
    expect(useConverter.getState()).toMatchObject({
      step: "dryRun",
      dryRun: null,
      error: "this dry run is out of date",
    });
  });

  it("a newer plan wins over an older answer", async () => {
    let first: (d: ConvertDryRun) => void = () => {};
    convertPlan.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          first = resolve;
        }),
    );
    convertPlan.mockResolvedValueOnce({ ...DRY_RUN, planId: 8 });
    const c = useConverter.getState();
    c.open([2]);
    const older = c.plan();
    await c.plan();
    first(DRY_RUN);
    await older;
    expect(useConverter.getState().dryRun?.planId).toBe(8);
  });
});
