/**
 * The rule builder's tree as pure data and functions. The builder edits a
 * *draft* (rows may be unfinished: a field picked, no value yet); `toRule`
 * turns the finished rows into the `RuleNode` that `library_query` and the
 * smart playlist ops take. Everything is id-based and immutable, so React can
 * key on ids and focus a new row by id. No store, no IPC.
 */
import type { Filter, Key, RuleNode, TagId } from "../lib/ipc";
import { BPM_X100_MAX, PLAYS_MAX } from "./library";

/** Levels the UI offers (the root group counts as one); the core allows 8. */
export const MAX_LEVELS = 3;

export type RuleField =
  | "tag"
  | "playlist"
  | "genre"
  | "key"
  | "bpm"
  | "color"
  | "rating"
  | "plays"
  | "lastPlayed"
  | "missing";

/** Field choices in the order the row's Select lists them. */
export const RULE_FIELDS: readonly { field: RuleField; label: string }[] = [
  { field: "tag", label: "Tag" },
  { field: "playlist", label: "Playlist" },
  { field: "genre", label: "Genre" },
  { field: "key", label: "Key" },
  { field: "bpm", label: "BPM" },
  { field: "color", label: "Colour" },
  { field: "rating", label: "Rating" },
  { field: "plays", label: "Plays" },
  { field: "lastPlayed", label: "Last played" },
  { field: "missing", label: "Missing" },
];

/** What a row holds per field. `field: null` is a row nobody has set up yet. */
export type RowValue =
  | { field: null }
  | { field: "tag"; ids: TagId[] }
  | { field: "playlist"; id: number | null }
  | { field: "genre"; genre: string | null }
  | { field: "key"; keys: Key[] }
  /** BPM as shown (128), not × 100. */
  | { field: "bpm"; min: number | null; max: number | null }
  | { field: "color"; color: string | null }
  /** 0 = not chosen. */
  | { field: "rating"; min: number }
  | { field: "plays"; min: number | null; max: number | null }
  /** `date` is `YYYY-MM-DD`; a saved rule can only hold a fixed date, not "last 30 days". */
  | { field: "lastPlayed"; mode: "never" | "since"; date: string }
  | { field: "missing"; missing: boolean };

export interface RowDraft {
  kind: "row";
  id: string;
  /** "is not" / "doesn't have" / "not in". */
  not: boolean;
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
  return { kind: "row", id: nextId(), not, value };
}

export function newGroup(mode: GroupMode = "all", rules: ItemDraft[] = []): GroupDraft {
  return { kind: "group", id: nextId(), mode, rules };
}

/** The builder's start: an "all" group with one empty row. */
export function emptyRoot(): GroupDraft {
  return newGroup("all", [newRow()]);
}

export function defaultValue(field: RuleField): RowValue {
  switch (field) {
    case "tag":
      return { field, ids: [] };
    case "playlist":
      return { field, id: null };
    case "genre":
      return { field, genre: null };
    case "key":
      return { field, keys: [] };
    case "bpm":
    case "plays":
      return { field, min: null, max: null };
    case "color":
      return { field, color: null };
    case "rating":
      return { field, min: 0 };
    case "lastPlayed":
      return { field, mode: "never", date: "" };
    case "missing":
      return { field, missing: true };
  }
}

/** Labels of the row's is / is not choice, by field. */
export function negationLabels(field: RuleField | null): readonly [string, string] {
  switch (field) {
    case "tag":
      return ["has", "doesn't have"];
    case "playlist":
      return ["in", "not in"];
    default:
      return ["is", "is not"];
  }
}

const DATE = /^\d{4}-\d{2}-\d{2}$/;

/** The filter a finished row stands for; `null` while the row is unfinished. */
export function rowFilter(v: RowValue): Filter | null {
  switch (v.field) {
    case null:
      return null;
    case "tag":
      return v.ids.length > 0 ? { type: "tagAny", ids: [...v.ids] } : null;
    case "playlist":
      return v.id === null ? null : { type: "playlist", id: v.id };
    case "genre":
      return v.genre === null ? null : { type: "genre", genre: v.genre };
    case "key":
      return v.keys.length > 0 ? { type: "keyIn", keys: [...v.keys] } : null;
    case "bpm":
      return v.min === null && v.max === null
        ? null
        : {
            type: "bpmRange",
            min: v.min === null ? 0 : Math.round(v.min * 100),
            max: v.max === null ? BPM_X100_MAX : Math.round(v.max * 100),
          };
    case "color":
      return v.color === null ? null : { type: "color", color: v.color };
    case "rating":
      return v.min > 0 ? { type: "ratingMin", rating: v.min } : null;
    case "plays":
      return v.min === null && v.max === null
        ? null
        : { type: "playCountRange", min: v.min ?? 0, max: v.max ?? PLAYS_MAX };
    case "lastPlayed":
      if (v.mode === "never") return { type: "neverPlayed" };
      return DATE.test(v.date) ? { type: "playedSince", date: v.date } : null;
    case "missing":
      return { type: "missing", missing: v.missing };
  }
}

