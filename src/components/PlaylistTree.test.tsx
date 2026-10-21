// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { NODE_MIME, TRACKS_MIME } from "../lib/dnd";
import type { PlaylistNode, PlaylistOutcome } from "../lib/ipc";

vi.mock("../lib/ipc", () => ({
  playlistsGet: vi.fn(),
  playlistsMutate: vi.fn(),
  libraryFacets: vi.fn(),
  libraryQuery: vi.fn(),
}));

const ipc = await import("../lib/ipc");
const playlistsMutate = vi.mocked(ipc.playlistsMutate);
const { useLibrary } = await import("../store/library");
const { usePlaylists } = await import("../store/playlists");
const { initialTree } = await import("../store/playlistTree");
const { PlaylistTree } = await import("./PlaylistTree");
const { DeleteDialog } = await import("./PlaylistDialogs");

const folder: PlaylistNode = {
  id: 1,
  parentId: null,
  name: "Gigs",
  isFolder: true,
  depth: 0,
  position: 0,
  entries: 0,
  smart: false,
};
const set: PlaylistNode = {
  id: 2,
  parentId: 1,
  name: "Warm-up",
  isFolder: false,
  depth: 1,
  position: 0,
  entries: 2,
  smart: false,
};
const done: PlaylistOutcome = { type: "done", createdId: null, added: 0, skipped: 0 };

function dataTransfer(data: Record<string, string>) {
  return {
    types: Object.keys(data),
    getData: (t: string) => data[t] ?? "",
    setData: vi.fn(),
    dropEffect: "none",
    effectAllowed: "all",
  };
}

const item = (name: string) => screen.getByRole("treeitem", { name: new RegExp(name) });

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(ipc.playlistsGet).mockResolvedValue({ nodes: [folder, set] });
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
  playlistsMutate.mockResolvedValue(done);
  usePlaylists.setState({
    nodes: [folder, set],
    tree: { ...initialTree, expanded: new Set([1]) },
    pendingDuplicates: null,
    confirmDelete: null,
    error: null,
  });
});
afterEach(cleanup);

describe("PlaylistTree", () => {
  it("selecting a playlist sorts by position; a folder or the collection drops that sort", () => {
    render(<PlaylistTree />);
    fireEvent.click(item("Warm-up"));
    expect(useLibrary.getState().filters.playlistId).toBe(2);
    expect(useLibrary.getState().sort).toEqual({ column: "playlistPosition", descending: false });
    fireEvent.click(item("Gigs"));
    expect(useLibrary.getState().filters.playlistId).toBe(1);
    expect(useLibrary.getState().sort.column).toBe("title");
    fireEvent.click(item("Collection"));
    expect(useLibrary.getState().filters.playlistId).toBeNull();
  });

  it("the Playlists root row opens and closes the whole tree", () => {
    render(<PlaylistTree />);
    fireEvent.click(item("Playlists"));
    expect(screen.queryByRole("treeitem", { name: /Gigs/ })).toBeNull();
    fireEvent.click(item("Playlists"));
    expect(item("Gigs")).toBeTruthy();
  });

  it("collapsing a folder hides its playlists", () => {
    render(<PlaylistTree />);
    fireEvent.click(item("Gigs"));
    fireEvent.keyDown(screen.getByRole("tree"), { key: "ArrowLeft" });
    expect(screen.queryByRole("treeitem", { name: /Warm-up/ })).toBeNull();
  });

  it("F2 renames in place; Enter commits", async () => {
    render(<PlaylistTree />);
    fireEvent.click(item("Warm-up"));
    fireEvent.keyDown(screen.getByRole("tree"), { key: "F2" });
    const input = screen.getByRole("textbox", { name: "Name" });
    fireEvent.change(input, { target: { value: "Opening" } });
    fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() =>
      expect(playlistsMutate).toHaveBeenCalledWith({ type: "rename", id: 2, name: "Opening" }),
    );
  });

  it("Escape cancels a rename", () => {
    render(<PlaylistTree />);
    fireEvent.click(item("Warm-up"));
    fireEvent.keyDown(screen.getByRole("tree"), { key: "Enter" });
    fireEvent.keyDown(screen.getByRole("textbox"), { key: "Escape" });
    expect(screen.queryByRole("textbox")).toBeNull();
    expect(playlistsMutate).not.toHaveBeenCalled();
  });

  it("right-click on a row selects it and offers rename for it", async () => {
    render(<PlaylistTree />);
    fireEvent.contextMenu(item("Warm-up"));
    expect(usePlaylists.getState().tree.selectedId).toBe(2);
    fireEvent.click(await screen.findByRole("menuitem", { name: "Rename" }));
    const input = await screen.findByRole("textbox", { name: "Name" });
    expect((input as HTMLInputElement).value).toBe("Warm-up");
  });

  it("right-click on empty tree space offers only new items", async () => {
    render(<PlaylistTree />);
    fireEvent.contextMenu(screen.getByRole("tree"));
    await screen.findByRole("menuitem", { name: "New playlist" });
    expect(screen.queryByRole("menuitem", { name: "Rename" })).toBeNull();
  });

  it("Delete asks first, then deletes", async () => {
    render(
      <>
        <PlaylistTree />
        <DeleteDialog />
      </>,
    );
    fireEvent.click(item("Gigs"));
    fireEvent.keyDown(screen.getByRole("tree"), { key: "Delete" });
    expect(playlistsMutate).not.toHaveBeenCalled();
    expect(screen.getByRole("dialog").textContent).toContain("Gigs");
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(playlistsMutate).toHaveBeenCalledWith({ type: "delete", id: 1 }));
  });

  it("dropping tracks on a playlist adds them, asking about duplicates", async () => {
    render(<PlaylistTree />);
    const dt = dataTransfer({
      [TRACKS_MIME]: JSON.stringify({ trackIds: [5, 6], playlistId: null, entryPositions: [] }),
    });
    fireEvent.dragOver(item("Warm-up"), { dataTransfer: dt });
    fireEvent.drop(item("Warm-up"), { dataTransfer: dt });
    await waitFor(() =>
      expect(playlistsMutate).toHaveBeenCalledWith({
        type: "addTracks",
        playlistId: 2,
        trackIds: [5, 6],
        before: null,
        duplicates: "ask",
      }),
    );
  });

  it("tracks dropped on a folder are ignored", () => {
    render(<PlaylistTree />);
    const dt = dataTransfer({
      [TRACKS_MIME]: JSON.stringify({ trackIds: [5], playlistId: null, entryPositions: [] }),
    });
    fireEvent.drop(item("Gigs"), { dataTransfer: dt });
    expect(playlistsMutate).not.toHaveBeenCalled();
  });

  it("dragging a node onto another moves it", async () => {
    render(<PlaylistTree />);
    // jsdom rows have no height, so the pointer is in the "before" zone.
    const dt = dataTransfer({ [NODE_MIME]: "2" });
    fireEvent.dragOver(item("Gigs"), { dataTransfer: dt });
    fireEvent.drop(item("Gigs"), { dataTransfer: dt });
    await waitFor(() =>
      expect(playlistsMutate).toHaveBeenCalledWith({
        type: "move",
        id: 2,
        parentId: null,
        position: 0,
      }),
    );
  });
});
