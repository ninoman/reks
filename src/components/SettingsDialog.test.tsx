// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { LibraryRoot, Settings } from "../lib/ipc";

vi.mock("../lib/ipc", () => ({
  settingsGet: vi.fn(),
  settingsSet: vi.fn(),
  libraryRoots: vi.fn(),
  libraryRootAdd: vi.fn(),
  libraryRootRemove: vi.fn(),
  cacheDirMove: vi.fn(),
  playerDevices: vi.fn(),
  playerSetDevice: vi.fn(),
  libraryQuery: vi.fn(),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

const ipc = await import("../lib/ipc");
const dialog = await import("@tauri-apps/plugin-dialog");
const { useSettings } = await import("../store/settings");
const { SettingsDialog } = await import("./SettingsDialog");

const settings: Settings = {
  audioDevice: "USB DAC",
  keyNotation: "classical",
  deckWaveform: "threeBand",
  cacheDir: "/Users/dj/Library/Caches/reks",
  anlzPreference: "rekordboxFirst",
  onelibraryKeySet: false,
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
  vi.mocked(ipc.playerDevices).mockResolvedValue(["Built-in Output"]);
  vi.mocked(ipc.libraryRootRemove).mockResolvedValue();
  vi.mocked(ipc.settingsSet).mockImplementation(async (p) => ({ ...settings, ...p }) as Settings);
  vi.mocked(ipc.cacheDirMove).mockResolvedValue({ ...settings, cacheDir: "/Volumes/SSD/cache" });
});
afterEach(() => {
  cleanup();
  useSettings.setState({ dialogOpen: false });
});

async function openDialog() {
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
    expect(groups).toEqual(["Folders", "Playback", "Display", "Decks", "Experimental"]);
    const folders = screen.getByRole("region", { name: "Folders" });
    expect(folders.textContent).toContain("/Music/House");
    expect(folders.textContent).toContain("/Users/dj/Library/Caches/reks");
    expect(screen.queryByRole("tab")).toBeNull();
  });

  it("stores the OneLibrary key without showing it, and removes it", async () => {
    await openDialog();
    const group = screen.getByRole("region", { name: "Experimental" });
    expect(group.textContent).toContain("experimental");
    expect(group.textContent).toContain("AlphaTheta does not support it");
    expect(group.textContent).toContain("reks does not include it");
    expect(group.textContent).toContain("REKS_ONELIBRARY_KEY");
    expect(screen.getByText("No key stored.")).toBeTruthy();

    const input = screen.getByLabelText("OneLibrary key") as HTMLInputElement;
    expect(input.type).toBe("password");
    const save = screen.getByRole("button", { name: "Save" }) as HTMLButtonElement;
    expect(save.disabled).toBe(true);
    vi.mocked(ipc.settingsSet).mockResolvedValueOnce({ ...settings, onelibraryKeySet: true });
    fireEvent.change(input, { target: { value: "  s3cret  " } });
    fireEvent.click(save);
    await screen.findByText("A key is stored.");
    expect(ipc.settingsSet).toHaveBeenCalledWith({ onelibraryKey: "s3cret" });
    expect(input.value).toBe("");

    vi.mocked(ipc.settingsSet).mockResolvedValueOnce({ ...settings, onelibraryKeySet: false });
    fireEvent.click(screen.getByRole("button", { name: "Remove" }));
    await screen.findByText("No key stored.");
    expect(ipc.settingsSet).toHaveBeenLastCalledWith({ onelibraryKey: null });
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
    expect(decks.querySelector('[aria-checked="true"]')?.textContent).toContain("3-band");
    fireEvent.click(screen.getByRole("radio", { name: /RGB/ }));
    await waitFor(() => expect(ipc.settingsSet).toHaveBeenCalledWith({ deckWaveform: "rgb" }));
    expect(useSettings.getState().settings?.deckWaveform).toBe("rgb");
  });

  it("keeps a chosen device that is not connected in the list", async () => {
    await openDialog();
    const select = screen.getByRole("combobox", { name: "Output device" }) as HTMLSelectElement;
    await waitFor(() => expect(select.options.length).toBe(3));
    expect(select.value).toBe("USB DAC");
    expect(select.selectedOptions[0]?.textContent).toBe("USB DAC (not connected)");
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
