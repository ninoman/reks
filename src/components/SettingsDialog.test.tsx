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
const { useSettings } = await import("../store/settings");
const { useMix } = await import("../store/mix");
const { SettingsDialog } = await import("./SettingsDialog");

const settings: Settings = {
  audioDevice: "USB DAC",
  audioSampleRate: null,
  audioBufferFrames: null,
  audioCue: null,
  keyNotation: "classical",
  deckWaveform: "rgb",
  syncAlign: "bar",
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

describe("SettingsDialog", () => {
  it("lists library folders and removes one", async () => {
    await openDialog();
    expect(screen.getByText("Never scanned")).toBeTruthy();
    vi.mocked(ipc.libraryRoots).mockResolvedValue(roots.slice(1));
    fireEvent.click(screen.getByRole("button", { name: "Remove /Music/House" }));
    await waitFor(() => expect(screen.queryByText("/Music/House")).toBeNull());
    expect(ipc.libraryRootRemove).toHaveBeenCalledWith(1);
  });

  it("shows every setting in one view, folders together", async () => {
    await openDialog();
    const groups = screen.getAllByRole("region").map((g) => g.getAttribute("aria-label"));
    expect(groups).toEqual(["Folders", "Playback", "Display", "Decks"]);
    const folders = screen.getByRole("region", { name: "Folders" });
    expect(folders.textContent).toContain("/Music/House");
    expect(folders.textContent).toContain("/Users/dj/Library/Caches/reks");
    expect(screen.queryByRole("tab")).toBeNull();
  });

  it("has no OneLibrary key field: release builds have the key built in", async () => {
    await openDialog();
    expect(screen.queryByLabelText("OneLibrary key")).toBeNull();
  });

  it("stores the key notation", async () => {
    await openDialog();
    fireEvent.click(screen.getByRole("radio", { name: /Camelot/ }));
    await waitFor(() => expect(ipc.settingsSet).toHaveBeenCalledWith({ keyNotation: "camelot" }));
    expect(useSettings.getState().settings?.keyNotation).toBe("camelot");
  });

  it("stores the deck waveform style", async () => {
    await openDialog();
    const decks = screen.getByRole("region", { name: "Decks" });
    expect(decks.querySelector('[aria-checked="true"]')?.textContent).toContain("RGB");
    fireEvent.click(screen.getByRole("radio", { name: /3-band/ }));
    await waitFor(() =>
      expect(ipc.settingsSet).toHaveBeenCalledWith({ deckWaveform: "threeBand" }),
    );
    expect(useSettings.getState().settings?.deckWaveform).toBe("threeBand");
  });

  it("keeps a chosen device that is not connected in the list", async () => {
    await openDialog();
    const select = screen.getByRole("combobox", { name: "Output device" }) as HTMLSelectElement;
    await waitFor(() => expect(select.options.length).toBe(3));
    expect(select.value).toBe("USB DAC");
    expect(select.selectedOptions[0]?.textContent).toBe("USB DAC (not connected)");
  });

  it("stores the output device and hands it to the engine", async () => {
    await openDialog();
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
