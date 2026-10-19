/**
 * Library window: playlist tree on the left, toolbar and track table in the
 * middle, then the detail panel for the primary selected track and the
 * collapsible filter panel on the right; the transport bar along the bottom.
 * Space plays and pauses unless a text field has focus.
 */
import { useEffect } from "react";
import { DEFAULT_HIDDEN } from "./components/columns";
import { FiltersPanel } from "./components/FiltersPanel";
import { LibraryToolbar } from "./components/LibraryToolbar";
import { DeleteDialog, DuplicateDialog } from "./components/PlaylistDialogs";
import { PlaylistTree } from "./components/PlaylistTree";
import { TrackDetailPanel } from "./components/TrackDetailPanel";
import { TrackTable } from "./components/TrackTable";
import { TransportBar } from "./components/TransportBar";
import { useTrackTable, visibleColumns } from "./components/trackTableModel";
import { rowAt, useLibrary } from "./store/library";
import { usePlayer } from "./store/player";
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

  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if (e.code !== "Space" || e.repeat || isEditable(e.target)) return;
      e.preventDefault();
      void usePlayer.getState().toggle();
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  return (
    <div className="shell">
      <div className="app">
        <aside className="sidebar">
          <PlaylistTree />
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
        <FiltersPanel />
        <DuplicateDialog />
        <DeleteDialog />
      </div>
      <TransportBar />
    </div>
  );
}

/** Text fields, selects, buttons and editable content keep Space for themselves. */
function isEditable(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return (
    target.isContentEditable || ["INPUT", "TEXTAREA", "SELECT", "BUTTON"].includes(target.tagName)
  );
}
