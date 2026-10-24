// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { RemapPlan } from "../lib/ipc";

vi.mock("../lib/ipc", () => ({
  DECKS: 4,
  remapPlan: vi.fn(),
  remapApply: vi.fn(),
  libraryFacets: vi.fn(),
  libraryQuery: vi.fn(),
  playlistsGet: vi.fn(),
  libraryRoots: vi.fn(),
  scanStart: vi.fn(),
}));

const ipc = await import("../lib/ipc");
const { useRemap, remapBlocker } = await import("../store/remap");
const { RemapDialog } = await import("./RemapDialog");

const root = { id: 3, path: "/Users/me/Music", lastScan: null };

const plan: RemapPlan = {
  rootId: 3,
  oldPath: "/Users/me/Music",
  newPath: "/Volumes/Big/Music",
  tracks: 2800,
  nestedRoots: 0,
  examples: [{ from: "/Users/me/Music/a.mp3", to: "/Volumes/Big/Music/a.mp3" }],
  sampled: 24,
  sampleFound: 24,
  collisions: 0,
};

beforeEach(() => {
  vi.mocked(ipc.remapPlan).mockResolvedValue(plan);
  vi.mocked(ipc.remapApply).mockResolvedValue({ tracks: 2800, roots: 1 });
  vi.mocked(ipc.libraryRoots).mockResolvedValue([]);
  vi.mocked(ipc.playlistsGet).mockResolvedValue({ nodes: [] });
  vi.mocked(ipc.scanStart).mockResolvedValue({
    found: 0,
    added: 0,
    updated: 0,
    unchanged: 0,
    moved: 0,
    missing: 0,
    errors: [],
    cancelled: false,
  });
});

afterEach(() => {
  cleanup();
  useRemap.getState().close();
  vi.clearAllMocks();
});

describe("remapBlocker", () => {
  it("blocks on collisions and on an empty sample, not on a partial one", () => {
    expect(remapBlocker({ ...plan, collisions: 2 })).toMatch(/2 tracks would get a path/);
    expect(remapBlocker({ ...plan, sampleFound: 0 })).toMatch(/None of the 24 files/);
    expect(remapBlocker({ ...plan, sampleFound: 3 })).toBeNull();
    expect(remapBlocker({ ...plan, sampled: 0, sampleFound: 0 })).toBeNull();
  });
});

describe("RemapDialog", () => {
  it("shows counts and examples, then changes the folder and rescans it", async () => {
    void useRemap.getState().start(root, "/Volumes/Big/Music");
    render(<RemapDialog />);
    await screen.findByText(/2800 tracks will change/);
    expect(screen.getByText(/24 of 24 files checked exist/)).toBeTruthy();
    expect(screen.getByText("→ /Volumes/Big/Music/a.mp3")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Change folder" }));
    await screen.findByText(/2800 tracks now point to the new folder/);
    expect(ipc.remapApply).toHaveBeenCalledWith(3, "/Volumes/Big/Music");
    await waitFor(() =>
      expect(ipc.scanStart).toHaveBeenCalledWith(["/Volumes/Big/Music"], expect.any(Function)),
    );
  });

  it("disables the change when no sampled file is at the new place", async () => {
    vi.mocked(ipc.remapPlan).mockResolvedValue({ ...plan, sampleFound: 0 });
    void useRemap.getState().start(root, "/Volumes/Empty");
    render(<RemapDialog />);
    await screen.findByText(/None of the 24 files checked exist/);
    expect(
      (screen.getByRole("button", { name: "Change folder" }) as HTMLButtonElement).disabled,
    ).toBe(true);
  });

  it("says when no track is under the folder", async () => {
    vi.mocked(ipc.remapPlan).mockResolvedValue({
      ...plan,
      tracks: 0,
      examples: [],
      sampled: 0,
      sampleFound: 0,
    });
    void useRemap.getState().start(root, "/Volumes/Big/Music");
    render(<RemapDialog />);
    await screen.findByText("No track in the library is under this folder.");
    expect(
      (screen.getByRole("button", { name: "Change folder" }) as HTMLButtonElement).disabled,
    ).toBe(true);
  });

  it("shows a refusal from the dry run and only offers Close", async () => {
    vi.mocked(ipc.remapPlan).mockRejectedValue("that folder is already a library folder");
    void useRemap.getState().start(root, "/Users/me/Other");
    render(<RemapDialog />);
    await screen.findByText("that folder is already a library folder");
    expect(screen.queryByRole("button", { name: "Change folder" })).toBeNull();
  });
});
