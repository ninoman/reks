import { describe, expect, it } from "vitest";

import type { RuleNode } from "../lib/ipc";
import {
  addAfter,
  addItem,
  canAddGroup,
  defaultValue,
  emptyRoot,
  fromRule,
  type GroupDraft,
  isBlank,
  levelOf,
  neighbourRow,
  newGroup,
  newRow,
  type RowDraft,
  removeItem,
  rowCount,
  ruleLeafCount,
  setMode,
  summarize,
  toRule,
  updateRow,
} from "./ruleTree";

const names = {
  tag: (id: number) => ({ 1: "Peak", 2: "Deep" })[id] ?? `t${id}`,
  playlist: (id: number) => ({ 7: "Warm-up" })[id] ?? `p${id}`,
  key: (k: string) => k,
};

const tagRow = (ids: number[], not = false): RowDraft => newRow({ field: "tag", ids }, not);
const playlistRow = (id: number | null, not = false): RowDraft =>
  newRow({ field: "playlist", id }, not);

describe("toRule", () => {
  it("leaves unfinished rows out, and a builder with none finished is null", () => {
    expect(toRule(emptyRoot())).toBeNull();
    const root = newGroup("all", [newRow(defaultValue("tag")), playlistRow(7)]);
    expect(toRule(root)).toEqual({
      type: "group",
      match: "all",
      not: false,
      rules: [{ type: "filter", not: false, filter: { type: "playlist", id: 7 } }],
    });
  });

  it("writes none as an any group with not, and rows as is / is not", () => {
    const root = newGroup("none", [tagRow([1, 2]), playlistRow(7, true)]);
    expect(toRule(root)).toEqual({
      type: "group",
      match: "any",
      not: true,
      rules: [
        { type: "filter", not: false, filter: { type: "tagAny", ids: [1, 2] } },
        { type: "filter", not: true, filter: { type: "playlist", id: 7 } },
      ],
    });
  });

  it("drops a nested group with nothing finished (an empty any would match nothing)", () => {
    const inner = newGroup("any", [newRow(defaultValue("genre"))]);
    const root = newGroup("all", [tagRow([1]), inner]);
    expect(ruleLeafCount(toRule(root))).toBe(1);
    expect(JSON.stringify(toRule(root))).not.toContain('"any"');
  });

  it("scales BPM to x100 and fills open ends like the filter bar", () => {
    const root = newGroup("all", [
      newRow({ field: "bpm", min: 120, max: null }),
      newRow({ field: "plays", min: null, max: 3 }),
    ]);
    const filters = (toRule(root) as Extract<RuleNode, { type: "group" }>).rules?.map(
      (r) => (r as Extract<RuleNode, { type: "filter" }>).filter,
    );
    expect(filters).toEqual([
      { type: "bpmRange", min: 12000, max: 100000 },
      { type: "playCountRange", min: 0, max: 3 },
    ]);
  });

  it("needs a real date for played since", () => {
    const row = (date: string) => newRow({ field: "lastPlayed", mode: "since", date });
    expect(toRule(newGroup("all", [row("soon")]))).toBeNull();
    expect(toRule(newGroup("all", [row("2026-01-02")]))).not.toBeNull();
  });
});

