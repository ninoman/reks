import { describe, expect, it } from "vitest";

import type { RuleNode } from "../lib/ipc";
import {
  activeRowCount,
  addField,
  addValue,
  canOr,
  clearRow,
  defaultKeys,
  emptyValue,
  ensureDefaults,
  type FieldKey,
  fieldKey,
  fillRow,
  findGroup,
  findRow,
  freshRoot,
  fromRule,
  type GroupDraft,
  hiddenRows,
  type ItemDraft,
  isBlank,
  newGroup,
  newRow,
  orRow,
  pruneTags,
  type RowDraft,
  type RowValue,
  removeItem,
  shownKeys,
  summarize,
  toggleMode,
  toRule,
  ungroup,
  updateRow,
  visibleRowIds,
} from "./ruleTree";

const DEFAULTS: FieldKey[] = ["bpm", "key", "genre", "rating", "tag:1"];

/** Tag 10, 11 in category 1 (Genre); 20, 21 in category 2 (Situation). */
const categoryOf = (id: number) => ({ 10: 1, 11: 1, 20: 2, 21: 2 })[id];

const names = {
  tag: (id: number) => ({ 10: "House", 11: "Techno", 20: "Peak", 21: "Warm-up" })[id] ?? `t${id}`,
  category: (id: number) => ({ 1: "Genre", 2: "Situation" })[id] ?? `c${id}`,
  playlist: (id: number) => ({ 7: "Closing" })[id] ?? `p${id}`,
  key: (k: string) => k,
};

const row = (value: RowValue, not = false): RowDraft => newRow(value, not);
const genre = (...genres: string[]) => row({ field: "genre", genres });
const tags = (category: number, ids: number[], all = false) =>
  row({ field: "tag", category, ids, all });

/** The rule of a root holding just `items`, unwrapped to its one rule. */
function only(...items: ItemDraft[]): RuleNode | undefined {
  const rule = toRule(newGroup("all", items));
  return rule?.type === "group" ? rule.rules?.[0] : undefined;
}

/** Field keys of the visible root rows, in order. */
function shownOrder(root: GroupDraft): (FieldKey | "group" | null)[] {
  const hidden = hiddenRows(root);
  return root.rules
    .filter((i) => !hidden.has(i.id))
    .map((i) => (i.kind === "group" ? "group" : fieldKey(i.value)));
}

