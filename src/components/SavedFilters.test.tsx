// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { FilterPreset, RuleNode } from "../lib/ipc";

vi.mock("../lib/ipc", () => ({
  DECKS: 4,
  filterPresetsGet: vi.fn(),
  filterPresetsMutate: vi.fn(),
  libraryFacets: vi.fn(),
  libraryQuery: vi.fn(),
  playlistsGet: vi.fn(),
  playlistsMutate: vi.fn(),
  playlistRule: vi.fn(),
  tagsGet: vi.fn(),
}));

const ipc = await import("../lib/ipc");
const { useFilterPresets } = await import("../store/filterPresets");
const { useLibrary } = await import("../store/library");
const { useRules } = await import("../store/rules");
const { freshRoot, toRule } = await import("../store/ruleTree");
const { useTags } = await import("../store/tags");
const { useToasts } = await import("../store/toasts");
const { FilterDrawer } = await import("./FilterDrawer");

const genre = (g: string): RuleNode => ({
  type: "filter",
  not: false,
  filter: { type: "genre", genre: g },
});
const preset = (id: number, name: string, rule: RuleNode = genre(name)): FilterPreset => ({
  id,
  name,
  rule,
  createdAt: "2026-10-23T00:00:00Z",
  updatedAt: "2026-10-23T00:00:00Z",
});
const LIST = [preset(1, "House"), preset(2, "Techno"), preset(3, "Disco")];

const picker = () => screen.getByRole("button", { name: /^Saved filter/ });
const list = () => screen.getByRole("list", { name: "Saved filters" });
const genreButton = () => screen.getByRole("button", { name: /^Genre: / });

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(ipc.libraryQuery).mockResolvedValue({
    rows: [],
    total: 3,
    dividers: [],
    dividerView: false,
  });
  vi.mocked(ipc.filterPresetsGet).mockResolvedValue(LIST);
  vi.mocked(ipc.filterPresetsMutate).mockResolvedValue({ createdId: 9 });
  useFilterPresets.setState({ presets: LIST, status: "ready", error: null });
  useTags.setState({ tree: { categories: [] }, status: "ready", error: null });
  useLibrary.setState({
    playlistId: null,
    rule: null,
    filtersEnabled: true,
    facetsError: null,
    facets: { genres: ["House", "Techno", "Disco"], keys: [], colours: [], playlists: [] },
  });
  useRules.setState({
    root: freshRoot(["bpm", "genre"]),
    editing: null,
    focus: null,
    defaultFields: null,
    dismissed: [],
    lastMatched: null,
    addOpen: false,
    preset: null,
    saveTarget: "filter",
  });
  useToasts.setState({ toast: null });
});
afterEach(cleanup);

