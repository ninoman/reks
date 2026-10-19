// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { DeviceInfo } from "../lib/ipc";

vi.mock("../lib/ipc", () => ({
  devicesList: vi.fn(),
  deviceEject: vi.fn(),
}));

const ipc = await import("../lib/ipc");
const devicesList = vi.mocked(ipc.devicesList);
const deviceEject = vi.mocked(ipc.deviceEject);
const { useDevices } = await import("../store/devices");
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

beforeEach(() => {
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