describe("toRule", () => {
  it("leaves empty, unfinished and paused rows out; nothing set is null", () => {
    expect(toRule(freshRoot(DEFAULTS))).toBeNull();
    const paused = { ...genre("House"), paused: true };
    expect(toRule(newGroup("all", [paused, row({ field: null })]))).toBeNull();
    expect(toRule(newGroup("all", [paused, genre("Techno")]))).toEqual({
      type: "group",
      match: "all",
      not: false,
      rules: [{ type: "filter", not: false, filter: { type: "genre", genre: "Techno" } }],
    });
  });

  it("lists: one value is a filter, more are an any group; None of is not", () => {
    expect(only(genre("House"))).toEqual({
      type: "filter",
      not: false,
      filter: { type: "genre", genre: "House" },
    });
    expect(only(row({ field: "color", colours: ["red", "blue"] }, true))).toEqual({
      type: "group",
      match: "any",
      not: true,
      rules: [
        { type: "filter", not: false, filter: { type: "color", color: "red" } },
        { type: "filter", not: false, filter: { type: "color", color: "blue" } },
      ],
    });
    expect(only(row({ field: "key", keys: ["Am", "C"] }, true))).toEqual({
      type: "filter",
      not: true,
      filter: { type: "keyIn", keys: ["Am", "C"] },
    });
  });

  it("tag categories: any is tagAny, all is an all group of single tags, none is not", () => {
    expect(only(tags(2, [20, 21]))).toEqual({
      type: "filter",
      not: false,
      filter: { type: "tagAny", ids: [20, 21] },
    });
    expect(only(tags(2, [20, 21], true))).toEqual({
      type: "group",
      match: "all",
      not: false,
      rules: [
        { type: "filter", not: false, filter: { type: "tagAny", ids: [20] } },
        { type: "filter", not: false, filter: { type: "tagAny", ids: [21] } },
      ],
    });
    expect(only({ ...tags(2, [21]), not: true })).toEqual({
      type: "filter",
      not: true,
      filter: { type: "tagAny", ids: [21] },
    });
  });

  it("rating: at least, exactly and at most", () => {
    const rating = (op: "atLeast" | "exactly" | "atMost", stars: number, not = false) =>
      only(row({ field: "rating", op, stars }, not));
    expect(rating("atLeast", 4)).toEqual({
      type: "filter",
      not: false,
      filter: { type: "ratingMin", rating: 4 },
    });
    expect(rating("atMost", 2)).toEqual({
      type: "filter",
      not: true,
      filter: { type: "ratingMin", rating: 3 },
    });
    expect(rating("exactly", 3)).toMatchObject({
      type: "group",
      match: "all",
      rules: [
        { not: false, filter: { rating: 3 } },
        { not: true, filter: { rating: 4 } },
      ],
    });
    expect(rating("exactly", 5)).toMatchObject({ type: "filter", filter: { rating: 5 } });
    // At most 5★ is every track: nothing to filter.
    expect(rating("atMost", 5)).toBeUndefined();
    expect(rating("atLeast", 0)).toBeUndefined();
  });

  it("ranges with one side open, last played never / since, file", () => {
    expect(only(row({ field: "bpm", min: 124, max: null }))).toMatchObject({
      filter: { type: "bpmRange", min: 12400, max: 100000 },
    });
    expect(only(row({ field: "plays", min: null, max: 0 }))).toMatchObject({
      filter: { type: "playCountRange", min: 0, max: 0 },
    });
    expect(only(row({ field: "lastPlayed", mode: "never", date: "" }))).toMatchObject({
      filter: { type: "neverPlayed" },
    });
    expect(only(row({ field: "lastPlayed", mode: "since", date: "2026-0" }))).toBeUndefined();
    expect(only(row({ field: "lastPlayed", mode: "since", date: "2026-01-02" }, true))).toEqual({
      type: "filter",
      not: true,
      filter: { type: "playedSince", date: "2026-01-02" },
    });
    expect(only(row({ field: "missing", missing: null }))).toBeUndefined();
    expect(only(row({ field: "missing", missing: true }))).toMatchObject({
      filter: { type: "missing", missing: true },
    });
  });

  it("writes groups by mode and leaves out a group with nothing that filters", () => {
    const root = newGroup("any", [
      newGroup("none", [genre("House"), row({ field: null })]),
      newGroup("all", [row({ field: null })]),
    ]);
    expect(toRule(root)).toEqual({
      type: "group",
      match: "any",
      not: false,
      rules: [
        {
          type: "group",
          match: "any",
          not: true,
          rules: [{ type: "filter", not: false, filter: { type: "genre", genre: "House" } }],
        },
      ],
    });
  });

  it("the badge counts rows that filter, not paused or empty ones", () => {
    const root = newGroup("all", [
      genre("House", "Techno"),
      { ...tags(2, [20]), paused: true },
      newGroup("any", [tags(2, [21]), row({ field: "rating", op: "atLeast", stars: 0 })]),
    ]);
    expect(activeRowCount(root)).toBe(2);
    expect(isBlank(freshRoot(DEFAULTS))).toBe(true);
    expect(isBlank(root)).toBe(false);
  });
});

