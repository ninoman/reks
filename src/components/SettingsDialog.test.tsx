// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { BackupInfo, LibraryRoot, Settings } from "../lib/ipc";

vi.mock("../lib/ipc", () => ({
  DECKS: 4,
  settingsGet: vi.fn(),
  settingsSet: vi.fn(),
  libraryRoots: vi.fn(),
  libraryRootAdd: vi.fn(),
  libraryRootRemove: vi.fn(),
  cacheDirMove: vi.fn(),
  pickFolder: vi.fn(),
  audioDevices: vi.fn(),
  mixDeviceOptions: vi.fn(),
  libraryQuery: vi.fn(),
  backupsList: vi.fn(),
  backupStatus: vi.fn(),
  backupNow: vi.fn(),
  backupRestore: vi.fn(),
  backupRestart: vi.fn(),
  backupShowFolder: vi.fn(),
}));

const ipc = await import("../lib/ipc");
const { startThemeSync, useSettings } = await import("../store/settings");
const { useMix } = await import("../store/mix");
const { useBackups } = await import("../store/backups");
const { SettingsDialog } = await import("./SettingsDialog");

const settings: Settings = {
  audioDevice: "USB DAC",
  audioSampleRate: null,
  audioBufferFrames: null,
  audioCue: null,
  keyNotation: "classical",
  deckWaveform: "rgb",
  theme: "booth",
  mixerEq: "fourBand",
  mixerView: "simple",
  mixerFilters: "show",
  cacheDir: "/Users/dj/Library/Caches/reks",
  anlzPreference: "rekordboxFirst",
  export: {
    missingAnalysis: "generate",
    artwork: false,
    cleanUp: true,
    dividers: true,
    activeLoops: false,
    onelibrary: false,
  },
  backupKeep: 10,
  tagsInComments: false,
  onelibraryKeyBuiltin: false,
  ui: {
    hiddenColumns: null,
    filterBarOpen: true,
    filtersEnabled: true,
    infoOpen: false,
    volume: 1,
    rowDensity: "default",
    columnsVersion: 1,
    columnWidths: {},
    sidebarWidth: null,
    sheetWidth: null,
    filterFields: null,
    filterSaveTarget: "filter",
  },
};
const roots: LibraryRoot[] = [
  { id: 1, path: "/Music/House", lastScan: null },
  { id: 2, path: "/Music/Techno", lastScan: "2026-10-19T10:00:00Z" },
];

const backup = (over: Partial<BackupInfo>): BackupInfo => ({
  name: "reks-20261024T153012123Z-startup.sqlite",
  takenAt: "2026-10-24T15:30:12.123Z",
  reason: "startup",
  sizeBytes: 2048,
  contents: { schemaVersion: 12, tracks: 3012, playlists: 41 },
  problem: null,
  tooNew: false,
  ...over,
});

beforeEach(() => {
  vi.mocked(ipc.backupsList).mockResolvedValue([]);
  vi.mocked(ipc.backupStatus).mockResolvedValue({
    dir: "/data/library-backups",
    lastFailure: null,
  });
  vi.mocked(ipc.settingsGet).mockResolvedValue(settings);
  vi.mocked(ipc.libraryRoots).mockResolvedValue(roots);
  vi.mocked(ipc.audioDevices).mockResolvedValue(["Built-in Output"]);
  vi.mocked(ipc.mixDeviceOptions).mockResolvedValue({
    rates: [44_100],
    defaultRate: 44_100,
    channels: 2,
    defaultCue: { type: "off" },
    bufferFrames: [64, 1024],
  });
  vi.mocked(ipc.libraryRootRemove).mockResolvedValue();
  vi.mocked(ipc.settingsSet).mockImplementation(async (p) => ({ ...settings, ...p }) as Settings);
  vi.mocked(ipc.cacheDirMove).mockResolvedValue({ ...settings, cacheDir: "/Volumes/SSD/cache" });
});
const initialMix = useMix.getState();

afterEach(() => {
  cleanup();
  useSettings.setState({ dialogOpen: false });
  useMix.setState(initialMix, true);
  useBackups.setState({ backups: null, status: null, busy: null, error: null });
});

