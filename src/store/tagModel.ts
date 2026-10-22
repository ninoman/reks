/**
 * Pure helpers for My Tags: how many of the selected tracks carry a tag (the
 * checkbox state), what a click does, the search filter over the tree, the
 * local update of a row's tags after an assign, and the move a drag-and-drop
 * makes. No store, no IPC.
 */
import type { TagCategory, TagId, TagInfo, TagOp, TagTree } from "../lib/ipc";

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

/** A category or tag row in the My Tags drawer, as a drag source or drop target. */
export interface TagNodeRef {
  kind: "category" | "tag";
  id: TagId;
}

/** Where a drop lands on a row: above or below it, or (category header) into it. */
export type TagDropZone = "before" | "after" | "into";

/**
 * The `move` a drop makes, or `null` when it is not allowed or changes
 * nothing. A tag dropped on a tag lands beside it, in that tag's category;
 * on a category header it goes to the end of that category. Categories only
 * reorder among themselves. `position` counts the dragged node where it
 * still is, as reks-core's `move` expects.
 */
export function dropMove(
  tree: TagTree,
  dragged: TagNodeRef,
  target: TagNodeRef,
  zone: TagDropZone,
): Extract<TagOp, { type: "move" }> | null {
  if (dragged.id === target.id) return null;
  const cats = tree.categories;
  if (dragged.kind === "category") {
    if (target.kind !== "category" || zone === "into") return null;
    const from = cats.findIndex((c) => c.id === dragged.id);
    const to = cats.findIndex((c) => c.id === target.id);
    if (from < 0 || to < 0) return null;
    const at = to + (zone === "after" ? 1 : 0);
    if (from === at || from + 1 === at) return null;
    return { type: "move", id: dragged.id, position: at };
  }
  const source = cats.find((c) => c.tags.some((t) => t.id === dragged.id));
  const dest =
    target.kind === "category"
      ? cats.find((c) => c.id === target.id)
      : cats.find((c) => c.tags.some((t) => t.id === target.id));
  if (source === undefined || dest === undefined) return null;
  // Over a tag, "into" (the middle) is not offered; treat it as "before".
  const at =
    target.kind === "category"
      ? dest.tags.length
      : dest.tags.findIndex((t) => t.id === target.id) + (zone === "after" ? 1 : 0);
  if (dest.id === source.id) {
    const from = source.tags.findIndex((t) => t.id === dragged.id);
    if (from === at || from + 1 === at) return null;
  }
  return { type: "move", id: dragged.id, categoryId: dest.id, position: at };
}
