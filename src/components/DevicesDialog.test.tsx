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
  profileSelection: vi.fn(),
  profileSetSelection: vi.fn(),
  profileSetTarget: vi.fn(),
  exportDefaultOptions: vi.fn(),
  exportPlan: vi.fn(),
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
const { usePlaylists } = await import("../store/playlists");
const { SticksDialog } = await import("./SticksDialog");

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
  vi.mocked(ipc.profileSelection)
    .mockReset()
    .mockResolvedValue({ playlists: [1] });
  vi.mocked(ipc.profileSetSelection).mockReset().mockResolvedValue();
  vi.mocked(ipc.profileSetTarget)
    .mockReset()
    .mockImplementation(async (_id, target) => device({ target }));
  vi.mocked(ipc.exportDefaultOptions)
    .mockReset()
    .mockReturnValue(new Promise(() => {}));
  usePlaylists.setState({
    nodes: [
      {
        id: 1,
        parentId: null,
        name: "Set",
        isFolder: true,
        depth: 0,
        position: 0,
        entries: 0,
        smart: false,
      },
      {
        id: 2,
        parentId: 1,
        name: "Peak",
        isFolder: false,
        depth: 1,
        position: 0,
        entries: 2,
        smart: false,
      },
      {
        id: 5,
        parentId: null,
        name: "Other",
        isFolder: false,
        depth: 0,
        position: 1,
        entries: 4,
        smart: false,
      },
    ],
  });
  useProfiles.setState({
    known: [],
    mounted: {},
    selectedId: null,
    status: null,
    ticks: null,
    error: null,
  });
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
    render(<SticksDialog />);
    expect(await screen.findByText("REKS")).toBeTruthy();
    expect(screen.getAllByText("FAT32 · 18 GB free of 32 GB")).toHaveLength(1);
    expect(screen.getByText(/exFAT: newer players only/)).toBeTruthy();
    // No label: named after the mount point.
    expect(screen.getByText("Untitled")).toBeTruthy();
    expect(screen.getByText(/Format it as FAT32/)).toBeTruthy();
  });

  it("says when nothing is plugged in", async () => {
    devicesList.mockResolvedValue([]);
    render(<SticksDialog />);
    expect(await screen.findByText(/No sticks yet/)).toBeTruthy();
  });

  it("ejects a drive and refreshes the list", async () => {
    devicesList.mockResolvedValueOnce([stick()]).mockResolvedValue([]);
    deviceEject.mockResolvedValue();
    render(<SticksDialog />);
    fireEvent.click(await screen.findByRole("button", { name: "Eject REKS" }));
    await waitFor(() => expect(deviceEject).toHaveBeenCalledWith("/Volumes/REKS"));
    expect(await screen.findByText("REKS ejected. You can remove it.")).toBeTruthy();
    expect(screen.queryByText("REKS")).toBeNull();
  });

  it("shows why an eject failed and keeps the drive", async () => {
    devicesList.mockResolvedValue([stick()]);
    deviceEject.mockRejectedValue("diskutil eject failed: busy");
    render(<SticksDialog />);
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
      render(<SticksDialog />);
      await act(async () => {
        await vi.advanceTimersByTimeAsync(2000);
      });
      expect(screen.getByText("REKS")).toBeTruthy();
    } finally {
      vi.useRealTimers();
    }
  });
});

