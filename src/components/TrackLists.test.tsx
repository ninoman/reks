// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { PlaylistNode } from "../lib/ipc";

vi.mock(import("../lib/ipc"), async (importOriginal) => ({
  ...(await importOriginal()),
  libraryQuery: vi.fn(),
}));

const ipc = await import("../lib/ipc");
const { useLibrary } = await import("../store/library");
const { usePanes } = await import("../store/panes");
const { usePlaylists } = await import("../store/playlists");
const { initialTree } = await import("../store/playlistTree");
const { PlaylistTree } = await import("./PlaylistTree");
const { TrackLists } = await import("./TrackLists");
const { COLUMNS } = await import("./columns");

const node = (id: number, name: string, position: number): PlaylistNode => ({
  id,
  parentId: null,
  name,
  isFolder: false,
  depth: 0,
  position,
  entries: 0,
  smart: false,
});
const warmUp = node(2, "Warm-up", 0);
const peak = node(3, "Peak", 1);

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(ipc.libraryQuery).mockResolvedValue({
    rows: [],
    total: 0,
    dividers: [],
    dividerView: false,
  });
  usePanes.setState({ panes: [] });
  useLibrary.getState().showPlaylist(null, true);
  usePlaylists.setState({ nodes: [warmUp, peak], tree: initialTree, error: null });
});

afterEach(cleanup);

function renderApp() {
  render(
    <>
      <PlaylistTree />
      <TrackLists columns={COLUMNS.slice(0, 2)} />
    </>,
  );
}

const lists = () => screen.getAllByRole("grid").length;

describe("TrackLists", () => {
  it("one list has no header or close button", () => {
    renderApp();
    expect(lists()).toBe(1);
    expect(screen.queryByRole("button", { name: /^Close/ })).toBeNull();
  });

  it("Shift+click splits the view; each list closes with its x", () => {
    renderApp();
    fireEvent.click(screen.getByRole("treeitem", { name: /Warm-up/ }), { shiftKey: true });
    fireEvent.click(screen.getByRole("treeitem", { name: /Peak/ }), { shiftKey: true });
    expect(lists()).toBe(3);
    const pane = screen.getByRole("region", { name: "Peak" });
    expect(within(pane).getByRole("grid")).toBeTruthy();
    expect(screen.getByRole("treeitem", { name: /Peak/ }).getAttribute("aria-selected")).toBe(
      "false",
    );

    fireEvent.click(screen.getByRole("button", { name: "Close Peak" }));
    expect(lists()).toBe(2);

    // The main list's x: Warm-up moves up and the view is single again.
    fireEvent.click(screen.getByRole("button", { name: "Close Collection" }));
    expect(lists()).toBe(1);
    expect(useLibrary.getState().filters.playlistId).toBe(2);
    expect(screen.queryByRole("button", { name: /^Close/ })).toBeNull();
  });

  it("the Playlists (i) explains Shift+click on focus", async () => {
    renderApp();
    fireEvent.focus(screen.getByRole("button", { name: "About opening several playlists" }));
    expect((await screen.findByRole("tooltip")).textContent).toMatch(/Shift\+click a playlist/);
  });
});