/** As at start-up: the saved output reaches the mix store, then the dialog opens. */
async function openDialog() {
  await act(async () => useSettings.getState().load({ hidden: [], version: 1, hiddenSince: [] }));
  await act(async () => useSettings.getState().setDialogOpen(true));
  render(<SettingsDialog />);
  await screen.findByText("/Music/House");
}

/** Radix tabs switch on mouse down, not click. */
function showTab(name: string) {
  fireEvent.mouseDown(screen.getByRole("tab", { name }));
  return screen.getByRole("tabpanel", { name });
}

describe("SettingsDialog", () => {
  it("lists library folders and removes one", async () => {
    await openDialog();
    expect(screen.getByText("Never scanned")).toBeTruthy();
    vi.mocked(ipc.libraryRoots).mockResolvedValue(roots.slice(1));
    fireEvent.click(screen.getByRole("button", { name: "Remove /Music/House" }));
    await waitFor(() => expect(screen.queryByText("/Music/House")).toBeNull());
    expect(ipc.libraryRootRemove).toHaveBeenCalledWith(1);
  });

  it("groups settings in tabs, opening on the library folders", async () => {
    await openDialog();
    const tabs = screen.getAllByRole("tab").map((t) => t.textContent);
    expect(tabs).toEqual(["Library", "Playback", "Display", "Decks", "Mixer", "Export"]);
    const library = screen.getByRole("tabpanel", { name: "Library" });
    expect(library.textContent).toContain("/Music/House");
    expect(library.textContent).toContain("/Users/dj/Library/Caches/reks");
    expect(screen.queryByRole("radio", { name: /Camelot/ })).toBeNull();
    const display = showTab("Display");
    expect(display.textContent).toContain("Key notation");
    expect(screen.queryByText("/Music/House")).toBeNull();
  });

  it("stores the mixer view and filters; Filters is disabled in Advanced", async () => {
    await openDialog();
    showTab("Mixer");
    const view = screen.getByRole("radiogroup", { name: "Mixer view" });
    expect(view.querySelector('[aria-checked="true"]')?.textContent).toContain("Simple");
    const filters = screen.getByRole("switch", { name: "Show filters in the Simple view" });
    expect(filters.getAttribute("aria-checked")).toBe("true");
    fireEvent.click(filters);
    await waitFor(() => expect(ipc.settingsSet).toHaveBeenCalledWith({ mixerFilters: "hide" }));
    fireEvent.click(screen.getByRole("radio", { name: "Advanced" }));
    await waitFor(() => expect(ipc.settingsSet).toHaveBeenCalledWith({ mixerView: "advanced" }));
    // Advanced always shows the filters: on, and not the user's to change.
    await waitFor(() => expect(filters.hasAttribute("disabled")).toBe(true));
    expect(filters.getAttribute("aria-checked")).toBe("true");
  });

  it("stores the EQ layout; Filters is disabled in 3-band, also in Simple", async () => {
    vi.mocked(ipc.settingsSet).mockResolvedValue({ ...settings, mixerEq: "threeBand" });
    await openDialog();
    showTab("Mixer");
    const eq = screen.getByRole("radiogroup", { name: "EQ" });
    expect(eq.querySelector('[aria-checked="true"]')?.textContent).toContain("4-band");
    const filters = screen.getByRole("switch", { name: "Show filters in the Simple view" });
    expect(filters.hasAttribute("disabled")).toBe(false);
    fireEvent.click(screen.getByRole("radio", { name: "3-band" }));
    await waitFor(() => expect(ipc.settingsSet).toHaveBeenCalledWith({ mixerEq: "threeBand" }));
    // 3-band has no filters X and Y: off, with the reason under the switch.
    await waitFor(() => expect(filters.hasAttribute("disabled")).toBe(true));
    expect(filters.getAttribute("aria-checked")).toBe("false");
    expect(screen.getByText(/each channel's FX knob is its filter/)).toBeTruthy();
  });

  it("opens on the tab it is asked for and focuses its first control", async () => {
    await act(async () => useSettings.getState().load({ hidden: [], version: 1, hiddenSince: [] }));
    await act(async () => useSettings.getState().setDialogOpen(true, "mixer"));
    render(<SettingsDialog />);
    const panel = await screen.findByRole("tabpanel", { name: "Mixer" });
    expect(screen.getByRole("tab", { name: "Mixer" }).getAttribute("aria-selected")).toBe("true");
    await waitFor(() => expect(document.activeElement).toBe(panel.querySelector('[role="radio"]')));
  });

  it("has no OneLibrary key field: release builds have the key built in", async () => {
    await openDialog();
    expect(screen.queryByLabelText("OneLibrary key")).toBeNull();
  });

  it("stores the key notation", async () => {
    await openDialog();
    showTab("Display");
    fireEvent.click(screen.getByRole("radio", { name: /Camelot/ }));
    await waitFor(() => expect(ipc.settingsSet).toHaveBeenCalledWith({ keyNotation: "camelot" }));
    expect(useSettings.getState().settings?.keyNotation).toBe("camelot");
  });

  it("switches the theme and stores it", async () => {
    const stop = startThemeSync();
    await openDialog();
    const display = showTab("Display");
    expect(display.querySelector('[aria-checked="true"]')?.textContent).toContain("Booth");
    fireEvent.click(screen.getByRole("radio", { name: /Paper/ }));
    await waitFor(() => expect(ipc.settingsSet).toHaveBeenCalledWith({ theme: "paper" }));
    expect(document.documentElement.dataset.theme).toBe("paper");
    expect(localStorage.getItem("reks.theme")).toBe("paper");
    stop();
  });

  it("stores the deck waveform style", async () => {
    await openDialog();
    const decks = showTab("Decks");
    expect(decks.querySelector('[aria-checked="true"]')?.textContent).toContain("RGB");
    fireEvent.click(screen.getByRole("radio", { name: /3-band/ }));
    await waitFor(() =>
      expect(ipc.settingsSet).toHaveBeenCalledWith({ deckWaveform: "threeBand" }),
    );
    expect(useSettings.getState().settings?.deckWaveform).toBe("threeBand");
  });

  it("keeps a chosen device that is not connected in the list", async () => {
    await openDialog();
    showTab("Playback");
    const select = screen.getByRole("combobox", { name: "Output device" }) as HTMLSelectElement;
    await waitFor(() => expect(select.options.length).toBe(3));
    expect(select.value).toBe("USB DAC");
    expect(select.selectedOptions[0]?.textContent).toBe("USB DAC (not connected)");
  });

  it("stores the output device and hands it to the engine", async () => {
    await openDialog();
    showTab("Playback");
    const select = screen.getByRole("combobox", { name: "Output device" }) as HTMLSelectElement;
    await waitFor(() => expect(select.options.length).toBe(3));
    fireEvent.change(select, { target: { value: "Built-in Output" } });
    await waitFor(() =>
      expect(ipc.settingsSet).toHaveBeenCalledWith({
        audioDevice: "Built-in Output",
        audioSampleRate: null,
        audioBufferFrames: null,
      }),
    );
    expect(useMix.getState().config.device).toBe("Built-in Output");
  });

  it("stores the buffer and headphone cue, offering only what the device takes", async () => {
    await openDialog();
    showTab("Playback");
    const buffer = screen.getByRole("combobox", { name: /Buffer/ }) as HTMLSelectElement;
    // 64…1024 frames: 2048 is not offered.
    await waitFor(() => expect(buffer.options.length).toBe(6));
    fireEvent.change(buffer, { target: { value: "256" } });
    await waitFor(() =>
      expect(ipc.settingsSet).toHaveBeenLastCalledWith(
        expect.objectContaining({ audioBufferFrames: 256 }),
      ),
    );
    expect(useMix.getState().config.bufferFrames).toBe(256);

    const cue = screen.getByRole("combobox", { name: /Headphone cue/ }) as HTMLSelectElement;
    // Two outputs: no cue on 3/4.
    expect(
      (screen.getByRole("option", { name: /cue on 3\/4/ }) as HTMLOptionElement).disabled,
    ).toBe(true);
    fireEvent.change(cue, { target: { value: "splitMono" } });
    await waitFor(() =>
      expect(ipc.settingsSet).toHaveBeenLastCalledWith(
        expect.objectContaining({ audioCue: { type: "splitMono" } }),
      ),
    );
  });

  it("asks before moving the cache", async () => {
    vi.mocked(ipc.pickFolder).mockResolvedValue("/Volumes/SSD/cache");
    await openDialog();
    fireEvent.click(screen.getByRole("button", { name: "Move…" }));
    const move = await screen.findByRole("button", { name: "Move" });
    expect(ipc.cacheDirMove).not.toHaveBeenCalled();
    fireEvent.click(move);
    await screen.findByText("/Volumes/SSD/cache");
    expect(ipc.cacheDirMove).toHaveBeenCalledWith("/Volumes/SSD/cache");
  });

  it("says so when there are no backups yet", async () => {
    await openDialog();
    expect(await screen.findByText(/No backups yet/)).toBeTruthy();
  });

  it("lists backups with their counts and restores one after asking", async () => {
    vi.mocked(ipc.backupsList).mockResolvedValue([
      backup({}),
      backup({
        name: "reks-20261023T100000000Z-migration.sqlite",
        takenAt: "2026-10-23T10:00:00.000Z",
        reason: "migration",
      }),
    ]);
    vi.mocked(ipc.backupRestore).mockResolvedValue();
    vi.mocked(ipc.backupRestart).mockResolvedValue();
    await openDialog();
    const list = await screen.findByRole("list", { name: "Backups" });
    expect(list.textContent).toContain("3012 tracks, 41 playlists");
    expect(list.textContent).toContain("Before a library update");

    const restoreButtons = screen.getAllByRole("button", { name: /^Restore the backup from/ });
    fireEvent.click(restoreButtons[1] as HTMLElement);
    expect(ipc.backupRestore).not.toHaveBeenCalled();
    fireEvent.click(await screen.findByRole("button", { name: "Restore and restart" }));
    await waitFor(() =>
      expect(ipc.backupRestore).toHaveBeenCalledWith("reks-20261023T100000000Z-migration.sqlite"),
    );
    await waitFor(() => expect(ipc.backupRestart).toHaveBeenCalled());
  });

  it("does not offer to restore a damaged backup or one from a newer reks", async () => {
    vi.mocked(ipc.backupsList).mockResolvedValue([
      backup({ problem: "file is not a database", contents: null }),
      backup({ name: "reks-20261022T100000000Z-manual.sqlite", tooNew: true }),
    ]);
    await openDialog();
    await screen.findByRole("list", { name: "Backups" });
    const buttons = screen.getAllByRole("button", { name: /^Restore the backup from/ });
    expect(buttons.map((b) => (b as HTMLButtonElement).disabled)).toEqual([true, true]);
    expect(screen.getByText("Damaged")).toBeTruthy();
    expect(screen.getByText("Newer reks")).toBeTruthy();
  });

  it("shows a failed backup and refused restores", async () => {
    vi.mocked(ipc.backupStatus).mockResolvedValue({
      dir: "/data/library-backups",
      lastFailure: { reason: "startup", at: "2026-10-24T16:00:00.000Z", message: "disk full" },
    });
    vi.mocked(ipc.backupsList).mockResolvedValue([backup({})]);
    vi.mocked(ipc.backupRestore).mockRejectedValue(new Error("an export is running"));
    await openDialog();
    expect(await screen.findByText(/The last backup failed: disk full/)).toBeTruthy();
    fireEvent.click(await screen.findByRole("button", { name: /^Restore the backup from/ }));
    fireEvent.click(await screen.findByRole("button", { name: "Restore and restart" }));
    expect(await screen.findByText("an export is running")).toBeTruthy();
    expect(ipc.backupRestart).not.toHaveBeenCalled();
  });

  it("backs up on request and stores how many recent backups to keep", async () => {
    vi.mocked(ipc.backupNow).mockResolvedValue(backup({ reason: "manual" }));
    await openDialog();
    fireEvent.click(await screen.findByRole("button", { name: "Back up now" }));
    await waitFor(() => expect(ipc.backupNow).toHaveBeenCalled());

    fireEvent.doubleClick(screen.getByRole("button", { name: "Recent backups kept" }));
    const field = screen.getByRole("textbox", { name: "Recent backups kept" });
    fireEvent.change(field, { target: { value: "20" } });
    fireEvent.keyDown(field, { key: "Enter" });
    await waitFor(() =>
      expect(ipc.settingsSet).toHaveBeenLastCalledWith(expect.objectContaining({ backupKeep: 20 })),
    );
  });
});
