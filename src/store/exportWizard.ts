/**
 * USB export (phase 2 task 10, D42): review → writing → done, for one known
 * stick. The stick's playlists (its profile, D27) and "Plays on" are set in
 * the Sticks view; the other options come from Settings › Export
 * (`export_default_options`). Opening plans at once (`export_plan`); a newer
 * plan drops older answers. Export writes exactly the plan the review shows
 * (`export_apply` with its `planId`) and records the sync in the stick's
 * profile; the dialog can't be closed while it runs.
 *
 * Track problems never block: the review lists them as issues with fixes.
 * A fix that runs elsewhere (find files, convert, analyse) hides the dialog
 * while it runs and plans again when it is done; "Remove from playlists"
 * edits the library and plans again; "Keep as it is" only collapses a card.
 * The export guard (unimported play history) and the OneLibrary removal
 * confirmation stay choices made in the review.
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
  exportDefaultOptions,
  exportPlan,
  type HistoryPolicy,
  type IssueKind,
  type IssueTrack,
  pickFolder,
  profileSelection,
} from "../lib/ipc";
import { isIdle, useAnalysis } from "./analysis";
import { useConverter } from "./converter";
import { useDevices } from "./devices";
import { usePlaylists } from "./playlists";
import { useProfiles } from "./profiles";
import { useRelocate } from "./relocate";
import { useSettings } from "./settings";

export type ExportStep = "closed" | "review" | "writing" | "done";

export const EXPORT_STEPS: Exclude<ExportStep, "closed">[] = ["review", "writing", "done"];

/** Ejecting the stick from the done step. */
export interface EjectState {
  state: "idle" | "ejecting" | "done" | "failed";
  /** "REKS ejected. You can remove it." or the error. */
  message: string | null;
}

/** A fix that runs in another dialog; the export dialog waits hidden. */
export type ExternalFix = "findFiles" | "convert" | "analyse" | "settings";

interface ExportWizardState {
  step: ExportStep;
  mountPoint: string | null;
  /** The known stick at `mountPoint`. */
  device: Device | null;
  /** The stick's ticked playlists (its profile). */
  selection: ExportSelection | null;
  /** null until the defaults arrive. */
  options: ExportOptions | null;
  dryRun: DryRun | null;
  planning: boolean;
  /** Issue cards collapsed with "Keep as it is". */
  kept: ReadonlySet<IssueKind>;
  /** A fix running in another dialog; the export dialog is hidden meanwhile. */
  away: ExternalFix | null;
  /** Latest progress of the running export. */
  progress: ExportProgress | null;
  cancelling: boolean;
  report: ExportReportView | null;
  eject: EjectState;
  error: string | null;

  /** Opens the review for the known stick at `mountPoint`. */
  open(mountPoint: string): Promise<void>;
  close(): void;
  /** Plans again with the current options (after a fix). */
  replan(): Promise<void>;
  /** "Keep as it is" on one card, or every card when `kind` is null. */
  keep(kind: IssueKind | null): void;
  /** Leaves this run's tracks without analysis out (the approximate-analysis card). */
  leaveOutUnanalysed(): Promise<void>;
  /** Removes `tracks` from the stick's playlists they are in (the confirmed card action). */
  removeFromPlaylists(tracks: IssueTrack[]): Promise<void>;
  /** Runs a fix in its own dialog, then plans again. */
  runFix(fix: Exclude<ExternalFix, "settings">, trackIds: number[]): Promise<void>;
  /** "Change…": Settings › Export, then plans again with the new settings. */
  changeSettings(): Promise<void>;
  /**
   * Export guard: what happens to the stick's unimported sessions. `discard` counts as its
   * explicit confirmation. Plans again.
   */
  chooseHistory(policy: HistoryPolicy): Promise<void>;
  /** The user confirmed deleting the stick's `exportLibrary.db`. Plans again. */
  confirmOnelibraryRemoval(): Promise<void>;
  /** Writes the shown plan (when it is ready). */
  startExport(): Promise<void>;
  /** Asks the running export to stop. */
  cancelExport(): Promise<void>;
  /** Done step, after problems: back to a fresh review. */
  backToReview(): Promise<void>;
  /** Done step: ejects the stick. */
  ejectStick(): Promise<void>;
}

