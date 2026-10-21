/** Which track list a `TrackTable` and its `EmptyResult` read: the main one unless a split pane says otherwise. */
import { createContext, useContext } from "react";

import { type LibraryStore, useLibrary } from "../store/library";

export const LibraryStoreContext = createContext<LibraryStore>(useLibrary);

export function useLibraryStore(): LibraryStore {
  return useContext(LibraryStoreContext);
}
