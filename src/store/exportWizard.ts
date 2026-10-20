/**
 * USB export wizard (phase 2 task 10): device → playlists → options → dry
 * run → writing → report. The stick must be a known stick (task 15, D27):
 * its saved profile ticks the playlists, leaving the playlists step saves
 * the ticks as its profile (`profile_set_selection`), and leaving the
 * options step saves the target. Entering the dry-run step asks Rust for the
 * plan (`export_plan`); going back and forward plans again, and a newer plan
 * drops older answers. Export writes exactly the plan the dry run shows
 * (`export_apply` with its `planId`) and records the sync in the stick's
 * profile; the wizard can't be closed while it runs.
 */
import { create } from "zustand";

import {
  type Device,
  type DryRun,
  deviceEject,
  type ExportOptions,
  type ExportProgress,
  type ExportReportView,
  type ExportSelection,
  exportApply,
  exportCancel,
  exportPlan,
  type PlaylistNode,
  profileSelection,
  profileSetSelection,
  profileSetTarget,
} from "../lib/ipc";
import { useDevices } from "./devices";
import { usePlaylists } from "./playlists";

export type ExportStep =
  | "closed"
  | "device"
  | "playlists"
  | "options"
  | "dryRun"
  | "writing"
  | "report";

export const EXPORT_STEPS: Exclude<ExportStep, "closed">[] = [
  "device",
  "playlists",
  "options",
  "dryRun",
  "writing",
  "report",
];

/** Ejecting the stick from the report step. */
export interface EjectState {
  state: "idle" | "ejecting" | "done" | "failed";
  /** "REKS ejected. You can remove it." or the error. */
  message: string | null;
}

/** `ExportOptions::default()` in Rust. */
export const DEFAULT_EXPORT_OPTIONS: ExportOptions = {
  target: "Generic",
  keepStale: false,
  missingAnalysis: "skip",
  artwork: false,
  skipUnsupported: false,
  history: "backUp",
  discardHistoryConfirmed: false,
  dividers: false,
};

interface ExportWizardState {
  step: ExportStep;
  mountPoint: string | null;
  /** The known stick at `mountPoint` (its profile); null until identified. */
  device: Device | null;
  /** Ticked playlists and folders. */
  checked: ReadonlySet<number>;
  options: ExportOptions;
  dryRun: DryRun | null;
  planning: boolean;
  /** Latest progress of the running export. */
  progress: ExportProgress | null;
  cancelling: boolean;
  report: ExportReportView | null;
  eject: EjectState;
  error: string | null;

