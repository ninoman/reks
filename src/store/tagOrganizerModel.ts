/**
 * Pure helpers for the tag organizer (`TagOrganizer`): which tags it shows,
 * which duplicate groups (from `tags_duplicates`) touch them, and the tag a
 * merge keeps by default.
 */
import type { CommentTagsPreview, TagCategory, TagId, TagInfo, TagTree } from "../lib/ipc";

export interface TagRef {
  tag: TagInfo;
  category: TagCategory;
}

export function tagIndex(tree: TagTree): Map<TagId, TagRef> {
  const out = new Map<TagId, TagRef>();
  for (const category of tree.categories)
    for (const tag of category.tags) out.set(tag.id, { tag, category });
  return out;
}

/** Categories with only the tags in `scope` (`null`: all); categories left empty by the scope go. */
export function scopedCategories(tree: TagTree, scope: ReadonlySet<TagId> | null): TagCategory[] {
  if (scope === null) return tree.categories;
  return tree.categories
    .map((c) => ({ ...c, tags: c.tags.filter((t) => scope.has(t.id)) }))
    .filter((c) => c.tags.length > 0);
}

/** Groups with a member in `scope` (`null`: all) and at least two tags that still exist. */
export function groupsInScope(
  groups: TagId[][],
  index: Map<TagId, TagRef>,
  scope: ReadonlySet<TagId> | null,
): TagRef[][] {
  return groups
    .map((g) => g.flatMap((id) => index.get(id) ?? []))
    .filter((g) => g.length > 1 && (scope === null || g.some((r) => scope.has(r.tag.id))));
}

/** The tag a merge keeps unless the user picks another: the one on most tracks, else the first. */
export function defaultKeeper(group: TagRef[]): TagId | null {
  let best: TagRef | null = null;
  for (const r of group) if (best === null || r.tag.trackCount > best.tag.trackCount) best = r;
  return best?.tag.id ?? null;
}

/** Where a tag moved into `category` lands: after its last tag. */
export function endOf(tree: TagTree, category: TagId): number {
  return tree.categories.find((c) => c.id === category)?.tags.length ?? 0;
}

function count(n: number, word: string): string {
  return `${n.toLocaleString()} ${word}${n === 1 ? "" : "s"}`;
}

/** "1,203 tracks, 87 tags (12 new)". */
export function commentTagsText(p: CommentTagsPreview): string {
  return `${count(p.tracks, "track")}, ${count(p.names, "tag")} (${p.newNames.toLocaleString()} new)`;
}
