// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type {
  Device,
  DeviceInfo,
  DryRun,
  ExportProgress,
  ExportReportView,
  PlaylistNode,
  ReviewIssue,
} from "../lib/ipc";

vi.mock("../lib/ipc", async (original) => ({
  ...(await original<typeof import("../lib/ipc")>()),
  devicesList: vi.fn(),
  deviceEject: vi.fn(),
  exportDefaultOptions: vi.fn(),
  exportPlan: vi.fn(),
  exportApply: vi.fn(),
  exportCancel: vi.fn(),
  profilesList: vi.fn(),
  profileIdentify: vi.fn(),
  profileSelection: vi.fn(),
  profileSetSelection: vi.fn(),
  profileSetTarget: vi.fn(),
  playlistsMutate: vi.fn(),
  playlistsGet: vi.fn(),
}));

const ipc = await import("../lib/ipc");
const devicesList = vi.mocked(ipc.devicesList);
const deviceEject = vi.mocked(ipc.deviceEject);
const exportPlan = vi.mocked(ipc.exportPlan);
const exportApply = vi.mocked(ipc.exportApply);
const exportCancel = vi.mocked(ipc.exportCancel);
const profileIdentify = vi.mocked(ipc.profileIdentify);
const profileSelection = vi.mocked(ipc.profileSelection);
const playlistsMutate = vi.mocked(ipc.playlistsMutate);
const { useConverter } = await import("../store/converter");
const { useSettings } = await import("../store/settings");
const { useExportWizard } = await import("../store/exportWizard");
const { usePlaylists } = await import("../store/playlists");
const { useProfiles } = await import("../store/profiles");
const { SticksDialog } = await import("./SticksDialog");

const stick: DeviceInfo = {
  mountPoint: "/Volumes/REKS",
  fs: { kind: "fat32" },
  fsName: "msdos",
  removable: true,
  totalBytes: 32_000_000_000,
  freeBytes: 18_400_000_000,
  label: "REKS",
  osName: "disk4s1",
};

const DEVICE: Device = {
  id: 1,
  volumeSerial: "4993-1CFB",
  label: "REKS",
  sizeBytes: 32_000_000_000,
  name: "Main",
  target: "Generic",
  createdAt: "2026-10-19T12:00:00.000Z",
  lastSyncedAt: null,
};

const node = (
  id: number,
  parentId: number | null,
  name: string,
  isFolder: boolean,
  depth: number,
): PlaylistNode => ({
  id,
  parentId,
  name,
  isFolder,
  depth,
  position: 0,
  entries: isFolder ? 0 : 12,
  smart: false,
});

const NODES = [
  node(1, null, "Gigs", true, 0),
  node(2, 1, "Friday", false, 1),
  node(3, null, "Other", false, 0),
];

const group = { count: 0, bytes: 0, totalText: "None", files: [] };

const OVERVIEW: DryRun["overview"] = {
  headline: "Ready to export to REKS",
  playlistsText: "1 playlist",
  copyText: "1 track to copy",
  copyDetail: "9.0 MB",
  spaceText: "18.4 GB free after export",
  spaceDetail: "",
  fits: true,
  tracksExported: 12,
  tracksTotal: 12,
  exportLabel: "Export",
};

const MISSING: ReviewIssue = {
  kind: "missing",
  title: "2 tracks are missing",
  detail:
    "Their files are gone or moved. They are left out of this export and stay in your playlists.",
  keptText: "2 tracks left out, still in your playlists",
  trackIds: [40, 41],
  tracks: [
    {
      trackId: 40,
      label: "Artist A – Track A",
      detail: "/music/a.aiff",
      playlists: [2],
      inText: "In: Friday",
    },
    {
      trackId: 41,
      label: "Artist C – Track C",
      detail: "/music/c.wav",
      playlists: [],
      inText: "In: Classics (smart)",
    },
  ],
  actions: [{ type: "findFiles" }, { type: "removeFromPlaylists" }],
};

function dryRun(over: Partial<DryRun> = {}): DryRun {
  return {
    planId: 7,
    ready: true,
    overview: OVERVIEW,
    issues: [],
    settings: ["Any player", "rekordbox analysis, then reks", "Cleans up unused files"],
    history: {
      atRisk: false,
      summary: "",
      policy: "backUp",
      guard: null,
    },
    problems: [],
    warnings: [],
    convertible: [],
    convertText: null,
    onelibraryRemoval: null,
    ...over,
  };
}

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

const backedUp = {
  title: "Play history backed up",
  text: "Before writing, reks copied 1 database file (4.1 KB) from the stick to the folder below.",
  folder:
    "/Users/dj/Library/Application Support/app.reks/stick-backups/4993-1CFB/2026-10-19T21-04-12Z",
  files: ["/PIONEER/rekordbox/export.pdb"],
};

