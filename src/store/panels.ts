/**
 * Which optional parts of the library window are shown: the Filters drawer beside
 * the table, the track info panel on the right, and the mix panel (instead of
 * the player deck) above the library. Selecting a row never opens the info
 * panel; only its toolbar button does. Also the track table's row density and
 * the widths the sidebar and the right-hand drawers were dragged to.
 */
import { create } from "zustand";

import type { RowDensity } from "../lib/ipc";

export interface PanelsState {
  filterBarOpen: boolean;
  infoOpen: boolean;
  /** The mix panel (decks and mixer, phase 5) instead of the player deck. */
  mixOpen: boolean;
  rowDensity: RowDensity;
  /** px; `null`: the `--sidebar-w` default. */
  sidebarWidth: number | null;
  /** The info panel's and drawers' width, px; `null`: the `--sheet-w` default. */
  sheetWidth: number | null;
  toggleFilterBar(): void;
  toggleInfo(): void;
  setMixOpen(open: boolean): void;
  setRowDensity(density: RowDensity): void;
  setSidebarWidth(width: number | null): void;
  setSheetWidth(width: number | null): void;
}

export const usePanels = create<PanelsState>()((set) => ({
  filterBarOpen: false,
  infoOpen: false,
  mixOpen: false,
  rowDensity: "default",
  sidebarWidth: null,
  sheetWidth: null,
  toggleFilterBar() {
    set((s) => ({ filterBarOpen: !s.filterBarOpen }));
  },
  toggleInfo() {
    set((s) => ({ infoOpen: !s.infoOpen }));
  },
  setMixOpen(open) {
    set({ mixOpen: open });
  },
  setRowDensity(density) {
    set({ rowDensity: density });
  },
  setSidebarWidth(sidebarWidth) {
    set({ sidebarWidth });
  },
  setSheetWidth(sheetWidth) {
    set({ sheetWidth });
  },
}));
