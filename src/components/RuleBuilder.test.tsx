// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { PlaylistNode, RuleNode, TrackQuery } from "../lib/ipc";

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
const { useLibrary, noFilters } = await import("../store/library");
const { usePlaylists } = await import("../store/playlists");
const { initialTree } = await import("../store/playlistTree");
const { useRules } = await import("../store/rules");
const { emptyRoot } = await import("../store/ruleTree");
const { useToasts } = await import("../store/toasts");
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

const lastRule = () => {
  const call = vi.mocked(ipc.libraryQuery).mock.calls.at(-1);
  return (call?.[0] as TrackQuery | undefined)?.rule;
};

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
  useLibrary.setState({
    filters: noFilters,
    filtersEnabled: true,
    rule: null,
    total: 42,
    generation: 1,
    pagesGeneration: 1,
    facets: { genres: ["House", "Techno"], keys: [], colours: ["red"], playlists: nodes },
  });
  useRules.setState({ root: emptyRoot(), open: true, editing: null, focusId: null });
  useToasts.setState({ toast: null });
});
afterEach(cleanup);

/** Fills the first row as "Genre is House". */
function pickGenre() {
  fireEvent.change(screen.getByRole("combobox", { name: "Field" }), {
    target: { value: "genre" },
  });
  fireEvent.change(screen.getByRole("combobox", { name: "Genre" }), {
    target: { value: "House" },
  });
}

describe("RuleBuilder", () => {
  it("opens with one empty row in an all group and says what to do", () => {
    render(<RuleBuilder />);
    expect(screen.getByRole("group", { name: "Group: match all" })).toBeTruthy();
    expect(screen.getAllByRole("combobox", { name: "Field" })).toHaveLength(1);
    expect(screen.getByText(/Add a condition to narrow the list/)).toBeTruthy();
    expect(
      screen.getByRole("button", { name: "Save as smart playlist…" }).hasAttribute("disabled"),
    ).toBe(true);
  });

  it("sends the rule once a row is finished, and an unfinished row sends nothing", async () => {
    render(<RuleBuilder />);
    fireEvent.change(screen.getByRole("combobox", { name: "Field" }), {
      target: { value: "genre" },
    });
    // A field with no value yet is muted and not applied.
    expect(screen.getByText("Not applied yet")).toBeTruthy();
    await new Promise((r) => setTimeout(r, 120));
    expect(ipc.libraryQuery).not.toHaveBeenCalled();

    fireEvent.change(screen.getByRole("combobox", { name: "Genre" }), {
      target: { value: "House" },
    });
    await waitFor(() => expect(lastRule()).toBeDefined());
    expect(lastRule()).toEqual({
      type: "group",
      match: "all",
      not: false,
      rules: [{ type: "filter", not: false, filter: { type: "genre", genre: "House" } }],
    } satisfies RuleNode);
    expect(screen.getByTestId("rule-summary").textContent).toBe("Genre is House → 42 tracks");
  });

  it("is not / none change the rule that is sent", async () => {
    render(<RuleBuilder />);
    pickGenre();
    fireEvent.change(screen.getByRole("combobox", { name: "Is or is not" }), {
      target: { value: "not" },
    });
    fireEvent.click(screen.getByRole("radio", { name: "none" }));
    await waitFor(() =>
      expect(lastRule()).toMatchObject({
        match: "any",
        not: true,
        rules: [{ not: true, filter: { type: "genre", genre: "House" } }],
      }),
    );
    expect(screen.getByRole("group", { name: "Group: match none" })).toBeTruthy();
  });

  it("adds a nested group, up to three levels", () => {
    render(<RuleBuilder />);
    fireEvent.click(screen.getByRole("button", { name: "Add group" }));
    const groups = screen.getAllByRole("group", { name: /^Group:/ });
    expect(groups).toHaveLength(2);
    fireEvent.click(within(groups[1] as HTMLElement).getByRole("button", { name: "Add group" }));
    expect(screen.getAllByRole("group", { name: /^Group:/ })).toHaveLength(3);
    // The third level offers no further group.
    const third = screen.getAllByRole("group", { name: /^Group:/ })[2] as HTMLElement;
    expect(within(third).queryByRole("button", { name: "Add group" })).toBeNull();
  });

  it("Enter adds a row and focuses its field; Backspace on an empty row removes it", () => {
    render(<RuleBuilder />);
    const first = screen.getByRole("combobox", { name: "Field" });
    first.focus();
    fireEvent.keyDown(first, { key: "Enter" });
    const fields = screen.getAllByRole("combobox", { name: "Field" });
    expect(fields).toHaveLength(2);
    expect(document.activeElement).toBe(fields[1]);

    fireEvent.keyDown(fields[1] as HTMLElement, { key: "Backspace" });
    expect(screen.getAllByRole("combobox", { name: "Field" })).toHaveLength(1);
    // The last row stays: nothing to remove it into.
    const only = screen.getByRole("combobox", { name: "Field" });
    fireEvent.keyDown(only, { key: "Backspace" });
    expect(screen.getAllByRole("combobox", { name: "Field" })).toHaveLength(1);
  });

  it("saves the rule as a smart playlist and clears the builder", async () => {
    render(<RuleBuilder />);
    pickGenre();
    fireEvent.click(await screen.findByRole("button", { name: "Save as smart playlist…" }));
    fireEvent.change(await screen.findByRole("textbox", { name: "Name" }), {
      target: { value: "House only" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(ipc.playlistsMutate).toHaveBeenCalled());
    expect(ipc.playlistsMutate).toHaveBeenCalledWith({
      type: "createSmart",
      parentId: null,
      name: "House only",
      rule: {
        type: "group",
        match: "all",
        not: false,
        rules: [{ type: "filter", not: false, filter: { type: "genre", genre: "House" } }],
      },
    });
    await waitFor(() => expect(useToasts.getState().toast?.message).toMatch(/House only/));
    expect(useLibrary.getState().rule).toBeNull();
  });

  it("edits a smart playlist's rules and saves them with setRule", async () => {
    const rule: RuleNode = {
      type: "group",
      match: "any",
      not: true,
      rules: [{ type: "filter", not: false, filter: { type: "playlist", id: 2 } }],
    };
    vi.mocked(ipc.playlistRule).mockResolvedValue(rule);
    render(<RuleBuilder />);
    await useRules.getState().editSmart(node(5, "Smart one", true));
    expect(await screen.findByText(/Editing the rules of/)).toBeTruthy();
    expect(screen.getByRole("group", { name: "Group: match none" })).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Save rules" }));
    await waitFor(() =>
      expect(ipc.playlistsMutate).toHaveBeenCalledWith({ type: "setRule", id: 5, rule }),
    );
    await waitFor(() => expect(useRules.getState().editing).toBeNull());
  });
});
