// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { PlaylistNode, PlaylistOutcome } from "../lib/ipc";

vi.mock("../lib/ipc", () => ({
  playlistsGet: vi.fn(),
  playlistsMutate: vi.fn(),
  libraryFacets: vi.fn(),
  libraryQuery: vi.fn(),
}));

const ipc = await import("../lib/ipc");
const playlistsMutate = vi.mocked(ipc.playlistsMutate);
const { usePlaylists } = await import("../store/playlists");
const { initialTree } = await import("../store/playlistTree");
const { DuplicateDialog } = await import("./PlaylistDialogs");

const node: PlaylistNode = {
  id: 7,
  parentId: null,
  name: "Warm-up",
  isFolder: false,
  depth: 0,
  position: 0,
  entries: 3,
  smart: false,
};
const done: PlaylistOutcome = { type: "done", createdId: null, added: 1, skipped: 0 };

function duplicates(duplicateCount: number, requested: number): PlaylistOutcome {
  return { type: "duplicates", playlistId: 7, trackIds: [1], duplicateCount, requested };
}

/** "Add tracks" with ask, answered with `outcome`, then the dialog on screen. */
async function askWith(outcome: PlaylistOutcome) {
  playlistsMutate.mockResolvedValueOnce(outcome);
  render(<DuplicateDialog />);
  await usePlaylists.getState().addTracks(7, [1, 2, 3]);
  expect(playlistsMutate).toHaveBeenCalledWith(expect.objectContaining({ duplicates: "ask" }));
  playlistsMutate.mockClear();
  playlistsMutate.mockResolvedValue(done);
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(ipc.playlistsGet).mockResolvedValue({ nodes: [node] });
  vi.mocked(ipc.libraryFacets).mockResolvedValue({
    genres: [],
    keys: [],
    colours: [],
    playlists: [],
  });
  vi.mocked(ipc.libraryQuery).mockResolvedValue({
    rows: [],
    total: 0,
    dividers: [],
    dividerView: false,
  });
  usePlaylists.setState({
    nodes: [node],
    tree: initialTree,
    pendingDuplicates: null,
    confirmDelete: null,
    error: null,
  });
});
afterEach(cleanup);

describe("DuplicateDialog", () => {
  it("shows nothing without duplicates", async () => {
    playlistsMutate.mockResolvedValueOnce(done);
    render(<DuplicateDialog />);
    await usePlaylists.getState().addTracks(7, [1]);
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("shows what core returned; Add anyway resends with allow", async () => {
    await askWith(duplicates(2, 3));
    expect(await screen.findByRole("dialog")).toHaveProperty(
      "textContent",
      expect.stringContaining("2 of 3 tracks are already in Warm-up"),
    );
    fireEvent.click(screen.getByRole("button", { name: "Add anyway" }));
    await waitFor(() =>
      expect(playlistsMutate).toHaveBeenCalledWith({
        type: "addTracks",
        playlistId: 7,
        trackIds: [1, 2, 3],
        before: null,
        duplicates: "allow",
      }),
    );
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("Skip duplicates resends with skip", async () => {
    await askWith(duplicates(1, 3));
    fireEvent.click(await screen.findByRole("button", { name: "Skip duplicates" }));
    await waitFor(() =>
      expect(playlistsMutate).toHaveBeenCalledWith(
        expect.objectContaining({ type: "addTracks", duplicates: "skip" }),
      ),
    );
  });

  it("Cancel sends nothing", async () => {
    await askWith(duplicates(1, 3));
    fireEvent.click(await screen.findByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(playlistsMutate).not.toHaveBeenCalled();
  });

  it("when every track is a duplicate, only Add anyway and Cancel", async () => {
    await askWith(duplicates(3, 3));
    await screen.findByRole("dialog");
    const labels = screen.getAllByRole("button").map((b) => b.textContent);
    expect(labels).toEqual(["Add anyway", "Cancel"]);
  });
});
