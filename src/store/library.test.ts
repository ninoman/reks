import { describe, expect, it } from "vitest";

import {
  buildFilters,
  buildQuery,
  defaultSort,
  noFilters,
  PAGE_SIZE,
  rowAt,
  type QueryInputs,
} from "./library";

const inputs = (patch: Partial<QueryInputs> = {}): QueryInputs => ({
  text: "",
  filters: noFilters,
  sort: defaultSort,
  ...patch,
});

describe("buildQuery", () => {
  it("defaults: no text, no filters, first page", () => {
    expect(buildQuery(inputs(), 0)).toEqual({
      text: null,
      filters: [],
      sort: { column: "title", descending: false },
      offset: 0,
      limit: PAGE_SIZE,
    });
  });

  it("trims text; blank text means no search", () => {
    expect(buildQuery(inputs({ text: "  deep night " }), 0).text).toBe("deep night");
    expect(buildQuery(inputs({ text: "   " }), 0).text).toBeNull();
  });

  it("pages by PAGE_SIZE", () => {
    const q = buildQuery(inputs(), 3);
    expect(q.offset).toBe(3 * PAGE_SIZE);
    expect(q.limit).toBe(PAGE_SIZE);
  });

  it("passes the sort through", () => {
    const sort = { column: "bpm", descending: true } as const;
    expect(buildQuery(inputs({ sort }), 0).sort).toEqual(sort);
  });
});

describe("buildFilters", () => {
  it("maps every sidebar filter", () => {
    expect(
      buildFilters({
        genre: "House",
        keys: ["Am", "F#m"],
        bpmMin: 120,
        bpmMax: 128.5,
        color: "red",
        ratingMin: 3,
        playlistId: 11,
        missing: false,
        needsAnalysis: true,
      }),
    ).toEqual([
      { type: "genre", genre: "House" },
      { type: "keyIn", keys: ["Am", "F#m"] },
      { type: "bpmRange", min: 12000, max: 12850 },
      { type: "color", color: "red" },
      { type: "ratingMin", rating: 3 },
      { type: "playlist", id: 11 },
      { type: "missing", missing: false },
      { type: "needsAnalysis", needs: true },
    ]);
  });

  it("leaves out unset filters", () => {
    expect(buildFilters(noFilters)).toEqual([]);
    expect(buildFilters({ ...noFilters, ratingMin: 0, keys: [] })).toEqual([]);
  });

  it("open-ended BPM ranges", () => {
    expect(buildFilters({ ...noFilters, bpmMin: 124 })).toEqual([
      { type: "bpmRange", min: 12400, max: 100000 },
    ]);
    expect(buildFilters({ ...noFilters, bpmMax: 90 })).toEqual([
      { type: "bpmRange", min: 0, max: 9000 },
    ]);
  });
});

describe("rowAt", () => {
  it("finds rows across pages", () => {
    const row = (id: number) => ({ id }) as never;
    const pages = new Map([[1, [row(PAGE_SIZE), row(PAGE_SIZE + 1)]]]);
    expect(rowAt(pages, PAGE_SIZE + 1)).toEqual({ id: PAGE_SIZE + 1 });
    expect(rowAt(pages, 0)).toBeUndefined();
  });
});
