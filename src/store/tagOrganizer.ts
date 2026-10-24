/**
 * Tag organizer (D39): after My Tags were read from comments (XML import or
 * "Read from Comments…" in the My Tags drawer), or from "Clean up…", group
 * tags into categories and merge or delete redundant ones. Every edit goes
 * through `useTags.mutate` (reks-core validates it and the tree reloads);
 * this keeps the run's tag ids, the scope and the duplicate groups.
 */
import { create } from "zustand";

import {
  type CommentTagsPreview,
  commentTagsImport,
  commentTagsPreview,
  type TagId,
  type TagOp,
  tagsDuplicates,
} from "../lib/ipc";
import { refreshAll } from "./panes";
import { endOf } from "./tagOrganizerModel";
import { useTags } from "./tags";

/** `confirmRead`/`reading`: the "Read from Comments" question; `organize`: the organizer. */
export type OrganizerStage = "closed" | "confirmRead" | "reading" | "organize";

export interface TagOrganizerState {
  stage: OrganizerStage;
  /** The read's counts while asking; `null` while they load. */
  preview: CommentTagsPreview | null;
  /** Tags of the run that opened it; `null` when opened on all tags. */
  runIds: TagId[] | null;
  /** Show only `runIds` (when there are some). */
  onlyRun: boolean;
  /** `null` while loading. */
  duplicates: TagId[][] | null;
  error: string | null;

  /** Asks before reading every track's comment block. */
  startRead(): Promise<void>;
  confirmRead(): Promise<void>;
  /** `ids`: the run's tags; `null`: every tag. */
  open(ids: TagId[] | null): Promise<void>;
  setOnlyRun(only: boolean): void;
  loadDuplicates(): Promise<void>;
  moveTo(ids: TagId[], categoryId: TagId): Promise<void>;
  /** Creates a category and moves `ids` into it. */
  moveToNew(ids: TagId[], name: string): Promise<void>;
  merge(ids: TagId[], into: TagId): Promise<void>;
  remove(ids: TagId[]): Promise<void>;
  close(): void;
}

const idle = {
  stage: "closed" as OrganizerStage,
  preview: null,
  runIds: null,
  onlyRun: true,
  duplicates: null,
  error: null,
};

export const useTagOrganizer = create<TagOrganizerState>()((set, get) => {
  const mutate = (op: TagOp) => useTags.getState().mutate(op);

  return {
    ...idle,

    async startRead() {
      set({ ...idle, stage: "confirmRead" });
      try {
        const preview = await commentTagsPreview();
        if (get().stage === "confirmRead") set({ preview });
      } catch (e) {
        set({ error: String(e) });
      }
    },

    async confirmRead() {
      if (get().stage !== "confirmRead") return;
      set({ stage: "reading", error: null });
      try {
        const report = await commentTagsImport();
        refreshAll(true);
        await get().open(report.tagIds);
      } catch (e) {
        set({ stage: "confirmRead", error: String(e) });
      }
    },

    async open(ids) {
      set({ ...idle, stage: "organize", runIds: ids, onlyRun: ids !== null });
      await Promise.all([useTags.getState().load(), get().loadDuplicates()]);
    },

    setOnlyRun(only) {
      set({ onlyRun: only });
    },

    async loadDuplicates() {
      set({ duplicates: null, error: null });
      try {
        set({ duplicates: await tagsDuplicates() });
      } catch (e) {
        set({ error: String(e) });
      }
    },

    async moveTo(ids, categoryId) {
      for (const id of ids) {
        const tree = useTags.getState().tree;
        if (tree === null) return;
        const out = await mutate({
          type: "move",
          id,
          categoryId,
          position: endOf(tree, categoryId),
        });
        if (out === null) return;
      }
    },

    async moveToNew(ids, name) {
      const out = await mutate({ type: "createCategory", name });
      if (out?.createdId == null) return;
      await get().moveTo(ids, out.createdId);
    },

    async merge(ids, into) {
      const others = ids.filter((id) => id !== into);
      if (others.length === 0) return;
      if ((await mutate({ type: "merge", ids: others, into })) !== null)
        await get().loadDuplicates();
    },

    async remove(ids) {
      for (const id of ids) if ((await mutate({ type: "delete", id })) === null) break;
      await get().loadDuplicates();
    },

    close() {
      if (get().stage === "reading") return;
      useTags.getState().clearError();
      set(idle);
    },
  };
});
