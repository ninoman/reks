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
}));

const ipc = await import("../lib/ipc");
const devicesList = vi.mocked(ipc.devicesList);
const deviceEject = vi.mocked(ipc.deviceEject);
const exportPlan = vi.mocked(ipc.exportPlan);
const exportApply = vi.mocked(ipc.exportApply);
const exportCancel = vi.mocked(ipc.exportCancel);
const profileIdentify = vi.mocked(ipc.profileIdentify);
const profileSelection = vi.mocked(ipc.profileSelection);
const profileSetSelection = vi.mocked(ipc.profileSetSelection);
const { useConverter } = await import("../store/converter");
const { useExportWizard } = await import("../store/exportWizard");
const { usePlaylists } = await import("../store/playlists");
const { useProfiles } = await import("../store/profiles");
const { ExportWizard } = await import("./ExportWizard");

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

function dryRun(over: Partial<DryRun> = {}): DryRun {
  return {
    planId: 7,
    ready: true,
    status: "Ready to export. Nothing has been written to the stick yet.",
    device: {
      mountPoint: "/Volumes/REKS",
      name: "REKS",
      fsText: "FAT32 (msdos)",
      spaceText: "18.4 GB free of 32.0 GB",
      volumeSerial: "4993-1CFB",
    },
    target: "Any player",
    summary: [{ label: "Tracks", value: "12: 12 to copy, 0 already on the stick" }],
    playlists: [
      { id: 2, name: "Friday", depth: 0, isFolder: false, smart: false, detail: "12 tracks" },
    ],
    copies: {
      count: 1,
      bytes: 9_000_000,
      totalText: "1 file, 9.0 MB",
      files: [{ path: "/Contents/A/B/a.mp3", from: "/music/a.mp3", sizeText: "9.0 MB" }],
    },
    deletes: group,
    space: {
      fits: true,
      totalBytes: 32_000_000_000,
      freeBytes: 18_400_000_000,
      writeBytes: 9_000_000,
      deleteBytes: 0,
      freeAfterBytes: 18_391_000_000,
      usedAfterBytes: 13_609_000_000,
      facts: [{ label: "Free after", value: "18.4 GB" }],
    },
    history: {
      atRisk: false,
      policy: "backUp",
      title: "No play history on this stick",
      text: "Players have not logged any sessions here, so the export loses nothing.",
      actionText: null,
      backups: group,
      guard: null,
    },
    problems: [],
    warnings: [],
    convertible: [],
    convertText: null,
    onelibrary: null,
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
    missingAnalysis: "skip",
    artwork: false,
    skipUnsupported: false,
    history: "backUp",
    discardHistoryConfirmed: false,
    historyChosen: false,
    dividers: true,
    activeLoops: true,
    anlzPreference: "rekordboxFirst",
    onelibrary: true,
    onelibraryRemovalConfirmed: false,
  });
  devicesList.mockReset().mockResolvedValue([stick]);
  deviceEject.mockReset().mockResolvedValue();
  exportPlan.mockReset();
  exportApply.mockReset();
  exportCancel.mockReset().mockResolvedValue();
  vi.mocked(ipc.profilesList).mockReset().mockResolvedValue([DEVICE]);
  profileIdentify.mockReset().mockResolvedValue({ type: "known", device: DEVICE });
  profileSelection.mockReset().mockResolvedValue({ playlists: [] });
  profileSetSelection.mockReset().mockResolvedValue();
  vi.mocked(ipc.profileSetTarget).mockReset();
  useProfiles.setState({ known: [], mounted: {}, selectedId: null, status: null, error: null });
  usePlaylists.setState({ nodes: NODES });
  useExportWizard.getState().close();
});
afterEach(cleanup);

const button = (name: string) => screen.getByRole("button", { name }) as HTMLButtonElement;

/** Picks the REKS stick and waits until it is identified (Next enabled). */
async function pickStick() {
  fireEvent.click(await screen.findByRole("radio", { name: "REKS" }));
  await waitFor(() => expect(button("Next").disabled).toBe(false));
}

