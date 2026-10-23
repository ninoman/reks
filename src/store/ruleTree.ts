/**
 * The Filters drawer's query as pure data and functions (D35). The drawer
 * edits one *draft*: a root group of rows and nested groups. Rows may be
 * empty (a default field with no value yet), unfinished or paused; `toRule`
 * turns the rows that filter into the `RuleNode` that `library_query` and the
 * smart playlist ops take. Everything is id-based and immutable, so React can
 * key on ids and the store can focus a row by id. No store, no IPC.
 */
import type { Filter, Key, RuleNode, TagId } from "../lib/ipc";
import { BPM_X100_MAX, PLAYS_MAX } from "./library";

/** Levels the UI offers (the root group counts as one); the core allows 8. */
export const MAX_LEVELS = 3;

export type RuleField =
  | "bpm"
  | "key"
  | "genre"
  | "rating"
  | "color"
  | "playlist"
  | "tag"
  | "plays"
  | "lastPlayed"
  | "missing";

/** Every field but My Tags, which has one field per category. */
export type PlainField = Exclude<RuleField, "tag">;

/** A field as the drawer counts it: each My Tags category is a field of its own. */
export type FieldKey = PlainField | `tag:${number}`;

export const FIELD_LABELS: Record<PlainField, string> = {
  bpm: "BPM",
  key: "Key",
  genre: "Genre",
  rating: "Rating",
  color: "Colour",
  playlist: "Playlist",
  plays: "Plays",
  lastPlayed: "Last played",
  missing: "File",
};

/** The field chooser's groups, in order; `tags` stands for one entry per My Tags category. */
export const FIELD_GROUPS: readonly { label: string; fields: readonly PlainField[] | "tags" }[] = [
  { label: "Music", fields: ["bpm", "key", "genre"] },
  { label: "Library", fields: ["rating", "color", "playlist"] },
  { label: "My Tags", fields: "tags" },
  { label: "History", fields: ["plays", "lastPlayed"] },
  { label: "File", fields: ["missing"] },
];

/** Default fields before the user picks their own; the first My Tags category joins them. */
export const BUILTIN_DEFAULTS: readonly PlainField[] = ["bpm", "key", "genre", "rating"];

export type RatingOp = "atLeast" | "exactly" | "atMost";

/** What a row holds per field. `field: null` is a row waiting for its field (made by Or…). */
export type RowValue =
  | { field: null }
  | { field: "genre"; genres: string[] }
  | { field: "color"; colours: string[] }
  | { field: "playlist"; ids: number[] }
  | { field: "key"; keys: Key[] }
  /**
   * One My Tags category: tracks with any (or, `all`, every) of `ids`.
   * `category` is `null` for tags whose category is not known (deleted, or
   * tags not loaded when a saved rule was read).
   */
  | { field: "tag"; category: TagId | null; ids: TagId[]; all: boolean }
  /** BPM as shown (128), not × 100; either side may be open. */
  | { field: "bpm"; min: number | null; max: number | null }
  | { field: "plays"; min: number | null; max: number | null }
  /** `stars` 0 = not chosen. */
  | { field: "rating"; op: RatingOp; stars: number }
  /** `date` is `YYYY-MM-DD`; a saved rule can only hold a fixed date. */
  | { field: "lastPlayed"; mode: "never" | "since" | null; date: string }
  | { field: "missing"; missing: boolean | null };

export interface RowDraft {
  kind: "row";
  id: string;
  /** Exclude: "none of" for lists, "not" for the rest. */
  not: boolean;
  /** Kept in place but left out of the query. */
  paused: boolean;
  /**
   * Put there as a default field, not by the user. An empty one is an
   * "unused default": added rows go above it, and it hides while its field
   * is used elsewhere in the query.
   */
  placeholder: boolean;
  value: RowValue;
}

/** "none" is a group of mode any with `not` in the rule. */
export type GroupMode = "all" | "any" | "none";

export interface GroupDraft {
  kind: "group";
  id: string;
  mode: GroupMode;
  rules: ItemDraft[];
}

export type ItemDraft = RowDraft | GroupDraft;