  open(init?: { mountPoint?: string | null; playlists?: number[] }): void;
  close(): void;
  setDevice(mountPoint: string): void;
  /** The stick at `mountPoint` is `device`: tick its profile and take its target. */
  takeProfile(device: Device): Promise<void>;
  /** Ticks or unticks a playlist or folder. */
  toggle(id: number): void;
  setOptions(patch: Partial<ExportOptions>): void;
  next(nodes: PlaylistNode[]): Promise<void>;
  back(): void;
  /** Plans again (dry-run step). */
  replan(nodes: PlaylistNode[]): Promise<void>;
  /** Writes the shown plan (dry-run step, when it is ready). */
  startExport(): Promise<void>;
  /** Asks the running export to stop. */
  cancelExport(): Promise<void>;
  /** Report step: back to a fresh dry run. */
  planAgain(nodes: PlaylistNode[]): Promise<void>;
  /** Report step: ejects the stick. */
  ejectStick(): Promise<void>;
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
  s: Pick<ExportWizardState, "step" | "mountPoint" | "device" | "checked" | "options">,
  nodes: PlaylistNode[],
): boolean {
  switch (s.step) {
    case "device":
      return s.mountPoint !== null && s.device !== null;
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
  device: null as Device | null,
  checked: new Set<number>() as ReadonlySet<number>,
  options: DEFAULT_EXPORT_OPTIONS,
  dryRun: null,
  planning: false,
  progress: null,
  cancelling: false,
  report: null,
  eject: { state: "idle", message: null } as EjectState,
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
    // The export runs to its end; the user can cancel it instead.
    if (get().step === "writing") return;
    generation++;
    set(idle);
  },

  setDevice(mountPoint) {
    if (mountPoint !== get().mountPoint) set({ mountPoint, device: null });
  },

  async takeProfile(device) {
    if (get().device?.id === device.id) return;
    const mine = ++generation;
    set({ device, options: { ...get().options, target: device.target }, error: null });
    try {
      const sel = await profileSelection(device.id);
      // An empty profile keeps what the wizard opened with (the sidebar's selection).
      if (mine === generation && sel.playlists.length > 0) {
        set({ checked: new Set(sel.playlists) });
      }
    } catch (e) {
      if (mine === generation) set({ error: `Could not read the stick's playlists: ${String(e)}` });
    }
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
    if (!(await saveProfile(s, nodes))) return;
    set({ step, error: null });
    if (step === "dryRun") await get().replan(nodes);
  },

  back() {
    if (get().step === "writing" || get().step === "report") return;
    const i = EXPORT_STEPS.indexOf(get().step as (typeof EXPORT_STEPS)[number]);
    const step = EXPORT_STEPS[i - 1];
    if (step === undefined) return;
    generation++;
    set({ step, dryRun: null, planning: false, error: null });
  },

  async replan(nodes) {
    const { mountPoint, device, checked, options } = get();
    if (mountPoint === null) return;
    const mine = ++generation;
    set({ planning: true, dryRun: null, error: null });
    try {
      const dryRun = await exportPlan(
        mountPoint,
        selectionOf(nodes, checked),
        options,
        device?.id ?? null,
      );
      if (mine === generation) set({ dryRun, planning: false });
    } catch (e) {
      if (mine === generation) set({ planning: false, error: `Could not plan: ${String(e)}` });
    }
  },

  async startExport() {
    const { step, dryRun, planning } = get();
    if (step !== "dryRun" || planning || dryRun === null || !dryRun.ready) return;
    const mine = ++generation;
    set({ step: "writing", progress: null, cancelling: false, report: null, error: null });
    try {
      const report = await exportApply(dryRun.planId, (progress) => {
        if (mine === generation) set({ progress });
      });
      if (mine === generation) set({ step: "report", report });
    } catch (e) {
      if (mine === generation) set({ step: "report", error: `Could not export: ${String(e)}` });
    }
  },

  async cancelExport() {
    const { step, progress, cancelling } = get();
    if (step !== "writing" || cancelling || progress?.cancellable === false) return;
    set({ cancelling: true });
    try {
      await exportCancel();
    } catch {
      set({ cancelling: false });
    }
  },

  async planAgain(nodes) {
    if (get().step !== "report") return;
    set({
      step: "dryRun",
      report: null,
      progress: null,
      eject: idle.eject,
      error: null,
    });
    await get().replan(nodes);
  },

  async ejectStick() {
    const { step, report, mountPoint, eject } = get();
    const target = report?.mountPoint ?? mountPoint;
    if (step !== "report" || target === null || eject.state === "ejecting") return;
    const name = report?.deviceName ?? target;
    set({ eject: { state: "ejecting", message: null } });
    try {
      await deviceEject(target);
      set({ eject: { state: "done", message: `${name} ejected. You can remove it.` } });
    } catch (e) {
      set({ eject: { state: "failed", message: `Could not eject ${name}: ${String(e)}` } });
    }
    void useDevices.getState().refresh();
  },
}));

/**
 * Saves what the step being left set on the stick's profile: the ticks
 * (playlists step) or the target (options step). False, with the error
 * shown, when saving failed.
 */
async function saveProfile(
  s: Pick<ExportWizardState, "step" | "device" | "checked" | "options">,
  nodes: PlaylistNode[],
): Promise<boolean> {
  const device = s.device;
  if (device === null) return true;
  try {
    if (s.step === "playlists") {
      await profileSetSelection(device.id, selectionOf(nodes, s.checked).playlists);
    } else if (s.step === "options" && s.options.target !== device.target) {
      const saved = await profileSetTarget(device.id, s.options.target);
      useExportWizard.setState({ device: saved });
    }
    return true;
  } catch (e) {
    useExportWizard.setState({ error: `Could not save the stick's profile: ${String(e)}` });
    return false;
  }
}

/** Opens the wizard with the sidebar's selected playlist or folder ticked. */
export function openExportWizard(mountPoint: string | null = null) {
  const selected = usePlaylists.getState().tree.selectedId;
  useExportWizard.getState().open({
    mountPoint,
    playlists: selected === null ? [] : [selected],
  });
}
