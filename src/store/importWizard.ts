/**
 * rekordbox XML import wizard: review (counts from a read-only preview, map
 * moved folders) → importing (progress, stop while files are read) → report.
 * Every remap change re-runs the preview; a newer preview drops older answers.
 */
import { create } from "zustand";

import {
  type ImportPreview,
  type ImportReport,
  type PrefixRemap,
  rbxmlImportCancel,
  rbxmlImportStart,
  rbxmlPreview,
} from "../lib/ipc";
import { useLibrary } from "./library";
import { refreshAll } from "./panes";
import { usePlaylists } from "./playlists";

export type WizardStep = "closed" | "review" | "importing" | "report";

export interface ImportProgress {
  stage: "files" | "database";
  done: number;
  total: number;
}

interface ImportWizardState {
  step: WizardStep;
  path: string | null;
  /** Tried in order; the newest first, so a later, narrower mapping wins. */
  remap: PrefixRemap[];
  preview: ImportPreview | null;
  previewing: boolean;
  progress: ImportProgress | null;
  /** Unmatched tracks reported so far by the running import. */
  unmatchedSoFar: number;
  stopping: boolean;
  report: ImportReport | null;
  error: string | null;

  open(path: string): Promise<void>;
  addRemap(from: string, to: string): Promise<void>;
  removeRemap(index: number): Promise<void>;
  start(): Promise<void>;
  cancel(): Promise<void>;
  /** From the report: map a folder and go back to review with fresh counts. */
  remapAndReview(from: string, to: string): Promise<void>;
  close(): void;
}

const idle = {
  step: "closed" as WizardStep,
  path: null,
  remap: [],
  preview: null,
  previewing: false,
  progress: null,
  unmatchedSoFar: 0,
  stopping: false,
  report: null,
  error: null,
};

let previewGeneration = 0;

export const useImportWizard = create<ImportWizardState>((set, get) => {
  async function runPreview() {
    const { path, remap } = get();
    if (path === null) return;
    const generation = ++previewGeneration;
    set({ previewing: true, error: null });
    try {
      const preview = await rbxmlPreview(path, remap);
      if (generation === previewGeneration) set({ preview, previewing: false });
    } catch (e) {
      if (generation === previewGeneration) {
        set({ preview: null, previewing: false, error: String(e) });
      }
    }
  }

  return {
    ...idle,

    async open(path) {
      set({ ...idle, step: "review", path });
      await runPreview();
    },

    async addRemap(from, to) {
      set({ remap: [{ from, to }, ...get().remap] });
      await runPreview();
    },

    async removeRemap(index) {
      set({ remap: get().remap.filter((_, i) => i !== index) });
      await runPreview();
    },

    async start() {
      const { path, remap, step } = get();
      if (path === null || step !== "review") return;
      set({ step: "importing", progress: null, unmatchedSoFar: 0, stopping: false, error: null });
      try {
        const report = await rbxmlImportStart(path, remap, (e) => {
          if (e.type === "progress") {
            set({ progress: { stage: e.stage, done: e.done, total: e.total } });
          } else {
            set({ unmatchedSoFar: get().unmatchedSoFar + 1 });
          }
        });
        set({ step: "report", report, progress: null, stopping: false });
      } catch (e) {
        set({
          step: "review",
          progress: null,
          stopping: false,
          error: `Import failed: ${String(e)}`,
        });
      }
      if (get().report?.cancelled !== true) await refreshLibrary();
    },

    async cancel() {
      if (get().step !== "importing" || get().progress?.stage === "database") return;
      set({ stopping: true });
      await rbxmlImportCancel();
    },

    async remapAndReview(from, to) {
      set({
        step: "review",
        remap: [{ from, to }, ...get().remap],
        preview: null,
        report: null,
      });
      await runPreview();
    },

    close() {
      if (get().step === "importing") return;
      previewGeneration++;
      set(idle);
    },
  };
});

/** The table, facets and playlist tree after an import changed the library. */
async function refreshLibrary() {
  refreshAll();
  await Promise.all([useLibrary.getState().loadFacets(), usePlaylists.getState().load()]);
}
