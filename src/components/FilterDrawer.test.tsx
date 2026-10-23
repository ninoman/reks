// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { RuleNode, Settings, TagTree, TrackQuery } from "../lib/ipc";

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
const { useTags } = await import("../store/tags");
const { useLibrary } = await import("../store/library");
const { useRules } = await import("../store/rules");
const { freshRoot, newGroup, newRow, toRule } = await import("../store/ruleTree");
const { useSettings } = await import("../store/settings");
const { useToasts } = await import("../store/toasts");
const { FilterDrawer, FiltersSwitch } = await import("./FilterDrawer");
const { EmptyResult } = await import("./EmptyResult");

const tree: TagTree = {
  categories: [
    {
      id: 1,
      name: "Genre",
      tags: [
        { id: 10, name: "House", trackCount: 4 },
        { id: 11, name: "Techno", trackCount: 2 },
      ],
    },
    {
      id: 2,
      name: "Situation",
      tags: [
        { id: 20, name: "Peak time", trackCount: 1 },
        { id: 21, name: "Closing", trackCount: 1 },
      ],
    },
  ],
};

function notation(keyNotation: Settings["keyNotation"]) {
  useSettings.setState({ settings: { keyNotation } as Settings });
}

const lastRule = () =>
  (vi.mocked(ipc.libraryQuery).mock.calls.at(-1)?.[0] as TrackQuery | undefined)?.rule;

const esc = (s: string) => s.replace(/[()]/g, "\\$&");

/** The row of a field, by its accessible name ("Genre", "Situation (My Tags)"). */
const row = (name: string) => screen.getByRole("group", { name: `${name} filter` });
const handle = (name: string) =>
  within(row(name)).getByRole("button", { name: new RegExp(`^${esc(name)}(, paused)?$`) });

/** Opens a row's value picker. */
function openPicker(name: string) {
  fireEvent.click(within(row(name)).getByRole("button", { name: new RegExp(`^${esc(name)}: `) }));
}

/** Opens a Radix dropdown from its trigger, as the keyboard does. */
function openMenu(trigger: HTMLElement) {
  fireEvent.keyDown(trigger, { key: "Enter" });
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(ipc.libraryQuery).mockResolvedValue({
    rows: [],
    total: 0,
    dividers: [],
    dividerView: false,
  });
  useTags.setState({ tree, status: "ready", error: null });
  useLibrary.setState({
    playlistId: 4,
    rule: null,
    filtersEnabled: true,
    facetsError: null,
    facets: {
      genres: ["House", "Techno"],
      keys: [
        { key: "Am", camelot: "8A", openKey: "1m" },
        { key: "C", camelot: "8B", openKey: "1d" },
      ],
      colours: ["red", "blue"],
      playlists: [],
    },
  });
  useRules.setState({
    root: freshRoot(["bpm", "key", "genre", "rating", "tag:1"]),
    editing: null,
    focus: null,
    defaultFields: null,
    dismissed: [],
    lastMatched: null,
  });
  useToasts.setState({ toast: null });
  notation("classical");
});
afterEach(cleanup);

