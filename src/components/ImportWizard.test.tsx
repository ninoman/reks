// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { ImportPreview, ImportReport } from "../lib/ipc";

vi.mock("../lib/ipc", () => ({
  rbxmlPreview: vi.fn(),
  rbxmlImportStart: vi.fn(),
  rbxmlImportCancel: vi.fn(),
  rbxmlForget: vi.fn(),
  playlistsGet: vi.fn(),
  libraryFacets: vi.fn(),
  libraryQuery: vi.fn(),
  tagsGet: vi.fn(),
  tagsDuplicates: vi.fn(),
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
  commentTags: { tracks: 2562, names: 87, newNames: 81 },
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
  commentTags: null,
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
  it("starts with how to export from rekordbox, then picks the file", async () => {
    render(<ImportWizard />);
    useImportWizard.getState().begin();
    expect(await screen.findByText(/Export Collection in xml format/)).toBeTruthy();
    expect(ipc.rbxmlPreview).not.toHaveBeenCalled();

    vi.mocked(dialog.open).mockResolvedValue("/x/collection.xml");
    fireEvent.click(screen.getByRole("button", { name: "Choose file…" }));
    expect(await screen.findByText("3,062")).toBeTruthy();
    expect(ipc.rbxmlPreview).toHaveBeenCalledWith("/x/collection.xml", []);
  });

  it("cancelling the file picker stays on the instructions", async () => {
    render(<ImportWizard />);
    useImportWizard.getState().begin();
    vi.mocked(dialog.open).mockResolvedValue(null);
    fireEvent.click(await screen.findByRole("button", { name: "Choose file…" }));
    await vi.waitFor(() => expect(dialog.open).toHaveBeenCalled());
    expect(useImportWizard.getState().step).toBe("choose");
  });

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

  it("comment tags are offered, on by default, and the choice is sent", async () => {
    vi.mocked(ipc.rbxmlImportStart).mockResolvedValue(report);
    render(<ImportWizard />);
    await useImportWizard.getState().open("/x/collection.xml");
    const box = await screen.findByRole("checkbox", { name: /Import My Tags from Comments/ });
    expect(screen.getByText(/2,562 tracks, 87 tags \(81 new\)/)).toBeTruthy();
    expect((box as HTMLInputElement).checked).toBe(true);
    fireEvent.click(box);
    fireEvent.click(screen.getByRole("button", { name: "Import" }));
    await vi.waitFor(() =>
      expect(ipc.rbxmlImportStart).toHaveBeenCalledWith(
        "/x/collection.xml",
        [],
        false,
        expect.any(Function),
      ),
    );
  });

  it("no comment-tags choice when no comment has a tag block", async () => {
    vi.mocked(ipc.rbxmlPreview).mockResolvedValue({
      ...preview,
      commentTags: { tracks: 0, names: 0, newNames: 0 },
    });
    render(<ImportWizard />);
    await useImportWizard.getState().open("/x/collection.xml");
    await screen.findByText("3,062");
    expect(screen.queryByRole("checkbox")).toBeNull();
  });

  it("the report offers to organize the tags it read", async () => {
    render(<ImportWizard />);
    await useImportWizard.getState().open("/x/collection.xml");
    useImportWizard.setState({
      step: "report",
      report: { ...report, commentTags: { tracks: 2, created: 1, assigned: 3, tagIds: [7, 9] } },
    });
    expect(await screen.findByText("My Tags created / assigned")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Organize tags…" })).toBeTruthy();
  });

  it("a cancelled import says nothing was imported", async () => {
    render(<ImportWizard />);
    await useImportWizard.getState().open("/x/collection.xml");
    useImportWizard.setState({ step: "report", report: { ...report, cancelled: true } });
    expect(await screen.findByText("Cancelled: nothing was imported.")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Map folder and re-import…" })).toBeNull();
  });
});
