/**
 * Change a library folder (phase 1 task 15, D37): the new folder was picked →
 * dry run (counts, examples, a sample of files at the new place) → apply →
 * report. The check and the rewrite are Rust's; the apply re-checks.
 */
import { create } from "zustand";

import {
  type LibraryRoot,
  type RemapPlan,
  type RemapReport,
  remapApply,
  remapPlan,
} from "../lib/ipc";
import { useLibrary } from "./library";
import { refreshAll } from "./panes";
import { usePlaylists } from "./playlists";
import { useScan } from "./scan";
import { useSettings } from "./settings";

export type RemapStep = "closed" | "planning" | "plan" | "applying" | "report" | "failed";

interface RemapState {
  step: RemapStep;
  root: LibraryRoot | null;
  folder: string | null;
  plan: RemapPlan | null;
  report: RemapReport | null;
  error: string | null;

  start(root: LibraryRoot, folder: string): Promise<void>;
  apply(): Promise<void>;
  close(): void;
}

const idle = {
  step: "closed" as RemapStep,
  root: null,
  folder: null,
  plan: null,
  report: null,
  error: null,
};

/** Why the change cannot be applied, or `null`. Rust refuses the same cases. */
export function remapBlocker(plan: RemapPlan): string | null {
  if (plan.collisions > 0) {
    return `${plan.collisions} tracks would get a path that another track already has.`;
  }
  if (plan.sampled > 0 && plan.sampleFound === 0) {
    return `None of the ${plan.sampled} files checked exist in the new folder. Is the drive connected, and is this the right folder?`;
  }
  return null;
}

let generation = 0;

export const useRemap = create<RemapState>()((set, get) => ({
  ...idle,

  async start(root, folder) {
    const mine = ++generation;
    set({ ...idle, step: "planning", root, folder });
    try {
      const plan = await remapPlan(root.id, folder);
      if (mine === generation) set({ step: "plan", plan });
    } catch (e) {
      if (mine === generation) set({ step: "failed", error: String(e) });
    }
  },

  async apply() {
    const { root, folder, plan } = get();
    if (root === null || folder === null || plan === null || get().step !== "plan") return;
    const mine = generation;
    set({ step: "applying", error: null });
    try {
      const report = await remapApply(root.id, folder);
      if (mine !== generation) return;
      set({ step: "report", report });
      refreshAll();
      await Promise.all([
        useSettings.getState().loadRoots(),
        useLibrary.getState().loadFacets(),
        usePlaylists.getState().load(),
      ]);
      // Clears "missing" on tracks whose files are at the new place.
      void useScan.getState().scan([plan.newPath]);
    } catch (e) {
      if (mine === generation) set({ step: "plan", error: String(e) });
    }
  },

  close() {
    generation++;
    set({ ...idle });
  },
}));
