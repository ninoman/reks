/**
 * In-memory My Tags for the dev mock backend. Same rules as `tag.rs`: names
 * are trimmed, non-empty and unique under one parent (case-insensitive); a tag
 * id where a category is needed (or the reverse) and unknown ids are rejected
 * before anything is written.
 */
import type { TagCategory, TagId, TagOp, TagOutcome, TagTree } from "../lib/ipc";

interface Cat {
  id: TagId;
  name: string;
  tags: { id: TagId; name: string }[];
}

export function createMockTags(trackCount: number) {
  let nextId = 1;
  const cats: Cat[] = ["Genre", "Components", "Situation"].map((name) => ({
    id: nextId++,
    name,
    tags: [],
  }));
  /** tag id → track ids. */
  const links = new Map<TagId, Set<number>>();

  const category = (id: TagId) => cats.find((c) => c.id === id);
  const tagLoc = (id: TagId) => {
    for (const c of cats) {
      const i = c.tags.findIndex((t) => t.id === id);
      if (i >= 0) return { cat: c, index: i };
    }
    return undefined;
  };

  function cleanName(name: string, siblings: { id: TagId; name: string }[], self?: TagId) {
    const n = name.trim();
    if (n === "") throw new Error("invalid tag name: must not be empty");
    if (siblings.some((s) => s.id !== self && s.name.toLowerCase() === n.toLowerCase()))
      throw new Error(`a tag named "${n}" already exists here`);
    return n;
  }

  function place<T>(list: T[], item: T, position?: number) {
    list.splice(Math.min(Math.max(position ?? list.length, 0), list.length), 0, item);
  }

  function checkTags(tagIds: TagId[]) {
    for (const id of tagIds) {
      if (tagLoc(id) === undefined)
        throw new Error(category(id) ? "expected a tag, got a category" : `unknown tag ${id}`);
    }
    return tagIds;
  }

  function mutate(op: TagOp): TagOutcome {
    let createdId: TagId | null = null;
    let changed = 0;
    switch (op.type) {
      case "createCategory": {
        const name = cleanName(op.name, cats);
        createdId = nextId++;
        place(cats, { id: createdId, name, tags: [] }, op.position);
        break;
      }
      case "createTag": {
        const cat = category(op.categoryId);
        if (cat === undefined)
          throw new Error(tagLoc(op.categoryId) ? "expected a category, got a tag" : "unknown id");
        const name = cleanName(op.name, cat.tags);
        createdId = nextId++;
        place(cat.tags, { id: createdId, name }, op.position);
        break;
      }
      case "rename": {
        const cat = category(op.id);
        if (cat !== undefined) cat.name = cleanName(op.name, cats, op.id);
        else {
          const loc = tagLoc(op.id);
          if (loc === undefined) throw new Error(`unknown id ${op.id}`);
          const t = loc.cat.tags[loc.index] as Cat["tags"][number];
          t.name = cleanName(op.name, loc.cat.tags, op.id);
        }
        changed = 1;
        break;
      }
      case "delete": {
        const ci = cats.findIndex((c) => c.id === op.id);
        if (ci >= 0) {
          for (const t of cats[ci]?.tags ?? []) links.delete(t.id);
          cats.splice(ci, 1);
        } else {
          const loc = tagLoc(op.id);
          if (loc === undefined) throw new Error(`unknown id ${op.id}`);
          loc.cat.tags.splice(loc.index, 1);
          links.delete(op.id);
        }
        changed = 1;
        break;
      }
      case "move": {
        const ci = cats.findIndex((c) => c.id === op.id);
        if (ci >= 0) {
          if (op.categoryId != null) throw new Error("a category has no parent");
          // Like reks-core: `position` counts the node where it still is.
          const [c] = cats.splice(ci, 1);
          place(cats, c as Cat, ci < op.position ? op.position - 1 : op.position);
        } else {
          const loc = tagLoc(op.id);
          if (loc === undefined) throw new Error(`unknown id ${op.id}`);
          const target = category(op.categoryId ?? loc.cat.id);
          if (target === undefined) throw new Error("expected a category");
          const [t] = loc.cat.tags.splice(loc.index, 1);
          const moved = t as Cat["tags"][number];
          if (target.tags.some((s) => s.name.toLowerCase() === moved.name.toLowerCase())) {
            place(loc.cat.tags, moved, loc.index);
            throw new Error(`a tag named "${moved.name}" already exists here`);
          }
          const same = target === loc.cat && loc.index < op.position;
          place(target.tags, moved, same ? op.position - 1 : op.position);
        }
        changed = 1;
        break;
      }
      case "assign": {
        for (const id of checkTags(op.tagIds)) {
          const set = links.get(id) ?? new Set<number>();
          for (const t of op.trackIds) {
            if (!set.has(t)) changed++;
            set.add(t);
          }
          links.set(id, set);
        }
        break;
      }
      case "unassign": {
        for (const id of checkTags(op.tagIds)) {
          const set = links.get(id);
          for (const t of op.trackIds) if (set?.delete(t)) changed++;
        }
        break;
      }
    }
    return { createdId, changed };
  }

  function tree(): TagTree {
    const categories: TagCategory[] = cats.map((c) => ({
      id: c.id,
      name: c.name,
      tags: c.tags.map((t) => ({ id: t.id, name: t.name, trackCount: links.get(t.id)?.size ?? 0 })),
    }));
    return { categories };
  }

  /** The track's tags, by category then tag order. */
  function tagsOf(trackId: number): TagId[] {
    return cats.flatMap((c) =>
      c.tags.filter((t) => links.get(t.id)?.has(trackId)).map((t) => t.id),
    );
  }

  function hasAny(trackId: number, ids: TagId[]): boolean {
    return ids.some((id) => links.get(id)?.has(trackId));
  }

  // A few tags and assignments so the UI is demoable.
  const add = (cat: number, name: string) => mutate({ type: "createTag", categoryId: cat, name });
  const house = add(1, "House").createdId as TagId;
  const techno = add(1, "Techno").createdId as TagId;
  const vocal = add(2, "Vocal").createdId as TagId;
  const dub = add(2, "Dub").createdId as TagId;
  const peak = add(3, "Peak time").createdId as TagId;
  const warm = add(3, "Warm-up").createdId as TagId;
  for (let id = 1; id <= trackCount; id++) {
    const pick = [house, techno][id % 2] as TagId;
    mutate({ type: "assign", trackIds: [id], tagIds: [pick] });
    if (id % 3 === 0) mutate({ type: "assign", trackIds: [id], tagIds: [vocal] });
    if (id % 7 === 0) mutate({ type: "assign", trackIds: [id], tagIds: [dub, peak] });
    if (id % 11 === 0) mutate({ type: "assign", trackIds: [id], tagIds: [warm] });
  }

  return { mutate, tree, tagsOf, hasAny };
}
