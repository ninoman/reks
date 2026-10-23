// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { PlaylistNode, RuleNode } from "../lib/ipc";

vi.mock("../lib/ipc", () => ({
  DECKS: 4,
  libraryFacets: vi.fn(),
  libraryQuery: vi.fn(),
  playlistsGet: vi.fn(),
  playlistsMutate: vi.fn(),
  playlistRule: vi.fn(),
  tagsGet: vi.fn(),
}));

const ipc = await import("../lib/ipc");
const { useLibrary } = await import("../store/library");
const { usePlaylists } = await import("../store/playlists");
const { initialTree } = await import("../store/playlistTree");
const { useRules } = await import("../store/rules");
const { freshRoot } = await import("../store/ruleTree");
const { useTags } = await import("../store/tags");
const { useToasts } = await import("../store/toasts");
const { FilterDrawer } = await import("./FilterDrawer");
const { RuleBuilder } = await import("./RuleBuilder");

const node = (id: number, name: string, smart = false): PlaylistNode => ({
  id,
  parentId: null,
  name,
  isFolder: false,
  depth: 0,
  position: id,
  entries: 3,
  smart,
});

const DEFAULTS = ["bpm", "key", "genre", "rating"] as const;

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(ipc.libraryQuery).mockResolvedValue({
    rows: [],
    total: 42,
    dividers: [],
    dividerView: false,
  });
  vi.mocked(ipc.libraryFacets).mockResolvedValue({
    genres: [],
    keys: [],
    colours: [],
    playlists: [],
  });
  vi.mocked(ipc.playlistsMutate).mockResolvedValue({
    type: "done",
    createdId: 9,
    added: 0,
    skipped: 0,
  });
  const nodes = [node(2, "Warm-up"), node(3, "Closing")];
  vi.mocked(ipc.playlistsGet).mockResolvedValue({ nodes });
  usePlaylists.setState({ nodes, tree: initialTree, error: null });
  useTags.setState({ tree: { categories: [] }, status: "ready", error: null });
  useLibrary.setState({
    playlistId: null,
    filtersEnabled: true,
    rule: null,
    facets: { genres: ["House", "Techno"], keys: [], colours: ["red"], playlists: nodes },
    facetsError: null,
  });
  useRules.setState({
    root: freshRoot([...DEFAULTS]),
    editing: null,
    focus: null,
    defaultFields: null,
    dismissed: [],
    lastMatched: null,
  });
  useToasts.setState({ toast: null });
});
afterEach(cleanup);

/** Sets the Genre row to House. */
function pickGenre() {
  const genre = screen.getByRole("group", { name: "Genre filter" });
  fireEvent.click(within(genre).getByRole("button", { name: /^Genre: / }));
  fireEvent.click(screen.getByRole("checkbox", { name: "House" }));
}

const genreRule: RuleNode = {
  type: "group",
  match: "all",
  not: false,
  rules: [{ type: "filter", not: false, filter: { type: "genre", genre: "House" } }],
};

describe("RuleBuilder", () => {
  it("says so when every row was removed, and Add filter brings one back", () => {
    useRules.setState({ root: freshRoot([]), dismissed: [...DEFAULTS] });
    render(<RuleBuilder />);
    expect(screen.getByText("No filters here. Add one below.")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Add filter" }));
    fireEvent.click(screen.getByRole("option", { name: "Plays" }));
    expect(screen.getByRole("group", { name: "Plays filter" })).toBeTruthy();
    expect(screen.getByRole("spinbutton", { name: "Minimum Plays" })).toBeTruthy();
  });

  it("saves the whole drawer as a smart playlist and empties it", async () => {
    render(<FilterDrawer />);
    pickGenre();
    fireEvent.keyDown(document.activeElement as HTMLElement, { key: "Escape" });
    fireEvent.click(await screen.findByRole("button", { name: "Save as smart playlist…" }));
    fireEvent.change(await screen.findByRole("textbox", { name: "Name" }), {
      target: { value: "House only" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(ipc.playlistsMutate).toHaveBeenCalledWith({
        type: "createSmart",
        parentId: null,
        name: "House only",
        rule: genreRule,
      }),
    );
    await waitFor(() => expect(useToasts.getState().toast?.message).toMatch(/House only/));
    expect(useLibrary.getState().rule).toBeNull();
  });

  it("edits a smart playlist's rules in the drawer and saves them with setRule", async () => {
    const rule: RuleNode = {
      type: "group",
      match: "any",
      not: true,
      rules: [{ type: "filter", not: false, filter: { type: "playlist", id: 2 } }],
    };
    vi.mocked(ipc.playlistRule).mockResolvedValue(rule);
    render(<FilterDrawer />);
    await useRules.getState().editSmart(node(5, "Smart one", true));
    expect(await screen.findByText(/\(smart playlist\)/)).toBeTruthy();
    // "Not in Warm-up" reads back as a Playlist row, None of; unused defaults stay empty around it.
    const playlist = screen.getByRole("group", { name: "Playlist filter" });
    expect(within(playlist).getByText("not")).toBeTruthy();
    expect(screen.getByRole("group", { name: "BPM filter" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Save as smart playlist…" })).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "Save rules" }));
    await waitFor(() =>
      expect(ipc.playlistsMutate).toHaveBeenCalledWith({
        type: "setRule",
        id: 5,
        rule: {
          type: "group",
          match: "all",
          not: false,
          rules: [{ type: "filter", not: true, filter: { type: "playlist", id: 2 } }],
        },
      }),
    );
    await waitFor(() => expect(useRules.getState().editing).toBeNull());
  });

  it("Cancel puts the earlier drawer back", async () => {
    vi.mocked(ipc.playlistRule).mockResolvedValue({
      type: "filter",
      filter: { type: "neverPlayed" },
    });
    render(<FilterDrawer />);
    pickGenre();
    await useRules.getState().editSmart(node(5, "Smart one", true));
    expect(await screen.findByRole("group", { name: "Last played filter" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(useLibrary.getState().rule).toEqual(genreRule));
    expect(screen.queryByRole("group", { name: "Last played filter" })).toBeNull();
  });
});
