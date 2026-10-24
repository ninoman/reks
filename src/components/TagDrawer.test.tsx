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
const { TagDrawer } = await import("./TagDrawer");

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

function select(indices: number[]) {
  useLibrary.setState({
    total: 2,
    dividers: [],
    pages: new Map([
      [
        0,
        [
          { id: 1, title: "One", artist: "A", tags: [10, 11] },
          { id: 2, title: "Two", artist: "B", tags: [11] },
        ] as TrackRow[],
      ],
    ]),
    selection: { selected: new Set(indices), anchor: null, focus: null },
  });
}

const box = (name: string) => screen.getByRole("checkbox", { name: `Attach ${name}` });

beforeEach(() => {
  vi.mocked(ipc.tagsMutate).mockResolvedValue({ createdId: null, changed: 1, commentsUpdated: 0 });
  vi.mocked(ipc.tagsGet).mockResolvedValue(tree);
  useTags.setState({ tree, status: "ready", error: null, managerOpen: true });
});
afterEach(cleanup);

describe("TagDrawer", () => {
  it("shows all / mixed / none for the selected tracks", () => {
    select([0, 1]);
    render(<TagDrawer />);
    expect(screen.getByText(/attach it to 2 selected tracks/)).toBeTruthy();
    expect((box("House") as HTMLInputElement).indeterminate).toBe(true);
    expect((box("Techno") as HTMLInputElement).checked).toBe(true);
  });

  it("ticking a tag attaches it to the selection", async () => {
    select([1]);
    render(<TagDrawer />);
    expect(screen.getByText(/attach it to B – Two/)).toBeTruthy();
    fireEvent.click(box("House"));
    await vi.waitFor(() =>
      expect(ipc.tagsMutate).toHaveBeenCalledWith({ type: "assign", trackIds: [2], tagIds: [10] }),
    );
  });

  it("shows no checkboxes with nothing selected", () => {
    select([]);
    render(<TagDrawer />);
    expect(screen.queryByRole("checkbox")).toBeNull();
  });

  it("× asks before deleting a tag", async () => {
    select([]);
    render(<TagDrawer />);
    fireEvent.click(screen.getByRole("button", { name: "Delete tag House" }));
    expect(ipc.tagsMutate).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    await vi.waitFor(() => expect(ipc.tagsMutate).toHaveBeenCalledWith({ type: "delete", id: 10 }));
  });

  it("a tag added while a track is selected is attached to it", async () => {
    vi.mocked(ipc.tagsMutate).mockResolvedValueOnce({
      createdId: 12,
      changed: 0,
      commentsUpdated: 0,
    });
    select([0]);
    render(<TagDrawer />);
    const field = screen.getByRole("textbox", { name: "New tag in Genre" });
    fireEvent.change(field, { target: { value: "Deep" } });
    fireEvent.keyDown(field, { key: "Enter" });
    await vi.waitFor(() =>
      expect(ipc.tagsMutate).toHaveBeenCalledWith({ type: "assign", trackIds: [1], tagIds: [12] }),
    );
    expect(ipc.tagsMutate).toHaveBeenCalledWith({ type: "createTag", categoryId: 1, name: "Deep" });
  });

  it("a click on a tag name opens it for renaming; Enter saves", async () => {
    select([]);
    render(<TagDrawer />);
    fireEvent.click(screen.getByRole("button", { name: "Tag name: House" }));
    const field = screen.getByRole("textbox", { name: "Tag name: House" });
    fireEvent.change(field, { target: { value: "Deep House" } });
    fireEvent.keyDown(field, { key: "Enter" });
    await vi.waitFor(() =>
      expect(ipc.tagsMutate).toHaveBeenCalledWith({ type: "rename", id: 10, name: "Deep House" }),
    );
  });

  it("Escape closes the drawer", () => {
    select([]);
    render(<TagDrawer />);
    fireEvent.keyDown(screen.getByRole("textbox", { name: "New category" }), { key: "Escape" });
    expect(useTags.getState().managerOpen).toBe(false);
  });
});

describe("TagDrawer drag and drop", () => {
  function dataTransfer() {
    const data = new Map<string, string>();
    return {
      get types() {
        return [...data.keys()];
      },
      setData: (k: string, v: string) => data.set(k, v),
      getData: (k: string) => data.get(k) ?? "",
      setDragImage: () => {},
      dropEffect: "none",
      effectAllowed: "all",
    };
  }

  it("dropping House below Techno moves it there", async () => {
    select([]);
    render(<TagDrawer />);
    const dt = dataTransfer();
    const grip = screen.getByTitle("Drag to move House");
    const techno = screen.getByRole("button", { name: "Tag name: Techno" }).closest("li");
    if (techno === null) throw new Error("no row");
    vi.spyOn(techno, "getBoundingClientRect").mockReturnValue({
      top: 0,
      height: 20,
    } as DOMRect);
    fireEvent.dragStart(grip, { dataTransfer: dt });
    fireEvent.dragOver(techno, { dataTransfer: dt, clientY: 15 });
    expect(techno.getAttribute("data-drop")).toBe("after");
    fireEvent.drop(techno, { dataTransfer: dt, clientY: 15 });
    await vi.waitFor(() =>
      expect(ipc.tagsMutate).toHaveBeenCalledWith({
        type: "move",
        id: 10,
        categoryId: 1,
        position: 2,
      }),
    );
  });
});