describe("fromRule", () => {
  /** draft → rule → draft → rule gives the same rule. */
  function roundTrips(items: ItemDraft[], mode: "all" | "any" = "all") {
    const rule = toRule(newGroup(mode, items));
    expect(rule).not.toBeNull();
    const back = fromRule(rule as RuleNode, categoryOf);
    expect(toRule(back)).toEqual(rule);
    return back;
  }

  it("reads back what the drawer writes as the same rows", () => {
    const back = roundTrips([
      genre("House", "Techno"),
      { ...row({ field: "color", colours: ["red"] }), not: true },
      tags(1, [10, 11], true),
      { ...tags(2, [21]), not: true },
      row({ field: "rating", op: "exactly", stars: 3 }),
      { ...row({ field: "rating", op: "exactly", stars: 2 }), not: true },
      row({ field: "rating", op: "atMost", stars: 2 }),
      row({ field: "bpm", min: 120, max: null }),
      row({ field: "lastPlayed", mode: "since", date: "2026-01-02" }),
    ]);
    expect(back.rules.map((i) => (i.kind === "row" ? i.value.field : "group"))).toEqual([
      "genre",
      "color",
      "tag",
      "tag",
      "rating",
      "rating",
      "rating",
      "bpm",
      "lastPlayed",
    ]);
    expect(back.rules[2]).toMatchObject({ value: { category: 1, ids: [10, 11], all: true } });
    expect(back.rules[5]).toMatchObject({ not: true, value: { op: "exactly", stars: 2 } });
    expect(back.rules[6]).toMatchObject({ not: false, value: { op: "atMost", stars: 2 } });
  });

  it("keeps groups and an any root", () => {
    const back = roundTrips(
      [genre("House"), newGroup("none", [tags(2, [20]), row({ field: "key", keys: ["Am"] })])],
      "any",
    );
    expect(back.mode).toBe("any");
    expect(back.rules[1]).toMatchObject({ kind: "group", mode: "none" });
  });

  it("splits a tagAny over several categories into an any group of category rows", () => {
    const rule: RuleNode = { type: "filter", not: true, filter: { type: "tagAny", ids: [10, 20] } };
    const back = fromRule(rule, categoryOf);
    expect(back.rules[0]).toMatchObject({
      kind: "group",
      mode: "none",
      rules: [
        { value: { field: "tag", category: 1, ids: [10] } },
        { value: { field: "tag", category: 2, ids: [20] } },
      ],
    });
    expect(toRule(back)).toMatchObject({ rules: [{ match: "any", not: true }] });
  });

  it("keeps tags of unknown categories in a row of their own", () => {
    const back = fromRule(
      { type: "filter", filter: { type: "tagAny", ids: [99] } },
      () => undefined,
    );
    expect(back.rules[0]).toMatchObject({ value: { category: null, ids: [99] } });
    expect(fieldKey((back.rules[0] as RowDraft).value)).toBeNull();
  });

  it("rewrites a negated all group as an any group of negated rows", () => {
    const back = fromRule(
      {
        type: "group",
        match: "all",
        not: true,
        rules: [
          { type: "filter", filter: { type: "playlist", id: 7 } },
          { type: "filter", filter: { type: "neverPlayed" } },
        ],
      },
      categoryOf,
    );
    expect(back.mode).toBe("any");
    expect(back.rules.map((r) => (r as RowDraft).not)).toEqual([true, true]);
  });
});

describe("default fields", () => {
  it("built in: BPM, Key, Genre, Rating and the first tag category once tags load", () => {
    expect(defaultKeys(null, null)).toEqual(["bpm", "key", "genre", "rating"]);
    expect(defaultKeys(null, [3, 4])).toEqual(["bpm", "key", "genre", "rating", "tag:3"]);
  });

  it("the user's list, without unknown fields or deleted categories", () => {
    expect(defaultKeys(["plays", "tag:4", "nope", "tag:9", "toString"], [3, 4])).toEqual([
      "plays",
      "tag:4",
    ]);
    expect(defaultKeys(["tag:4"], null)).toEqual([]);
  });

  it("a fresh drawer has an empty row per default; missing ones are added at the end", () => {
    const fresh = freshRoot(DEFAULTS);
    expect(shownOrder(fresh)).toEqual(DEFAULTS);
    const lost = removeItem(fresh, (fresh.rules[1] as RowDraft).id);
    expect(shownOrder(ensureDefaults(lost, DEFAULTS))).toEqual([
      "bpm",
      "genre",
      "rating",
      "tag:1",
      "key",
    ]);
    expect(ensureDefaults(fresh, DEFAULTS)).toBe(fresh);
  });

  it("a default used elsewhere shows no extra empty row, and comes back when that use goes", () => {
    const fresh = freshRoot(DEFAULTS);
    const ratingRow = fresh.rules[3] as RowDraft;
    const set = updateRow(fresh, ratingRow.id, {
      value: { field: "rating", op: "atLeast", stars: 4 },
    });
    const made = orRow(set, ratingRow.id);
    if (made === null) throw new Error("no Or…");
    // The rating row moved into a group; the put-back default row hides.
    const root = ensureDefaults(made.root, DEFAULTS);
    expect(shownOrder(root)).toEqual(["bpm", "key", "genre", "group", "tag:1"]);
    const group = root.rules.find((i) => i.kind === "group") as GroupDraft;
    expect(shownOrder(removeItem(root, group.id))).toEqual([
      "bpm",
      "key",
      "genre",
      "tag:1",
      "rating",
    ]);
  });
});

