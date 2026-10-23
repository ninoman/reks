/**
 * Which optional parts of the library window are shown: the Filters drawer beside
 * the table, the track info panel on the right, and the mix panel (instead of
 * the player deck) above the library. Selecting a row never opens the info
 * panel; only its toolbar button does. Also the track table's row density.
 */
import { create } from "zustand";

import type { RowDensity } from "../lib/ipc";

export interface PanelsState {
  filterBarOpen: boolean;
  infoOpen: boolean;
  /** The mix panel (decks and mixer, phase 5) instead of the player deck. */
  mixOpen: boolean;
  rowDensity: RowDensity;
  toggleFilterBar(): void;
  toggleInfo(): void;
  setMixOpen(open: boolean): void;
  setRowDensity(density: RowDensity): void;
}

export const usePanels = create<PanelsState>()((set) => ({
  filterBarOpen: false,
  infoOpen: false,
  mixOpen: false,
  rowDensity: "default",
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
}));
