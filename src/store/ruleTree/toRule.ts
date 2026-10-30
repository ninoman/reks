/** Draft → `RuleNode`: each row writes the one core filter that says it. */
import type { Filter, RuleNode } from "../../lib/ipc";
import { BPM_X100_MAX, PLAYS_MAX } from "../library";
import type { GroupDraft, ItemDraft, RowDraft, RowValue } from "./model";

// ---- rows → rule -----------------------------------------------------------

const DATE = /^\d{4}-\d{2}-\d{2}$/;

function leaf(filter: Filter, not = false): RuleNode {
  return { type: "filter", not, filter };
}

/** One filter, or an "any" group of them; `not` on the outside. */
function anyOf(filters: Filter[], not: boolean): RuleNode | null {
  if (filters.length === 0) return null;
  if (filters.length === 1) return leaf(filters[0] as Filter, not);
  return { type: "group", match: "any", not, rules: filters.map((f) => leaf(f)) };
}

/** The value as a rule, before Exclude; `null` while it filters nothing. */
function valueRule(v: RowValue): RuleNode | null {
  switch (v.field) {
    case null:
      return null;
    case "genre":
      return anyOf(
        v.genres.map((genre) => ({ type: "genre", genre })),
        false,
      );
    case "color":
      return anyOf(
        v.colours.map((color) => ({ type: "color", color })),
        false,
      );
    case "playlist":
      return anyOf(
        v.ids.map((id) => ({ type: "playlist", id })),
        false,
      );
    case "key":
      return v.keys.length > 0 ? leaf({ type: "keyIn", keys: [...v.keys] }) : null;
    case "tag":
      if (v.ids.length === 0) return null;
      return leaf(
        v.all && v.ids.length > 1
          ? { type: "tagAll", ids: [...v.ids] }
          : { type: "tagAny", ids: [...v.ids] },
      );
    case "bpm":
      return v.min === null && v.max === null
        ? null
        : leaf({
            type: "bpmRange",
            min: v.min === null ? 0 : Math.round(v.min * 100),
            max: v.max === null ? BPM_X100_MAX : Math.round(v.max * 100),
          });
    case "plays":
      return v.min === null && v.max === null
        ? null
        : leaf({ type: "playCountRange", min: v.min ?? 0, max: v.max ?? PLAYS_MAX });
    case "rating": {
      const n = v.stars;
      if (n <= 0) return null;
      if (v.op === "atLeast") return leaf({ type: "ratingMin", rating: n });
      if (v.op === "exactly") return leaf({ type: "ratingRange", min: n, max: n });
      // At most 5★ is every track.
      return n >= 5 ? null : leaf({ type: "ratingRange", min: 0, max: n });
    }
    case "lastPlayed":
      if (v.mode === "never") return leaf({ type: "neverPlayed" });
      return v.mode === "since" && DATE.test(v.date)
        ? leaf({ type: "playedSince", date: v.date })
        : null;
    case "missing":
      return v.missing === null ? null : leaf({ type: "missing", missing: v.missing });
  }
}

function negate(n: RuleNode): RuleNode {
  return { ...n, not: !(n.not ?? false) };
}

/** The rule of a row whether or not it is paused; `null` while it filters nothing. */
export function rowRule(row: RowDraft): RuleNode | null {
  const rule = valueRule(row.value);
  return rule === null || !row.not ? rule : negate(rule);
}

/** The row has a value (paused or not). */
export function isRowSet(row: RowDraft): boolean {
  return valueRule(row.value) !== null;
}

/** The row filters: set and not paused. */
export function isRowActive(row: RowDraft): boolean {
  return !row.paused && isRowSet(row);
}

function itemRule(item: ItemDraft): RuleNode | null {
  if (item.kind === "row") return item.paused ? null : rowRule(item);
  const rules = item.rules.flatMap((r) => itemRule(r) ?? []);
  // A group with nothing that filters is left out: an empty "any" would match no track.
  if (rules.length === 0) return null;
  return {
    type: "group",
    match: item.mode === "all" ? "all" : "any",
    not: item.mode === "none",
    rules,
  };
}

/** The rule of the rows that filter; `null` when none does. */
export function toRule(root: GroupDraft): RuleNode | null {
  return itemRule(root);
}