describe("order", () => {
  const set = (root: GroupDraft, key: FieldKey, value: RowValue) => {
    const r = root.rules.find((i) => i.kind === "row" && fieldKey(i.value) === key) as RowDraft;
    return updateRow(root, r.id, { value });
  };

  it("an added row goes after the last row that filters, above the unused defaults", () => {
    let root = freshRoot(DEFAULTS);
    root = set(root, "bpm", { field: "bpm", min: 124, max: 130 });
    root = set(root, "key", { field: "key", keys: ["Am"] });
    root = addField(root, root.id, "tag:2").root;
    root = addField(root, root.id, "tag:3").root;
    expect(shownOrder(root)).toEqual(["bpm", "key", "tag:2", "tag:3", "genre", "rating", "tag:1"]);
  });

  it("default rows keep their place when set; nothing set means added rows go first", () => {
    let root = freshRoot(DEFAULTS);
    root = set(root, "rating", { field: "rating", op: "atLeast", stars: 4 });
    expect(shownOrder(root)).toEqual(DEFAULTS);
    root = addField(root, root.id, "plays").root;
    expect(shownOrder(root)).toEqual(["bpm", "key", "genre", "rating", "plays", "tag:1"]);
    expect(shownOrder(addField(freshRoot(DEFAULTS), freshRoot([]).id, "plays").root)).toEqual(
      DEFAULTS,
    );
  });

  it("a field shows once per level: picking a shown one jumps to its row", () => {
    const root = freshRoot(DEFAULTS);
    const { root: same, rowId } = addField(root, root.id, "genre");
    expect(same).toBe(root);
    expect(rowId).toBe(root.rules[2]?.id);
    expect(shownKeys(root, root)).toEqual(new Set(DEFAULTS));
  });

  it("a hidden default comes back as an added row when picked again", () => {
    const root = newGroup("all", [
      newGroup("any", [genre("House"), row({ field: "plays", min: 2, max: null })]),
      { ...newRow(emptyValue("genre")), placeholder: true },
    ]);
    expect(shownOrder(root)).toEqual(["group"]);
    const added = addField(root, root.id, "genre");
    expect(shownOrder(added.root)).toEqual(["group", "genre"]);
    expect(added.root.rules).toHaveLength(2);
  });

  it("inside a group every field is offered and a row goes last", () => {
    const g = newGroup("any", [genre("House")]);
    const root = newGroup("all", [g, ...freshRoot(DEFAULTS).rules]);
    const { root: next } = addField(root, g.id, "genre");
    expect(
      (findGroup(next, g.id) as GroupDraft).rules.map((r) => fieldKey((r as RowDraft).value)),
    ).toEqual(["genre", "genre"]);
  });
});

describe("Or…, groups and connectors", () => {
  it("Or… wraps the row in an any group with an empty row below it", () => {
    const house = genre("House");
    const root = newGroup("all", [house]);
    const made = orRow(root, house.id);
    expect(made?.root.rules[0]).toMatchObject({
      kind: "group",
      mode: "any",
      rules: [{ id: house.id }, { id: made?.rowId, value: { field: null } }],
    });
  });

  it("Or… in a group that reads or adds a sibling; at the deepest level it can't wrap", () => {
    const a = genre("House");
    const b = genre("Techno");
    const inner = newGroup("all", [b]);
    const root = newGroup("all", [newGroup("any", [a, inner])]);
    const made = orRow(root, a.id);
    expect(made?.root.rules[0]).toMatchObject({ kind: "group", rules: [{}, {}, {}] });
    expect(canOr(root, b.id)).toBe(false);
    expect(orRow(root, b.id)).toBeNull();
  });

  it("a connector click switches the level; Ungroup puts the items back", () => {
    const g = newGroup("any", [genre("House"), genre("Techno")]);
    const root = newGroup("all", [g]);
    expect(toggleMode(root, root.id).mode).toBe("any");
    expect(toggleMode(toggleMode(root, root.id), root.id).mode).toBe("all");
    expect((findGroup(toggleMode(root, g.id), g.id) as GroupDraft).mode).toBe("all");
    expect(ungroup(root, g.id).rules.map((r) => r.id)).toEqual(g.rules.map((r) => r.id));
  });

  it("visible rows in reading order skip hidden defaults", () => {
    const g = newGroup("any", [genre("House")]);
    const placeholder = { ...newRow(emptyValue("genre")), placeholder: true };
    const bpm = row({ field: "bpm", min: null, max: null });
    const root = newGroup("all", [bpm, g, placeholder]);
    expect(visibleRowIds(root)).toEqual([bpm.id, g.rules[0]?.id]);
  });
});

