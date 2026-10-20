import { describe, expect, it } from "vitest";

import {
  buildFilters,
  buildQuery,
  daysAgo,
  defaultSort,
  entryPosition,
  itemAt,
  noFilters,
  PAGE_SIZE,
  type QueryInputs,
  rowAt,
  trackIndex,
  viewCount,
} from "./library";

const inputs = (patch: Partial<QueryInputs> = {}): QueryInputs => ({
  text: "",
  filters: noFilters,
  filtersEnabled: true,
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

  it("filters switched off are kept but not sent, except the playlist", () => {
    const filters = { ...noFilters, genre: "House", ratingMin: 4, playlistId: 7 };
    expect(buildQuery(inputs({ filters }), 0).filters).toHaveLength(3);
    expect(buildQuery(inputs({ filters, filtersEnabled: false }), 0).filters).toEqual([
      { type: "playlist", id: 7 },
    ]);
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
        playsMin: null,
        playsMax: null,
        lastPlayed: null,
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

  it("play filters", () => {
    const now = new Date(2026, 9, 20);
    expect(buildFilters({ ...noFilters, playsMin: 2 }, now)).toEqual([
      { type: "playCountRange", min: 2, max: 1_000_000 },
    ]);
    expect(buildFilters({ ...noFilters, playsMax: 0 }, now)).toEqual([
      { type: "playCountRange", min: 0, max: 0 },
    ]);
    expect(buildFilters({ ...noFilters, lastPlayed: "never" }, now)).toEqual([
      { type: "neverPlayed" },
    ]);
    expect(buildFilters({ ...noFilters, lastPlayed: 30 }, now)).toEqual([
      { type: "playedSince", date: "2026-09-20" },
    ]);
    expect(daysAgo(365, now)).toBe("2025-10-20");
  });
});

describe("rowAt", () => {
  it("finds rows across pages", () => {
    const row = (id: number) => ({ id }) as never;
    const pages = new Map([[1, [row(PAGE_SIZE), row(PAGE_SIZE + 1)]]]);
    const v = { pages, total: PAGE_SIZE + 2, dividers: [] };
    expect(rowAt(v, PAGE_SIZE + 1)).toEqual({ id: PAGE_SIZE + 1 });
    expect(rowAt(v, 0)).toBeUndefined();
  });
});

describe("table rows with dividers", () => {
  // Entries: D0 t0 t1 D3 t2 D5 (dividers at 0, 3 and the end).
  const row = (id: number) => ({ id, entryPosition: null }) as never;
  const div = (position: number, label: string) => ({ position, label, ascii: true });
  const v = {
    pages: new Map([[0, [row(10), row(11), row(12)]]]),
    total: 3,
    dividers: [div(0, "OPEN"), div(3, "PEAK"), div(5, "END")],
  };

  it("counts tracks and dividers", () => {
    expect(viewCount(v)).toBe(6);
  });

  it("maps each table row to its divider or track", () => {
    const kinds = Array.from({ length: 6 }, (_, i) => {
      const item = itemAt(v, i);
      return item?.kind === "divider" ? item.divider.label : item?.row.id;
    });
    expect(kinds).toEqual(["OPEN", 10, 11, "PEAK", 12, "END"]);
    expect(rowAt(v, 0)).toBeUndefined();
    expect(rowAt(v, 4)).toEqual(row(12));
  });

  it("a divider row's track index is the track after it", () => {
    expect([0, 1, 2, 3, 4, 5].map((i) => trackIndex(v, i))).toEqual([0, 0, 1, 2, 2, 3]);
  });

  it("entry positions of tracks and dividers", () => {
    expect(entryPosition({ kind: "divider", divider: div(3, "PEAK") })).toBe(3);
    expect(entryPosition({ kind: "track", row: { entryPosition: 4 } as never })).toBe(4);
  });

  it("without dividers rows are tracks", () => {
    const plain = { ...v, dividers: [] };
    expect(viewCount(plain)).toBe(3);
    expect(itemAt(plain, 2)).toEqual({ kind: "track", row: row(12) });
    expect(trackIndex(plain, 2)).toBe(2);
  });
});