let counter = 0;
/** A fresh id for a row or group; unique within the session. */
export function nextId(): string {
  counter += 1;
  return `r${counter}`;
}

export function newRow(value: RowValue = { field: null }, not = false): RowDraft {
  return { kind: "row", id: nextId(), not, paused: false, placeholder: false, value };
}

export function newGroup(mode: GroupMode = "all", rules: ItemDraft[] = []): GroupDraft {
  return { kind: "group", id: nextId(), mode, rules };
}

// ---- fields ----------------------------------------------------------------

export function isFieldKey(s: string): s is FieldKey {
  return Object.hasOwn(FIELD_LABELS, s) || /^tag:\d+$/.test(s);
}

export function tagKey(category: TagId): FieldKey {
  return `tag:${category}`;
}

/** The category of a `tag:<id>` key; `null` for a plain field. */
export function keyCategory(key: FieldKey): TagId | null {
  return key.startsWith("tag:") ? Number(key.slice(4)) : null;
}

/** The row's field as the drawer counts it; `null` with no field or an unknown tag category. */
export function fieldKey(v: RowValue): FieldKey | null {
  if (v.field === "tag") return v.category === null ? null : tagKey(v.category);
  return v.field;
}

/** An empty value of the field `key`: the row shows "Any" and filters nothing. */
export function emptyValue(key: FieldKey): RowValue {
  const category = keyCategory(key);
  if (category !== null) return { field: "tag", category, ids: [], all: false };
  switch (key as PlainField) {
    case "genre":
      return { field: "genre", genres: [] };
    case "color":
      return { field: "color", colours: [] };
    case "playlist":
      return { field: "playlist", ids: [] };
    case "key":
      return { field: "key", keys: [] };
    case "bpm":
      return { field: "bpm", min: null, max: null };
    case "plays":
      return { field: "plays", min: null, max: null };
    case "rating":
      return { field: "rating", op: "atLeast", stars: 0 };
    case "lastPlayed":
      return { field: "lastPlayed", mode: null, date: "" };
    case "missing":
      return { field: "missing", missing: null };
  }
}

/**
 * The default fields: the user's list (settings `ui.filterFields`) or, with
 * `null`, the built-in ones and the first My Tags category. Tag categories
 * that don't exist (or aren't loaded yet: `categories` null) are left out.
 */
export function defaultKeys(
  setting: readonly string[] | null,
  categories: readonly TagId[] | null,
): FieldKey[] {
  if (setting === null) {
    const first = categories?.[0];
    return [...BUILTIN_DEFAULTS, ...(first === undefined ? [] : [tagKey(first)])];
  }
  return setting.filter(isFieldKey).filter((k) => {
    const c = keyCategory(k);
    return c === null || (categories?.includes(c) ?? false);
  });
}

/** A fresh drawer: one empty row per default field. */
export function freshRoot(defaults: readonly FieldKey[]): GroupDraft {
  return newGroup(
    "all",
    defaults.map((k) => ({ ...newRow(emptyValue(k)), placeholder: true })),
  );
}

/** Adds an empty row for each default field the root level has no row for, at the end. */
export function ensureDefaults(root: GroupDraft, defaults: readonly FieldKey[]): GroupDraft {
  const have = new Set(root.rules.flatMap((i) => (i.kind === "row" ? [fieldKey(i.value)] : [])));
  const missing = defaults.filter((k) => !have.has(k));
  if (missing.length === 0) return root;
  return {
    ...root,
    rules: [
      ...root.rules,
      ...missing.map((k) => ({ ...newRow(emptyValue(k)), placeholder: true })),
    ],
  };
}

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
      if (v.all && v.ids.length > 1) {
        return {
          type: "group",
          match: "all",
          not: false,
          rules: v.ids.map((id) => leaf({ type: "tagAny", ids: [id] })),
        };
      }
      return leaf({ type: "tagAny", ids: [...v.ids] });
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
      if (v.op === "atLeast" || (v.op === "exactly" && n >= 5)) {
        return leaf({ type: "ratingMin", rating: n });
      }
      // At most 5★ is every track.
      if (v.op === "atMost")
        return n >= 5 ? null : leaf({ type: "ratingMin", rating: n + 1 }, true);
      return {
        type: "group",
        match: "all",
        not: false,
        rules: [
          leaf({ type: "ratingMin", rating: n }),
          leaf({ type: "ratingMin", rating: n + 1 }, true),
        ],
      };
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
  // "Exactly n★" excluded: written as "below n or above n" so it reads back as one row.
  if (
    n.type === "group" &&
    n.match === "all" &&
    n.not !== true &&
    n.rules?.length === 2 &&
    isExactly(n.rules)
  ) {
    return { ...n, match: "any", rules: n.rules.map((r) => ({ ...r, not: !(r.not ?? false) })) };
  }
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

