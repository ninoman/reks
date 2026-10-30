import { describe, expect, it } from "vitest";

import type { RuleNode } from "../lib/ipc";
import {
  buildQuery,
  defaultSort,
  entryPosition,
  itemAt,
  PAGE_SIZE,
  type QueryInputs,
  rowAt,
  trackIndex,
  trackPages,
  viewCount,
} from "./library";

const inputs = (patch: Partial<QueryInputs> = {}): QueryInputs => ({
  text: "",
  playlistId: null,
  filtersEnabled: true,
  rule: null,
  sort: defaultSort,
  ...patch,
});

describe("buildQuery", () => {
  it("defaults: no text, no filters, first page", () => {
    expect(buildQuery(inputs(), 0)).toEqual({
      text: null,
      filters: [],
      sort: { column: "dateAdded", descending: true },
      offset: 0,
      limit: PAGE_SIZE,
    });
  });

  it("sends the rule with the shown playlist, and the rule only while filters are on", () => {
    const rule: RuleNode = {
      type: "group",
      match: "all",
      not: false,
      rules: [
        { type: "filter", not: false, filter: { type: "tagAny", ids: [4, 9] } },
        { type: "filter", not: true, filter: { type: "playlist", id: 2 } },
      ],
    };
    const on = buildQuery(inputs({ rule, playlistId: 7 }), 0);
    expect(on.rule).toEqual(rule);
    expect(on.filters).toEqual([{ type: "playlist", id: 7 }]);
    const off = buildQuery(inputs({ rule, playlistId: 7, filtersEnabled: false }), 0);
    expect("rule" in off).toBe(false);
    expect(off.filters).toEqual([{ type: "playlist", id: 7 }]);
    expect("rule" in buildQuery(inputs(), 0)).toBe(false);
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

describe("trackPages", () => {
  const view = (dividers: { position: number }[]) => ({
    pages: new Map(),
    total: PAGE_SIZE * 3,
    dividers: dividers.map((d) => ({ ...d, label: "", ascii: true })),
  });

  it("maps table rows to the distinct pages of their tracks", () => {
    expect(trackPages(view([]), [0, 1, PAGE_SIZE, PAGE_SIZE * 2 + 7, 5])).toEqual([0, 1, 2]);
  });

  it("skips dividers and shifts rows after them", () => {
    // A divider at table row 0 (position 0): row 0 has no track, row PAGE_SIZE is track PAGE_SIZE-1.
    const v = view([{ position: 0 }]);
    expect(trackPages(v, [0])).toEqual([]);
    expect(trackPages(v, [PAGE_SIZE])).toEqual([0]);
    expect(trackPages(v, [PAGE_SIZE + 1])).toEqual([1]);
  });
});
