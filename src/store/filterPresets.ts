/**
 * Saved filters (D35): the list the Filters drawer and the toolbar menu
 * pick from, and its edits through `filter_presets_mutate`. Which one is
 * loaded into the drawer, and whether it changed since, lives in
 * `rules.ts`; this store only holds the list.
 */
import { create } from "zustand";

import {
  type FilterPreset,
  type FilterPresetOp,
  type FilterPresetOutcome,
  filterPresetsGet,
  filterPresetsMutate,
} from "../lib/ipc";
import { reportAction } from "./report";
import { useToasts } from "./toasts";

export interface FilterPresetsState {
  presets: FilterPreset[];
  status: "loading" | "ready" | "error";
  error: string | null;
  load(): Promise<void>;
  /** Applies `op` and reloads the list; `null` (and an error toast) when it fails. */
  mutate(op: FilterPresetOp): Promise<FilterPresetOutcome | null>;
}

export const useFilterPresets = create<FilterPresetsState>()((set, get) => ({
  presets: [],
  status: "loading",
  error: null,

  async load() {
    try {
      set({ presets: await filterPresetsGet(), status: "ready", error: null });
    } catch (e) {
      set({ status: "error", error: String(e) });
    }
  },
  async mutate(op) {
    try {
      const outcome = await filterPresetsMutate(op);
      await get().load();
      return outcome;
    } catch (e) {
      const msg = `Could not change the saved filters: ${e}`;
      useToasts.getState().show(msg, reportAction(msg));
      return null;
    }
  },
}));

/** The saved filter called `name`, any case (names are unique that way). */
export function presetNamed(presets: readonly FilterPreset[], name: string): FilterPreset | null {
  const n = name.trim().toLowerCase();
  return presets.find((p) => p.name.toLowerCase() === n) ?? null;
}
