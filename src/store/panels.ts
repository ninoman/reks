/**
 * Which optional parts of the library window are shown: the filter bar above
 * the table and the track info panel on the right. Selecting a row never
 * opens the info panel; only its toolbar button does.
 */
import { create } from "zustand";

export interface PanelsState {
  filterBarOpen: boolean;
  infoOpen: boolean;
  toggleFilterBar(): void;
  toggleInfo(): void;
}

export const usePanels = create<PanelsState>()((set) => ({
  filterBarOpen: true,
  infoOpen: false,
  toggleFilterBar() {
    set((s) => ({ filterBarOpen: !s.filterBarOpen }));
  },
  toggleInfo() {
    set((s) => ({ infoOpen: !s.infoOpen }));
  },
}));
