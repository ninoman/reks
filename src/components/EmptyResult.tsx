/**
 * Said under the table's header when the result has no tracks, so the panel
 * is never blank: why (filters, a smart list's rules, an empty playlist) and
 * what to do about it. Waits for the query to land first.
 */

import { activeFilterCount, useLibrary } from "../store/library";
import { shownPlaylist, usePlaylists } from "../store/playlists";
import { clearAllFilters } from "../store/rules";
import { ruleLeafCount } from "../store/ruleTree";
import { Button } from "./ui/Button";

export function EmptyResult() {
  const total = useLibrary((s) => s.total);
  const landed = useLibrary((s) => s.pagesGeneration === s.generation);
  const failed = useLibrary((s) => s.error !== null);
  const text = useLibrary((s) => s.text.trim() !== "");
  const filters = useLibrary((s) =>
    s.filtersEnabled ? activeFilterCount(s.filters) + ruleLeafCount(s.rule) : 0,
  );
  const shown = useLibrary((s) => s.filters.playlistId);
  const nodes = usePlaylists((s) => s.nodes);
  const dividers = useLibrary((s) => s.dividers.length);
  if (!landed || failed || total > 0 || dividers > 0) return null;

  const list = shownPlaylist(nodes, shown);
  let message = "No tracks yet. Scan a folder or import a rekordbox library to fill the library.";
  if (filters > 0 || text) message = "No tracks match.";
  else if (list?.smart) message = "No tracks match this smart playlist's rules right now.";
  else if (list !== null) message = "This playlist is empty. Drag tracks here to add them.";
  else if (shown !== null) message = "This folder holds no tracks.";

  return (
    <div
      className="pointer-events-none absolute inset-x-0 top-12 flex flex-col items-center gap-2 text-sm text-fg-muted"
      role="status"
    >
      <p className="m-0">{message}</p>
      {filters > 0 && (
        <Button
          size="sm"
          variant="outline"
          className="pointer-events-auto"
          onClick={clearAllFilters}
        >
          Clear all filters
        </Button>
      )}
    </div>
  );
}
