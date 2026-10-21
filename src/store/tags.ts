/**
 * My Tags (Zustand): the tag tree from `tags_get`, the manager dialog's open
 * state, and every edit through `tags_mutate`. Naming and uniqueness rules
 * live in reks-core; this forwards what the user did and shows what came
 * back. Assigning updates the loaded rows in place (selection and scroll stay
 * put, so several tags can be ticked in a row); deleting a tag re-queries,
 * since it may be part of the active filter.
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
import { useLibrary } from "./library";
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
    await edit(op, (tree) =>
      useLibrary
        .getState()
        .patchRowTags(op.trackIds, (tags) =>
          applyTags(tree, tags, op.tagIds, op.type === "assign"),
        ),
    );
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
    mutate(op) {
      return edit(op, () => {
        if (op.type !== "delete") return;
        // The deleted id may be in the active filter and is on loaded rows.
        const lib = useLibrary.getState();
        const known = new Set(get().tree?.categories.flatMap((c) => c.tags.map((t) => t.id)));
        const tagIds = lib.filters.tagIds.filter((id) => known.has(id));
        if (tagIds.length !== lib.filters.tagIds.length) lib.setFilters({ tagIds });
        else lib.refresh(true);
      });
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
      set({ managerOpen: true });
    },
    closeManager() {
      set({ managerOpen: false });
    },
  };
});
