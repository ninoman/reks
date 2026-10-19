/**
 * Library window: the app header (library-wide actions) across the top; the
 * playlist tree on the left; toolbar, filter bar and track table in the
 * middle, with the track info panel sliding over the table's right edge when
 * its toolbar button is on; the transport bar along the bottom. Space plays
 * and pauses unless a text field has focus; Cmd/Ctrl+, opens Settings.
 * Settings load first and restore the window state (columns, panels, volume).
 */
import { useEffect } from "react";
import { AppHeader } from "./components/AppHeader";
import { DEFAULT_HIDDEN } from "./components/columns";
import { DevicesDialog } from "./components/DevicesDialog";
import { ExportWizard } from "./components/ExportWizard";
import { FilterBar } from "./components/FilterBar";
import { ImportWizard } from "./components/ImportWizard";
import { LibraryToolbar } from "./components/LibraryToolbar";
import { DeleteDialog, DuplicateDialog } from "./components/PlaylistDialogs";
import { PlaylistTree } from "./components/PlaylistTree";
import { SettingsDialog } from "./components/SettingsDialog";
import { TrackInfoPanel } from "./components/TrackDetailPanel";
import { TrackTable } from "./components/TrackTable";
import { TransportBar } from "./components/TransportBar";
import { useTrackTable, visibleColumns } from "./components/trackTableModel";
import { Sheet } from "./components/ui/Sheet";
import { useLibrary } from "./store/library";
import { usePanels } from "./store/panels";
import { usePlayer } from "./store/player";
import { usePlaylists } from "./store/playlists";
import { startUiSync, useSettings } from "./store/settings";

export default function App() {
  const table = useTrackTable();
  const error = useLibrary((s) => s.error);
  const filterBarOpen = usePanels((s) => s.filterBarOpen);
  const infoOpen = usePanels((s) => s.infoOpen);

  useEffect(() => {
    // Defaults first so the table does not flash every column; settings may replace them.
    useLibrary
      .getState()
      .setColumnVisibility(Object.fromEntries(DEFAULT_HIDDEN.map((id) => [id, false])));
    let stopSync: (() => void) | null = null;
    let live = true;
    void useSettings
      .getState()
      .load(DEFAULT_HIDDEN)
      .then(() => {
        if (live) stopSync = startUiSync();
      });
    const s = useLibrary.getState();
    s.refresh();
    void s.loadFacets();
    void usePlaylists.getState().load();
    return () => {
      live = false;
      stopSync?.();
    };
  }, []);

  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === "," && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        useSettings.getState().setDialogOpen(true);
        return;
      }
      if (e.code !== "Space" || e.repeat || isEditable(e.target)) return;
      e.preventDefault();
      void usePlayer.getState().toggle();
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  return (
    <div className="flex h-full flex-col">
      <AppHeader />
      <div className="flex min-h-0 flex-1">
        <aside className="flex w-(--sidebar-w) shrink-0 flex-col border-r border-border bg-bg-subtle">
          <PlaylistTree />
        </aside>
        <main className="flex min-w-0 flex-1 flex-col">
          <LibraryToolbar table={table} />
          {filterBarOpen && <FilterBar />}
          {error !== null && (
            <div
              className="border-b border-border px-2.5 py-1 text-danger select-text"
              role="alert"
            >
              {error}
            </div>
          )}
          <div className="relative flex min-h-0 flex-1 flex-col overflow-hidden">
            <TrackTable columns={visibleColumns(table)} />
            <Sheet open={infoOpen}>
              <TrackInfoPanel />
            </Sheet>
          </div>
        </main>
      </div>
      <TransportBar />
      <DuplicateDialog />
      <DeleteDialog />
      <ImportWizard />
      <SettingsDialog />
      <DevicesDialog />
      <ExportWizard />
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
