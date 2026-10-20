// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { DeviceInfo, HistoryScan, PlannedSession } from "../lib/ipc";

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("../lib/ipc", () => ({
  devicesList: vi.fn(),
  historyScan: vi.fn(),
  historyImport: vi.fn(),
  historySessions: vi.fn(),
  historySession: vi.fn(),
  libraryQuery: vi.fn(),
  libraryFacets: vi.fn(),
}));

const ipc = await import("../lib/ipc");
const { useHistory } = await import("../store/history");
const { HistoryImportDialog } = await import("./HistoryImportDialog");

const stick: DeviceInfo = {
  mountPoint: "/Volumes/REKS",
  fs: { kind: "fat32" },
  fsName: "msdos",
  removable: true,
  totalBytes: 1,
  freeBytes: 1,
  label: "REKS",
  osName: "disk4s1",
};

function planned(
  name: string,
  action: PlannedSession["action"],
  defaultExclusion: PlannedSession["defaultExclusion"] = null,
): PlannedSession {
  return {
    sourceName: name,
    playedOn: "2026-10-11",
    fingerprint: `fp-${name}`,
    action,
    defaultExclusion,
    entries:
      action.kind === "skip"
        ? []
        : [
            {
              position: 0,
              kind: "track",
              trackId: 1,
              matchedBy: "manifest",
              raw: { stickPath: "/Contents/a.mp3", title: "A", artist: "B", durationMs: 1 },
            },
          ],
  };
}

const scan: HistoryScan = {
  planId: 7,
  source: { kind: "stick", path: "/Volumes/REKS" },
  plan: {
    source: "device_library",
    volumeSerial: "1234-ABCD",
    baselineDate: "2026-03-01",
    sessions: [
      planned("HISTORY 004", { kind: "skip", reason: "alreadyImported", sessionId: 4 }),
      planned("HISTORY 006", { kind: "new" }),
      planned("HISTORY 008", { kind: "new" }, "preBaseline"),
    ],
  },
};

beforeEach(() => {
  vi.mocked(ipc.devicesList).mockResolvedValue([stick]);
  vi.mocked(ipc.historyScan).mockResolvedValue(scan);
  vi.mocked(ipc.historyImport).mockResolvedValue({
    created: [6, 7],
    extended: [],
    skipped: 1,
    entries: 2,
    unmatched: 0,
  });
  vi.mocked(ipc.historySessions).mockResolvedValue([]);
  useHistory.setState({ importOpen: true, openId: null });
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("HistoryImportDialog", () => {
  it("scans a stick, shows Practice per new session and imports the choices", async () => {
    render(<HistoryImportDialog />);
    fireEvent.click(await screen.findByRole("button", { name: "Read history" }));
    expect(ipc.historyScan).toHaveBeenCalledWith({ kind: "stick", device: stick });

    const fresh = (await screen.findByLabelText("Practice: HISTORY 006")) as HTMLInputElement;
    const old = screen.getByLabelText("Practice: HISTORY 008") as HTMLInputElement;
    expect(fresh.checked).toBe(false);
    expect(old.checked).toBe(true);
    expect(screen.queryByLabelText("Practice: HISTORY 004")).toBeNull();
    expect(screen.getByText("Already imported")).toBeTruthy();

    fireEvent.click(fresh);
    fireEvent.click(screen.getByRole("button", { name: "Import 2 sessions" }));
    await waitFor(() =>
      expect(ipc.historyImport).toHaveBeenCalledWith(7, {
        excluded: { "fp-HISTORY 006": true },
      }),
    );
    expect(await screen.findByText(/Imported 2 new sessions/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Done" }));
    expect(useHistory.getState().importOpen).toBe(false);
  });

  it("a failed scan shows the error and stays on the source step", async () => {
    vi.mocked(ipc.historyScan).mockRejectedValue("export.pdb: damaged page");
    render(<HistoryImportDialog />);
    fireEvent.click(await screen.findByRole("button", { name: "Read history" }));
    expect((await screen.findByRole("alert")).textContent).toContain("damaged page");
    expect(screen.getByRole("button", { name: "Read history" })).toBeTruthy();
  });
});
