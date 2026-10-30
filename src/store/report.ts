/**
 * The report dialog (D34): a bug, feature request or feedback draft, its
 * preview, and sending it to the feedback relay (or saving it to a file when
 * the build has no relay or sending fails). Nothing leaves the machine until
 * the user presses Send.
 */

import { create } from "zustand";
import {
  pickSavePath,
  type Report,
  type ReportDraft,
  type ReportKind,
  reportCanSend,
  reportNewCrash,
  reportPreview,
  reportSave,
  reportSend,
} from "../lib/ipc";
import { type Toast, useToasts } from "./toasts";

export type ReportPhase = "editing" | "sending" | "sent" | "failed";

export interface ReportState {
  open: boolean;
  draft: ReportDraft;
  /** Whether this build knows a relay; loaded on first open. */
  canSend: boolean;
  phase: ReportPhase;
  /** Why the last send or save failed. */
  error: string | null;
  /** What would be sent; null until asked for, or while stale. */
  preview: Report | null;
  previewLoading: boolean;
  /** A note after saving to a file. */
  notice: string | null;
  openReport(init?: { kind?: ReportKind; context?: string }): Promise<void>;
  close(): void;
  setKind(kind: ReportKind): void;
  update(patch: Partial<Omit<ReportDraft, "kind">>): void;
  loadPreview(): Promise<void>;
  send(): Promise<void>;
  saveToFile(): Promise<void>;
}

function emptyDraft(kind: ReportKind, context = ""): ReportDraft {
  return {
    kind,
    title: "",
    description: "",
    steps: "",
    expected: "",
    contact: "",
    context,
    includeDiagnostics: kind === "bug",
  };
}

function message(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

/** A draft has what the relay requires. */
export function isComplete(d: ReportDraft): boolean {
  return d.title.trim() !== "" && d.description.trim() !== "";
}

export const useReport = create<ReportState>()((set, get) => ({
  open: false,
  draft: emptyDraft("bug"),
  canSend: false,
  phase: "editing",
  error: null,
  preview: null,
  previewLoading: false,
  notice: null,

  async openReport(init) {
    const kind = init?.kind ?? "bug";
    set({
      open: true,
      draft: emptyDraft(kind, init?.context ?? ""),
      phase: "editing",
      error: null,
      preview: null,
      notice: null,
    });
    try {
      set({ canSend: await reportCanSend() });
    } catch {
      set({ canSend: false });
    }
  },

  close() {
    set({ open: false });
  },

  setKind(kind) {
    set((s) => ({
      draft: { ...s.draft, kind, includeDiagnostics: kind === "bug" },
      preview: null,
    }));
  },

  update(patch) {
    set((s) => ({ draft: { ...s.draft, ...patch }, preview: null }));
  },

  async loadPreview() {
    set({ previewLoading: true });
    try {
      const preview = await reportPreview(get().draft);
      set({ preview, previewLoading: false });
    } catch (e) {
      set({ previewLoading: false, error: message(e) });
    }
  },

  async send() {
    const { draft } = get();
    if (!isComplete(draft) || get().phase === "sending") return;
    set({ phase: "sending", error: null, notice: null });
    try {
      await reportSend(draft);
      set({ phase: "sent" });
    } catch (e) {
      set({ phase: "failed", error: message(e) });
    }
  },

  async saveToFile() {
    const path = await pickSavePath({
      title: "Save report",
      defaultPath: "reks-report.txt",
      filter: { name: "Text", extensions: ["txt"] },
    });
    if (path === null) return;
    try {
      await reportSave(get().draft, path);
      set({ notice: `Saved to ${path}. Send it to the developers by email or chat.`, error: null });
    } catch (e) {
      set({ error: message(e) });
    }
  },
}));

/** A toast action that opens a bug report about `context` (an error message). */
export function reportAction(context: string): Toast["action"] {
  return {
    label: "Report",
    run: () => void useReport.getState().openReport({ kind: "bug", context }),
  };
}

/** Offers a report once after the app closed unexpectedly. */
export async function offerCrashReport(): Promise<void> {
  try {
    if (!(await reportNewCrash())) return;
  } catch {
    return;
  }
  useToasts
    .getState()
    .show("reks closed unexpectedly last time.", reportAction("reks closed unexpectedly."));
}
