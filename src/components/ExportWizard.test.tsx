// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { DeviceInfo, DryRun, PlaylistNode } from "../lib/ipc";

vi.mock("../lib/ipc", async (original) => ({
  ...(await original<typeof import("../lib/ipc")>()),
  devicesList: vi.fn(),
  deviceEject: vi.fn(),
  exportPlan: vi.fn(),
}));

const ipc = await import("../lib/ipc");
const devicesList = vi.mocked(ipc.devicesList);
const exportPlan = vi.mocked(ipc.exportPlan);
const { useExportWizard } = await import("../store/exportWizard");
const { usePlaylists } = await import("../store/playlists");
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
});

const NODES = [
  node(1, null, "Gigs", true, 0),
  node(2, 1, "Friday", false, 1),
  node(3, null, "Other", false, 0),
];

const group = { count: 0, bytes: 0, totalText: "None", files: [] };

function dryRun(over: Partial<DryRun> = {}): DryRun {
  return {
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
    playlists: [{ id: 2, name: "Friday", depth: 0, isFolder: false, detail: "12 tracks" }],
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
    },
    problems: [],
    warnings: [],
    ...over,
  };
}

beforeEach(() => {
  devicesList.mockReset().mockResolvedValue([stick]);
  exportPlan.mockReset();
  usePlaylists.setState({ nodes: NODES });
  useExportWizard.getState().close();
});
afterEach(cleanup);

const button = (name: string) => screen.getByRole("button", { name }) as HTMLButtonElement;

/** Opens the wizard and walks to the options step. */
async function toOptions() {
  useExportWizard.getState().open();
  render(<ExportWizard />);
  fireEvent.click(await screen.findByRole("radio", { name: "REKS" }));
  fireEvent.click(button("Next"));
  fireEvent.click(screen.getByRole("checkbox", { name: /Other/ }));
  fireEvent.click(button("Next"));
  await screen.findByText("Step 3 of 4: Options");
}

describe("ExportWizard", () => {
  it("needs a stick and a playlist before going on", async () => {
    useExportWizard.getState().open();
    render(<ExportWizard />);
    expect(screen.getByText("Step 1 of 4: Stick")).toBeTruthy();
    expect(button("Next").disabled).toBe(true);
    fireEvent.click(await screen.findByRole("radio", { name: "REKS" }));
    expect(button("Next").disabled).toBe(false);
    fireEvent.click(button("Next"));

    expect(screen.getByText("Step 2 of 4: Playlists")).toBeTruthy();
    expect(button("Next").disabled).toBe(true);
    // Ticking a folder ticks (and locks) what is under it.
    fireEvent.click(screen.getByRole("checkbox", { name: /Gigs/ }));
    const friday = screen.getByRole("checkbox", { name: /Friday/ }) as HTMLInputElement;
    expect(friday.checked).toBe(true);
    expect(friday.disabled).toBe(true);
    expect(button("Next").disabled).toBe(false);

    fireEvent.click(button("Back"));
    expect(screen.getByText("Step 1 of 4: Stick")).toBeTruthy();
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
    );
  });

  it("shows the dry run; Export stays disabled until writing is wired in", async () => {
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
    expect(button("Export").disabled).toBe(true);
    expect(screen.getByText(/Writing to the stick isn't available/)).toBeTruthy();

    fireEvent.click(button("Back"));
    expect(screen.getByText("Step 3 of 4: Options")).toBeTruthy();
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
});
