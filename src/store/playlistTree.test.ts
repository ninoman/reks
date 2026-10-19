import { describe, expect, it } from "vitest";

import type { PlaylistNode } from "../lib/ipc";
import {
  ancestors,
  dropTarget,
  initialTree,
  type TreeState,
  treeReducer,
  visibleNodes,
} from "./playlistTree";

function node(
  id: number,
  parentId: number | null,
  depth: number,
  position: number,
  isFolder = false,
): PlaylistNode {
  return { id, parentId, name: `n${id}`, isFolder, depth, position, entries: 0 };
}

// 1 folder { 2, 3 folder { 4 } }, 5
const nodes = [
  node(1, null, 0, 0, true),
  node(2, 1, 1, 0),
  node(3, 1, 1, 1, true),
  node(4, 3, 2, 0),
  node(5, null, 0, 1),
];
const ids = (ns: PlaylistNode[]) => ns.map((n) => n.id);

describe("treeReducer", () => {
  it("expands, collapses and toggles folders", () => {
    let s = treeReducer(initialTree, { type: "expand", id: 1 });
    expect([...s.expanded]).toEqual([1]);
    expect(treeReducer(s, { type: "expand", id: 1 })).toBe(s);
    s = treeReducer(s, { type: "toggle", id: 3 });
    expect([...s.expanded].sort()).toEqual([1, 3]);
    s = treeReducer(s, { type: "toggle", id: 1 });
    expect([...s.expanded]).toEqual([3]);
    s = treeReducer(s, { type: "collapse", id: 3 });
    expect(s.expanded.size).toBe(0);
  });

  it("opens and closes the Playlists root", () => {
    expect(initialTree.playlistsOpen).toBe(true);
    const closed = treeReducer(initialTree, { type: "togglePlaylists" });
    expect(closed.playlistsOpen).toBe(false);
    expect(treeReducer(closed, { type: "togglePlaylists" }).playlistsOpen).toBe(true);
  });

  it("selects a node or the collection, ending a rename", () => {
    let s = treeReducer(initialTree, { type: "startRename", id: 2, name: "n2" });
    s = treeReducer(s, { type: "select", id: 5 });
    expect(s.selectedId).toBe(5);
    expect(s.renaming).toBeNull();
    expect(treeReducer(s, { type: "select", id: null }).selectedId).toBeNull();
  });

  it("rename flow: start, edit, end", () => {
    let s = treeReducer(initialTree, { type: "editDraft", draft: "ignored" });
    expect(s.renaming).toBeNull();
    s = treeReducer(s, { type: "startRename", id: 2, name: "n2" });
    expect(s.renaming).toEqual({ id: 2, draft: "n2" });
    s = treeReducer(s, { type: "editDraft", draft: "Warm-up" });
    expect(s.renaming).toEqual({ id: 2, draft: "Warm-up" });
    s = treeReducer(s, { type: "endRename" });
    expect(s.renaming).toBeNull();
  });

  it("forgets ids that are gone after a reload", () => {
    const s: TreeState = {
      playlistsOpen: false,
      expanded: new Set([1, 3]),
      selectedId: 4,
      renaming: { id: 4, draft: "x" },
    };
    const after = treeReducer(s, { type: "treeLoaded", nodes: nodes.slice(0, 2) });
    expect([...after.expanded]).toEqual([1]);
    expect(after.selectedId).toBeNull();
    expect(after.renaming).toBeNull();
    const kept = treeReducer(s, { type: "treeLoaded", nodes });
    expect(kept.selectedId).toBe(4);
    expect(kept.renaming).toEqual({ id: 4, draft: "x" });
    expect(kept.playlistsOpen).toBe(false);
  });
});

describe("visibleNodes", () => {
  it("hides everything inside a collapsed folder", () => {
    expect(ids(visibleNodes(nodes, new Set()))).toEqual([1, 5]);
    expect(ids(visibleNodes(nodes, new Set([1])))).toEqual([1, 2, 3, 5]);
    expect(ids(visibleNodes(nodes, new Set([1, 3])))).toEqual([1, 2, 3, 4, 5]);
    // An expanded folder inside a collapsed one stays hidden.
    expect(ids(visibleNodes(nodes, new Set([3])))).toEqual([1, 5]);
  });
});

describe("tree helpers", () => {
  it("ancestors, nearest first", () => {
    expect(ancestors(nodes, 4)).toEqual([3, 1]);
    expect(ancestors(nodes, 5)).toEqual([]);
  });

  it("drop zones map to the move op's parent and current index", () => {
    const [f1, n2, f3, , n5] = nodes as [
      PlaylistNode,
      PlaylistNode,
      PlaylistNode,
      PlaylistNode,
      PlaylistNode,
    ];
    expect(dropTarget(nodes, n2, "before")).toEqual({ parentId: 1, position: 0 });
    expect(dropTarget(nodes, n2, "after")).toEqual({ parentId: 1, position: 1 });
    expect(dropTarget(nodes, f1, "into")).toEqual({ parentId: 1, position: 2 });
    expect(dropTarget(nodes, f3, "into")).toEqual({ parentId: 3, position: 1 });
    expect(dropTarget(nodes, n5, "after")).toEqual({ parentId: null, position: 2 });
    // "Into" a playlist is treated as "before" it.
    expect(dropTarget(nodes, n5, "into")).toEqual({ parentId: null, position: 1 });
  });
});