describe("stick profiles", () => {
  it("shows a known stick's status, frees and capacity", async () => {
    devicesList.mockResolvedValue([stick()]);
    profilesList.mockResolvedValue([device()]);
    profileIdentify.mockResolvedValue({ type: "known", device: device() });
    render(<SticksDialog />);
    // One list: the plugged-in stick is picked and expanded.
    const main = (await screen.findByRole("radio", { name: "Main" })) as HTMLInputElement;
    await waitFor(() => expect(main.checked).toBe(true));
    expect(screen.getByText(/^FAT32 · .+ free of .+$/)).toBeTruthy();
    expect(await screen.findByText("1 changed, 1 to remove")).toBeTruthy();
    expect(profileStatus).toHaveBeenLastCalledWith(1, "/Volumes/REKS");
    expect(screen.getByText("Read from the stick")).toBeTruthy();
    expect(screen.getByText("120.4 MB of 32.0 GB")).toBeTruthy();
    expect(screen.getByText("Unticking frees 120.4 MB")).toBeTruthy();
    const rows = within(screen.getByRole("list", { name: "Playlist status" }));
    expect(rows.getByText("1 track added")).toBeTruthy();
    expect(rows.getByText("Removed from the stick at the next sync")).toBeTruthy();

    // Sync, in the footer, opens the review on this stick in the same dialog,
    // so it resizes instead of closing and opening.
    const dialog = screen.getByRole("dialog");
    fireEvent.click(screen.getByRole("button", { name: "Sync Main" }));
    expect(useExportWizard.getState().step).toBe("review");
    expect(screen.getByRole("dialog")).toBe(dialog);
    expect(screen.getByText("Sync Main")).toBeTruthy();
    expect(useExportWizard.getState().mountPoint).toBe("/Volumes/REKS");
    expect(useDevices.getState().dialogOpen).toBe(false);
  });

  it("ticks playlists and sets Plays on, saving each change", async () => {
    devicesList.mockResolvedValue([stick()]);
    profilesList.mockResolvedValue([device()]);
    profileIdentify.mockResolvedValue({ type: "known", device: device() });
    render(<SticksDialog />);
    const ticks = within(await screen.findByRole("group", { name: "Playlists on this stick" }));
    // The ticked folder covers (and locks) what's under it, and shows what unticking frees.
    const peak = ticks.getByRole("checkbox", { name: /Peak/ }) as HTMLInputElement;
    expect(peak.checked && peak.disabled).toBe(true);
    expect(ticks.getByText("Unticking frees 120.4 MB")).toBeTruthy();
    fireEvent.click(ticks.getByRole("checkbox", { name: /Other/ }));
    await waitFor(() => expect(vi.mocked(ipc.profileSetSelection)).toHaveBeenCalledWith(1, [1, 5]));

    fireEvent.change(screen.getByRole("combobox", { name: "Plays on" }), {
      target: { value: "Cdj3000" },
    });
    await waitFor(() => expect(vi.mocked(ipc.profileSetTarget)).toHaveBeenCalledWith(1, "Cdj3000"));
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
    render(<SticksDialog />);
    expect(await screen.findByText("As of the last sync, 2026-10-19 14:03 UTC")).toBeTruthy();
    expect(profileStatus).toHaveBeenLastCalledWith(1, null);
    expect(screen.getByText(/more than the stick's/).className).toContain("text-danger");
    const sync = screen.getByRole("button", { name: "Sync Main" }) as HTMLButtonElement;
    expect(sync.disabled).toBe(true);
  });

  it("names a new stick", async () => {
    devicesList.mockResolvedValue([stick()]);
    profileCreate.mockResolvedValue(device({ name: "Club" }));
    render(<SticksDialog />);
    expect(await screen.findByText("This stick is new to reks.")).toBeTruthy();
    const name = screen.getByRole("textbox", { name: "Stick name" }) as HTMLInputElement;
    expect(name.value).toBe("REKS");
    fireEvent.change(name, { target: { value: "Club" } });
    profilesList.mockResolvedValue([device({ name: "Club" })]);
    fireEvent.click(screen.getByRole("button", { name: "New stick" }));
    await waitFor(() =>
      expect(profileCreate).toHaveBeenCalledWith(stick(), "AAAA-0001", "Club", "Generic"),
    );
    // The new stick joins the list, picked, and the prompt goes.
    const club = (await screen.findByRole("radio", { name: "Club" })) as HTMLInputElement;
    await waitFor(() => expect(club.checked).toBe(true));
    expect(screen.queryByText("This stick is new to reks.")).toBeNull();
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
    render(<SticksDialog />);
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
    render(<SticksDialog />);
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
    render(<SticksDialog />);
    expect(await screen.findByText("Is this 'Main'?")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Yes" }));
    await waitFor(() => expect(profileLink).toHaveBeenCalledWith(1, stick(), "BBBB-0002"));
    expect(await screen.findByRole("radio", { name: "Main" })).toBeTruthy();
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
    render(<SticksDialog />);
    fireEvent.click(await screen.findByRole("button", { name: "No, it's another stick" }));
    expect(screen.getByRole("textbox", { name: "Stick name" })).toBeTruthy();
    expect(profileLink).not.toHaveBeenCalled();
  });
});
