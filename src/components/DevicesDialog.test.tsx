// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { Device, DeviceInfo, ProfileStatus } from "../lib/ipc";

vi.mock("../lib/ipc", () => ({
  DECKS: 4,
  devicesList: vi.fn(),
  deviceEject: vi.fn(),
  profilesList: vi.fn(),
  profileIdentify: vi.fn(),
  profileCreate: vi.fn(),
  profileLink: vi.fn(),
  profileStatus: vi.fn(),
  playlistsGet: vi.fn(),
}));

const ipc = await import("../lib/ipc");
const devicesList = vi.mocked(ipc.devicesList);
const deviceEject = vi.mocked(ipc.deviceEject);
const profilesList = vi.mocked(ipc.profilesList);
const profileIdentify = vi.mocked(ipc.profileIdentify);
const profileCreate = vi.mocked(ipc.profileCreate);
const profileLink = vi.mocked(ipc.profileLink);
const profileStatus = vi.mocked(ipc.profileStatus);
const { useDevices } = await import("../store/devices");
const { useProfiles } = await import("../store/profiles");
const { useExportWizard } = await import("../store/exportWizard");
const { useAdopt } = await import("../store/adopt");
const { DeviceList, DevicesDialog } = await import("./DevicesDialog");

const stick = (over: Partial<DeviceInfo> = {}): DeviceInfo => ({
  mountPoint: "/Volumes/REKS",
  fs: { kind: "fat32" },
  fsName: "msdos",
  removable: true,
  totalBytes: 32_000_000_000,
  freeBytes: 18_400_000_000,
  label: "REKS",
  osName: "disk4s1",
  ...over,
});

const device = (over: Partial<Device> = {}): Device => ({
  id: 1,
  volumeSerial: "4993-1CFB",
  label: "REKS",
  sizeBytes: 32_000_000_000,
  name: "Main",
  target: "Generic",
  createdAt: "2026-10-19T12:00:00.000Z",
  lastSyncedAt: "2026-10-19T14:03:07.000Z",
  ...over,
});

const status = (over: Partial<ProfileStatus> = {}): ProfileStatus => ({
  device: device(),
  source: "stick",
  sourceText: "Read from the stick",
  playlists: [
    {
      playlistId: 2,
      path: ["Set", "Peak"],
      pathText: "Set / Peak",
      tracks: 2,
      state: { state: "inSync" },
      text: "In sync",
    },
    {
      playlistId: 3,
      path: ["Set", "Chill"],
      pathText: "Set / Chill",
      tracks: 3,
      state: { state: "changed", added: 1, removed: 0, reordered: false, movedFrom: null },
      text: "1 track added",
    },
    {
      playlistId: null,
      path: ["Old"],
      pathText: "Old",
      tracks: 4,
      state: { state: "willRemove" },
      text: "Removed from the stick at the next sync",
    },
  ],
  entries: [
    {
      playlistId: 1,
      name: "Set",
      pathText: "Set",
      isFolder: true,
      playlists: 2,
      tracks: 3,
      freesBytes: 120_400_000,
      freesText: "Unticking frees 120.4 MB",
    },
  ],
  plannedBytes: 120_400_000,
  capacityBytes: 32_000_000_000,
  overCapacity: false,
  capacityText: "120.4 MB of 32.0 GB",
  summary: "1 changed, 1 to remove",
  notes: [],
  ...over,
});

beforeEach(() => {
  profilesList.mockReset().mockResolvedValue([]);
  profileIdentify.mockReset().mockResolvedValue({
    type: "new",
    volumeSerial: "AAAA-0001",
    suggestedName: "REKS",
    prompt: "This stick is new to reks.",
    note: "Name it and tick the playlists it should carry.",
  });
  profileCreate.mockReset();
  profileLink.mockReset();
  profileStatus.mockReset().mockResolvedValue(status());
  useProfiles.setState({ known: [], mounted: {}, selectedId: null, status: null, error: null });
  useExportWizard.getState().close();
  useDevices.setState({
    dialogOpen: true,
    devices: [],
    loaded: false,
    error: null,
    ejecting: null,
    notice: null,
  });
  devicesList.mockReset();
  deviceEject.mockReset();
});
afterEach(cleanup);

