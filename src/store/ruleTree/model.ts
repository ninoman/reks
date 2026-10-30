/** The draft the Filters drawer edits (D35): types, ids, fields and defaults. No store, no IPC. */
import type { Key, TagId } from "../../lib/ipc";

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
