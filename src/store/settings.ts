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
  type MixConfig,
  type Settings,
  type SettingsPatch,
  settingsGet,
  settingsSet,
  type Theme,
  type UiState,
} from "../lib/ipc";
import { applyTheme } from "../lib/theme";
import { useLibrary } from "./library";
import { useMix } from "./mix";
import { usePanels } from "./panels";

export const UI_SAVE_MS = 300;

/** The Settings dialog's tabs, in sidebar order. */
export type SettingsTab = "library" | "playback" | "display" | "decks" | "mixer";

export interface SettingsStore {
  settings: Settings | null;
  roots: LibraryRoot[];
  dialogOpen: boolean;
  /** The tab the dialog opens on. */
  dialogTab: SettingsTab;
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
  /** Opens or closes the Settings dialog; `tab` picks the tab it opens on. */
  setDialogOpen(open: boolean, tab?: SettingsTab): void;
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
    dialogTab: "library",
    moving: false,
    error: null,

    async load(defaultHidden) {
      await run(async () => {
        const [settings, roots] = await Promise.all([settingsGet(), libraryRoots()]);
        applyUi(settings.ui, defaultHidden);
        set({ settings, roots });
        const { audioDevice, audioSampleRate, audioBufferFrames, audioCue } = settings;
        await useMix.getState().setConfig({
          device: audioDevice,
          sampleRate: audioSampleRate,
          bufferFrames: audioBufferFrames,
          // Unset: each device gets its own default routing.
          ...(audioCue === null ? {} : { cue: audioCue }),
        });
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
    setDialogOpen(dialogOpen, tab = "library") {
      set({ dialogOpen, dialogTab: tab, error: null });
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
 * Puts the theme on <html> whenever the settings bring a new one. A store
 * listener runs inside `set`, before React re-renders, so canvases that read
 * their colours while rendering get the new theme's. Returns the unsubscribe
 * function.
 */
export function startThemeSync(): () => void {
  return useSettings.subscribe((s, prev) => {
    const theme = s.settings?.theme;
    if (theme !== undefined && theme !== prev.settings?.theme) applyTheme(theme);
  });
}

/**
 * Opens the output as `patch` says from now on (device, rate, buffer, cue)
 * and stores the choice; a running output reopens with it. A new device goes
 * back to its own rate and buffer, and that is stored too.
 */
export async function chooseOutput(patch: Partial<MixConfig>): Promise<void> {
  await useMix.getState().setOutput(patch);
  const c = useMix.getState().config;
  await useSettings.getState().update({
    audioDevice: c.device,
    audioSampleRate: c.sampleRate,
    audioBufferFrames: c.bufferFrames,
    ...("cue" in patch ? { audioCue: c.cue } : {}),
  });
}

/** A function labelling canonical keys in the notation chosen in Settings. */
export function useKeyLabel(): (key: Key) => string {
  const notation = useSettings((s) => s.settings?.keyNotation ?? "classical");
  const keys = useLibrary((s) => s.facets?.keys);
  return (key) => keyLabel(key, notation, keys);
}

/** The theme in use; `null` until settings load (the last launch's theme is on <html>). */
export function useTheme(): Theme | null {
  return useSettings((s) => s.settings?.theme ?? null);
}

/** Whether SYNC lines up bars (Settings → Decks → Sync aligns; bar until settings load). */
export function syncBar(): boolean {
  return (useSettings.getState().settings?.syncAlign ?? "bar") === "bar";
}

/**
 * Keeps the mixer's view (Settings → Mixer) in the mix store, which sends
 * what the engine gets for the controls that view hides. Call after `load`;
 * returns the unsubscribe function.
 */
export function startMixerLookSync(): () => void {
  const apply = (s: Settings | null) => {
    if (s !== null)
      useMix.getState().setLook({ eq: s.mixerEq, view: s.mixerView, filters: s.mixerFilters });
  };
  apply(useSettings.getState().settings);
  return useSettings.subscribe((state, prev) => {
    if (state.settings !== prev.settings) apply(state.settings);
  });
}
