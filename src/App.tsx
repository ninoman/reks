/**
 * Library window: the app header (library-wide actions) across the top; the
 * playlist tree on the left; toolbar, filter bar and track table in the
 * middle, with the track info panel sliding over the table's right edge when
 * its toolbar button is on; the transport bar along the bottom. Space plays
 * and pauses unless a text field has focus.
 */
import { useEffect } from "react";
import { AppHeader } from "./components/AppHeader";
import { DEFAULT_HIDDEN } from "./components/columns";
import { FilterBar } from "./components/FilterBar";
import { ImportWizard } from "./components/ImportWizard";
import { LibraryToolbar } from "./components/LibraryToolbar";
import { DeleteDialog, DuplicateDialog } from "./components/PlaylistDialogs";
import { PlaylistTree } from "./components/PlaylistTree";
import { SlideOver } from "./components/SlideOver";
import { TrackInfoPanel } from "./components/TrackDetailPanel";
import { TrackTable } from "./components/TrackTable";
import { TransportBar } from "./components/TransportBar";
import { useTrackTable, visibleColumns } from "./components/trackTableModel";
import { useLibrary } from "./store/library";
import { usePanels } from "./store/panels";
import { usePlayer } from "./store/player";
import { usePlaylists } from "./store/playlists";

export default function App() {
  const table = useTrackTable();
  const error = useLibrary((s) => s.error);
  const filterBarOpen = usePanels((s) => s.filterBarOpen);
  const infoOpen = usePanels((s) => s.infoOpen);

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
      <AppHeader />
      <div className="app">
        <aside className="sidebar">
          <PlaylistTree />
        </aside>
        <main className="main">
          <LibraryToolbar table={table} />
          {filterBarOpen && <FilterBar />}
          {error !== null && (
            <div className="error" role="alert">
              {error}
            </div>
          )}
          <div className="table-area">
            <TrackTable columns={visibleColumns(table)} />
            <SlideOver open={infoOpen}>
              <TrackInfoPanel />
            </SlideOver>
          </div>
        </main>
      </div>
      <TransportBar />
      <DuplicateDialog />
      <DeleteDialog />
      <ImportWizard />
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
