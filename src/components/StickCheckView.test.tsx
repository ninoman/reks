// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { DeviceInfo, StickReport } from "../lib/ipc";

vi.mock("../lib/ipc", () => ({
  deviceCheck: vi.fn(),
  deviceLibrary: vi.fn(),
  historySession: vi.fn(),
}));

const ipc = await import("../lib/ipc");
const { useDeviceBrowser } = await import("../store/deviceBrowser");
const { useDevices } = await import("../store/devices");
const { useStickCheck } = await import("../store/stickCheck");
const { StickCheckView } = await import("./StickCheckView");

const stick: DeviceInfo = {
  mountPoint: "/Volumes/GIG",
  fs: { kind: "fat32" },
  fsName: "msdos",
  removable: true,
  totalBytes: 1,
  freeBytes: 1,
  label: "GIG",
  osName: "disk5s1",
};

const report: StickReport = {
  mountPoint: stick.mountPoint,
  label: "GIG",
  target: "Cdj2000Nxs",
  targetText: "Audio checked against CDJ-2000NXS, the strictest players.",
  summary: {
    status: "problems",
    problems: 1,
    warnings: 1,
    notes: 0,
    headline: "1 problem, 1 warning",
  },
  formats: [
    {
      format: "deviceLibrary",
      name: "Device Library",
      file: "/PIONEER/rekordbox/export.pdb",
      present: true,
      status: "2 tracks in 1 playlist",
      players: ["CDJ-3000", "XDJ-XZ"],
      playersVerified: false,
      note: "The classic stick library.",
    },
  ],
  playersSource: "AlphaTheta notice. UNVERIFIED.",
  filesystem: {
    fs: { kind: "fat32" },
    fsName: "msdos",
    totalBytes: 1,
    freeBytes: 1,
    text: "FAT32 · 1 B free of 1 B",
  },
  tracksChecked: 2,
  sections: [
    {
      section: "files",
      title: "Track files",
      groups: [
        {
          kind: "audioMissing",
          severity: "problem",
          title: "1 track: audio file missing",
          detail: "Players list these tracks but can't load them.",
          items: [{ subject: "A – Gone", detail: null, path: "/Contents/A/gone.wav" }],
        },
        {
          kind: "noOneLibrary",
          severity: "warning",
          title: "No OneLibrary (exportLibrary.db)",
          detail: "Some players will show no playlists.",
          items: [],
        },
      ],
    },
  ],
};

beforeEach(() => {
  useDevices.setState({ devices: [stick], loaded: true });
});

afterEach(() => {
  cleanup();
  useDeviceBrowser.getState().close();
  useStickCheck.setState({ loads: {} });
  vi.clearAllMocks();
});

describe("StickCheckView", () => {
  it("shows checking, then the summary, library files and grouped findings", async () => {
    let resolve: (r: StickReport) => void = () => {};
    vi.mocked(ipc.deviceCheck).mockReturnValue(new Promise((r) => (resolve = r)));
    render(<StickCheckView />);
    useDeviceBrowser.getState().openCheck(stick);
    expect((await screen.findByRole("status")).textContent).toContain("Checking every track");
    expect(ipc.deviceCheck).toHaveBeenCalledWith(stick);
    expect(screen.getByRole<HTMLButtonElement>("button", { name: "Check again" }).disabled).toBe(
      true,
    );

    resolve(report);
    expect(await screen.findByText("1 problem, 1 warning")).toBeTruthy();
    expect(screen.getByText("Problems")).toBeTruthy();
    expect(screen.getByText("2 tracks checked · FAT32 · 1 B free of 1 B")).toBeTruthy();
    expect(screen.getByText("Read by CDJ-3000, XDJ-XZ")).toBeTruthy();
    expect(screen.getByText("UNVERIFIED")).toBeTruthy();
    const files = screen.getByRole("region", { name: "Track files" });
    expect(files.textContent).toContain("1 track: audio file missing");
    expect(files.textContent).toContain("No OneLibrary (exportLibrary.db)");
    expect(screen.getByText("Show the list (1)")).toBeTruthy();
    expect(screen.getByText("/Contents/A/gone.wav")).toBeTruthy();
  });

  it("checks again, shows errors with Try again, and closes", async () => {
    vi.mocked(ipc.deviceCheck).mockRejectedValueOnce("cannot list /Volumes/GIG");
    render(<StickCheckView />);
    useDeviceBrowser.getState().openCheck(stick);
    expect((await screen.findByRole("alert")).textContent).toContain("cannot list /Volumes/GIG");

    vi.mocked(ipc.deviceCheck).mockResolvedValueOnce({
      ...report,
      summary: {
        status: "ok",
        problems: 0,
        warnings: 0,
        notes: 0,
        headline: "Nothing wrong found",
      },
      sections: [],
    });
    fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByText("Nothing wrong found")).toBeTruthy();
    expect(screen.getByText("OK")).toBeTruthy();
    expect(screen.getByText("Nothing to report: every check passed.")).toBeTruthy();
    expect(ipc.deviceCheck).toHaveBeenCalledTimes(2);

    fireEvent.click(screen.getByRole("button", { name: "Close the check" }));
    await waitFor(() => expect(useDeviceBrowser.getState().checking).toBeNull());
  });

  it("a stick list replaces the report, and an unplugged drive closes it", () => {
    vi.mocked(ipc.deviceCheck).mockResolvedValue(report);
    const b = useDeviceBrowser.getState();
    b.openCheck(stick);
    expect(useDeviceBrowser.getState().checking).toBe(stick.mountPoint);
    b.openList({ mountPoint: stick.mountPoint, kind: "playlist", id: 1 });
    expect(useDeviceBrowser.getState().checking).toBeNull();

    b.openCheck(stick);
    useDeviceBrowser.setState({ libraries: { [stick.mountPoint]: { state: "loading" } } });
    b.syncDevices([]);
    expect(useDeviceBrowser.getState().checking).toBeNull();
    expect(useStickCheck.getState().loads[stick.mountPoint]).toBeUndefined();
  });
});
