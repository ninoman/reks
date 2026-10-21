/**
 * The folder scan, shared by the app header ("Scan folder…") and Settings
 * ("Rescan all"); its status shows in the header's status line.
 */
import { create } from "zustand";

import { scanCancel, scanStart } from "../lib/ipc";
import { useLibrary } from "./library";
import { refreshAll } from "./panes";
import { usePlaylists } from "./playlists";

export interface ScanState {
  scanning: boolean;
  status: string;
  /** Scans `roots`; none = every library folder. */
  scan(roots: string[]): Promise<void>;
  cancel(): void;
}

export const useScan = create<ScanState>()((set, get) => ({
  scanning: false,
  status: "",
  async scan(roots) {
    if (get().scanning) return;
    set({ scanning: true, status: "Scanning…" });
    try {
      const r = await scanStart(roots, (e) => {
        if (e.type === "progress") set({ status: `Scanning ${e.done}/${e.total}` });
      });
      set({
        status:
          `Scan ${r.cancelled ? "cancelled" : "done"}: ${r.added} added, ${r.updated} updated, ` +
          `${r.moved} moved, ${r.missing} missing, ${r.errors.length} errors`,
      });
    } catch (e) {
      set({ status: `Scan failed: ${String(e)}` });
    } finally {
      set({ scanning: false });
      refreshAll();
      await Promise.all([useLibrary.getState().loadFacets(), usePlaylists.getState().load()]);
    }
  },
  cancel() {
    void scanCancel();
  },
}));
