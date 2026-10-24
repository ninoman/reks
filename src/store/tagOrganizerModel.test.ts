import { describe, expect, it } from "vitest";

import type { TagTree } from "../lib/ipc";
import {
  commentTagsText,
  defaultKeeper,
  endOf,
  groupsInScope,
  scopedCategories,
  tagIndex,
} from "./tagOrganizerModel";

const tree: TagTree = {
  categories: [
    {
      id: 1,
      name: "Genre",
      tags: [
        { id: 10, name: "Prog Trance", trackCount: 4 },
        { id: 11, name: "Deep", trackCount: 1 },
      ],
    },
    { id: 2, name: "Situation", tags: [] },
    {
      id: 3,
      name: "Imported",
      tags: [
        { id: 30, name: "prog-trance", trackCount: 9 },
        { id: 31, name: "DEEP", trackCount: 1 },
      ],
    },
  ],
};

describe("tag organizer model", () => {
  it("scopes categories to the run's tags", () => {
    expect(scopedCategories(tree, null)).toBe(tree.categories);
    const scoped = scopedCategories(tree, new Set([30, 11]));
    expect(scoped.map((c) => [c.id, c.tags.map((t) => t.id)])).toEqual([
      [1, [11]],
      [3, [30]],
    ]);
  });

  it("keeps duplicate groups that touch the scope and still exist", () => {
    const index = tagIndex(tree);
    const groups = [
      [10, 30],
      [11, 31],
      [99, 31],
    ];
    expect(groupsInScope(groups, index, null).map((g) => g.map((r) => r.tag.id))).toEqual([
      [10, 30],
      [11, 31],
    ]);
    expect(groupsInScope(groups, index, new Set([30])).map((g) => g.map((r) => r.tag.id))).toEqual([
      [10, 30],
    ]);
  });

  it("keeps the tag on most tracks, else the first", () => {
    const index = tagIndex(tree);
    const refs = (ids: number[]) => ids.flatMap((id) => index.get(id) ?? []);
    expect(defaultKeeper(refs([10, 30]))).toBe(30);
    expect(defaultKeeper(refs([11, 31]))).toBe(11);
    expect(defaultKeeper([])).toBeNull();
  });

  it("moves land after a category's last tag", () => {
    expect(endOf(tree, 1)).toBe(2);
    expect(endOf(tree, 2)).toBe(0);
  });

  it("describes the counts", () => {
    expect(commentTagsText({ tracks: 2562, names: 1, newNames: 0 })).toBe(
      "2,562 tracks, 1 tag (0 new)",
    );
  });
});
