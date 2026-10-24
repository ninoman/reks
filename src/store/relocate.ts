/**
 * Relocate missing files (phase 1 task 15, D36): the folder was picked →
 * planning (a dry run in Rust, progress, cancellable) → the user's picks →
 * applying → report. Matching and the checks are Rust's; this keeps the
 * step, the picks and the dry run's id.
 */
import { create } from "zustand";

import {
  type RelocateEvent,
  type RelocateReport,
  type RelocateScan,
  type RelocateScope,
  relocateApply,
  relocateCancel,
  relocatePlan,
} from "../lib/ipc";
import { useLibrary } from "./library";
import { refreshAll } from "./panes";
import { usePlaylists } from "./playlists";
import { useSettings } from "./settings";

export type RelocateStep = "closed" | "planning" | "plan" | "applying" | "report" | "failed";

interface RelocateState {
  step: RelocateStep;
  scope: RelocateScope | null;
  folder: string | null;
  progress: RelocateEvent | null;
  scan: RelocateScan | null;
  /** Review track id → candidate path picked; absent = skip. */
  picks: Record<number, string>;
  addRoot: boolean;
  stopping: boolean;
  report: RelocateReport | null;
  error: string | null;

  /** Runs the dry run for `scope` in `folder` and opens the dialog. */
  start(scope: RelocateScope, folder: string): Promise<void>;
  /** Picks `path` for a review track; `null` skips it. */
  pick(trackId: number, path: string | null): void;
  setAddRoot(addRoot: boolean): void;
  apply(): Promise<void>;
  /** Stops the running dry run or apply (nothing is written). */
  cancel(): Promise<void>;
  close(): void;
}

const idle = {
  step: "closed" as RelocateStep,
  scope: null,
  folder: null,
  progress: null,
  scan: null,
  picks: {},
  addRoot: false,
  stopping: false,
  report: null,
  error: null,
};

/** Tracks the apply would relink: the sure matches and the picks. */
export function relinkCount(scan: RelocateScan, picks: Record<number, string>): number {
  return scan.plan.sure.length + Object.keys(picks).length;
}

let generation = 0;

export const useRelocate = create<RelocateState>()((set, get) => ({
  ...idle,

  async start(scope, folder) {
    const mine = ++generation;
    set({ ...idle, step: "planning", scope, folder });
    try {
      const scan = await relocatePlan(scope, folder, (progress) => {
        if (mine === generation) set({ progress });
      });
      if (mine !== generation) return;
      // Adding a library folder stays the user's tick.
      set({ step: "plan", scan, progress: null, stopping: false });
    } catch (e) {
      if (mine === generation) set({ step: "failed", error: String(e), progress: null });
    }
  },

  pick(trackId, path) {
    set((s) => {
      const picks = { ...s.picks };
      if (path === null) delete picks[trackId];
      else picks[trackId] = path;
      return { picks };
    });
  },

  setAddRoot(addRoot) {
    set({ addRoot });
  },

  async apply() {
    const { scan, picks, addRoot } = get();
    if (scan === null || get().step !== "plan") return;
    const mine = generation;
    set({ step: "applying", error: null, progress: null });
    try {
      const report = await relocateApply(
        scan.planId,
        { picks, addRoot: addRoot && !scan.plan.insideRoot },
        (progress) => {
          if (mine === generation) set({ progress });
        },
      );
      if (mine !== generation) return;
      if (report.cancelled) {
        set({ step: "plan", progress: null, stopping: false });
        return;
      }
      set({ step: "report", report, progress: null });
      refreshAll();
      await Promise.all([
        useLibrary.getState().loadFacets(),
        usePlaylists.getState().load(),
        useSettings.getState().loadRoots(),
      ]);
    } catch (e) {
      // A refused or stale apply leaves the dry run on screen to retry or close.
      if (mine === generation) set({ step: "plan", error: String(e), progress: null });
    }
  },

  async cancel() {
    set({ stopping: true });
    await relocateCancel();
  },

  close() {
    const { step } = get();
    generation++;
    if (step === "planning" || step === "applying") void relocateCancel();
    set({ ...idle });
  },
}));