describe("DevicesDialog", () => {
  it("lists removable drives with file system, space and notes", async () => {
    devicesList.mockResolvedValue([
      stick(),
      stick({ mountPoint: "/Volumes/GIGS", label: "GIGS", fs: { kind: "exFat" } }),
      stick({ mountPoint: "/Volumes/Untitled", label: "", fs: { kind: "other", name: "apfs" } }),
    ]);
    render(<DevicesDialog />);
    expect(await screen.findByText("REKS")).toBeTruthy();
    expect(screen.getAllByText("FAT32 · 18 GB free of 32 GB")).toHaveLength(1);
    expect(screen.getByText(/exFAT: newer players only/)).toBeTruthy();
    // No label: named after the mount point.
    expect(screen.getByText("Untitled")).toBeTruthy();
    expect(screen.getByText(/Format it as FAT32/)).toBeTruthy();
  });

  it("says when nothing is plugged in", async () => {
    devicesList.mockResolvedValue([]);
    render(<DevicesDialog />);
    expect(await screen.findByText(/No USB drive found/)).toBeTruthy();
  });

  it("ejects a drive and refreshes the list", async () => {
    devicesList.mockResolvedValueOnce([stick()]).mockResolvedValue([]);
    deviceEject.mockResolvedValue();
    render(<DevicesDialog />);
    fireEvent.click(await screen.findByRole("button", { name: "Eject REKS" }));
    await waitFor(() => expect(deviceEject).toHaveBeenCalledWith("/Volumes/REKS"));
    expect(await screen.findByText("REKS ejected. You can remove it.")).toBeTruthy();
    expect(screen.queryByText("REKS")).toBeNull();
  });

  it("shows why an eject failed and keeps the drive", async () => {
    devicesList.mockResolvedValue([stick()]);
    deviceEject.mockRejectedValue("diskutil eject failed: busy");
    render(<DevicesDialog />);
    fireEvent.click(await screen.findByRole("button", { name: "Eject REKS" }));
    expect((await screen.findByRole("alert")).textContent).toBe(
      "Could not eject REKS: diskutil eject failed: busy",
    );
    expect(screen.getByText("REKS")).toBeTruthy();
  });

  it("refreshes while open, so a new stick shows up", async () => {
    vi.useFakeTimers();
    try {
      devicesList.mockResolvedValueOnce([]).mockResolvedValue([stick()]);
      render(<DevicesDialog />);
      await act(async () => {
        await vi.advanceTimersByTimeAsync(2000);
      });
      expect(screen.getByText("REKS")).toBeTruthy();
    } finally {
      vi.useRealTimers();
    }
  });
});

describe("DeviceList as a picker", () => {
  it("is a radio group that reports the chosen drive", async () => {
    devicesList.mockResolvedValue([stick(), stick({ mountPoint: "/Volumes/GIGS", label: "GIGS" })]);
    const onSelect = vi.fn();
    render(<DeviceList selected="/Volumes/REKS" onSelect={onSelect} />);
    const gigs = await screen.findByRole("radio", { name: "GIGS" });
    expect((screen.getByRole("radio", { name: "REKS" }) as HTMLInputElement).checked).toBe(true);
    expect((gigs as HTMLInputElement).checked).toBe(false);
    fireEvent.click(gigs);
    expect(onSelect).toHaveBeenCalledWith(expect.objectContaining({ mountPoint: "/Volumes/GIGS" }));
  });
});

