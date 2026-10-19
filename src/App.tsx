/**
 * Library window: sidebar (playlist tree and filters), toolbar, the track
 * table and the detail panel for the primary selected track. The player lands
 * in a later task (docs/phases/phase-1.md).
 */
import { useEffect } from "react";
import { DEFAULT_HIDDEN } from "./components/columns";
import { FiltersSidebar } from "./components/FiltersSidebar";
import { LibraryToolbar } from "./components/LibraryToolbar";
import { DeleteDialog, DuplicateDialog } from "./components/PlaylistDialogs";
import { PlaylistTree } from "./components/PlaylistTree";
import { TrackDetailPanel } from "./components/TrackDetailPanel";
import { TrackTable } from "./components/TrackTable";
import { useTrackTable, visibleColumns } from "./components/trackTableModel";
import { rowAt, useLibrary } from "./store/library";
import { usePlaylists } from "./store/playlists";
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
    void usePlaylists.getState().load();
  }, []);

  return (
    <div className="app">
      <aside className="sidebar">
        <PlaylistTree />
        <FiltersSidebar />
      </aside>
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
      <DuplicateDialog />
      <DeleteDialog />
    </div>
  );
}
