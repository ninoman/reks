// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { LibraryRoot, Settings } from "../lib/ipc";

vi.mock("../lib/ipc", () => ({
  DECKS: 4,
  settingsGet: vi.fn(),
  settingsSet: vi.fn(),
  libraryRoots: vi.fn(),
  libraryRootAdd: vi.fn(),
  libraryRootRemove: vi.fn(),
  cacheDirMove: vi.fn(),
  audioDevices: vi.fn(),
  mixDeviceOptions: vi.fn(),
  libraryQuery: vi.fn(),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

const ipc = await import("../lib/ipc");
const dialog = await import("@tauri-apps/plugin-dialog");
const { startThemeSync, useSettings } = await import("../store/settings");
const { useMix } = await import("../store/mix");
const { SettingsDialog } = await import("./SettingsDialog");

const settings: Settings = {
  audioDevice: "USB DAC",
  audioSampleRate: null,
  audioBufferFrames: null,
  audioCue: null,
  keyNotation: "classical",
  deckWaveform: "rgb",
  theme: "booth",
  syncAlign: "bar",
  mixerEq: "fourBand",
  mixerView: "simple",
  mixerFilters: "show",
  cacheDir: "/Users/dj/Library/Caches/reks",
  anlzPreference: "rekordboxFirst",
  onelibraryKeyBuiltin: false,
  ui: {
    hiddenColumns: null,
    filterBarOpen: true,
    filtersEnabled: true,
    infoOpen: false,
    volume: 1,
  },
};
const roots: LibraryRoot[] = [
  { id: 1, path: "/Music/House", lastScan: null },
  { id: 2, path: "/Music/Techno", lastScan: "2026-10-19T10:00:00Z" },
];

beforeEach(() => {
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
});

/** As at start-up: the saved output reaches the mix store, then the dialog opens. */
async function openDialog() {
  await act(async () => useSettings.getState().load([]));
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
    expect(tabs).toEqual(["Library", "Playback", "Display", "Decks", "Mixer"]);
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
    const filters = screen.getByRole("radiogroup", { name: "Filters" });
    expect(filters.querySelector('[aria-checked="true"]')?.textContent).toBe("Show");
    fireEvent.click(screen.getByRole("radio", { name: "Hide" }));
    await waitFor(() => expect(ipc.settingsSet).toHaveBeenCalledWith({ mixerFilters: "hide" }));
    fireEvent.click(screen.getByRole("radio", { name: /Advanced/ }));
    await waitFor(() => expect(ipc.settingsSet).toHaveBeenCalledWith({ mixerView: "advanced" }));
    for (const name of ["Show", "Hide"]) {
      expect(screen.getByRole("radio", { name }).hasAttribute("disabled")).toBe(true);
    }
  });

  it("stores the EQ layout; Filters is disabled in 3-band, also in Simple", async () => {
    vi.mocked(ipc.settingsSet).mockResolvedValue({ ...settings, mixerEq: "threeBand" });
    await openDialog();
    showTab("Mixer");
    const eq = screen.getByRole("radiogroup", { name: "EQ" });
    expect(eq.querySelector('[aria-checked="true"]')?.textContent).toContain("4-band");
    expect(screen.getByRole("radio", { name: "Show" }).hasAttribute("disabled")).toBe(false);
    fireEvent.click(screen.getByRole("radio", { name: /3-band/ }));
    await waitFor(() => expect(ipc.settingsSet).toHaveBeenCalledWith({ mixerEq: "threeBand" }));
    await waitFor(() =>
      expect(screen.getByRole("radio", { name: "Show" }).hasAttribute("disabled")).toBe(true),
    );
  });

  it("opens on the tab it is asked for and focuses its first control", async () => {
    await act(async () => useSettings.getState().load([]));
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
    vi.mocked(dialog.open).mockResolvedValue("/Volumes/SSD/cache");
    await openDialog();
    fireEvent.click(screen.getByRole("button", { name: "Move…" }));
    const move = await screen.findByRole("button", { name: "Move" });
    expect(ipc.cacheDirMove).not.toHaveBeenCalled();
    fireEvent.click(move);
    await screen.findByText("/Volumes/SSD/cache");
    expect(ipc.cacheDirMove).toHaveBeenCalledWith("/Volumes/SSD/cache");
  });
});
