/**
 * Which optional parts of the library window are shown: the Filters drawer beside
 * the table, the track info panel on the right, and the mix panel (instead of
 * the player deck) above the library. Selecting a row never opens the info
 * panel; only its toolbar button does.
 */
import { create } from "zustand";

export interface PanelsState {
  filterBarOpen: boolean;
  infoOpen: boolean;
  /** The mix panel (decks and mixer, phase 5) instead of the player deck. */
  mixOpen: boolean;
  toggleFilterBar(): void;
  toggleInfo(): void;
  setMixOpen(open: boolean): void;
}

export const usePanels = create<PanelsState>()((set) => ({
  filterBarOpen: false,
  infoOpen: false,
  mixOpen: false,
  toggleFilterBar() {
    set((s) => ({ filterBarOpen: !s.filterBarOpen }));
  },
  toggleInfo() {
    set((s) => ({ infoOpen: !s.infoOpen }));
  },
  setMixOpen(open) {
    set({ mixOpen: open });
  },
}));
