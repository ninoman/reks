/**
 * Background analysis (Phase 3 task 11): the queue's counts for the toolbar
 * indicator, pause/resume/cancel, and re-analyse. The queue itself runs in
 * Rust; an analysed track's row is swapped in place (no refresh, so
 * selection and scroll stay).
 */
import { create } from "zustand";

import {
  type AnalysisStatus,
  analysisCancel,
  analysisPause,
  analysisReanalyse,
  analysisResume,
  analysisSubscribe,
  libraryTrack,
} from "../lib/ipc";
import { replaceRowEverywhere } from "./panes";

export interface AnalysisState {
  /** `null` until subscribed, or when analysis is off (no beat model). */
  status: AnalysisStatus | null;
  error: string | null;
  subscribe(): Promise<void>;
  pause(): Promise<void>;
  resume(): Promise<void>;
  cancel(): Promise<void>;
  reanalyse(ids: number[]): Promise<void>;
}

export function isIdle(s: AnalysisStatus): boolean {
  return s.queued === 0 && s.running === 0;
}

export const useAnalysis = create<AnalysisState>()((set) => {
  /** Shows the new BPM/key of `id` wherever its row is loaded. */
  function updateRow(id: number) {
    libraryTrack(id).then(
      (d) => replaceRowEverywhere(d.track),
      () => {}, // removed meanwhile; the next refresh shows it anyway
    );
  }
  async function run(f: () => Promise<AnalysisStatus>) {
    try {
      set({ status: await f(), error: null });
    } catch (e) {
      set({ error: String(e) });
    }
  }

  return {
    status: null,
    error: null,
    async subscribe() {
      try {
        const status = await analysisSubscribe((e) => {
          if (e.type === "status") set({ status: e.status });
          else if (e.type === "analysed" && (e.stored.grid || e.stored.bpm || e.stored.key))
            updateRow(e.trackId);
        });
        set({ status, error: null });
      } catch (e) {
        set({ error: String(e) });
      }
    },
    pause: () => run(analysisPause),
    resume: () => run(analysisResume),
    cancel: () => run(analysisCancel),
    async reanalyse(ids) {
      if (ids.length === 0) return;
      try {
        await analysisReanalyse(ids);
        set({ error: null });
      } catch (e) {
        set({ error: String(e) });
      }
    },
  };
});
