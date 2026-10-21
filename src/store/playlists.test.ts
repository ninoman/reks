import { beforeEach, describe, expect, it, vi } from "vitest";

import type { PlaylistNode, PlaylistOutcome } from "../lib/ipc";

vi.mock("../lib/ipc", () => ({
  playlistsGet: vi.fn(),
  playlistsMutate: vi.fn(),
  libraryFacets: vi.fn(),
  libraryQuery: vi.fn(),
}));

const ipc = await import("../lib/ipc");
const playlistsGet = vi.mocked(ipc.playlistsGet);
const playlistsMutate = vi.mocked(ipc.playlistsMutate);
const { useLibrary } = await import("./library");
const { NEW_DIVIDER_LABEL, usePlaylists } = await import("./playlists");
const { initialTree } = await import("./playlistTree");

const set: PlaylistNode = {
  id: 7,
  parentId: null,
  name: "Warm-up",
  isFolder: false,
  depth: 0,
  position: 0,
  entries: 2,
  smart: false,
};
const done: PlaylistOutcome = { type: "done", createdId: null, added: 2, skipped: 0 };
const dups: PlaylistOutcome = {
  type: "duplicates",
  playlistId: 7,
  trackIds: [1],
  duplicateCount: 1,
  requested: 2,
};

beforeEach(() => {
  vi.clearAllMocks();
  playlistsGet.mockResolvedValue({ nodes: [set] });
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
  usePlaylists.setState({
    nodes: [set],
    tree: initialTree,
    pendingDuplicates: null,
    confirmDelete: null,
    dividerRename: null,
    error: null,
  });
});

describe("usePlaylists", () => {
  it("a mutation reloads the tree, the facets and the table", async () => {
    playlistsMutate.mockResolvedValue(done);
    const refresh = vi.spyOn(useLibrary.getState(), "refresh");
    await usePlaylists.getState().mutate({ type: "rename", id: 7, name: "Peak" });
    expect(playlistsGet).toHaveBeenCalledTimes(1);
    expect(ipc.libraryFacets).toHaveBeenCalled();
    expect(refresh).toHaveBeenCalledWith(true);
  });

  it("a core error is kept for display", async () => {
    playlistsMutate.mockRejectedValue("invalid name: must not be empty");
    expect(await usePlaylists.getState().mutate({ type: "rename", id: 7, name: "" })).toBeNull();
    expect(usePlaylists.getState().error).toBe("invalid name: must not be empty");
    expect(playlistsGet).not.toHaveBeenCalled();
  });

  it("adding tracks always asks first", async () => {
    playlistsMutate.mockResolvedValue(done);
    await usePlaylists.getState().addTracks(7, [1, 2]);
    expect(playlistsMutate).toHaveBeenCalledWith({
      type: "addTracks",
      playlistId: 7,
      trackIds: [1, 2],
      before: null,
      duplicates: "ask",
    });
    expect(usePlaylists.getState().pendingDuplicates).toBeNull();
  });

  it("duplicates wait for the user; the answer resends with that mode", async () => {
    playlistsMutate.mockResolvedValueOnce(dups);
    await usePlaylists.getState().addTracks(7, [1, 2]);
    const pending = usePlaylists.getState().pendingDuplicates;
    expect(pending?.playlistName).toBe("Warm-up");
    expect(pending?.outcome).toEqual(dups);
    expect(playlistsGet).not.toHaveBeenCalled();

    playlistsMutate.mockResolvedValueOnce(done);
    await usePlaylists.getState().resolveDuplicates("skip");
    expect(playlistsMutate).toHaveBeenLastCalledWith(
      expect.objectContaining({ type: "addTracks", trackIds: [1, 2], duplicates: "skip" }),
    );
    expect(usePlaylists.getState().pendingDuplicates).toBeNull();
  });

  it("deleting the shown playlist goes back to the collection", async () => {
    useLibrary.getState().showPlaylist(7, false);
    expect(useLibrary.getState().sort.column).toBe("playlistPosition");
    playlistsMutate.mockResolvedValue(done);
    playlistsGet.mockResolvedValue({ nodes: [] });
    usePlaylists.getState().requestDelete(set);
    await usePlaylists.getState().resolveDelete(true);
    expect(playlistsMutate).toHaveBeenCalledWith({ type: "delete", id: 7 });
    expect(useLibrary.getState().filters.playlistId).toBeNull();
    expect(useLibrary.getState().sort.column).toBe("title");
  });

  it("create puts the node next to the selection and starts renaming it", async () => {
    playlistsMutate.mockResolvedValue({ type: "done", createdId: 9, added: 0, skipped: 0 });
    playlistsGet.mockResolvedValue({
      nodes: [set, { ...set, id: 9, name: "New playlist", position: 1 }],
    });
    await usePlaylists.getState().create("playlist", set);
    expect(playlistsMutate).toHaveBeenCalledWith({
      type: "createPlaylist",
      parentId: null,
      name: "New playlist",
      position: 1,
    });
    expect(usePlaylists.getState().tree.renaming).toEqual({ id: 9, draft: "New playlist" });
  });

  it("insert divider selects the new row and renames it in place", async () => {
    playlistsMutate.mockResolvedValue(done);
    await usePlaylists.getState().insertDivider(7, 2);
    expect(playlistsMutate).toHaveBeenCalledWith({
      type: "insertDivider",
      playlistId: 7,
      position: 2,
      label: NEW_DIVIDER_LABEL,
    });
    expect([...useLibrary.getState().selection.selected]).toEqual([2]);
    expect(usePlaylists.getState().dividerRename).toEqual({
      playlistId: 7,
      position: 2,
      label: NEW_DIVIDER_LABEL,
      draft: NEW_DIVIDER_LABEL,
    });
  });

  it("a divider rename sends the draft; unchanged or cancelled sends nothing", async () => {
    playlistsMutate.mockResolvedValue(done);
    const p = usePlaylists.getState();
    p.startDividerRename(7, 3, "PEAK");
    await usePlaylists.getState().commitDividerRename();
    p.startDividerRename(7, 3, "PEAK");
    p.editDividerDraft("CLOSE");
    p.cancelDividerRename();
    await usePlaylists.getState().commitDividerRename();
    expect(playlistsMutate).not.toHaveBeenCalled();

    p.startDividerRename(7, 3, "PEAK");
    p.editDividerDraft("---- CLOSE ----");
    await usePlaylists.getState().commitDividerRename();
    expect(playlistsMutate).toHaveBeenCalledWith({
      type: "renameDivider",
      playlistId: 7,
      position: 3,
      label: "---- CLOSE ----",
    });
    expect(usePlaylists.getState().dividerRename).toBeNull();
  });

  it("a rejected label is kept for display", async () => {
    playlistsMutate.mockRejectedValue("invalid divider label: must not be empty");
    const p = usePlaylists.getState();
    p.startDividerRename(7, 0, "PEAK");
    p.editDividerDraft("  ");
    await usePlaylists.getState().commitDividerRename();
    expect(usePlaylists.getState().error).toBe("invalid divider label: must not be empty");
  });

  it("a drag of dividers only adds nothing", async () => {
    await usePlaylists.getState().addTracks(7, []);
    expect(playlistsMutate).not.toHaveBeenCalled();
  });
});
