import { beforeEach, describe, expect, it, vi } from "vitest";

import type { TagTree } from "../lib/ipc";

vi.mock("../lib/ipc", () => ({
  libraryFacets: vi.fn(),
  libraryQuery: vi.fn(),
  playlistsGet: vi.fn(),
  playlistsMutate: vi.fn(),
  playlistRule: vi.fn(),
  tagsGet: vi.fn(),
}));

const ipc = await import("../lib/ipc");
const { useLibrary } = await import("./library");
const { usePanels } = await import("./panels");
const { clearAllFilters, RULE_DEBOUNCE_MS, useRules } = await import("./rules");
const { fieldKey, findRow, freshRoot, hiddenRows, toRule } = await import("./ruleTree");
const { useTags } = await import("./tags");
const { useToasts } = await import("./toasts");

const tree: TagTree = {
  categories: [
    { id: 1, name: "Genre", tags: [{ id: 10, name: "House", trackCount: 1 }] },
    { id: 2, name: "Situation", tags: [{ id: 20, name: "Peak", trackCount: 1 }] },
  ],
};

/** Field keys of the root rows that show, in order. */
function shown(): (string | null)[] {
  const { root } = useRules.getState();
  const hidden = hiddenRows(root);
  return root.rules
    .filter((i) => !hidden.has(i.id))
    .map((i) => (i.kind === "group" ? "group" : fieldKey(i.value)));
}

const rowOf = (key: string) => {
  const r = useRules
    .getState()
    .root.rules.find((i) => i.kind === "row" && fieldKey(i.value) === key);
  if (r === undefined) throw new Error(`no ${key} row`);
  return r.id;
};

beforeEach(() => {
  vi.mocked(ipc.libraryQuery).mockResolvedValue({
    rows: [],
    total: 0,
    dividers: [],
    dividerView: false,
  });
  useTags.setState({ tree: null, status: "loading", error: null });
  useLibrary.setState({ playlistId: 4, rule: null, filtersEnabled: true });
  useRules.setState({
    root: freshRoot(["bpm", "key", "genre", "rating"]),
    editing: null,
    focus: null,
    defaultFields: null,
    dismissed: [],
    lastMatched: null,
  });
  useToasts.setState({ toast: null });
  usePanels.setState({ filterBarOpen: false });
});

describe("default fields", () => {
  it("tags loading add the first category as a default row", () => {
    useTags.setState({ tree });
    expect(shown()).toEqual(["bpm", "key", "genre", "rating", "tag:1"]);
  });

  it("the user's list replaces a blank drawer; Reset brings the built-in ones back", () => {
    useTags.setState({ tree });
    useRules.getState().setDefaultFields(["plays", "tag:2"]);
    expect(shown()).toEqual(["plays", "tag:2"]);
    useRules.getState().resetDefaults();
    expect(useRules.getState().defaultFields).toBeNull();
    expect(shown()).toEqual(["bpm", "key", "genre", "rating", "tag:1"]);
  });

  it("Keep in default fields adds the row's field; unticked and removed, it stays out", () => {
    const rules = useRules.getState();
    rules.addField(rules.root.id, "plays");
    rules.keepDefault("plays", true);
    expect(useRules.getState().defaultFields).toEqual(["bpm", "key", "genre", "rating", "plays"]);
    useRules.getState().keepDefault("rating", false);
    useRules.getState().remove(rowOf("rating"));
    useRules.getState().clear();
    expect(shown()).toEqual(["bpm", "key", "genre", "plays"]);
  });

  it("a default row removed stays out of this drawer until a fresh one", () => {
    useRules.getState().remove(rowOf("key"));
    useRules.getState().updateRow(rowOf("genre"), { value: { field: "genre", genres: ["House"] } });
    expect(shown()).toEqual(["bpm", "genre", "rating"]);
    clearAllFilters();
    expect(shown()).toEqual(["bpm", "key", "genre", "rating"]);
  });
});

describe("edits reach the query", () => {
  it("re-queries after the pause, and pause leaves a row out", async () => {
    vi.useFakeTimers();
    const genre = rowOf("genre");
    useRules.getState().updateRow(genre, { value: { field: "genre", genres: ["House"] } });
    expect(useLibrary.getState().rule).toBeNull();
    vi.advanceTimersByTime(RULE_DEBOUNCE_MS);
    expect(useLibrary.getState().rule).toMatchObject({
      rules: [{ filter: { type: "genre", genre: "House" } }],
    });
    useRules.getState().updateRow(genre, { paused: true });
    vi.advanceTimersByTime(RULE_DEBOUNCE_MS);
    expect(useLibrary.getState().rule).toBeNull();
    vi.useRealTimers();
  });

  it("Or… makes an any group with an empty row and focuses it with its chooser open", () => {
    const genre = rowOf("genre");
    useRules.getState().updateRow(genre, { value: { field: "genre", genres: ["House"] } });
    useRules.getState().or(genre);
    const { root, focus } = useRules.getState();
    expect(shown()).toEqual(["bpm", "key", "group", "rating"]);
    expect(focus?.open).toBe(true);
    expect(findRow(root, focus?.id ?? "")?.value.field).toBeNull();
    useRules.getState().chooseField(focus?.id ?? "", "rating");
    // Rating is used in the group now: its empty default row hides.
    expect(shown()).toEqual(["bpm", "key", "group"]);
  });
});

describe("clearAllFilters", () => {
  it("empties the drawer, keeps the playlist; one Undo brings it back", () => {
    const genre = rowOf("genre");
    useRules.getState().updateRow(genre, { value: { field: "genre", genres: ["House"] } });
    const before = useRules.getState().root;
    clearAllFilters();
    expect(toRule(useRules.getState().root)).toBeNull();
    expect(useLibrary.getState().playlistId).toBe(4);
    useToasts.getState().toast?.action?.run();
    expect(useRules.getState().root).toBe(before);
  });
});

describe("addCondition", () => {
  it("opens the drawer and fills the field's row or adds one", () => {
    useTags.setState({ tree });
    useRules.getState().addCondition({ field: "tag", category: 2, ids: [20], all: false }, false);
    expect(usePanels.getState().filterBarOpen).toBe(true);
    expect(shown()).toEqual(["tag:2", "bpm", "key", "genre", "rating", "tag:1"]);
    useRules.getState().addCondition({ field: "tag", category: 1, ids: [10], all: false }, false);
    expect(findRow(useRules.getState().root, rowOf("tag:1"))?.value).toMatchObject({
      ids: [10],
    });
    useRules.getState().addCondition({ field: "playlist", ids: [2] }, true);
    expect(shown()).toEqual(["tag:2", "bpm", "key", "genre", "rating", "tag:1", "playlist"]);
  });
});

describe("undoLastChange", () => {
  it("goes back to the last draft whose result had tracks", async () => {
    const genre = rowOf("genre");
    useRules.getState().updateRow(genre, { value: { field: "genre", genres: ["House"] } });
    const matched = useRules.getState().root;
    useLibrary.setState({ rule: toRule(matched) });
    // A result with tracks lands for that draft.
    useLibrary.setState((s) => ({ total: 12, generation: s.generation + 1 }));
    useLibrary.setState((s) => ({ pagesGeneration: s.generation }));
    expect(useRules.getState().lastMatched).toBe(matched);

    useRules.getState().updateRow(genre, { value: { field: "genre", genres: ["Polka"] } });
    useRules.getState().undoLastChange();
    expect(useRules.getState().root).toBe(matched);
  });
});
