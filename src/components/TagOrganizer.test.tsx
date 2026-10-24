// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { TagTree } from "../lib/ipc";

vi.mock("../lib/ipc", () => ({
  tagsGet: vi.fn(),
  tagsMutate: vi.fn(),
  tagsDuplicates: vi.fn(),
  commentTagsPreview: vi.fn(),
  commentTagsImport: vi.fn(),
  libraryFacets: vi.fn(),
  libraryQuery: vi.fn(),
}));

const ipc = await import("../lib/ipc");
const { useTagOrganizer } = await import("../store/tagOrganizer");
const { TagOrganizer } = await import("./TagOrganizer");

const tree: TagTree = {
  categories: [
    { id: 1, name: "Genre", tags: [{ id: 10, name: "House", trackCount: 3 }] },
    {
      id: 2,
      name: "Imported",
      tags: [
        { id: 20, name: "Prog Trance", trackCount: 2 },
        { id: 21, name: "prog-trance", trackCount: 5 },
        { id: 22, name: "Closing", trackCount: 0 },
      ],
    },
  ],
};

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(ipc.tagsGet).mockResolvedValue(tree);
  vi.mocked(ipc.tagsDuplicates).mockResolvedValue([[20, 21]]);
  vi.mocked(ipc.tagsMutate).mockResolvedValue({ createdId: null, changed: 1, commentsUpdated: 0 });
  useTagOrganizer.getState().close();
});
afterEach(cleanup);

describe("TagOrganizer", () => {
  it("asks with the counts, reads, then organizes the tags read", async () => {
    vi.mocked(ipc.commentTagsPreview).mockResolvedValue({ tracks: 12, names: 3, newNames: 2 });
    vi.mocked(ipc.commentTagsImport).mockResolvedValue({
      tracks: 12,
      created: 2,
      assigned: 20,
      tagIds: [20, 21, 22],
    });
    render(<TagOrganizer />);
    await useTagOrganizer.getState().startRead();
    expect(await screen.findByText(/12 tracks, 3 tags \(2 new\)/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Read" }));
    expect(await screen.findByRole("heading", { name: "Organize My Tags" })).toBeTruthy();
    // Only the run's tags: House is not shown.
    expect(screen.getByRole("checkbox", { name: "Select Closing" })).toBeTruthy();
    expect(screen.queryByRole("checkbox", { name: "Select House" })).toBeNull();
    fireEvent.click(screen.getByRole("radio", { name: "All tags" }));
    expect(screen.getByRole("checkbox", { name: "Select House" })).toBeTruthy();
  });

  it("says when no comment has a block", async () => {
    vi.mocked(ipc.commentTagsPreview).mockResolvedValue({ tracks: 0, names: 0, newNames: 0 });
    render(<TagOrganizer />);
    await useTagOrganizer.getState().startRead();
    expect(await screen.findByText(/No comment ends with/)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Read" })).toBeNull();
  });

  it("merges a duplicate group into the tag on most tracks", async () => {
    render(<TagOrganizer />);
    await useTagOrganizer.getState().open(null);
    const keep = await screen.findByRole("combobox", { name: /Tag to keep of/ });
    expect((keep as HTMLSelectElement).value).toBe("21");
    fireEvent.click(screen.getByRole("button", { name: "Merge" }));
    await vi.waitFor(() =>
      expect(ipc.tagsMutate).toHaveBeenCalledWith({ type: "merge", ids: [20], into: 21 }),
    );
  });

  it("moves, merges and deletes the ticked tags", async () => {
    render(<TagOrganizer />);
    await useTagOrganizer.getState().open(null);
    await screen.findByRole("heading", { name: "Organize My Tags" });
    const bar = screen.getByRole("toolbar", { name: "Selected tags" });
    expect(within(bar).getByText(/Tick tags/)).toBeTruthy();

    fireEvent.click(screen.getByRole("checkbox", { name: "Select Closing" }));
    fireEvent.change(within(bar).getByRole("combobox", { name: "Move selected to category" }), {
      target: { value: "1" },
    });
    await vi.waitFor(() =>
      expect(ipc.tagsMutate).toHaveBeenCalledWith({
        type: "move",
        id: 22,
        categoryId: 1,
        position: 1,
      }),
    );

    // The selection clears once the move is done.
    await within(bar).findByText(/Tick tags/);
    fireEvent.click(screen.getByRole("checkbox", { name: "Select Prog Trance" }));
    fireEvent.click(screen.getByRole("checkbox", { name: "Select Closing" }));
    fireEvent.click(within(bar).getByRole("button", { name: "Merge…" }));
    const merge = await screen.findByRole("dialog", { name: "Merge tags" });
    fireEvent.click(within(merge).getByRole("button", { name: "Merge" }));
    await vi.waitFor(() =>
      expect(ipc.tagsMutate).toHaveBeenCalledWith({ type: "merge", ids: [22], into: 20 }),
    );

    await within(bar).findByText(/Tick tags/);
    fireEvent.click(screen.getByRole("checkbox", { name: "Select House" }));
    fireEvent.click(within(bar).getByRole("button", { name: "Delete…" }));
    const del = await screen.findByRole("dialog", { name: "Delete tag" });
    expect(within(del).getByText(/comes off 3 tracks/)).toBeTruthy();
    fireEvent.click(within(del).getByRole("button", { name: "Delete" }));
    await vi.waitFor(() => expect(ipc.tagsMutate).toHaveBeenCalledWith({ type: "delete", id: 10 }));
  });

  it("shows a retry when the tags can't load", async () => {
    vi.mocked(ipc.tagsGet).mockRejectedValue(new Error("db locked"));
    const { useTags } = await import("../store/tags");
    useTags.setState({ tree: null, status: "loading", error: null });
    render(<TagOrganizer />);
    await useTagOrganizer.getState().open(null);
    expect(await screen.findByRole("button", { name: "Retry" })).toBeTruthy();
  });
});
