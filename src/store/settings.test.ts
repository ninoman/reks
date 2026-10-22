import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { Settings } from "../lib/ipc";

vi.mock("../lib/ipc", () => ({
  settingsGet: vi.fn(),
  settingsSet: vi.fn(),
  libraryRoots: vi.fn(),
  libraryQuery: vi.fn(),
}));

const ipc = await import("../lib/ipc");
const { useLibrary } = await import("./library");
const { usePanels } = await import("./panels");
const { usePlayer } = await import("./player");
const { startUiSync, UI_SAVE_MS, useSettings } = await import("./settings");

const settingsOf = (ui: Partial<Settings["ui"]> = {}): Settings => ({
  audioDevice: null,
  keyNotation: "camelot",
  cacheDir: "/cache",
  anlzPreference: "rekordboxFirst",
  onelibraryKeyBuiltin: false,
  ui: {
    hiddenColumns: null,
    filterBarOpen: true,
    filtersEnabled: true,
    infoOpen: false,
    volume: 1,
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
      }),
    );
    await useSettings.getState().load(["album"]);
    expect(useLibrary.getState().columnVisibility).toEqual({ bpm: false });
    expect(useLibrary.getState().filtersEnabled).toBe(false);
    expect(usePanels.getState()).toMatchObject({ filterBarOpen: false, infoOpen: true });
    expect(usePlayer.getState().status.volume).toBe(0.4);
    expect(useSettings.getState().settings?.keyNotation).toBe("camelot");
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
      },
    });
    // A change undone before the delay saves nothing.
    usePanels.getState().toggleFilterBar();
    usePanels.getState().toggleFilterBar();
    vi.advanceTimersByTime(UI_SAVE_MS);
    expect(ipc.settingsSet).toHaveBeenCalledTimes(1);
    stop();
  });
});
