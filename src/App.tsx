/**
 * Library window: the app header (library-wide actions) across the top; the
 * playlist and Histories trees on the left (an open history session replaces
 * the toolbar, filters and table); toolbar, filter bar and track table in the
 * middle, with the track info panel sliding over the table's right edge when
 * its toolbar button is on; the mix panel (decks and mixer) under them when
 * the Mix button is on, with its own shortcuts (`src/lib/mixKeys.ts`),
 * otherwise the player deck alone (D30), under the player's header row
 * (volume, output, Mix). Space plays and pauses the player deck wherever focus is, except in text fields, dialogs and menus;
 * Cmd/Ctrl+, opens Settings.
 * Settings load first and restore the window state (columns, panels, volume).
 * Mounted only once the library is open (main.tsx); takes the splash down.
 */
import { useEffect } from "react";
import { AdoptDialog } from "./components/AdoptDialog";
import { AppHeader } from "./components/AppHeader";
import { ConvertDialog } from "./components/ConvertDialog";
import { DEFAULT_HIDDEN } from "./components/columns";
import { DevicesDialog } from "./components/DevicesDialog";
import { EmptyResult } from "./components/EmptyResult";
import { ExportWizard } from "./components/ExportWizard";
import { FilterBar } from "./components/FilterBar";
import { HistoryImportDialog } from "./components/HistoryImportDialog";
import { HistoryTree } from "./components/HistoryTree";
import { ImportWizard } from "./components/ImportWizard";
import { LibraryToolbar } from "./components/LibraryToolbar";
import { MixPanel } from "./components/mix/MixPanel";
import { PlayerBar } from "./components/PlayerBar";
import { PlayerDeck } from "./components/PlayerDeck";
import { DeleteDialog, DuplicateDialog } from "./components/PlaylistDialogs";
import { PlaylistTree } from "./components/PlaylistTree";
import { RuleBuilder } from "./components/RuleBuilder";
import { SessionView } from "./components/SessionView";
import { SettingsDialog } from "./components/SettingsDialog";
import { SmartListHeader } from "./components/SmartListHeader";
import { TagManagerDialog } from "./components/TagManagerDialog";
import { TagsSidebarEntry } from "./components/TagsSidebarEntry";
import { ToastHost } from "./components/ToastHost";
import { TrackInfoPanel } from "./components/TrackDetailPanel";
import { TrackTable } from "./components/TrackTable";
import { useTrackTable, visibleColumns } from "./components/trackTableModel";
import { Sheet } from "./components/ui/Sheet";
import { keepsSpace } from "./lib/keys";
import { dismissSplash } from "./lib/splash";
import { useAnalysis } from "./store/analysis";
import { useHistory } from "./store/history";
import { useLibrary } from "./store/library";
import { useMixShortcuts } from "./store/mixShortcuts";
import { usePanels } from "./store/panels";
import { togglePlayer } from "./store/player";
import { usePlaylists } from "./store/playlists";
import { useRules } from "./store/rules";
import { startUiSync, useSettings } from "./store/settings";
import { useTags } from "./store/tags";

/** Pressed alone, these change nothing on screen. */
const MODIFIERS = new Set(["Shift", "Control", "Alt", "Meta"]);

export default function App() {
  const table = useTrackTable();
  const error = useLibrary((s) => s.error);
  const filterBarOpen = usePanels((s) => s.filterBarOpen);
  const infoOpen = usePanels((s) => s.infoOpen);
  const sessionOpen = useHistory((s) => s.openId !== null);
  const mixOpen = usePanels((s) => s.mixOpen);
  const rulesOpen = useRules((s) => s.open);
  useMixShortcuts(mixOpen);

  useEffect(() => {
    dismissSplash();
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
    void useTags.getState().load();
    void useHistory.getState().load();
    void useAnalysis.getState().subscribe();
    return () => {
      live = false;
      stopSync?.();
    };
  }, []);

  useEffect(() => {
    const root = document.documentElement;
    function onKeyDown(e: KeyboardEvent) {
      const space = e.code === "Space" && !keepsSpace(e.target);
      // Hides the focus ring a key press would draw on the last clicked
      // control (index.css); any other key brings it back.
      if (!MODIFIERS.has(e.key)) root.toggleAttribute("data-space-key", space);
      if (e.key === "," && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        useSettings.getState().setDialogOpen(true);
        return;
      }
      if (!space) return;
      // Also stops a focused button or checkbox from taking the press.
      e.preventDefault();
      if (!e.repeat) togglePlayer();
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
          <HistoryTree />
          <TagsSidebarEntry />
        </aside>
        <main className="flex min-w-0 flex-1 flex-col">
          {sessionOpen ? (
            <SessionView />
          ) : (
            <>
              <LibraryToolbar table={table} />
              {filterBarOpen && <FilterBar />}
              {filterBarOpen && rulesOpen && <RuleBuilder />}
              <SmartListHeader />
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
                <EmptyResult />
                <Sheet open={infoOpen}>
                  <TrackInfoPanel />
                </Sheet>
              </div>
            </>
          )}
        </main>
      </div>
      <PlayerBar />
      {mixOpen ? <MixPanel /> : <PlayerDeck />}
      <DuplicateDialog />
      <DeleteDialog />
      <ImportWizard />
      <SettingsDialog />
      <DevicesDialog />
      <AdoptDialog />
      <ExportWizard />
      <ConvertDialog />
      <TagManagerDialog />
      <HistoryImportDialog />
      <ToastHost />
    </div>
  );
}
