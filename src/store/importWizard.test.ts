import { beforeEach, describe, expect, it, vi } from "vitest";

import type { ImportEvent, ImportPreview, ImportReport } from "../lib/ipc";

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

const ipc = await import("../lib/ipc");
const rbxmlPreview = vi.mocked(ipc.rbxmlPreview);
const rbxmlImportStart = vi.mocked(ipc.rbxmlImportStart);
const rbxmlImportCancel = vi.mocked(ipc.rbxmlImportCancel);
const rbxmlForget = vi.mocked(ipc.rbxmlForget);
const { useLibrary } = await import("./library");
const { useImportWizard } = await import("./importWizard");

const previewOf = (over: Partial<ImportPreview> = {}): ImportPreview => ({
  total: 3,
  playlists: 2,
  folders: 1,
  playlistEntries: 4,
  withGrid: 3,
  withCues: 3,
  filesFound: 0,
  filesNotFound: 3,
  noLocation: 0,
  notAFile: 0,
  invalidLocation: 0,
  inLibrary: 0,
  missingRoot: "/Music/old",
  notFoundSample: ["/Music/old/a.mp3"],
  commentTags: { tracks: 0, names: 0, newNames: 0 },
  ...over,
});

const reportOf = (over: Partial<ImportReport> = {}): ImportReport => ({
  total: 3,
  matchedByPath: 0,
  matchedByLocation: 0,
  matchedByFallback: 0,
  added: 3,
  missing: 0,
  unmatched: [],
  warnings: [],
  beatgridRows: 3,
  cueRows: 10,
  playlistsCreated: 2,
  commentTags: null,
  playlistsMatched: 0,
  playlistEntries: 4,
  playlistEntriesSkipped: 0,
  missingRoot: null,
  elapsedMs: 1200,
  cancelled: false,
  ...over,
});

beforeEach(() => {
  vi.clearAllMocks();
  useImportWizard.getState().close();
  rbxmlPreview.mockResolvedValue(previewOf());
  vi.mocked(ipc.playlistsGet).mockResolvedValue({ nodes: [] });
  vi.mocked(ipc.libraryFacets).mockResolvedValue({
    genres: [],
    keys: [],
    colours: [],
    playlists: [],
  });
  vi.mocked(ipc.libraryQuery).mockResolvedValue({
    rows: [],
    total: 0,
    dividers: [],
    dividerView: false,
  });
});

describe("useImportWizard", () => {
  it("open previews the file without remaps", async () => {
    await useImportWizard.getState().open("/x/collection.xml");
    const s = useImportWizard.getState();
    expect(s.step).toBe("review");
    expect(rbxmlPreview).toHaveBeenCalledWith("/x/collection.xml", []);
    expect(s.preview?.filesNotFound).toBe(3);
  });

  it("adding and removing a remap re-runs the preview, newest mapping first", async () => {
    const w = useImportWizard.getState();
    await w.open("/x/c.xml");
    await w.addRemap("/Music/old", "/Volumes/new");
    await w.addRemap("/Music/old/sub", "/Volumes/sub");
    expect(rbxmlPreview).toHaveBeenLastCalledWith("/x/c.xml", [
      { from: "/Music/old/sub", to: "/Volumes/sub" },
      { from: "/Music/old", to: "/Volumes/new" },
    ]);
    await w.removeRemap(0);
    expect(rbxmlPreview).toHaveBeenLastCalledWith("/x/c.xml", [
      { from: "/Music/old", to: "/Volumes/new" },
    ]);
    expect(rbxmlPreview).toHaveBeenCalledTimes(4);
  });

  it("an older preview answer is dropped", async () => {
    let resolveFirst: (p: ImportPreview) => void = () => {};
    rbxmlPreview.mockReturnValueOnce(new Promise((r) => (resolveFirst = r)));
    rbxmlPreview.mockResolvedValueOnce(previewOf({ filesFound: 3, filesNotFound: 0 }));
    const w = useImportWizard.getState();
    const first = w.open("/x/c.xml");
    await w.addRemap("/Music/old", "/new");
    resolveFirst(previewOf());
    await first;
    expect(useImportWizard.getState().preview?.filesFound).toBe(3);
  });

  it("start streams progress, ends on the report and refreshes the library", async () => {
    const refresh = vi.spyOn(useLibrary.getState(), "refresh");
    let send: (e: ImportEvent) => void = () => {};
    let finish: (r: ImportReport) => void = () => {};
    rbxmlImportStart.mockImplementation((_path, _remap, _tags, onEvent) => {
      send = onEvent;
      return new Promise((r) => (finish = r));
    });
    const w = useImportWizard.getState();
    await w.open("/x/c.xml");
    const running = w.start();
    expect(useImportWizard.getState().step).toBe("importing");
    send({ type: "progress", stage: "files", done: 1, total: 3 });
    send({
      type: "unmatched",
      trackId: 1,
      name: null,
      artist: null,
      location: null,
      reason: { kind: "noLocation" },
    });
    expect(useImportWizard.getState().progress).toEqual({ stage: "files", done: 1, total: 3 });
    expect(useImportWizard.getState().unmatchedSoFar).toBe(1);
    finish(reportOf());
    await running;
    expect(useImportWizard.getState().step).toBe("report");
    expect(refresh).toHaveBeenCalled();
  });

  it("stop is sent only while files are read", async () => {
    rbxmlImportStart.mockImplementation((_p, _r, _t, onEvent) => {
      onEvent({ type: "progress", stage: "database", done: 1, total: 3 });
      return new Promise(() => {});
    });
    const w = useImportWizard.getState();
    await w.open("/x/c.xml");
    void w.start();
    await w.cancel();
    expect(rbxmlImportCancel).not.toHaveBeenCalled();
    useImportWizard.setState({ progress: { stage: "files", done: 1, total: 3 } });
    await w.cancel();
    expect(rbxmlImportCancel).toHaveBeenCalledOnce();
    expect(useImportWizard.getState().stopping).toBe(true);
    // The wizard can't be closed mid-import.
    w.close();
    expect(useImportWizard.getState().step).toBe("importing");
  });

  it("closing the wizard frees the backend's parsed XML", async () => {
    rbxmlPreview.mockResolvedValue(previewOf());
    await useImportWizard.getState().open("/x/c.xml");
    useImportWizard.getState().close();
    expect(rbxmlForget).toHaveBeenCalledOnce();
  });

  it("a failed import returns to review with the error", async () => {
    rbxmlImportStart.mockRejectedValue("disk full");
    const w = useImportWizard.getState();
    await w.open("/x/c.xml");
    await w.start();
    const s = useImportWizard.getState();
    expect(s.step).toBe("review");
    expect(s.error).toContain("disk full");
  });

  it("remapAndReview goes back to review with the new mapping in front", async () => {
    rbxmlImportStart.mockResolvedValue(reportOf({ missing: 3, missingRoot: "/Music/old" }));
    const w = useImportWizard.getState();
    await w.open("/x/c.xml");
    await w.addRemap("/A", "/B");
    await w.start();
    await w.remapAndReview("/Music/old", "/Volumes/new");
    const s = useImportWizard.getState();
    expect(s.step).toBe("review");
    expect(s.report).toBeNull();
    expect(s.remap).toEqual([
      { from: "/Music/old", to: "/Volumes/new" },
      { from: "/A", to: "/B" },
    ]);
    expect(rbxmlPreview).toHaveBeenLastCalledWith("/x/c.xml", s.remap);
  });
});