describe("edits", () => {
  it("adds, updates and removes by id without touching the original", () => {
    const row = tagRow([]);
    const root = newGroup("all", [row]);
    const inner = newGroup("any");
    const added = addItem(root, root.id, inner);
    expect(root.rules).toHaveLength(1);
    expect(added.rules).toHaveLength(2);

    const second = playlistRow(null);
    const nested = addItem(added, inner.id, second);
    const updated = updateRow(nested, second.id, { not: true });
    const found = (updated.rules[1] as GroupDraft).rules[0] as RowDraft;
    expect(found.not).toBe(true);
    expect(rowCount(removeItem(updated, inner.id))).toBe(1);
  });

  it("inserts after a row, also inside a nested group", () => {
    const a = tagRow([1]);
    const b = tagRow([2]);
    const inner = newGroup("all", [a]);
    const root = newGroup("all", [inner, b]);
    const c = tagRow([3]);
    const next = addAfter(root, a.id, c);
    expect((next.rules[0] as GroupDraft).rules.map((r) => r.id)).toEqual([a.id, c.id]);
  });

  it("toggling mode changes the rule: none negates", () => {
    const root = newGroup("all", [tagRow([1])]);
    expect(toRule(setMode(root, root.id, "none"))).toMatchObject({ match: "any", not: true });
    expect(toRule(setMode(root, root.id, "any"))).toMatchObject({ match: "any", not: false });
  });

  it("offers at most three levels", () => {
    const l3 = newGroup("all");
    const l2 = newGroup("all", [l3]);
    const root = newGroup("all", [l2]);
    expect(levelOf(root, l3.id)).toBe(3);
    expect(canAddGroup(root, root.id)).toBe(true);
    expect(canAddGroup(root, l2.id)).toBe(true);
    expect(canAddGroup(root, l3.id)).toBe(false);
  });

  it("names a neighbour to focus when a row goes", () => {
    const a = tagRow([1]);
    const b = tagRow([2]);
    const root = newGroup("all", [a, b]);
    expect(neighbourRow(root, a.id)).toBe(b.id);
    expect(neighbourRow(root, b.id)).toBe(a.id);
    expect(neighbourRow(newGroup("all", [a]), a.id)).toBeNull();
  });

  it("knows a blank builder", () => {
    expect(isBlank(emptyRoot())).toBe(true);
    expect(isBlank(newGroup("all", [tagRow([])]))).toBe(false);
  });
});

describe("summarize", () => {
  it("reads like the plan's example", () => {
    const root = newGroup("all", [
      tagRow([1, 2]),
      playlistRow(7, true),
      newRow({ field: "bpm", min: 120, max: 128 }),
    ]);
    expect(summarize(root, names)).toBe(
      "Has tag Peak/Deep · not in playlist Warm-up · BPM is 120–128",
    );
  });

  it("brackets nested any groups and spells none out; skips unfinished rows", () => {
    const root = newGroup("all", [
      newGroup("any", [tagRow([1]), newRow({ field: "genre", genre: "House" })]),
      newGroup("none", [playlistRow(7)]),
      newRow(defaultValue("key")),
    ]);
    expect(summarize(root, names)).toBe(
      "(Has tag Peak or genre is House) · none of (in playlist Warm-up)",
    );
    expect(summarize(emptyRoot(), names)).toBeNull();
  });
});

describe("fromRule", () => {
  it("round-trips what toRule wrote", () => {
    const root = newGroup("none", [
      tagRow([1, 2]),
      newGroup("any", [
        playlistRow(7, true),
        newRow({ field: "bpm", min: 120, max: null }),
        newRow({ field: "lastPlayed", mode: "since", date: "2026-02-03" }),
        newRow({ field: "missing", missing: false }),
      ]),
    ]);
    const rule = toRule(root) as RuleNode;
    expect(toRule(fromRule(rule))).toEqual(rule);
  });

  it("wraps a bare filter in an all group", () => {
    const rule: RuleNode = { type: "filter", filter: { type: "neverPlayed" } };
    const group = fromRule(rule);
    expect(group.mode).toBe("all");
    expect(rowCount(group)).toBe(1);
  });

  it("rewrites a negated all group (hand-made) as an any group of negated rules", () => {
    const rule: RuleNode = {
      type: "group",
      match: "all",
      not: true,
      rules: [
        { type: "filter", filter: { type: "playlist", id: 7 } },
        { type: "filter", not: true, filter: { type: "genre", genre: "House" } },
      ],
    };
    expect(toRule(fromRule(rule))).toEqual({
      type: "group",
      match: "any",
      not: false,
      rules: [
        { type: "filter", not: true, filter: { type: "playlist", id: 7 } },
        { type: "filter", not: false, filter: { type: "genre", genre: "House" } },
      ],
    });
  });
});