export function isRowComplete(row: RowDraft): boolean {
  return rowFilter(row.value) !== null;
}

function itemRule(item: ItemDraft): RuleNode | null {
  if (item.kind === "row") {
    const filter = rowFilter(item.value);
    return filter === null ? null : { type: "filter", not: item.not, filter };
  }
  const rules = item.rules.flatMap((r) => itemRule(r) ?? []);
  // A group with nothing finished in it is left out: an empty "any" would match no track.
  if (rules.length === 0) return null;
  return {
    type: "group",
    match: item.mode === "all" ? "all" : "any",
    not: item.mode === "none",
    rules,
  };
}

/** The rule of the finished rows; `null` when none is finished. */
export function toRule(root: GroupDraft): RuleNode | null {
  return itemRule(root);
}

/** Finished conditions in a rule (what the Filters badge counts). */
export function ruleLeafCount(rule: RuleNode | null | undefined): number {
  if (rule === null || rule === undefined) return 0;
  if (rule.type === "filter") return 1;
  return (rule.rules ?? []).reduce((n, r) => n + ruleLeafCount(r), 0);
}

/** Every row in the draft, finished or not. */
export function rowCount(item: ItemDraft): number {
  return item.kind === "row" ? 1 : item.rules.reduce((n, r) => n + rowCount(r), 0);
}

/** A draft with no row set up at all (the builder shows its empty state). */
export function isBlank(root: GroupDraft): boolean {
  const blank = (i: ItemDraft): boolean =>
    i.kind === "row" ? i.value.field === null : i.rules.every(blank);
  return blank(root);
}

// ---- rule → draft (to edit a saved smart playlist) --------------------------

/**
 * Rewrites a negated "all" group as an "any" group of negated rules (De
 * Morgan), the one shape the three-way group control cannot say. Only
 * hand-made rules have it.
 */
function negateNode(n: RuleNode): RuleNode {
  return { ...n, not: !(n.not ?? false) };
}

function normalize(n: RuleNode): RuleNode {
  if (n.type === "filter") return n;
  const rules = (n.rules ?? []).map(normalize);
  if (n.match === "all" && n.not === true) {
    return { type: "group", match: "any", not: false, rules: rules.map(negateNode).map(normalize) };
  }
  return { ...n, rules };
}

function filterValue(f: Filter): RowValue {
  switch (f.type) {
    case "tagAny":
      return { field: "tag", ids: [...f.ids] };
    case "playlist":
      return { field: "playlist", id: f.id };
    case "genre":
      return { field: "genre", genre: f.genre };
    case "keyIn":
      return { field: "key", keys: [...f.keys] };
    case "bpmRange":
      return {
        field: "bpm",
        min: f.min === 0 ? null : f.min / 100,
        max: f.max >= BPM_X100_MAX ? null : f.max / 100,
      };
    case "color":
      return { field: "color", color: f.color };
    case "ratingMin":
      return { field: "rating", min: f.rating };
    case "playCountRange":
      return {
        field: "plays",
        min: f.min === 0 ? null : f.min,
        max: f.max >= PLAYS_MAX ? null : f.max,
      };
    case "neverPlayed":
      return { field: "lastPlayed", mode: "never", date: "" };
    case "playedSince":
      return { field: "lastPlayed", mode: "since", date: f.date };
    case "missing":
      return { field: "missing", missing: f.missing };
  }
}

function itemOf(n: RuleNode): ItemDraft {
  if (n.type === "filter") return newRow(filterValue(n.filter), n.not ?? false);
  const mode: GroupMode = n.match === "all" ? "all" : n.not === true ? "none" : "any";
  return newGroup(mode, (n.rules ?? []).map(itemOf));
}

/** A draft for a saved rule; a bare filter becomes a one-row "all" group. */
export function fromRule(rule: RuleNode): GroupDraft {
  const n = normalize(rule);
  const item = itemOf(n);
  return item.kind === "group" ? item : newGroup("all", [item]);
}

// ---- edits -----------------------------------------------------------------

function mapItems(group: GroupDraft, f: (i: ItemDraft) => ItemDraft | null): GroupDraft {
  return {
    ...group,
    rules: group.rules.flatMap((i) => {
      const next = f(i);
      return next === null ? [] : [next.kind === "group" ? mapItems(next, f) : next];
    }),
  };
}

