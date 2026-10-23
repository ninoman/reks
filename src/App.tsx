/**
 * Library window: the app header (library-wide actions) across the top; under
 * it the deck section, as in rekordbox and Traktor: the player bar (deck
 * layout, output status, volume, output device), then the player deck alone
 * (1 deck, D30) or the mix panel (2 or 4 decks and the mixer, with its own
 * shortcuts, `src/lib/mixKeys.ts`). Below that the library: the playlist,
 * Devices and Histories trees on the left (an open stick list or history
 * session replaces the toolbar, filters and table); toolbar, filter bar and track table in the
 * middle, with Shift+clicked playlists split under it (`TrackLists`) and the
 * track info panel and the My Tags drawer sliding over the tables' right edge
 * when their toolbar buttons are on. Space plays and pauses the player deck
 * wherever focus is, except in text fields, dialogs and menus; Cmd/Ctrl+,
 * opens Settings.
 * Settings load first and restore the window state (columns, panels, volume)
 * and the theme.
 * Mounted only once the library is open (main.tsx); takes the splash down.
 */
import { useEffect } from "react";
import { AdoptDialog } from "./components/AdoptDialog";
import { AppHeader } from "./components/AppHeader";
import { ConvertDialog } from "./components/ConvertDialog";
import { DEFAULT_HIDDEN } from "./components/columns";
import { DevicesDialog } from "./components/DevicesDialog";
import { DeviceTree } from "./components/DeviceTree";
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
import { StickListView } from "./components/StickListView";
import { ToastHost } from "./components/ToastHost";
import { TrackLists } from "./components/TrackLists";
import { useTrackTable, visibleColumns } from "./components/trackTableModel";
import { keepsSpace, PLAYER_PLAY_KEY, waveZoomKey } from "./lib/keys";
import { dismissSplash } from "./lib/splash";
import { useAnalysis } from "./store/analysis";
import { useDeviceBrowser } from "./store/deviceBrowser";
import { useHistory } from "./store/history";
import { useLibrary } from "./store/library";
import { useMix } from "./store/mix";
import { useMixShortcuts } from "./store/mixShortcuts";
import { usePanels } from "./store/panels";
import { togglePlayer } from "./store/player";
import { usePlaylists } from "./store/playlists";
import { useRules } from "./store/rules";
import { startThemeSync, startUiSync, useSettings } from "./store/settings";
import { useTags } from "./store/tags";

/** Pressed alone, these change nothing on screen. */
const MODIFIERS = new Set(["Shift", "Control", "Alt", "Meta"]);

export default function App() {
  const table = useTrackTable();
  const error = useLibrary((s) => s.error);
  const filterBarOpen = usePanels((s) => s.filterBarOpen);
  const sessionOpen = useHistory((s) => s.openId !== null);
  const stickListOpen = useDeviceBrowser((s) => s.open !== null);
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
    const stopThemeSync = startThemeSync();
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
      stopThemeSync();
    };
  }, []);

  useEffect(() => {
    const root = document.documentElement;
    function onKeyDown(e: KeyboardEvent) {
      const space = e.code === PLAYER_PLAY_KEY.code && !keepsSpace(e.target);
      // Hides the focus ring a key press would draw on the last clicked
      // control (index.css); any other key brings it back.
      if (!MODIFIERS.has(e.key)) root.toggleAttribute("data-space-key", space);
      if (e.key === "," && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        useSettings.getState().setDialogOpen(true);
        return;
      }
      const zoom = waveZoomKey(e);
      if (zoom !== null) {
        e.preventDefault();
        useMix.getState().zoom(zoom);
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
      <div className="flex shrink-0 flex-col border-b border-border bg-bg-subtle">
        <PlayerBar />
        {mixOpen ? <MixPanel /> : <PlayerDeck />}
      </div>
      <div className="flex min-h-0 flex-1">
        <aside className="flex w-(--sidebar-w) shrink-0 flex-col border-r border-border bg-bg-subtle">
          <PlaylistTree />
          <DeviceTree />
          <HistoryTree />
        </aside>
        <main className="flex min-w-0 flex-1 flex-col">
          {stickListOpen ? (
            <StickListView />
          ) : sessionOpen ? (
            <SessionView />
          ) : (
            <>
              <LibraryToolbar table={table} />
              {filterBarOpen && <FilterBar />}
              {filterBarOpen && rulesOpen && <RuleBuilder />}
              {error !== null && (
                <div
                  className="border-b border-border px-2.5 py-1 text-danger select-text"
                  role="alert"
                >
                  {error}
                </div>
              )}
              <TrackLists columns={visibleColumns(table)} />
            </>
          )}
        </main>
      </div>
      <DuplicateDialog />
      <DeleteDialog />
      <ImportWizard />
      <SettingsDialog />
      <DevicesDialog />
      <AdoptDialog />
      <ExportWizard />
      <ConvertDialog />
      <HistoryImportDialog />
      <ToastHost />
    </div>
  );
}
