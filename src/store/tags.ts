/**
 * My Tags (Zustand): the tag tree from `tags_get`, whether the My Tags drawer
 * is open, and every edit through `tags_mutate`. Naming and uniqueness rules
 * live in reks-core; this forwards what the user did and shows what came
 * back. Assigning updates the loaded rows in place (selection and scroll stay
 * put, so several tags can be ticked in a row); deleting or merging a tag
 * re-queries, since it may be part of the active filter, and so does any edit
 * that rewrote comments (write-to-Comments on, D36).
 */
import { create } from "zustand";

import {
  type TagId,
  type TagOp,
  type TagOutcome,
  type TagTree,
  tagsGet,
  tagsMutate,
} from "../lib/ipc";
import { usePanels } from "./panels";
import { allLibraries, refreshAll } from "./panes";
import { applyTags } from "./tagModel";

export type TagsStatus = "loading" | "ready" | "error";

export interface TagsState {
  tree: TagTree | null;
  status: TagsStatus;
  error: string | null;
  managerOpen: boolean;

  load(): Promise<void>;
  /** Sends `op`, then reloads the tree. `null` when it failed (the reason is in `error`). */
  mutate(op: TagOp): Promise<TagOutcome | null>;
  assign(trackIds: number[], tagIds: TagId[]): Promise<void>;
  unassign(trackIds: number[], tagIds: TagId[]): Promise<void>;
  /** Creates a tag and puts it on `trackIds` (the picker's "Create 'X'"). */
  createAndAssign(categoryId: TagId, name: string, trackIds: number[]): Promise<void>;
  clearError(): void;
  openManager(): void;
  closeManager(): void;
  toggleManager(): void;
}

export const useTags = create<TagsState>()((set, get) => {
  async function edit(op: TagOp, afterTree?: (tree: TagTree) => void): Promise<TagOutcome | null> {
    try {
      const outcome = await tagsMutate(op);
      set({ error: null });
      await get().load();
      const tree = get().tree;
      if (tree !== null) afterTree?.(tree);
      return outcome;
    } catch (e) {
      set({ error: String(e) });
      return null;
    }
  }

  async function change(op: Extract<TagOp, { type: "assign" | "unassign" }>) {
    if (op.trackIds.length === 0 || op.tagIds.length === 0) return;
    const outcome = await edit(op, (tree) => {
      for (const lib of allLibraries())
        lib
          .getState()
          .patchRowTags(op.trackIds, (tags) =>
            applyTags(tree, tags, op.tagIds, op.type === "assign"),
          );
    });
    if ((outcome?.commentsUpdated ?? 0) > 0) refreshAll(true);
  }

  return {
    tree: null,
    status: "loading",
    error: null,
    managerOpen: false,

    async load() {
      try {
        set({ tree: await tagsGet(), status: "ready" });
      } catch (e) {
        set({ status: "error", error: String(e) });
      }
    },
    async mutate(op) {
      const outcome = await edit(op);
      // A deleted or merged id is on loaded rows; the Filters drawer drops it from its rows (`rules.ts`).
      const stale = op.type === "delete" || op.type === "merge";
      if (outcome !== null && (stale || outcome.commentsUpdated > 0)) refreshAll(true);
      return outcome;
    },
    assign: (trackIds, tagIds) => change({ type: "assign", trackIds, tagIds }),
    unassign: (trackIds, tagIds) => change({ type: "unassign", trackIds, tagIds }),
    async createAndAssign(categoryId, name, trackIds) {
      const outcome = await edit({ type: "createTag", categoryId, name });
      if (outcome?.createdId == null) return;
      await get().assign(trackIds, [outcome.createdId]);
    },
    clearError() {
      set({ error: null });
    },
    openManager() {
      usePanels.setState({ filterBarOpen: false });
      set({ managerOpen: true });
    },
    closeManager() {
      set({ managerOpen: false });
    },
    toggleManager() {
      if (!get().managerOpen) usePanels.setState({ filterBarOpen: false });
      set((s) => ({ managerOpen: !s.managerOpen }));
    },
  };
});

// The two right-hand drawers share one spot: opening Filters closes My Tags.
usePanels.subscribe((s, prev) => {
  if (s.filterBarOpen && !prev.filterBarOpen) useTags.setState({ managerOpen: false });
});
