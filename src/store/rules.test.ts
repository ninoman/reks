import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../lib/ipc", () => ({
  libraryFacets: vi.fn(),
  libraryQuery: vi.fn(),
  playlistsGet: vi.fn(),
  playlistsMutate: vi.fn(),
  playlistRule: vi.fn(),
}));

const ipc = await import("../lib/ipc");
const { noFilters, useLibrary } = await import("./library");
const { clearAllFilters, useRules } = await import("./rules");
const { emptyRoot, newGroup, newRow, toRule } = await import("./ruleTree");
const { useToasts } = await import("./toasts");

beforeEach(() => {
  vi.mocked(ipc.libraryQuery).mockResolvedValue({
    rows: [],
    total: 0,
    dividers: [],
    dividerView: false,
  });
  useLibrary.setState({ filters: { ...noFilters, playlistId: 4, genre: "House" }, rule: null });
  useRules.setState({ root: emptyRoot(), open: false, editing: null, focusId: null });
  useToasts.setState({ toast: null });
});

describe("clearAllFilters", () => {
  it("clears the bar and the builder together; one Undo brings both back", () => {
    const root = newGroup("all", [newRow({ field: "genre", genre: "Techno" })]);
    useRules.setState({ root });
    useLibrary.setState({ rule: toRule(root) });

    clearAllFilters();
    expect(useLibrary.getState().filters).toEqual({ ...noFilters, playlistId: 4 });
    expect(useLibrary.getState().rule).toBeNull();
    expect(toRule(useRules.getState().root)).toBeNull();

    const toast = useToasts.getState().toast;
    expect(toast?.action?.label).toBe("Undo");
    toast?.action?.run();
    expect(useLibrary.getState().filters.genre).toBe("House");
    expect(useLibrary.getState().filters.playlistId).toBe(4);
    expect(useRules.getState().root).toBe(root);
    expect(useLibrary.getState().rule).toEqual(toRule(root));
  });
});

describe("addCondition", () => {
  it("replaces an untouched empty row, opens the builder, and appends after real rows", () => {
    useRules.getState().addCondition({ field: "tag", ids: [3] }, false);
    expect(useRules.getState().open).toBe(true);
    expect(useRules.getState().root.rules).toHaveLength(1);
    useRules.getState().addCondition({ field: "playlist", id: 2 }, true);
    expect(useRules.getState().root.rules).toHaveLength(2);
  });
});
