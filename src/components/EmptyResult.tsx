/**
 * Said under the table's header when the result has no tracks, so the panel
 * is never blank: why (filters, a smart list's rules, an empty playlist) and
 * what to do about it. With filters it offers "Undo last change" (back to
 * the last filters that matched tracks) and "Clear filters". Waits for the
 * query to land first.
 */

import { shownPlaylist, usePlaylists } from "../store/playlists";
import { clearAllFilters, useRules } from "../store/rules";
import { useLibraryStore } from "./libraryStoreContext";
import { Button } from "./ui/Button";

export function EmptyResult() {
  const useLibrary = useLibraryStore();
  const total = useLibrary((s) => s.total);
  const landed = useLibrary((s) => s.pagesGeneration === s.generation);
  const failed = useLibrary((s) => s.error !== null);
  const text = useLibrary((s) => s.text.trim() !== "");
  const filters = useLibrary((s) => s.filtersEnabled && s.rule !== null);
  const shown = useLibrary((s) => s.playlistId);
  const nodes = usePlaylists((s) => s.nodes);
  const dividers = useLibrary((s) => s.dividers.length);
  const canUndo = useRules((s) => s.lastMatched !== null && s.lastMatched !== s.root);
  if (!landed || failed || total > 0 || dividers > 0) return null;

  const list = shownPlaylist(nodes, shown);
  let message = "No tracks yet. Scan a folder or import a rekordbox library to fill the library.";
  if (filters) message = "No tracks match these filters";
  else if (text) message = "No tracks match.";
  else if (list?.smart) message = "No tracks match this smart playlist's rules right now.";
  else if (list !== null) message = "This playlist is empty. Drag tracks here to add them.";
  else if (shown !== null) message = "This folder holds no tracks.";

  return (
    <div
      className="pointer-events-none absolute inset-x-0 top-12 flex flex-col items-center gap-2 text-sm text-fg-muted"
      role="status"
    >
      <p className={filters ? "m-0 font-semibold text-fg" : "m-0"}>{message}</p>
      {filters && canUndo && <p className="m-0">The last change left nothing.</p>}
      {filters && (
        <div className="pointer-events-auto flex gap-1.5">
          {canUndo && (
            <Button size="sm" onClick={() => useRules.getState().undoLastChange()}>
              Undo last change
            </Button>
          )}
          <Button size="sm" variant="ghost" onClick={clearAllFilters}>
            Clear filters
          </Button>
        </div>
      )}
    </div>
  );
}