const idle = {
  step: "closed" as ExportStep,
  mountPoint: null,
  device: null as Device | null,
  selection: null as ExportSelection | null,
  options: null as ExportOptions | null,
  dryRun: null,
  planning: false,
  kept: new Set<IssueKind>() as ReadonlySet<IssueKind>,
  away: null as ExternalFix | null,
  progress: null,
  cancelling: false,
  report: null,
  eject: { state: "idle", message: null } as EjectState,
  error: null,
};

let generation = 0;

/** The known stick plugged in at `mountPoint`, if identified. */
function knownAt(mountPoint: string): Device | null {
  const match = useProfiles.getState().mounted[mountPoint]?.match;
  return match?.type === "known" ? match.device : null;
}

/** Resolves once `done` holds for `store`'s state (checked on every change). */
function waitFor<S>(
  store: { getState(): S; subscribe(f: (s: S) => void): () => void },
  done: (s: S) => boolean,
): Promise<void> {
  return new Promise((resolve) => {
    if (done(store.getState())) return resolve();
    const stop = store.subscribe((s) => {
      if (done(s)) {
        stop();
        resolve();
      }
    });
  });
}

export const useExportWizard = create<ExportWizardState>((set, get) => ({
  ...idle,

  async open(mountPoint) {
    if (get().step === "writing") return;
    const mine = ++generation;
    const device = knownAt(mountPoint);
    set({ ...idle, step: "review", mountPoint, device, planning: true });
    if (device === null) {
      set({ planning: false, error: "Set this stick up in Sticks first." });
      return;
    }
    try {
      const [selection, options] = await Promise.all([
        profileSelection(device.id),
        exportDefaultOptions(device.target),
      ]);
      if (mine !== generation) return;
      set({ selection, options });
      if (selection.playlists.length === 0) {
        set({
          planning: false,
          error: `No playlists are ticked for ${device.name}. Tick some in Sticks.`,
        });
        return;
      }
      await get().replan();
    } catch (e) {
      if (mine === generation) {
        set({ planning: false, error: `Could not read the stick's settings: ${String(e)}` });
      }
    }
  },

  close() {
    // The export runs to its end; the user can cancel it instead.
    if (get().step === "writing") return;
    generation++;
    set(idle);
  },

  async replan() {
    const { mountPoint, device, selection, options } = get();
    if (mountPoint === null || selection === null || options === null) return;
    const mine = ++generation;
    set({ planning: true, error: null });
    try {
      const dryRun = await exportPlan(mountPoint, selection, options, device?.id ?? null);
      if (mine === generation) set({ dryRun, planning: false });
    } catch (e) {
      if (mine === generation) set({ planning: false, error: `Could not plan: ${String(e)}` });
    }
  },

  keep(kind) {
    const kinds = kind === null ? (get().dryRun?.issues.map((i) => i.kind) ?? []) : [kind];
    set({ kept: new Set([...get().kept, ...kinds]) });
  },

  async leaveOutUnanalysed() {
    const options = get().options;
    if (options === null || get().planning) return;
    set({ options: { ...options, missingAnalysis: "skip" } });
    await get().replan();
  },

  async removeFromPlaylists(tracks) {
    if (get().planning) return;
    const playlistIds = [...new Set(tracks.flatMap((t) => t.playlists))];
    const trackIds = tracks.filter((t) => t.playlists.length > 0).map((t) => t.trackId);
    if (playlistIds.length === 0 || trackIds.length === 0) return;
    const outcome = await usePlaylists
      .getState()
      .mutate({ type: "removeTracks", playlistIds, trackIds });
    if (outcome === null) {
      set({ error: `Could not remove the tracks: ${usePlaylists.getState().error ?? ""}` });
      return;
    }
    await get().replan();
  },

  async runFix(fix, trackIds) {
    if (get().step !== "review" || get().planning || get().away !== null) return;
    set({ away: fix });
    if (fix === "findFiles") {
      // The folder picker comes first; cancelling it opens nothing.
      const dir = await pickFolder("Find missing files in…");
      if (dir) {
        await useRelocate.getState().start({ kind: "tracks", ids: trackIds }, dir);
        await waitFor(useRelocate, (s) => s.step === "closed");
      }
    } else if (fix === "convert") {
      useConverter.getState().open(trackIds, "olderPlayers");
      await waitFor(useConverter, (s) => s.step === "closed");
    } else {
      await useAnalysis.getState().reanalyse(trackIds);
      // No status: analysis is off; nothing to wait for.
      await waitFor(useAnalysis, (s) => s.error !== null || s.status === null || isIdle(s.status));
    }
    if (get().away !== fix) return;
    set({ away: null });
    await get().replan();
  },

  async changeSettings() {
    const { step, planning, away, device, options } = get();
    if (step !== "review" || planning || away !== null || device === null) return;
    set({ away: "settings" });
    useSettings.getState().setDialogOpen(true, "export");
    await waitFor(useSettings, (s) => !s.dialogOpen);
    if (get().away !== "settings") return;
    try {
      const fresh = await exportDefaultOptions(device.target);
      // The run's own choices stay; everything the settings decide is fresh.
      set({
        away: null,
        options: options && {
          ...fresh,
          history: options.history,
          historyChosen: options.historyChosen,
          discardHistoryConfirmed: options.discardHistoryConfirmed,
        },
      });
    } catch (e) {
      set({ away: null, error: `Could not read the export settings: ${String(e)}` });
      return;
    }
    await get().replan();
  },

  async chooseHistory(policy) {
    const options = get().options;
    if (get().step !== "review" || get().planning || options === null) return;
    set({
      options: {
        ...options,
        history: policy,
        historyChosen: true,
        discardHistoryConfirmed: policy === "discard",
      },
    });
    await get().replan();
  },

  async confirmOnelibraryRemoval() {
    const options = get().options;
    if (get().step !== "review" || get().planning || options === null) return;
    set({ options: { ...options, onelibraryRemovalConfirmed: true } });
    await get().replan();
  },

  async startExport() {
    const { step, dryRun, planning } = get();
    if (step !== "review" || planning || dryRun === null || !dryRun.ready) return;
    const mine = ++generation;
    set({ step: "writing", progress: null, cancelling: false, report: null, error: null });
    try {
      const report = await exportApply(dryRun.planId, (progress) => {
        if (mine === generation) set({ progress });
      });
      if (mine === generation) set({ step: "done", report });
    } catch (e) {
      if (mine === generation) set({ step: "done", error: `Could not export: ${String(e)}` });
    }
    void useProfiles.getState().loadStatus();
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

  async backToReview() {
    if (get().step !== "done") return;
    const options = get().options;
    set({
      step: "review",
      report: null,
      progress: null,
      eject: idle.eject,
      error: null,
      kept: idle.kept,
      // The guard's and the OneLibrary choices were for the run just made.
      options: options && {
        ...options,
        history: options.history === "import" ? "backUp" : options.history,
        historyChosen: false,
        onelibraryRemovalConfirmed: false,
      },
    });
    await get().replan();
  },

  async ejectStick() {
    const { step, report, mountPoint, eject } = get();
    const target = report?.mountPoint ?? mountPoint;
    if (step !== "done" || target === null || eject.state === "ejecting") return;
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

/** Opens the review for the known stick at `mountPoint` (closing the Sticks view). */
export function openExportWizard(mountPoint: string) {
  useDevices.getState().setDialogOpen(false);
  void useExportWizard.getState().open(mountPoint);
}