function report(over: Partial<ExportReportView> = {}): ExportReportView {
  return {
    outcome: "success",
    title: "Export finished",
    status: "reks wrote the stick and read it back: it matches the plan.",
    mountPoint: "/Volumes/REKS",
    deviceName: "REKS",
    facts: [
      { label: "Tracks", value: "12 in the stick's database" },
      { label: "Library record", value: "12 tracks recorded as on this stick" },
    ],
    history: backedUp,
    problems: [],
    warnings: [],
    leftOut: [],
    ...over,
  };
}

/** `exportApply` that hands back its progress callback and a way to finish. */
function pendingExport() {
  const run = {
    send: (_: ExportProgress) => {},
    finish: (_: ExportReportView) => {},
  };
  exportApply.mockImplementation((_id, onEvent) => {
    run.send = (p) => act(() => onEvent(p));
    return new Promise((resolve) => {
      run.finish = resolve;
    });
  });
  return run;
}

beforeEach(() => {
  vi.mocked(ipc.exportDefaultOptions).mockReset().mockResolvedValue({
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
  });
  devicesList.mockReset().mockResolvedValue([stick]);
  deviceEject.mockReset().mockResolvedValue();
  exportPlan.mockReset();
  exportApply.mockReset();
  exportCancel.mockReset().mockResolvedValue();
  vi.mocked(ipc.profilesList).mockReset().mockResolvedValue([DEVICE]);
  profileIdentify.mockReset().mockResolvedValue({ type: "known", device: DEVICE });
  profileSelection.mockReset().mockResolvedValue({ playlists: [2] });
  vi.mocked(ipc.profileSetSelection).mockReset().mockResolvedValue();
  vi.mocked(ipc.profileSetTarget).mockReset();
  playlistsMutate.mockReset().mockResolvedValue({
    type: "done",
    createdId: null,
    added: 0,
    skipped: 0,
  });
  vi.mocked(ipc.playlistsGet).mockReset().mockResolvedValue({ nodes: NODES });
  useProfiles.setState({
    ticks: null,
    known: [DEVICE],
    mounted: {
      [stick.mountPoint]: { info: stick, match: { type: "known", device: DEVICE }, error: null },
    },
    selectedId: null,
    status: null,
    error: null,
  });
  usePlaylists.setState({ nodes: NODES });
  useExportWizard.setState({ step: "closed" });
  useExportWizard.getState().close();
});
afterEach(cleanup);

const button = (name: string) => screen.getByRole("button", { name }) as HTMLButtonElement;

/** Opens the review for the REKS stick and waits for the plan. */
async function toReview(plan: DryRun = dryRun()) {
  exportPlan.mockResolvedValue(plan);
  render(<SticksDialog />);
  await act(() => useExportWizard.getState().open(stick.mountPoint));
  await screen.findByText(plan.overview.headline);
}

