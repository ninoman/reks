/** `RuleNode` → draft, to edit a saved filter or smart playlist. */
import type { Filter, RuleNode, TagId } from "../../lib/ipc";
import { BPM_X100_MAX, PLAYS_MAX } from "../library";
import { legacyGroupRow } from "./legacy";
import {
  type GroupDraft,
  type GroupMode,
  type ItemDraft,
  newGroup,
  newRow,
  type RatingOp,
  type RowDraft,
  type RowValue,
} from "./model";

// ---- rule → draft (to edit a saved smart playlist) --------------------------

/**
 * Rewrites a negated "all" group as an "any" group of negated rules (De
 * Morgan), the one shape the three-way group control cannot say. Only
 * hand-made rules have it.
 */
function normalize(n: RuleNode): RuleNode {
  if (n.type === "filter") return n;
  const rules = (n.rules ?? []).map(normalize);
  if (n.match === "all" && n.not === true) {
    return {
      type: "group",
      match: "any",
      not: false,
      rules: rules.map((r) => normalize({ ...r, not: !(r.not ?? false) })),
    };
  }
  return { ...n, rules };
}

/** Tag id → its category, from the tag tree; `undefined` when not known. */
export type CategoryOf = (tag: TagId) => TagId | undefined;

function rowOf(value: RowValue, not = false): RowDraft {
  return newRow(value, not);
}

function filterItem(f: Filter, not: boolean, categoryOf: CategoryOf): ItemDraft {
  switch (f.type) {
    case "genre":
      return rowOf({ field: "genre", genres: [f.genre] }, not);
    case "color":
      return rowOf({ field: "color", colours: [f.color] }, not);
    case "playlist":
      return rowOf({ field: "playlist", ids: [f.id] }, not);
    case "keyIn":
      return rowOf({ field: "key", keys: [...f.keys] }, not);
    case "bpmRange":
      return rowOf(
        {
          field: "bpm",
          min: f.min === 0 ? null : f.min / 100,
          max: f.max >= BPM_X100_MAX ? null : f.max / 100,
        },
        not,
      );
    case "ratingMin":
      // "Not at least n" is "at most n − 1" while there is a star left to say it with.
      return not && f.rating >= 2
        ? rowOf({ field: "rating", op: "atMost", stars: Math.min(5, f.rating - 1) })
        : rowOf({ field: "rating", op: "atLeast", stars: f.rating }, not);
    case "playCountRange":
      return rowOf(
        {
          field: "plays",
          min: f.min === 0 ? null : f.min,
          max: f.max >= PLAYS_MAX ? null : f.max,
        },
        not,
      );
    case "neverPlayed":
      return rowOf({ field: "lastPlayed", mode: "never", date: "" }, not);
    case "playedSince":
      return rowOf({ field: "lastPlayed", mode: "since", date: f.date }, not);
    case "missing":
      return rowOf({ field: "missing", missing: f.missing }, not);
    case "ratingRange":
      return ratingRangeItem(f.min, f.max, not);
    case "tagAny":
      return tagItem(f.ids, false, not, categoryOf);
    case "tagAll":
      return tagItem(f.ids, true, not, categoryOf);
  }
}

/** One row per category; several categories are "any of" (any) or "all of" (all) those rows. */
function tagItem(
  ids: readonly TagId[],
  all: boolean,
  not: boolean,
  categoryOf: CategoryOf,
): ItemDraft {
  const byCategory = new Map<TagId | null, TagId[]>();
  for (const id of ids) {
    const c = categoryOf(id) ?? null;
    byCategory.set(c, [...(byCategory.get(c) ?? []), id]);
  }
  const made = [...byCategory].map(([category, list]) =>
    rowOf({ field: "tag", category, ids: list, all }),
  );
  if (made.length === 1) return { ...(made[0] as RowDraft), not };
  // Tags of several categories: any-of is one "any" group; all-of is one "all" group.
  return newGroup(
    all ? (not ? "any" : "all") : not ? "none" : "any",
    all && not ? made.map((r) => ({ ...r, not: true })) : made,
  );
}

/** Bounds a row can't say at once become two rows. */
function ratingRangeItem(min: number, max: number, not: boolean): ItemDraft {
  const stars = (op: RatingOp, n: number, negated: boolean) =>
    rowOf({ field: "rating", op, stars: n }, negated);
  if (min === max) return stars("exactly", min, not);
  if (min <= 0) return stars("atMost", max, not);
  if (max >= 5) return stars("atLeast", min, not);
  return not
    ? newGroup("any", [stars("atLeast", min, true), stars("atMost", max, true)])
    : newGroup("all", [stars("atLeast", min, false), stars("atMost", max, false)]);
}

/** A group the drawer wrote for one row (a list's "any of", or a legacy shape); else `null`. */
function groupRow(n: RuleNode & { type: "group" }, categoryOf: CategoryOf): RowDraft | null {
  const rules = n.rules ?? [];
  const not = n.not ?? false;
  const filters = rules.flatMap((r) => (r.type === "filter" && r.not !== true ? [r.filter] : []));
  const legacy = legacyGroupRow(n, categoryOf);
  if (legacy !== null) return legacy;
  if (filters.length !== rules.length || filters.length === 0) return null;
  const type = filters[0]?.type;
  if (!filters.every((f) => f.type === type)) return null;
  if (n.match === "any") {
    if (type === "genre")
      return rowOf(
        { field: "genre", genres: filters.map((f) => (f as { genre: string }).genre) },
        not,
      );
    if (type === "color")
      return rowOf(
        { field: "color", colours: filters.map((f) => (f as { color: string }).color) },
        not,
      );
    if (type === "playlist")
      return rowOf({ field: "playlist", ids: filters.map((f) => (f as { id: number }).id) }, not);
    return null;
  }
  return null;
}

function itemOf(n: RuleNode, categoryOf: CategoryOf): ItemDraft {
  if (n.type === "filter") return filterItem(n.filter, n.not ?? false, categoryOf);
  const row = groupRow(n, categoryOf);
  if (row !== null) return row;
  const mode: GroupMode = n.match === "all" ? "all" : n.not === true ? "none" : "any";
  return newGroup(
    mode,
    (n.rules ?? []).map((r) => itemOf(r, categoryOf)),
  );
}

/** A draft for a saved rule; anything that is not a plain root group becomes its one item. */
export function fromRule(rule: RuleNode, categoryOf: CategoryOf): GroupDraft {
  const n = normalize(rule);
  const item = itemOf(n, categoryOf);
  return item.kind === "group" && item.mode !== "none" ? item : newGroup("all", [item]);
}
