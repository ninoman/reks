import { describe, expect, it } from "vitest";

import type { TagTree } from "../lib/ipc";
import {
  applyTags,
  clickAction,
  coverage,
  creatableIn,
  dropMove,
  searchTags,
  tagIndex,
} from "./tagModel";

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
    { id: 2, name: "Situation", tags: [{ id: 20, name: "Peak time", trackCount: 0 }] },
    { id: 3, name: "Empty", tags: [] },
  ],
};

describe("coverage", () => {
  it("is none for no tracks", () => {
    expect(coverage([], 10)).toBe("none");
  });
  it("is all / some / none by how many tracks have it", () => {
    expect(coverage([[10], [10, 11]], 10)).toBe("all");
    expect(coverage([[10], [11]], 10)).toBe("some");
    expect(coverage([[11], []], 10)).toBe("none");
  });
});

describe("clickAction", () => {
  it("removes only when every track has it; mixed and none add", () => {
    expect(clickAction("all")).toBe("unassign");
    expect(clickAction("some")).toBe("assign");
    expect(clickAction("none")).toBe("assign");
  });
});

describe("searchTags", () => {
  it("returns everything for a blank query", () => {
    expect(searchTags(tree, "  ")).toBe(tree.categories);
  });
  it("matches case-insensitively and drops empty categories", () => {
    const out = searchTags(tree, "PEAK");
    expect(out.map((c) => c.name)).toEqual(["Situation"]);
    expect(out[0]?.tags.map((t) => t.name)).toEqual(["Peak time"]);
  });
});

describe("creatableIn", () => {
  it("offers every category without that exact name", () => {
    expect(creatableIn(tree, "house").map((c) => c.name)).toEqual(["Situation", "Empty"]);
    expect(creatableIn(tree, "Deep").map((c) => c.name)).toEqual(["Genre", "Situation", "Empty"]);
    expect(creatableIn(tree, " ")).toEqual([]);
  });
});

describe("applyTags", () => {
  it("adds and removes in tree order, idempotently", () => {
    expect(applyTags(tree, [20], [11, 10], true)).toEqual([10, 11, 20]);
    expect(applyTags(tree, [10, 20], [10], true)).toEqual([10, 20]);
    expect(applyTags(tree, [10, 11, 20], [11, 99], false)).toEqual([10, 20]);
  });
});

describe("tagIndex", () => {
  it("maps tag ids to the tag and its category", () => {
    expect(tagIndex(tree).get(20)?.category.name).toBe("Situation");
    expect(tagIndex(tree).get(1)).toBeUndefined();
  });
});

describe("dropMove", () => {
  const tag = (id: number) => ({ kind: "tag", id }) as const;
  const cat = (id: number) => ({ kind: "category", id }) as const;

  it("moves a tag below the next one, counting itself in place", () => {
    expect(dropMove(tree, tag(10), tag(11), "after")).toEqual({
      type: "move",
      id: 10,
      categoryId: 1,
      position: 2,
    });
  });
  it("is a no-op where the tag already is", () => {
    expect(dropMove(tree, tag(10), tag(11), "before")).toBeNull();
    expect(dropMove(tree, tag(11), tag(10), "after")).toBeNull();
    expect(dropMove(tree, tag(10), tag(10), "before")).toBeNull();
  });
  it("moves a tag beside a tag of another category, or to the end of a category", () => {
    expect(dropMove(tree, tag(10), tag(20), "before")).toEqual({
      type: "move",
      id: 10,
      categoryId: 2,
      position: 0,
    });
    expect(dropMove(tree, tag(10), cat(3), "into")).toEqual({
      type: "move",
      id: 10,
      categoryId: 3,
      position: 0,
    });
  });
  it("reorders categories among themselves only", () => {
    expect(dropMove(tree, cat(1), cat(3), "after")).toEqual({ type: "move", id: 1, position: 3 });
    expect(dropMove(tree, cat(3), cat(1), "before")).toEqual({ type: "move", id: 3, position: 0 });
    expect(dropMove(tree, cat(1), cat(2), "before")).toBeNull();
    expect(dropMove(tree, cat(1), tag(20), "before")).toBeNull();
  });
});