describe("ExportWizard", () => {
  it("opens on the review with the stick's playlists, its target and the settings", async () => {
    await toReview();
    expect(screen.getByText("Sync Main")).toBeTruthy();
    expect(screen.getByText("Step 1 of 3: Review")).toBeTruthy();
    expect(vi.mocked(ipc.exportDefaultOptions)).toHaveBeenCalledWith("Generic");
    expect(exportPlan).toHaveBeenCalledWith(
      "/Volumes/REKS",
      { playlists: [2] },
      expect.objectContaining({ missingAnalysis: "generate", onelibrary: false }),
      1,
    );
    const summary = screen.getByLabelText("Summary");
    expect(within(summary).getByText("1 track to copy")).toBeTruthy();
    expect(within(summary).getByText("18.4 GB free after export")).toBeTruthy();
    const chips = screen.getByLabelText("Writing with");
    expect(within(chips).getByText("Cleans up unused files")).toBeTruthy();
    // Nothing to say about play history on a stick without any.
    expect(screen.queryByText(/play history/)).toBeNull();
    // No technical details in the review.
    expect(screen.queryByText(/Technical details/)).toBeNull();
    expect(button("Export").disabled).toBe(false);
  });

  it("says when the stick has no playlists ticked", async () => {
    profileSelection.mockResolvedValue({ playlists: [] });
    render(<SticksDialog />);
    await act(() => useExportWizard.getState().open(stick.mountPoint));
    expect(await screen.findByText("No playlists are ticked for Main.")).toBeTruthy();
    expect(exportPlan).not.toHaveBeenCalled();
    expect(button("Export").disabled).toBe(true);
  });

  it("left-out tracks don't block: the button counts them", async () => {
    await toReview(
      dryRun({
        issues: [MISSING],
        overview: {
          ...OVERVIEW,
          headline: "Ready to export · 2 tracks will be left out",
          exportLabel: "Export 10 of 12 tracks",
        },
      }),
    );
    const card = screen.getByRole("region", { name: "2 tracks are missing" });
    expect(within(card).getByText("Artist A – Track A")).toBeTruthy();
    expect(within(card).getByText("In: Friday · /music/a.aiff")).toBeTruthy();
    // A smart playlist's track has no checkbox.
    expect(within(card).getAllByRole("checkbox")).toHaveLength(1);
    expect(button("Export 10 of 12 tracks").disabled).toBe(false);
  });

  it("Keep as it is collapses a card without planning again", async () => {
    await toReview(dryRun({ issues: [MISSING] }));
    fireEvent.click(button("Keep as it is"));
    expect(screen.getByText("✓ 2 tracks left out, still in your playlists")).toBeTruthy();
    expect(exportPlan).toHaveBeenCalledTimes(1);
  });

  it("removes ticked tracks from this stick's playlists after a confirm, then plans again", async () => {
    await toReview(dryRun({ issues: [MISSING] }));
    exportPlan.mockResolvedValue(dryRun());
    fireEvent.click(button("Remove from playlists…"));
    expect(screen.getByText(/Remove 1 track from 1 playlist\?/)).toBeTruthy();
    fireEvent.click(button("Cancel"));
    expect(playlistsMutate).not.toHaveBeenCalled();

    fireEvent.click(button("Remove from playlists…"));
    fireEvent.click(button("Remove"));
    await waitFor(() =>
      expect(playlistsMutate).toHaveBeenCalledWith({
        type: "removeTracks",
        playlistIds: [2],
        trackIds: [40],
      }),
    );
    await waitFor(() => expect(exportPlan).toHaveBeenCalledTimes(2));
    expect(await screen.findByText("Ready to export to REKS")).toBeTruthy();
  });

  it("a fix in another dialog hides the review and plans again when it closes", async () => {
    const convert: ReviewIssue = {
      ...MISSING,
      kind: "unsupported",
      title: "1 track won't play on this stick's players",
      actions: [{ type: "convert", trackIds: [40] }, { type: "removeFromPlaylists" }],
    };
    await toReview(dryRun({ issues: [convert] }));
    fireEvent.click(button("Convert…"));
    expect(useConverter.getState().step).toBe("settings");
    expect(screen.queryByRole("dialog")).toBeNull();
    act(() => useConverter.setState({ step: "closed" }));
    await waitFor(() => expect(exportPlan).toHaveBeenCalledTimes(2));
    expect(screen.getByRole("dialog")).toBeTruthy();
  });

  it("Change… opens Settings › Export and plans again with the new settings", async () => {
    await toReview();
    fireEvent.click(button("Change…"));
    expect(useSettings.getState().dialogOpen).toBe(true);
    expect(useSettings.getState().dialogTab).toBe("export");
    act(() => useSettings.getState().setDialogOpen(false));
    await waitFor(() => expect(exportPlan).toHaveBeenCalledTimes(2));
    expect(vi.mocked(ipc.exportDefaultOptions)).toHaveBeenCalledTimes(2);
  });

  it("stops on unimported play history; Discard is behind More and needs its confirmation", async () => {
    const guard = {
      title: "2 sessions on this stick are not in your library",
      text: "Exporting erases them from the stick.",
      sessions: ["HISTORY 001: 2 plays", "HISTORY 002: 1 play"],
      chosen: false,
      canImport: true,
    };
    const blocked = dryRun({
      ready: false,
      overview: { ...OVERVIEW, headline: "Can't export to REKS yet" },
      history: { ...dryRun().history, atRisk: true, guard },
    });
    await toReview(blocked);
    expect(button("Export").disabled).toBe(true);
    expect(screen.queryByRole("button", { name: "Discard…" })).toBeNull();
    fireEvent.click(button("More"));
    fireEvent.click(button("Discard…"));
    const discard = button("Discard and continue");
    expect(discard.disabled).toBe(true);
    fireEvent.click(
      screen.getByRole("checkbox", { name: /erased from the stick without a backup/ }),
    );

    exportPlan.mockResolvedValue(dryRun());
    fireEvent.click(discard);
    await waitFor(() =>
      expect(exportPlan).toHaveBeenLastCalledWith(
        "/Volumes/REKS",
        { playlists: [2] },
        expect.objectContaining({
          history: "discard",
          historyChosen: true,
          discardHistoryConfirmed: true,
        }),
        1,
      ),
    );
  });

  it("asks to confirm deleting the stick's exportLibrary.db and plans again", async () => {
    const removal = {
      title: "The stick's OneLibrary library is deleted",
      text: "OneLibrary is off, so this export writes only the Device Library.",
      files: group,
      confirmText: "I understand exportLibrary.db is deleted.",
      confirmed: false,
    };
    await toReview(dryRun({ ready: false, onelibraryRemoval: removal }));
    fireEvent.click(screen.getByRole("checkbox", { name: removal.confirmText }));
    await waitFor(() =>
      expect(exportPlan).toHaveBeenLastCalledWith(
        "/Volumes/REKS",
        { playlists: [2] },
        expect.objectContaining({ onelibraryRemovalConfirmed: true }),
        1,
      ),
    );
  });

  it("a stick-wide problem blocks and says why", async () => {
    await toReview(
      dryRun({
        ready: false,
        overview: { ...OVERVIEW, headline: "Can't export to REKS yet" },
        problems: [
          {
            kind: "notEnoughSpace",
            title: "Not enough space: the export writes 9.0 MB, the stick has 10 B free.",
            items: [],
          },
        ],
      }),
    );
    expect(screen.getByRole("alert").textContent).toBe("Can't export to REKS yet");
    expect(screen.getByText(/Not enough space/)).toBeTruthy();
    expect(button("Export").disabled).toBe(true);
  });

  it("shows a planning error with a retry", async () => {
    exportPlan.mockRejectedValueOnce("stick gone");
    render(<SticksDialog />);
    await act(() => useExportWizard.getState().open(stick.mountPoint));
    expect(await screen.findByText(/Could not plan: stick gone/)).toBeTruthy();
    exportPlan.mockResolvedValue(dryRun());
    fireEvent.click(button("Try again"));
    expect(await screen.findByText("Ready to export to REKS")).toBeTruthy();
  });

  it("exports with progress, then shows the report and ejects", async () => {
    const run = pendingExport();
    await toReview();
    fireEvent.click(button("Export"));

    expect(screen.getByText("Step 2 of 3: Writing")).toBeTruthy();
    expect(exportApply).toHaveBeenCalledWith(7, expect.any(Function));
    run.send(progress({ detail: "Track 3 of 12: Alpha – First", done: 3, total: 12 }));
    expect(screen.getByRole("status").textContent).toBe("Writing tracks");
    const bar = screen.getByRole("progressbar", { name: "Export progress" }) as HTMLProgressElement;
    expect([bar.value, bar.max]).toEqual([3, 12]);

    // Escape doesn't close a running export.
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(screen.getByText("Step 2 of 3: Writing")).toBeTruthy();

    await act(async () => run.finish(report({ leftOut: [MISSING] })));
    expect(screen.getByText("Step 3 of 3: Done")).toBeTruthy();
    expect(screen.getByRole("status").textContent).toBe("Export finished");
    // Left-out tracks are repeated in the report.
    expect(screen.getByRole("region", { name: "2 tracks are missing" })).toBeTruthy();
    expect(screen.getByText(backedUp.folder)).toBeTruthy();

    fireEvent.click(button("Eject"));
    expect(deviceEject).toHaveBeenCalledWith("/Volumes/REKS");
    expect(await screen.findByText("REKS ejected. You can remove it.")).toBeTruthy();
    fireEvent.click(button("Close"));
    expect(useExportWizard.getState().step).toBe("closed");
  });

  it("cancels until the new database is being written", async () => {
    const run = pendingExport();
    await toReview();
    fireEvent.click(button("Export"));
    run.send(progress({ cancellable: true }));
    fireEvent.click(button("Cancel export"));
    expect(exportCancel).toHaveBeenCalledTimes(1);
    run.send(progress({ cancellable: false }));
    expect(button("Cancel export").disabled).toBe(true);
  });

  it("after problems, goes back to a fresh review", async () => {
    const run = pendingExport();
    await toReview();
    fireEvent.click(button("Export"));
    await act(async () =>
      run.finish(
        report({
          outcome: "problems",
          title: "Export finished with problems",
          problems: [{ kind: "trackFailed", title: "1 track could not be written", items: ["x"] }],
        }),
      ),
    );
    expect(screen.getByRole("alert").textContent).toBe("Export finished with problems");
    fireEvent.click(button("Back to review"));
    await waitFor(() => expect(exportPlan).toHaveBeenCalledTimes(2));
    expect(screen.getByText("Step 1 of 3: Review")).toBeTruthy();
  });

  it("Back to Sticks closes the export and opens the Sticks view", async () => {
    const { useDevices } = await import("../store/devices");
    await toReview();
    fireEvent.click(button("Back to Sticks"));
    expect(useExportWizard.getState().step).toBe("closed");
    expect(useDevices.getState().dialogOpen).toBe(true);
    useDevices.getState().setDialogOpen(false);
  });
});
