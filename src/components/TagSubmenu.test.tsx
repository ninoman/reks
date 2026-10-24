// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { TrackRow } from "../lib/ipc";

vi.mock("../lib/ipc", () => ({
  tagsGet: vi.fn(),
  tagsMutate: vi.fn(),
  libraryFacets: vi.fn(),
  libraryQuery: vi.fn(),
}));

const ipc = await import("../lib/ipc");
const { useLibrary } = await import("../store/library");
const { useTags } = await import("../store/tags");
const { ContextMenu, ContextMenuContent, ContextMenuTrigger } = await import("./ui/ContextMenu");
const { TagSubmenu } = await import("./TagSubmenu");

const tree = {
  categories: [
    {
      id: 1,
      name: "Genre",
      tags: [
        { id: 10, name: "House", trackCount: 1 },
        { id: 11, name: "Techno", trackCount: 2 },
      ],
    },
  ],
};

function openMenu(trackIds: number[]) {
  render(
    <ContextMenu>
      <ContextMenuTrigger>
        <div>area</div>
      </ContextMenuTrigger>
      <ContextMenuContent>
        <TagSubmenu trackIds={trackIds} />
      </ContextMenuContent>
    </ContextMenu>,
  );
  fireEvent.contextMenu(screen.getByText("area"));
  fireEvent.keyDown(screen.getByRole("menuitem", { name: "My Tags" }), { key: "ArrowRight" });
}

beforeEach(() => {
  vi.mocked(ipc.tagsMutate).mockResolvedValue({ createdId: null, changed: 1, commentsUpdated: 0 });
  vi.mocked(ipc.tagsGet).mockResolvedValue(tree);
  useTags.setState({ tree, status: "ready", error: null, managerOpen: false });
  useLibrary.setState({
    pages: new Map([
      [
        0,
        [
          { id: 1, tags: [10, 11] },
          { id: 2, tags: [11] },
        ] as TrackRow[],
      ],
    ]),
  });
});
afterEach(cleanup);

describe("TagSubmenu", () => {
  it("shows all / mixed / none per tag for the selected tracks", () => {
    openMenu([1, 2]);
    const state = (name: string) =>
      screen.getByRole("menuitemcheckbox", { name }).getAttribute("aria-checked");
    expect(state("House")).toBe("mixed");
    expect(state("Techno")).toBe("true");
  });

  it("clicking a mixed tag assigns it to every selected track; the menu stays open", async () => {
    openMenu([1, 2]);
    fireEvent.click(screen.getByRole("menuitemcheckbox", { name: "House" }));
    await vi.waitFor(() =>
      expect(ipc.tagsMutate).toHaveBeenCalledWith({
        type: "assign",
        trackIds: [1, 2],
        tagIds: [10],
      }),
    );
    expect(screen.getByRole("menuitemcheckbox", { name: "Techno" })).toBeTruthy();
  });

  it("clicking a tag every track has removes it", async () => {
    openMenu([1, 2]);
    fireEvent.click(screen.getByRole("menuitemcheckbox", { name: "Techno" }));
    await vi.waitFor(() =>
      expect(ipc.tagsMutate).toHaveBeenCalledWith({
        type: "unassign",
        trackIds: [1, 2],
        tagIds: [11],
      }),
    );
  });

  it("offers Create 'X' per category and assigns the new tag", async () => {
    vi.mocked(ipc.tagsMutate).mockResolvedValueOnce({
      createdId: 12,
      changed: 0,
      commentsUpdated: 0,
    });
    openMenu([2]);
    fireEvent.change(screen.getByRole("searchbox", { name: "Find a tag" }), {
      target: { value: "Deep" },
    });
    fireEvent.click(screen.getByRole("menuitem", { name: "Create “Deep” in Genre" }));
    await vi.waitFor(() =>
      expect(ipc.tagsMutate).toHaveBeenCalledWith({
        type: "createTag",
        categoryId: 1,
        name: "Deep",
      }),
    );
    await vi.waitFor(() =>
      expect(ipc.tagsMutate).toHaveBeenCalledWith({
        type: "assign",
        trackIds: [2],
        tagIds: [12],
      }),
    );
  });
});
