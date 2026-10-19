/**
 * File converter dialog (phase 2.6 task 6): settings (a preset and the
 * table from the phase doc) → dry run (`convert_plan`) → converting
 * (`convert_apply`, cancellable) → report. The dialog can be hidden while it
 * converts; the app header then shows the progress and reopens it. Planning,
 * wording and totals come from Rust; this keeps the choices and the step.
 */
import { create } from "zustand";

import {
  type ConvertDepth,
  type ConvertDryRun,
  type ConvertOptions,
  type ConvertReportView,
  type ConvertSpec,
  type ConvertTarget,
  convertApply,
  convertCancel,
  convertPlan,
} from "../lib/ipc";
import { useLibrary } from "./library";
import { usePlaylists } from "./playlists";

export type ConvertStep = "closed" | "settings" | "dryRun" | "converting" | "report";

export type Preset = "olderPlayers" | "saveSpace";

/** The sample-rate choices: `keepMax48` is the Older players cap (88.2 → 44.1, 96 → 48). */
export type RateChoice = "keep" | "keepMax48" | "hz44100" | "hz48000";

export interface ConvertSettings {
  target: ConvertTarget;
  depth: ConvertDepth;
  rate: RateChoice;
  /** null: next to each original. */
  folder: string | null;
  library: ConvertOptions["library"];
  originals: ConvertOptions["originals"];
}

const PRESETS: Record<Preset, Pick<ConvertSettings, "target" | "depth" | "rate">> = {
  olderPlayers: { target: "aiff", depth: "keep", rate: "keepMax48" },
  saveSpace: { target: "flac", depth: "keep", rate: "keep" },
};

export const DEFAULT_SETTINGS: ConvertSettings = {
  ...PRESETS.olderPlayers,
  folder: null,
  library: "replace",
  originals: "keep",
};

/** The preset the format settings match, or null (custom). */
export function presetOf(s: ConvertSettings): Preset | null {
  for (const [name, p] of Object.entries(PRESETS) as [Preset, (typeof PRESETS)[Preset]][]) {
    if (s.target === p.target && s.depth === p.depth && s.rate === p.rate) return name;
  }
  return null;
}

/** The `convert_plan` arguments for `s`. */
export function toRequest(s: ConvertSettings): { spec: ConvertSpec; options: ConvertOptions } {
  return {
    spec: {
      target: s.target,
      depth: s.depth,
      rate: s.rate === "keepMax48" ? "keep" : s.rate,
    },
    options: {
      destination: s.folder === null ? "nextToOriginal" : { folder: s.folder },
      library: s.library,
      // The Trash needs Replace (Rust refuses it otherwise).
      originals: s.library === "replace" ? s.originals : "keep",
      capRate48k: s.rate === "keepMax48",
    },
  };
}

export interface ConvertProgress {
  /** 0-based item of `total`. */
  index: number;
  total: number;
  /** Of the current file, 0..1. */
  fraction: number;
  /** `Artist – Title` of the current file, from the dry run. */
  label: string;
  failed: number;
}

interface ConverterState {
  step: ConvertStep;
  /** Hidden while converting or on the report; the app header shows it. */
  hidden: boolean;
  trackIds: number[];
  settings: ConvertSettings;
  dryRun: ConvertDryRun | null;
  planning: boolean;
  progress: ConvertProgress | null;
  stopping: boolean;
  report: ConvertReportView | null;
  error: string | null;

  /** Opens on `trackIds` with `preset`'s format (other settings as last time). */
  open(trackIds: number[], preset?: Preset): void;
  applyPreset(preset: Preset): void;
  set(patch: Partial<ConvertSettings>): void;
  /** Settings → dry run. */
  plan(): Promise<void>;
  back(): void;
  start(): Promise<void>;
  cancel(): Promise<void>;
  setHidden(hidden: boolean): void;
  close(): void;
}

let planGeneration = 0;

export const useConverter = create<ConverterState>((set, get) => ({
  step: "closed",
  hidden: false,
  trackIds: [],
  settings: DEFAULT_SETTINGS,
  dryRun: null,
  planning: false,
  progress: null,
  stopping: false,
  report: null,
  error: null,

  open(trackIds, preset = "olderPlayers") {
    if (get().step === "converting") {
      set({ hidden: false });
      return;
    }
    planGeneration++;
    set({
      step: "settings",
      hidden: false,
      trackIds: [...new Set(trackIds)],
      settings: { ...get().settings, ...PRESETS[preset] },
      dryRun: null,
      planning: false,
      progress: null,
      stopping: false,
      report: null,
      error: null,
    });
  },

  applyPreset(preset) {
    set({ settings: { ...get().settings, ...PRESETS[preset] } });
  },

  set(patch) {
    const settings = { ...get().settings, ...patch };
    if (settings.library === "addAsNew") settings.originals = "keep";
    set({ settings });
  },

  async plan() {
    const { trackIds, settings } = get();
    const generation = ++planGeneration;
    set({ step: "dryRun", planning: true, dryRun: null, error: null });
    const { spec, options } = toRequest(settings);
    try {
      const dryRun = await convertPlan(trackIds, spec, options);
      if (generation === planGeneration) set({ dryRun, planning: false });
    } catch (e) {
      if (generation === planGeneration) set({ planning: false, error: String(e) });
    }
  },

  back() {
    if (get().step !== "dryRun") return;
    planGeneration++;
    set({ step: "settings", dryRun: null, planning: false, error: null });
  },

  async start() {
    const { dryRun, step } = get();
    if (step !== "dryRun" || dryRun === null || !dryRun.ready) return;
    const labels = new Map(dryRun.items.map((i) => [i.trackId, i.label]));
    set({
      step: "converting",
      progress: { index: 0, total: dryRun.items.length, fraction: 0, label: "", failed: 0 },
      stopping: false,
      error: null,
    });
    try {
      const report = await convertApply(dryRun.planId, (e) => {
        const p = get().progress;
        if (p === null) return;
        if (e.type === "started") {
          set({
            progress: {
              ...p,
              index: e.index,
              total: e.total,
              fraction: 0,
              label: labels.get(e.trackId) ?? "",
            },
          });
        } else if (e.type === "progress") {
          set({ progress: { ...p, fraction: e.fraction } });
        } else if (e.type === "failed") {
          set({ progress: { ...p, failed: p.failed + 1 } });
        }
      });
      set({ step: "report", report, progress: null, stopping: false });
    } catch (e) {
      set({ step: "dryRun", dryRun: null, progress: null, stopping: false, error: String(e) });
    }
    await refreshLibrary();
  },

  async cancel() {
    if (get().step !== "converting" || get().stopping) return;
    set({ stopping: true });
    await convertCancel();
  },

  setHidden(hidden) {
    set({ hidden });
  },

  close() {
    const { step } = get();
    if (step === "converting") {
      set({ hidden: true });
      return;
    }
    planGeneration++;
    set({
      step: "closed",
      hidden: false,
      trackIds: [],
      dryRun: null,
      planning: false,
      progress: null,
      report: null,
      error: null,
    });
  },
}));

/** Paths, kinds and sizes changed (Replace) or tracks were added (Add as new). */
async function refreshLibrary() {
  const { refresh, loadFacets } = useLibrary.getState();
  refresh();
  await Promise.all([loadFacets(), usePlaylists.getState().load()]);
}
