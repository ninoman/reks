/**
 * Rule trees and smart playlists for the dev mock backend. Mirrors the core's
 * rules: an empty "all" group matches every track and an empty "any" none; NOT
 * of a filter keeps tracks where the field is empty; a folder in a playlist
 * condition means every list below it (smart ones by their rules); a rule that
 * would include its own list is refused.
 */
import type { Filter, PlaylistNode, PlaylistOp, RuleNode, Sort, TrackRow } from "../lib/ipc";

/** What the rules need from the rest of the mock. */
export interface SmartContext {
  nodes: PlaylistNode[];
  /** Track ids of a static playlist, in order. */
  staticTracks(id: number): number[];
  tracks: readonly TrackRow[];
  hasAnyTag(trackId: number, tagIds: number[]): boolean;
}

interface SmartDef {
  rule: RuleNode;
  sort: Sort;
}

const DEFAULT_SORT: Sort = { column: "dateAdded", descending: true };

export function createMockSmart(ctx: SmartContext) {
  const defs = new Map<number, SmartDef>();
  let nextId = 100;

  const smartError = (id: number) =>
    new Error(`${id} is a smart playlist; its tracks follow its rules`);

  /** `seen` guards a cycle at query time: a list met again matches nothing. */
  function inPlaylist(id: number, track: TrackRow, seen: Set<number>): boolean {
    const node = ctx.nodes.find((n) => n.id === id);
    if (node === undefined || seen.has(id)) return false;
    const def = defs.get(id);
    if (def !== undefined) return matches(def.rule, track, new Set(seen).add(id));
    if (node.isFolder) {
      return ctx.nodes.some(
        (c) => c.parentId === id && inPlaylist(c.id, track, new Set(seen).add(id)),
      );
    }
    return ctx.staticTracks(id).includes(track.id);
  }

  function filterMatches(f: Filter, r: TrackRow, seen: Set<number>): boolean {
    switch (f.type) {
      case "genre":
        return r.genre === f.genre;
      case "keyIn":
        return r.key !== null && f.keys.includes(r.key);
      case "bpmRange":
        return r.bpmX100 !== null && r.bpmX100 >= f.min && r.bpmX100 <= f.max;
      case "color":
        return r.color === f.color;
      case "ratingMin":
        return r.rating >= f.rating;
      case "playlist":
        return inPlaylist(f.id, r, seen);
      case "missing":
        return r.missing === f.missing;
      case "neverPlayed":
        return r.playCount === 0 && r.lastPlayed === null;
      case "playCountRange":
        return r.playCount >= f.min && r.playCount <= f.max;
      case "playedSince":
        return r.lastPlayed !== null && r.lastPlayed >= f.date;
      case "tagAny":
        return ctx.hasAnyTag(r.id, f.ids);
    }
  }

  function matches(rule: RuleNode, r: TrackRow, seen: Set<number> = new Set()): boolean {
    if (rule.type === "filter") return filterMatches(rule.filter, r, seen) !== (rule.not ?? false);
    const rules = rule.rules ?? [];
    const hit =
      rule.match === "all"
        ? rules.every((n) => matches(n, r, seen))
        : rules.some((n) => matches(n, r, seen));
    return hit !== (rule.not ?? false);
  }

  /** Filter ids a rule refers to through playlist conditions. */
  function referenced(rule: RuleNode, out: number[] = []): number[] {
    if (rule.type === "filter") {
      if (rule.filter.type === "playlist") out.push(rule.filter.id);
    } else for (const n of rule.rules ?? []) referenced(n, out);
    return out;
  }

  /** Would `rule` on list `id` end up including `id` itself? */
  function cycles(id: number, rule: RuleNode): boolean {
    const seen = new Set<number>();
    const visit = (target: number): boolean => {
      if (target === id) return true;
      if (seen.has(target)) return false;
      seen.add(target);
      const def = defs.get(target);
      const below = ctx.nodes.filter((n) => n.parentId === target).map((n) => n.id);
      return [...(def ? referenced(def.rule) : below)].some(visit);
    };
    return referenced(rule).some(visit);
  }

  function checkRule(id: number | null, rule: RuleNode) {
    let nodes = 0;
    const walk = (n: RuleNode, depth: number) => {
      nodes += 1;
      if (depth > 8) throw new Error("invalid rule: nested more than 8 levels");
      if (n.type === "group") for (const c of n.rules ?? []) walk(c, depth + 1);
    };
    walk(rule, 1);
    if (nodes > 200) throw new Error("invalid rule: more than 200 conditions");
    if (id !== null && cycles(id, rule))
      throw new Error(`smart playlist ${id} would include itself through its rules`);
  }

  /** The list's tracks: all of its rule's matches (callers sort). */
  function smartRows(id: number): TrackRow[] {
    const def = defs.get(id);
    return def === undefined ? [] : ctx.tracks.filter((t) => matches(def.rule, t));
  }

  function refreshCounts() {
    for (const n of ctx.nodes) {
      if (defs.has(n.id)) n.entries = smartRows(n.id).length;
    }
  }

  /** Returns the outcome for the smart ops and the guarded static ones; `undefined` for the rest. */
  function mutate(op: PlaylistOp) {
    const done = (createdId: number | null) =>
      ({ type: "done", createdId, added: 0, skipped: 0 }) as const;
    switch (op.type) {
      case "createSmart": {
        checkRule(null, op.rule);
        const name = op.name.trim();
        if (name === "") throw new Error("invalid playlist name: must not be empty");
        const parent = ctx.nodes.find((n) => n.id === op.parentId);
        const siblings = ctx.nodes.filter((n) => n.parentId === (op.parentId ?? null));
        const id = nextId++;
        const node: PlaylistNode = {
          id,
          parentId: op.parentId ?? null,
          name,
          isFolder: false,
          depth: parent === undefined ? 0 : parent.depth + 1,
          position: siblings.length,
          entries: 0,
          smart: true,
        };
        const at =
          parent === undefined
            ? ctx.nodes.length
            : ctx.nodes.indexOf(parent) + 1 + below(parent.id);
        ctx.nodes.splice(at, 0, node);
        defs.set(id, { rule: op.rule, sort: op.sort ?? DEFAULT_SORT });
        refreshCounts();
        return done(id);
      }
      case "setRule": {
        const def = defs.get(op.id);
        if (def === undefined) throw new Error(`${op.id} is not a smart playlist`);
        checkRule(op.id, op.rule);
        defs.set(op.id, { rule: op.rule, sort: op.sort ?? def.sort });
        refreshCounts();
        return done(null);
      }
      case "addTracks":
      case "removeEntries":
      case "reorderEntries":
      case "insertDivider":
      case "renameDivider":
        if (defs.has(op.playlistId)) throw smartError(op.playlistId);
        return undefined;
      case "delete":
        defs.delete(op.id);
        return undefined;
      default:
        return undefined;
    }
  }

  const below = (id: number): number => {
    const kids = ctx.nodes.filter((n) => n.parentId === id);
    return kids.length + kids.reduce((n, k) => n + below(k.id), 0);
  };

  return {
    isSmart: (id: number) => defs.has(id),
    matches: (rule: RuleNode, r: TrackRow) => matches(rule, r),
    smartRows,
    sortOf: (id: number): Sort => defs.get(id)?.sort ?? DEFAULT_SORT,
    ruleOf(id: number): RuleNode {
      const def = defs.get(id);
      if (def === undefined) throw new Error(`${id} is not a smart playlist`);
      return def.rule;
    },
    refreshCounts,
    mutate,
  };
}
