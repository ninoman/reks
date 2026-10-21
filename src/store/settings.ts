/**
 * Settings (stored in Rust, `reks-core` `settings.rs`): loaded at start-up,
 * where they seed the library, panels and mix stores (volume, output
 * device); edited in the
 * Settings dialog. Library window state (`UiState`) is saved by
 * `startUiSync`, `UI_SAVE_MS` after the last change.
 */
import { create } from "zustand";

import { keyLabel } from "../lib/format";

import {
  cacheDirMove,
  type Key,
  type LibraryRoot,
  libraryRootAdd,
  libraryRootRemove,
  libraryRoots,
  type Settings,
  type SettingsPatch,
  settingsGet,
  settingsSet,
  type UiState,
} from "../lib/ipc";
import { useLibrary } from "./library";
import { useMix } from "./mix";
import { usePanels } from "./panels";

export const UI_SAVE_MS = 300;

export interface SettingsStore {
  settings: Settings | null;
  roots: LibraryRoot[];
  dialogOpen: boolean;
  /** A cache move is running. */
  moving: boolean;
  error: string | null;

  /** Loads settings and roots; `defaultHidden` seeds columns until they were changed. */
  load(defaultHidden: string[]): Promise<void>;
  /** Re-reads the settings. */
  refresh(): Promise<void>;
  update(patch: SettingsPatch): Promise<void>;
  loadRoots(): Promise<void>;
  addRoot(path: string): Promise<LibraryRoot | null>;
  removeRoot(id: number): Promise<void>;
  moveCache(path: string): Promise<void>;
  setDialogOpen(open: boolean): void;
}

function message(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

/** The window state now, as `UiState`. */
export function currentUi(): UiState {
  const lib = useLibrary.getState();
  const panels = usePanels.getState();
  return {
    hiddenColumns: Object.entries(lib.columnVisibility)
      .filter(([, shown]) => !shown)
      .map(([id]) => id)
      .sort(),
    filterBarOpen: panels.filterBarOpen,
    filtersEnabled: lib.filtersEnabled,
    infoOpen: panels.infoOpen,
    volume: useMix.getState().volume,
  };
}

/** Puts `ui` into the library, panels and mix stores. */
function applyUi(ui: UiState, defaultHidden: string[]) {
  const hidden = ui.hiddenColumns ?? defaultHidden;
  const lib = useLibrary.getState();
  lib.setColumnVisibility(Object.fromEntries(hidden.map((id) => [id, false])));
  lib.setFiltersEnabled(ui.filtersEnabled);
  usePanels.setState({ filterBarOpen: ui.filterBarOpen, infoOpen: ui.infoOpen });
  useMix.getState().setVolume(ui.volume);
}

export const useSettings = create<SettingsStore>()((set, get) => {
  async function run(action: () => Promise<void>) {
    try {
      await action();
      set({ error: null });
    } catch (e) {
      set({ error: message(e) });
    }
  }

  return {
    settings: null,
    roots: [],
    dialogOpen: false,
    moving: false,
    error: null,

    async load(defaultHidden) {
      await run(async () => {
        const [settings, roots] = await Promise.all([settingsGet(), libraryRoots()]);
        applyUi(settings.ui, defaultHidden);
        set({ settings, roots });
        await useMix.getState().setConfig({ device: settings.audioDevice });
      });
    },
    refresh: () => run(async () => set({ settings: await settingsGet() })),
    update: (patch) => run(async () => set({ settings: await settingsSet(patch) })),
    loadRoots: () => run(async () => set({ roots: await libraryRoots() })),

    async addRoot(path) {
      let root: LibraryRoot | null = null;
      await run(async () => {
        root = await libraryRootAdd(path);
        set({ roots: await libraryRoots() });
      });
      return root;
    },
    removeRoot: (id) =>
      run(async () => {
        await libraryRootRemove(id);
        set({ roots: await libraryRoots() });
      }),

    async moveCache(path) {
      if (get().moving) return;
      set({ moving: true });
      await run(async () => set({ settings: await cacheDirMove(path) }));
      set({ moving: false });
    },
    setDialogOpen(dialogOpen) {
      set({ dialogOpen, error: null });
      // The transport's device menu and scans change settings and roots too.
      if (dialogOpen) void Promise.all([get().refresh(), get().loadRoots()]);
    },
  };
});

/**
 * Saves the window state whenever it changes, debounced. Call after `load`.
 * Returns the unsubscribe function.
 */
export function startUiSync(): () => void {
  let saved = JSON.stringify(currentUi());
  let timer: ReturnType<typeof setTimeout> | null = null;
  const save = () => {
    timer = null;
    const ui = currentUi();
    const json = JSON.stringify(ui);
    if (json === saved) return;
    saved = json;
    void useSettings.getState().update({ ui });
  };
  const check = () => {
    if (useSettings.getState().settings === null) return;
    if (timer === null && JSON.stringify(currentUi()) === saved) return;
    if (timer !== null) clearTimeout(timer);
    timer = setTimeout(save, UI_SAVE_MS);
  };
  const unsubs = [
    useLibrary.subscribe(check),
    usePanels.subscribe(check),
    useMix.subscribe((s, prev) => s.volume !== prev.volume && check()),
  ];
  return () => {
    if (timer !== null) clearTimeout(timer);
    for (const u of unsubs) u();
  };
}

/**
 * Plays through `device` from now on (`null`: system default) and stores the
 * choice; a running output reopens on it.
 */
export async function chooseOutputDevice(device: string | null): Promise<void> {
  await useMix.getState().setDevice(device);
  await useSettings.getState().update({ audioDevice: device });
}

/** A function labelling canonical keys in the notation chosen in Settings. */
export function useKeyLabel(): (key: Key) => string {
  const notation = useSettings((s) => s.settings?.keyNotation ?? "classical");
  const keys = useLibrary((s) => s.facets?.keys);
  return (key) => keyLabel(key, notation, keys);
}
