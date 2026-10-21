/**
 * Split panes: playlists opened under the main track list (Shift+click in the
 * tree), stacked top to bottom. Each pane has its own library store (sort,
 * pages, selection); search, filters, the rule builder and the info panel stay
 * with the main list. A playlist is open in at most one list at a time, so a
 * divider rename or drop indicator has one place to show.
 */
import { create } from "zustand";

import type { TrackRow } from "../lib/ipc";
import { createLibraryStore, type LibraryStore, useLibrary } from "./library";

export interface Pane {
  /** Stable React key; a playlist id could come back after close and reopen. */
  key: number;
  playlistId: number;
  store: LibraryStore;
}

export interface PanesState {
  panes: Pane[];
  /** Opens `playlistId` in a new pane under the others, unless a list already shows it. */
  open(playlistId: number, isFolder: boolean): void;
  close(key: number): void;
  /** Closes the pane showing `playlistId`, if any (the main list is about to show it). */
  closePlaylist(playlistId: number | null): void;
}

let nextKey = 1;

export const usePanes = create<PanesState>()((set, get) => ({
  panes: [],

  open(playlistId, isFolder) {
    const shown = [
      useLibrary.getState().filters.playlistId,
      ...get().panes.map((p) => p.playlistId),
    ];
    if (shown.includes(playlistId)) return;
    const store = createLibraryStore();
    store.getState().showPlaylist(playlistId, isFolder);
    set((s) => ({ panes: [...s.panes, { key: nextKey++, playlistId, store }] }));
  },
  close(key) {
    set((s) => ({ panes: s.panes.filter((p) => p.key !== key) }));
  },
  closePlaylist(playlistId) {
    const pane = get().panes.find((p) => p.playlistId === playlistId);
    if (pane !== undefined) get().close(pane.key);
  },
}));

/** Every open track list, main first: edits that change rows reach all of them. */
export function allLibraries(): LibraryStore[] {
  return [useLibrary, ...usePanes.getState().panes.map((p) => p.store)];
}

/** `replaceRow` in every open list. */
export function replaceRowEverywhere(row: TrackRow): void {
  for (const lib of allLibraries()) lib.getState().replaceRow(row);
}

/** Re-runs every open list's query; `keepPlace` as in `LibraryState.refresh`. */
export function refreshAll(keepPlace = false): void {
  for (const lib of allLibraries()) lib.getState().refresh(keepPlace);
}