function rows(item: ItemDraft): RowDraft[] {
  return item.kind === "row" ? [item] : item.rules.flatMap(rows);
}

/** Rows that filter (what the Filters badge counts). */
export function activeRowCount(root: GroupDraft): number {
  return rows(root).filter(isRowActive).length;
}

/** Nothing set anywhere and no groups: Clear all has nothing to do. */
export function isBlank(root: GroupDraft): boolean {
  return root.rules.every((i) => i.kind === "row" && !isRowSet(i));
}

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
    case "tagAny": {
      // One row per category; tags of several categories are "any of" those rows.
      const byCategory = new Map<TagId | null, TagId[]>();
      for (const id of f.ids) {
        const c = categoryOf(id) ?? null;
        byCategory.set(c, [...(byCategory.get(c) ?? []), id]);
      }
      const made = [...byCategory].map(([category, ids]) =>
        rowOf({ field: "tag", category, ids, all: false }),
      );
      if (made.length === 1) return { ...(made[0] as RowDraft), not };
      return newGroup(not ? "none" : "any", made);
    }
  }
}

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

/** A group the drawer wrote for one row (a list's "any of", tags' "all of", "exactly"); else `null`. */
function groupRow(n: RuleNode & { type: "group" }, categoryOf: CategoryOf): RowDraft | null {
  const rules = n.rules ?? [];
  const not = n.not ?? false;
  const filters = rules.flatMap((r) => (r.type === "filter" && r.not !== true ? [r.filter] : []));
  if (rules.length === 2 && isExactly(rules)) {
    const first = rules[0] as RuleNode & { type: "filter" };
    const stars = (first.filter as { rating: number }).rating;
    // all [≥n, not ≥n+1] is exactly n; any [not ≥n, ≥n+1] is that excluded.
    const excluded = n.match === "any" ? !not : not;
    return rowOf({ field: "rating", op: "exactly", stars }, excluded);
  }
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
  if (type === "tagAny" && !not && filters.length > 1) {
    const ids = filters.map((f) => (f as { ids: TagId[] }).ids);
    if (ids.some((i) => i.length !== 1)) return null;
    const flat = ids.map((i) => i[0] as TagId);
    const category = categoryOf(flat[0] as TagId);
    if (category === undefined || flat.some((id) => categoryOf(id) !== category)) return null;
    return rowOf({ field: "tag", category, ids: flat, all: true });
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

// ---- reading the draft -----------------------------------------------------

function isUnusedDefault(item: ItemDraft): item is RowDraft {
  return item.kind === "row" && item.placeholder && !isRowSet(item);
}

/**
 * Root rows that don't show: an unused default whose field is used
 * elsewhere in the query (it comes back when that use goes), or a second
 * unused default of the same field.
 */
export function hiddenRows(root: GroupDraft): Set<string> {
  const used = new Map<FieldKey, number>();
  const unused: RowDraft[] = [];
  for (const r of rows(root)) {
    const key = fieldKey(r.value);
    if (key === null) continue;
    if (root.rules.includes(r) && isUnusedDefault(r)) unused.push(r);
    else used.set(key, (used.get(key) ?? 0) + 1);
  }
  const hidden = new Set<string>();
  const seen = new Set<FieldKey>();
  for (const r of unused) {
    const key = fieldKey(r.value) as FieldKey;
    if (used.has(key) || seen.has(key)) hidden.add(r.id);
    seen.add(key);
  }
  return hidden;
}

/** Ids of the rows that show, in reading order (↑ / ↓ move along it). */
export function visibleRowIds(root: GroupDraft): string[] {
  const hidden = hiddenRows(root);
  return rows(root)
    .filter((r) => !hidden.has(r.id))
    .map((r) => r.id);
}

/** Levels from the root down to the item with `id` (the root group is level 1). */
export function levelOf(root: GroupDraft, id: string): number | null {
  if (root.id === id) return 1;
  for (const i of root.rules) {
    if (i.id === id) return 2;
    if (i.kind === "group") {
      const inner = levelOf(i, id);
      if (inner !== null) return inner + 1;
    }
  }
  return null;
}

export function findGroup(root: GroupDraft, id: string): GroupDraft | null {
  if (root.id === id) return root;
  for (const i of root.rules) {
    if (i.kind !== "group") continue;
    const hit = findGroup(i, id);
    if (hit !== null) return hit;
  }
  return null;
}

export function findRow(root: GroupDraft, id: string): RowDraft | null {
  return rows(root).find((r) => r.id === id) ?? null;
}

/** The group directly holding the item `id`. */
export function parentOf(root: GroupDraft, id: string): GroupDraft | null {
  if (root.rules.some((i) => i.id === id)) return root;
  for (const i of root.rules) {
    if (i.kind !== "group") continue;
    const hit = parentOf(i, id);
    if (hit !== null) return hit;
  }
  return null;
}

/** Field keys of the rows that show directly in `group` (the field chooser greys them "shown"). */
export function shownKeys(root: GroupDraft, group: GroupDraft): Set<FieldKey> {
  const hidden = group === root ? hiddenRows(root) : new Set<string>();
  return new Set(
    group.rules.flatMap((i) => {
      const key = i.kind === "row" && !hidden.has(i.id) ? fieldKey(i.value) : null;
      return key === null ? [] : [key];
    }),
  );
}

/** Or… on the row can go one level deeper, or its group already reads "or". */
export function canOr(root: GroupDraft, rowId: string): boolean {
  const parent = parentOf(root, rowId);
  if (parent === null) return false;
  return parent.mode !== "all" || (levelOf(root, parent.id) ?? MAX_LEVELS) < MAX_LEVELS;
}

// ---- edits -----------------------------------------------------------------

function mapItems(
  group: GroupDraft,
  f: (i: ItemDraft) => ItemDraft | ItemDraft[] | null,
): GroupDraft {
  return {
    ...group,
    rules: group.rules.flatMap((i) => {
      const next = f(i);
      const list = next === null ? [] : Array.isArray(next) ? next : [next];
      return list.map((n) => (n.kind === "group" && n === i ? mapItems(n, f) : n));
    }),
  };
}

function inGroup(root: GroupDraft, groupId: string, edit: (g: GroupDraft) => GroupDraft) {
  if (root.id === groupId) return edit(root);
  return mapItems(root, (i) => (i.kind === "group" && i.id === groupId ? edit(i) : i));
}

/** Where a row added to `group` goes: after the last row that isn't an unused default. */
export function insertIndex(root: GroupDraft, group: GroupDraft): number {
  if (group !== root) return group.rules.length;
  let at = 0;
  group.rules.forEach((item, i) => {
    if (!isUnusedDefault(item)) at = i + 1;
  });
  return at;
}

function insertInto(root: GroupDraft, groupId: string, item: ItemDraft): GroupDraft {
  return inGroup(root, groupId, (g) => {
    const at = insertIndex(root, g);
    return { ...g, rules: [...g.rules.slice(0, at), item, ...g.rules.slice(at)] };
  });
}

/** Removes a row or group (not the root). */
export function removeItem(root: GroupDraft, id: string): GroupDraft {
  return mapItems(root, (i) => (i.id === id ? null : i));
}

export function setMode(root: GroupDraft, groupId: string, mode: GroupMode): GroupDraft {
  return inGroup(root, groupId, (g) => ({ ...g, mode }));
}

/** A connector click: "and" becomes "or" for the whole level, and back. */
export function toggleMode(root: GroupDraft, groupId: string): GroupDraft {
  return inGroup(root, groupId, (g) => ({ ...g, mode: g.mode === "all" ? "any" : "all" }));
}

export function updateRow(
  root: GroupDraft,
  rowId: string,
  patch: Partial<Pick<RowDraft, "not" | "paused" | "value">>,
): GroupDraft {
  return mapItems(root, (i) => (i.kind === "row" && i.id === rowId ? { ...i, ...patch } : i));
}

/** × on a row: back to "Any", not excluded, not paused; the row stays. */
export function clearRow(root: GroupDraft, rowId: string): GroupDraft {
  const row = findRow(root, rowId);
  const key = row === null ? null : fieldKey(row.value);
  const value: RowValue =
    key !== null
      ? emptyValue(key)
      : row?.value.field === "tag"
        ? { ...row.value, ids: [], all: false }
        : { field: null };
  return updateRow(root, rowId, { value, not: false, paused: false });
}

/** Gives a field to a row that has none yet (one made by Or…). */
export function chooseField(root: GroupDraft, rowId: string, key: FieldKey): GroupDraft {
  return updateRow(root, rowId, { value: emptyValue(key), not: false, paused: false });
}

/**
 * Add filter / + Or…: a row for `key` in `groupId`. On the root level a
 * field shows once: picking one that shows jumps to its row, and an unused
 * default hidden by a use elsewhere comes back as an added row.
 */
export function addField(
  root: GroupDraft,
  groupId: string,
  key: FieldKey,
): { root: GroupDraft; rowId: string } {
  if (groupId === root.id) {
    const hidden = hiddenRows(root);
    const same = root.rules.filter(
      (i): i is RowDraft => i.kind === "row" && fieldKey(i.value) === key,
    );
    const shown = same.find((r) => !hidden.has(r.id));
    if (shown !== undefined) return { root, rowId: shown.id };
    root = { ...root, rules: root.rules.filter((i) => !same.includes(i as RowDraft)) };
  }
  const row = newRow(emptyValue(key));
  return { root: insertInto(root, groupId, row), rowId: row.id };
}

/**
 * Or… on a row: in a group that already reads "or", an empty row after it;
 * else the row moves into a new "any" group with an empty row below it.
 * Returns `null` when that group would pass `MAX_LEVELS`.
 */
export function orRow(root: GroupDraft, rowId: string): { root: GroupDraft; rowId: string } | null {
  const parent = parentOf(root, rowId);
  const row = findRow(root, rowId);
  if (parent === null || row === null || !canOr(root, rowId)) return null;
  const empty = newRow();
  const next = inGroup(root, parent.id, (p) => ({
    ...p,
    rules: p.rules.flatMap((i) => {
      if (i.id !== rowId) return [i];
      return p.mode === "all"
        ? [newGroup("any", [{ ...row, placeholder: false }, empty])]
        : [i, empty];
    }),
  }));
  return { root: next, rowId: empty.id };
}

/** Puts a group's items in its place in the parent. */
export function ungroup(root: GroupDraft, groupId: string): GroupDraft {
  return mapItems(root, (i) => (i.kind === "group" && i.id === groupId ? i.rules : i));
}

function mergeValues(a: RowValue, b: RowValue): RowValue {
  const union = <T>(x: readonly T[], y: readonly T[]) => [...new Set([...x, ...y])];
  if (a.field === "genre" && b.field === "genre")
    return { ...a, genres: union(a.genres, b.genres) };
  if (a.field === "color" && b.field === "color")
    return { ...a, colours: union(a.colours, b.colours) };
  if (a.field === "playlist" && b.field === "playlist") return { ...a, ids: union(a.ids, b.ids) };
  if (a.field === "key" && b.field === "key") return { ...a, keys: union(a.keys, b.keys) };
  if (a.field === "tag" && b.field === "tag") return { ...a, ids: union(a.ids, b.ids) };
  return b;
}

/**
 * Quick paths ("Filter by tag X", "Show tracks not in this playlist"): fills
 * the root row of that field, adding to its values when it already filters
 * the same way, or adds the row.
 */
export function fillRow(
  root: GroupDraft,
  value: RowValue,
  not: boolean,
): { root: GroupDraft; rowId: string } {
  const key = fieldKey(value);
  const row = root.rules.find(
    (i): i is RowDraft => i.kind === "row" && key !== null && fieldKey(i.value) === key,
  );
  if (row === undefined) {
    const added = newRow(value, not);
    return { root: insertInto(root, root.id, added), rowId: added.id };
  }
  const merge = isRowSet(row) && row.not === not && !(row.value.field === "tag" && row.value.all);
  return {
    root: updateRow(root, row.id, {
      value: merge ? mergeValues(row.value, value) : value,
      not,
      paused: false,
    }),
    rowId: row.id,
  };
}

/** Tag ids gone from the tree leave their rows; rows of a deleted category go. */
export function pruneTags(
  root: GroupDraft,
  categories: ReadonlyMap<TagId, ReadonlySet<TagId>>,
): GroupDraft {
  const known = new Set([...categories.values()].flatMap((s) => [...s]));
  return mapItems(root, (i) => {
    if (i.kind !== "row" || i.value.field !== "tag") return i;
    const v = i.value;
    if (v.category !== null && !categories.has(v.category)) return null;
    const ids = v.ids.filter((id) => known.has(id));
    return ids.length === v.ids.length ? i : { ...i, value: { ...v, ids } };
  });
}

// ---- summary (tooltips only: the drawer's rows say it better) --------------

/** Names the summary needs from the rest of the app. */
export interface SummaryNames {
  tag(id: TagId): string;
  category(id: TagId): string;
  playlist(id: number): string;
  key(key: Key): string;
}

function range(min: number | null, max: number | null): string {
  if (min !== null && max !== null) return `${min}–${max}`;
  return min !== null ? `${min}+` : `up to ${max}`;
}

/** "A", "A or B", "A, B or C". */
function joinList(items: string[], word: "or" | "and"): string {
  return items.length < 2
    ? items.join("")
    : `${items.slice(0, -1).join(", ")} ${word} ${items.at(-1)}`;
}

const RATING_TEXT: Record<RatingOp, (n: number) => string> = {
  atLeast: (n) => `${n}★ or more`,
  exactly: (n) => `${n}★`,
  atMost: (n) => `up to ${n}★`,
};

/** One row that filters as a phrase; `null` for one that doesn't. */
export function rowText(row: RowDraft, names: SummaryNames): string | null {
  if (!isRowActive(row)) return null;
  const v = row.value;
  const not = row.not ? "not " : "";
  switch (v.field) {
    case null:
      return null;
    case "genre":
      return `genre ${not}${joinList(v.genres, "or")}`;
    case "color":
      return `colour ${not}${joinList(v.colours, "or")}`;
    case "playlist":
      return `${not}in playlist ${joinList(v.ids.map(names.playlist), "or")}`;
    case "key":
      return `key ${not}${joinList(v.keys.map(names.key), "or")}`;
    case "tag": {
      const cat = v.category === null ? "tag" : names.category(v.category);
      return `${cat} ${not}${joinList(v.ids.map(names.tag), v.all ? "and" : "or")}`;
    }
    case "bpm":
      return `BPM ${not}${range(v.min, v.max)}`;
    case "plays":
      return `plays ${not}${range(v.min, v.max)}`;
    case "rating":
      return `rating ${not}${RATING_TEXT[v.op](v.stars)}`;
    case "lastPlayed":
      if (v.mode === "never") return row.not ? "has been played" : "never played";
      return `${not}played since ${v.date}`;
    case "missing":
      return `file ${(v.missing ?? true) !== row.not ? "missing" : "present"}`;
  }
}

function itemText(item: ItemDraft, names: SummaryNames, nested: boolean): string | null {
  if (item.kind === "row") return rowText(item, names);
  const parts = item.rules.flatMap((r) => itemText(r, names, true) ?? []);
  if (parts.length === 0) return null;
  const text = parts.join(item.mode === "all" ? " · " : " or ");
  if (item.mode === "none") return `none of (${text})`;
  return nested && parts.length > 1 ? `(${text})` : text;
}

/** "BPM 124–130 · key 8A or 9A · Situation not Warm-up"; `null` while nothing filters. */
export function summarize(root: GroupDraft, names: SummaryNames): string | null {
  const text = itemText(root, names, false);
  return text === null ? null : text.charAt(0).toUpperCase() + text.slice(1);
}