describe("stick profiles", () => {
  it("shows a known stick's status, frees and capacity", async () => {
    devicesList.mockResolvedValue([stick()]);
    profilesList.mockResolvedValue([device()]);
    profileIdentify.mockResolvedValue({ type: "known", device: device() });
    render(<DevicesDialog />);
    expect(await screen.findByText("Stick: Main")).toBeTruthy();
    expect(await screen.findByText("Main: 1 changed, 1 to remove")).toBeTruthy();
    expect(profileStatus).toHaveBeenLastCalledWith(1, "/Volumes/REKS");
    expect(screen.getByText("Read from the stick")).toBeTruthy();
    expect(screen.getByText("120.4 MB of 32.0 GB")).toBeTruthy();
    expect(screen.getByText("Unticking frees 120.4 MB")).toBeTruthy();
    const rows = within(screen.getByRole("list", { name: "Playlist status" }));
    expect(rows.getByText("1 track added")).toBeTruthy();
    expect(rows.getByText("Removed from the stick at the next sync")).toBeTruthy();
    expect(screen.getByText("Plugged in")).toBeTruthy();

    // Edit and sync opens the wizard on this stick.
    fireEvent.click(screen.getByRole("button", { name: "Edit and sync…" }));
    expect(useExportWizard.getState().step).toBe("device");
    expect(useExportWizard.getState().mountPoint).toBe("/Volumes/REKS");
    expect(useDevices.getState().dialogOpen).toBe(false);
  });

  it("an unplugged stick shows the last sync and can't be synced", async () => {
    devicesList.mockResolvedValue([]);
    profilesList.mockResolvedValue([device()]);
    profileStatus.mockResolvedValue(
      status({
        source: "lastSync",
        sourceText: "As of the last sync, 2026-10-19 14:03 UTC",
        overCapacity: true,
        capacityText: "40.0 GB planned, more than the stick's 32.0 GB",
      }),
    );
    render(<DevicesDialog />);
    expect(await screen.findByText("As of the last sync, 2026-10-19 14:03 UTC")).toBeTruthy();
    expect(profileStatus).toHaveBeenLastCalledWith(1, null);
    expect(screen.getByText(/more than the stick's/).className).toContain("text-danger");
    const sync = screen.getByRole("button", { name: "Edit and sync…" }) as HTMLButtonElement;
    expect(sync.disabled).toBe(true);
  });

  it("names a new stick", async () => {
    devicesList.mockResolvedValue([stick()]);
    profileCreate.mockResolvedValue(device({ name: "Club" }));
    render(<DevicesDialog />);
    expect(await screen.findByText("This stick is new to reks.")).toBeTruthy();
    expect(screen.getByText(/No sticks saved yet/)).toBeTruthy();
    const name = screen.getByRole("textbox", { name: "Stick name" }) as HTMLInputElement;
    expect(name.value).toBe("REKS");
    fireEvent.change(name, { target: { value: "Club" } });
    profilesList.mockResolvedValue([device({ name: "Club" })]);
    fireEvent.click(screen.getByRole("button", { name: "New stick" }));
    await waitFor(() =>
      expect(profileCreate).toHaveBeenCalledWith(stick(), "AAAA-0001", "Club", "Generic"),
    );
    expect(await screen.findByText("Stick: Club")).toBeTruthy();
    expect(await screen.findByRole("radio", { name: "Club" })).toBeTruthy();
  });

  it("offers Adopt for a foreign stick: saves it, then opens the adopt dry run", async () => {
    devicesList.mockResolvedValue([stick({ label: "GIGS" })]);
    profileIdentify.mockResolvedValue({
      type: "foreign",
      volumeSerial: "AAAA-0001",
      suggestedName: "GIGS",
      prompt: "This stick is new to reks.",
      note: "It holds a rekordbox library that reks did not write.",
    });
    profileCreate.mockResolvedValue(device({ id: 4, name: "GIGS", label: "GIGS" }));
    render(<DevicesDialog />);
    fireEvent.click(await screen.findByRole("button", { name: "Adopt…" }));
    await waitFor(() => expect(useAdopt.getState().target?.deviceId).toBe(4));
    expect(profileCreate).toHaveBeenCalledWith(
      stick({ label: "GIGS" }),
      "AAAA-0001",
      "GIGS",
      "Generic",
    );
    expect(useAdopt.getState().target?.info.mountPoint).toBe("/Volumes/REKS");
    expect(useDevices.getState().dialogOpen).toBe(false);
    useAdopt.getState().close();
  });

  it("a new stick that is not foreign has no Adopt", async () => {
    devicesList.mockResolvedValue([stick()]);
    render(<DevicesDialog />);
    expect(await screen.findByRole("button", { name: "New stick" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Adopt…" })).toBeNull();
  });

  it("asks before relinking a stick with a known label and size", async () => {
    devicesList.mockResolvedValue([stick()]);
    profilesList.mockResolvedValue([device()]);
    profileIdentify.mockResolvedValue({
      type: "ambiguous",
      volumeSerial: "BBBB-0002",
      reason: "serialChanged",
      candidates: [device()],
      prompt: "Is this 'Main'?",
      note: "Its volume ID is new to reks, but its label and size match.",
    });
    profileLink.mockResolvedValue(device({ volumeSerial: "BBBB-0002" }));
    render(<DevicesDialog />);
    expect(await screen.findByText("Is this 'Main'?")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Yes" }));
    await waitFor(() => expect(profileLink).toHaveBeenCalledWith(1, stick(), "BBBB-0002"));
    expect(await screen.findByText("Stick: Main")).toBeTruthy();
  });

  it("No on the relink prompt offers a new stick", async () => {
    devicesList.mockResolvedValue([stick()]);
    profileIdentify.mockResolvedValue({
      type: "ambiguous",
      volumeSerial: null,
      reason: "noSerial",
      candidates: [device()],
      prompt: "Is this 'Main'?",
      note: "This computer does not report the stick's volume ID.",
    });
    render(<DevicesDialog />);
    fireEvent.click(await screen.findByRole("button", { name: "No, it's another stick" }));
    expect(screen.getByRole("textbox", { name: "Stick name" })).toBeTruthy();
    expect(profileLink).not.toHaveBeenCalled();
  });
});
