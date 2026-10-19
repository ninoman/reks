/**
 * Library window: filter sidebar, toolbar and the track table. Detail
 * panel, playlists UI and player land in later tasks (docs/phases/phase-1.md).
 */
import { useEffect } from "react";

import { FiltersSidebar } from "./components/FiltersSidebar";
import { LibraryToolbar } from "./components/LibraryToolbar";
import { TrackTable } from "./components/TrackTable";
import { useTrackTable, visibleColumns } from "./components/trackTableModel";
import { DEFAULT_HIDDEN } from "./components/columns";
import { useLibrary } from "./store/library";

export default function App() {
  const table = useTrackTable();
  const error = useLibrary((s) => s.error);

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
    </div>
  );
}
