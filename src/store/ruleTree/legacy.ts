/**
 * Shapes the drawer wrote before core had `ratingRange` and `tagAll`, still
 * valid in saved filters and smart playlists: "exactly n★" as `ratingMin n`
 * plus a negated `ratingMin n+1`, and "all of these tags" as a group of
 * one-tag `tagAny`s. Read only; new rules use the 1:1 filters.
 */
import type { RuleNode, TagId } from "../../lib/ipc";
import type { CategoryOf } from "./fromRule";
import { newRow, type RowDraft } from "./model";

function isExactly(rules: readonly RuleNode[]): boolean {
  const [a, b] = rules;
  return (
    a?.type === "filter" &&
    b?.type === "filter" &&
    a.filter.type === "ratingMin" &&
    b.filter.type === "ratingMin" &&
    b.filter.rating === a.filter.rating + 1 &&
    (a.not ?? false) !== (b.not ?? false)
  );
}

export function legacyGroupRow(
  n: RuleNode & { type: "group" },
  categoryOf: CategoryOf,
): RowDraft | null {
  const rules = n.rules ?? [];
  const not = n.not ?? false;
  if (rules.length === 2 && isExactly(rules)) {
    const first = rules[0] as RuleNode & { type: "filter" };
    const stars = (first.filter as { rating: number }).rating;
    // all [≥n, not ≥n+1] is exactly n; any [not ≥n, ≥n+1] is that excluded.
    const excluded = n.match === "any" ? !not : not;
    return newRow({ field: "rating", op: "exactly", stars }, excluded);
  }
  const filters = rules.flatMap((r) => (r.type === "filter" && r.not !== true ? [r.filter] : []));
  if (n.match === "all" && !not && filters.length > 1 && filters.length === rules.length) {
    const ids = filters.map((f) => (f.type === "tagAny" ? f.ids : []));
    if (ids.some((i) => i.length !== 1)) return null;
    const flat = ids.map((i) => i[0] as TagId);
    const category = categoryOf(flat[0] as TagId);
    if (category === undefined || flat.some((id) => categoryOf(id) !== category)) return null;
    return newRow({ field: "tag", category, ids: flat, all: true });
  }
  return null;
}
