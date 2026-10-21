/**
 * Pure helpers for My Tags: how many of the selected tracks carry a tag (the
 * checkbox state), what a click does, the search filter over the tree, and
 * the local update of a row's tags after an assign. No store, no IPC.
 */
import type { TagCategory, TagId, TagInfo, TagTree } from "../lib/ipc";

/** `all`/`none`: every/no selected track has the tag; `some`: mixed. */
export type TagCoverage = "all" | "some" | "none";

export function coverage(trackTags: readonly (readonly TagId[])[], tag: TagId): TagCoverage {
  if (trackTags.length === 0) return "none";
  const n = trackTags.filter((t) => t.includes(tag)).length;
  return n === 0 ? "none" : n === trackTags.length ? "all" : "some";
}

/** A click adds the tag to every track unless all of them already have it. */
export function clickAction(c: TagCoverage): "assign" | "unassign" {
  return c === "all" ? "unassign" : "assign";
}

/** Tag id → its name and category, for chips and summaries. */
export function tagIndex(tree: TagTree): Map<TagId, { tag: TagInfo; category: TagCategory }> {
  const out = new Map<TagId, { tag: TagInfo; category: TagCategory }>();
  for (const category of tree.categories)
    for (const tag of category.tags) out.set(tag.id, { tag, category });
  return out;
}

/** Categories keeping only tags whose name contains `query`; empty ones dropped. */
export function searchTags(tree: TagTree, query: string): TagCategory[] {
  const q = query.trim().toLowerCase();
  if (q === "") return tree.categories;
  return tree.categories
    .map((c) => ({ ...c, tags: c.tags.filter((t) => t.name.toLowerCase().includes(q)) }))
    .filter((c) => c.tags.length > 0);
}

/** Categories where "Create 'query'" is offered: no tag of that exact name there yet. */
export function creatableIn(tree: TagTree, query: string): TagCategory[] {
  const q = query.trim().toLowerCase();
  if (q === "") return [];
  return tree.categories.filter((c) => !c.tags.some((t) => t.name.toLowerCase() === q));
}

/** `tags` after adding or removing `ids`, kept in category-then-tag order. */
export function applyTags(
  tree: TagTree,
  tags: readonly TagId[],
  ids: readonly TagId[],
  add: boolean,
): TagId[] {
  const next = new Set(tags);
  for (const id of ids) {
    if (add) next.add(id);
    else next.delete(id);
  }
  return tree.categories.flatMap((c) => c.tags.map((t) => t.id)).filter((id) => next.has(id));
}
