import { beforeEach, describe, expect, it, vi } from "vitest";

import type { FilterPreset, RuleNode } from "../lib/ipc";

vi.mock("../lib/ipc", () => ({
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
const { useFilterPresets } = await import("./filterPresets");
const { useLibrary } = await import("./library");
const { clearAllFilters, presetEdited, useRules } = await import("./rules");
const { fieldKey, freshRoot, newGroup, newRow, suggestName, toRule } = await import("./ruleTree");
const { useToasts } = await import("./toasts");

const house: RuleNode = {
  type: "filter",
  not: false,
  filter: { type: "genre", genre: "House" },
};
const preset = (id: number, name: string, rule: RuleNode | null = house): FilterPreset => ({
  id,
  name,
  rule,
  createdAt: "2026-10-23T00:00:00Z",
  updatedAt: "2026-10-23T00:00:00Z",
});

const genreRow = () => {
  const r = useRules
    .getState()
    .root.rules.find((i) => i.kind === "row" && fieldKey(i.value) === "genre");
  if (r === undefined || r.kind !== "row") throw new Error("no genre row");
  return r;
};

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(ipc.libraryQuery).mockResolvedValue({
    rows: [],
    total: 0,
    dividers: [],
    dividerView: false,
  });
  vi.mocked(ipc.filterPresetsGet).mockResolvedValue([preset(1, "House")]);
  vi.mocked(ipc.filterPresetsMutate).mockResolvedValue({ createdId: 7 });
  useFilterPresets.setState({ presets: [preset(1, "House")], status: "ready", error: null });
  useLibrary.setState({ playlistId: null, rule: null, filtersEnabled: false });
  useRules.setState({
    root: freshRoot(["bpm", "key", "genre", "rating"]),
    editing: null,
    focus: null,
    defaultFields: null,
    dismissed: [],
    lastMatched: null,
    preset: null,
    saveTarget: "filter",
  });
  useToasts.setState({ toast: null });
});

describe("saved filters in the drawer", () => {
  it("loading one fills the drawer, turns filters on and is not edited", () => {
    useRules.getState().loadPreset(preset(1, "House"));
    const s = useRules.getState();
    expect(genreRow().value).toEqual({ field: "genre", genres: ["House"] });
    expect(s.preset).toMatchObject({ id: 1, name: "House" });
    expect(presetEdited(s)).toBe(false);
    expect(useLibrary.getState().filtersEnabled).toBe(true);
    // The drawer was blank: nothing to undo.
    expect(useToasts.getState().toast).toBeNull();
  });

  it("loading over edits offers Undo, which brings them and the old preset back", () => {
    const techno = newGroup("all", [newRow({ field: "bpm", min: 124, max: 130 })]);
    useRules.getState().restore(techno);
    const before = toRule(useRules.getState().root);
    useRules.getState().loadPreset(preset(1, "House"));
    expect(useToasts.getState().toast?.message).toBe("Loaded “House”");
    useToasts.getState().toast?.action?.run();
    expect(toRule(useRules.getState().root)).toEqual(before);
    expect(useRules.getState().preset).toBeNull();
  });

  it("an edit marks it edited; Revert puts it back, Update saves it", async () => {
    useRules.getState().loadPreset(preset(1, "House"));
    const id = genreRow().id;
    useRules.getState().updateRow(id, { value: { field: "genre", genres: ["Techno"] } });
    expect(presetEdited(useRules.getState())).toBe(true);
    useRules.getState().revertPreset();
    expect(genreRow().value).toEqual({ field: "genre", genres: ["House"] });
    expect(presetEdited(useRules.getState())).toBe(false);

    useRules.getState().updateRow(genreRow().id, { value: { field: "genre", genres: ["Techno"] } });
    expect(await useRules.getState().updatePreset()).toBe(true);
    expect(ipc.filterPresetsMutate).toHaveBeenCalledWith({
      type: "setRule",
      id: 1,
      rule: toRule(useRules.getState().root),
    });
    expect(presetEdited(useRules.getState())).toBe(false);
  });

  it("saves a new one or over an existing one, and it stays loaded", async () => {
    useRules.getState().updateRow(genreRow().id, { value: { field: "genre", genres: ["Disco"] } });
    expect(await useRules.getState().savePreset("  Disco  ")).toBe(true);
    const rule = toRule(useRules.getState().root);
    expect(ipc.filterPresetsMutate).toHaveBeenCalledWith({ type: "create", name: "Disco", rule });
    expect(useRules.getState().preset).toEqual({ id: 7, name: "Disco", baseline: rule });

    expect(await useRules.getState().savePreset("house", preset(1, "House"))).toBe(true);
    expect(ipc.filterPresetsMutate).toHaveBeenLastCalledWith({ type: "setRule", id: 1, rule });
    expect(useRules.getState().preset?.name).toBe("House");
  });

  it("a failed save says so and keeps nothing loaded", async () => {
    vi.mocked(ipc.filterPresetsMutate).mockRejectedValue(new Error("name taken"));
    useRules.getState().updateRow(genreRow().id, { value: { field: "genre", genres: ["Disco"] } });
    expect(await useRules.getState().savePreset("Disco")).toBe(false);
    expect(useRules.getState().preset).toBeNull();
    expect(useToasts.getState().toast?.message).toMatch(/name taken/);
  });

  it("Clear all unloads it, and its Undo loads it again", () => {
    useRules.getState().loadPreset(preset(1, "House"));
    clearAllFilters();
    expect(useRules.getState().preset).toBeNull();
    useToasts.getState().toast?.action?.run();
    expect(useRules.getState().preset?.name).toBe("House");
  });

  it("follows a rename and lets go when it is deleted", () => {
    useRules.getState().loadPreset(preset(1, "House"));
    useFilterPresets.setState({ presets: [preset(1, "Deep house")] });
    expect(useRules.getState().preset?.name).toBe("Deep house");
    useFilterPresets.setState({ presets: [] });
    expect(useRules.getState().preset).toBeNull();
    // The rows stay.
    expect(genreRow().value).toEqual({ field: "genre", genres: ["House"] });
  });

  it("refuses one whose rule can't be read", () => {
    useRules.getState().loadPreset(preset(2, "Future", null));
    expect(useRules.getState().preset).toBeNull();
    expect(useToasts.getState().toast?.message).toMatch(/newer reks/);
  });
});

describe("suggestName", () => {
  const names = {
    tag: (id: number) => (id === 20 ? "Peak time" : `tag ${id}`),
    category: () => "Situation",
    playlist: () => "Warm-up",
    key: (k: string) => k,
  };

  it("joins the rows that filter, in order, and fits them", () => {
    const root = newGroup("all", [
      newRow({ field: "lastPlayed", mode: "never", date: "" }),
      newRow({ field: "bpm", min: 124, max: 130 }),
      newRow({ field: "genre", genres: [] }),
      newGroup("any", [newRow({ field: "tag", category: 2, ids: [20], all: false })]),
      newRow({ field: "genre", genres: ["Tech House", "Minimal", "Deep Techno"] }),
    ]);
    expect(suggestName(root, names)).toBe("Unplayed, 124–130 BPM, Peak time");
  });

  it("falls back when nothing filters", () => {
    expect(suggestName(freshRoot(["genre"]), names)).toBe("My filter");
  });
});
