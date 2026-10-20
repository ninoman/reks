// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { AdoptPlaylist, AdoptScan, DeviceInfo } from "../lib/ipc";

vi.mock("../lib/ipc", () => ({
  adoptScan: vi.fn(),
  adoptApply: vi.fn(),
  playlistsGet: vi.fn(),
  libraryFacets: vi.fn(),
  libraryQuery: vi.fn(),
  historySessions: vi.fn(),
  historySession: vi.fn(),
  profileStatus: vi.fn(),
}));

const ipc = await import("../lib/ipc");
const { useAdopt } = await import("../store/adopt");
const { AdoptDialog } = await import("./AdoptDialog");

const stick: DeviceInfo = {
  mountPoint: "/Volumes/GIGS",
  fs: { kind: "fat32" },
  fsName: "msdos",
  removable: true,
  totalBytes: 1,
  freeBytes: 1,
  label: "GIGS",
  osName: "disk5s1",
};

function playlist(pdbId: number, pathText: string, link: AdoptPlaylist["link"]): AdoptPlaylist {
  return {
    pdbId,
    path: pathText.split(" / "),
    pathText,
    entries: [],
    matched: 2,
    unmatched: 0,
    dividers: 0,
    link,
    defaultChoice: { kind: "create" },
    text: "2 tracks",
  };
}

const scan: AdoptScan = {
  planId: 3,
  plan: {
    volumeSerial: "ABCD",
    writtenByReks: false,
    history: null,
    historySessions: 1,
    historyText: "1 play history session is imported first.",
    playlists: [
      playlist(12, "Sets / Warmup", [{ playlistId: 2, pathText: "Warmup" }]),
      playlist(11, "Sets / Peak", []),
      playlist(13, "Odd", []),
    ],
    unmatched: [{ stickPath: "/Contents/a/Bootleg.mp3", title: "Bootleg", artist: "X" }],
    summary: "3 playlists, 6 track entries: 5 in your library, 1 not",
    unmatchedText: "1 track is not in your library.",
    notes: [],
  },
};

beforeEach(() => {
  vi.mocked(ipc.adoptScan).mockResolvedValue(scan);
  vi.mocked(ipc.adoptApply).mockResolvedValue({
    history: null,
    folderId: 9,
    created: [10],
    linked: [2],
    skipped: 1,
    unmatched: [],
    summary: "1 playlist created, 1 linked, 1 skipped.",
  });
  vi.mocked(ipc.playlistsGet).mockResolvedValue({ nodes: [] });
  vi.mocked(ipc.historySessions).mockResolvedValue([]);
  useAdopt.getState().open({ deviceId: 5, deviceName: "Gigs", info: stick });
});

afterEach(() => {
  cleanup();
  useAdopt.getState().close();
  vi.clearAllMocks();
});

describe("AdoptDialog", () => {
  it("shows the dry run with Link only where the plan offers it", async () => {
    render(<AdoptDialog />);
    await screen.findByText(scan.plan.summary);
    expect(ipc.adoptScan).toHaveBeenCalledWith(stick);
    const warmup = screen.getByLabelText("What to do with Sets / Warmup") as HTMLSelectElement;
    const peak = screen.getByLabelText("What to do with Sets / Peak") as HTMLSelectElement;
    expect(warmup.value).toBe("create");
    expect([...warmup.options].map((o) => o.value)).toEqual(["create", "link:2", "skip"]);
    expect([...peak.options].map((o) => o.value)).toEqual(["create", "skip"]);
    expect(screen.getByText(scan.plan.unmatchedText)).toBeTruthy();
  });

  it("applies the choices and reports", async () => {
    render(<AdoptDialog />);
    const warmup = await screen.findByLabelText("What to do with Sets / Warmup");
    fireEvent.change(warmup, { target: { value: "link:2" } });
    fireEvent.change(screen.getByLabelText("What to do with Odd"), {
      target: { value: "skip" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Adopt" }));
    await screen.findByText("1 playlist created, 1 linked, 1 skipped.");
    expect(ipc.adoptApply).toHaveBeenCalledWith(3, {
      deviceId: 5,
      playlists: { 12: { kind: "link", playlistId: 2 }, 13: { kind: "skip" } },
      history: { excluded: {} },
    });
    await waitFor(() => expect(ipc.playlistsGet).toHaveBeenCalled());
  });

  it("shows a scan error and only offers Close", async () => {
    vi.mocked(ipc.adoptScan).mockRejectedValue("no rekordbox library");
    render(<AdoptDialog />);
    await screen.findByText("no rekordbox library");
    expect(screen.queryByRole("button", { name: "Adopt" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(useAdopt.getState().target).toBeNull();
  });
});
