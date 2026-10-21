// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { ImportPreview, ImportReport } from "../lib/ipc";

vi.mock("../lib/ipc", () => ({
  rbxmlPreview: vi.fn(),
  rbxmlImportStart: vi.fn(),
  rbxmlImportCancel: vi.fn(),
  playlistsGet: vi.fn(),
  libraryFacets: vi.fn(),
  libraryQuery: vi.fn(),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

const ipc = await import("../lib/ipc");
const dialog = await import("@tauri-apps/plugin-dialog");
const { useImportWizard } = await import("../store/importWizard");
const { ImportWizard } = await import("./ImportWizard");

const preview: ImportPreview = {
  total: 3062,
  playlists: 40,
  folders: 5,
  playlistEntries: 900,
  withGrid: 3000,
  withCues: 2900,
  filesFound: 2646,
  filesNotFound: 416,
  noLocation: 0,
  notAFile: 0,
  invalidLocation: 0,
  inLibrary: 0,
  missingRoot: "/Music/old",
  notFoundSample: ["/Music/old/a.mp3"],
};

const report: ImportReport = {
  total: 3,
  matchedByPath: 0,
  matchedByLocation: 0,
  matchedByFallback: 0,
  added: 3,
  missing: 1,
  unmatched: [
    {
      trackId: 1,
      name: "Gone",
      artist: "DJ X",
      location: null,
      reason: { kind: "fileNotFound", path: "/Music/old/gone.mp3" },
    },
    {
      trackId: 2,
      name: "Stream",
      artist: null,
      location: "file://localhostsoundcloud:1",
      reason: { kind: "notAFile" },
    },
  ],
  warnings: [],
  beatgridRows: 3,
  cueRows: 10,
  playlistsCreated: 2,
  tagsCreated: 0,
  tagsAssigned: 0,
  playlistsMatched: 0,
  playlistEntries: 4,
  playlistEntriesSkipped: 0,
  missingRoot: "/Music/old",
  elapsedMs: 1200,
  cancelled: false,
};

beforeEach(() => {
  vi.clearAllMocks();
  useImportWizard.getState().close();
  vi.mocked(ipc.rbxmlPreview).mockResolvedValue(preview);
});
afterEach(cleanup);

describe("ImportWizard", () => {
  it("review shows the counts and maps the missing folder", async () => {
    render(<ImportWizard />);
    await useImportWizard.getState().open("/x/collection.xml");
    expect(await screen.findByText("3,062")).toBeTruthy();
    expect(screen.getByText(/416 files not found in \/Music\/old/)).toBeTruthy();

    vi.mocked(dialog.open).mockResolvedValue("/Volumes/new");
    fireEvent.click(screen.getByRole("button", { name: "Map folder…" }));
    await vi.waitFor(() =>
      expect(ipc.rbxmlPreview).toHaveBeenLastCalledWith("/x/collection.xml", [
        { from: "/Music/old", to: "/Volumes/new" },
      ]),
    );
    expect(screen.getByRole("button", { name: "Remove mapping of /Music/old" })).toBeTruthy();
  });

  it("no Map folder button when every file is found", async () => {
    vi.mocked(ipc.rbxmlPreview).mockResolvedValue({
      ...preview,
      filesFound: 3062,
      filesNotFound: 0,
      missingRoot: null,
      notFoundSample: [],
    });
    render(<ImportWizard />);
    await useImportWizard.getState().open("/x/collection.xml");
    // Tracks and files found are both 3,062.
    expect(await screen.findAllByText("3,062")).toHaveLength(2);
    expect(screen.queryByRole("button", { name: "Map folder…" })).toBeNull();
  });

  it("the report groups unmatched tracks by reason", async () => {
    render(<ImportWizard />);
    await useImportWizard.getState().open("/x/collection.xml");
    useImportWizard.setState({ step: "report", report });
    expect(await screen.findByText("File not found (stored as missing): 1")).toBeTruthy();
    expect(screen.getByText("Not a local file (skipped): 1")).toBeTruthy();
    expect(screen.getByText("/Music/old/gone.mp3")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Map folder and re-import…" })).toBeTruthy();
  });

  it("a cancelled import says nothing was imported", async () => {
    render(<ImportWizard />);
    await useImportWizard.getState().open("/x/collection.xml");
    useImportWizard.setState({ step: "report", report: { ...report, cancelled: true } });
    expect(await screen.findByText("Cancelled: nothing was imported.")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Map folder and re-import…" })).toBeNull();
  });
});
