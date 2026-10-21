/**
 * Which optional parts of the library window are shown: the filter bar above
 * the table, the track info panel on the right and the mix panel below. Selecting a row never
 * opens the info panel; only its toolbar button does.
 */
import { create } from "zustand";

export interface PanelsState {
  filterBarOpen: boolean;
  infoOpen: boolean;
  /** The mix panel (decks and mixer, phase 5) under the table. */
  mixOpen: boolean;
  toggleFilterBar(): void;
  toggleInfo(): void;
  toggleMix(): void;
}

export const usePanels = create<PanelsState>()((set) => ({
  filterBarOpen: true,
  infoOpen: false,
  mixOpen: false,
  toggleFilterBar() {
    set((s) => ({ filterBarOpen: !s.filterBarOpen }));
  },
  toggleInfo() {
    set((s) => ({ infoOpen: !s.infoOpen }));
  },
  toggleMix() {
    set((s) => ({ mixOpen: !s.mixOpen }));
  },
}));
