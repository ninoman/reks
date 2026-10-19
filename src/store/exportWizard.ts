/**
 * USB export wizard (phase 2 task 10): device → playlists → options → dry
 * run. Entering the dry-run step asks Rust for the plan (`export_plan`);
 * going back and forward plans again, and a newer plan drops older answers.
 * Writing (task 9's `apply`), progress and the report come later.
 */
import { create } from "zustand";

import {
  type DryRun,
  type ExportOptions,
  type ExportSelection,
  exportPlan,
  type PlaylistNode,
} from "../lib/ipc";
import { usePlaylists } from "./playlists";

export type ExportStep = "closed" | "device" | "playlists" | "options" | "dryRun";

export const EXPORT_STEPS: Exclude<ExportStep, "closed">[] = [
  "device",
  "playlists",
  "options",
  "dryRun",
];

/** `ExportOptions::default()` in Rust. */
export const DEFAULT_EXPORT_OPTIONS: ExportOptions = {
  target: "Generic",
  keepStale: false,
  missingAnalysis: "skip",
  artwork: false,
  skipUnsupported: false,
  history: "backUp",
  discardHistoryConfirmed: false,
};

interface ExportWizardState {
  step: ExportStep;
  mountPoint: string | null;
  /** Ticked playlists and folders. */
  checked: ReadonlySet<number>;
  options: ExportOptions;
  dryRun: DryRun | null;
  planning: boolean;
  error: string | null;

  open(init?: { mountPoint?: string | null; playlists?: number[] }): void;
  close(): void;
  setDevice(mountPoint: string): void;
  /** Ticks or unticks a playlist or folder. */
  toggle(id: number): void;
  setOptions(patch: Partial<ExportOptions>): void;
  next(nodes: PlaylistNode[]): Promise<void>;
  back(): void;
  /** Plans again (dry-run step). */
  replan(nodes: PlaylistNode[]): Promise<void>;
}

/** Ids that a ticked folder above them already covers. */
export function coveredIds(nodes: PlaylistNode[], checked: ReadonlySet<number>): Set<number> {
  const covered = new Set<number>();
  // Depth first: a parent comes before its children.
  for (const n of nodes) {
    if (n.parentId !== null && (checked.has(n.parentId) || covered.has(n.parentId))) {
      covered.add(n.id);
    }
  }
  return covered;
}

/** The ticked nodes no ticked folder covers, in tree order: the `Selection`. */
export function selectionOf(nodes: PlaylistNode[], checked: ReadonlySet<number>): ExportSelection {
  const covered = coveredIds(nodes, checked);
  return {
    playlists: nodes.filter((n) => checked.has(n.id) && !covered.has(n.id)).map((n) => n.id),
  };
}

/** Whether `step` has what it needs to go on. */
export function canContinue(
  s: Pick<ExportWizardState, "step" | "mountPoint" | "checked" | "options">,
  nodes: PlaylistNode[],
): boolean {
  switch (s.step) {
    case "device":
      return s.mountPoint !== null;
    case "playlists":
      return selectionOf(nodes, s.checked).playlists.length > 0;
    case "options":
      return s.options.history !== "discard" || s.options.discardHistoryConfirmed;
    default:
      return false;
  }
}

const idle = {
  step: "closed" as ExportStep,
  mountPoint: null,
  checked: new Set<number>() as ReadonlySet<number>,
  options: DEFAULT_EXPORT_OPTIONS,
  dryRun: null,
  planning: false,
  error: null,
};

let generation = 0;

export const useExportWizard = create<ExportWizardState>((set, get) => ({
  ...idle,

  open(init) {
    generation++;
    set({
      ...idle,
      step: "device",
      mountPoint: init?.mountPoint ?? null,
      checked: new Set(init?.playlists ?? []),
    });
  },

  close() {
    generation++;
    set(idle);
  },

  setDevice(mountPoint) {
    set({ mountPoint });
  },

  toggle(id) {
    const checked = new Set(get().checked);
    if (!checked.delete(id)) checked.add(id);
    set({ checked });
  },

  setOptions(patch) {
    const options = { ...get().options, ...patch };
    // Leaving `discard` withdraws the confirmation; it is given for one choice.
    if (options.history !== "discard") options.discardHistoryConfirmed = false;
    set({ options });
  },

  async next(nodes) {
    const s = get();
    if (!canContinue(s, nodes)) return;
    const i = EXPORT_STEPS.indexOf(s.step as (typeof EXPORT_STEPS)[number]);
    const step = EXPORT_STEPS[i + 1];
    if (step === undefined) return;
    set({ step, error: null });
    if (step === "dryRun") await get().replan(nodes);
  },

  back() {
    const i = EXPORT_STEPS.indexOf(get().step as (typeof EXPORT_STEPS)[number]);
    const step = EXPORT_STEPS[i - 1];
    if (step === undefined) return;
    generation++;
    set({ step, dryRun: null, planning: false, error: null });
  },

  async replan(nodes) {
    const { mountPoint, checked, options } = get();
    if (mountPoint === null) return;
    const mine = ++generation;
    set({ planning: true, dryRun: null, error: null });
    try {
      const dryRun = await exportPlan(mountPoint, selectionOf(nodes, checked), options);
      if (mine === generation) set({ dryRun, planning: false });
    } catch (e) {
      if (mine === generation) set({ planning: false, error: `Could not plan: ${String(e)}` });
    }
  },
}));

/** Opens the wizard with the sidebar's selected playlist or folder ticked. */
export function openExportWizard(mountPoint: string | null = null) {
  const selected = usePlaylists.getState().tree.selectedId;
  useExportWizard.getState().open({
    mountPoint,
    playlists: selected === null ? [] : [selected],
  });
}