describe("FilterDrawer", () => {
  it("first open: the default fields, empty; Clear all and Save are off; no count", () => {
    render(<FilterDrawer />);
    const rows = screen.getAllByRole("group", { name: / filter$/ }).map((g) => g.ariaLabel);
    expect(rows).toEqual([
      "BPM filter",
      "Key filter",
      "Genre filter",
      "Rating filter",
      "Genre (My Tags) filter",
    ]);
    expect(screen.getByRole("button", { name: "Clear all" }).hasAttribute("disabled")).toBe(true);
    expect(
      screen.getByRole("button", { name: "Save as smart playlist…" }).hasAttribute("disabled"),
    ).toBe(true);
    expect(screen.queryByText(/tracks?$/)).toBeNull();
    expect(screen.getAllByRole("button", { name: "Match any of these instead" })).toHaveLength(4);
  });

  it("a list picker sets Any of / None of and the row filters at once", async () => {
    render(<FilterDrawer />);
    openPicker("Genre");
    fireEvent.click(screen.getByRole("checkbox", { name: "House" }));
    fireEvent.click(screen.getByRole("checkbox", { name: "Techno" }));
    expect(row("Genre").dataset.active).toBe("true");
    await waitFor(() =>
      expect(lastRule()).toMatchObject({
        rules: [{ type: "group", match: "any", not: false, rules: [{}, {}] }],
      }),
    );
    fireEvent.click(screen.getByRole("radio", { name: "None of" }));
    await waitFor(() => expect(lastRule()).toMatchObject({ rules: [{ not: true }] }));
    expect(within(row("Genre")).getByText("not")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Clear all" }).hasAttribute("disabled")).toBe(false);
  });

  it("a tag category row picks only its tags; All of shows the all mark", async () => {
    render(<FilterDrawer />);
    openPicker("Genre (My Tags)");
    expect(screen.queryByRole("checkbox", { name: "Peak time" })).toBeNull();
    fireEvent.click(screen.getByRole("checkbox", { name: "House" }));
    fireEvent.click(screen.getByRole("checkbox", { name: "Techno" }));
    fireEvent.click(screen.getByRole("radio", { name: "All of" }));
    expect(within(row("Genre (My Tags)")).getByText("all")).toBeTruthy();
    await waitFor(() =>
      expect(lastRule()).toMatchObject({
        rules: [
          {
            type: "group",
            match: "all",
            rules: [{ filter: { type: "tagAny", ids: [10] } }, { filter: { ids: [11] } }],
          },
        ],
      }),
    );
  });

  it("labels keys in the notation chosen in Settings", () => {
    notation("camelot");
    render(<FilterDrawer />);
    openPicker("Key");
    fireEvent.click(screen.getByRole("button", { name: "8A" }));
    expect(within(row("Key")).getByRole("button", { name: "Key: 8A" })).toBeTruthy();
  });

  it("rating takes at least / exactly / at most", async () => {
    render(<FilterDrawer />);
    openPicker("Rating");
    fireEvent.click(screen.getByRole("radio", { name: "At most" }));
    fireEvent.click(screen.getByRole("button", { name: "3 stars" }));
    await waitFor(() =>
      expect(lastRule()).toMatchObject({
        rules: [{ type: "filter", not: true, filter: { type: "ratingMin", rating: 4 } }],
      }),
    );
  });

  it("pickers show loading, and an error with Retry", () => {
    useTags.setState({ tree: null, status: "error", error: "boom" });
    useLibrary.setState({ facets: null, facetsError: "db closed" });
    const loadFacets = vi.spyOn(useLibrary.getState(), "loadFacets").mockResolvedValue();
    render(<FilterDrawer />);
    openPicker("Genre");
    const alert = screen.getByRole("alert");
    expect(alert.textContent).toContain("Couldn't load genres: db closed");
    fireEvent.click(within(alert).getByRole("button", { name: "Retry" }));
    expect(loadFacets).toHaveBeenCalled();
    cleanup();
    useLibrary.setState({ facetsError: null });
    render(<FilterDrawer />);
    openPicker("Genre");
    expect(screen.getByText("Loading…")).toBeTruthy();
  });

  it("a connector click switches the level to or", async () => {
    render(<FilterDrawer />);
    fireEvent.click(
      screen.getAllByRole("button", { name: "Match any of these instead" })[0] as HTMLElement,
    );
    expect(useRules.getState().root.mode).toBe("any");
    expect(screen.getAllByRole("button", { name: "Match all of these instead" })).toHaveLength(4);
  });

  describe("row menu", () => {
    async function setGenre() {
      openPicker("Genre");
      fireEvent.click(screen.getByRole("checkbox", { name: "House" }));
      await waitFor(() => expect(lastRule()).toBeDefined());
    }

    it("Pause keeps the row but leaves it out; Exclude says not", async () => {
      render(<FilterDrawer />);
      await setGenre();
      openMenu(screen.getByRole("button", { name: "Genre options" }));
      fireEvent.click(screen.getByRole("menuitemcheckbox", { name: /Pause/ }));
      expect(row("Genre").dataset.paused).toBe("true");
      await waitFor(() => expect(lastRule()).toBeUndefined());
      openMenu(screen.getByRole("button", { name: "Genre options" }));
      fireEvent.click(screen.getByRole("menuitemcheckbox", { name: /Pause/ }));
      openMenu(screen.getByRole("button", { name: "Genre options" }));
      fireEvent.click(screen.getByRole("menuitemcheckbox", { name: /Exclude/ }));
      await waitFor(() => expect(lastRule()).toMatchObject({ rules: [{ not: true }] }));
    });

    it("Or… groups the row with an empty one whose field list opens", async () => {
      render(<FilterDrawer />);
      await setGenre();
      openMenu(screen.getByRole("button", { name: "Genre options" }));
      fireEvent.click(screen.getByRole("menuitem", { name: /Or…/ }));
      // The empty row's field list is open (the rest is hidden from it while it is).
      fireEvent.click(await screen.findByRole("menuitem", { name: "Rating" }));
      const group = screen.getByRole("group", { name: "Group: any of these" });
      expect(within(group).getByRole("group", { name: "Genre filter" })).toBeTruthy();
      expect(within(group).getByRole("group", { name: "Rating filter" })).toBeTruthy();
      // Rating is used in the group: no extra empty Rating row on top.
      expect(screen.getAllByRole("group", { name: "Rating filter" })).toHaveLength(1);
      // Ungroup puts both back on the top level.
      openMenu(within(group).getByRole("button", { name: /Any of these/ }));
      fireEvent.click(screen.getByRole("menuitem", { name: "Ungroup" }));
      expect(screen.queryByRole("group", { name: /^Group:/ })).toBeNull();
    });

    it("Keep in default fields and Remove", () => {
      render(<FilterDrawer />);
      openMenu(screen.getByRole("button", { name: "Rating options" }));
      fireEvent.click(screen.getByRole("menuitemcheckbox", { name: "Keep in default fields" }));
      expect(useRules.getState().defaultFields).toEqual(["bpm", "key", "genre", "tag:1"]);
      openMenu(screen.getByRole("button", { name: "Rating options" }));
      fireEvent.click(screen.getByRole("menuitem", { name: "Remove" }));
      expect(screen.queryByRole("group", { name: "Rating filter" })).toBeNull();
    });
  });

  it("keyboard: arrows move between rows, Space pauses, Backspace empties then removes", async () => {
    render(<FilterDrawer />);
    handle("BPM").focus();
    fireEvent.keyDown(handle("BPM"), { key: "ArrowDown" });
    expect(document.activeElement).toBe(handle("Key"));
    fireEvent.keyDown(handle("Key"), { key: "ArrowDown" });
    fireEvent.keyDown(handle("Genre"), { key: "Enter" });
    fireEvent.click(screen.getByRole("checkbox", { name: "House" }));
    fireEvent.keyDown(document.activeElement as HTMLElement, { key: "Escape" });

    fireEvent.click(handle("Genre"));
    expect(row("Genre").dataset.paused).toBe("true");
    fireEvent.keyDown(handle("Genre"), { key: "Backspace" });
    expect(row("Genre").dataset.paused).toBeUndefined();
    expect(row("Genre").dataset.active).toBeUndefined();
    fireEvent.keyDown(handle("Genre"), { key: "Backspace" });
    expect(screen.queryByRole("group", { name: "Genre filter" })).toBeNull();

    fireEvent.keyDown(handle("BPM"), { key: "F10", shiftKey: true });
    expect(await screen.findByRole("menuitem", { name: /Or…/ })).toBeTruthy();
  });

  it("Add filter greys shown fields and adds a new one after the rows that filter", async () => {
    render(<FilterDrawer />);
    openPicker("Genre");
    fireEvent.click(screen.getByRole("checkbox", { name: "House" }));
    fireEvent.keyDown(document.activeElement as HTMLElement, { key: "Escape" });
    openMenu(screen.getByRole("button", { name: "Add filter" }));
    expect(screen.getByRole("menuitem", { name: /^BPM\s*shown$/ })).toBeTruthy();
    fireEvent.click(screen.getByRole("menuitem", { name: "# Situation" }));
    const order = screen.getAllByRole("group", { name: / filter$/ }).map((g) => g.ariaLabel);
    expect(order).toEqual([
      "BPM filter",
      "Key filter",
      "Genre filter",
      "Situation (My Tags) filter",
      "Rating filter",
      "Genre (My Tags) filter",
    ]);
    // Its picker opens at once.
    expect(await screen.findByRole("checkbox", { name: "Peak time" })).toBeTruthy();
  });

  it("Clear all empties the drawer with an Undo toast and keeps the playlist", async () => {
    render(<FilterDrawer />);
    openPicker("Genre");
    fireEvent.click(screen.getByRole("checkbox", { name: "House" }));
    fireEvent.click(screen.getByRole("button", { name: "Clear all" }));
    expect(row("Genre").dataset.active).toBeUndefined();
    expect(useLibrary.getState().playlistId).toBe(4);
    expect(useToasts.getState().toast?.message).toBe("Cleared all filters");
    useToasts.getState().toast?.action?.run();
    await waitFor(() => expect(row("Genre").dataset.active).toBe("true"));
  });

  it("set aside: rows stay editable under a banner that turns them back on", () => {
    useLibrary.setState({ filtersEnabled: false });
    render(<FilterDrawer />);
    openPicker("Genre");
    fireEvent.click(screen.getByRole("checkbox", { name: "House" }));
    expect(screen.getByText(/Filters are set aside\. 1 kept\./)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Turn on" }));
    expect(useLibrary.getState().filtersEnabled).toBe(true);
  });

  it("the switch turns filters off and on, keeping the rule", async () => {
    const rule: RuleNode = { type: "filter", filter: { type: "neverPlayed" } };
    useLibrary.setState({ rule });
    render(<FiltersSwitch />);
    const toggle = () => screen.getByRole("switch", { name: "Apply filters" });
    fireEvent.click(toggle());
    expect(useLibrary.getState().filtersEnabled).toBe(false);
    const sent = vi.mocked(ipc.libraryQuery).mock.lastCall?.[0];
    expect(sent?.filters).toEqual([{ type: "playlist", id: 4 }]);
    expect(sent?.rule).toBeUndefined();
    fireEvent.click(toggle());
    expect(lastRule()).toEqual(rule);
  });
});

describe("EmptyResult", () => {
  it("with no match offers Undo last change and Clear filters", () => {
    const matched = newGroup("all", [newRow({ field: "genre", genres: ["House"] })]);
    useRules.setState({ lastMatched: matched, root: freshRoot(["genre"]) });
    useLibrary.setState({
      rule: { type: "filter", filter: { type: "neverPlayed" } },
      total: 0,
      error: null,
      dividers: [],
      generation: 3,
      pagesGeneration: 3,
      text: "",
    });
    render(<EmptyResult />);
    expect(screen.getByText("No tracks match these filters")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Clear filters" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Undo last change" }));
    expect(toRule(useRules.getState().root)).toEqual(toRule(matched));
  });
});