describe("saved filters picker", () => {
  it("loads one: its name shows, and an edit offers Update and Revert", async () => {
    render(<FilterDrawer />);
    expect(picker().textContent).toBe("Saved filters");
    fireEvent.click(picker());
    fireEvent.click(within(list()).getByRole("button", { name: "Techno" }));
    expect(picker().getAttribute("aria-label")).toBe("Saved filter: Techno");
    expect(genreButton().getAttribute("aria-label")).toBe("Genre: Techno");
    expect(screen.queryByText("Changed since you saved it.")).toBeNull();

    fireEvent.click(genreButton());
    fireEvent.click(screen.getByRole("checkbox", { name: "House" }));
    expect(screen.getByText("edited")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Update" }));
    await waitFor(() =>
      expect(ipc.filterPresetsMutate).toHaveBeenCalledWith({
        type: "setRule",
        id: 2,
        rule: toRule(useRules.getState().root),
      }),
    );
    await waitFor(() => expect(screen.queryByText("edited")).toBeNull());
  });

  it("says when there are none yet, or they failed to load", () => {
    useFilterPresets.setState({ presets: [], status: "ready" });
    render(<FilterDrawer />);
    fireEvent.click(picker());
    expect(screen.getByText(/No saved filters yet/)).toBeTruthy();
    cleanup();
    useFilterPresets.setState({ status: "error", error: "disk full" });
    render(<FilterDrawer />);
    fireEvent.click(picker());
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(ipc.filterPresetsGet).toHaveBeenCalled();
  });

  it("moves, renames and deletes with Undo", async () => {
    render(<FilterDrawer />);
    fireEvent.click(picker());
    const options = (name: string) =>
      fireEvent.keyDown(within(list()).getByRole("button", { name: `${name} options` }), {
        key: "Enter",
      });

    options("House");
    fireEvent.click(await screen.findByRole("menuitem", { name: "Move down" }));
    // Past Techno: the list as it is, House included.
    expect(ipc.filterPresetsMutate).toHaveBeenCalledWith({ type: "move", id: 1, position: 2 });

    options("Techno");
    fireEvent.click(await screen.findByRole("menuitem", { name: "Rename…" }));
    const field = screen.getByRole("textbox", { name: "Rename Techno" });
    fireEvent.change(field, { target: { value: "Hard techno" } });
    fireEvent.keyDown(field, { key: "Enter" });
    expect(ipc.filterPresetsMutate).toHaveBeenCalledWith({
      type: "rename",
      id: 2,
      name: "Hard techno",
    });

    options("Disco");
    fireEvent.click(await screen.findByRole("menuitem", { name: "Delete" }));
    await waitFor(() => expect(useToasts.getState().toast?.message).toBe("Deleted “Disco”"));
    useToasts.getState().toast?.action?.run();
    expect(ipc.filterPresetsMutate).toHaveBeenLastCalledWith({
      type: "create",
      name: "Disco",
      rule: genre("Disco"),
      position: 2,
    });
  });

  it("drags one onto another to reorder", () => {
    render(<FilterDrawer />);
    fireEvent.click(picker());
    const item = (name: string) =>
      within(list()).getByRole("button", { name }).closest("li") as HTMLElement;
    fireEvent.dragStart(item("Disco"), { dataTransfer: { effectAllowed: "" } });
    fireEvent.dragOver(item("House"));
    fireEvent.drop(item("House"));
    expect(ipc.filterPresetsMutate).toHaveBeenCalledWith({ type: "move", id: 3, position: 0 });
  });
});

describe("Save…", () => {
  function setHouse() {
    useRules.getState().loadPreset(preset(99, "tmp", genre("House")));
    useRules.setState({ preset: null });
  }

  it("suggests a name and saves a saved filter by default", async () => {
    setHouse();
    render(<FilterDrawer />);
    fireEvent.click(screen.getByRole("button", { name: "Save…" }));
    const name = await screen.findByRole("textbox", { name: "Name" });
    expect((name as HTMLInputElement).value).toBe("House");
    expect(screen.getByRole("radio", { name: "Saved filter" }).getAttribute("aria-checked")).toBe(
      "true",
    );
    fireEvent.change(name, { target: { value: "Just house" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(ipc.filterPresetsMutate).toHaveBeenCalledWith({
        type: "create",
        name: "Just house",
        rule: toRule(useRules.getState().root),
      }),
    );
    await waitFor(() =>
      expect(picker().getAttribute("aria-label")).toBe("Saved filter: Just house"),
    );
  });

  it("a name already taken asks first, then replaces that one", async () => {
    setHouse();
    render(<FilterDrawer />);
    fireEvent.click(screen.getByRole("button", { name: "Save…" }));
    fireEvent.change(await screen.findByRole("textbox", { name: "Name" }), {
      target: { value: "techno" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(
      await screen.findByText("“Techno” already exists. Save again to replace it."),
    ).toBeTruthy();
    expect(ipc.filterPresetsMutate).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Replace" }));
    await waitFor(() =>
      expect(ipc.filterPresetsMutate).toHaveBeenCalledWith({
        type: "setRule",
        id: 2,
        rule: toRule(useRules.getState().root),
      }),
    );
  });

  it("remembers a smart playlist as the choice", async () => {
    setHouse();
    render(<FilterDrawer />);
    fireEvent.click(screen.getByRole("button", { name: "Save…" }));
    fireEvent.click(await screen.findByRole("radio", { name: "Smart playlist" }));
    expect(useRules.getState().saveTarget).toBe("smart");
    expect(screen.getByText(/written as a fixed list/)).toBeTruthy();
  });
});