function inGroup(root: GroupDraft, groupId: string, edit: (g: GroupDraft) => GroupDraft) {
  if (root.id === groupId) return edit(root);
  return mapItems(root, (i) => (i.kind === "group" && i.id === groupId ? edit(i) : i));
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

/** Another group can go inside `groupId` without passing `MAX_LEVELS`. */
export function canAddGroup(root: GroupDraft, groupId: string): boolean {
  const level = levelOf(root, groupId);
  return level !== null && level < MAX_LEVELS;
}

export function addItem(root: GroupDraft, groupId: string, item: ItemDraft): GroupDraft {
  return inGroup(root, groupId, (g) => ({ ...g, rules: [...g.rules, item] }));
}

/** Inserts `item` right after the item `afterId` (Enter on a row). */
export function addAfter(root: GroupDraft, afterId: string, item: ItemDraft): GroupDraft {
  const place = (g: GroupDraft): GroupDraft => {
    const i = g.rules.findIndex((r) => r.id === afterId);
    const rules =
      i < 0
        ? g.rules.map((r) => (r.kind === "group" ? place(r) : r))
        : [...g.rules.slice(0, i + 1), item, ...g.rules.slice(i + 1)];
    return { ...g, rules };
  };
  return place(root);
}

/** Removes a row or group (not the root). */
export function removeItem(root: GroupDraft, id: string): GroupDraft {
  return mapItems(root, (i) => (i.id === id ? null : i));
}

export function setMode(root: GroupDraft, groupId: string, mode: GroupMode): GroupDraft {
  return inGroup(root, groupId, (g) => ({ ...g, mode }));
}

export function updateRow(
  root: GroupDraft,
  rowId: string,
  patch: Partial<Pick<RowDraft, "not" | "value">>,
): GroupDraft {
  return mapItems(root, (i) => (i.kind === "row" && i.id === rowId ? { ...i, ...patch } : i));
}

/** The id of the row after the one with `id` in reading order (to focus it), else the previous. */
export function neighbourRow(root: GroupDraft, id: string): string | null {
  const rows: string[] = [];
  const walk = (g: GroupDraft) => {
    for (const i of g.rules) {
      if (i.kind === "row") rows.push(i.id);
      else walk(i);
    }
  };
  walk(root);
  const at = rows.indexOf(id);
  return rows[at + 1] ?? rows[at - 1] ?? null;
}

// ---- summary ---------------------------------------------------------------

/** Names the summary needs from the rest of the app. */
export interface SummaryNames {
  tag(id: TagId): string;
  playlist(id: number): string;
  key(key: Key): string;
}

function range(min: number | null, max: number | null): string {
  if (min !== null && max !== null) return `${min}–${max}`;
  return min !== null ? `${min}+` : `up to ${max}`;
}

/** One finished row as a phrase; `null` for an unfinished one. */
export function rowText(row: RowDraft, names: SummaryNames): string | null {
  const v = row.value;
  const [yes, no] = negationLabels(v.field);
  const verb = row.not ? no : yes;
  switch (v.field) {
    case null:
      return null;
    case "tag":
      return v.ids.length === 0 ? null : `${verb} tag ${v.ids.map(names.tag).join("/")}`;
    case "playlist":
      return v.id === null ? null : `${verb} playlist ${names.playlist(v.id)}`;
    case "genre":
      return v.genre === null ? null : `genre ${verb} ${v.genre}`;
    case "key":
      return v.keys.length === 0 ? null : `key ${verb} ${v.keys.map(names.key).join("/")}`;
    case "bpm":
      return v.min === null && v.max === null ? null : `BPM ${verb} ${range(v.min, v.max)}`;
    case "color":
      return v.color === null ? null : `colour ${verb} ${v.color}`;
    case "rating":
      return v.min === 0 ? null : `rating ${verb} ${v.min}★ or more`;
    case "plays":
      return v.min === null && v.max === null ? null : `plays ${verb} ${range(v.min, v.max)}`;
    case "lastPlayed":
      if (v.mode === "never") return row.not ? "has been played" : "never played";
      return DATE.test(v.date) ? `${row.not ? "not played" : "played"} since ${v.date}` : null;
    case "missing":
      return `file ${verb} missing`;
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

/** "Tag Peak/Deep · not in playlist Warm-up · BPM is 120–128"; `null` while nothing is finished. */
export function summarize(root: GroupDraft, names: SummaryNames): string | null {
  const text = itemText(root, names, false);
  return text === null ? null : text.replace(/[a-z]/, (c) => c.toUpperCase());
}
