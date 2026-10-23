import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { Settings } from "../lib/ipc";

vi.mock("../lib/ipc", () => ({
  DECKS: 4,
  settingsGet: vi.fn(),
  settingsSet: vi.fn(),
  libraryRoots: vi.fn(),
  libraryQuery: vi.fn(),
}));

const ipc = await import("../lib/ipc");
const { useLibrary } = await import("./library");
const { usePanels } = await import("./panels");
const { useMix } = await import("./mix");
const { useRules } = await import("./rules");
const { fieldKey } = await import("./ruleTree");
const { startUiSync, UI_SAVE_MS, useSettings } = await import("./settings");

const settingsOf = (ui: Partial<Settings["ui"]> = {}): Settings => ({
  audioDevice: null,
  audioSampleRate: null,
  audioBufferFrames: null,
  audioCue: null,
  keyNotation: "camelot",
  deckWaveform: "rgb",
  theme: "graphite",
  syncAlign: "bar",
  mixerEq: "fourBand",
  mixerView: "simple",
  mixerFilters: "show",
  cacheDir: "/cache",
  anlzPreference: "rekordboxFirst",
  onelibraryKeyBuiltin: false,
  ui: {
    hiddenColumns: null,
    filterBarOpen: true,
    filtersEnabled: true,
    infoOpen: false,
    volume: 1,
    filterFields: null,
    filterSaveTarget: "filter",
    ...ui,
  },
});

beforeEach(() => {
  vi.useFakeTimers();
  vi.mocked(ipc.libraryQuery).mockResolvedValue({
    rows: [],
    total: 0,
    dividers: [],
    dividerView: false,
  });
  vi.mocked(ipc.libraryRoots).mockResolvedValue([]);
  vi.mocked(ipc.settingsSet).mockReset();
  useSettings.setState({ settings: null });
});
afterEach(() => vi.useRealTimers());

describe("settings store", () => {
  it("load seeds columns, filters, panels and volume", async () => {
    vi.mocked(ipc.settingsGet).mockResolvedValue(
      settingsOf({
        hiddenColumns: ["bpm"],
        filterBarOpen: false,
        filtersEnabled: false,
        infoOpen: true,
        volume: 0.4,
        filterFields: ["plays", "bpm"],
      }),
    );
    await useSettings.getState().load(["album"]);
    // The Filters drawer starts from the saved default fields.
    expect(useRules.getState().defaultFields).toEqual(["plays", "bpm"]);
    expect(
      useRules.getState().root.rules.map((r) => (r.kind === "row" ? fieldKey(r.value) : null)),
    ).toEqual(["plays", "bpm"]);
    expect(useLibrary.getState().columnVisibility).toEqual({ bpm: false });
    expect(useLibrary.getState().filtersEnabled).toBe(false);
    expect(usePanels.getState()).toMatchObject({ filterBarOpen: false, infoOpen: true });
    expect(useMix.getState().volume).toBe(0.4);
    expect(useSettings.getState().settings?.keyNotation).toBe("camelot");
  });

  it("saves the Filters drawer's default fields", async () => {
    vi.mocked(ipc.settingsGet).mockResolvedValue(settingsOf());
    vi.mocked(ipc.settingsSet).mockResolvedValue(settingsOf());
    await useSettings.getState().load([]);
    const stop = startUiSync();
    useRules.getState().keepDefault("plays", true);
    vi.advanceTimersByTime(UI_SAVE_MS);
    expect(vi.mocked(ipc.settingsSet).mock.lastCall?.[0].ui?.filterFields).toEqual([
      "bpm",
      "key",
      "genre",
      "rating",
      "plays",
    ]);
    stop();
  });

  it("uses the default hidden columns until they were changed", async () => {
    vi.mocked(ipc.settingsGet).mockResolvedValue(settingsOf());
    await useSettings.getState().load(["album", "label"]);
    expect(useLibrary.getState().columnVisibility).toEqual({ album: false, label: false });
  });

  it("saves window state once, after changes settle", async () => {
    vi.mocked(ipc.settingsGet).mockResolvedValue(settingsOf());
    vi.mocked(ipc.settingsSet).mockResolvedValue(settingsOf());
    await useSettings.getState().load(["album"]);
    const stop = startUiSync();
    usePanels.getState().toggleInfo();
    useLibrary.getState().setColumnVisibility({ album: false, bpm: false });
    vi.advanceTimersByTime(UI_SAVE_MS - 1);
    expect(ipc.settingsSet).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(ipc.settingsSet).toHaveBeenCalledTimes(1);
    expect(vi.mocked(ipc.settingsSet).mock.lastCall?.[0]).toEqual({
      ui: {
        hiddenColumns: ["album", "bpm"],
        filterBarOpen: true,
        filtersEnabled: true,
        infoOpen: true,
        volume: 1,
        filterFields: null,
        filterSaveTarget: "filter",
      },
    });
    // A change undone before the delay saves nothing.
    usePanels.getState().toggleFilterBar();
    usePanels.getState().toggleFilterBar();
    vi.advanceTimersByTime(UI_SAVE_MS);
    expect(ipc.settingsSet).toHaveBeenCalledTimes(1);
    // The player's volume lives in the mix store.
    useMix.getState().setVolume(0.6);
    vi.advanceTimersByTime(UI_SAVE_MS);
    expect(vi.mocked(ipc.settingsSet).mock.lastCall?.[0].ui?.volume).toBe(0.6);
    stop();
  });
});
