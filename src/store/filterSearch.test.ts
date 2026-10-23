import { describe, expect, it } from "vitest";

import { type SearchSource, searchFilters } from "./filterSearch";

const src: SearchSource = {
  genres: ["House", "Tech House", "Techno", "Deep Tech"],
  keys: [
    { key: "Am", camelot: "8A", openKey: "1m" },
    { key: "C", camelot: "8B", openKey: "1d" },
  ],
  tags: {
    categories: [
      { id: 1, name: "Components", tags: [{ id: 10, name: "Techy", trackCount: 3 }] },
      { id: 2, name: "Situation", tags: [{ id: 20, name: "Peak time", trackCount: 1 }] },
    ],
  },
  keyLabel: (k) => (k === "Am" ? "8A" : k),
};

const labels = (q: string) =>
  searchFilters(q, src).values.map((v) => `${v.field} ${v.verb} ${v.label}`);

describe("searchFilters", () => {
  it("finds nothing for a blank query", () => {
    expect(searchFilters("  ", src)).toEqual({ values: [], fields: [] });
  });

  it("lists genres starting with the query first, then tags with their category", () => {
    expect(labels("tech")).toEqual([
      "Genre is Tech House",
      "Genre is Techno",
      "Genre is Deep Tech",
      "# Components has Techy",
    ]);
    const tag = searchFilters("techy", src).values[0];
    expect(tag?.value).toEqual({ field: "tag", category: 1, ids: [10], all: false });
  });

  it("reads a number as a BPM range around it", () => {
    const [hit] = searchFilters("128", src).values;
    expect(hit?.value).toEqual({ field: "bpm", min: 126, max: 130 });
    expect(hit?.detail).toBe("126–130");
    expect(searchFilters("12", src).values).toEqual([]);
    expect(searchFilters("999", src).values).toEqual([]);
  });

  it("reads star ratings as at least", () => {
    for (const q of ["4*", "4 stars", "4★", "★★★★"]) {
      expect(searchFilters(q, src).values[0]?.value).toEqual({
        field: "rating",
        op: "atLeast",
        stars: 4,
      });
    }
    expect(searchFilters("6*", src).values).toEqual([]);
  });

  it("matches a key exactly in any notation and shows it in the user's", () => {
    for (const q of ["8a", "am", "1M"]) {
      expect(labels(q)).toEqual(["Key is 8A"]);
    }
    expect(searchFilters("8", src).values).toEqual([]);
  });

  it("matches fields by name, tag categories with their mark", () => {
    expect(searchFilters("situ", src).fields).toEqual([{ key: "tag:2", label: "# Situation" }]);
    expect(searchFilters("play", src).fields.map((f) => f.key)).toEqual([
      "playlist",
      "plays",
      "lastPlayed",
    ]);
  });
});