describe("row edits", () => {
  it("clearRow empties the value and Exclude / Pause, keeping the field", () => {
    const r = { ...tags(2, [20], true), not: true, paused: true };
    const root = clearRow(newGroup("all", [r]), r.id);
    expect(findRow(root, r.id)).toMatchObject({
      not: false,
      paused: false,
      value: { field: "tag", category: 2, ids: [], all: false },
    });
  });

  it("quick paths fill the field's row, add to it, or add the row", () => {
    let root = freshRoot(DEFAULTS);
    const genreRow = root.rules[2] as RowDraft;
    root = fillRow(root, { field: "genre", genres: ["House"] }, false).root;
    root = fillRow(root, { field: "genre", genres: ["Techno"] }, false).root;
    expect(findRow(root, genreRow.id)?.value).toEqual({
      field: "genre",
      genres: ["House", "Techno"],
    });
    // The other way round replaces the values.
    root = fillRow(root, { field: "genre", genres: ["Disco"] }, true).root;
    expect(findRow(root, genreRow.id)).toMatchObject({ not: true, value: { genres: ["Disco"] } });
    const added = fillRow(root, { field: "playlist", ids: [7] }, true);
    expect(findRow(added.root, added.rowId)).toMatchObject({ not: true });
    expect(added.root.rules.findIndex((r) => r.id === added.rowId)).toBe(3);
  });

  it("addValue extends the root row of its field, and adds a row in a nested group", () => {
    let root = freshRoot(DEFAULTS);
    const genreRow = root.rules[2] as RowDraft;
    root = addValue(root, root.id, { field: "genre", genres: ["House"] }).root;
    const extended = addValue(root, root.id, { field: "genre", genres: ["Techno"] });
    expect(extended.rowId).toBe(genreRow.id);
    expect(findRow(extended.root, genreRow.id)?.value).toEqual({
      field: "genre",
      genres: ["House", "Techno"],
    });
    const group = newGroup("any", [newRow({ field: "genre", genres: ["Disco"] })]);
    const nested = addValue(newGroup("all", [group]), group.id, {
      field: "genre",
      genres: ["Funk"],
    });
    expect(findGroup(nested.root, group.id)?.rules).toHaveLength(2);
    expect(findRow(nested.root, nested.rowId)?.value).toEqual({ field: "genre", genres: ["Funk"] });
  });

  it("pruneTags drops deleted tags and the rows of deleted categories", () => {
    const keep = tags(1, [10, 11]);
    const gone = tags(2, [20]);
    const root = pruneTags(newGroup("all", [keep, gone]), new Map([[1, new Set([11])]]));
    expect(root.rules).toHaveLength(1);
    expect(findRow(root, keep.id)?.value).toMatchObject({ ids: [11] });
  });
});

describe("summarize", () => {
  it("reads the rows that filter, groups in brackets", () => {
    const root = newGroup("all", [
      row({ field: "bpm", min: 124, max: 130 }),
      { ...tags(2, [21]), not: true },
      { ...genre("Disco"), paused: true },
      newGroup("any", [tags(1, [10, 11], true), row({ field: "playlist", ids: [7] })]),
    ]);
    expect(summarize(root, names)).toBe(
      "BPM 124–130 · Situation not Warm-up · (Genre House and Techno or in playlist Closing)",
    );
    expect(summarize(freshRoot(DEFAULTS), names)).toBeNull();
  });
});