/** Opens the wizard and walks to the options step. */
async function toOptions() {
  useExportWizard.getState().open();
  render(<ExportWizard />);
  await pickStick();
  fireEvent.click(button("Next"));
  await screen.findByText("Step 2 of 6: Playlists");
  fireEvent.click(screen.getByRole("checkbox", { name: /Other/ }));
  fireEvent.click(button("Next"));
  await screen.findByText("Step 3 of 6: Options");
  expect(profileSetSelection).toHaveBeenCalledWith(1, [3]);
}

describe("ExportWizard", () => {
  it("ticks the stick's saved playlists", async () => {
    profileSelection.mockResolvedValue({ playlists: [1] });
    useExportWizard.getState().open();
    render(<ExportWizard />);
    await pickStick();
    fireEvent.click(button("Next"));
    await screen.findByText("Step 2 of 6: Playlists");
    expect(screen.getByText(/Pick playlists and folders for Main/)).toBeTruthy();
    expect((screen.getByRole("checkbox", { name: /Gigs/ }) as HTMLInputElement).checked).toBe(true);
    expect((screen.getByRole("checkbox", { name: /Other/ }) as HTMLInputElement).checked).toBe(
      false,
    );
  });

  it("asks to name a stick it does not know before going on", async () => {
    profileIdentify.mockResolvedValue({
      type: "new",
      volumeSerial: "AAAA-0001",
      suggestedName: "REKS",
      prompt: "This stick is new to reks.",
      note: "Name it and tick the playlists it should carry.",
    });
    useExportWizard.getState().open();
    render(<ExportWizard />);
    fireEvent.click(await screen.findByRole("radio", { name: "REKS" }));
    expect(await screen.findByText("This stick is new to reks.")).toBeTruthy();
    expect(button("Next").disabled).toBe(true);
  });

  it("needs a stick and a playlist before going on", async () => {
    useExportWizard.getState().open();
    render(<ExportWizard />);
    expect(screen.getByText("Step 1 of 6: Stick")).toBeTruthy();
    expect(button("Next").disabled).toBe(true);
    await pickStick();
    expect(screen.getByText("Stick: Main")).toBeTruthy();
    fireEvent.click(button("Next"));

    expect(await screen.findByText("Step 2 of 6: Playlists")).toBeTruthy();
    expect(button("Next").disabled).toBe(true);
    // Ticking a folder ticks (and locks) what is under it.
    fireEvent.click(screen.getByRole("checkbox", { name: /Gigs/ }));
    const friday = screen.getByRole("checkbox", { name: /Friday/ }) as HTMLInputElement;
    expect(friday.checked).toBe(true);
    expect(friday.disabled).toBe(true);
    expect(button("Next").disabled).toBe(false);

    fireEvent.click(button("Back"));
    expect(screen.getByText("Step 1 of 6: Stick")).toBeTruthy();
  });

  it("Discard needs the explicit confirmation", async () => {
    exportPlan.mockResolvedValue(dryRun());
    await toOptions();
    expect(screen.queryByRole("checkbox", { name: /erased without a backup/ })).toBeNull();
    fireEvent.click(screen.getByRole("radio", { name: "Discard" }));
    const confirm = screen.getByRole("checkbox", { name: /erased without a backup/ });
    expect(button("Dry run").disabled).toBe(true);
    fireEvent.click(confirm);
    expect(button("Dry run").disabled).toBe(false);

    fireEvent.click(button("Dry run"));
    await screen.findByText(/Ready to export/);
    expect(exportPlan).toHaveBeenCalledWith(
      "/Volumes/REKS",
      { playlists: [3] },
      expect.objectContaining({ history: "discard", discardHistoryConfirmed: true }),
      1,
    );
  });

  it("stops on unimported play history until a choice, then plans again", async () => {
    const guard = {
      title: "2 sessions on this stick are not in your library",
      text: "Exporting erases them from the stick.",
      sessions: ["HISTORY 001: 2 plays", "HISTORY 002: 1 play"],
      chosen: false,
      canImport: true,
    };
    const blocked = dryRun({
      ready: false,
      status: "Blocked by 1 problem. Fix it or change the options; nothing has been written.",
      history: {
        atRisk: true,
        policy: "backUp",
        title: "This stick holds play history",
        text: "The stick's export.pdb holds 2 history sessions with 3 played tracks.",
        actionText: null,
        backups: group,
        guard,
      },
    });
    exportPlan.mockResolvedValueOnce(blocked).mockResolvedValue(
      dryRun({
        history: {
          ...blocked.history,
          policy: "import",
          actionText: "Before writing anything, reks imports 2 sessions (3 plays)…",
          guard: { ...guard, chosen: true },
        },
      }),
    );
    await toOptions();
    fireEvent.click(button("Dry run"));
    const section = await screen.findByRole("region", { name: guard.title });
    expect(within(section).getByText("HISTORY 001: 2 plays")).toBeTruthy();
    expect(button("Export").disabled).toBe(true);

    fireEvent.click(within(section).getByRole("button", { name: "Import now" }));
    await screen.findByText("You chose: Import now.");
    expect(exportPlan).toHaveBeenLastCalledWith(
      "/Volumes/REKS",
      { playlists: [3] },
      expect.objectContaining({ history: "import", historyChosen: true }),
      1,
    );
    expect(button("Export").disabled).toBe(false);
  });

  it("Continue and discard needs its own confirmation", async () => {
    exportPlan.mockResolvedValue(
      dryRun({
        ready: false,
        history: {
          atRisk: true,
          policy: "backUp",
          title: "This stick holds play history",
          text: "",
          actionText: null,
          backups: group,
          guard: {
            title: "1 session on this stick is not in your library",
            text: "",
            sessions: [],
            chosen: false,
            canImport: false,
          },
        },
      }),
    );
    await toOptions();
    fireEvent.click(button("Dry run"));
    const section = await screen.findByRole("region", {
      name: "1 session on this stick is not in your library",
    });
    expect(within(section).queryByRole("button", { name: "Import now" })).toBeNull();
    fireEvent.click(within(section).getByRole("button", { name: "Continue and discard…" }));
    const discard = within(section).getByRole("button", {
      name: "Discard and continue",
    }) as HTMLButtonElement;
    expect(discard.disabled).toBe(true);
    fireEvent.click(within(section).getByRole("checkbox", { name: /can't be imported later/ }));
    fireEvent.click(discard);
    await waitFor(() =>
      expect(exportPlan).toHaveBeenLastCalledWith(
        "/Volumes/REKS",
        { playlists: [3] },
        expect.objectContaining({
          history: "discard",
          historyChosen: true,
          discardHistoryConfirmed: true,
        }),
        1,
      ),
    );
  });

  it("playlist dividers and active loops are on by default, marked UNVERIFIED, and turn off", async () => {
    exportPlan.mockResolvedValue(dryRun());
    await toOptions();
    const toggle = screen.getByRole("switch", { name: "Playlist dividers" });
    expect(toggle.getAttribute("aria-checked")).toBe("true");
    // Dividers, active loops and OneLibrary each carry the badge.
    expect(screen.getAllByText("UNVERIFIED")).toHaveLength(3);
    const loops = screen.getByRole("switch", { name: "Active loops" });
    expect(loops.getAttribute("aria-checked")).toBe("true");
    fireEvent.click(toggle);
    fireEvent.click(loops);

    fireEvent.click(button("Dry run"));
    await screen.findByText(/Ready to export/);
    expect(exportPlan).toHaveBeenCalledWith(
      "/Volumes/REKS",
      { playlists: [3] },
      expect.objectContaining({ dividers: false, activeLoops: false }),
      1,
    );
  });

  it("asks to confirm deleting the stick's exportLibrary.db and plans again", async () => {
    const removal = {
      title: "The stick's OneLibrary library is deleted",
      text: "OneLibrary is off. Players that read only OneLibrary (OPUS-QUAD, XDJ-AZ) will find no library on this stick.",
      files: {
        count: 1,
        bytes: 3,
        totalText: "1 file, 3 B",
        files: [{ path: "/PIONEER/rekordbox/exportLibrary.db", from: null, sizeText: "3 B" }],
      },
      confirmText: "I understand exportLibrary.db is deleted.",
      confirmed: false,
    };
    exportPlan
      .mockResolvedValueOnce(dryRun({ ready: false, onelibraryRemoval: removal }))
      .mockResolvedValueOnce(dryRun({ onelibraryRemoval: { ...removal, confirmed: true } }));
    await toOptions();
    fireEvent.click(button("Dry run"));
    expect(await screen.findByText("The stick's OneLibrary library is deleted")).toBeTruthy();
    expect(screen.getByText(/OPUS-QUAD, XDJ-AZ/)).toBeTruthy();
    expect(screen.getByText("Delete: 1 file, 3 B")).toBeTruthy();
    fireEvent.click(screen.getByRole("checkbox", { name: /exportLibrary.db is deleted/ }));
    await screen.findByText(/Ready to export/);
    expect(exportPlan).toHaveBeenLastCalledWith(
      "/Volumes/REKS",
      { playlists: [3] },
      expect.objectContaining({ onelibraryRemovalConfirmed: true }),
      1,
    );
    const box = screen.getByRole("checkbox", { name: /exportLibrary.db is deleted/ });
    expect((box as HTMLInputElement).checked).toBe(true);
  });

  it("OneLibrary is on by default, marked UNVERIFIED, and its dry run lists the file", async () => {
    exportPlan.mockResolvedValue(
      dryRun({
        onelibrary: {
          title: "OneLibrary (experimental)",
          text: "Also writes exportLibrary.db for players that read OneLibrary.",
          write: {
            path: "/PIONEER/rekordbox/exportLibrary.db",
            from: null,
            sizeText: "size not known yet",
          },
          removes: {
            count: 1,
            bytes: 3,
            totalText: "1 file, 3 B",
            files: [{ path: "/PIONEER/rekordbox/exportExt.pdb", from: null, sizeText: "3 B" }],
          },
        },
      }),
    );
    await toOptions();
    const toggle = screen.getByRole("switch", { name: "OneLibrary" });
    expect(toggle.getAttribute("aria-checked")).toBe("true");
    expect(screen.getByText(/not supported by AlphaTheta/)).toBeTruthy();
    expect(screen.getByText(/has no OneLibrary key, so only export.pdb is written/)).toBeTruthy();

    fireEvent.click(button("Dry run"));
    await screen.findByText(/Ready to export/);
    expect(exportPlan).toHaveBeenCalledWith(
      "/Volumes/REKS",
      { playlists: [3] },
      expect.objectContaining({ onelibrary: true }),
      1,
    );
    expect(screen.getByText("OneLibrary (experimental)")).toBeTruthy();
    expect(screen.getByText(/Write \/PIONEER\/rekordbox\/exportLibrary.db/)).toBeTruthy();
    expect(screen.getByText("Remove: 1 file, 3 B")).toBeTruthy();
  });

  it("shows the dry run with the history backup", async () => {
    exportPlan.mockResolvedValue(
      dryRun({
        history: {
          atRisk: true,
          policy: "backUp",
          title: "This stick holds play history",
          text: "The stick's export.pdb holds 2 history sessions with 3 played tracks.",
          actionText: "Before writing anything, reks copies 1 database file (4.1 KB)…",
          backups: {
            count: 1,
            bytes: 4096,
            totalText: "1 file, 4.1 KB",
            files: [{ path: "/PIONEER/rekordbox/export.pdb", from: null, sizeText: "4.1 KB" }],
          },
          guard: null,
        },
        warnings: [{ kind: "skipped", title: "1 track left out", items: ["A – B: no analysis"] }],
      }),
    );
    await toOptions();
    fireEvent.click(button("Dry run"));
    expect((await screen.findByRole("status")).textContent).toMatch(/Ready to export/);

    expect(screen.getByText("This stick holds play history")).toBeTruthy();
    expect(screen.getByText(/reks copies 1 database file/)).toBeTruthy();
    expect(screen.getByText("Backed up: 1 file, 4.1 KB")).toBeTruthy();
    expect(screen.getByText("1 track left out")).toBeTruthy();
    expect(screen.getByText("A – B: no analysis")).toBeTruthy();
    expect(screen.getByText("Copy: 1 file, 9.0 MB")).toBeTruthy();
    expect(button("Export").disabled).toBe(false);

    fireEvent.click(button("Back"));
    expect(screen.getByText("Step 3 of 6: Options")).toBeTruthy();
  });

  it("offers the converter for tracks it fixes", async () => {
    exportPlan.mockResolvedValue(
      dryRun({
        convertible: [4, 9],
        convertText: "2 tracks can be converted to a format older players load.",
      }),
    );
    await toOptions();
    fireEvent.click(button("Dry run"));
    expect(await screen.findByText(/2 tracks can be converted/)).toBeTruthy();
    useConverter.getState().set({ target: "wav" });
    fireEvent.click(button("Convert…"));
    const c = useConverter.getState();
    expect(c.step).toBe("settings");
    expect(c.trackIds).toEqual([4, 9]);
    expect(c.settings).toMatchObject({ target: "aiff", rate: "keepMax48" });
    c.close();
  });

  it("a blocked plan says why and can't proceed", async () => {
    exportPlan.mockResolvedValue(
      dryRun({
        ready: false,
        status: "Blocked by 2 problems. Fix them or change the options; nothing has been written.",
        problems: [
          {
            kind: "sourceMissing",
            title: "1 track: file not found",
            items: ["Alpha – First: /music/gone.mp3"],
          },
          {
            kind: "notEnoughSpace",
            title: "Not enough space: the export writes 40.0 GB, the stick has 18.4 GB free.",
            items: [],
          },
        ],
      }),
    );
    await toOptions();
    fireEvent.click(button("Dry run"));
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toMatch(/^Blocked by 2 problems/);
    const problems = screen.getByRole("region", { name: "Must be fixed first" });
    expect(within(problems).getByText("1 track: file not found")).toBeTruthy();
    expect(within(problems).getByText("Alpha – First: /music/gone.mp3")).toBeTruthy();
    expect(within(problems).getByText(/^Not enough space/)).toBeTruthy();
    expect(button("Export").disabled).toBe(true);
  });

  it("shows a planning error with a retry", async () => {
    exportPlan.mockRejectedValueOnce("volume not found").mockResolvedValue(dryRun());
    await toOptions();
    fireEvent.click(button("Dry run"));
    expect((await screen.findByRole("alert")).textContent).toBe("Could not plan: volume not found");
    fireEvent.click(button("Try again"));
    await waitFor(() => expect(screen.getByText(/Ready to export/)).toBeTruthy());
  });

  it("exports with progress, then reports the backup folder and ejects", async () => {
    exportPlan.mockResolvedValue(dryRun());
    const run = pendingExport();
    await toOptions();
    fireEvent.click(button("Dry run"));
    await screen.findByText(/Ready to export/);
    fireEvent.click(button("Export"));

    expect(screen.getByText("Step 5 of 6: Writing")).toBeTruthy();
    expect(exportApply).toHaveBeenCalledWith(7, expect.any(Function));
    expect(screen.getByRole("status").textContent).toBe("Starting the export…");
    expect(screen.queryByRole("button", { name: "Back" })).toBeNull();
    run.send(
      progress({
        detail: "Track 3 of 12: Alpha – First",
        done: 3,
        total: 12,
        warnings: ["rekordbox is running; quit it while exporting so it does not touch the stick."],
      }),
    );
    expect(screen.getByRole("status").textContent).toBe("Writing tracks");
    expect(screen.getByText("Track 3 of 12: Alpha – First")).toBeTruthy();
    const bar = screen.getByRole("progressbar", { name: "Export progress" }) as HTMLProgressElement;
    expect([bar.value, bar.max]).toEqual([3, 12]);
    expect(screen.getByText(/rekordbox is running/)).toBeTruthy();

    // Escape doesn't close a running export.
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(screen.getByText("Step 5 of 6: Writing")).toBeTruthy();

    await act(async () => run.finish(report()));
    expect(screen.getByText("Step 6 of 6: Report")).toBeTruthy();
    expect(screen.getByRole("status").textContent).toBe("Export finished");
    expect(screen.getByText("12 tracks recorded as on this stick")).toBeTruthy();
    const history = screen.getByRole("region", { name: "Play history backed up" });
    expect(within(history).getByText(backedUp.folder)).toBeTruthy();
    expect(within(history).getByText("/PIONEER/rekordbox/export.pdb")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Plan again" })).toBeNull();

    fireEvent.click(button("Eject"));
    expect(deviceEject).toHaveBeenCalledWith("/Volumes/REKS");
    expect(await screen.findByText("REKS ejected. You can remove it.")).toBeTruthy();
    expect(button("Eject").disabled).toBe(true);

    fireEvent.click(button("Done"));
    expect(useExportWizard.getState().step).toBe("closed");
  });

  it("cancels until the new database is being written", async () => {
    exportPlan.mockResolvedValue(dryRun());
    const run = pendingExport();
    await toOptions();
    fireEvent.click(button("Dry run"));
    await screen.findByText(/Ready to export/);
    fireEvent.click(button("Export"));

    run.send(progress({ done: 1, total: 12 }));
    fireEvent.click(button("Cancel export"));
    await waitFor(() => expect(exportCancel).toHaveBeenCalledTimes(1));
    expect(button("Cancel export").disabled).toBe(true);
    expect(screen.getByText("Cancelling…")).toBeTruthy();

    await act(async () =>
      run.finish(
        report({
          outcome: "cancelled",
          title: "Export cancelled",
          status: "The stick keeps its previous database.",
          facts: [],
        }),
      ),
    );
    expect(screen.getByRole("status").textContent).toBe("Export cancelled");
    expect(screen.getByText("The stick keeps its previous database.")).toBeTruthy();

    // Plan again goes back to a fresh dry run.
    exportPlan.mockResolvedValue(dryRun({ planId: 8 }));
    fireEvent.click(button("Plan again"));
    await screen.findByText(/Ready to export/);
    expect(screen.getByText("Step 4 of 6: Dry run")).toBeTruthy();
    expect(exportPlan).toHaveBeenCalledTimes(2);
  });

  it("can't cancel once the database is being written", async () => {
    exportPlan.mockResolvedValue(dryRun());
    const run = pendingExport();
    await toOptions();
    fireEvent.click(button("Dry run"));
    await screen.findByText(/Ready to export/);
    fireEvent.click(button("Export"));
    run.send(
      progress({
        title: "Writing the stick's database and removing stale files",
        cancellable: false,
      }),
    );
    expect(button("Cancel export").disabled).toBe(true);
    // No count: an indeterminate bar.
    expect(screen.getByRole("progressbar").hasAttribute("value")).toBe(false);
  });

  it("reports problems and failures", async () => {
    exportPlan.mockResolvedValue(dryRun());
    exportApply.mockResolvedValueOnce(
      report({
        outcome: "problems",
        title: "Export finished with problems",
        problems: [
          {
            kind: "trackFailed",
            title: "1 track could not be written",
            items: ["Alpha – First: source unreadable"],
          },
          {
            kind: "databaseFinding",
            title: "Read back: 1 problem with the database",
            items: ["No export.pdb on the stick"],
          },
        ],
        warnings: [{ kind: "applyWarning", title: "rekordbox is running.", items: [] }],
      }),
    );
    await toOptions();
    fireEvent.click(button("Dry run"));
    await screen.findByText(/Ready to export/);
    fireEvent.click(button("Export"));

    expect((await screen.findByRole("alert")).textContent).toBe("Export finished with problems");
    const problems = screen.getByRole("region", { name: "Problems" });
    expect(within(problems).getByText("1 track could not be written")).toBeTruthy();
    expect(within(problems).getByText("Alpha – First: source unreadable")).toBeTruthy();
    expect(within(problems).getByText("No export.pdb on the stick")).toBeTruthy();
    expect(screen.getByRole("region", { name: "Warnings" }).textContent).toBe(
      "Warningsrekordbox is running.",
    );
    expect(button("Plan again")).toBeTruthy();
  });

  it("shows a refused export", async () => {
    exportPlan.mockResolvedValue(dryRun());
    exportApply.mockRejectedValueOnce("this dry run is out of date or was already exported");
    await toOptions();
    fireEvent.click(button("Dry run"));
    await screen.findByText(/Ready to export/);
    fireEvent.click(button("Export"));
    expect((await screen.findByRole("alert")).textContent).toBe(
      "Could not export: this dry run is out of date or was already exported",
    );
    expect(button("Plan again")).toBeTruthy();
  });
});
