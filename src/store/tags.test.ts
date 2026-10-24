import { beforeEach, describe, expect, it, vi } from "vitest";

import type { TagTree, TrackRow } from "../lib/ipc";

vi.mock("../lib/ipc", () => ({
  tagsGet: vi.fn(),
  tagsMutate: vi.fn(),
  libraryFacets: vi.fn(),
  libraryQuery: vi.fn(),
}));

const ipc = await import("../lib/ipc");
const tagsGet = vi.mocked(ipc.tagsGet);
const tagsMutate = vi.mocked(ipc.tagsMutate);
const { useLibrary } = await import("./library");
const { useRules } = await import("./rules");
const { findRow, newGroup, newRow } = await import("./ruleTree");
const { useTags } = await import("./tags");

const tree: TagTree = {
  categories: [
    {
      id: 1,
      name: "Genre",
      tags: [
        { id: 10, name: "House", trackCount: 0 },
        { id: 11, name: "Techno", trackCount: 0 },
      ],
    },
  ],
};

const row = (id: number, tags: number[]) => ({ id, tags }) as TrackRow;

beforeEach(() => {
  vi.clearAllMocks();
  tagsGet.mockResolvedValue(tree);
  tagsMutate.mockResolvedValue({ createdId: null, changed: 1, commentsUpdated: 0 });
  vi.mocked(ipc.libraryQuery).mockResolvedValue({
    rows: [],
    total: 0,
    dividers: [],
    dividerView: false,
  });
  useTags.setState({ tree: null, status: "loading", error: null, managerOpen: false });
  useLibrary.setState({
    pages: new Map([[0, [row(1, [10]), row(2, []), row(3, [11])]]]),
  });
});

describe("tags store", () => {
  it("loads the tree", async () => {
    await useTags.getState().load();
    expect(useTags.getState().tree).toEqual(tree);
    expect(useTags.getState().status).toBe("ready");
  });

  it("keeps an error when loading fails", async () => {
    tagsGet.mockRejectedValue("db closed");
    await useTags.getState().load();
    expect(useTags.getState().status).toBe("error");
    expect(useTags.getState().error).toBe("db closed");
  });

  it("assign sends the op, reloads, and patches loaded rows in tree order", async () => {
    await useTags.getState().assign([1, 2], [11, 10]);
    expect(tagsMutate).toHaveBeenCalledWith({
      type: "assign",
      trackIds: [1, 2],
      tagIds: [11, 10],
    });
    expect(tagsGet).toHaveBeenCalled();
    const rows = useLibrary.getState().pages.get(0) ?? [];
    expect(rows.map((r) => r.tags)).toEqual([[10, 11], [10, 11], [11]]);
  });

  it("unassign removes the tag from the loaded rows", async () => {
    await useTags.getState().unassign([1, 3], [10, 11]);
    const rows = useLibrary.getState().pages.get(0) ?? [];
    expect(rows.map((r) => r.tags)).toEqual([[], [], []]);
  });

  it("sends nothing for no tracks or no tags", async () => {
    await useTags.getState().assign([], [10]);
    await useTags.getState().unassign([1], []);
    expect(tagsMutate).not.toHaveBeenCalled();
  });

  it("a rejected edit leaves the rows alone and shows the reason", async () => {
    tagsMutate.mockRejectedValue("unknown tag 99");
    expect(await useTags.getState().mutate({ type: "delete", id: 99 })).toBeNull();
    expect(useTags.getState().error).toBe("unknown tag 99");
    await useTags.getState().assign([1], [99]);
    expect(useLibrary.getState().pages.get(0)?.[0]?.tags).toEqual([10]);
  });

  it("createAndAssign creates the tag, then puts it on the tracks", async () => {
    tagsMutate.mockResolvedValueOnce({ createdId: 12, changed: 0, commentsUpdated: 0 });
    tagsGet.mockResolvedValue({
      categories: [
        {
          id: 1,
          name: "Genre",
          tags: [...(tree.categories[0]?.tags ?? []), { id: 12, name: "Deep", trackCount: 0 }],
        },
      ],
    });
    await useTags.getState().createAndAssign(1, "Deep", [2]);
    expect(tagsMutate).toHaveBeenNthCalledWith(1, {
      type: "createTag",
      categoryId: 1,
      name: "Deep",
    });
    expect(tagsMutate).toHaveBeenNthCalledWith(2, { type: "assign", trackIds: [2], tagIds: [12] });
    expect(useLibrary.getState().pages.get(0)?.[1]?.tags).toEqual([12]);
  });

  it("deleting a tag drops it from the drawer's rows", async () => {
    useTags.setState({ tree });
    const row = newRow({ field: "tag", category: 1, ids: [10, 11], all: false });
    useRules.setState({ root: newGroup("all", [row]) });
    tagsGet.mockResolvedValue({
      categories: [{ id: 1, name: "Genre", tags: [{ id: 11, name: "Techno", trackCount: 0 }] }],
    });
    await useTags.getState().mutate({ type: "delete", id: 10 });
    expect(findRow(useRules.getState().root, row.id)?.value).toMatchObject({ ids: [11] });
  });

  it("deleting a tag re-queries the loaded rows", async () => {
    const gen = useLibrary.getState().generation;
    await useTags.getState().mutate({ type: "delete", id: 10 });
    expect(useLibrary.getState().generation).toBeGreaterThan(gen);
  });
});
