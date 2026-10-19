/**
 * Playlist tree view state as a pure reducer: expanded folders, the selected
 * node (`null` = the whole collection) and rename-in-place. The tree itself
 * (`PlaylistNode[]`, depth first) comes from `playlists_get`.
 */

import type { DropZone } from "../lib/dnd";
import type { PlaylistNode } from "../lib/ipc";

export interface TreeState {
  expanded: ReadonlySet<number>;
  selectedId: number | null;
  renaming: { id: number; draft: string } | null;
}

export type TreeAction =
  | { type: "toggle"; id: number }
  | { type: "expand"; id: number }
  | { type: "collapse"; id: number }
  | { type: "select"; id: number | null }
  | { type: "startRename"; id: number; name: string }
  | { type: "editDraft"; draft: string }
  | { type: "endRename" }
  /** A fresh tree: forget ids that are gone. */
  | { type: "treeLoaded"; nodes: PlaylistNode[] };

export const initialTree: TreeState = { expanded: new Set(), selectedId: null, renaming: null };

export function treeReducer(state: TreeState, action: TreeAction): TreeState {
  switch (action.type) {
    case "toggle":
      return treeReducer(state, {
        type: state.expanded.has(action.id) ? "collapse" : "expand",
        id: action.id,
      });
    case "expand": {
      if (state.expanded.has(action.id)) return state;
      return { ...state, expanded: new Set(state.expanded).add(action.id) };
    }
    case "collapse": {
      if (!state.expanded.has(action.id)) return state;
      const expanded = new Set(state.expanded);
      expanded.delete(action.id);
      return { ...state, expanded };
    }
    case "select":
      return { ...state, selectedId: action.id, renaming: null };
    case "startRename":
      return { ...state, renaming: { id: action.id, draft: action.name } };
    case "editDraft":
      return state.renaming === null
        ? state
        : { ...state, renaming: { ...state.renaming, draft: action.draft } };
    case "endRename":
      return { ...state, renaming: null };
    case "treeLoaded": {
      const ids = new Set(action.nodes.map((n) => n.id));
      const expanded = new Set([...state.expanded].filter((id) => ids.has(id)));
      const selectedId =
        state.selectedId !== null && ids.has(state.selectedId) ? state.selectedId : null;
      const renaming =
        state.renaming !== null && ids.has(state.renaming.id) ? state.renaming : null;
      return { expanded, selectedId, renaming };
    }
  }
}

/** Nodes to draw: everything except what sits inside a collapsed folder. */
export function visibleNodes(
  nodes: readonly PlaylistNode[],
  expanded: ReadonlySet<number>,
): PlaylistNode[] {
  const out: PlaylistNode[] = [];
  let hideBelow: number | null = null;
  for (const n of nodes) {
    if (hideBelow !== null && n.depth > hideBelow) continue;
    hideBelow = n.isFolder && !expanded.has(n.id) ? n.depth : null;
    out.push(n);
  }
  return out;
}

/** Folders above `id`, nearest first (to reveal a node). */
export function ancestors(nodes: readonly PlaylistNode[], id: number): number[] {
  const byId = new Map(nodes.map((n) => [n.id, n]));
  const out: number[] = [];
  let parent = byId.get(id)?.parentId ?? null;
  while (parent !== null) {
    out.push(parent);
    parent = byId.get(parent)?.parentId ?? null;
  }
  return out;
}

/**
 * A drop on `target` as the `move` op's parent and index in the current
 * sibling list. Into a folder means at its end.
 */
export function dropTarget(
  nodes: readonly PlaylistNode[],
  target: PlaylistNode,
  zone: DropZone,
): { parentId: number | null; position: number } {
  if (zone === "into" && target.isFolder) {
    const children = nodes.filter((n) => n.parentId === target.id).length;
    return { parentId: target.id, position: children };
  }
  return {
    parentId: target.parentId,
    position: zone === "after" ? target.position + 1 : target.position,
  };
}
