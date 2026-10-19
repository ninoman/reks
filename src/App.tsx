/**
 * Library window: filter sidebar, toolbar, the track table and the detail
 * panel for the primary selected track. Playlists UI and player land in
 * later tasks (docs/phases/phase-1.md).
 */
import { useEffect } from "react";

import { FiltersSidebar } from "./components/FiltersSidebar";
import { LibraryToolbar } from "./components/LibraryToolbar";
import { TrackDetailPanel } from "./components/TrackDetailPanel";
import { TrackTable } from "./components/TrackTable";
import { useTrackTable, visibleColumns } from "./components/trackTableModel";
import { DEFAULT_HIDDEN } from "./components/columns";
import { rowAt, useLibrary } from "./store/library";
import { primaryIndex } from "./store/selection";

export default function App() {
  const table = useTrackTable();
  const error = useLibrary((s) => s.error);
  const primaryId = useLibrary((s) => {
    const i = primaryIndex(s.selection);
    return i === null ? null : (rowAt(s.pages, i)?.id ?? null);
  });

  useEffect(() => {
    const s = useLibrary.getState();
    s.setColumnVisibility(Object.fromEntries(DEFAULT_HIDDEN.map((id) => [id, false])));
    s.refresh();
    void s.loadFacets();
  }, []);

  return (
    <div className="app">
      <FiltersSidebar />
      <main className="main">
        <LibraryToolbar table={table} />
        {error !== null && (
          <div className="error" role="alert">
            {error}
          </div>
        )}
        <TrackTable columns={visibleColumns(table)} />
      </main>
      {primaryId !== null && <TrackDetailPanel key={primaryId} trackId={primaryId} />}
    </div>
  );
}
